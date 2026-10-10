//! Milestone test: two local nodes pair, A routes a Codex turn to B, B uses
//! its own login against a mock upstream and streams text + image back.
//! Failure paths must fail closed and never replay.

use aillies_core::FailureCode;
use aillies_node::store::Destination;
use aillies_node::{Node, NodeConfig};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use bytes::Bytes;
use futures_util::stream;
use http_body_util::{BodyExt, StreamBody};
use hyper::body::Frame;
use serde_json::json;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SSE: &str = "event: response.created\ndata: {\"type\":\"response.created\"}\n\n\
event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello from B\"}\n\n\
event: response.image_generation_call.partial_image\ndata: {\"type\":\"response.image_generation_call.partial_image\",\"partial_image_b64\":\"iVBORw0KGgo=\"}\n\n\
event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";

struct Upstream {
    url: String,
    hits: Arc<AtomicUsize>,
    mode: Arc<AtomicU8>, // 0 = full stream, 1 = drop mid-stream
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

async fn upstream() -> Upstream {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/backend-api/codex/responses", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let mode = Arc::new(AtomicU8::new(0));
    let seen = Arc::new(Mutex::new(vec![]));
    let (h, m, s) = (hits.clone(), mode.clone(), seen.clone());
    tokio::spawn(async move {
        loop {
            let (sock, _) = listener.accept().await.unwrap();
            let (h, m, s) = (h.clone(), m.clone(), s.clone());
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                    let (h, m, s) = (h.clone(), m.clone(), s.clone());
                    async move {
                        h.fetch_add(1, Ordering::SeqCst);
                        let auth = req.headers().get("authorization").unwrap().to_str().unwrap().to_string();
                        let acct = req.headers().get("chatgpt-account-id").unwrap().to_str().unwrap().to_string();
                        let body = req.into_body().collect().await.unwrap().to_bytes();
                        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
                        assert_eq!(v["stream"], true);
                        assert_eq!(v["store"], false);
                        s.lock().unwrap().push((auth, acct));
                        let frames: Vec<Result<Frame<Bytes>, std::io::Error>> = if m.load(Ordering::SeqCst) == 1 {
                            vec![Ok(Frame::data(Bytes::from_static(b"event: response.created\ndata: {}\n\n"))), Err(std::io::Error::other("boom"))]
                        } else {
                            vec![Ok(Frame::data(Bytes::from_static(SSE.as_bytes())))]
                        };
                        let body = StreamBody::new(stream::iter(frames));
                        Ok::<_, std::convert::Infallible>(hyper::Response::builder().header("content-type", "text/event-stream").body(body).unwrap())
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new().serve_connection(hyper_util::rt::TokioIo::new(sock), svc).await;
            });
        }
    });
    Upstream { url, hits, mode, seen }
}

fn jwt(exp: f64, account: &str) -> String {
    let claims = json!({"exp": exp, "https://api.openai.com/auth": {"chatgpt_account_id": account}});
    format!("{}.{}.sig", URL_SAFE_NO_PAD.encode(b"{}"), URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap()))
}

