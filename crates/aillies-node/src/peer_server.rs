//! Sharing side: pinned mutual-TLS listener for approved devices.

use crate::body::{self, Body};
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
            let Ok(Ok(tls)) = tokio::time::timeout(Duration::from_secs(10), acceptor.accept(sock)).await else { return };
            let Some(fp) = tls.get_ref().1.peer_certificates().and_then(|c| c.first()).map(|c| fingerprint(c.as_ref())) else { return };
            let svc = hyper::service::service_fn(move |req| handle(node.clone(), fp.clone(), req));
            let conn = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(tls), svc);
            tokio::select! {
                _ = conn => {}
                _ = epoch.changed() => {}
            }
        });
    }
}

async fn handle(node: Arc<Node>, client_fp: String, req: Request<Incoming>) -> Result<Response<Body>, Infallible> {
    if let Some(v) = req.headers().get(PAIR_HEADER_VERSION) {
        if v.to_str().ok() != Some(&PROTOCOL_VERSION.to_string()) {
            return Ok(body::failure(&Failure::new(FailureCode::ProtocolMismatch)));
        }
    }
    let path = req.uri().path().to_string();
    let method = req.method().as_str().to_string();
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let approved = || node.snapshot().client(&client_fp).is_some();

    Ok(match (method.as_str(), parts.as_slice()) {
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
                body::json(200, &node.peer_status_doc())
            } else {
                body::failure(&Failure::new(FailureCode::NotApproved))
            }
        }
        ("POST", ["v1", "codex", "responses"]) => {
            if !approved() {
                return Ok(body::failure(&Failure::new(FailureCode::NotApproved)));
            }
            if !node.snapshot().sharing {
                return Ok(body::failure(&Failure::new(FailureCode::SharingOff)));
            }
            let Ok(permit) = node.turns.clone().try_acquire_owned() else {
                return Ok(body::failure(&Failure::new(FailureCode::PeerBusy)));
            };
            let Ok(raw) = body::read_limited(req.into_body(), MAX_REQUEST_BYTES).await else {
                return Ok(body::failure(&Failure::new(FailureCode::RequestInvalid)));
            };
            let exec = match node.host_executor() {
                Ok(e) => e,
                Err(f) => return Ok(body::failure(&f)),
            };
            node.touch_client(&client_fp);
            match exec.execute(&raw).await {
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
        _ => body::json(404, &serde_json::json!({"error": "not found"})),
    })
}
