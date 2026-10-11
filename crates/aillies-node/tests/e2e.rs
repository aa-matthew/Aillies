//! Milestone test: two local nodes pair, A routes a Codex turn to B, B uses
//! its own login against a mock upstream and streams text + image back.
//! Requests retry only within the explicitly authorized requester budget.

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
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
    release: Arc<tokio::sync::Semaphore>,
}

async fn upstream() -> Upstream {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/backend-api/codex/responses", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let mode = Arc::new(AtomicU8::new(0));
    let seen = Arc::new(Mutex::new(vec![]));
    let requests = Arc::new(Mutex::new(vec![]));
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let (h, m, s, r, gate) = (hits.clone(), mode.clone(), seen.clone(), requests.clone(), release.clone());
    tokio::spawn(async move {
        loop {
            let (sock, _) = listener.accept().await.unwrap();
            let (h, m, s, r, gate) = (h.clone(), m.clone(), s.clone(), r.clone(), gate.clone());
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                    let (h, m, s, r, gate) = (h.clone(), m.clone(), s.clone(), r.clone(), gate.clone());
                    async move {
                        let attempt = h.fetch_add(1, Ordering::SeqCst) + 1;
                        let auth = req.headers().get("authorization").unwrap().to_str().unwrap().to_string();
                        let acct = req.headers().get("chatgpt-account-id").unwrap().to_str().unwrap().to_string();
                        let body = req.into_body().collect().await.unwrap().to_bytes();
                        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
                        r.lock().unwrap().push(v.clone());
                        assert_eq!(v["stream"], true);
                        assert_eq!(v["store"], false);
                        s.lock().unwrap().push((auth, acct));
                        let slow = m.load(Ordering::SeqCst) == 3;
                        let frames: Vec<Result<Frame<Bytes>, std::io::Error>> = if slow {
                            vec![Ok(Frame::data(Bytes::from_static(b"event: response.created\ndata: {}\n\n")))]
                        } else if matches!(m.load(Ordering::SeqCst), 7 | 10) && (attempt <= 2 || m.load(Ordering::SeqCst) == 10) {
                            vec![Ok(Frame::data(Bytes::from_static(b"SECRET_UPSTREAM_ERROR_BODY")))]
                        } else if m.load(Ordering::SeqCst) == 8 && attempt == 1 {
                            vec![Ok(Frame::data(Bytes::from_static(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"DISCARDED_PARTIAL\"}\n\n")))]
                        } else if m.load(Ordering::SeqCst) == 9 && attempt == 1 {
                            vec![Ok(Frame::data(Bytes::from_static(b"data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"image_generation_call\",\"result\":\"iVBORw0KGgo=\"}}\n\ndata: {\"type\":\"response.completed\"}\n\n")))]
                        } else if m.load(Ordering::SeqCst) == 5 {
                            vec![Ok(Frame::data(Bytes::from_static(b"SECRET_UPSTREAM_ERROR_BODY")))]
                        } else if m.load(Ordering::SeqCst) == 6 {
                            vec![Ok(Frame::data(Bytes::from_static(
                                b"data: {\"type\":\"response.failed\",\"error\":{\"message\":\"SECRET_STREAM_ERROR\"}}\n\n",
                            )))]
                        } else if matches!(m.load(Ordering::SeqCst), 4 | 9) {
                            vec![Ok(Frame::data(Bytes::from_static(b"data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"image_generation_call\",\"result\":\"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGNgYPgPAAEDAQAIicLsAAAAAElFTkSuQmCC\"}}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n")))]
                        } else if m.load(Ordering::SeqCst) == 1 {
                            vec![Ok(Frame::data(Bytes::from_static(b"event: response.created\ndata: {}\n\n"))), Err(std::io::Error::other("boom"))]
                        } else if m.load(Ordering::SeqCst) == 2 {
                            // Ends cleanly without a terminal event, but the text mentions one.
                            vec![Ok(Frame::data(Bytes::from_static(
                                b"event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"wait for response.completed\"}\n\n",
                            )))]
                        } else {
                            vec![Ok(Frame::data(Bytes::from_static(SSE.as_bytes())))]
                        };
                        let body = StreamBody::new(futures_util::StreamExt::chain(
                            stream::iter(frames),
                            stream::once(async move {
                                if slow {
                                    let _permit = gate.acquire().await.unwrap();
                                }
                                Ok::<_, std::io::Error>(Frame::data(if slow { Bytes::from_static(SSE.as_bytes()) } else { Bytes::new() }))
                            }),
                        ));
                        Ok::<_, std::convert::Infallible>(
                            hyper::Response::builder()
                                .status(if m.load(Ordering::SeqCst) == 5 {
                                    400
                                } else if matches!(m.load(Ordering::SeqCst), 7 | 10) && (attempt <= 2 || m.load(Ordering::SeqCst) == 10) {
                                    503
                                } else {
                                    200
                                })
                                .header("content-type", "text/event-stream")
                                .body(body)
                                .unwrap(),
                        )
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new().serve_connection(hyper_util::rt::TokioIo::new(sock), svc).await;
            });
        }
    });
    Upstream { url, hits, mode, seen, requests, release }
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

    // Mid-stream drop: retry at most three times, without releasing partial output.
    up.mode.store(1, Ordering::SeqCst);
    let (status, body) = turn(&a, a.codex_key()).await;
    assert!(status == 200 || status == 502, "status {status}");
    assert!(body.is_err() || !body.unwrap().contains("response.completed"));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(up.hits.load(Ordering::SeqCst), 5, "first request plus four attempts");
    let lf = a.status().last_failure.unwrap();
    assert!(matches!(lf.code, FailureCode::DeliveryUncertain | FailureCode::StreamFailed), "{lf:?}");
    up.mode.store(0, Ordering::SeqCst);

    // Regression: a stream that ends early must not count as complete just
    // because the generated text mentions a terminal event name.
    a.dismiss_failure();
    up.mode.store(2, Ordering::SeqCst);
    let (status, body) = turn(&a, a.codex_key()).await;
    assert_eq!(status, 200);
    assert!(body.as_ref().unwrap().contains("response.failed"));
    assert!(!body.as_ref().unwrap().contains("wait for response.completed"));
    tokio::time::sleep(Duration::from_millis(200)).await;
    let lf = a.status().last_failure.expect("early end must be reported");
    assert_eq!(lf.code, FailureCode::DeliveryUncertain);
    assert_eq!(up.hits.load(Ordering::SeqCst), 9, "at most four attempts per request");
    up.mode.store(0, Ordering::SeqCst);

    // Revocation is per device and immediate.
    b.revoke_client(&a.fingerprint()[..8]).unwrap();
    let (status, body) = turn(&a, a.codex_key()).await;
    assert_eq!(status, 401);
    assert!(body.unwrap().contains("not_approved"));
    assert_eq!(up.hits.load(Ordering::SeqCst), 9);

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

