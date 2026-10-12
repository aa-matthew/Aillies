//! Native Anthropic request policy. Never translate signed thinking or tool history.

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_history_is_validated_without_rewriting() {
        let raw = br#"{"model":"claude-sonnet-4-6","max_tokens":1024,"stream":true,"system":[{"type":"text","text":"system","cache_control":{"type":"ephemeral","ttl":"1h"}}],"messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"thought","signature":"signed"},{"type":"tool_use","id":"tool1","name":"read","input":{"path":"a"}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool1","content":"result"}]}],"tools":[{"name":"read","input_schema":{"type":"object"}}]}"#;
        let request = Request::parse(raw, Endpoint::Messages, Headers::default()).unwrap();
        assert!(request.streaming);
        assert_eq!(request.endpoint, Endpoint::Messages);
    }
    #[test]
    fn refuses_other_protocols_and_bad_headers() {
        for raw in [
            br#"{"model":"gpt-6.1-sol","max_tokens":1,"messages":[]}"#.as_slice(),
            br#"{"model":"claude-sonnet-4-6","max_tokens":1,"input":"hello"}"#,
            br#"{"model":"claude-sonnet-4-6","max_tokens":1,"messages":[],"stream":"yes"}"#,
        ] {
            assert!(Request::parse(raw, Endpoint::Messages, Headers::default()).is_err());
        }
        assert!(Headers::new("2023-06-01", Some("beta\r\nx-api-key: stolen")).is_err());
        assert!(Headers::new("2023-06-01", Some("interleaved-thinking-2025-05-14,prompt-caching-2024-07-31")).is_ok());
    }
    #[test]
    fn model_discovery_is_bodyless_and_only_allows_bounded_catalog_queries() {
        let mut r = Request::parse(b"", Endpoint::Models, Headers::default()).unwrap();
        assert!(!r.streaming);
        assert_eq!(r.endpoint.method(), "GET");
        r.set_query(Some("limit=1000&after_id=claude-sonnet-4-6&lifecycle%5B%5D=active&beta=true")).unwrap();
        assert!(r.query.contains("after_id=claude-sonnet-4-6"));
        for q in [
            "limit=0",
            "limit=1001",
            "limit=2&limit=3",
            "after_id=a&before_id=b",
            "api_key=SECRET",
            "after_id=%0Aevil",
            "lifecycle=unknown",
            "beta=false",
            "after_id=",
            "limit=%ZZ",
        ] {
            assert!(r.set_query(Some(q)).is_err(), "{q}");
        }
        assert!(Request::parse(b"{}", Endpoint::Models, Headers::default()).is_err());
    }
    #[test]
    fn model_catalog_requires_native_envelope_and_valid_model_records() {
        let r = Request::parse(b"", Endpoint::Models, Headers::default()).unwrap();
        let valid = br#"{"data":[{"type":"model","id":"claude-sonnet-4-6","display_name":"Claude Sonnet","created_at":"2026-01-01T00:00:00Z","capabilities":{"thinking":{"supported":true}}}],"has_more":false,"first_id":"claude-sonnet-4-6","last_id":"claude-sonnet-4-6"}"#;
        assert!(r.valid_json(valid));
        assert!(r.valid_json(br#"{"data":[],"has_more":false,"first_id":null,"last_id":null}"#));
        for bad in
            [br#"{"error":"SECRET"}"#.as_slice(), br#"{"data":[],"has_more":"yes"}"#, br#"{"data":[{"type":"model","id":"gpt-other"}],"has_more":false}"#]
        {
            assert!(!r.valid_json(bad));
        }
    }
}

use crate::{request::MAX_REQUEST_BYTES, Failure, FailureCode};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint {
    Messages,
    CountTokens,
    Models,
}
impl Endpoint {
    pub fn path(self) -> &'static str {
        match self {
            Self::Messages => "/v1/messages",
            Self::CountTokens => "/v1/messages/count_tokens",
            Self::Models => "/v1/models",
        }
    }
    pub fn method(self) -> &'static str {
        if self == Self::Models {
            "GET"
        } else {
            "POST"
        }
    }
}

