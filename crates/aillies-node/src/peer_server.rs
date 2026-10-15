//! Sharing side: pinned mutual-TLS listener for approved devices.

use crate::body::{self, Body};
use crate::diagnostics::{observe_response, RequestContext, Route};
use crate::identity::fingerprint;
use crate::node::Node;
use aillies_core::protocol::{PairRequest, PAIR_HEADER_VERSION, PROTOCOL_VERSION};
use aillies_core::request::MAX_REQUEST_BYTES;
use aillies_core::{Failure, FailureCode};
use futures_util::StreamExt;
use hyper::body::Incoming;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;

const MAX_CONNECTIONS: usize = 64;

pub async fn serve(node: Arc<Node>, tcp: TcpListener, tls: Arc<rustls::ServerConfig>) {
    let acceptor = TlsAcceptor::from(tls);
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    loop {
        let Ok((sock, _)) = tcp.accept().await else { continue };
        let Ok(permit) = slots.clone().try_acquire_owned() else { continue };
        let node = node.clone();
        let acceptor = acceptor.clone();
        let mut epoch = node.epoch.subscribe();
        tokio::spawn(async move {
            let _permit = permit;
            serve_stream(node, acceptor, sock, &mut epoch).await;
        });
    }
}

async fn handle(node: Arc<Node>, client_fp: String, req: Request<Incoming>) -> Result<Response<Body>, Infallible> {
    if let Some(v) = req.headers().get(PAIR_HEADER_VERSION) {
        if v.to_str().ok() != Some(&PROTOCOL_VERSION.to_string()) {
            let f = Failure::new(FailureCode::ProtocolMismatch);
            if req.method() == "POST" && req.uri().path() == "/v1/codex/responses" {
                node.record_diagnostic(RequestContext::from_raw(Route::Hosted, b""), &f);
            }
            return Ok(body::failure(&f));
        }
    }
    let path = req.uri().path().to_string();
    let method = req.method().as_str().to_string();
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let approved = || node.snapshot().client(&client_fp).is_some();

    Ok(match (method.as_str(), parts.as_slice()) {
        ("POST", ["v1", "pair", "bootstrap"]) => {
            let Ok(raw) = body::read_limited(req.into_body(), 256).await else { return Ok(body::failure(&Failure::new(FailureCode::RequestInvalid))) };
            match serde_json::from_slice::<crate::pairing::BootstrapRequest>(&raw) {
                Ok(request) => match node.bootstrap_pairing(&client_fp, &request) {
                    Ok(reply) => body::json(200, &reply),
                    Err(f) => body::failure(&f),
                },
                Err(_) => body::failure(&Failure::new(FailureCode::RequestInvalid)),
            }
        }
        ("POST", ["v1", "pair"]) => {
            let Ok(raw) = body::read_limited(req.into_body(), 4096).await else { return Ok(body::failure(&Failure::new(FailureCode::RequestInvalid))) };
            match serde_json::from_slice::<PairRequest>(&raw) {
                Ok(pr) => match node.accept_pair_request(&client_fp, pr) {
                    Ok(state) => body::json(200, &state),
                    Err(f) => body::failure(&f),
                },
                Err(_) => body::failure(&Failure::new(FailureCode::RequestInvalid)),
            }
        }
        ("GET", ["v1", "pair", id]) => body::json(200, &node.pair_poll(&client_fp, id)),
        ("POST", ["v1", "pair", id, "cancel"]) => {
            node.pair_cancel(&client_fp, id);
            body::json(200, &serde_json::json!({}))
        }
        ("GET", ["v1", "status"]) => {
            if approved() {
                if let Some(name) = req.headers().get("x-aillies-device-name").and_then(|h| h.to_str().ok()) {
                    node.refresh_client_name(&client_fp, name);
                }
                let state = node.snapshot();
                let client = state.client(&client_fp);
                let mut status = node.peer_status_doc();
                status.providers.retain(|p| match p.provider {
                    aillies_core::protocol::Provider::Codex => state.sharing && client.is_some_and(|c| c.codex),
                    aillies_core::protocol::Provider::Claude => state.claude.sharing && client.is_some_and(|c| c.claude),
                });
                status.sharing = !status.providers.is_empty();
                body::json(200, &status)
            } else {
                body::failure(&Failure::new(FailureCode::NotApproved))
            }
        }
        ("POST", ["v1", "claude", "messages"]) | ("POST", ["v1", "claude", "messages", "count_tokens"]) | ("GET", ["v1", "claude", "models"]) => {
            crate::claude_route::hosted(node, client_fp, req).await
        }
        ("POST", ["v1", "codex", "responses"]) => {
            let mut context = RequestContext::from_raw(Route::Hosted, b"");
            let response = codex_turn(node.clone(), client_fp, req, &mut context).await;
            observe_response(node, response, context).await
        }
        _ => body::json(404, &serde_json::json!({"error": "not found"})),
    })
}

