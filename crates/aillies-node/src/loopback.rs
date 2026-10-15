//! 127.0.0.1 API: Codex `/v1/responses` and the local `/control/*` API.

use crate::body::{self, Body};
use crate::diagnostics::{observe_response, RequestContext, Route};
use crate::node::Node;
use crate::peer_client;
use crate::store::Destination;
use aillies_core::request::MAX_REQUEST_BYTES;
use aillies_core::wire::TerminalScanner;
use aillies_core::{Failure, FailureCode};
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::net::TcpListener;

pub async fn serve(node: Arc<Node>, listener: TcpListener) {
    loop {
        let Ok((sock, peer)) = listener.accept().await else { continue };
        if !peer.ip().is_loopback() {
            continue;
        }
        let node = node.clone();
        tokio::spawn(async move {
            let svc = hyper::service::service_fn(move |req| handle(node.clone(), req));
            let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(sock), svc).await;
        });
    }
}

/// Block DNS-rebinding: only accept Host headers naming loopback.
fn host_ok(req: &Request<Incoming>) -> bool {
    let Some(h) = req.headers().get("host").and_then(|v| v.to_str().ok()) else { return false };
    let host = h.rsplit_once(':').map(|(a, _)| a).unwrap_or(h);
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

async fn handle(node: Arc<Node>, req: Request<Incoming>) -> Result<Response<Body>, Infallible> {
    if !host_ok(&req) {
        return Ok(body::json(421, &serde_json::json!({"error": "bad host"})));
    }
    let path = req.uri().path().to_string();
    if path == "/healthz" {
        return Ok(body::json(200, &serde_json::json!({"ok": true, "product": "Aillies", "version": aillies_core::APP_VERSION})));
    }
    if path.starts_with("/control/") {
        if !body::bearer_ok(req.headers(), node.control_token()) {
            return Ok(body::json(401, &serde_json::json!({"error": "Missing or wrong control token."})));
        }
        return Ok(crate::control::handle(node, req).await);
    }
    if req.method() == "GET" && matches!(path.as_str(), "/v1/models" | "/claude-desktop/v1/models") {
        return Ok(crate::claude_route::incoming(node, req).await);
    }
    if req.method() == "POST"
        && matches!(path.as_str(), "/v1/messages" | "/v1/messages/count_tokens" | "/claude-desktop/v1/messages" | "/claude-desktop/v1/messages/count_tokens")
    {
        return Ok(crate::claude_route::incoming(node, req).await);
    }
    if path == "/mcp/images" {
        if !body::bearer_ok(req.headers(), node.codex_key()) {
            return Ok(body::failure(&Failure::new(FailureCode::Unauthorized)));
        }
        return Ok(crate::image_mcp::handle(node, req).await);
    }
    if req.method() == "POST" && (path == "/v1/responses" || path == "/responses") {
        if !body::bearer_ok(req.headers(), node.codex_key()) {
            let f = Failure::new(FailureCode::Unauthorized);
            node.record_diagnostic(RequestContext::from_raw(Route::Loopback, b""), &f);
            return Ok(body::failure(&f));
        }
        return Ok(codex_turn(node, req).await);
    }
    Ok(body::json(404, &serde_json::json!({"error": "not found"})))
}

fn fail(node: &Node, f: Failure) -> Response<Body> {
    tracing::warn!(code = ?f.code, "codex turn refused");
    node.record_failure(&f);
    body::failure(&f)
}

async fn codex_turn(node: Arc<Node>, req: Request<Incoming>) -> Response<Body> {
    let session_hint = aillies_codex::session::SESSION_HEADERS
        .iter()
        .find_map(|key| req.headers().get(*key).and_then(|v| v.to_str().ok()).and_then(aillies_codex::session::bounded));
    let Ok(raw) = body::read_limited(req.into_body(), MAX_REQUEST_BYTES).await else {
        let f = Failure::new(FailureCode::RequestInvalid);
        node.record_diagnostic(RequestContext::from_raw(Route::Loopback, b""), &f);
        return body::failure(&f);
    };
    let hint = aillies_codex::session::client_session(&raw, session_hint.as_deref());
    route_with_session(node, raw, hint).await
}

pub(crate) async fn route_turn(node: Arc<Node>, raw: Bytes) -> Response<Body> {
    let hint = aillies_codex::session::client_session(&raw, None);
    route_with_session(node, raw, hint).await
}

async fn route_with_session(node: Arc<Node>, raw: Bytes, hint: Option<String>) -> Response<Body> {
    let state = node.snapshot();
    let route = match state.destination {
        Destination::Local => Route::Local,
        Destination::Peer(_) => Route::Peer,
    };
    let fast = match &state.destination {
        Destination::Local => state.local_fast_mode,
        Destination::Peer(fp) => state.peer(fp).is_some_and(|p| p.fast_mode),
    };
    let context = RequestContext::from_raw(route, &raw).with_fast(fast);
    let pending = crate::diagnostics::PendingIncoming::new(node.clone(), context);
    let response = crate::retry::route(node.clone(), raw, state, context, hint).await;
    let response = observe_response(node, response, context).await;
    pending.handoff();
    response
}

pub(crate) async fn route_turn_inner(
    node: Arc<Node>,
    raw: Bytes,
    state: crate::store::State,
    context: RequestContext,
    hint: Option<String>,
    local: Option<Arc<aillies_codex::Executor>>,
) -> Response<Body> {
    if !state.routing {
        tracing::warn!("codex turn refused: routing paused");
        return body::failure(&Failure::new(FailureCode::RoutingPaused));
    }
    let fast = match &state.destination {
        Destination::Local => state.local_fast_mode,
        Destination::Peer(fp) => state.peer(fp).is_some_and(|p| p.fast_mode),
    };
    let raw = match aillies_core::request::select_service_tier(&raw, fast) {
        Ok(raw) => Bytes::from(raw),
        Err(_) => return fail(&node, Failure::new(FailureCode::RequestInvalid)),
    };
    match state.destination.clone() {
        Destination::Local => {
            let exec = match local.map(Ok).unwrap_or_else(|| node.local_executor()) {
                Ok(e) => e,
                Err(f) => return fail(&node, f),
            };
            match exec.execute_observed(&raw, hint.as_deref(), Some(node.upstream_observer(context))).await {
                Ok(stream) => {
                    node.clear_failure();
                    body::sse(body::stream(stream.map(|r| r.map_err(|f| std::io::Error::other(f.message())))))
                }
                Err(f) => fail(&node, f),
            }
        }
        Destination::Peer(fp) => {
            let Some(peer) = state.peer(&fp).cloned() else { return fail(&node, Failure::new(FailureCode::NoDestination)) };
            forward(node, peer, raw, context, hint, None).await
        }
    }
}

/// Forward exactly once. No retry on another address after the request is
/// written, no other device, no local fallback.
pub(crate) async fn forward(
    node: Arc<Node>,
    peer: crate::store::PeerRecord,
    raw: Bytes,
    context: RequestContext,
    hint: Option<String>,
    claude: Option<aillies_core::claude::Request>,
) -> Response<Body> {
    let Some(mut conn) = peer_client::connect_with_internet(&node.identity, &node.internet, &peer.fp, &peer.addrs).await else {
        return fail(&node, Failure::new(FailureCode::PeerUnavailable));
    };
    let path = match &claude {
        Some(r) => format!("/v1/claude{}{}{}", r.endpoint.path().trim_start_matches("/v1"), if r.query.is_empty() { "" } else { "?" }, r.query),
        None => "/v1/codex/responses".into(),
    };
    let mut request = peer_client::request(claude.as_ref().map(|r| r.endpoint.method()).unwrap_or("POST"), &path, raw);
    if let Some(r) = &claude {
        request.headers_mut().insert("anthropic-version", r.headers.version.parse().expect("validated version"));
        if let Some(agent) = &r.headers.user_agent {
            request.headers_mut().insert("user-agent", agent.parse().expect("validated user agent"));
        }
        if let Some(beta) = &r.headers.beta {
            request.headers_mut().insert("anthropic-beta", beta.parse().expect("validated beta"));
        }
    }
    request.headers_mut().insert("x-aillies-trace", context.trace_id.to_string().parse().expect("UUID header"));
    request.headers_mut().insert("x-aillies-attempt", context.attempt.to_string().parse().expect("attempt header"));
    request.headers_mut().insert("x-aillies-initial-send", if context.initial_send { "true" } else { "false" }.parse().expect("initial send header"));
    if let Some(hint) = hint.and_then(|s| aillies_codex::session::bounded(&s)) {
        if let Ok(value) = hint.parse() {
            request.headers_mut().insert("x-aillies-session", value);
        }
    }
    let resp = match conn.sender.send_request(request).await {
        Ok(r) => r,
        Err(_) => return fail(&node, Failure::new(FailureCode::DeliveryUncertain)),
    };
    let status = resp.status().as_u16();
    if status != 200 {
        let bytes = http_body_util::Limited::new(resp.into_body(), 4096).collect().await.map(|b| b.to_bytes()).unwrap_or_default();
        let f = Failure::from_json(&bytes).unwrap_or(Failure::new(match status {
            401 => FailureCode::NotApproved,
            429 => FailureCode::PeerBusy,
            426 => FailureCode::ProtocolMismatch,
            _ => FailureCode::DeliveryUncertain,
        }));
        return fail(&node, f);
    }
    node.clear_failure();
    tracing::info!(peer = %aillies_core::protocol::short_id(&peer.fp), "request streaming from peer");
    let json = claude.as_ref().is_some_and(|r| !r.streaming);
    let mut scanner = if claude.is_some() { TerminalScanner::claude() } else { TerminalScanner::default() };
    let watcher = node.clone();
    let mut incoming = resp.into_body();
    let s = async_stream(move |tx| async move {
        loop {
            match incoming.frame().await {
                Some(Ok(frame)) => {
                    if let Ok(data) = frame.into_data() {
                        scanner.feed(&data);
                        if tx.send(Ok(data)).await.is_err() {
                            return; // Codex went away; dropping cancels upstream.
                        }
                    }
                }
                Some(Err(_)) => {
                    watcher.record_failure(&Failure::new(FailureCode::DeliveryUncertain));
                    let _ = tx.send(Err(std::io::Error::other("peer stream interrupted"))).await;
                    return;
                }
                None => {
                    if !json && !scanner.completed {
                        watcher.record_failure(&Failure::new(FailureCode::DeliveryUncertain));
                        let _ = tx.send(Err(std::io::Error::other("peer stream ended early"))).await;
                    }
                    return;
                }
            }
        }
    });
    body::sse(body::stream(s))
}

/// Tiny channel-backed stream so the forwarding loop can own its state.
fn async_stream<F, Fut>(f: F) -> impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send
where
    F: FnOnce(tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let task = tokio::spawn(f(tx));
    let guard = AbortOnDrop(task);
    futures_util::stream::poll_fn(move |cx| {
        let _g = &guard;
        rx.poll_recv(cx)
    })
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