struct PairFixture {
    a: Arc<Node>,
    b: Arc<Node>,
    up: Upstream,
    _a_dir: tempfile::TempDir,
    _b_dir: tempfile::TempDir,
}

impl PairFixture {
    async fn new() -> Self {
        let up = upstream().await;
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let auth = b_dir.path().join("auth.json");
        write_auth(&auth, 4e9, "acct_b");
        let b = Node::start(cfg(b_dir.path(), auth, &up.url, free_port())).await.unwrap();
        let a = Node::start(cfg(a_dir.path(), a_dir.path().join("missing.json"), &up.url, free_port())).await.unwrap();
        let started = a.start_pairing(&b.create_invite().await.unwrap()).await.unwrap();
        b.approve(&b.status().pending[0].id, true).unwrap();
        a.confirm_pairing(&started.id, Duration::from_secs(5)).await.unwrap();
        Self { a, b, up, _a_dir: a_dir, _b_dir: b_dir }
    }
}

impl Drop for PairFixture {
    fn drop(&mut self) {
        self.a.shutdown();
        self.b.shutdown();
    }
}

#[tokio::test]
async fn forgetting_selected_peer_never_selects_local_account() {
    let f = PairFixture::new().await;
    f.a.remove_peer(f.b.fingerprint()).unwrap();
    assert!(matches!(f.a.status().destination, Destination::Peer(_)));
    let (status, body) = turn(&f.a, f.a.codex_key()).await;
    assert_eq!(status, 503);
    assert!(body.unwrap().contains("no_destination"));
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn loopback_host_and_token_boundaries() {
    let f = PairFixture::new().await;
    let base = format!("http://127.0.0.1:{}", f.a.loopback_port());
    let http = reqwest::Client::new();
    assert_eq!(http.get(format!("{base}/healthz")).header("host", "evil.example").send().await.unwrap().status(), 421);
    assert_eq!(http.get(format!("{base}/control/status")).bearer_auth(f.a.codex_key()).send().await.unwrap().status(), 401);
    assert_eq!(http.post(format!("{base}/v1/responses")).bearer_auth(f.a.control_token()).body("{}").send().await.unwrap().status(), 401);
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn status_and_response_never_expose_sharer_credentials() {
    let f = PairFixture::new().await;
    let (_, body) = turn(&f.a, f.a.codex_key()).await;
    let status = serde_json::to_string(&f.a.status()).unwrap();
    let health = serde_json::to_string(&f.a.peer_health(f.b.fingerprint()).await).unwrap();
    let token = jwt(4e9, "acct_b");
    for text in [body.unwrap(), status, health] {
        assert!(!text.contains(&token));
        assert!(!text.contains(f.b.codex_key()));
        assert!(!text.contains(f.b.control_token()));
    }
}

#[tokio::test]
async fn pinned_tls_rejects_wrong_server_identity() {
    let f = PairFixture::new().await;
    let path = f._a_dir.path().join("home/state.json");
    let mut state: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let fake_fp = "f".repeat(64);
    state["peers"][0]["fp"] = json!(fake_fp);
    state["destination"] = json!({"kind":"peer","id":fake_fp});
    std::fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
    f.a.shutdown();
    let cfg = cfg(f._a_dir.path(), f._a_dir.path().join("missing.json"), &f.up.url, free_port());
    let restarted = Node::start(cfg).await.unwrap();
    let (status, body) = turn(&restarted, restarted.codex_key()).await;
    assert_eq!(status, 503);
    assert!(body.unwrap().contains("peer_unavailable"));
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 0);
    restarted.shutdown();
}

async fn peer_turn(f: &PairFixture, body: serde_json::Value, version: Option<&str>) -> (u16, String) {
    let identity = aillies_node::identity::Identity::load_or_create(&f._a_dir.path().join("home")).unwrap();
    let peer = &f.a.status().peers[0];
    let mut conn = aillies_node::peer_client::connect(&identity, &peer.fp, &peer.addrs).await.unwrap();
    let mut req = aillies_node::peer_client::request("POST", "/v1/codex/responses", Bytes::from(serde_json::to_vec(&body).unwrap()));
    if let Some(version) = version {
        req.headers_mut().insert("x-aillies-protocol", version.parse().unwrap());
    }
    let resp = conn.sender.send_request(req).await.unwrap();
    (resp.status().as_u16(), String::from_utf8(resp.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
}

#[tokio::test]
async fn fast_mode_requires_owner_permission_and_remains_optional() {
    let f = PairFixture::new().await;
    assert!(!f.a.status().fast_mode);
    assert!(!f.b.status().allow_fast_mode);
    assert!(f.a.set_fast_mode(true).await.is_err());
    let (status, body) = peer_turn(&f, json!({"model":"gpt-5.5","input":"hi","service_tier":"priority"}), None).await;
    assert_eq!(status, 403);
    assert!(body.contains("fast_mode_not_allowed"));
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 0);
    f.b.set_allow_fast_mode(true).unwrap();
    f.a.set_fast_mode(true).await.unwrap();
    assert_eq!(turn(&f.a, f.a.codex_key()).await.0, 200);
    assert_eq!(f.up.requests.lock().unwrap().last().unwrap()["service_tier"], "priority");
    f.b.set_allow_fast_mode(false).unwrap();
    let (status, body) = turn(&f.a, f.a.codex_key()).await;
    assert_eq!(status, 403);
    assert!(body.unwrap().contains("fast_mode_not_allowed"));
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 1, "revoked permission never sends or downgrades");
    f.a.set_fast_mode(false).await.unwrap();
    assert_eq!(turn(&f.a, f.a.codex_key()).await.0, 200);
    assert!(f.up.requests.lock().unwrap().last().unwrap().get("service_tier").is_none());
    f.a.set_destination("local").unwrap();
    assert!(!f.a.status().fast_mode, "preferences are per destination");
}

#[tokio::test]
async fn astra_upstream_shape_preserves_full_history_and_sends_once() {
    let f = PairFixture::new().await;
    let input = json!([
        {"role":"developer","content":"fixture instructions"},
        {"type":"reasoning","encrypted_content":"fixture encrypted state","summary":[]},
        {"type":"message","role":"assistant","phase":"commentary","content":"checking"},
        {"role":"user","content":"fixture question"}
    ]);
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/v1/responses", f.a.loopback_port()))
        .bearer_auth(f.a.codex_key())
        .header("x-session-id", "fixture-session")
        .json(&json!({"model":"gpt-6-astra", "input":input, "reasoning":{"effort":"low"},
            "include":["web_search_call.action.sources"], "parallel_tool_calls":true, "service_tier":"default"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert!(response.text().await.unwrap().contains("response.completed"));
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 1);
    let sent = f.up.requests.lock().unwrap().last().unwrap().clone();
    assert_eq!(sent["input"], input);
    assert_eq!(sent["prompt_cache_key"], "fixture-session");
    assert_eq!(sent["model"], "gpt-6-astra");
    assert_eq!(sent["reasoning"]["effort"], "low");
    assert!(sent.get("service_tier").is_none());
    assert!(sent.get("parallel_tool_calls").is_none());
    assert_eq!(sent["include"], json!(["web_search_call.action.sources", "reasoning.encrypted_content"]));
    assert_eq!(f.up.seen.lock().unwrap()[0].1, "acct_b");
}

#[tokio::test]
async fn peer_protocol_mismatch_is_refused_before_upstream() {
    let f = PairFixture::new().await;
    let (status, body) = peer_turn(&f, json!({"model":"gpt-5.5","input":"hi"}), Some("999")).await;
    assert_eq!(status, 426);
    assert!(body.contains("protocol_mismatch"));
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authorized_retries_recover_without_exposing_failed_attempts() {
    for (mode, attempts) in [(7, 3), (8, 2)] {
        let f = PairFixture::new().await;
        f.up.mode.store(mode, Ordering::SeqCst);
        let (status, body) = turn(&f.a, f.a.codex_key()).await;
        assert_eq!(status, 200);
        let body = body.unwrap();
        assert!(body.contains("Hello from B"));
        assert!(!body.contains("DISCARDED_PARTIAL") && !body.contains("SECRET"));
        assert_eq!(f.up.hits.load(Ordering::SeqCst), attempts);
        let sent = f.up.requests.lock().unwrap();
        assert!(sent.iter().all(|request| request == &sent[0]), "same model, history and tier on every attempt");
        assert!(f.up.seen.lock().unwrap().iter().all(|(_, account)| account == "acct_b"));
        let report = f.b.diagnostics();
        assert_eq!(report["upstream"].as_array().unwrap().len(), attempts);
        for (index, entry) in report["upstream"].as_array().unwrap().iter().enumerate() {
            assert_eq!(entry["request"]["attempt"], index + 1);
        }
        assert_eq!(f.a.diagnostics()["entries"].as_array().unwrap().len(), attempts - 1);
        assert!(f.a.status().last_failure.is_none());
        assert!(!f.a.diagnostics().to_string().contains("SECRET"));
    }
}

#[tokio::test]
async fn authorized_retries_stop_after_three_without_nested_peer_retries() {
    let f = PairFixture::new().await;
    f.up.mode.store(10, Ordering::SeqCst);
    let (status, body) = turn(&f.a, f.a.codex_key()).await;
    assert_eq!(status, 502);
    assert!(!body.unwrap().contains("SECRET"));
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 4);
    assert_eq!(f.b.diagnostics()["upstream"].as_array().unwrap().len(), 4);
    let entries = f.a.diagnostics()["entries"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 4);
    for (index, entry) in entries[..3].iter().enumerate() {
        assert_eq!(entry["level"], "warning");
        assert_eq!(entry["retry"], index + 1);
    }
}

async fn wait_for_retry(node: &Node) {
    tokio::time::timeout(Duration::from_secs(60), async {
        while !node.status().recent_warnings.iter().any(|warning| warning.retry.is_some()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retry_stops_on_destination_change_without_local_fallback() {
    let f = PairFixture::new().await;
    f.up.mode.store(10, Ordering::SeqCst);
    let node = f.a.clone();
    let task = tokio::spawn(async move { turn(&node, node.codex_key()).await });
    wait_for_retry(&f.a).await;
    f.a.set_destination("local").unwrap();
    let (_, body) = task.await.unwrap();
    assert!(body.unwrap().contains("delivery_uncertain"));
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 1);
    assert!(f.up.seen.lock().unwrap().iter().all(|(_, account)| account == "acct_b"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retries_pin_local_and_peer_accounts_even_after_reapproval() {
    for local in [false, true] {
        let f = PairFixture::new().await;
        f.up.mode.store(10, Ordering::SeqCst);
        let node = if local { f.b.clone() } else { f.a.clone() };
        if local {
            node.set_destination("local").unwrap();
        }
        let running = node.clone();
        let task = tokio::spawn(async move { turn(&running, running.codex_key()).await });
        wait_for_retry(&node).await;
        write_auth(&f._b_dir.path().join("auth.json"), 4e9, "acct_other");
        f.b.set_sharing(true).await.unwrap();
        let (_, body) = task.await.unwrap();
        assert!(body.unwrap().contains("delivery_uncertain"));
        assert_eq!(f.up.hits.load(Ordering::SeqCst), 1, "never use a different account on a retry");
        assert_eq!(f.up.seen.lock().unwrap()[0].1, "acct_b");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_during_backoff_cancels_all_remaining_retries() {
    let f = PairFixture::new().await;
    f.up.mode.store(10, Ordering::SeqCst);
    let node = f.a.clone();
    let task = tokio::spawn(async move { turn(&node, node.codex_key()).await });
    wait_for_retry(&f.a).await;
    task.abort();
    let _ = task.await;
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn forced_image_retries_invalid_png_and_saves_only_one_complete_image() {
    let f = PairFixture::new().await;
    f.up.mode.store(9, Ordering::SeqCst);
    let call =
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"generate_image","arguments":{"prompt":"a blue square","model":"gpt-5.6-sol"}}});
    let result: serde_json::Value = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/mcp/images", f.a.loopback_port()))
        .bearer_auth(f.a.codex_key())
        .json(&call)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(result["result"]["content"][0]["type"], "image");
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 2);
    assert_eq!(std::fs::read_dir(f._a_dir.path().join("codex/generated_images/aillies")).unwrap().count(), 1);
    assert_eq!(f.a.status().recent_warnings[0].retry, Some(1));
    assert!(f.up.requests.lock().unwrap().iter().all(|request| request["model"] == "gpt-5.6-sol"));
}

#[tokio::test]
async fn cancellation_withdraws_an_already_approved_pairing() {
    let f = PairFixture::new().await;
    let started = f.a.start_pairing(&f.b.create_invite().await.unwrap()).await.unwrap();
    f.b.approve(&f.b.status().pending[0].id, true).unwrap();
    f.a.cancel_pairing(&started.id).await;
    assert!(f.b.status().clients.is_empty());
    assert_eq!(turn(&f.a, f.a.codex_key()).await.0, 401);
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn busy_retries_and_live_revocation_never_execute_another_account() {
    let f = PairFixture::new().await;
    f.up.mode.store(3, Ordering::SeqCst);
    let mut live = Vec::new();
    for _ in 0..4 {
        let node = f.a.clone();
        live.push(tokio::spawn(async move { turn(&node, node.codex_key()).await }));
    }
    tokio::time::timeout(Duration::from_secs(15), async {
        while f.up.hits.load(Ordering::SeqCst) < 4 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let (status, body) = turn(&f.a, f.a.codex_key()).await;
    assert_eq!(status, 429);
    assert!(body.unwrap().contains("peer_busy"));
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 4);
    f.b.revoke_client(f.a.fingerprint()).unwrap();
    for task in live {
        let (_, body) = tokio::time::timeout(Duration::from_secs(30), task).await.unwrap().unwrap();
        assert!(body.is_err() || !body.unwrap().contains("response.completed"));
    }
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 4);
    f.up.release.add_permits(4);
    assert_eq!(turn(&f.a, f.a.codex_key()).await.0, 401);
}

#[tokio::test]
async fn oversized_request_is_rejected_without_upstream() {
    let f = PairFixture::new().await;
    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/v1/responses", f.a.loopback_port()))
        .bearer_auth(f.a.codex_key())
        .body(vec![b' '; aillies_core::request::MAX_REQUEST_BYTES + 1])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn image_tool_discovery_authentication_and_selected_peer_route() {
    let f = PairFixture::new().await;
    let url = format!("http://127.0.0.1:{}/mcp/images", f.a.loopback_port());
    let client = reqwest::Client::new();
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
    assert_eq!(client.post(&url).json(&request).send().await.unwrap().status(), 401);
    assert_eq!(client.post(&url).bearer_auth(f.a.control_token()).json(&request).send().await.unwrap().status(), 401);
    assert_eq!(client.post(&url).bearer_auth(f.a.codex_key()).header("Origin", "https://evil.example").json(&request).send().await.unwrap().status(), 401);
    let list: serde_json::Value = client.post(&url).bearer_auth(f.a.codex_key()).json(&request).send().await.unwrap().json().await.unwrap();
    assert_eq!(list["result"]["tools"][0]["name"], "generate_image");
    assert!(list["result"]["tools"][0]["description"].as_str().unwrap().contains("Markdown image reference"));
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 0);
    f.up.mode.store(4, Ordering::SeqCst);
    let call =
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"generate_image","arguments":{"prompt":"a blue square","model":"gpt-5.6-sol"}}});
    let out: serde_json::Value = client.post(&url).bearer_auth(f.a.codex_key()).json(&call).send().await.unwrap().json().await.unwrap();
    assert_eq!(out["result"]["content"][0]["type"], "image");
    assert!(out["result"]["content"][0]["data"].as_str().unwrap().len() > 80);
    let display = out["result"]["content"][1]["text"].as_str().unwrap();
    assert!(display.contains("Show this image in your final reply:"));
    let image_path = display.split("![Generated image](<").nth(1).unwrap().split(">)").next().unwrap();
    assert!(std::path::Path::new(image_path).is_absolute());
    assert!(std::path::Path::new(image_path).is_file());
    assert_eq!(std::fs::read_dir(f._a_dir.path().join("codex/generated_images/aillies")).unwrap().count(), 1);
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 1);
    let sent = f.up.requests.lock().unwrap().last().unwrap().clone();
    assert_eq!(sent["tools"][0]["type"], "image_generation");
    assert_eq!(sent["model"], "gpt-5.6-sol");
    assert!(sent.get("service_tier").is_none());
    assert_eq!(f.up.seen.lock().unwrap()[0].1, "acct_b");
    f.b.set_sharing(false).await.unwrap();
    let out: serde_json::Value = client.post(&url).bearer_auth(f.a.codex_key()).json(&call).send().await.unwrap().json().await.unwrap();
    assert_eq!(out["result"]["isError"], true);
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 1, "no replay or local fallback");
}

#[tokio::test]
async fn reset_after_request_is_written_uses_only_three_authorized_retries() {
    let f = PairFixture::new().await;
    let identity_dir = tempfile::tempdir().unwrap();
    let identity = aillies_node::identity::Identity::load_or_create(identity_dir.path()).unwrap();
    let fp = identity.fingerprint.clone();
    let acceptor = tokio_rustls::TlsAcceptor::from(aillies_node::tls::server_config(&identity).unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let received = Arc::new(AtomicUsize::new(0));
    let hits = received.clone();
    let server = tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let tls = acceptor.accept(socket).await.unwrap();
            let hits = hits.clone();
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                    let hits = hits.clone();
                    async move {
                        let bytes = req.into_body().collect().await.unwrap().to_bytes();
                        if bytes.is_empty() {
                            return Ok::<_, std::io::Error>(hyper::Response::new(http_body_util::Full::new(Bytes::from_static(
                                br#"{"protocol":1,"app_version":"synthetic","name":"synthetic","platform":"macos","sharing":true,"providers":[{"provider":"codex","capabilities":[],"account":{"state":"ready"},"supports_pinned_retries":true}]}"#,
                            ))));
                        }
                        hits.fetch_add(1, Ordering::SeqCst);
                        Err::<hyper::Response<http_body_util::Full<Bytes>>, _>(std::io::Error::other("test reset after receipt"))
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new().serve_connection(hyper_util::rt::TokioIo::new(tls), svc).await;
            });
        }
    });
    f.a.shutdown();
    let state_path = f._a_dir.path().join("home/state.json");
    let mut state: serde_json::Value = serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
    state["peers"][0]["fp"] = json!(fp);
    state["peers"][0]["addrs"] = json!([addr.clone(), addr]);
    state["destination"] = json!({"kind":"peer","id":fp});
    std::fs::write(state_path, serde_json::to_vec(&state).unwrap()).unwrap();
    let a = Node::start(cfg(f._a_dir.path(), f._a_dir.path().join("missing.json"), &f.up.url, free_port())).await.unwrap();
    let (status, body) = turn(&a, a.codex_key()).await;
    assert_eq!(status, 502);
    assert!(body.unwrap().contains("delivery_uncertain"));
    assert_eq!(received.load(Ordering::SeqCst), 4, "four total attempts even with duplicate addresses");
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 0, "never fall back to B");
    a.shutdown();
    server.abort();
}

