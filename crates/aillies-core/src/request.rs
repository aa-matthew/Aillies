//! Validation and normalization of Codex Responses requests.
//!
//! Ported from Peer Route 0.8.3 `TurnRequest.normalize`. The sharing device runs
//! this before using its account, so it is the authoritative gate.

use crate::request_compat as compat;
use serde_json::{json, Map, Value};

pub const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

/// Carry only a bounded session hint across the peer link; never arbitrary headers.
pub fn with_session_hint(raw: &[u8], hint: Option<&str>) -> Result<Vec<u8>, RequestError> {
    let Some(hint) = hint.map(str::trim).filter(|h| !h.is_empty() && h.len() <= 256 && h.bytes().all(|b| (32..=126).contains(&b))) else {
        return Ok(raw.to_vec());
    };
    if raw.len() > MAX_REQUEST_BYTES {
        return Err(RequestError("Request exceeds 16 MiB."));
    }
    let mut body: Map<String, Value> = serde_json::from_slice(raw).map_err(|_| RequestError("Invalid request."))?;
    if body.get("prompt_cache_key").and_then(Value::as_str).is_none_or(|s| s.trim().is_empty()) {
        body.insert("prompt_cache_key".into(), json!(hint));
    }
    serde_json::to_vec(&body).map_err(|_| RequestError("Invalid request."))
}

/// Text models that use the Responses Lite request shape (from 9router ce4460e).
pub const LITE_MODELS: &[&str] = &["gpt-6.1-sol", "gpt-6-sol", "gpt-6-luna"];

const ALLOWED_FIELDS: &[&str] = &[
    "model",
    "input",
    "instructions",
    "tools",
    "tool_choice",
    "reasoning",
    "text",
    "include",
    "parallel_tool_calls",
    "stream",
    "store",
    "service_tier",
    "prompt_cache_key",
    "client_metadata",
];
/// Tool types Codex sends today (verified against codex-cli 0.162.1). Hosted
/// tools run under the sharing account, which approved this device. Anything
/// else (for example remote `mcp`) is refused.
const ALLOWED_TOOLS: &[&str] = &["function", "custom", "namespace", "image_generation", "web_search", "tool_search", "local_shell"];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct RequestError(pub &'static str);

/// Closed categories only: never put request keys, tool names or content in diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestRejection {
    InvalidShape,
    TooLarge,
    EmptyInput,
    HistoryReference,
    InvalidTools,
    UnsupportedTool,
    TooManyTools,
    InvalidFunction,
    InvalidReasoning,
}