fn write_auth(path: &std::path::Path, exp: f64, account: &str) {
    let token = jwt(exp, account);
    std::fs::write(path, serde_json::to_vec(&json!({"OPENAI_API_KEY": null, "tokens": {"access_token": token, "account_id": account}})).unwrap()).unwrap();
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn cfg(dir: &std::path::Path, auth: std::path::PathBuf, upstream: &str, peer_port: u16) -> NodeConfig {
    NodeConfig {
        home: dir.join("home"),
        loopback_port: 0,
        peer_port,
        codex_auth_path: Some(auth),
        codex_home: Some(dir.join("codex")),
        codex_upstream: Some(upstream.to_string()),
        advertise_loopback: true,
    }
}

async fn turn(node: &Node, key: &str) -> (u16, Result<String, String>) {
    let url = format!("http://127.0.0.1:{}/v1/responses", node.loopback_port());
    let resp = reqwest::Client::new()
        .post(url)
        .bearer_auth(key)
        .json(&json!({"model": "gpt-5.5", "input": [{"type":"message","role":"user","content":[{"type":"input_text","text":"draw a cat"}]}],
            "tools": [{"type": "image_generation", "output_format": "png"}], "stream": true}))
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    (status, resp.text().await.map_err(|e| e.to_string()))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pair_route_and_fail_closed() {
    let up = upstream().await;
    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let b_auth = b_dir.path().join("auth.json");
    write_auth(&b_auth, 4e9, "acct_b");
    let b_port = free_port();

    let b = Node::start(cfg(b_dir.path(), b_auth.clone(), &up.url, b_port)).await.unwrap();
    // A has no Codex login at all: proves A's account is never used.
    let a = Node::start(cfg(a_dir.path(), a_dir.path().join("missing-auth.json"), &up.url, free_port())).await.unwrap();

    // --- Pairing: invite on B, codes compared, approval on both sides ---
    let code = b.create_invite().await.unwrap();
    assert!(b.status().sharing);
    let started = a.start_pairing(&code).await.unwrap();
    let pending = b.status().pending;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].code, started.code, "both devices must show the same verification code");
    assert!(a.start_pairing(&code).await.is_err(), "invite is one-time");
    b.approve(&pending[0].id, true).unwrap();
    let peer = a.confirm_pairing(&started.id, Duration::from_secs(10)).await.unwrap();
    assert_eq!(peer.fp, b.fingerprint());
    assert_eq!(a.status().destination, Destination::Peer(b.fingerprint().to_string()));
    assert_eq!(b.status().clients.len(), 1);

    // Transport and account readiness are reported separately.
    let health = a.peer_health(b.fingerprint()).await;
    assert!(health.reachable);
    let ps = health.status.unwrap();
    assert!(matches!(ps.providers[0].account, aillies_core::protocol::AccountReadiness::Ready));

    // Codex config points at A's loopback.
    a.codex_enable(None).unwrap();
    let cfg_text = std::fs::read_to_string(a_dir.path().join("codex/config.toml")).unwrap();
    assert!(cfg_text.contains(&format!("http://127.0.0.1:{}/v1", a.loopback_port())));
    assert!(cfg_text.contains(a.codex_key()));

    // --- Happy path: text + image stream back from B's account ---
    let (status, body) = turn(&a, a.codex_key()).await;
    assert_eq!(status, 200);
    let body = body.unwrap();
    assert!(body.contains("Hello from B") && body.contains("partial_image_b64") && body.contains("response.completed"));
    assert_eq!(up.hits.load(Ordering::SeqCst), 1);
    {
        let seen = up.seen.lock().unwrap();
        assert_eq!(seen[0].1, "acct_b");
        assert!(seen[0].0.starts_with("Bearer ey") || seen[0].0.starts_with("Bearer e30"));
    }
    assert_eq!(a.status().state, "ready");

    // Wrong local key.
    let (status, _) = turn(&a, "nope").await;
    assert_eq!(status, 401);

    // Routing paused: nothing sent anywhere.
    a.set_routing(false).unwrap();
    let (status, body) = turn(&a, a.codex_key()).await;
    assert_eq!(status, 503);
    assert!(body.unwrap().contains("routing_paused"));
    a.set_routing(true).unwrap();

    // Sharing off on B: unreachable, nothing sent, no fallback to A.
    b.set_sharing(false).await.unwrap();
    let (status, body) = turn(&a, a.codex_key()).await;
    assert_eq!(status, 503);
    assert!(body.unwrap().contains("peer_unavailable"));
    assert_eq!(a.status().last_failure.unwrap().code, FailureCode::PeerUnavailable);
    assert_eq!(a.status().state, "action");
    b.set_sharing(true).await.unwrap();

    // Expired login on B fails closed before any upstream call.
    write_auth(&b_auth, 1.0, "acct_b");
    let (status, body) = turn(&a, a.codex_key()).await;
    assert_eq!(status, 503);
    assert!(body.unwrap().contains("oauth_expired"));
    let health = a.peer_health(b.fingerprint()).await;
    assert!(health.reachable, "transport still fine while account is not");

    // A different account on B is refused until sharing is re-approved.
    write_auth(&b_auth, 4e9, "acct_other");
    let (_, body) = turn(&a, a.codex_key()).await;
    assert!(body.unwrap().contains("account_changed"));
    write_auth(&b_auth, 4e9, "acct_b");
    assert_eq!(up.hits.load(Ordering::SeqCst), 1, "no upstream call on any failure path");

    // Mid-stream drop: request may have run. Reported, never resent.
    up.mode.store(1, Ordering::SeqCst);
    // Depending on timing B either relays the partial stream (A sees a cut
    // stream) or fails before relaying (A gets stream_failed). Both are
    // reported as possibly delivered and neither is retried.
    let (status, body) = turn(&a, a.codex_key()).await;
    assert!(status == 200 || status == 502, "status {status}");
    assert!(body.is_err() || !body.unwrap().contains("response.completed"));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(up.hits.load(Ordering::SeqCst), 2, "exactly one upstream attempt");
    let lf = a.status().last_failure.unwrap();
    assert!(matches!(lf.code, FailureCode::DeliveryUncertain | FailureCode::StreamFailed), "{lf:?}");
    up.mode.store(0, Ordering::SeqCst);

    // Revocation is per device and immediate.
    b.revoke_client(&a.fingerprint()[..8]).unwrap();
    let (status, body) = turn(&a, a.codex_key()).await;
    assert_eq!(status, 401);
    assert!(body.unwrap().contains("not_approved"));
    assert_eq!(up.hits.load(Ordering::SeqCst), 2);

    // Restore Codex config exactly.
    a.codex_restore().unwrap();
    assert!(!a_dir.path().join("codex/config.toml").exists());

    a.shutdown();
    b.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejected_pairing_and_local_destination() {
    let up = upstream().await;
    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let a_auth = a_dir.path().join("auth.json");
    write_auth(&a_auth, 4e9, "acct_a");
    let b_auth = b_dir.path().join("auth.json");
    write_auth(&b_auth, 4e9, "acct_b");
    let b = Node::start(cfg(b_dir.path(), b_auth, &up.url, free_port())).await.unwrap();
    let a = Node::start(cfg(a_dir.path(), a_auth, &up.url, free_port())).await.unwrap();

    let code = b.create_invite().await.unwrap();
    let started = a.start_pairing(&code).await.unwrap();
    let id = b.status().pending[0].id.clone();
    b.approve(&id, false).unwrap();
    assert!(a.confirm_pairing(&started.id, Duration::from_secs(5)).await.is_err());
    assert!(a.status().peers.is_empty());
    assert!(b.status().clients.is_empty());

    // "This device" uses A's own login.
    assert_eq!(a.status().destination, Destination::Local);
    let (status, body) = turn(&a, a.codex_key()).await;
    assert_eq!(status, 200);
    assert!(body.unwrap().contains("response.completed"));
    assert_eq!(up.seen.lock().unwrap()[0].1, "acct_a");
    assert_eq!(a.status().state, "local");
    a.shutdown();
    b.shutdown();
}