#[derive(Clone, Debug)]
pub struct Headers {
    pub version: String,
    pub beta: Option<String>,
    pub user_agent: Option<String>,
}
impl Default for Headers {
    fn default() -> Self {
        Self { version: "2023-06-01".into(), beta: None, user_agent: None }
    }
}
impl Headers {
    pub fn new(version: &str, beta: Option<&str>) -> Result<Self, Failure> {
        let invalid = || Failure::new(FailureCode::RequestInvalid);
        if version != "2023-06-01"
            || beta.is_some_and(|s| s.is_empty() || s.len() > 4096 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_,. ".contains(&b)))
        {
            return Err(invalid());
        }
        Ok(Self { version: version.into(), beta: beta.map(str::to_owned), user_agent: None })
    }
    pub fn client(&mut self, agent: Option<&str>) -> Result<(), Failure> {
        if agent.is_some_and(|s| s.is_empty() || s.len() > 256 || !s.bytes().all(|b| b.is_ascii_graphic() || b == b' ')) {
            return Err(Failure::new(FailureCode::RequestInvalid));
        }
        self.user_agent = agent.map(str::to_owned);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Request {
    pub desktop: bool,
    pub query: String,
    pub endpoint: Endpoint,
    pub headers: Headers,
    pub streaming: bool,
}
impl Request {
    pub fn parse(raw: &[u8], endpoint: Endpoint, headers: Headers) -> Result<Self, Failure> {
        let invalid = || Failure::new(FailureCode::RequestInvalid);
        if raw.len() > MAX_REQUEST_BYTES {
            return Err(invalid());
        }
        if endpoint == Endpoint::Models {
            if !raw.is_empty() {
                return Err(invalid());
            }
            return Ok(Self { desktop: false, query: String::new(), endpoint, headers, streaming: false });
        }
        let v: Value = serde_json::from_slice(raw).map_err(|_| invalid())?;
        let object = v.as_object().ok_or_else(invalid)?;
        const FIELDS: &[&str] = &[
            "model",
            "messages",
            "system",
            "max_tokens",
            "stream",
            "tools",
            "tool_choice",
            "metadata",
            "thinking",
            "temperature",
            "top_p",
            "top_k",
            "stop_sequences",
            "service_tier",
            "output_config",
            "container",
            "context_management",
            "mcp_servers",
            "inference_geo",
            "speed",
            "thread",
            "cache_control",
        ];
        if object.keys().any(|k| !FIELDS.contains(&k.as_str())) {
            return Err(invalid());
        }
        let model = v["model"].as_str().ok_or_else(invalid)?;
        if !model.starts_with("claude-") || model.len() > 128 || !model.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)) {
            return Err(invalid());
        }
        let messages = v["messages"].as_array().filter(|m| !m.is_empty()).ok_or_else(invalid)?;
        for message in messages {
            if !matches!(message["role"].as_str(), Some("user" | "assistant" | "system")) || !(message["content"].is_array() || message["content"].is_string())
            {
                return Err(invalid());
            }
        }
        crate::claude_compat::validate_extensions(&v)?;
        let streaming = match v.get("stream") {
            None => false,
            Some(v) => v.as_bool().ok_or_else(invalid)?,
        };
        if endpoint == Endpoint::Messages && v["max_tokens"].as_u64().is_none_or(|n| n == 0) {
            return Err(invalid());
        }
        if endpoint == Endpoint::CountTokens && streaming {
            return Err(invalid());
        }
        Ok(Self { desktop: false, query: String::new(), endpoint, headers, streaming })
    }
    pub fn set_query(&mut self, query: Option<&str>) -> Result<(), Failure> {
        let invalid = || Failure::new(FailureCode::RequestInvalid);
        if self.endpoint != Endpoint::Models {
            self.query = match query {
                None => String::new(),
                Some("beta=true") => "beta=true".into(),
                _ => return Err(invalid()),
            };
            return Ok(());
        }
        let query = query.unwrap_or("");
        if query.len() > 2048 {
            return Err(invalid());
        }
        let mut seen = std::collections::HashSet::new();
        let mut lifecycle = std::collections::HashSet::new();
        let mut result = form_urlencoded::Serializer::new(String::new());
        for (key, value) in form_urlencoded::parse(query.as_bytes()) {
            if key == "lifecycle" || key == "lifecycle[]" {
                if !matches!(value.as_ref(), "active" | "deprecated" | "retired") || !lifecycle.insert(value.to_string()) {
                    return Err(invalid());
                }
            } else {
                if !seen.insert(key.to_string()) {
                    return Err(invalid());
                }
                match key.as_ref() {
                    "limit" if value.parse::<u16>().is_ok_and(|n| (1..=1000).contains(&n)) => {}
                    "after_id" | "before_id" if model_id(&value) => {}
                    "beta" if value == "true" => {}
                    _ => return Err(invalid()),
                }
            }
            result.append_pair(&key, &value);
        }
        if seen.contains("after_id") && seen.contains("before_id") {
            return Err(invalid());
        }
        self.query = result.finish();
        Ok(())
    }
    pub fn valid_json(&self, bytes: &[u8]) -> bool {
        let Ok(v) = serde_json::from_slice::<Value>(bytes) else { return false };
        match self.endpoint {
            Endpoint::CountTokens => v["input_tokens"].as_u64().is_some(),
            Endpoint::Messages => v["type"] == "message" && v["content"].is_array() && v["stop_reason"].is_string(),
            Endpoint::Models => {
                let Some(data) = v["data"].as_array().filter(|d| d.len() <= 1000) else { return false };
                let mut ids = std::collections::HashSet::new();
                v["has_more"].is_boolean()
                    && (v["has_more"] != true || !data.is_empty())
                    && ["first_id", "last_id"].iter().all(|k| v[*k].is_null() || v[*k].as_str().is_some_and(model_id))
                    && data.iter().all(|m| {
                        m["type"] == "model"
                            && m["id"].as_str().is_some_and(|id| model_id(id) && ids.insert(id))
                            && m["display_name"].as_str().is_some_and(|s| !s.is_empty() && s.len() <= 512 && !s.chars().any(char::is_control))
                            && m["created_at"].as_str().is_some_and(|s| !s.is_empty() && s.len() <= 64 && s.is_ascii())
                    })
            }
        }
    }
}
fn model_id(s: &str) -> bool {
    s.starts_with("claude-") && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

/// Fixed, Anthropic-shaped failure; never copies upstream text.
pub fn failure_json(failure: &Failure) -> Value {
    let kind = if failure.code == FailureCode::ClaudeOptionsUnsupported || failure.code == FailureCode::RequestInvalid {
        "invalid_request_error"
    } else if failure.code == FailureCode::ClaudeStreamRefused {
        "permission_error"
    } else {
        match failure.upstream_status {
            Some(400 | 422) => "invalid_request_error",
            Some(401) => "authentication_error",
            Some(403) => "permission_error",
            Some(404) => "not_found_error",
            Some(429) => "rate_limit_error",
            _ => "api_error",
        }
    };
    serde_json::json!({"type":"error","error":{"type":kind,"code":failure.code,"upstream_status":failure.upstream_status,"message":format!("Aillies: {}",failure.message())}})
}
