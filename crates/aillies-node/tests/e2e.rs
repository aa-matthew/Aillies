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

#[tokio::test]
async fn scoped_invitation_grants_only_the_selected_services_on_approval() {
    use aillies_node::node::PairPermissions;
    let upstream = claude_upstream().await;
    let ad = tempfile::tempdir().unwrap();
    let bd = tempfile::tempdir().unwrap();
    let auth = bd.path().join("auth.json");
    write_auth(&auth, 4e9, "owner");
    let a = Node::start(cfg(ad.path(), ad.path().join("missing"), "http://127.0.0.1:1/", free_port())).await.unwrap();
    let mut bc = cfg(bd.path(), auth, "http://127.0.0.1:1/", free_port());
    bc.claude_upstream = Some(upstream.origin.clone());
    let b = Node::start(bc).await.unwrap();
    let code = oauth_code(&b.begin_claude_connect().await.unwrap());
    b.finish_claude_connect(&code).await.unwrap();
    b.set_sharing(true).await.unwrap();
    assert!(b.create_scoped_pairing(PairPermissions::default(), true).await.is_err());
    for (permissions, full) in [(PairPermissions { codex: false, claude: true }, true), (PairPermissions { codex: true, claude: false }, false)] {
        let invite = b.create_scoped_pairing(permissions, full).await.unwrap();
        let started = if full {
            a.start_pairing(&invite).await.unwrap()
        } else {
            a.start_pairing_with_candidates(
                &invite,
                &[aillies_node::PairingCandidate { fp: b.fingerprint().into(), addrs: vec![format!("127.0.0.1:{}", b.peer_port().unwrap())] }],
            )
            .await
            .unwrap()
        };
        let pending = b.status().pending[0].clone();
        assert_eq!(pending.permissions, permissions);
        assert!(!b.status().clients.iter().any(|c| c.fp == a.fingerprint()));
        b.approve(&pending.id, true).unwrap();
        a.confirm_pairing(&started.id, Duration::from_secs(5)).await.unwrap();
        let client = b.status().clients[0].clone();
        assert_eq!((client.codex, client.claude), (permissions.codex, permissions.claude));
        let services = a.peer_health(b.fingerprint()).await.status.unwrap().providers;
        assert_eq!(services.iter().any(|p| p.provider == aillies_core::protocol::Provider::Codex), permissions.codex);
        assert_eq!(services.iter().any(|p| p.provider == aillies_core::protocol::Provider::Claude), permissions.claude);
        b.revoke_client(a.fingerprint()).unwrap();
    }
    a.shutdown();
    b.shutdown();
}

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
                                b"data: {\"type\":\"response.failed\",\"error\":{\"type\":\"api_error\",\"message\":\"SECRET_STREAM_ERROR\"}}\n\n",
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
        claude_home: Some(dir.join("claude")),
        claude_desktop_reader: None,
        claude_upstream: None,
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

    // Names are editable metadata; identity and grants remain pinned.
    b.set_name("Owner’s Mac").unwrap();
    a.set_name("My renamed Mac").unwrap();
    let renamed = a.peer_health(b.fingerprint()).await.status.unwrap();
    assert_eq!(renamed.name, "Owner’s Mac");
    assert_eq!(a.status().peers[0].name, "Owner’s Mac");
    assert_eq!(b.status().clients[0].name, "My renamed Mac");
    assert_eq!(a.status().peers[0].fp, b.fingerprint());
    assert!(b.status().clients[0].codex);
    b.grant_codex(a.fingerprint(), false).unwrap();
    assert!(!a.peer_health(b.fingerprint()).await.status.unwrap().providers.iter().any(|p| p.provider == aillies_core::protocol::Provider::Codex));
    b.grant_codex(a.fingerprint(), true).unwrap();

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
    assert!(logs.contains("request streaming from peer"), "must capture real route logs");
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

