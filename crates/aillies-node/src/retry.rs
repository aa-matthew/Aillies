//! The requesting node owns one retry budget. Peers execute each attempt once.
//! Hold generation events until success; only keepalive comments escape early.
use crate::{
    body::{self, Body},
    diagnostics::RequestContext,
    node::Node,
    store::{Destination, State},
};
use aillies_codex::Executor;
use aillies_core::{request::MAX_RESPONSE_BYTES, wire::TerminalScanner, Failure, FailureCode};
use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::Response;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::time::Instant;

pub(crate) const MAX_RETRIES: u8 = 3;
const TURN_BUDGET: Duration = Duration::from_secs(880);

// Account identities stay on the executing device, never on the peer link or disk.
pub(crate) struct AccountBindings<E = Executor> {
    entries: HashMap<(String, uuid::Uuid), (Instant, Arc<E>)>,
    generation: u64,
}
impl<E> Default for AccountBindings<E> {
    fn default() -> Self {
        Self { entries: HashMap::new(), generation: 0 }
    }
}
impl<E> AccountBindings<E> {
    pub(crate) fn invalidate(&mut self) {
        self.entries.clear();
        self.generation = self.generation.wrapping_add(1);
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
    pub(crate) fn executor(
        &mut self,
        client: &str,
        trace: uuid::Uuid,
        initial_send: bool,
        get: impl FnOnce() -> Result<Arc<E>, Failure>,
    ) -> Result<Arc<E>, Failure> {
        self.entries.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(900));
        let key = (client.to_owned(), trace);
        if let Some((_, executor)) = self.entries.get(&key) {
            return Ok(executor.clone());
        }
        // A restarted sharer cannot establish a different account for a retry.
        if !initial_send {
            return Err(Failure::new(FailureCode::AccountChanged));
        }
        if self.entries.len() >= 1024 {
            return Err(Failure::new(FailureCode::PeerBusy));
        }
        let executor = get()?;
        self.entries.insert(key, (Instant::now(), executor.clone()));
        Ok(executor)
    }
}

#[derive(Clone)]
enum RequestKind {
    Codex,
    Claude(aillies_core::claude::Request),
}
impl RequestKind {
    fn destination<'a>(&self, state: &'a State) -> &'a Destination {
        match self {
            Self::Codex => &state.destination,
            Self::Claude(_) => &state.claude.destination,
        }
    }
    fn provider(&self) -> aillies_core::protocol::Provider {
        match self {
            Self::Codex => aillies_core::protocol::Provider::Codex,
            Self::Claude(_) => aillies_core::protocol::Provider::Claude,
        }
    }
    fn scanner(&self) -> TerminalScanner {
        match self {
            Self::Codex => TerminalScanner::default(),
            Self::Claude(_) => TerminalScanner::claude(),
        }
    }
}

struct Turn {
    kind: RequestKind,
    local_claude: Option<Arc<aillies_claude::Executor>>,
    node: Arc<Node>,
    raw: Bytes,
    state: State,
    context: RequestContext,
    hint: Option<String>,
    local: Option<Arc<Executor>>,
    attempt: u8,
    deadline: Instant,
    may_have_run: bool,
    sent: bool,
}

pub(crate) async fn route(node: Arc<Node>, raw: Bytes, state: State, context: RequestContext, hint: Option<String>) -> Response<Body> {
    route_kind(node, raw, state, context, hint, RequestKind::Codex).await
}

pub(crate) async fn route_claude(node: Arc<Node>, raw: Bytes, state: State, context: RequestContext, request: aillies_core::claude::Request) -> Response<Body> {
    route_kind(node, raw, state, context, None, RequestKind::Claude(request)).await
}

