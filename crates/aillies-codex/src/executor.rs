//! Execute a Codex Responses turn with this device's own login.
//!
//! No redirects, no retries, no proxy, no cookies. Failures are fixed codes.

use crate::auth::CodexOAuth;
use aillies_core::request::{normalize, Normalized, MAX_RESPONSE_BYTES};
use aillies_core::wire::{completed_json, detect, Format};
use aillies_core::{Failure, FailureCode};
use bytes::Bytes;
use futures_util::{stream, Stream, StreamExt};
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

pub const CODEX_UPSTREAM: &str = "https://chatgpt.com/backend-api/codex/responses";
pub const CLIENT_VERSION: &str = "0.159.0";

pub const CLIENT_ORIGINATOR: &str = "codex_cli_rs";

pub type TurnStream = Pin<Box<dyn Stream<Item = Result<Bytes, Failure>> + Send>>;

/// Final private-backend wire shape, after the node has enforced Fast permission.
/// Keep the explicit standard tier on the peer link for policy and diagnostics;
/// the Codex backend uses omission for standard and `priority` for Fast (9router
/// ce4460e). Unknown tiers fail closed rather than becoming a downgrade.
fn upstream_body(req: &Normalized) -> Result<Vec<u8>, Failure> {
    use serde_json::{json, Value};
    let invalid = || Failure::new(FailureCode::RequestInvalid);
    let mut body: serde_json::Map<String, Value> = serde_json::from_slice(&req.body).map_err(|_| invalid())?;
    match body.get("service_tier") {
        None | Some(Value::Null) => {
            body.remove("service_tier");
        }
        Some(Value::String(tier)) if tier == "default" || tier == "auto" => {
            body.remove("service_tier");
        }
        Some(Value::String(tier)) if tier == "priority" || tier == "fast" => {
            body.insert("service_tier".into(), json!("priority"));
        }
        _ => return Err(invalid()),
    }
    if !req.lite {
        body.remove("parallel_tool_calls");
    }
    if body.get("reasoning").and_then(|r| r.get("effort")).and_then(Value::as_str).is_some_and(|e| e != "none") {
        let include = body.entry("include").or_insert_with(|| json!([])).as_array_mut().ok_or_else(invalid)?;
        let encrypted = json!("reasoning.encrypted_content");
        if !include.contains(&encrypted) {
            include.push(encrypted);
        }
    }
    serde_json::to_vec(&body).map_err(|_| invalid())
}

/// Optional upstream override for local testing. The URL is parsed (not
/// prefix-matched) and accepted only for plain HTTP to a loopback host on an
/// explicit, non-default port, with no user info or fragment, so a stray
/// environment variable can never send tokens off the machine.
pub fn upstream_override(value: Option<String>) -> Option<String> {
    use reqwest::Url;
    let url = Url::parse(value?.trim()).ok()?;
    let clean = url.scheme() == "http" && url.username().is_empty() && url.password().is_none() && url.fragment().is_none() && url.port().is_some();
    let loopback = match url.host()? {
        url::Host::Ipv4(ip) => ip.is_loopback(),
        url::Host::Ipv6(ip) => ip.is_loopback(),
        url::Host::Domain(d) => d.eq_ignore_ascii_case("localhost"),
    };
    (clean && loopback).then(|| url.to_string())
}

pub struct Executor {
    auth_path: PathBuf,
    expected_account: Option<String>,
    upstream: String,
    client: reqwest::Client,
}

impl Executor {
    pub fn new(auth_path: PathBuf, expected_account: Option<String>) -> Self {
        let upstream = upstream_override(std::env::var("AILLIES_CODEX_UPSTREAM").ok()).unwrap_or_else(|| CODEX_UPSTREAM.to_string());
        Self::with_upstream(auth_path, expected_account, upstream)
    }