// Claude uses native Messages, separate grants and a key that never leaves B.
// These encrypted fixtures were generated independently with OpenSSL, using a
// public test password; they contain no real account or login data.
const DESKTOP_READY: &str = include_str!("fixtures/claude-desktop-ready.json");
const DESKTOP_ROTATED: &str = include_str!("fixtures/claude-desktop-rotated.json");
const DESKTOP_CHANGED: &str = include_str!("fixtures/claude-desktop-changed.json");
const DESKTOP_EXPIRED: &str = include_str!("fixtures/claude-desktop-expired.json");
fn desktop_config(config: &mut aillies_node::NodeConfig, bytes: &str) -> std::path::PathBuf {
    let home = config.claude_home.as_ref().unwrap().join("desktop");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.json"), bytes).unwrap();
    config.claude_desktop_reader = Some(aillies_claude::desktop_auth::DesktopReader::with_test_password(home.clone(), b"aillies-fixture-password"));
    home.join("config.json")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn desktop_existing_login_shares_without_relogin_and_never_uses_code_fallback() {
    use aillies_node::store::ClaudeAccountSource;
    let upstream = claude_upstream().await;
    let ad = tempfile::tempdir().unwrap();
    let bd = tempfile::tempdir().unwrap();
    let mut ac = cfg(ad.path(), ad.path().join("missing"), "http://127.0.0.1:1/", free_port());
    ac.claude_upstream = Some(upstream.origin.clone());
    let a = Node::start(ac).await.unwrap();
    let mut bc = cfg(bd.path(), bd.path().join("missing"), "http://127.0.0.1:1/", free_port());
    bc.claude_upstream = Some(upstream.origin.clone());
    let login = desktop_config(&mut bc, DESKTOP_READY);
    let b = Node::start(bc).await.unwrap();
    claude_login(&b, "sk-ant-oat01-CODE_SHOULD_NOT_RUN", "other-code-account");
    let code_settings = b.claude_config().unwrap().home.join("settings.json");
    let settings_bytes = br#"{"env":{"ANTHROPIC_BASE_URL":"https://SECRET_GATEWAY.example","ANTHROPIC_AUTH_TOKEN":"SECRET_CONFIGURED_TOKEN"}}"#;
    std::fs::write(&code_settings, settings_bytes).unwrap();
    assert_eq!(b.status().claude.account, aillies_core::protocol::AccountReadiness::Failed { code: FailureCode::ClaudeLoginUnavailable });
    let code_error = b.set_claude_sharing_from(true, ClaudeAccountSource::Code).await.unwrap_err().to_string();
    assert!(code_error.contains("another gateway") && !code_error.contains("SECRET"));
    assert!(!b.status().claude.sharing && upstream.seen.lock().unwrap().is_empty());
    b.set_claude_sharing_from(true, ClaudeAccountSource::Desktop).await.unwrap();
    assert_eq!(std::fs::read(code_settings).unwrap(), settings_bytes, "desktop sharing must not repair or replace Code settings");
    claude_pair(&a, &b).await;
    a.switch_claude_desktop_provider(true).unwrap();
    assert!(a.status().claude.desktop_installed && !a.status().claude.installed);
    assert!(a.shutdown_if_normal().is_err(), "Quit must not leave desktop Chat pointing to a stopped gateway");
    assert_eq!(claude_call(&a, a.claude_key(), "/claude-desktop/v1/messages", &claude_request()).await.0, 401);
    assert!(upstream.seen.lock().unwrap().is_empty());
    b.grant_claude(a.fingerprint(), true).unwrap();
    assert_eq!(claude_call(&a, a.claude_key(), "/claude-desktop/v1/messages", &claude_request()).await.0, 200);
    assert_eq!(upstream.seen.lock().unwrap()[0].0["authorization"], "Bearer sk-ant-oat01-DESKTOP_OWNER");
    assert_eq!(std::fs::read_to_string(&login).unwrap(), DESKTOP_READY, "sharing never rewrites the owner's login");
    std::fs::write(&login, DESKTOP_ROTATED).unwrap();
    assert_eq!(claude_call(&a, a.claude_key(), "/claude-desktop/v1/messages", &claude_request()).await.0, 200);
    std::fs::write(&login, DESKTOP_EXPIRED).unwrap();
    assert_eq!(claude_call(&a, a.claude_key(), "/claude-desktop/v1/messages", &claude_request()).await.0, 503);
    assert_eq!(upstream.seen.lock().unwrap().len(), 2);
    std::fs::write(&login, DESKTOP_CHANGED).unwrap();
    assert_eq!(claude_call(&a, a.claude_key(), "/claude-desktop/v1/messages", &claude_request()).await.0, 503);
    assert_eq!(upstream.seen.lock().unwrap().len(), 2);
    let journals = format!("{}{}", a.diagnostics(), b.diagnostics());
    assert!(!journals.contains("sk-ant") && !journals.contains("SECRET_SYSTEM") && !journals.contains("11111111-1111"));
    a.switch_claude_desktop_provider(false).unwrap();
    assert!(!a.status().claude.desktop_installed);
    a.shutdown();
    b.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn desktop_retry_stops_when_native_token_rotates() {
    use aillies_node::store::ClaudeAccountSource;
    let upstream = claude_upstream().await;
    upstream.mode.store(2, Ordering::SeqCst);
    let ad = tempfile::tempdir().unwrap();
    let bd = tempfile::tempdir().unwrap();
    let a = Node::start(cfg(ad.path(), ad.path().join("missing"), "http://127.0.0.1:1/", free_port())).await.unwrap();
    let mut bc = cfg(bd.path(), bd.path().join("missing"), "http://127.0.0.1:1/", free_port());
    bc.claude_upstream = Some(upstream.origin.clone());
    let login = desktop_config(&mut bc, DESKTOP_READY);
    let b = Node::start(bc).await.unwrap();
    b.set_claude_sharing_from(true, ClaudeAccountSource::Desktop).await.unwrap();
    claude_pair(&a, &b).await;
    b.grant_claude(a.fingerprint(), true).unwrap();
    let requester = a.clone();
    let task = tokio::spawn(async move { claude_call(&requester, requester.claude_key(), "/claude-desktop/v1/messages", &claude_request()).await });
    wait_for_retry(&a).await;
    std::fs::write(login, DESKTOP_ROTATED).unwrap();
    let (status, _, body) = task.await.unwrap();
    assert_ne!(status, 200);
    assert!(!body.contains("sk-ant"));
    assert_eq!(upstream.seen.lock().unwrap().len(), 1);
    a.shutdown();
    b.shutdown();
}

#[tokio::test]
async fn local_desktop_request_uses_desktop_credentials_and_refuses_missing_cache() {
    let upstream = claude_upstream().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = cfg(dir.path(), dir.path().join("missing"), "http://127.0.0.1:1/", free_port());
    config.claude_upstream = Some(upstream.origin.clone());
    let login = desktop_config(&mut config, DESKTOP_READY);
    let a = Node::start(config).await.unwrap();
    claude_login(&a, "sk-ant-oat01-CODE_LOCAL", "code-local");
    assert_eq!(claude_call(&a, a.claude_key(), "/claude-desktop/v1/messages", &claude_request()).await.0, 200);
    assert_eq!(upstream.seen.lock().unwrap()[0].0["authorization"], "Bearer sk-ant-oat01-DESKTOP_OWNER");
    std::fs::remove_file(login).unwrap();
    assert_eq!(claude_call(&a, a.claude_key(), "/claude-desktop/v1/messages", &claude_request()).await.0, 503);
    assert_eq!(upstream.seen.lock().unwrap().len(), 1);
    assert_eq!(claude_call(&a, a.claude_key(), "/v1/messages", &claude_request()).await.0, 200);
    assert_eq!(upstream.seen.lock().unwrap()[1].0["authorization"], "Bearer sk-ant-oat01-CODE_LOCAL");
    a.shutdown();
}

struct ClaudeUpstream {
    origin: String,
    oauth_mode: Arc<AtomicU8>,
    oauth_seen: Arc<Mutex<Vec<serde_json::Value>>>,
    mode: Arc<AtomicU8>,
    seen: Arc<Mutex<Vec<(hyper::HeaderMap, Bytes)>>>,
    models_seen: Arc<Mutex<Vec<(hyper::HeaderMap, String)>>>,
}
const CLAUDE_SSE: &str = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_test\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-6\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":2,\"output_tokens\":1}}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello Claude\"}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":3}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
const FRONTIER_MODELS: &[&str] = &[
    "claude-opus-4-7",
    "claude-opus-4-8",
    "claude-opus-5",
    "claude-opus-5-5",
    "claude-sonnet-5",
    "claude-sonnet-5-5",
    "claude-haiku-5-5",
    "claude-fable-5",
    "claude-fable-5-1",
    "claude-mythos-5-1",
];
const FRONTIER_SSE: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_frontier\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-5-5\",\"content\":[]}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"NATIVE_THINKING\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"NATIVE_SIGNATURE\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_native\",\"name\":\"read\",\"input\":{}}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
);
async fn claude_upstream() -> ClaudeUpstream {
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let mode = Arc::new(AtomicU8::new(0));
    let oauth_mode = Arc::new(AtomicU8::new(0));
    let oauth_seen: Arc<Mutex<Vec<serde_json::Value>>> = Default::default();
    let (om, os) = (oauth_mode.clone(), oauth_seen.clone());
    let seen: Arc<Mutex<Vec<(hyper::HeaderMap, Bytes)>>> = Default::default();
    let (m, s) = (mode.clone(), seen.clone());
    let models_seen: Arc<Mutex<Vec<(hyper::HeaderMap, String)>>> = Default::default();
    let ms = models_seen.clone();
    tokio::spawn(async move {
        loop {
            let (sock, _) = tcp.accept().await.unwrap();
            let (m, s, om, os, ms) = (m.clone(), s.clone(), om.clone(), os.clone(), ms.clone());
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                    let (m, s, om, os, ms) = (m.clone(), s.clone(), om.clone(), os.clone(), ms.clone());
                    async move {
                        if req.uri().path() == "/v1/models" {
                            assert_eq!(req.method(), "GET");
                            let headers = req.headers().clone();
                            let query = req.uri().query().unwrap_or("").to_owned();
                            assert!(req.into_body().collect().await.unwrap().to_bytes().is_empty());
                            let attempt = {
                                let mut seen = ms.lock().unwrap();
                                seen.push((headers, query));
                                seen.len()
                            };
                            let mode = m.load(Ordering::SeqCst);
                            let status = if mode == 1 {
                                401
                            } else if mode == 2 || mode == 4 && attempt == 1 {
                                503
                            } else {
                                200
                            };
                            let text = if status != 200 || mode == 3 {
                                "{\"error\":\"SECRET_MODELS_ERROR\"}".to_owned()
                            } else if mode == 12 {
                                json!({"data":FRONTIER_MODELS.iter().map(|id| json!({"type":"model","id":id,"display_name":id,"created_at":"2026-10-01T00:00:00Z","capabilities":{"thinking":{"supported":true},"effort":{"supported":true}}})).collect::<Vec<_>>(),"has_more":false,"first_id":FRONTIER_MODELS[0],"last_id":FRONTIER_MODELS.last().unwrap()}).to_string()
                            } else {
                                json!({"data":[{"type":"model","id":"claude-sonnet-4-6","display_name":"Claude Sonnet","created_at":"2026-01-01T00:00:00Z","capabilities":{"thinking":{"supported":true}}}],"has_more":false,"first_id":"claude-sonnet-4-6","last_id":"claude-sonnet-4-6"}).to_string()
                            };
                            return Ok::<_, std::convert::Infallible>(
                                hyper::Response::builder()
                                    .status(status)
                                    .header("content-type", "application/json")
                                    .body(http_body_util::Full::new(Bytes::from(text)))
                                    .unwrap(),
                            );
                        }
                        if req.uri().path() == "/v1/oauth/token" || req.uri().path() == "/api/claude_cli/bootstrap" {
                            let bootstrap = req.uri().path().ends_with("bootstrap");
                            let form = req.headers().get("content-type").is_some_and(|s| s == "application/x-www-form-urlencoded");
                            let body = req.into_body().collect().await.unwrap().to_bytes();
                            let value: serde_json::Value = if bootstrap {
                                json!({"bootstrap":true})
                            } else if form {
                                let pairs: std::collections::HashMap<String, String> =
                                    reqwest::Url::parse(&format!("http://localhost/?{}", std::str::from_utf8(&body).unwrap()))
                                        .unwrap()
                                        .query_pairs()
                                        .into_owned()
                                        .collect();
                                serde_json::to_value(pairs).unwrap()
                            } else {
                                serde_json::from_slice(&body).unwrap()
                            };
                            let refresh = value["grant_type"] == "refresh_token";
                            os.lock().unwrap().push(value);
                            let mode = om.load(Ordering::SeqCst);
                            if mode == 5 {
                                tokio::time::sleep(Duration::from_millis(200)).await;
                            }
                            let mut response = if bootstrap {
                                json!({"oauth_account":{"account_uuid":"11111111-1111-4111-8111-111111111111", "organization_uuid":"22222222-2222-4222-8222-222222222222"}})
                            } else {
                                json!({"token_type":"Bearer","access_token":if refresh {"sk-ant-oat01-REFRESHED"} else {"sk-ant-oat01-CONNECTED"},"refresh_token":if refresh {"refresh-ROTATED"} else {"refresh-INITIAL"},"expires_in":if refresh {3600} else {120},"scope":"user:profile user:inference","account":{"uuid":"11111111-1111-4111-8111-111111111111"},"organization":{"uuid":"22222222-2222-4222-8222-222222222222"}})
                            };
                            if mode == 2 {
                                response["organization"]["uuid"] = "33333333-3333-4333-8333-333333333333".into();
                            }
                            if mode == 3 && !bootstrap {
                                response.as_object_mut().unwrap().remove("account");
                                response.as_object_mut().unwrap().remove("organization");
                            }
                            if mode == 4 {
                                response["scope"] = "user:profile".into();
                            }
                            if mode == 1 {
                                response = json!({"error":"invalid_grant","message":"SECRET_OAUTH_ERROR"});
                            }
                            return Ok::<_, std::convert::Infallible>(
                                hyper::Response::builder()
                                    .status(if mode == 1 { 400 } else { 200 })
                                    .header("content-type", "application/json")
                                    .body(http_body_util::Full::new(Bytes::from(response.to_string())))
                                    .unwrap(),
                            );
                        }
                        let count = req.uri().path().ends_with("count_tokens");
                        let headers = req.headers().clone();
                        let bytes = req.into_body().collect().await.unwrap().to_bytes();
                        let streaming = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["stream"] == true;
                        let attempt = {
                            let mut seen = s.lock().unwrap();
                            seen.push((headers, bytes));
                            seen.len()
                        };
                        let mode = m.load(Ordering::SeqCst);
                        let status = match mode {
                            1 => 401,
                            2 => 503,
                            7 => 400,
                            8 => 403,
                            _ => 200,
                        };
                        let text = if status != 200 {
                            "SECRET_UPSTREAM_ERROR".into()
                        } else if mode == 3 || matches!(mode, 4 | 13) && attempt == 1 {
                            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"DISCARDED_PARTIAL message_stop\"}}\n\n"
                                .into()
                        } else if mode == 5 {
                            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"api_error\",\"message\":\"SECRET_STREAM_ERROR\"}}\n\n".into()
                        } else if matches!(mode, 9..=11) {
                            let kind = match mode {
                                9 => "authentication_error",
                                10 => "permission_error",
                                _ => "invalid_request_error",
                            };
                            format!("event: error\ndata: {{\"type\":\"error\",\"error\":{{\"type\":\"{kind}\",\"message\":\"SECRET_REFUSAL\"}}}}\n\n")
                        } else if count {
                            "{\"input_tokens\":12}".into()
                        } else if !streaming {
                            "{\"type\":\"message\",\"content\":[{\"type\":\"text\",\"text\":\"Hello Claude\"}],\"stop_reason\":\"end_turn\"}".into()
                        } else if matches!(mode, 12 | 13) {
                            FRONTIER_SSE.to_string()
                        } else {
                            CLAUDE_SSE.to_string()
                        };
                        Ok::<_, std::convert::Infallible>(
                            hyper::Response::builder()
                                .status(status)
                                .header("content-type", if streaming { "text/event-stream" } else { "application/json" })
                                .body(http_body_util::Full::new(Bytes::from(text)))
                                .unwrap(),
                        )
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new().serve_connection(hyper_util::rt::TokioIo::new(sock), svc).await;
            });
        }
    });
    ClaudeUpstream { origin, mode, seen, oauth_mode, oauth_seen, models_seen }
}
fn claude_login(node: &Node, token: &str, account: &str) {
    let home = node.claude_config().unwrap().home;
    std::fs::create_dir_all(&home).unwrap();
    aillies_node::store::private_write(
        &home.join(".credentials.json"),
        &serde_json::to_vec(&json!({"claudeAiOauth":{"accessToken":token,"expiresAt":9999999999999u64,"scopes":["user:inference"],"accountUuid":account}}))
            .unwrap(),
    )
    .unwrap();
}
async fn claude_models(node: &Node, key: &str, path: &str) -> (u16, String) {
    let response = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{}{path}", node.loopback_port()))
        .bearer_auth(key)
        .header("anthropic-version", "2023-06-01")
        .header("user-agent", "claude-cli/2.1.227 (external, cli)")
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    assert_eq!(response.headers()["x-should-retry"], "false");
    (status, response.text().await.unwrap())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claude_models_routes_both_clients_with_selected_account_and_peer_grants() {
    use aillies_node::store::ClaudeAccountSource;
    let upstream = claude_upstream().await;
    let ad = tempfile::tempdir().unwrap();
    let bd = tempfile::tempdir().unwrap();
    let a = Node::start(cfg(ad.path(), ad.path().join("missing"), "http://127.0.0.1:1/", free_port())).await.unwrap();
    let mut bc = cfg(bd.path(), bd.path().join("missing"), "http://127.0.0.1:1/", free_port());
    bc.claude_upstream = Some(upstream.origin.clone());
    let b = Node::start(bc).await.unwrap();
    let code = oauth_code(&b.begin_claude_connect().await.unwrap());
    b.finish_claude_connect(&code).await.unwrap();
    b.set_claude_sharing_from(true, ClaudeAccountSource::Connected).await.unwrap();
    claude_pair(&a, &b).await;
    assert_eq!(claude_models(&a, a.codex_key(), "/v1/models").await.0, 401);
    assert_eq!(claude_models(&a, a.claude_key(), "/claude-desktop/v1/models").await.0, 401);
    assert!(upstream.models_seen.lock().unwrap().is_empty());
    b.grant_claude(a.fingerprint(), true).unwrap();
    for path in ["/v1/models?limit=10&after_id=claude-old&lifecycle%5B%5D=active", "/claude-desktop/v1/models?limit=1000&before_id=claude-old"] {
        let (status, text) = claude_models(&a, a.claude_key(), path).await;
        assert_eq!(status, 200);
        assert!(text.contains("claude-sonnet-4-6") && text.contains("capabilities"));
    }
    {
        let seen = upstream.models_seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert!(seen[0].1.contains("limit=10") && seen[0].1.contains("after_id=claude-old") && seen[0].1.contains("lifecycle%5B%5D=active"));
        assert!(seen[1].1.contains("before_id=claude-old"));
        for (headers, query) in seen.iter() {
            assert_eq!(headers["authorization"], "Bearer sk-ant-oat01-REFRESHED");
            assert_eq!(headers["user-agent"], "claude-cli/2.1.227 (external, cli)");
            assert!(headers["anthropic-beta"].to_str().unwrap().contains("oauth-2025-04-20"));
            assert!(!headers.contains_key("x-api-key"));
            assert!(query.contains("beta=true"));
        }
    }
    assert_eq!(claude_models(&b, b.claude_key(), "/v1/models").await.0, 200);
    assert_eq!(claude_models(&b, b.claude_key(), "/claude-desktop/v1/models").await.0, 200);
    for query in ["limit=0", "url=https://evil.example", "after_id=claude-a&before_id=claude-b"] {
        assert_eq!(claude_models(&a, a.claude_key(), &format!("/v1/models?{query}")).await.0, 400);
    }
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .get(format!("http://127.0.0.1:{}/v1/models", a.loopback_port()))
            .bearer_auth(a.claude_key())
            .header("Host", "evil.example")
            .send()
            .await
            .unwrap()
            .status(),
        421
    );
    let text = client.get(format!("http://127.0.0.1:{}/v1/models", a.loopback_port())).bearer_auth(a.claude_key()).body("{}").send().await.unwrap();
    assert_eq!(text.status(), 400);
    b.grant_claude(a.fingerprint(), false).unwrap();
    assert_eq!(claude_models(&a, a.claude_key(), "/v1/models").await.0, 401);
    b.grant_claude(a.fingerprint(), true).unwrap();
    b.set_claude_sharing(false).await.unwrap();
    assert_eq!(claude_models(&a, a.claude_key(), "/v1/models").await.0, 503);
    assert_eq!(upstream.models_seen.lock().unwrap().len(), 4);
    let diagnostics = format!("{}{}", a.diagnostics(), b.diagnostics());
    assert!(!diagnostics.contains("sk-ant") && !diagnostics.contains("claude-sonnet-4-6") && !diagnostics.contains("SECRET"));
    a.shutdown();
    b.shutdown();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claude_models_retries_bounded_and_rejects_malformed_catalog_without_fallback() {
    let upstream = claude_upstream().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = cfg(dir.path(), dir.path().join("missing"), "http://127.0.0.1:1/", free_port());
    config.claude_upstream = Some(upstream.origin.clone());
    let n = Node::start(config).await.unwrap();
    let code = oauth_code(&n.begin_claude_connect().await.unwrap());
    n.finish_claude_connect(&code).await.unwrap();
    upstream.mode.store(4, Ordering::SeqCst);
    assert_eq!(claude_models(&n, n.claude_key(), "/v1/models").await.0, 200);
    assert_eq!(upstream.models_seen.lock().unwrap().len(), 2);
    for (mode, expected) in [(1, 1), (2, 4), (3, 4)] {
        upstream.models_seen.lock().unwrap().clear();
        upstream.mode.store(mode, Ordering::SeqCst);
        let (status, text) = claude_models(&n, n.claude_key(), "/claude-desktop/v1/models").await;
        assert_ne!(status, 200);
        assert!(!text.contains("SECRET") && !text.contains("claude-sonnet") && !text.contains("sk-ant"));
        assert_eq!(upstream.models_seen.lock().unwrap().len(), expected);
    }
    assert!(upstream.seen.lock().unwrap().is_empty());
    n.shutdown();
}
#[tokio::test]
async fn desktop_model_discovery_upgrades_existing_owned_profile_on_node_start() {
    let dir = tempfile::tempdir().unwrap();
    let config = cfg(dir.path(), dir.path().join("missing"), "http://127.0.0.1:1/", free_port());
    let n = Node::start(config.clone()).await.unwrap();
    n.switch_claude_desktop_provider(true).unwrap();
    let library = n.claude_desktop_config().unwrap().library;
    let receipt = config.home.join("claude-desktop-restore.json");
    let mut r: serde_json::Value = serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
    r["installed"]["modelDiscoveryEnabled"] = false.into();
    r["installed"]["inferenceModels"] = json!(["claude-old"]);
    let profile = library.join(format!("{}.json", r["id"].as_str().unwrap()));
    aillies_node::store::private_write(&profile, &serde_json::to_vec(&r["installed"]).unwrap()).unwrap();
    aillies_node::store::private_write(&receipt, &serde_json::to_vec(&r).unwrap()).unwrap();
    n.shutdown();
    tokio::task::yield_now().await;
    let mut restarted = config;
    restarted.loopback_port = 0;
    let n = Node::start(restarted).await.unwrap();
    let upgraded: serde_json::Value = serde_json::from_slice(&std::fs::read(&profile).unwrap()).unwrap();
    assert_eq!(upgraded["modelDiscoveryEnabled"], true);
    assert!(upgraded.get("inferenceModels").is_none());
    assert_eq!(upgraded["inferenceGatewayApiKey"], r["installed"]["inferenceGatewayApiKey"]);
    n.switch_claude_desktop_provider(false).unwrap();
    assert!(!profile.exists() && !receipt.exists());
    n.shutdown();
}
fn claude_request() -> serde_json::Value {
    json!({"model":"claude-sonnet-4-6","max_tokens":1024,"stream":true,"system":[{"type":"text","text":"SECRET_SYSTEM","cache_control":{"type":"ephemeral","ttl":"1h"}}],"messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"SECRET_THINKING","signature":"ORIGINAL_SIGNATURE"},{"type":"tool_use","id":"tool1","name":"read","input":{"path":"file"}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool1","content":"SECRET_TOOL_RESULT"}]}],"tools":[{"name":"read","input_schema":{"type":"object"}}]})
}
async fn claude_call(node: &Node, token: &str, path: &str, request: &serde_json::Value) -> (u16, hyper::HeaderMap, String) {
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}{path}", node.loopback_port()))
        .bearer_auth(token)
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "interleaved-thinking-2025-05-14")
        .header("user-agent", "claude-cli/2.1.227 (external, cli)")
        .json(request)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    (status, headers, response.text().await.unwrap())
}
async fn claude_pair(a: &Arc<Node>, b: &Arc<Node>) {
    let mut invite = aillies_core::protocol::Invite::decode(&b.create_invite().await.unwrap()).unwrap();
    invite.addrs = vec![format!("127.0.0.1:{}", b.peer_port().unwrap())];
    let started = a.start_pairing(&invite.encode()).await.unwrap();
    let id = b.status().pending[0].id.clone();
    b.approve(&id, true).unwrap();
    a.confirm_pairing(&started.id, Duration::from_secs(2)).await.unwrap();
    a.set_claude_destination(b.fingerprint()).unwrap();
}
#[tokio::test]
async fn claude_missing_effort_defaults_for_local_and_peer_clients() {
    use aillies_node::store::ClaudeAccountSource;
    let upstream = claude_upstream().await;
    let ad = tempfile::tempdir().unwrap();
    let bd = tempfile::tempdir().unwrap();
    let a = Node::start(cfg(ad.path(), ad.path().join("missing"), "http://127.0.0.1:1/", free_port())).await.unwrap();
    let mut bc = cfg(bd.path(), bd.path().join("missing"), "http://127.0.0.1:1/", free_port());
    bc.claude_upstream = Some(upstream.origin.clone());
    let b = Node::start(bc).await.unwrap();
    let code = oauth_code(&b.begin_claude_connect().await.unwrap());
    b.finish_claude_connect(&code).await.unwrap();
    b.set_claude_sharing_from(true, ClaudeAccountSource::Connected).await.unwrap();
    claude_pair(&a, &b).await;
    b.grant_claude(a.fingerprint(), true).unwrap();
    for (node, peer) in [(&a, true), (&b, false)] {
        for prefix in ["", "/claude-desktop"] {
            for streaming in [false, true] {
                for effort in [None, Some(json!(null)), Some(json!("unspecified")), Some(json!("high"))] {
                    let mut request = claude_request();
                    request["stream"] = json!(streaming);
                    request["output_config"] = json!({"format":{"type":"json_schema","schema":{"type":"object"}}});
                    if let Some(effort) = &effort {
                        request["output_config"]["effort"] = effort.clone();
                    }
                    let explicit = effort == Some(json!("high"));
                    assert_eq!(claude_call(node, node.claude_key(), &format!("{prefix}/v1/messages"), &request).await.0, 200);
                    let raw = upstream.seen.lock().unwrap().last().unwrap().1.clone();
                    let sent: serde_json::Value = serde_json::from_slice(&raw).unwrap();
                    assert_eq!(sent["output_config"]["effort"], if explicit { "high" } else { "medium" });
                    assert_eq!(sent["output_config"]["format"], request["output_config"]["format"]);
                    for field in ["messages", "system", "tools", "model"] {
                        assert_eq!(sent[field], request[field]);
                    }
                    assert!(sent.get("thinking").is_none());
                    let report = b.diagnostics();
                    let event = report["upstream"].as_array().unwrap().last().unwrap();
                    // The paired executor already receives medium from the sender.
                    assert_eq!(
                        event["request"]["reasoning_effort"],
                        if explicit {
                            "high"
                        } else if peer {
                            "medium"
                        } else {
                            "unspecified"
                        }
                    );
                    assert_eq!(event["event"]["effective_reasoning_effort"], sent["output_config"]["effort"]);
                    let sender = node.diagnostics();
                    let incoming = sender["incoming"].as_array().unwrap().last().unwrap();
                    assert_eq!(incoming["request"]["reasoning_effort"], if explicit { "high" } else { "unspecified" });
                    assert_eq!(incoming["request"]["forwarded_reasoning_effort"], sent["output_config"]["effort"]);
                }
            }
        }
    }
    a.shutdown();
    b.shutdown();
}
#[tokio::test]
async fn claude_frontier_discovery_to_inference_preserves_signed_history_and_streams() {
    use aillies_node::store::ClaudeAccountSource;
    let upstream = claude_upstream().await;
    let ad = tempfile::tempdir().unwrap();
    let bd = tempfile::tempdir().unwrap();
    let a = Node::start(cfg(ad.path(), ad.path().join("missing"), "http://127.0.0.1:1/", free_port())).await.unwrap();
    let mut bc = cfg(bd.path(), bd.path().join("missing"), "http://127.0.0.1:1/", free_port());
    bc.claude_upstream = Some(upstream.origin.clone());
    let b = Node::start(bc).await.unwrap();
    let code = oauth_code(&b.begin_claude_connect().await.unwrap());
    b.finish_claude_connect(&code).await.unwrap();
    b.set_claude_sharing_from(true, ClaudeAccountSource::Connected).await.unwrap();
    claude_pair(&a, &b).await;
    b.grant_claude(a.fingerprint(), true).unwrap();
    upstream.mode.store(12, Ordering::SeqCst);
    for node in [&a, &b] {
        for prefix in ["", "/claude-desktop"] {
            let (status, catalog) = claude_models(node, node.claude_key(), &format!("{prefix}/v1/models")).await;
            assert_eq!(status, 200);
            let catalog: serde_json::Value = serde_json::from_str(&catalog).unwrap();
            for record in catalog["data"].as_array().unwrap() {
                let mut request = claude_request();
                request["model"] = record["id"].clone();
                request["thinking"] = json!({"type":"enabled","budget_tokens":32000});
                request["output_config"] = json!({"effort":"max"});
                request["temperature"] = json!(0.5);
                let (status, _, stream) = claude_call(node, node.claude_key(), &format!("{prefix}/v1/messages"), &request).await;
                assert_eq!(status, 200, "{}", record["id"]);
                assert_eq!(stream, FRONTIER_SSE);
                let (headers, raw) = upstream.seen.lock().unwrap().last().unwrap().clone();
                let sent: serde_json::Value = serde_json::from_slice(&raw).unwrap();
                assert_eq!(sent["model"], request["model"]);
                assert_eq!(sent["thinking"], json!({"type":"adaptive"}));
                assert_eq!(sent["output_config"]["effort"], "max");
                assert!(sent.get("temperature").is_none());
                for field in ["messages", "system", "tools"] {
                    assert_eq!(sent[field], request[field]);
                }
                assert_eq!(headers["authorization"], "Bearer sk-ant-oat01-REFRESHED");
                assert!(!headers.contains_key("x-api-key"));
            }
        }
    }
    for report in [a.diagnostics(), b.diagnostics()] {
        let report = report.to_string();
        for secret in ["SECRET", "ORIGINAL_SIGNATURE", "NATIVE_SIGNATURE", "NATIVE_THINKING", "sk-ant"] {
            assert!(!report.contains(secret));
        }
        assert!(report.contains("claude-opus-5-5"));
    }
    a.shutdown();
    b.shutdown();
}
#[tokio::test]
async fn claude_frontier_retry_is_identical_and_unsupported_options_never_reach_upstream() {
    let upstream = claude_upstream().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = cfg(dir.path(), dir.path().join("missing"), "http://127.0.0.1:1/", free_port());
    config.claude_upstream = Some(upstream.origin.clone());
    let n = Node::start(config).await.unwrap();
    let code = oauth_code(&n.begin_claude_connect().await.unwrap());
    n.finish_claude_connect(&code).await.unwrap();
    let mut request = claude_request();
    request["model"] = json!("claude-opus-5-5");
    request["thinking"] = json!({"type":"enabled","budget_tokens":20000});
    request["speed"] = json!("fast");
    upstream.mode.store(13, Ordering::SeqCst);
    let (status, _, stream) = claude_call(&n, n.claude_key(), "/v1/messages", &request).await;
    assert_eq!(status, 200);
    assert_eq!(stream, FRONTIER_SSE);
    {
        let seen = upstream.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].1, seen[1].1);
        assert!(seen[0].0["anthropic-beta"].to_str().unwrap().contains("fast-mode-2026-02-01"));
    }
    upstream.mode.store(0, Ordering::SeqCst);
    request["stream"] = json!(false);
    assert_eq!(claude_call(&n, n.claude_key(), "/claude-desktop/v1/messages", &request).await.0, 200);
    assert_eq!(claude_call(&n, n.claude_key(), "/v1/messages/count_tokens", &request).await.0, 200);
    upstream.seen.lock().unwrap().clear();
    request["tool_choice"] = json!({"type":"tool","name":"read"});
    let (status, _, text) = claude_call(&n, n.claude_key(), "/v1/messages", &request).await;
    assert_eq!(status, 400);
    assert!(text.contains("These options are not supported"));
    request.as_object_mut().unwrap().remove("tool_choice");
    request["model"] = json!("claude-sonnet-5-5");
    assert_eq!(claude_call(&n, n.claude_key(), "/claude-desktop/v1/messages", &request).await.0, 400);
    assert!(upstream.seen.lock().unwrap().is_empty());
    n.shutdown();
}
#[tokio::test]
async fn claude_native_peer_grants_headers_credentials_and_diagnostics() {
    let upstream = claude_upstream().await;
    let ad = tempfile::tempdir().unwrap();
    let bd = tempfile::tempdir().unwrap();
    let a = Node::start(cfg(ad.path(), ad.path().join("missing-auth"), "http://127.0.0.1:1/", free_port())).await.unwrap();
    let mut bc = cfg(bd.path(), bd.path().join("missing-auth"), "http://127.0.0.1:1/", free_port());
    bc.claude_upstream = Some(upstream.origin.clone());
    let b = Node::start(bc).await.unwrap();
    claude_login(&b, "sk-ant-oat01-SECRET_OWNER_TOKEN", "account-owner");
    b.set_claude_sharing(true).await.unwrap();
    claude_pair(&a, &b).await;
    assert!(!b.status().clients[0].codex, "Claude-only pairing never grants Codex access");
    let mut request = claude_request();
    request["output_config"] = json!({"effort":"medium"});
    assert_eq!(claude_call(&a, a.claude_key(), "/v1/messages?beta=true", &request).await.0, 401);
    assert!(upstream.seen.lock().unwrap().is_empty());
    b.grant_claude(a.fingerprint(), true).unwrap();
    let (status, headers, text) = claude_call(&a, a.claude_key(), "/v1/messages?beta=true", &request).await;
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "text/event-stream");
    assert!(text.contains("Hello Claude"));
    {
        let seen = upstream.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0["authorization"], "Bearer sk-ant-oat01-SECRET_OWNER_TOKEN");
        assert!(!seen[0].0.contains_key("x-api-key"));
        assert_eq!(seen[0].0["user-agent"], "claude-cli/2.1.227 (external, cli)");
        assert_eq!(seen[0].0["anthropic-beta"], "interleaved-thinking-2025-05-14,oauth-2025-04-20,claude-code-20250219");
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&seen[0].1).unwrap(), request);
        assert_eq!(seen[0].1, serde_json::to_vec(&request).unwrap(), "native request bytes must be unchanged");
    }
    for key in [a.codex_key(), a.control_token(), "wrong"] {
        assert_eq!(claude_call(&a, key, "/v1/messages", &request).await.0, 401);
    }
    for report in [a.diagnostics(), b.diagnostics(), serde_json::to_value(b.status()).unwrap()] {
        assert!(!report.to_string().contains("SECRET"));
        assert!(!report.to_string().contains("ORIGINAL_SIGNATURE"));
    }
    assert_eq!(b.diagnostics()["upstream"][0]["request"]["provider"], "claude");
    b.grant_claude(a.fingerprint(), false).unwrap();
    assert_eq!(claude_call(&a, a.claude_key(), "/v1/messages", &request).await.0, 401);
    a.shutdown();
    b.shutdown();
}
#[tokio::test]
async fn claude_retry_budget_refusals_partial_suppression_and_json_endpoints() {
    let upstream = claude_upstream().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = cfg(dir.path(), dir.path().join("missing"), "http://127.0.0.1:1/", free_port());
    config.claude_upstream = Some(upstream.origin.clone());
    let a = Node::start(config).await.unwrap();
    claude_login(&a, "sk-ant-oat01-SECRET_LOCAL_TOKEN", "account-local");
    for (mode, expected) in [(1, 1), (2, 4), (3, 4), (4, 2), (5, 4), (7, 1), (8, 1), (9, 1), (10, 1), (11, 1)] {
        upstream.mode.store(mode, Ordering::SeqCst);
        upstream.seen.lock().unwrap().clear();
        let (_, headers, text) = claude_call(&a, a.claude_key(), "/v1/messages", &claude_request()).await;
        assert_eq!(upstream.seen.lock().unwrap().len(), expected, "mode {mode}");
        assert_eq!(headers["x-should-retry"], "false");
        assert!(!text.contains("SECRET") && !text.contains("DISCARDED_PARTIAL"), "{text}");
        if mode == 4 {
            assert!(text.contains("Hello Claude"));
        }
    }
    upstream.mode.store(0, Ordering::SeqCst);
    let mut request = claude_request();
    request["stream"] = json!(false);
    let (status, headers, text) = claude_call(&a, a.claude_key(), "/v1/messages", &request).await;
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(serde_json::from_str::<serde_json::Value>(&text).unwrap()["type"], "message");
    let (status, _, text) = claude_call(&a, a.claude_key(), "/v1/messages/count_tokens", &request).await;
    assert_eq!(status, 200);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&text).unwrap()["input_tokens"], 12);
    a.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claude_peer_retries_pin_key_and_stop_on_destination_changes() {
    for change in ["account", "token", "destination", "source", "consent"] {
        let upstream = claude_upstream().await;
        upstream.mode.store(2, Ordering::SeqCst);
        let ad = tempfile::tempdir().unwrap();
        let bd = tempfile::tempdir().unwrap();
        let mut ac = cfg(ad.path(), ad.path().join("missing"), "http://127.0.0.1:1/", free_port());
        ac.claude_upstream = Some(upstream.origin.clone());
        let a = Node::start(ac).await.unwrap();
        claude_login(&a, "sk-ant-oat01-SECRET_REQUESTER_TOKEN", "account-requester");
        let mut bc = cfg(bd.path(), bd.path().join("missing"), "http://127.0.0.1:1/", free_port());
        bc.claude_upstream = Some(upstream.origin.clone());
        desktop_config(&mut bc, DESKTOP_READY);
        let b = Node::start(bc).await.unwrap();
        claude_login(&b, "sk-ant-oat01-SECRET_OWNER_TOKEN", "account-owner");
        b.set_claude_sharing(true).await.unwrap();
        claude_pair(&a, &b).await;
        b.grant_claude(a.fingerprint(), true).unwrap();
        let requester = a.clone();
        let task = tokio::spawn(async move { claude_call(&requester, requester.claude_key(), "/v1/messages", &claude_request()).await });
        wait_for_retry(&a).await;
        if change == "account" {
            b.set_claude_sharing(false).await.unwrap();
            claude_login(&b, "sk-ant-oat01-SECRET_NEW_TOKEN", "account-new");
            b.set_claude_sharing(true).await.unwrap();
        } else if change == "source" || change == "consent" {
            b.set_claude_sharing(false).await.unwrap();
            let source = if change == "source" { aillies_node::store::ClaudeAccountSource::Desktop } else { aillies_node::store::ClaudeAccountSource::Code };
            b.set_claude_sharing_from(true, source).await.unwrap();
        } else if change == "token" {
            claude_login(&b, "sk-ant-oat01-SECRET_ROTATED_TOKEN", "account-owner");
        } else {
            a.set_claude_destination("local").unwrap();
        }
        let (_, _, text) = task.await.unwrap();
        assert!(!text.contains("SECRET"));
        assert_eq!(upstream.seen.lock().unwrap().len(), 1);
        if change == "source" || change == "consent" {
            upstream.mode.store(0, Ordering::SeqCst);
            assert_eq!(claude_call(&a, a.claude_key(), "/v1/messages", &claude_request()).await.0, 200, "new turns can use the newly approved scope");
            assert_eq!(upstream.seen.lock().unwrap().len(), 2);
        }
        a.shutdown();
        b.shutdown();
    }
}