#[tokio::test]
async fn tracing_never_contains_request_response_or_credentials() {
    #[derive(Clone)]
    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let writer = Capture(bytes.clone());
    let subscriber =
        tracing_subscriber::fmt().with_ansi(false).without_time().with_max_level(tracing::Level::TRACE).with_writer(move || writer.clone()).finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();
    let f = PairFixture::new().await;
    assert_eq!(turn(&f.a, f.a.codex_key()).await.0, 200);
    f.b.revoke_client(f.a.fingerprint()).unwrap();
    assert_eq!(turn(&f.a, f.a.codex_key()).await.0, 401);
    let logs = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("codex turn streaming"), "must capture real route logs");
    for secret in ["draw a cat".to_string(), "Hello from B".to_string(), jwt(4e9, "acct_b"), f.a.codex_key().to_string(), f.b.control_token().to_string()] {
        assert!(!logs.contains(&secret), "content or credentials leaked to tracing");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn diagnostics_capture_logical_turns_and_each_retry_without_content() {
    let f = PairFixture::new().await;
    f.up.mode.store(5, Ordering::SeqCst);
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/v1/responses", f.a.loopback_port()))
        .bearer_auth(f.a.codex_key())
        .json(&json!({"model":"gpt-6.1-sol","input":"SECRET_PROMPT", "reasoning":{"effort":"low"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 502);
    let _ = response.text().await.unwrap();
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 4);
    for (node, route) in [(&f.a, "peer"), (&f.b, "hosted")] {
        let report = node.diagnostics();
        assert_eq!(report["entries"].as_array().unwrap().len(), 4);
        assert_eq!(report["entries"][0]["code"], "upstream_rejected");
        assert_eq!(report["entries"][0]["upstream_status"], 400);
        assert_eq!(report["entries"][0]["request"]["model"], "gpt-6.1-sol");
        assert_eq!(report["entries"][0]["request"]["route"], route);
        let text = report.to_string();
        for secret in ["SECRET", node.codex_key(), node.control_token(), "acct_b", "Bearer"] {
            assert!(!text.contains(secret));
        }
        node.dismiss_failure();
        assert_eq!(node.diagnostics()["entries"].as_array().unwrap().len(), 4);
    }
    let hosted = f.b.diagnostics();
    assert_eq!(hosted["upstream"][0]["event"]["status"], 400);
    assert_eq!(hosted["upstream"][0]["event"]["failure"], "upstream_rejected");
    assert_eq!(f.a.diagnostics()["incoming"][0]["request"]["trace_id"], hosted["upstream"][0]["request"]["trace_id"]);
    assert_eq!(f.a.diagnostics()["upstream"].as_array().unwrap().len(), 0);
    // Successful requests must not evict diagnostic entries.
    f.up.mode.store(0, Ordering::SeqCst);
    assert_eq!(turn(&f.a, f.a.codex_key()).await.0, 200);
    assert_eq!(f.a.diagnostics()["entries"].as_array().unwrap().len(), 4);
    assert_eq!(f.b.diagnostics()["upstream"][4]["event"]["phase"], "completed");
    assert_eq!(f.a.diagnostics()["incoming"][1]["event"]["phase"], "completed");
    // A terminal failure is observable without copying its error message.
    f.up.mode.store(6, Ordering::SeqCst);
    assert_eq!(turn(&f.a, f.a.codex_key()).await.0, 200);
    assert_eq!(f.a.diagnostics()["entries"][4]["code"], "stream_failed");
    assert_eq!(f.b.diagnostics()["entries"][4]["code"], "stream_failed");
    assert!(!f.a.diagnostics().to_string().contains("SECRET"));
    // The local route is covered too.
    f.b.set_destination("local").unwrap();
    f.up.mode.store(5, Ordering::SeqCst);
    assert_eq!(turn(&f.b, f.b.codex_key()).await.0, 502);
    assert_eq!(f.b.diagnostics()["entries"][8]["request"]["route"], "local");
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 13);
}

#[tokio::test]
async fn diagnostics_control_api_requires_control_key_and_refusals_are_logged() {
    let f = PairFixture::new().await;
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/control/diagnostics", f.a.loopback_port());
    for key in ["", f.a.codex_key()] {
        assert_eq!(client.get(&url).bearer_auth(key).send().await.unwrap().status(), 401);
    }
    f.a.set_routing(false).unwrap();
    assert_eq!(turn(&f.a, f.a.codex_key()).await.0, 503);
    let report: serde_json::Value = client.get(&url).bearer_auth(f.a.control_token()).send().await.unwrap().json().await.unwrap();
    assert_eq!(report["entries"][0]["code"], "routing_paused");
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 0);
    let (_, _) = peer_turn(&f, json!({"model":"gpt-5.5","input":"hi"}), Some("999")).await;
    assert_eq!(f.b.diagnostics()["entries"][0]["code"], "protocol_mismatch");
}

#[tokio::test]
async fn cancelled_stream_is_recorded_on_requester_and_sharer_without_replay() {
    let f = PairFixture::new().await;
    f.up.mode.store(3, Ordering::SeqCst);
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/v1/responses", f.a.loopback_port()))
        .bearer_auth(f.a.codex_key())
        .json(&json!({"model":"gpt-5.6-sol","input":"SYNTHETIC_CANCEL"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    drop(response);
    for _ in 0..100 {
        if f.b.diagnostics()["upstream"][0]["event"]["phase"] == "delivery_uncertain" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(f.up.hits.load(Ordering::SeqCst), 1);
    for node in [&f.a, &f.b] {
        assert_eq!(node.diagnostics()["incoming"][0]["event"]["phase"], "delivery_uncertain");
        assert_eq!(node.diagnostics()["incoming"][0]["event"]["status"], 200);
        assert!(!node.diagnostics().to_string().contains("SYNTHETIC_CANCEL"));
    }
    assert_eq!(f.b.diagnostics()["upstream"][0]["event"]["phase"], "delivery_uncertain");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn numeric_pairing_bootstrap_requires_password_certificates_and_both_approvals() {
    let up = upstream().await;
    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let auth = b_dir.path().join("auth.json");
    write_auth(&auth, 4e9, "acct_b");
    let a = Node::start(cfg(a_dir.path(), a_dir.path().join("missing"), &up.url, free_port())).await.unwrap();
    let b = Node::start(cfg(b_dir.path(), auth, &up.url, free_port())).await.unwrap();
    let digits = b.create_pairing_code().await.unwrap();
    assert_eq!(digits.len(), 8);
    assert!(digits.bytes().all(|b| b.is_ascii_digit()));
    let candidates = [aillies_node::PairingCandidate { fp: b.fingerprint().into(), addrs: vec![format!("127.0.0.1:{}", b.peer_port().unwrap())] }];
    let impostor = [aillies_node::PairingCandidate { fp: "d".repeat(64), addrs: candidates[0].addrs.clone() }];
    assert!(a.start_pairing_with_candidates(&digits, &impostor).await.is_err(), "wrong pinned server certificate must refuse");
    let wrong = if digits == "00000000" { "11111111" } else { "00000000" };
    assert!(a.start_pairing_with_candidates(wrong, &candidates).await.is_err());
    assert!(b.status().pending.is_empty() && b.status().clients.is_empty());
    let started = a.start_pairing_with_candidates(&digits, &candidates).await.unwrap();
    assert_eq!(started.code, b.status().pending[0].code);
    assert!(a.status().peers.is_empty() && b.status().clients.is_empty());
    assert!(a.confirm_pairing(&started.id, Duration::ZERO).await.is_err());
    b.approve(&b.status().pending[0].id, true).unwrap();
    assert!(a.status().peers.is_empty(), "requester must also confirm");
    let peer = a.confirm_pairing(&started.id, Duration::from_secs(2)).await.unwrap();
    assert_eq!(peer.fp, b.fingerprint());
    assert_eq!(b.status().clients[0].fp, a.fingerprint());
    assert!(a.start_pairing_with_candidates(&digits, &candidates).await.is_err(), "code is one-time");
    assert_eq!(up.hits.load(Ordering::SeqCst), 0, "pairing never touches OpenAI");
    let status = serde_json::to_string(&b.status()).unwrap();
    let diagnostics = b.diagnostics().to_string();
    assert!(!status.contains(&digits) && !diagnostics.contains(&digits));
    let second = b.create_pairing_code().await.unwrap();
    b.set_sharing(false).await.unwrap();
    b.set_sharing(true).await.unwrap();
    assert!(a.start_pairing_with_candidates(&second, &candidates).await.is_err(), "turning sharing off invalidates bootstrap");
    let replaced = b.create_pairing_code().await.unwrap();
    let full = b.create_full_invite().await.unwrap();
    assert!(a.start_pairing_with_candidates(&replaced, &candidates).await.is_err(), "switching to a full code invalidates the numeric code");
    assert!(a.start_pairing(&full).await.is_ok(), "full-code compatibility remains");
    a.shutdown();
    b.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires local multicast; synthetic homes, no live account or inference"]
async fn numeric_pairing_via_local_discovery() {
    let up = upstream().await;
    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let auth = b_dir.path().join("auth.json");
    write_auth(&auth, 4e9, "acct_b");
    let a = Node::start(cfg(a_dir.path(), a_dir.path().join("missing"), &up.url, free_port())).await.unwrap();
    let b = Node::start(cfg(b_dir.path(), auth, &up.url, free_port())).await.unwrap();
    let code = b.create_pairing_code().await.unwrap();
    let started = a.start_pairing(&code).await.unwrap();
    assert_eq!(started.code, b.status().pending[0].code);
    b.approve(&b.status().pending[0].id, true).unwrap();
    assert_eq!(a.confirm_pairing(&started.id, Duration::from_secs(2)).await.unwrap().fp, b.fingerprint());
    assert_eq!(up.hits.load(Ordering::SeqCst), 0);
    a.shutdown();
    b.shutdown();
}