async fn route_kind(node: Arc<Node>, raw: Bytes, state: State, context: RequestContext, hint: Option<String>, kind: RequestKind) -> Response<Body> {
    let mut turn = Turn {
        kind: kind.clone(),
        local_claude: None,
        node: node.clone(),
        raw,
        state,
        context,
        hint,
        local: None,
        attempt: 1,
        deadline: Instant::now() + if matches!(&kind, RequestKind::Claude(request) if request.desktop) { Duration::from_secs(240) } else { TURN_BUDGET },
        may_have_run: false,
        sent: false,
    };
    match turn.open().await {
        Ok(response) => {
            if let RequestKind::Claude(request) = &turn.kind {
                // Claude Code can replay an empty failed SSE stream even with its
                // retry setting disabled. Do not commit HTTP 200 before success.
                let streaming = request.streaming;
                match turn.collect(response).await {
                    Ok(bytes) if streaming => body::sse(body::full(bytes)),
                    Ok(bytes) => hyper::Response::builder().status(200).header("content-type", "application/json").body(body::full(bytes)).expect("response"),
                    Err(failure) => {
                        node.record_failure(&failure);
                        body::failure(&failure)
                    }
                }
            } else {
                body::sse(turn.stream(response))
            }
        }
        Err(failure) => {
            node.record_failure(&failure);
            body::failure(&failure)
        }
    }
}

fn retryable(code: FailureCode) -> bool {
    use FailureCode::*;
    matches!(
        code,
        PeerUnavailable
            | PeerBusy
            | ClaudeUnavailable
            | ClaudeRejected
            | UpstreamUnavailable
            | UpstreamRejected
            | UnexpectedUpstreamFormat
            | UnexpectedJsonResponse
            | StreamFailed
            | DeliveryUncertain
    )
}

impl Turn {
    fn stopped(&self, failure: Failure) -> Failure {
        if self.may_have_run && !failure.may_have_been_delivered() {
            Failure::new(FailureCode::DeliveryUncertain)
        } else {
            failure
        }
    }

    fn check_destination(&self) -> Result<(), Failure> {
        let current = self.node.snapshot();
        if !current.routing {
            return Err(self.stopped(Failure::new(FailureCode::RoutingPaused)));
        }
        if self.kind.destination(&current) != self.kind.destination(&self.state)
            || matches!(self.kind.destination(&self.state), Destination::Peer(fp) if current.peer(fp).is_none())
        {
            return Err(self.stopped(Failure::new(FailureCode::NoDestination)));
        }
        if matches!(self.kind, RequestKind::Claude(_))
            && self.state.claude.destination == Destination::Local
            && (current.claude.account_source != self.state.claude.account_source || current.claude.connected_account != self.state.claude.connected_account)
        {
            return Err(self.stopped(Failure::new(FailureCode::AccountChanged)));
        }
        Ok(())
    }

    async fn again(&mut self, failure: Failure) -> Result<(), Failure> {
        self.may_have_run |= failure.may_have_been_delivered();
        let refusal = matches!(failure.code, FailureCode::ClaudeRejected) && failure.upstream_status.is_some_and(|s| matches!(s, 400 | 401 | 403 | 404 | 422));
        if self.attempt > MAX_RETRIES || !retryable(failure.code) || refusal {
            return Err(self.stopped(failure));
        }
        self.check_destination()?;
        let delay = Duration::from_millis(500 * (1 << (self.attempt - 1)));
        if Instant::now() + delay >= self.deadline {
            return Err(Failure::new(FailureCode::DeliveryUncertain));
        }
        self.node.record_retry(self.context.with_attempt(self.attempt), &failure, self.attempt);
        tracing::warn!(code = ?failure.code, retry = self.attempt, "request retry scheduled");
        tokio::time::sleep(delay).await;
        self.attempt += 1;
        Ok(())
    }