    pub fn with_upstream(auth_path: PathBuf, expected_account: Option<String>, upstream: String) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(900))
            .build()
            .expect("http client");
        Self { auth_path, expected_account, upstream, client }
    }

    pub fn auth_path(&self) -> &PathBuf {
        &self.auth_path
    }

    pub fn credentials(&self) -> Result<CodexOAuth, Failure> {
        let c = CodexOAuth::read(&self.auth_path)?;
        if let Some(expected) = &self.expected_account {
            if &c.account_id != expected {
                return Err(Failure::new(FailureCode::AccountChanged));
            }
        }
        Ok(c)
    }

    /// Account readiness without any inference.
    pub fn preflight(&self) -> Result<(), Failure> {
        self.credentials().map(|_| ())
    }

    pub async fn execute(&self, raw: &[u8]) -> Result<TurnStream, Failure> {
        self.execute_observed(raw, None, None).await
    }

    pub async fn execute_observed(&self, raw: &[u8], hint: Option<&str>, observer: Option<crate::telemetry::Observer>) -> Result<TurnStream, Failure> {
        let hint = crate::session::client_session(raw, hint);
        let req = normalize(raw).map_err(|_| Failure::new(FailureCode::RequestInvalid))?;
        let body = upstream_body(&req)?;
        let creds = self.credentials()?;
        let mut body: serde_json::Value = serde_json::from_slice(&body).map_err(|_| Failure::new(FailureCode::RequestInvalid))?;
        let session = crate::session::resolve(&body, &creds.account_id, hint.as_deref());
        if body.get("prompt_cache_key").and_then(serde_json::Value::as_str).is_none_or(|s| s.trim().is_empty()) {
            body["prompt_cache_key"] = serde_json::json!(session);
        }
        let body = serde_json::to_vec(&body).map_err(|_| Failure::new(FailureCode::RequestInvalid))?;
        let mut builder = self
            .client
            .post(&self.upstream)
            .header("Authorization", format!("Bearer {}", creds.access_token))
            .header("ChatGPT-Account-Id", &creds.account_id)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .header("originator", CLIENT_ORIGINATOR)
            .header("User-Agent", format!("{CLIENT_ORIGINATOR}/{CLIENT_VERSION}"))
            .header("version", CLIENT_VERSION)
            .header("x-client-request-id", uuid::Uuid::new_v4().to_string())
            .header("session_id", session)
            .body(body);
        if req.lite {
            builder = builder.header("x-openai-internal-codex-responses-lite", "true");
        }
        drop(creds);
        // A connect failure means nothing left this device. Any later error
        // means OpenAI may have received the request: say so, never retry.
        let mut observation = crate::telemetry::Observation::start(observer);
        let result = self.open_stream(builder, &mut observation).await;
        match result {
            Ok(stream) => Ok(crate::telemetry::observe(stream, observation)),
            Err(f) => {
                observation.finish(Some(f.code));
                Err(f)
            }
        }
    }

    async fn open_stream(&self, builder: reqwest::RequestBuilder, observation: &mut crate::telemetry::Observation) -> Result<TurnStream, Failure> {
        let resp =
            builder.send().await.map_err(|e| Failure::new(if e.is_connect() { FailureCode::UpstreamUnavailable } else { FailureCode::DeliveryUncertain }))?;
        let status = resp.status().as_u16();
        observation.headers(status);
        if status != 200 {
            return Err(Failure::upstream(status));
        }
        let mut body = resp.bytes_stream();
        let mut prefix: Vec<u8> = Vec::new();
        let mut format = Format::Pending;
        while prefix.len() < 4096 {
            match body.next().await {
                Some(Ok(chunk)) => {
                    observation.bytes();
                    prefix.extend_from_slice(&chunk);
                    format = detect(&prefix);
                    if format != Format::Pending {
                        break;
                    }
                }
                Some(Err(_)) => return Err(Failure::new(FailureCode::StreamFailed)),
                None => break,
            }
        }
        match format {
            Format::Sse => {
                let first = stream::once(async move { Ok(Bytes::from(prefix)) });
                let mut total = 0usize;
                let rest = body.map(move |item| match item {
                    Ok(chunk) => {
                        total += chunk.len();
                        if total > MAX_RESPONSE_BYTES {
                            Err(Failure::new(FailureCode::StreamFailed))
                        } else {
                            Ok(chunk)
                        }
                    }
                    Err(_) => Err(Failure::new(FailureCode::StreamFailed)),
                });
                Ok(Box::pin(first.chain(rest)))
            }
            Format::Json => {
                let mut data = prefix;
                while let Some(item) = body.next().await {
                    let chunk = item.map_err(|_| Failure::new(FailureCode::StreamFailed))?;
                    if data.len() + chunk.len() > MAX_RESPONSE_BYTES {
                        return Err(Failure { code: FailureCode::UnexpectedUpstreamFormat, upstream_status: Some(200) });
                    }
                    data.extend_from_slice(&chunk);
                }
                let events = completed_json(&data).ok_or(Failure { code: FailureCode::UnexpectedJsonResponse, upstream_status: Some(200) })?;
                Ok(Box::pin(stream::iter(events.into_iter().map(|e| Ok(Bytes::from(e))))))
            }
            _ => Err(Failure { code: FailureCode::UnexpectedUpstreamFormat, upstream_status: Some(200) }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::tests::auth_file;
    use http_body_util::{BodyExt, Full};
    use hyper::{body::Incoming, Request, Response};
    use hyper_util::rt::TokioIo;
    use std::sync::{Arc, Mutex};

    type Seen = Arc<Mutex<Vec<(String, String, bool, hyper::HeaderMap, serde_json::Value)>>>;

    #[test]
    fn request_bodies_match_pinned_9router_fixtures() {
        let cases: serde_json::Value = serde_json::from_str(include_str!("../tests/9router-fixtures.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let normalized = normalize(&serde_json::to_vec(&case["request"]).unwrap()).unwrap();
            let actual: serde_json::Value = serde_json::from_slice(&upstream_body(&normalized).unwrap()).unwrap();
            assert_eq!(actual, case["expected"], "fixture {}", case["name"]);
            let twice = normalize(&normalized.body).unwrap();
            assert_eq!(normalized.body, twice.body, "idempotent {}", case["name"]);
        }
    }

    /// Minimal upstream: records auth headers and replies with a fixed body.
    async fn mock(status: u16, content_type: &'static str, body: &'static str) -> (String, Seen) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen: Seen = Default::default();
        let s2 = seen.clone();
        tokio::spawn(async move {
            loop {
                let (sock, _) = listener.accept().await.unwrap();
                let seen = s2.clone();
                tokio::spawn(async move {
                    let svc = hyper::service::service_fn(move |req: Request<Incoming>| {
                        let seen = seen.clone();
                        async move {
                            let auth = req.headers().get("authorization").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
                            let acct = req.headers().get("chatgpt-account-id").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
                            let lite = req.headers().contains_key("x-openai-internal-codex-responses-lite");
                            let headers = req.headers().clone();
                            let request_body = req.into_body().collect().await.unwrap().to_bytes();
                            let request_body = serde_json::from_slice(&request_body).unwrap();
                            seen.lock().unwrap().push((auth, acct, lite, headers, request_body));
                            Ok::<_, std::convert::Infallible>(
                                Response::builder()
                                    .status(status)
                                    .header("content-type", content_type)
                                    .body(Full::new(Bytes::from_static(body.as_bytes())))
                                    .unwrap(),
                            )
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(sock), svc).await;
                });
            }
        });
        (format!("http://{addr}/responses"), seen)
    }

    fn auth(dir: &tempfile::TempDir, exp: f64) -> PathBuf {
        let p = dir.path().join("auth.json");
        std::fs::write(&p, auth_file(exp, "acct_1")).unwrap();
        p
    }

    async fn collect(s: TurnStream) -> Result<String, Failure> {
        let parts: Vec<_> = s.collect().await;
        let mut out = Vec::new();
        for p in parts {
            out.extend_from_slice(&p?);
        }
        Ok(String::from_utf8(out).unwrap())
    }

    const REQ: &[u8] = br#"{"model":"gpt-5.5","input":"hi"}"#;

    #[tokio::test]
    async fn streams_sse_even_with_wrong_mime() {
        let (url, seen) = mock(200, "application/json", "event: response.created\ndata: {}\n\nevent: response.completed\ndata: {}\n\n").await;
        let dir = tempfile::tempdir().unwrap();
        let ex = Executor::with_upstream(auth(&dir, 4e9), Some("acct_1".into()), url);
        let text = collect(ex.execute(REQ).await.unwrap()).await.unwrap();
        assert!(text.contains("response.completed"));
        let s = seen.lock().unwrap();
        assert!(s[0].0.starts_with("Bearer ") && s[0].1 == "acct_1" && !s[0].2);
    }

    #[tokio::test]
    async fn lite_header_for_lite_models() {
        let (url, seen) = mock(200, "text/event-stream", "data: {}\n\n").await;
        let dir = tempfile::tempdir().unwrap();
        let ex = Executor::with_upstream(auth(&dir, 4e9), None, url);
        collect(ex.execute(br#"{"model":"gpt-6.1-sol","input":"hi"}"#).await.unwrap()).await.unwrap();
        assert!(seen.lock().unwrap()[0].2);
    }

    #[tokio::test]
    async fn router_identity_and_session_match_cache_key_across_executors() {
        let (url, seen) = mock(200, "text/event-stream", "data: {}\n\n").await;
        let dir = tempfile::tempdir().unwrap();
        let path = auth(&dir, 4e9);
        for _ in 0..2 {
            let ex = Executor::with_upstream(path.clone(), None, url.clone());
            collect(ex.execute(br#"{"model":"gpt-6-astra","input":"hi","prompt_cache_key":"conversation-123"}"#).await.unwrap()).await.unwrap();
        }
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        for (_, _, _, headers, body) in seen.iter() {
            assert_eq!(headers["originator"], "codex_cli_rs");
            assert_eq!(headers["user-agent"], "codex_cli_rs/0.159.0");
            assert_eq!(headers["version"], "0.159.0");
            assert_eq!(headers["session_id"], "conversation-123");
            assert_eq!(body["prompt_cache_key"], "conversation-123");
        }
        assert_ne!(seen[0].3["x-client-request-id"], seen[1].3["x-client-request-id"]);
    }

    #[tokio::test]
    async fn router_session_fallback_is_stable_without_retaining_content() {
        let (url, seen) = mock(200, "text/event-stream", "data: {}\n\n").await;
        let dir = tempfile::tempdir().unwrap();
        let path = auth(&dir, 4e9);
        for _ in 0..2 {
            let ex = Executor::with_upstream(path.clone(), None, url.clone());
            collect(ex.execute(REQ).await.unwrap()).await.unwrap();
        }
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].3["session_id"], seen[1].3["session_id"]);
        assert_eq!(seen[0].4["prompt_cache_key"], seen[0].3["session_id"].to_str().unwrap());
        assert!(!seen[0].3["session_id"].to_str().unwrap().contains("acct_1"));
    }

    #[tokio::test]
    async fn ordinary_models_and_hosted_tools_omit_lite_header() {
        let (url, seen) = mock(200, "text/event-stream", "data: {}\n\n").await;
        let dir = tempfile::tempdir().unwrap();
        let ex = Executor::with_upstream(auth(&dir, 4e9), None, url);
        for request in [
            serde_json::json!({"model":"gpt-6-astra","input":"hi"}),
            serde_json::json!({"model":"gpt-5.6-sol","input":"hi"}),
            serde_json::json!({"model":"gpt-6.1-sol","input":"hi","tools":[{"type":"web_search"}]}),
            serde_json::json!({"model":"gpt-6.1-sol","input":"hi","tools":[{"type":"image_generation"}]}),
        ] {
            collect(ex.execute(&serde_json::to_vec(&request).unwrap()).await.unwrap()).await.unwrap();
        }
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 4);
        assert!(seen.iter().all(|entry| !entry.2));
    }

    #[test]
    fn upstream_tiers_never_silently_downgrade_and_lite_keeps_parallel_flag() {
        use serde_json::{json, Value};
        for tier in ["default", "auto", "fast", "priority", "ultrafast", "flex"] {
            let n = normalize(&serde_json::to_vec(&json!({"model":"gpt-6.1-sol","input":"hi","service_tier":tier})).unwrap()).unwrap();
            let wire = upstream_body(&n);
            if ["ultrafast", "flex"].contains(&tier) {
                assert!(wire.is_err());
                continue;
            }
            let body: Value = serde_json::from_slice(&wire.unwrap()).unwrap();
            assert_eq!(body["parallel_tool_calls"], false);
            assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));
            if ["fast", "priority"].contains(&tier) {
                assert_eq!(body["service_tier"], "priority");
            } else {
                assert!(body.get("service_tier").is_none());
            }
            assert_eq!(serde_json::from_slice::<Value>(&n.body).unwrap()["service_tier"], tier);
        }
    }

    #[tokio::test]
    async fn completed_json_is_converted() {
        let (url, _) =
            mock(200, "application/json", r#"{"object":"response","status":"completed","output":[{"type":"image_generation_call","result":"iVBOR"}]}"#).await;
        let dir = tempfile::tempdir().unwrap();
        let ex = Executor::with_upstream(auth(&dir, 4e9), None, url);
        let text = collect(ex.execute(REQ).await.unwrap()).await.unwrap();
        assert!(text.contains("image_generation_call") && text.contains("event: response.completed"));
    }

    #[tokio::test]
    async fn rejections_and_html_are_sanitized() {
        let dir = tempfile::tempdir().unwrap();
        let (url, _) = mock(429, "application/json", r#"{"error":{"message":"secret upstream detail"}}"#).await;
        let err = Executor::with_upstream(auth(&dir, 4e9), None, url).execute(REQ).await.err().unwrap();
        assert_eq!(err, Failure::upstream(429));
        assert!(!String::from_utf8(err.to_json()).unwrap().contains("secret upstream detail"));
        let (url, _) = mock(200, "text/html", "<html>login</html>").await;
        let err = Executor::with_upstream(auth(&dir, 4e9), None, url).execute(REQ).await.err().unwrap();
        assert_eq!(err.code, FailureCode::UnexpectedUpstreamFormat);
        let (url, _) = mock(200, "application/json", r#"{"error":"x"}"#).await;
        let err = Executor::with_upstream(auth(&dir, 4e9), None, url).execute(REQ).await.err().unwrap();
        assert_eq!(err.code, FailureCode::UnexpectedJsonResponse);
    }

    #[tokio::test]
    async fn account_problems_fail_before_network() {
        let dir = tempfile::tempdir().unwrap();
        let (url, seen) = mock(200, "text/event-stream", "data: {}\n\n").await;
        let err = Executor::with_upstream(auth(&dir, 1.0), None, url.clone()).execute(REQ).await.err().unwrap();
        assert_eq!(err.code, FailureCode::OauthExpired);
        let err = Executor::with_upstream(auth(&dir, 4e9), Some("someone_else".into()), url.clone()).execute(REQ).await.err().unwrap();
        assert_eq!(err.code, FailureCode::AccountChanged);
        let err = Executor::with_upstream(auth(&dir, 4e9), None, url).execute(br#"{"model":"m","input":"x","conversation":"c"}"#).await.err().unwrap();
        assert_eq!(err.code, FailureCode::RequestInvalid);
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn override_rejects_userinfo_and_lookalike_hosts() {
        // Regression: a prefix check accepted these although they target example.com.
        for bad in [
            "http://127.0.0.1:80@example.com/responses",
            "http://localhost:1@example.com/responses",
            "http://127.0.0.1:80:pw@example.com/x",
            "http://user@127.0.0.1:9/x",
            "http://127.0.0.1.example.com:9/x",
            "http://127.0.0.1/x",
            "https://127.0.0.1:9/x",
            "http://127.0.0.1:9/x#frag",
            "http://10.0.0.1:9/x",
            "not a url",
        ] {
            assert!(upstream_override(Some(bad.into())).is_none(), "{bad} must be rejected");
        }
        for good in ["http://127.0.0.1:19999/backend-api/codex/responses", "http://localhost:9/x", "http://[::1]:9/x"] {
            assert!(upstream_override(Some(good.into())).is_some(), "{good} should be accepted");
        }
    }

    #[test]
    fn override_only_loopback() {
        assert!(upstream_override(Some("http://127.0.0.1:9/x".into())).is_some());
        assert!(upstream_override(Some("https://evil.example/x".into())).is_none());
        assert!(upstream_override(Some("http://127.0.0.1.evil.example/x".into())).is_none());
    }
}
