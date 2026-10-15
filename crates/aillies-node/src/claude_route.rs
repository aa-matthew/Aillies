//! Native Messages adapter over the same authenticated loopback and pinned transport.
use crate::{
    body::{self, Body},
    diagnostics::{observe_response, PendingIncoming, RequestContext, Route},
    node::Node,
    store::{Destination, State},
};
use aillies_core::{
    claude::{Endpoint, Headers, Request as ClaudeRequest},
    request::MAX_REQUEST_BYTES,
    Failure, FailureCode,
};
use bytes::Bytes;
use futures_util::StreamExt;
use hyper::{body::Incoming, Request, Response};
use std::sync::Arc;

fn endpoint(path: &str) -> Endpoint {
    if path.ends_with("/models") {
        Endpoint::Models
    } else if path.ends_with("count_tokens") {
        Endpoint::CountTokens
    } else {
        Endpoint::Messages
    }
}
fn request<B>(req: &Request<B>, raw: &[u8]) -> Result<ClaudeRequest, Failure> {
    if req.headers().contains_key("content-encoding")
        || req.headers().get_all("anthropic-version").iter().count() > 1
        || req.headers().get_all("anthropic-beta").iter().count() > 1
        || req.headers().get_all("user-agent").iter().count() > 1
    {
        return Err(Failure::new(FailureCode::RequestInvalid));
    }
    let version =
        req.headers().get("anthropic-version").map(|h| h.to_str()).transpose().map_err(|_| Failure::new(FailureCode::RequestInvalid))?.unwrap_or("2023-06-01");
    let beta = req.headers().get("anthropic-beta").map(|h| h.to_str()).transpose().map_err(|_| Failure::new(FailureCode::RequestInvalid))?;
    let endpoint = endpoint(req.uri().path());
    if req.method().as_str() != endpoint.method() {
        return Err(Failure::new(FailureCode::RequestInvalid));
    }
    let mut headers = Headers::new(version, beta)?;
    headers.client(req.headers().get("user-agent").map(|h| h.to_str()).transpose().map_err(|_| Failure::new(FailureCode::RequestInvalid))?)?;
    let mut request = ClaudeRequest::parse(raw, endpoint, headers)?;
    request.set_query(req.uri().query())?;
    Ok(request)
}
fn no_retry(mut response: Response<Body>) -> Response<Body> {
    response.headers_mut().insert("x-should-retry", "false".parse().expect("static header"));
    response
}
async fn refuse(node: Arc<Node>, context: RequestContext, failure: Failure) -> Response<Body> {
    let _ = observe_response(node, body::failure(&failure), context).await;
    no_retry(body::json(failure.http_status(), &aillies_core::claude::failure_json(&failure)))
}
pub(crate) async fn incoming(node: Arc<Node>, req: Request<Incoming>) -> Response<Body> {
    let empty = RequestContext::from_claude(Route::Loopback, b"", false);
    if !body::bearer_ok(req.headers(), node.claude_key()) {
        let f = Failure::new(FailureCode::Unauthorized);
        return refuse(node, empty, f).await;
    }
    let (parts, body) = req.into_parts();
    let raw = match body::read_limited(body, MAX_REQUEST_BYTES).await {
        Ok(raw) => raw,
        Err(_) => {
            let f = Failure::new(FailureCode::RequestInvalid);
            return refuse(node, empty, f).await;
        }
    };
    let req = Request::from_parts(parts, ());
    let original = raw;
    let raw = match aillies_core::claude::default_sender_effort(&original, endpoint(req.uri().path())) {
        Ok(std::borrow::Cow::Borrowed(_)) => original.clone(),
        Ok(std::borrow::Cow::Owned(body)) => Bytes::from(body),
        Err(f) => return refuse(node, RequestContext::from_claude(Route::Loopback, &original, false), f).await,
    };
    let mut request = match request(&req, &raw) {
        Ok(r) => r,
        Err(f) => {
            return refuse(node, RequestContext::from_claude(Route::Loopback, &original, false), f).await;
        }
    };
    request.desktop = req.uri().path().starts_with("/claude-desktop/");
    let state = node.snapshot();
    let route = match state.claude.destination {
        Destination::Local => Route::Local,
        Destination::Peer(_) => Route::Peer,
    };
    let context = RequestContext::from_claude(route, &original, !request.streaming).with_claude_forwarded(&raw);
    let pending = PendingIncoming::new(node.clone(), context);
    let response = crate::retry::route_claude(node.clone(), raw, state, context, request).await;
    let response = observe_response(node, response, context).await;
    pending.handoff();
    let response = if response.status() != 200 {
        use http_body_util::BodyExt;
        let status = response.status().as_u16();
        let bytes = http_body_util::Limited::new(response.into_body(), 4096).collect().await.map(|b| b.to_bytes()).unwrap_or_default();
        let failure = Failure::from_json(&bytes).unwrap_or(Failure::new(FailureCode::DeliveryUncertain));
        body::json(status, &aillies_core::claude::failure_json(&failure))
    } else {
        response
    };
    no_retry(response)
}
pub(crate) async fn route_inner(
    node: Arc<Node>,
    raw: Bytes,
    state: State,
    context: RequestContext,
    request: ClaudeRequest,
    local: Option<Arc<aillies_claude::Executor>>,
) -> Response<Body> {
    if !state.routing {
        return body::failure(&Failure::new(FailureCode::RoutingPaused));
    }
    match state.claude.destination.clone() {
        Destination::Local => {
            let exec = match local {
                Some(e) => e,
                None => return body::failure(&Failure::new(FailureCode::ClaudeLoginUnavailable)),
            };
            match exec.execute(&raw, &request, Some(node.upstream_observer(context))).await {
                Ok(stream) => response(stream, request.streaming),
                Err(f) => body::failure(&f),
            }
        }
        Destination::Peer(fp) => {
            let Some(peer) = state.peer(&fp).cloned() else { return body::failure(&Failure::new(FailureCode::NoDestination)) };
            crate::loopback::forward(node, peer, raw, context, None, Some(request)).await
        }
    }
}
fn response(stream: aillies_core::telemetry::TurnStream, streaming: bool) -> Response<Body> {
    let body = body::stream(stream.map(|r| r.map_err(|f| std::io::Error::other(f.message()))));
    if streaming {
        body::sse(body)
    } else {
        Response::builder().status(200).header("content-type", "application/json").body(body).expect("response")
    }
}
pub(crate) async fn hosted(node: Arc<Node>, client: String, req: Request<Incoming>) -> Response<Body> {
    let mut context = RequestContext::from_claude(Route::Hosted, b"", false);
    let result = hosted_inner(node.clone(), client, req, &mut context).await;
    observe_response(node, result, context).await
}