    async fn open(&mut self) -> Result<Response<Body>, Failure> {
        loop {
            self.check_destination()?;
            if self.attempt > 1 {
                if let Destination::Peer(fp) = self.kind.destination(&self.state) {
                    let health =
                        tokio::time::timeout_at(self.deadline, self.node.peer_health(fp)).await.map_err(|_| Failure::new(FailureCode::DeliveryUncertain))?;
                    if let Some(code) = health.failure.filter(|code| !retryable(*code)) {
                        return Err(self.stopped(Failure::new(code)));
                    }
                    if health.status.is_some_and(|status| {
                        !status.providers.iter().any(|provider| provider.provider == self.kind.provider() && provider.supports_pinned_retries)
                    }) {
                        return Err(self.stopped(Failure::new(FailureCode::ProtocolMismatch)));
                    }
                }
            }
            match &self.kind {
                RequestKind::Codex => {
                    aillies_core::request::normalize(&self.raw).map_err(|_| Failure::new(FailureCode::RequestInvalid))?;
                    if self.state.destination == Destination::Local && self.local.is_none() {
                        self.local = Some(self.node.pinned_local_executor()?);
                    }
                }
                RequestKind::Claude(request) => {
                    aillies_core::claude::Request::parse(&self.raw, request.endpoint, request.headers.clone())?;
                    if self.state.claude.destination == Destination::Local && self.local_claude.is_none() {
                        self.local_claude = Some(
                            tokio::time::timeout_at(self.deadline, self.node.claude_executor(false, request.desktop))
                                .await
                                .map_err(|_| Failure::new(FailureCode::DeliveryUncertain))??,
                        );
                    }
                }
            }
            let mut context = self.context.with_attempt(self.attempt);
            context.initial_send = !self.sent;
            let response = tokio::time::timeout_at(self.deadline, async {
                match &self.kind {
                    RequestKind::Codex => {
                        crate::loopback::route_turn_inner(
                            self.node.clone(),
                            self.raw.clone(),
                            self.state.clone(),
                            context,
                            self.hint.clone(),
                            self.local.clone(),
                        )
                        .await
                    }
                    RequestKind::Claude(request) => {
                        crate::claude_route::route_inner(
                            self.node.clone(),
                            self.raw.clone(),
                            self.state.clone(),
                            context,
                            request.clone(),
                            self.local_claude.clone(),
                        )
                        .await
                    }
                }
            })
            .await
            .map_err(|_| Failure::new(FailureCode::DeliveryUncertain))?;
            if response.status() == 200 {
                self.sent = true;
                return Ok(response);
            }
            let failure = match http_body_util::Limited::new(response.into_body(), 4096).collect().await {
                Ok(body) => Failure::from_json(&body.to_bytes()).unwrap_or(Failure::new(FailureCode::DeliveryUncertain)),
                Err(_) => Failure::new(FailureCode::DeliveryUncertain),
            };
            self.sent |= !matches!(failure.code, FailureCode::PeerUnavailable | FailureCode::PeerBusy);
            self.again(failure).await?;
        }
    }

    async fn collect(mut self, mut response: Response<Body>) -> Result<Bytes, Failure> {
        loop {
            let result = tokio::time::timeout_at(self.deadline, collect_attempt(response, &self.kind))
                .await
                .map_err(|_| Failure::new(FailureCode::DeliveryUncertain))?;
            let forced_image = matches!(self.kind, RequestKind::Codex)
                && serde_json::from_slice::<serde_json::Value>(&self.raw).ok().is_some_and(|body| body["tool_choice"]["type"] == "image_generation");
            let result = result.and_then(|bytes| {
                if forced_image {
                    crate::image_mcp::validate_response(&bytes)?;
                }
                Ok(bytes)
            });
            match result {
                Ok(bytes) => {
                    self.node.clear_failure();
                    return Ok(bytes);
                }
                Err(failure) => {
                    self.again(failure).await?;
                    response = self.open().await?;
                }
            }
        }
    }