impl RequestError {
    pub fn reason(&self) -> RequestRejection {
        use RequestRejection::*;
        match self.0 {
            "Request exceeds 16 MiB." => TooLarge,
            "Input cannot be empty." | "Input must contain message items." => EmptyInput,
            "Include full history instead of item references." | "Server conversation references are unsupported." => HistoryReference,
            "Invalid tools." => InvalidTools,
            "Unsupported hosted tool." => UnsupportedTool,
            "Too many tools." => TooManyTools,
            "Invalid function name." => InvalidFunction,
            "Invalid reasoning." => InvalidReasoning,
            _ => InvalidShape,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Normalized {
    pub body: Vec<u8>,
    /// Send `x-openai-internal-codex-responses-lite: true` upstream.
    pub lite: bool,
    pub model: String,
    pub wants_image: bool,
}

fn has_image_tool(body: &Map<String, Value>) -> bool {
    has_tool(body, "image_generation")
}

fn has_tool(body: &Map<String, Value>, kind: &str) -> bool {
    let top = body.get("tools").and_then(Value::as_array).into_iter().flatten();
    let prefix = body
        .get("input")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("additional_tools"))
        .filter_map(|item| item.get("tools").and_then(Value::as_array))
        .flatten();
    top.chain(prefix).any(|tool| tool.get("type").and_then(Value::as_str) == Some(kind))
}

pub fn uses_lite(body: &Map<String, Value>) -> bool {
    let model = body.get("model").and_then(Value::as_str).unwrap_or("");
    LITE_MODELS.contains(&model) && !has_image_tool(body) && !has_tool(body, "web_search")
}

fn validate_tools(tools: &Value) -> Result<usize, RequestError> {
    let tools = tools.as_array().ok_or(RequestError("Invalid tools."))?;
    if tools.len() > 128 || tools.iter().any(|t| !t.get("type").and_then(Value::as_str).is_some_and(|k| ALLOWED_TOOLS.contains(&k))) {
        return Err(RequestError("Unsupported hosted tool."));
    }
    let mut count = tools.len();
    for tool in tools {
        if tool.get("type").and_then(Value::as_str) == Some("namespace") {
            count += validate_tools(tool.get("tools").ok_or(RequestError("Invalid tools."))?)?;
            if count > 128 {
                return Err(RequestError("Too many tools."));
            }
        }
    }
    Ok(count)
}

/// Older peers and saved history may already contain a Lite tools prefix.
/// Ordinary Responses expects those tools at the top level instead.
fn lift_tools_prefix(body: &mut Map<String, Value>) {
    let mut tools = body.get("tools").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut lifted = false;
    if let Some(Value::Array(input)) = body.get_mut("input") {
        input.retain(|item| {
            if item.get("type").and_then(Value::as_str) != Some("additional_tools") {
                return true;
            }
            lifted = true;
            if let Some(prefix_tools) = item.get("tools").and_then(Value::as_array) {
                for tool in prefix_tools {
                    if !tools.contains(tool) {
                        tools.push(tool.clone());
                    }
                }
            }
            false
        });
    }
    if lifted {
        body.insert("tools".into(), Value::Array(tools));
    }
}

pub fn normalize(data: &[u8]) -> Result<Normalized, RequestError> {
    const SHAPE: RequestError = RequestError("Send a model and the full turn history in input. Server conversation references are unsupported.");
    if data.len() > MAX_REQUEST_BYTES {
        return Err(RequestError("Request exceeds 16 MiB."));
    }
    let mut body: Map<String, Value> = match serde_json::from_slice(data) {
        Ok(Value::Object(m)) => m,
        _ => return Err(SHAPE),
    };
    compat::prepare(&mut body);
    let model = match body.get("model").and_then(Value::as_str) {
        Some(m) if !m.is_empty() && m.chars().count() <= 100 => m.to_string(),
        _ => return Err(SHAPE),
    };
    match body.get("input") {
        Some(Value::String(_)) | Some(Value::Array(_)) => {}
        _ => return Err(SHAPE),
    }
    for field in ["previous_response_id", "conversation"] {
        if body.get(field).is_some_and(|v| !v.is_null()) {
            return Err(RequestError("Server conversation references are unsupported."));
        }
        body.remove(field);
    }
    if let Some(tools) = body.get_mut("tools") {
        compat::tools(tools)?;
    }
    match body.get("tools") {
        None | Some(Value::Null) => 0,
        Some(tools) => validate_tools(tools)?,
    };
    // 9router filters client-only options instead of rejecting otherwise valid turns.
    // History references and tools were checked above and are never silently dropped.
    body.retain(|k, _| ALLOWED_FIELDS.contains(&k.as_str()));

    if let Some(Value::String(text)) = body.get("input") {
        if text.trim().is_empty() {
            return Err(RequestError("Input cannot be empty."));
        }
        let text = text.clone();
        body.insert("input".into(), json!([{ "type": "message", "role": "user", "content": [{ "type": "input_text", "text": text }] }]));
    }
    let preserve_lite_prefix = uses_lite(&body);
    let items = match body.get_mut("input") {
        Some(Value::Array(items)) if !items.is_empty() => items,
        _ => return Err(RequestError("Input must contain message items.")),
    };
    for item in items.iter_mut() {
        let Value::Object(obj) = item else { return Err(RequestError("Input must contain message items.")) };
        if obj.get("type").and_then(Value::as_str) == Some("item_reference") {
            return Err(RequestError("Include full history instead of item references."));
        }
        if obj.get("type").and_then(Value::as_str) == Some("additional_tools") {
            compat::tools(obj.get_mut("tools").ok_or(RequestError("Invalid tools."))?)?;
            validate_tools(obj.get("tools").ok_or(RequestError("Invalid tools."))?)?;
        }
        if obj.get("role").and_then(Value::as_str) == Some("system") {
            obj.insert("role".into(), json!("developer"));
        }
        let strip = obj.get("id").and_then(Value::as_str).is_some_and(|id| ["rs_", "fc_", "resp_", "msg_"].iter().any(|p| id.starts_with(p)));
        let keep_prefix_id = preserve_lite_prefix
            && obj.get("role").and_then(Value::as_str) == Some("developer")
            && obj.get("id").and_then(Value::as_str).is_some_and(|id| id.starts_with("msg_"));
        if strip && !keep_prefix_id {
            obj.remove("id");
        }
    }

    body.insert("stream".into(), json!(true));
    body.insert("store".into(), json!(false));
    body.entry("instructions").or_insert(json!(""));

    let lite = uses_lite(&body);
    if !lite {
        lift_tools_prefix(&mut body);
        // Do not duplicate instructions already carried by an older peer's prefix.
        let developer_prefix =
            body.get("input").and_then(Value::as_array).is_some_and(|items| items.iter().any(|i| i.get("role").and_then(Value::as_str) == Some("developer")));
        if !developer_prefix && body.get("instructions").and_then(Value::as_str).is_none_or(|s| s.trim().is_empty()) {
            body.insert("instructions".into(), json!(compat::DEFAULT_INSTRUCTIONS));
        }
    }
    if lite {
        let mut input = body.get("input").and_then(Value::as_array).cloned().unwrap_or_default();
        let has_prefix = input.iter().any(|i| i.get("type").and_then(Value::as_str) == Some("additional_tools"));
        if !has_prefix {
            let tools = body.get("tools").filter(|t| t.is_array()).cloned().unwrap_or(json!([]));
            let mut prefix = vec![json!({ "type": "additional_tools", "role": "developer", "tools": tools })];
            let instr = body.get("instructions").and_then(Value::as_str).filter(|s| !s.trim().is_empty()).unwrap_or(compat::DEFAULT_INSTRUCTIONS);
            prefix.push(json!({ "type": "message", "role": "developer", "content": [{ "type": "input_text", "text": instr }] }));
            input.splice(0..0, prefix);
        }
        body.insert("input".into(), Value::Array(input));
        body.insert("instructions".into(), json!(""));
        body.insert("tools".into(), Value::Null);
        body.insert("parallel_tool_calls".into(), json!(false));
        body.entry("tool_choice").or_insert(json!("auto"));
    }
    // Count the effective definitions after ordinary prefixes have been deduplicated.
    let mut effective = body.get("tools").and_then(Value::as_array).cloned().unwrap_or_default();
    if let Some(items) = body.get("input").and_then(Value::as_array) {
        for item in items.iter().filter(|i| i["type"] == "additional_tools") {
            if let Some(tools) = item.get("tools").and_then(Value::as_array) {
                for tool in tools {
                    if !effective.contains(tool) {
                        effective.push(tool.clone());
                    }
                }
            }
        }
    }
    validate_tools(&Value::Array(effective))?;
    compat::reasoning(&mut body, lite)?;
    let wants_image = has_image_tool(&body);
    let body = serde_json::to_vec(&Value::Object(body)).map_err(|_| SHAPE)?;
    Ok(Normalized { body, lite, model, wants_image })
}

/// Aillies' requester preference is authoritative, including when Codex itself
/// still has a saved Fast preference. Never silently downgrade a sent turn.
pub fn select_service_tier(raw: &[u8], fast: bool) -> Result<Vec<u8>, RequestError> {
    let normalized = normalize(raw)?;
    let mut body: Value = serde_json::from_slice(&normalized.body).map_err(|_| RequestError("Invalid request."))?;
    body["service_tier"] = json!(if fast { "priority" } else { "default" });
    serde_json::to_vec(&body).map_err(|_| RequestError("Invalid request."))
}

/// Enforce the account owner's permission even for manually crafted requests.
pub fn validate_shared_tier(raw: &[u8], allow_fast: bool) -> Result<(), crate::FailureCode> {
    let normalized = normalize(raw).map_err(|_| crate::FailureCode::RequestInvalid)?;
    let body: Value = serde_json::from_slice(&normalized.body).map_err(|_| crate::FailureCode::RequestInvalid)?;
    match body.get("service_tier") {
        None | Some(Value::Null) => Ok(()),
        Some(Value::String(tier)) if tier == "default" || tier == "auto" => Ok(()),
        Some(Value::String(_)) if !allow_fast => Err(crate::FailureCode::FastModeNotAllowed),
        Some(Value::String(tier)) if tier == "fast" || tier == "priority" => Ok(()),
        _ => Err(crate::FailureCode::RequestInvalid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(v: Value) -> Result<Value, RequestError> {
        normalize(&serde_json::to_vec(&v).unwrap()).map(|n| serde_json::from_slice(&n.body).unwrap())
    }

    #[test]
    fn string_input_becomes_typed_message_and_flags_forced() {
        let out = norm(json!({"model":"gpt-5.5","input":"hi","stream":false,"store":true})).unwrap();
        assert_eq!(out["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(out["stream"], true);
        assert_eq!(out["store"], false);
        assert_eq!(out["instructions"], compat::DEFAULT_INSTRUCTIONS);
    }

    #[test]
    fn rejects_server_state_and_unknown_fields() {
        assert!(norm(json!({"model":"m","input":"x","previous_response_id":"resp_1"})).is_err());
        assert!(norm(json!({"model":"m","input":"x","conversation":"c"})).is_err());
        assert!(norm(json!({"model":"m","input":"x","metadata":{}})).unwrap().get("metadata").is_none());
        assert!(norm(json!({"model":"m","input":[{"type":"item_reference","id":"x"}]})).is_err());
        assert!(norm(json!({"model":"","input":"x"})).is_err());
        assert!(norm(json!({"model":"m","input":"   "})).is_err());
        assert!(norm(json!({"model":"m","input":[]})).is_err());
        assert!(norm(json!({"model":"m","input":"x","tools":[{"type":"mcp","server_url":"https://x"}]})).is_err());
        assert!(norm(json!({"model":"m","input":"x","tools":[{"type":"web_search"},{"type":"tool_search"}]})).is_ok());
        assert!(normalize(b"not json").is_err());
    }

    #[test]
    fn strips_server_ids_and_maps_system_role() {
        let out = norm(json!({"model":"m","input":[
            {"type":"message","role":"system","content":[]},
            {"type":"reasoning","id":"rs_123","summary":[]},
            {"type":"function_call","id":"custom_9","name":"f","arguments":"{}","call_id":"c"}
        ]}))
        .unwrap();
        assert_eq!(out["input"][0]["role"], "developer");
        assert!(out["input"][1].get("id").is_none());
        assert_eq!(out["input"][2]["id"], "custom_9");
    }

    #[test]
    fn lite_models_get_prefix_shape() {
        let n = normalize(
            &serde_json::to_vec(&json!({"model":"gpt-6-luna","input":"hi","instructions":"be brief","tools":[{"type":"function","name":"f"}]})).unwrap(),
        )
        .unwrap();
        assert!(n.lite);
        let out: Value = serde_json::from_slice(&n.body).unwrap();
        assert_eq!(out["input"][0]["type"], "additional_tools");
        assert_eq!(out["input"][1]["content"][0]["text"], "be brief");
        assert_eq!(out["tools"], Value::Null);
        assert_eq!(out["reasoning"]["context"], "all_turns");
        assert_eq!(out["tool_choice"], "auto");
    }

    #[test]
    fn sol_61_text_uses_lite_across_requester_and_sharer_normalization() {
        let raw = serde_json::to_vec(&json!({
            "model":"gpt-6.1-sol", "input":"hi", "instructions":"be brief",
            "tools":[{"type":"function","name":"f","parameters":{"type":"object"}}],
            "reasoning":{"effort":"high","summary":"auto"}
        }))
        .unwrap();
        let selected = select_service_tier(&raw, false).unwrap();
        validate_shared_tier(&selected, false).unwrap();
        let outgoing = normalize(&selected).unwrap();
        assert!(outgoing.lite);
        let body: Value = serde_json::from_slice(&outgoing.body).unwrap();
        assert_eq!(body["model"], "gpt-6.1-sol");
        assert_eq!(body["input"][0]["tools"][0]["name"], "f");
        assert_eq!(body["input"][1]["content"][0]["text"], "be brief");
        assert_eq!(body["input"].as_array().unwrap().len(), 3);
        assert_eq!(body["tools"], Value::Null);
        assert_eq!(body["reasoning"]["summary"], "auto");
        assert_eq!(body["reasoning"]["context"], "all_turns");
        assert_eq!(body["service_tier"], "default");
        assert_eq!(normalize(&outgoing.body).unwrap().body, outgoing.body);
    }

    #[test]
    fn fast_preference_controls_service_tier_without_changing_image_tools() {
        let raw = br#"{"model":"gpt-5.5","input":"cat","service_tier":"fast","tools":[{"type":"image_generation"}]}"#;
        for (fast, expected) in [(true, "priority"), (false, "default")] {
            let out: Value = serde_json::from_slice(&select_service_tier(raw, fast).unwrap()).unwrap();
            assert_eq!(out["service_tier"], expected);
            assert_eq!(out["tools"][0]["type"], "image_generation");
        }
    }

    #[test]
    fn sharing_permission_cannot_be_bypassed_with_tier_aliases() {
        for tier in ["fast", "priority", "ultrafast", "scale", "flex"] {
            let raw = serde_json::to_vec(&json!({"model":"gpt-5.5","input":"hi","service_tier":tier})).unwrap();
            assert_eq!(validate_shared_tier(&raw, false), Err(crate::FailureCode::FastModeNotAllowed));
        }
        for tier in ["fast", "priority", "default", "auto"] {
            let raw = serde_json::to_vec(&json!({"model":"gpt-5.5","input":"hi","service_tier":tier})).unwrap();
            assert!(validate_shared_tier(&raw, true).is_ok());
        }
        assert_eq!(validate_shared_tier(br#"{"model":"m","input":"hi","service_tier":{}}"#, true), Err(crate::FailureCode::RequestInvalid));
    }

    #[test]
    fn image_requests_keep_ordinary_shape() {
        let n = normalize(&serde_json::to_vec(&json!({"model":"gpt-6-luna","input":"cat","tools":[{"type":"image_generation","output_format":"png"}],"tool_choice":{"type":"image_generation"}})).unwrap()).unwrap();
        assert!(!n.lite);
        assert!(n.wants_image);
    }

    #[test]
    fn ordinary_models_and_hosted_search_do_not_use_lite() {
        for model in ["gpt-6-astra", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"] {
            let raw = serde_json::to_vec(&json!({"model":model,"input":"hello"})).unwrap();
            assert!(!normalize(&raw).unwrap().lite, "{model}");
        }
        let raw = serde_json::to_vec(&json!({"model":"gpt-6.1-sol","input":"hello","tools":[{"type":"web_search"}]})).unwrap();
        let n = normalize(&raw).unwrap();
        assert!(!n.lite);
        let out: Value = serde_json::from_slice(&n.body).unwrap();
        assert_eq!(out["tools"][0]["type"], "web_search");
    }

    #[test]
    fn ordinary_requests_lift_lite_prefix_without_losing_history() {
        for model in ["gpt-6-astra", "gpt-6.1-sol"] {
            let out = norm(json!({"model":model,"input":[
                {"type":"additional_tools","role":"developer","tools":[{"type":"web_search"}]},
                {"type":"message","role":"developer","content":"instructions"},
                {"type":"reasoning","encrypted_content":"opaque","summary":[]},
                {"type":"message","role":"assistant","phase":"commentary","content":"checking"},
                {"type":"message","role":"user","content":"hello"}
            ],"tools":[{"type":"web_search"}]}))
            .unwrap();
            assert_eq!(out["tools"], json!([{"type":"web_search"}]));
            assert_eq!(out["input"].as_array().unwrap().len(), 4);
            assert_eq!(out["input"][0]["content"], "instructions");
            assert_eq!(out["input"][1]["encrypted_content"], "opaque");
            assert_eq!(out["input"][2]["phase"], "commentary");
            assert_eq!(norm(out.clone()).unwrap(), out);
        }
    }

    #[test]
    fn prefixed_tools_obey_the_same_allowlist() {
        for model in ["gpt-6-astra", "gpt-6.1-sol"] {
            assert!(norm(json!({"model":model,"input":[
                {"type":"additional_tools","role":"developer","tools":[{"type":"mcp","server_url":"https://example.com"}]},
                {"role":"user","content":"hello"}
            ]}))
            .is_err());
        }
    }

    #[test]
    fn router_aliases_effort_and_tools_are_normalized_before_lite() {
        let out = norm(json!({"model":"gpt-6-sol[1m]", "input":"hello", "reasoning":{"effort":"ultra"},
        "temperature":0.5, "max_output_tokens":100,
        "tools":[{"type":"function","function":{"name":"check","parameters":{"type":"object","properties":{
            "pattern":{"type":"string","pattern":"\\p{L}+"},"normal":{"type":"string","pattern":"[a-z]+"}
        }}}}]}))
        .unwrap();
        assert_eq!(out["model"], "gpt-6-sol");
        assert_eq!(out["reasoning"]["effort"], "max");
        assert!(out.get("temperature").is_none());
        assert!(out.get("max_output_tokens").is_none());
        let tool = &out["input"][0]["tools"][0];
        assert_eq!(tool["name"], "check");
        assert!(tool["parameters"]["properties"]["pattern"].get("pattern").is_none());
        assert_eq!(tool["parameters"]["properties"]["normal"]["pattern"], "[a-z]+");
        assert_eq!(norm(out.clone()).unwrap(), out);
    }

    #[test]
    fn router_defaults_preserve_explicit_instructions_and_lite_prefix_ids() {
        let out = norm(json!({"model":"gpt-6-astra","input":"hello"})).unwrap();
        assert!(out["instructions"].as_str().unwrap().starts_with("You are Codex"));
        assert_eq!(out["reasoning"], json!({"effort":"low","summary":"auto"}));
        let out = norm(json!({"model":"gpt-6.1-sol","input":[
            {"type":"additional_tools","role":"developer","tools":[]},
            {"type":"message","role":"developer","id":"msg_prefix","content":"keep"},
            {"role":"user","content":"hello"}
        ],"reasoning":{"effort":"minimal"}}))
        .unwrap();
        assert_eq!(out["input"][1]["id"], "msg_prefix");
        assert_eq!(out["reasoning"]["effort"], "low");
        assert_eq!(out["input"].as_array().unwrap().len(), 3);
        assert_eq!(norm(json!({"model":"gpt-6-astra","input":"hello","instructions":"my own"})).unwrap()["instructions"], "my own");
    }

    #[test]
    fn session_hint_is_bounded_and_preserves_explicit_cache_key() {
        let raw = br#"{"model":"gpt-6-astra","input":"hello"}"#;
        let out: Value = serde_json::from_slice(&with_session_hint(raw, Some(" session-1 ")).unwrap()).unwrap();
        assert_eq!(out["prompt_cache_key"], "session-1");
        assert_eq!(with_session_hint(raw, Some("bad\r\nheader")).unwrap(), raw);
        assert_eq!(with_session_hint(raw, Some(&"a".repeat(257))).unwrap(), raw);
        let out = with_session_hint(&serde_json::to_vec(&out).unwrap(), Some("other")).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&out).unwrap()["prompt_cache_key"], "session-1");
    }

    #[test]
    fn client_session_fields_and_optional_options_do_not_reject_review_requests() {
        let out = norm(json!({"model":"gpt-5.6-sol","input":[{"role":"user","content":"synthetic review"}],
            "session_id":"chat-1","conversation_id":"chat-1","future_client_option":true,
            "previous_response_id":null,"conversation":null}))
        .unwrap();
        for field in ["session_id", "conversation_id", "future_client_option", "previous_response_id", "conversation"] {
            assert!(out.get(field).is_none());
        }
        assert!(!normalize(&serde_json::to_vec(&out).unwrap()).unwrap().lite);
    }

    #[test]
    fn repeated_tool_prefixes_count_unique_effective_tools() {
        let tools: Vec<Value> = (0..70).map(|i| json!({"type":"function","name":format!("f{i}")})).collect();
        let out = norm(json!({"model":"gpt-5.6-sol", "tools":tools,"input":[
            {"type":"additional_tools","tools":tools},{"role":"user","content":"hi"}
        ]}))
        .unwrap();
        assert_eq!(out["tools"].as_array().unwrap().len(), 70);
        assert_eq!(norm(out.clone()).unwrap(), out);
    }
}
