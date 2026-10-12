//! One attempt against Anthropic using the selected owner's native Claude login.
use crate::auth::Reader;
use aillies_core::{
    claude::Request,
    request::MAX_RESPONSE_BYTES,
    telemetry::{self, Observation, Observer, TurnStream},
    wire::TerminalScanner,
    Failure, FailureCode,
};
use bytes::Bytes;
use futures_util::{stream, StreamExt};
use std::time::Duration;

pub const UPSTREAM: &str = "https://api.anthropic.com";

pub struct Executor {
    auth: Reader,
    expected_account: String,
    expected_token: String,
    upstream: String,
    client: reqwest::Client,
}
impl Executor {
    pub fn new(auth: Reader, expected_account: String, expected_token: String, test_upstream: Option<String>) -> Self {
        let upstream = test_upstream
            .and_then(|s| {
                let u = reqwest::Url::parse(&s).ok()?;
                let local =
                    u.host_str().is_some_and(|h| h == "localhost" || h.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()));
                (local
                    && u.scheme() == "http"
                    && u.port().is_some()
                    && u.username().is_empty()
                    && u.password().is_none()
                    && u.fragment().is_none()
                    && u.query().is_none()
                    && u.path() == "/")
                    .then(|| u.to_string())
            })
            .unwrap_or_else(|| UPSTREAM.into());
        Self::with_origin(auth, expected_account, expected_token, upstream)
    }
    fn with_origin(auth: Reader, expected_account: String, expected_token: String, upstream: String) -> Self {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(900))
            .build()
            .expect("http client");
        Self { auth, expected_account, expected_token, upstream, client }
    }
    pub async fn execute(&self, raw: &[u8], request: &Request, observer: Option<Observer>) -> Result<TurnStream, Failure> {
        let mut guard = Observation::start(observer);
        let result = self.open(raw, request, &mut guard).await;
        match result {
            Ok(stream) if request.streaming => Ok(telemetry::observe_with(stream, guard, TerminalScanner::claude())),
            Ok(stream) => {
                guard.finish(None);
                Ok(stream)
            }
            Err(f) => {
                guard.finish(Some(f.code));
                Err(f)
            }
        }
    }
    async fn open(&self, raw: &[u8], request: &Request, guard: &mut Observation) -> Result<TurnStream, Failure> {
        let mut checked = Request::parse(raw, request.endpoint, request.headers.clone())?;
        checked.set_query((!request.query.is_empty()).then_some(&request.query))?;
        let prepared = checked.prepare(raw)?;
        let reader = self.auth.clone();
        let auth = tokio::task::spawn_blocking(move || reader.read()).await.map_err(|_| Failure::new(FailureCode::ClaudeLoginUnavailable))??;
        if auth.account != self.expected_account || auth.token_digest() != self.expected_token {
            return Err(Failure::new(FailureCode::AccountChanged));
        }
        let query = if checked.query.is_empty() {
            "beta=true".into()
        } else if checked.query.split('&').any(|p| p == "beta=true") {
            checked.query
        } else {
            format!("{}&beta=true", checked.query)
        };
        let url = format!("{}{}?{}", self.upstream.trim_end_matches('/'), request.endpoint.path(), query);
        let mut call = self
            .client
            .request(if request.endpoint.method() == "GET" { reqwest::Method::GET } else { reqwest::Method::POST }, url)
            .bearer_auth(auth.access_token)
            .header("x-app", "cli")
            .header("x-stainless-retry-count", "0")
            .header("anthropic-version", &request.headers.version);
        if request.endpoint.method() == "POST" {
            call = call.header("content-type", "application/json").body(prepared.body.into_owned());
        }
        let mut betas: Vec<&str> = prepared.headers.beta.as_deref().unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        for required in ["oauth-2025-04-20", "claude-code-20250219"] {
            if !betas.contains(&required) {
                betas.push(required);
            }
        }
        call = call.header("anthropic-beta", betas.join(","));
        if let Some(agent) = &request.headers.user_agent {
            call = call.header("user-agent", agent);
        }
        let response =
            call.send().await.map_err(|e| Failure::new(if e.is_connect() { FailureCode::ClaudeUnavailable } else { FailureCode::DeliveryUncertain }))?;
        let status = response.status().as_u16();
        guard.headers(status);
        if status != 200 {
            return Err(Failure { code: FailureCode::ClaudeRejected, upstream_status: Some(status) });
        }
        if !request.streaming {
            let mut body = response.bytes_stream();
            let mut bytes = Vec::new();
            while let Some(chunk) = body.next().await {
                let chunk = chunk.map_err(|_| Failure::new(FailureCode::DeliveryUncertain))?;
                guard.bytes();
                if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                    return Err(Failure::new(FailureCode::StreamFailed));
                }
                bytes.extend_from_slice(&chunk);
            }
            if !request.valid_json(&bytes) {
                return Err(Failure::new(FailureCode::UnexpectedJsonResponse));
            }
            return Ok(Box::pin(stream::once(async move { Ok(Bytes::from(bytes)) })));
        }
        let mut total = 0usize;
        let mut scanner = TerminalScanner::claude();
        Ok(Box::pin(response.bytes_stream().map(move |item| {
            let bytes = item.map_err(|_| Failure::new(FailureCode::DeliveryUncertain))?;
            total = total.saturating_add(bytes.len());
            if total > MAX_RESPONSE_BYTES {
                return Err(Failure::new(FailureCode::StreamFailed));
            }
            scanner.feed(&bytes);
            if scanner.unsuccessful {
                let failure = Failure::new(if scanner.claude_refused { FailureCode::ClaudeStreamRefused } else { FailureCode::StreamFailed });
                let event = aillies_core::claude::failure_json(&failure);
                return Ok(Bytes::from(format!("event: error\ndata: {event}\n\n")));
            }
            Ok(bytes)
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn unused() -> Reader {
        Reader {
            connected: None,
            desktop: None,
            home: "unused".into(),
            storage: "unused".into(),
            receipts: "unused".into(),
            metadata: "unused".into(),
            keychain_service: None,
        }
    }
    #[test]
    fn override_is_loopback_only_and_never_changes_the_production_origin() {
        for bad in [
            "https://evil.example",
            "http://127.0.0.1.evil.example:9",
            "http://user@127.0.0.1:9",
            "http://127.0.0.1",
            "http://10.0.0.1:9",
            "http://127.0.0.1:9/#secret",
            "http://127.0.0.1:9/?token=secret",
            "http://127.0.0.1:9/path",
        ] {
            assert_eq!(Executor::new(unused(), "unused".into(), "unused".into(), Some(bad.into())).upstream, UPSTREAM);
        }
        assert_eq!(Executor::new(unused(), "unused".into(), "unused".into(), Some("http://127.0.0.1:9".into())).upstream, "http://127.0.0.1:9/");
    }
}