#[tokio::test]
async fn claude_native_account_expiry_rotation_and_no_api_key_fallback() {
    let upstream = claude_upstream().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = cfg(dir.path(), dir.path().join("missing"), "http://127.0.0.1:1/", free_port());
    config.claude_upstream = Some(upstream.origin.clone());
    let a = Node::start(config).await.unwrap();
    // A leftover alpha.1 key must never become an account fallback.
    aillies_node::store::private_write(&dir.path().join("home/anthropic-api-key"), b"sk-ant-api03-SECRET_OLD").unwrap();
    assert_eq!(claude_call(&a, a.claude_key(), "/v1/messages", &claude_request()).await.0, 503);
    assert!(upstream.seen.lock().unwrap().is_empty());
    claude_login(&a, "sk-ant-oat01-SECRET_FIRST", "owner");
    a.set_claude_sharing(true).await.unwrap();
    assert_eq!(claude_call(&a, a.claude_key(), "/v1/messages", &claude_request()).await.0, 200);
    claude_login(&a, "sk-ant-oat01-SECRET_ROTATED", "owner");
    assert_eq!(a.status().claude.account, aillies_core::protocol::AccountReadiness::Ready);
    assert_eq!(claude_call(&a, a.claude_key(), "/v1/messages", &claude_request()).await.0, 200);
    let path = a.claude_config().unwrap().home.join(".credentials.json");
    let mut auth: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    auth["claudeAiOauth"]["expiresAt"] = json!(1);
    std::fs::write(path, serde_json::to_vec(&auth).unwrap()).unwrap();
    assert_eq!(claude_call(&a, a.claude_key(), "/v1/messages", &claude_request()).await.0, 503);
    assert_eq!(upstream.seen.lock().unwrap().len(), 2);
    for report in [a.diagnostics(), serde_json::to_value(a.status()).unwrap()] {
        assert!(!report.to_string().contains("SECRET"));
    }
    a.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "installed Claude Code; isolated homes and two nodes, mock Anthropic only"]
