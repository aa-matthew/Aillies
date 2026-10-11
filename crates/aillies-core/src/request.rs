//! Validation and normalization of Codex Responses requests.
//!
//! Ported from Peer Route 0.8.3 `TurnRequest.normalize`. The sharing device runs
//! this before using its account, so it is the authoritative gate.

use serde_json::{json, Map, Value};

pub const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

/// Text models that use the Responses Lite request shape (from 9router ce4460e).
pub const LITE_MODELS: &[&str] = &["gpt-6-sol", "gpt-6-luna", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"];

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
const ALLOWED_TOOLS: &[&str] = &["function", "custom", "image_generation", "web_search", "tool_search", "local_shell"];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct RequestError(pub &'static str);

#[derive(Debug, Clone)]
pub struct Normalized {
    pub body: Vec<u8>,
    /// Send `x-openai-internal-codex-responses-lite: true` upstream.
    pub lite: bool,
    pub model: String,
    pub wants_image: bool,
}

fn has_image_tool(body: &Map<String, Value>) -> bool {
    body.get("tools").and_then(Value::as_array).map(|t| t.iter().any(|x| x.get("type").and_then(Value::as_str) == Some("image_generation"))).unwrap_or(false)
}

pub fn uses_lite(body: &Map<String, Value>) -> bool {
    let model = body.get("model").and_then(Value::as_str).unwrap_or("");
    LITE_MODELS.contains(&model) && !has_image_tool(body)
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
    let model = match body.get("model").and_then(Value::as_str) {
        Some(m) if !m.is_empty() && m.chars().count() <= 100 => m.to_string(),
        _ => return Err(SHAPE),
    };
    match body.get("input") {
        Some(Value::String(_)) | Some(Value::Array(_)) => {}
        _ => return Err(SHAPE),
    }
    if body.contains_key("previous_response_id") || body.contains_key("conversation") {
        return Err(SHAPE);
    }
    match body.get("tools") {
        None | Some(Value::Null) => {}
        Some(Value::Array(tools)) => {
            let ok = tools.len() <= 128 && tools.iter().all(|t| t.get("type").and_then(Value::as_str).is_some_and(|k| ALLOWED_TOOLS.contains(&k)));
            if !ok {
                return Err(RequestError(
                    "Unsupported hosted tool. Aillies supports the tools Codex sends: functions, image generation, web search and tool search.",
                ));
            }
        }
        Some(_) => return Err(RequestError("Invalid tools.")),
    }
    if body.keys().any(|k| !ALLOWED_FIELDS.contains(&k.as_str())) {
        return Err(RequestError("Unsupported request field."));
    }

    if let Some(Value::String(text)) = body.get("input") {
        if text.trim().is_empty() {
            return Err(RequestError("Input cannot be empty."));
        }
        let text = text.clone();
        body.insert("input".into(), json!([{ "type": "message", "role": "user", "content": [{ "type": "input_text", "text": text }] }]));
    }
    let items = match body.get_mut("input") {
        Some(Value::Array(items)) if !items.is_empty() => items,
        _ => return Err(RequestError("Input must contain message items.")),
    };
    for item in items.iter_mut() {
        let Value::Object(obj) = item else { return Err(RequestError("Input must contain message items.")) };
        if obj.get("type").and_then(Value::as_str) == Some("item_reference") {
            return Err(RequestError("Include full history instead of item references."));
        }
        if obj.get("role").and_then(Value::as_str) == Some("system") {
            obj.insert("role".into(), json!("developer"));
        }
        let strip = obj.get("id").and_then(Value::as_str).is_some_and(|id| ["rs_", "fc_", "resp_", "msg_"].iter().any(|p| id.starts_with(p)));
        if strip {
            obj.remove("id");
        }
    }

    body.insert("stream".into(), json!(true));
    body.insert("store".into(), json!(false));
    body.entry("instructions").or_insert(json!(""));

    let lite = uses_lite(&body);
    if lite {
        let mut input = body.get("input").and_then(Value::as_array).cloned().unwrap_or_default();
        let has_prefix = input.iter().any(|i| i.get("type").and_then(Value::as_str) == Some("additional_tools"));
        if !has_prefix {
            let tools = body.get("tools").filter(|t| t.is_array()).cloned().unwrap_or(json!([]));
            let mut prefix = vec![json!({ "type": "additional_tools", "role": "developer", "tools": tools })];
            if let Some(instr) = body.get("instructions").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                prefix.push(json!({ "type": "message", "role": "developer", "content": [{ "type": "input_text", "text": instr }] }));
            }
            input.splice(0..0, prefix);
        }
        body.insert("input".into(), Value::Array(input));
        body.insert("instructions".into(), json!(""));
        body.insert("tools".into(), Value::Null);
        body.insert("parallel_tool_calls".into(), json!(false));
        body.entry("tool_choice").or_insert(json!("auto"));
        let mut reasoning = body.get("reasoning").and_then(Value::as_object).cloned().unwrap_or_else(|| {
            let mut m = Map::new();
            m.insert("effort".into(), json!("medium"));
            m
        });
        reasoning.remove("summary");
        reasoning.insert("context".into(), json!("all_turns"));
        body.insert("reasoning".into(), Value::Object(reasoning));
    }
    let wants_image = has_image_tool(&body);
    let body = serde_json::to_vec(&Value::Object(body)).map_err(|_| SHAPE)?;
    Ok(Normalized { body, lite, model, wants_image })
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
        assert_eq!(out["instructions"], "");
    }

    #[test]
    fn rejects_server_state_and_unknown_fields() {
        assert!(norm(json!({"model":"m","input":"x","previous_response_id":"resp_1"})).is_err());
        assert!(norm(json!({"model":"m","input":"x","conversation":"c"})).is_err());
        assert!(norm(json!({"model":"m","input":"x","metadata":{}})).is_err());
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
            &serde_json::to_vec(&json!({"model":"gpt-5.6-luna","input":"hi","instructions":"be brief","tools":[{"type":"function","name":"f"}]})).unwrap(),
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
    fn image_requests_keep_ordinary_shape() {
        let n = normalize(&serde_json::to_vec(&json!({"model":"gpt-5.6-luna","input":"cat","tools":[{"type":"image_generation","output_format":"png"}],"tool_choice":{"type":"image_generation"}})).unwrap()).unwrap();
        assert!(!n.lite);
        assert!(n.wants_image);
    }
}
