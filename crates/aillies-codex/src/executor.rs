//! Execute a Codex Responses turn with this device's own login.
//!
//! No redirects, no retries, no proxy, no cookies. Failures are fixed codes.

use crate::auth::CodexOAuth;
use aillies_core::request::{normalize, MAX_RESPONSE_BYTES};
use aillies_core::wire::{completed_json, detect, Format};
use aillies_core::{Failure, FailureCode};
use bytes::Bytes;
use futures_util::{stream, Stream, StreamExt};
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

pub const CODEX_UPSTREAM: &str = "https://chatgpt.com/backend-api/codex/responses";
const CLIENT_VERSION: &str = "0.159.0";

pub type TurnStream = Pin<Box<dyn Stream<Item = Result<Bytes, Failure>> + Send>>;

/// Optional upstream override for local testing. Only loopback HTTP targets
/// are accepted so a stray environment variable can never send tokens elsewhere.
pub fn upstream_override(value: Option<String>) -> Option<String> {
    let v = value?;
    let ok = v.starts_with("http://127.0.0.1:") || v.starts_with("http://localhost:");
    ok.then_some(v)
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
        let req = normalize(raw).map_err(|_| Failure::new(FailureCode::RequestInvalid))?;
        let creds = self.credentials()?;
        let mut builder = self
            .client
            .post(&self.upstream)
            .header("Authorization", format!("Bearer {}", creds.access_token))
            .header("ChatGPT-Account-Id", &creds.account_id)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .header("originator", "codex_cli_rs")
            .header("User-Agent", format!("codex_cli_rs/{CLIENT_VERSION}"))
            .header("version", CLIENT_VERSION)
            .header("x-client-request-id", uuid::Uuid::new_v4().to_string())
            .header("session_id", uuid::Uuid::new_v4().to_string())
            .body(req.body);
        if req.lite {
            builder = builder.header("x-openai-internal-codex-responses-lite", "true");
        }
        drop(creds);
        // A connect failure means nothing left this device. Any later error
        // means OpenAI may have received the request: say so, never retry.
        let resp =
            builder.send().await.map_err(|e| Failure::new(if e.is_connect() { FailureCode::UpstreamUnavailable } else { FailureCode::DeliveryUncertain }))?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(Failure::upstream(status));
        }
        let mut body = resp.bytes_stream();
        let mut prefix: Vec<u8> = Vec::new();
        let mut format = Format::Pending;
        while prefix.len() < 4096 {
            match body.next().await {
                Some(Ok(chunk)) => {
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

    type Seen = Arc<Mutex<Vec<(String, String, bool)>>>;

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
                            let _ = req.into_body().collect().await;
                            seen.lock().unwrap().push((auth, acct, lite));
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
        collect(ex.execute(br#"{"model":"gpt-5.6-luna","input":"hi"}"#).await.unwrap()).await.unwrap();
        assert!(seen.lock().unwrap()[0].2);
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
    fn override_only_loopback() {
        assert!(upstream_override(Some("http://127.0.0.1:9/x".into())).is_some());
        assert!(upstream_override(Some("https://evil.example/x".into())).is_none());
        assert!(upstream_override(Some("http://127.0.0.1.evil.example/x".into())).is_none());
    }
}