async fn codex_turn(node: Arc<Node>, client_fp: String, req: Request<Incoming>, context: &mut RequestContext) -> Response<Body> {
    if !node.snapshot().client(&client_fp).is_some_and(|c| c.codex) {
        return body::failure(&Failure::new(FailureCode::NotApproved));
    }
    if !node.snapshot().sharing {
        return body::failure(&Failure::new(FailureCode::SharingOff));
    }
    let Ok(permit) = node.turns.clone().try_acquire_owned() else {
        return body::failure(&Failure::new(FailureCode::PeerBusy));
    };
    let trace = req.headers().get("x-aillies-trace").and_then(|h| h.to_str().ok()).and_then(|h| uuid::Uuid::parse_str(h).ok());
    let attempt = match req.headers().get("x-aillies-attempt") {
        None => None,
        Some(value) => match value.to_str().ok().and_then(|value| value.parse::<u8>().ok()).filter(|attempt| (1..=4).contains(attempt)) {
            Some(attempt) if trace.is_some() => Some(attempt),
            _ => return body::failure(&Failure::new(FailureCode::RequestInvalid)),
        },
    };
    let hint = req.headers().get("x-aillies-session").and_then(|h| h.to_str().ok()).and_then(aillies_codex::session::bounded);
    let initial_send = match req.headers().get("x-aillies-initial-send").and_then(|value| value.to_str().ok()) {
        Some("true") => true,
        Some("false") => false,
        _ if attempt.is_none() => true,
        _ => return body::failure(&Failure::new(FailureCode::RequestInvalid)),
    };
    let Ok(raw) = body::read_limited(req.into_body(), MAX_REQUEST_BYTES).await else {
        return body::failure(&Failure::new(FailureCode::RequestInvalid));
    };
    *context = RequestContext::from_raw(Route::Hosted, &raw);
    if let Some(trace) = trace {
        context.trace_id = trace;
    }
    if let Some(attempt) = attempt {
        context.attempt = attempt;
    }
    let pending = crate::diagnostics::PendingIncoming::new(node.clone(), *context);
    let response = async {
        if let Err(code) = aillies_core::request::validate_shared_tier(&raw, node.snapshot().allow_fast_mode) {
            return body::failure(&Failure::new(code));
        }
        let exec = match if let (Some(trace), Some(_)) = (trace, attempt) {
            node.host_retry_executor(&client_fp, trace, initial_send)
        } else {
            node.host_executor()
        } {
            Ok(e) => e,
            Err(f) => return body::failure(&f),
        };
        node.touch_client(&client_fp);
        match exec.execute_observed(&raw, hint.as_deref(), Some(node.upstream_observer(*context))).await {
            Ok(stream) => {
                // The permit lives as long as the stream: a dropped requester frees the slot.
                let s = stream.map(move |r| {
                    let _keep = &permit;
                    r.map_err(|f| std::io::Error::other(f.message()))
                });
                body::sse(body::stream(s))
            }
            Err(f) => body::failure(&f),
        }
    }
    .await;
    pending.handoff();
    response
}

/// The same pinned mTLS and request authorization apply to TCP and Iroh.
pub(crate) async fn serve_stream<S>(node: Arc<Node>, acceptor: TlsAcceptor, sock: S, epoch: &mut tokio::sync::watch::Receiver<u64>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let Ok(Ok(tls)) = tokio::time::timeout(Duration::from_secs(10), acceptor.accept(sock)).await else { return };
    let Some(fp) = tls.get_ref().1.peer_certificates().and_then(|c| c.first()).map(|c| fingerprint(c.as_ref())) else { return };
    let svc = hyper::service::service_fn(move |req| handle(node.clone(), fp.clone(), req));
    let conn = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(tls), svc);
    tokio::select! {
        _ = conn => {}
        _ = epoch.changed() => {}
    }
}
