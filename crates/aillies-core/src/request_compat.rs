//! Codex compatibility adapted from 9router ce4460e (MIT; see THIRD_PARTY_NOTICES.md).
use crate::request::{RequestError, LITE_MODELS};
use serde_json::{json, Map, Value};

pub const DEFAULT_INSTRUCTIONS: &str = include_str!("codex_default_instructions.txt");
const MODELS: &[&str] = &["gpt-6.1-sol", "gpt-6-astra", "gpt-6-sol", "gpt-6-luna", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5"];

pub fn prepare(body: &mut Map<String, Value>) {
    // These are API-client options the private Codex endpoint does not accept.
    // Server-side conversation references are deliberately NOT removed.
    for key in [
        "temperature",
        "top_p",
        "frequency_penalty",
        "presence_penalty",
        "logprobs",
        "top_logprobs",
        "n",
        "seed",
        "max_tokens",
        "max_completion_tokens",
        "max_output_tokens",
        "user",
        "prompt_cache_retention",
        "metadata",
        "stream_options",
        "safety_identifier",
    ] {
        body.remove(key);
    }
    let original = body.get("model").and_then(Value::as_str).unwrap_or_default().to_owned();
    let mut model = original.as_str();
    let mut suffix_effort = None;
    for effort in ["none", "minimal", "low", "medium", "high", "xhigh"] {
        if let Some(base) = model.strip_suffix(&format!("-{effort}")) {
            model = base;
            suffix_effort = Some(effort);
            break;
        }
    }
    let base = model.strip_suffix("[1m]").or_else(|| model.strip_suffix("-review")).unwrap_or(model);
    if MODELS.contains(&base) {
        body.insert("model".into(), json!(base));
    } else {
        suffix_effort = None; // Unknown model IDs are not guessed or substituted.
    }
    let effort = body.remove("reasoning_effort").or_else(|| suffix_effort.map(|e| json!(e)));
    if let Some(effort) = effort {
        if !body.contains_key("reasoning") || body.get("reasoning") == Some(&Value::Null) {
            body.insert("reasoning".into(), json!({"effort":effort}));
        }
    }
}

pub fn reasoning(body: &mut Map<String, Value>, lite: bool) -> Result<(), RequestError> {
    let model = body.get("model").and_then(Value::as_str).unwrap_or_default().to_owned();
    let value = body.entry("reasoning").or_insert_with(|| json!({"effort": if lite {"medium"} else {"low"}}));
    if value.is_null() {
        *value = json!({"effort": if lite {"medium"} else {"low"}});
    }
    let reasoning = value.as_object_mut().ok_or(RequestError("Invalid reasoning."))?;
    if let Some(effort) = reasoning.get("effort").and_then(Value::as_str) {
        let mapped = if LITE_MODELS.contains(&model.as_str()) && ["none", "minimal"].contains(&effort) {
            "low"
        } else if effort == "ultra" && ["gpt-5.6-sol", "gpt-5.6-terra"].contains(&model.as_str()) {
            "ultra"
        } else if effort == "ultra" && (model.starts_with("gpt-6") || model == "gpt-5.6-luna") {
            "max"
        } else if ["max", "ultra"].contains(&effort) && !model.starts_with("gpt-6") && !model.starts_with("gpt-5.6") {
            "xhigh"
        } else {
            effort
        };
        let mapped = mapped.to_owned();
        reasoning.insert("effort".into(), json!(mapped));
    }
    if lite {
        reasoning.insert("context".into(), json!("all_turns"));
    } else if reasoning.get("summary").is_none_or(|s| s.is_null() || s == "") {
        reasoning.insert("summary".into(), json!("auto"));
    }
    Ok(())
}

pub fn tools(value: &mut Value) -> Result<(), RequestError> {
    let Some(tools) = value.as_array_mut() else {
        return Ok(());
    };
    for tool in tools {
        if tool.get("type").and_then(Value::as_str) == Some("namespace") {
            let nested = tool.get_mut("tools").ok_or(RequestError("Invalid tools."))?;
            self::tools(nested)?;
            continue;
        }
        if tool.get("type").and_then(Value::as_str) != Some("function") {
            continue;
        }
        let nested = tool.get("function").and_then(Value::as_object);
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| nested.and_then(|f| f.get("name")).and_then(Value::as_str))
            .ok_or(RequestError("Invalid function name."))?
            .trim()
            .chars()
            .take(128)
            .collect::<String>();
        if name.is_empty() {
            return Err(RequestError("Invalid function name."));
        }
        let mut out = json!({"type":"function","name":name});
        for key in ["description", "parameters", "strict"] {
            if let Some(value) = tool.get(key).or_else(|| nested.and_then(|f| f.get(key))) {
                out[key] = value.clone();
            }
        }
        if out.get("parameters").is_none() {
            out["parameters"] = json!({"type":"object","properties":{}});
        }
        strip_patterns(&mut out["parameters"]);
        *tool = out;
    }
    Ok(())
}

fn unicode_escape(pattern: &str) -> bool {
    let mut slashes = 0;
    let bytes = pattern.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\\' {
            slashes += 1;
            continue;
        }
        if slashes % 2 == 1 && b"pP".contains(b) && bytes.get(i + 1) == Some(&b'{') {
            return true;
        }
        slashes = 0;
    }
    false
}

fn strip_patterns(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(strip_patterns),
        Value::Object(object) => {
            if object.get("pattern").and_then(Value::as_str).is_some_and(unicode_escape) {
                object.remove("pattern");
            }
            for (key, value) in object.iter_mut() {
                if key == "properties" {
                    if let Some(properties) = value.as_object_mut() {
                        properties.values_mut().for_each(strip_patterns);
                    }
                } else {
                    strip_patterns(value);
                }
            }
        }
        _ => {}
    }
}