async fn installed_claude_cli_uses_native_peer_gateway_without_nested_retries() {
    let upstream = claude_upstream().await;
    let ad = tempfile::tempdir().unwrap();
    let bd = tempfile::tempdir().unwrap();
    let ac = cfg(ad.path(), ad.path().join("missing"), "http://127.0.0.1:1/", free_port());
    let home = ac.claude_home.clone().unwrap();
    let a = Node::start(ac).await.unwrap();
    let mut bc = cfg(bd.path(), bd.path().join("missing"), "http://127.0.0.1:1/", free_port());
    bc.claude_upstream = Some(upstream.origin.clone());
    let b = Node::start(bc).await.unwrap();
    claude_login(&b, "sk-ant-oat01-SECRET_OWNER_TOKEN", "account-owner");
    b.set_claude_sharing(true).await.unwrap();
    claude_pair(&a, &b).await;
    b.grant_claude(a.fingerprint(), true).unwrap();
    a.switch_claude_provider(true).unwrap();
    let path = std::env::var_os("PATH").unwrap_or_default();
    for mode in [0, 1, 2, 3, 5] {
        eprintln!("Claude installed-client mock mode {mode}");
        upstream.mode.store(mode, Ordering::SeqCst);
        upstream.seen.lock().unwrap().clear();
        let home = home.clone();
        let path = path.clone();
        let cwd = ad.path().to_path_buf();
        let result = tokio::task::spawn_blocking(move || {
            let mut child = std::process::Command::new("claude")
                .args(["--print", "Say hello", "--model", "claude-sonnet-4-6", "--max-turns", "1", "--tools", "", "--setting-sources", "user"])
                .env_clear()
                .env("PATH", path)
                .env("HOME", &cwd)
                .env("CLAUDE_CONFIG_DIR", home)
                .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
                .env("DISABLE_AUTOUPDATER", "1")
                .env("HTTP_PROXY", "http://127.0.0.1:1")
                .env("HTTPS_PROXY", "http://127.0.0.1:1")
                .env("NO_PROXY", "127.0.0.1,localhost")
                .current_dir(cwd)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(45);
            loop {
                if child.try_wait().unwrap().is_some() {
                    break;
                }
                if std::time::Instant::now() > deadline {
                    child.kill().unwrap();
                    panic!("isolated Claude mock probe timed out");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            child.wait_with_output().unwrap()
        })
        .await
        .unwrap();
        let text = String::from_utf8_lossy(&result.stdout);
        let seen = upstream.seen.lock().unwrap();
        assert!(!seen.is_empty(), "Claude made no gateway call: {}", String::from_utf8_lossy(&result.stderr));
        if mode == 0 {
            assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
            assert!(text.contains("Hello Claude"), "{text}");
        } else {
            assert_eq!(seen.len(), if mode == 1 { 1 } else { 4 }, "client must not multiply Aillies retries for mode {mode}");
            assert!(!text.contains("DISCARDED_PARTIAL"));
        }
        for (headers, _) in seen.iter() {
            assert_eq!(headers["authorization"], "Bearer sk-ant-oat01-SECRET_OWNER_TOKEN");
            assert!(!headers.contains_key("x-api-key"));
        }
    }
    a.shutdown();
    b.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claude_cancel_before_headers_stops_the_retry_budget() {
    let upstream = claude_upstream().await;
    upstream.mode.store(2, Ordering::SeqCst);
    let dir = tempfile::tempdir().unwrap();
    let mut config = cfg(dir.path(), dir.path().join("missing"), "http://127.0.0.1:1/", free_port());
    config.claude_upstream = Some(upstream.origin.clone());
    let a = Node::start(config).await.unwrap();
    claude_login(&a, "sk-ant-oat01-SECRET_LOCAL_TOKEN", "account-local");
    let requester = a.clone();
    let task = tokio::spawn(async move { claude_call(&requester, requester.claude_key(), "/v1/messages", &claude_request()).await });
    wait_for_retry(&a).await;
    task.abort();
    let _ = task.await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(upstream.seen.lock().unwrap().len(), 1, "cancelled Claude call must not replay");
    a.shutdown();
}

fn oauth_code(url: &str) -> String {
    let url = reqwest::Url::parse(url).unwrap();
    let state = url.query_pairs().find(|(key, _)| key == "state").unwrap().1.into_owned();
    format!("SYNTHETIC_CODE#{state}")
}
fn expire_connected(home: &std::path::Path) {
    let path = home.join("home/claude-oauth-fixture.json");
    let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["expires_at"] = 1.into();
    aillies_node::store::private_write(&path, &serde_json::to_vec(&value).unwrap()).unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claude_connected_oauth_routes_both_clients_rotates_and_keeps_credentials_home() {
    use aillies_node::store::ClaudeAccountSource;
    let upstream = claude_upstream().await;
    let ad = tempfile::tempdir().unwrap();
    let bd = tempfile::tempdir().unwrap();
    let a = Node::start(cfg(ad.path(), ad.path().join("missing"), "http://127.0.0.1:1/", free_port())).await.unwrap();
    let mut bc = cfg(bd.path(), bd.path().join("missing"), "http://127.0.0.1:1/", free_port());
    bc.claude_upstream = Some(upstream.origin.clone());
    let b = Node::start(bc).await.unwrap();
    claude_login(&b, "sk-ant-oat01-NATIVE_MUST_NOT_RUN", "native-other-account");
    let native = b.claude_config().unwrap().home.join(".credentials.json");
    let before = std::fs::read(&native).unwrap();
    let code = oauth_code(&b.begin_claude_connect().await.unwrap());
    b.finish_claude_connect(&code).await.unwrap();
    assert!(b.status().claude.connected && !b.status().claude.sharing);
    assert!(b.finish_claude_connect(&code).await.is_err(), "authorization is one use");
    b.set_claude_sharing_from(true, ClaudeAccountSource::Connected).await.unwrap();
    let calls = upstream.oauth_seen.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["grant_type"], "authorization_code");
    assert_eq!(calls[1]["refresh_token"], "refresh-INITIAL");
    assert_eq!(calls[0]["code_challenge"], serde_json::Value::Null);
    assert!(!calls[0]["code_verifier"].as_str().unwrap().is_empty());
    assert!(b.begin_claude_connect().await.is_err());
    assert!(b.disconnect_claude_account().await.is_err());
    claude_pair(&a, &b).await;
    assert_eq!(claude_call(&a, a.claude_key(), "/claude-desktop/v1/messages", &claude_request()).await.0, 401);
    b.grant_claude(a.fingerprint(), true).unwrap();
    for path in ["/claude-desktop/v1/messages", "/v1/messages"] {
        assert_eq!(claude_call(&a, a.claude_key(), path, &claude_request()).await.0, 200);
    }
    for (headers, _) in upstream.seen.lock().unwrap().iter() {
        assert_eq!(headers["authorization"], "Bearer sk-ant-oat01-REFRESHED");
    }
    assert_eq!(upstream.oauth_seen.lock().unwrap().len(), 2);
    expire_connected(bd.path());
    assert_eq!(claude_call(&a, a.claude_key(), "/v1/messages", &claude_request()).await.0, 200);
    assert_eq!(upstream.oauth_seen.lock().unwrap().last().unwrap()["refresh_token"], "refresh-ROTATED");
    assert_eq!(std::fs::read(native).unwrap(), before);
    for report in [a.diagnostics(), b.diagnostics(), serde_json::to_value(b.status()).unwrap()] {
        let report = report.to_string();
        for secret in ["sk-ant", "refresh-", "SYNTHETIC_CODE", "11111111-1111", "code_verifier"] {
            assert!(!report.contains(secret));
        }
    }
    b.set_claude_sharing(false).await.unwrap();
    b.disconnect_claude_account().await.unwrap();
    assert!(!b.status().claude.connected);
    a.set_claude_destination("local").unwrap();
    assert_eq!(claude_call(&b, b.claude_key(), "/v1/messages", &claude_request()).await.0, 503, "disconnect never falls back to native login");
    a.shutdown();
    b.shutdown();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claude_oauth_cancel_state_scope_bootstrap_and_refresh_fail_closed() {
    use aillies_node::store::ClaudeAccountSource;
    let upstream = claude_upstream().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = cfg(dir.path(), dir.path().join("missing"), "http://127.0.0.1:1/", free_port());
    config.claude_upstream = Some(upstream.origin.clone());
    let n = Node::start(config).await.unwrap();
    claude_login(&n, "sk-ant-oat01-NATIVE_MUST_NOT_RUN", "other");
    let first = oauth_code(&n.begin_claude_connect().await.unwrap());
    n.cancel_claude_connect();
    assert!(n.finish_claude_connect(&first).await.is_err());
    let _ = n.begin_claude_connect().await.unwrap();
    assert!(n.finish_claude_connect("code#wrong").await.is_err());
    assert!(upstream.oauth_seen.lock().unwrap().is_empty());
    upstream.oauth_mode.store(5, Ordering::SeqCst);
    let code = oauth_code(&n.begin_claude_connect().await.unwrap());
    let nn = n.clone();
    let finish = tokio::spawn(async move { nn.finish_claude_connect(&code).await });
    while upstream.oauth_seen.lock().unwrap().is_empty() {
        tokio::task::yield_now().await;
    }
    n.cancel_claude_connect();
    assert!(finish.await.unwrap().is_err());
    assert!(!n.status().claude.connected);
    upstream.oauth_mode.store(4, Ordering::SeqCst);
    let code = oauth_code(&n.begin_claude_connect().await.unwrap());
    assert!(n.finish_claude_connect(&code).await.is_err());
    assert!(!n.status().claude.connected);
    upstream.oauth_mode.store(3, Ordering::SeqCst);
    let code = oauth_code(&n.begin_claude_connect().await.unwrap());
    n.finish_claude_connect(&code).await.unwrap();
    assert_eq!(upstream.oauth_seen.lock().unwrap().last().unwrap()["bootstrap"], true);
    upstream.oauth_mode.store(1, Ordering::SeqCst);
    let error = n.set_claude_sharing_from(true, ClaudeAccountSource::Connected).await.unwrap_err().to_string();
    assert!(!error.contains("SECRET"));
    assert!(!n.status().claude.sharing);
    let attempts = upstream.oauth_seen.lock().unwrap().len();
    upstream.oauth_mode.store(0, Ordering::SeqCst);
    assert!(n.set_claude_sharing_from(true, ClaudeAccountSource::Connected).await.is_err());
    assert_eq!(upstream.oauth_seen.lock().unwrap().len(), attempts, "never replay an uncertain or refused refresh");
    let code = oauth_code(&n.begin_claude_connect().await.unwrap());
    n.finish_claude_connect(&code).await.unwrap();
    upstream.oauth_mode.store(2, Ordering::SeqCst);
    assert!(n.set_claude_sharing_from(true, ClaudeAccountSource::Connected).await.is_err(), "organization change stops refresh");
    assert!(upstream.seen.lock().unwrap().is_empty());
    n.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claude_connected_refresh_serializes_new_turns_and_never_rebinds_a_retry() {
    use aillies_node::store::ClaudeAccountSource;
    let upstream = claude_upstream().await;
    let ad = tempfile::tempdir().unwrap();
    let bd = tempfile::tempdir().unwrap();
    let a = Node::start(cfg(ad.path(), ad.path().join("missing"), "http://127.0.0.1:1/", free_port())).await.unwrap();
    let mut config = cfg(bd.path(), bd.path().join("missing"), "http://127.0.0.1:1/", free_port());
    config.claude_upstream = Some(upstream.origin.clone());
    let b = Node::start(config).await.unwrap();
    let code = oauth_code(&b.begin_claude_connect().await.unwrap());
    b.finish_claude_connect(&code).await.unwrap();
    b.set_claude_sharing_from(true, ClaudeAccountSource::Connected).await.unwrap();
    expire_connected(bd.path());
    let request = claude_request();
    let before = upstream.oauth_seen.lock().unwrap().len();
    let (first, second) =
        tokio::join!(claude_call(&b, b.claude_key(), "/v1/messages", &request), claude_call(&b, b.claude_key(), "/claude-desktop/v1/messages", &request));
    assert_eq!((first.0, second.0), (200, 200));
    assert_eq!(upstream.oauth_seen.lock().unwrap().len(), before + 1, "concurrent new turns perform only one refresh");
    claude_pair(&a, &b).await;
    b.grant_claude(a.fingerprint(), true).unwrap();
    upstream.seen.lock().unwrap().clear();
    upstream.mode.store(3, Ordering::SeqCst);
    let aa = a.clone();
    let call = tokio::spawn(async move { claude_call(&aa, aa.claude_key(), "/v1/messages", &claude_request()).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while upstream.seen.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let path = bd.path().join("home/claude-oauth-fixture.json");
    let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["access_token"] = "sk-ant-oat01-CHANGED_DURING_RETRY".into();
    aillies_node::store::private_write(&path, &serde_json::to_vec(&value).unwrap()).unwrap();
    assert_ne!(call.await.unwrap().0, 200);
    assert_eq!(upstream.seen.lock().unwrap().len(), 1, "retry cannot rebind rotated token");
    assert_eq!(upstream.oauth_seen.lock().unwrap().len(), before + 1, "retry never refreshes");
    a.shutdown();
    b.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn iroh_full_code_keeps_pinned_identity_approvals_and_revocation() {
    iroh_full_code_probe(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "uses public n0 discovery/relay with synthetic accounts and mock inference"]
async fn iroh_public_discovery_full_code_probe() {
    let _ = tracing_subscriber::fmt().with_env_filter("iroh=warn,iroh_relay=warn").try_init();
    iroh_full_code_probe(true).await;
}

async fn iroh_full_code_probe(public: bool) {
    let up = upstream().await;
    let ad = tempfile::tempdir().unwrap();
    let bd = tempfile::tempdir().unwrap();
    let auth = bd.path().join("auth.json");
    write_auth(&auth, 4e9, "iroh-owner");
    let mut ac = cfg(ad.path(), ad.path().join("missing"), &up.url, free_port());
    let mut bc = cfg(bd.path(), auth, &up.url, free_port());
    ac.advertise_loopback = !public;
    bc.advertise_loopback = !public;
    let a = Node::start(ac).await.unwrap();
    let b = Node::start(bc).await.unwrap();
    if public {
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    let mut invite = aillies_core::protocol::Invite::decode(&b.create_invite().await.unwrap()).unwrap();
    invite.addrs.retain(|a| a.starts_with("iroh:"));
    assert_eq!(invite.addrs.len(), 1);
    let mut wrong = invite.clone();
    wrong.fp = "0".repeat(64);
    assert!(a.start_pairing(&wrong.encode()).await.is_err());
    assert!(b.status().pending.is_empty());
    let started = a.start_pairing(&invite.encode()).await.unwrap();
    assert_eq!(b.status().pending[0].code, started.code);
    assert!(b.status().clients.is_empty(), "transport alone cannot grant access");
    b.approve(&b.status().pending[0].id, true).unwrap();
    a.confirm_pairing(&started.id, Duration::from_secs(10)).await.unwrap();
    assert!(a.status().peers[0].addrs.iter().all(|a| a.starts_with("iroh:")));
    assert_eq!(turn(&a, a.codex_key()).await.0, 200);
    assert_eq!(up.hits.load(Ordering::SeqCst), 1);
    b.revoke_client(a.fingerprint()).unwrap();
    assert_ne!(turn(&a, a.codex_key()).await.0, 200);
    assert_eq!(up.hits.load(Ordering::SeqCst), 1);
    b.set_sharing(false).await.unwrap();
    assert!(!a.peer_health(b.fingerprint()).await.reachable);
    // Native synchronous Quit can run outside an entered Tokio runtime.
    std::thread::spawn(move || a.shutdown()).join().unwrap();
    b.shutdown();
}

#[tokio::test]
async fn history_diagnostics_are_authenticated_bounded_and_redacted() {
    let dir = tempfile::tempdir().unwrap();
    let node = Node::start(cfg(dir.path(), dir.path().join("missing"), "http://127.0.0.1:1/", free_port())).await.unwrap();
    let entry = json!({"code":"damaged_record","stage":"prepare","chat_ref":"a".repeat(64),"record":7,"at_unix_seconds":1});
    let mut invalid = entry.clone();
    invalid["chat_ref"] = json!("PRIVATE_PATH_OR_CONTENT");
    aillies_node::store::private_write(&dir.path().join("home/history-diagnostics.json"), &serde_json::to_vec(&vec![entry, invalid]).unwrap()).unwrap();
    let url = format!("http://127.0.0.1:{}/control/diagnostics", node.loopback_port());
    let client = reqwest::Client::new();
    assert_eq!(client.get(&url).send().await.unwrap().status(), 401);
    let data: serde_json::Value = client.get(&url).bearer_auth(node.control_token()).send().await.unwrap().json().await.unwrap();
    assert_eq!(data["history"].as_array().unwrap().len(), 1);
    assert_eq!(data["history"][0]["record"], 7);
    assert!(!data.to_string().contains("PRIVATE_PATH_OR_CONTENT"));
    assert_eq!(node.status().history_warnings.len(), 1);
    node.shutdown();
}