    fn stream(self, response: Response<Body>) -> Body {
        let node = self.node.clone();
        let trace = self.context.trace_id;
        let claude = matches!(self.kind, RequestKind::Claude(_));
        let future = Box::pin(self.collect(response));
        let heartbeat = tokio::time::interval(Duration::from_secs(15));
        body::stream(futures_util::stream::unfold((future, heartbeat, false), move |(mut future, mut heartbeat, done)| {
            let node = node.clone();
            async move {
                if done {
                    return None;
                }
                tokio::select! {
                    result = &mut future => {
                        let bytes = match result {
                            Ok(bytes) => bytes,
                            Err(failure) => {
                                node.record_failure(&failure);
                                let envelope: serde_json::Value = serde_json::from_slice(&failure.to_json()).unwrap_or_default();
                                let event = serde_json::json!({"type":"response.failed", "response":{
                                    "id":format!("aillies_{trace}"), "object":"response", "status":"failed", "error":envelope["error"]
                                }});
                                if claude {
                                    let event = serde_json::json!({"type":"error","error":{"type":"api_error","code":failure.code,"upstream_status":failure.upstream_status,"message":format!("Aillies: {}",failure.message())}});
                                    Bytes::from(format!("event: error\ndata: {event}\n\n"))
                                } else { Bytes::from(format!("event: response.failed\ndata: {event}\n\n")) }
                            }
                        };
                        Some((Ok(bytes), (future, heartbeat, true)))
                    }
                    _ = heartbeat.tick() => Some((Ok(Bytes::from_static(b": aillies waiting\n\n")), (future, heartbeat, false))),
                }
            }
        }))
    }
}

async fn collect_attempt(response: Response<Body>, kind: &RequestKind) -> Result<Bytes, Failure> {
    let mut body = response.into_body();
    let mut scanner = kind.scanner();
    let json_request = match kind {
        RequestKind::Claude(r) if !r.streaming => Some(r),
        _ => None,
    };
    let mut buffered = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| Failure::new(FailureCode::DeliveryUncertain))?;
        if let Ok(bytes) = frame.into_data() {
            if buffered.len().saturating_add(bytes.len()) > MAX_RESPONSE_BYTES {
                return Err(Failure::new(FailureCode::StreamFailed));
            }
            if json_request.is_none() {
                scanner.feed(&bytes);
            }
            buffered.extend_from_slice(&bytes);
            if scanner.unsuccessful {
                return Err(Failure::new(if scanner.claude_refused { FailureCode::ClaudeStreamRefused } else { FailureCode::StreamFailed }));
            }
            if scanner.completed {
                return Ok(Bytes::from(buffered));
            }
        }
    }
    if json_request.is_some_and(|r| r.valid_json(&buffered)) {
        return Ok(Bytes::from(buffered));
    }
    Err(Failure::new(FailureCode::DeliveryUncertain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_scope_never_rebinds_a_used_account_or_crosses_clients() {
        let first = Arc::new(Executor::with_upstream("missing".into(), Some("synthetic".into()), "http://127.0.0.1:1/".into()));
        let mut bindings = AccountBindings::default();
        let trace = uuid::Uuid::new_v4();
        let bound = bindings.executor("client", trace, true, || Ok(first.clone())).unwrap();
        let repeated = bindings.executor("client", trace, false, || panic!("must reuse the original executor")).unwrap();
        assert!(Arc::ptr_eq(&bound, &repeated));
        assert_eq!(bindings.executor("other", trace, false, || Ok(first.clone())).err().unwrap().code, FailureCode::AccountChanged);
        let generation = bindings.generation();
        bindings.invalidate();
        assert_ne!(bindings.generation(), generation);
        assert_eq!(bindings.executor("client", trace, false, || Ok(first.clone())).err().unwrap().code, FailureCode::AccountChanged);
        // No attempt reached the account; a later first send can establish a binding.
        assert!(bindings.executor("client", trace, true, || Ok(first)).is_ok());
    }

    #[test]
    fn retry_scope_cache_is_bounded_and_expires_without_evicting_live_bindings() {
        let executor = Arc::new(Executor::with_upstream("missing".into(), None, "http://127.0.0.1:1/".into()));
        let mut bindings = AccountBindings::default();
        for _ in 0..1024 {
            bindings.executor("client", uuid::Uuid::new_v4(), true, || Ok(executor.clone())).unwrap();
        }
        assert_eq!(bindings.executor("client", uuid::Uuid::new_v4(), true, || Ok(executor.clone())).err().unwrap().code, FailureCode::PeerBusy);
        for (at, _) in bindings.entries.values_mut() {
            *at -= Duration::from_secs(901);
        }
        assert!(bindings.executor("client", uuid::Uuid::new_v4(), true, || Ok(executor)).is_ok());
        assert_eq!(bindings.entries.len(), 1);
    }
}