async fn hosted_inner(node: Arc<Node>, client: String, req: Request<Incoming>, context: &mut RequestContext) -> Response<Body> {
    let state = node.snapshot();
    if !state.client(&client).is_some_and(|c| c.claude) {
        return body::failure(&Failure::new(FailureCode::NotApproved));
    }
    if !state.claude.sharing {
        return body::failure(&Failure::new(FailureCode::SharingOff));
    }
    let Ok(permit) = node.turns.clone().try_acquire_owned() else { return body::failure(&Failure::new(FailureCode::PeerBusy)) };
    let trace = req.headers().get("x-aillies-trace").and_then(|h| h.to_str().ok()).and_then(|h| uuid::Uuid::parse_str(h).ok());
    let attempt = req.headers().get("x-aillies-attempt").and_then(|h| h.to_str().ok()).and_then(|s| s.parse::<u8>().ok()).filter(|n| (1..=4).contains(n));
    let initial_send = match req.headers().get("x-aillies-initial-send").and_then(|h| h.to_str().ok()) {
        Some("true") => true,
        Some("false") => false,
        _ => return body::failure(&Failure::new(FailureCode::RequestInvalid)),
    };
    let (Some(trace), Some(attempt)) = (trace, attempt) else { return body::failure(&Failure::new(FailureCode::RequestInvalid)) };
    let (parts, body) = req.into_parts();
    let raw = match body::read_limited(body, MAX_REQUEST_BYTES).await {
        Ok(raw) => raw,
        Err(_) => return body::failure(&Failure::new(FailureCode::RequestInvalid)),
    };
    let req = Request::from_parts(parts, ());
    let request = match request(&req, &raw) {
        Ok(r) => r,
        Err(f) => return body::failure(&f),
    };
    *context = RequestContext::from_claude(Route::Hosted, &raw, !request.streaming);
    context.trace_id = trace;
    context.attempt = attempt;
    let pending = PendingIncoming::new(node.clone(), *context);
    let result = async {
        let exec = match node.claude_retry_executor(&client, trace, initial_send).await {
            Ok(e) => e,
            Err(f) => return body::failure(&f),
        };
        node.touch_client(&client);
        match exec.execute(&raw, &request, Some(node.upstream_observer(*context))).await {
            Ok(stream) => {
                let stream = Box::pin(stream.map(move |r| {
                    let _keep = &permit;
                    r
                }));
                response(stream, request.streaming)
            }
            Err(f) => body::failure(&f),
        }
    }
    .await;
    pending.handoff();
    result
}
