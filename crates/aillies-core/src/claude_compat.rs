//! Reviewed native Claude request compatibility. Signed history is opaque RawValue.
//! Rules follow current Anthropic docs; OmniRoute 7f3cbb0 informed adapter separation.
use crate::{
    claude::{Endpoint, Headers, Request},
    Failure, FailureCode,
};
use serde_json::{value::RawValue, Map, Value};
use std::{borrow::Cow, collections::BTreeMap};

pub struct Prepared<'a> {
    pub body: Cow<'a, [u8]>,
    pub headers: Headers,
}
#[derive(Clone, Copy)]
enum Effort {
    Five,
    NoXhigh,
    Three,
    None,
}
#[derive(Clone, Copy)]
struct Capabilities {
    adaptive_only: bool,
    always_thinking: bool,
    force_forbidden: bool,
    between_tools: bool,
    disabled_effort_cap: bool,
    per_message: bool,
    mid_system: bool,
    default_effort: &'static str,
    fast: bool,
    effort: Effort,
}
fn capabilities(model: &str) -> Option<Capabilities> {
    // Only recognized native snapshots inherit a reviewed rule; future models are not guessed.
    let model = model.rsplit_once('-').filter(|(_, date)| date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit())).map_or(model, |(base, _)| base);
    let mut c = Capabilities {
        adaptive_only: true,
        always_thinking: false,
        force_forbidden: false,
        between_tools: false,
        disabled_effort_cap: false,
        per_message: false,
        mid_system: true,
        default_effort: "high",
        fast: false,
        effort: Effort::Five,
    };
    match model {
        "claude-opus-5-5" => {
            c.force_forbidden = true;
            c.default_effort = "medium";
            c.always_thinking = true;
            c.per_message = true;
            c.fast = true;
        }
        "claude-fable-5-1" | "claude-mythos-5-1" => {
            c.force_forbidden = true;
            c.always_thinking = true;
            c.per_message = true;
        }
        "claude-fable-5" | "claude-mythos-5" => c.always_thinking = true,
        "claude-opus-5" => {
            c.disabled_effort_cap = true;
            c.per_message = true;
            c.fast = true;
        }
        "claude-opus-4-8" => c.fast = true,
        "claude-opus-4-7" | "claude-sonnet-5" => c.mid_system = false,
        "claude-sonnet-5-5" => {
            c.force_forbidden = true;
            c.between_tools = true;
            c.per_message = true;
        }
        "claude-haiku-5-5" => {
            c.default_effort = "medium";
            c.disabled_effort_cap = true;
            c.per_message = true;
        }
        "claude-opus-4-6" | "claude-sonnet-4-6" => {
            c.mid_system = false;
            c.adaptive_only = false;
            c.effort = Effort::NoXhigh;
        }
        "claude-opus-4-5" => {
            c.mid_system = false;
            c.adaptive_only = false;
            c.effort = Effort::Three;
        }
        "claude-sonnet-4-5" | "claude-haiku-4-5" => {
            c.mid_system = false;
            c.adaptive_only = false;
            c.effort = Effort::None;
        }
        _ => return None,
    }
    Some(c)
}
fn invalid() -> Failure {
    Failure::new(FailureCode::RequestInvalid)
}
fn unsupported() -> Failure {
    Failure::new(FailureCode::ClaudeOptionsUnsupported)
}
fn effort(value: &Value) -> Result<Option<&str>, Failure> {
    let Some(v) = value.get("effort").filter(|v| !v.is_null()) else { return Ok(None) };
    match v.as_str() {
        Some(s @ ("low" | "medium" | "high" | "xhigh" | "max")) => Ok(Some(s)),
        _ => Err(invalid()),
    }
}
/// Validate only reviewed extensions. Other native content remains opaque to the adapter.
pub(crate) fn validate_extensions(v: &Value) -> Result<(), Failure> {
    if v.get("speed").is_some_and(|s| !s.is_null() && !matches!(s.as_str(), Some("standard" | "fast"))) {
        return Err(invalid());
    }
    if let Some(cache) = v.get("cache_control").filter(|v| !v.is_null()) {
        let o = cache.as_object().ok_or_else(invalid)?;
        if o.keys().any(|k| !["type", "ttl"].contains(&k.as_str()))
            || cache["type"] != "ephemeral"
            || cache.get("ttl").is_some_and(|ttl| !matches!(ttl.as_str(), Some("5m" | "1h")))
        {
            return Err(invalid());
        }
    }
    if let Some(thread) = v.get("thread").filter(|v| !v.is_null()) {
        let o = thread.as_object().ok_or_else(invalid)?;
        let previous = thread["previous_message_id"].as_str();
        let valid = match thread["type"].as_str() {
            Some("create") => !o.contains_key("previous_message_id"),
            Some("continue") => previous
                .is_some_and(|s| s.starts_with("msg_") && s.len() > 4 && s.len() <= 256 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))),
            _ => false,
        };
        if !valid || o.keys().any(|k| !["type", "previous_message_id"].contains(&k.as_str())) {
            return Err(invalid());
        }
    }
    if let Some(output) = v.get("output_config").filter(|v| !v.is_null()) {
        if !output.is_object() {
            return Err(invalid());
        }
        effort(output)?;
    }
    for (index, m) in v["messages"].as_array().into_iter().flatten().enumerate() {
        if m["role"] == "system" {
            if index == 0 {
                return Err(unsupported());
            }
            if let Some(o) = m.get("output_config").filter(|v| !v.is_null()) {
                if !o.is_object() || o.as_object().is_some_and(|o| o.keys().any(|k| k != "effort")) {
                    return Err(invalid());
                }
                effort(o)?;
            }
            if m.get("clear_at").is_some_and(|s| !s.is_null() && !matches!(s.as_str(), Some("never" | "next_user_message"))) {
                return Err(invalid());
            }
            let empty = m["content"].as_str() == Some("") || m["content"].as_array().is_some_and(Vec::is_empty);
            if empty && effort(&m["output_config"])?.is_none() {
                return Err(invalid());
            }
        } else if m.get("output_config").is_some() || m.get("clear_at").is_some() {
            return Err(invalid());
        }
    }
    Ok(())
}
fn add_beta(headers: &mut Headers, beta: &str) -> Result<(), Failure> {
    let mut values: Vec<&str> = headers.beta.as_deref().unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if !values.iter().any(|s| s.eq_ignore_ascii_case(beta)) {
        values.push(beta);
    }
    let joined = values.join(",");
    if joined.len() > 4096 {
        return Err(invalid());
    }
    headers.beta = Some(joined);
    Ok(())
}
fn forced_tool(v: &Value) -> bool {
    matches!(v.as_str(), Some("any" | "required")) || matches!(v["type"].as_str(), Some("any" | "tool"))
}
fn map_effort(output: &mut Value, c: Capabilities, cap_high: bool) -> Result<(), Failure> {
    let current = effort(output)?.map(str::to_owned);
    let Some(current) = current else { return Ok(()) };
    let mapped = match c.effort {
        Effort::None => None,
        Effort::Three if matches!(current.as_str(), "xhigh" | "max") => Some("high"),
        Effort::NoXhigh if current == "xhigh" => Some("high"),
        _ if cap_high && matches!(current.as_str(), "xhigh" | "max") => Some("high"),
        _ => Some(current.as_str()),
    };
    let object = output.as_object_mut().ok_or_else(invalid)?;
    if let Some(mapped) = mapped {
        object.insert("effort".into(), mapped.into());
    } else {
        object.remove("effort");
    }
    Ok(())
}

fn get(fields: &BTreeMap<String, &RawValue>, key: &str) -> Result<Value, Failure> {
    fields.get(key).map_or(Ok(Value::Null), |r| serde_json::from_str(r.get()).map_err(|_| invalid()))
}
fn normalize_options(
    fields: &BTreeMap<String, &RawValue>,
    c: Option<Capabilities>,
    speed: Value,
    tools: &Value,
    thinking: &mut Value,
    output: &mut Value,
    changes: &mut Map<String, Value>,
) -> Result<(), Failure> {
    if let Some(c) = c {
        if speed == "fast" && !c.fast {
            return Err(unsupported());
        }
        if forced_tool(tools) && c.force_forbidden {
            return Err(unsupported());
        }
        if c.adaptive_only {
            if let Some(t) = thinking.as_object_mut() {
                if t.get("type").is_some_and(|s| s == "enabled") {
                    t.insert("type".into(), "adaptive".into());
                }
                t.remove("budget_tokens");
                t.remove("max_tokens");
            }
            if thinking["type"] == "disabled" {
                if c.always_thinking {
                    thinking["type"] = "adaptive".into();
                } else if c.between_tools {
                    thinking["type"] = "between_tools".into();
                }
            }
            for key in ["temperature", "top_p", "top_k"] {
                if fields.contains_key(key) {
                    changes.insert(key.into(), Value::Null);
                }
            }
        }
        if thinking["type"] == "between_tools" && !c.between_tools {
            return Err(unsupported());
        }
        if matches!(thinking["type"].as_str(), Some("disabled" | "between_tools")) && thinking.as_object().is_some_and(|o| o.keys().any(|k| k != "type")) {
            return Err(unsupported());
        }
        let cap_high = (c.disabled_effort_cap && thinking["type"] == "disabled") || (c.between_tools && thinking["type"] == "between_tools");
        map_effort(output, c, cap_high)?;
        if forced_tool(tools) && thinking["type"] == "enabled" {
            return Err(unsupported());
        }
        if output.as_object().is_some_and(Map::is_empty) {
            *output = Value::Null;
        }
    }
    Ok(())
}
fn history_headers(messages: &Value, c: Option<Capabilities>, thinking: &Value, output: &Value, headers: &mut Headers) -> Result<(), Failure> {
    for m in messages.as_array().into_iter().flatten() {
        if m["role"] == "system" {
            if let Some(c) = c {
                if m.get("output_config").is_some_and(|o| !o.is_null()) && !c.per_message {
                    return Err(unsupported());
                }
                // Preserve history; never lower or rewrite a per-message effort directive.
                if (thinking["type"] == "between_tools" || thinking["type"] == "disabled")
                    && effort(&m["output_config"])?.is_some_and(|e| e != effort(output).ok().flatten().unwrap_or(c.default_effort))
                {
                    return Err(unsupported());
                }
            }
            if c.is_some_and(|c| !c.mid_system) {
                return Err(unsupported());
            }
            // Plain mid-conversation system messages are GA. Do not inject
            // long-context entitlement flags merely because history contains one.
            for block in m["content"].as_array().into_iter().flatten() {
                if matches!(block["type"].as_str(), Some("tool_addition" | "tool_removal")) {
                    add_beta(headers, "inline-tools-2026-09-15")?;
                }
            }
            if m.get("output_config").is_some_and(|o| !o.is_null()) {
                add_beta(headers, "mid-conversation-output-config-2026-07-01")?;
            }
            if m.get("clear_at").is_some_and(|o| !o.is_null()) {
                add_beta(headers, "mid-conversation-system-clear-at-2026-08-21")?;
            }
        }
    }
    Ok(())
}
fn feature_headers(fields: &BTreeMap<String, &RawValue>, thinking: &Value, headers: &mut Headers) -> Result<(), Failure> {
    if get(fields, "speed")? == "fast" {
        add_beta(headers, "fast-mode-2026-02-01")?;
    }
    if !get(fields, "thread")?.is_null() {
        add_beta(headers, "message-threads-2026-08-12")?;
    }
    if get(fields, "cache_control")?["ttl"] == "1h" {
        add_beta(headers, "extended-cache-ttl-2025-04-11")?;
    }
    if thinking["display"] == "updates" {
        add_beta(headers, "thinking-display-updates-2026-08-18")?;
    }
    if !thinking["block_binding"].is_null() {
        add_beta(headers, "thinking-binding-controls-2026-08-01")?;
    }
    Ok(())
}

impl Request {
    /// Apply only top-level compatibility edits. Untouched fields, especially messages,
    /// retain their original JSON bytes. A no-op returns the complete borrowed input.
    pub fn prepare<'a>(&self, raw: &'a [u8]) -> Result<Prepared<'a>, Failure> {
        // Callers cannot use this public helper to skip the normal policy/size check.
        Request::parse(raw, self.endpoint, self.headers.clone())?;
        let mut headers = self.headers.clone();
        if self.endpoint == Endpoint::Models {
            return Ok(Prepared { body: Cow::Borrowed(raw), headers });
        }
        let fields: BTreeMap<String, &RawValue> = serde_json::from_slice(raw).map_err(|_| invalid())?;
        let model = get(&fields, "model")?;
        let c = capabilities(model.as_str().ok_or_else(invalid)?);
        let original_thinking = get(&fields, "thinking")?;
        let mut thinking = original_thinking.clone();
        let original_output = get(&fields, "output_config")?;
        let mut output = original_output.clone();
        let tools = get(&fields, "tool_choice")?;
        let messages = get(&fields, "messages")?;
        let mut changes: Map<String, Value> = Map::new();
        normalize_options(&fields, c, get(&fields, "speed")?, &tools, &mut thinking, &mut output, &mut changes)?;
        history_headers(&messages, c, &thinking, &output, &mut headers)?;
        feature_headers(&fields, &thinking, &mut headers)?;
        if thinking != original_thinking {
            changes.insert("thinking".into(), thinking);
        }
        if output != original_output {
            changes.insert("output_config".into(), output);
        }
        if changes.is_empty() {
            return Ok(Prepared { body: Cow::Borrowed(raw), headers });
        }
        let owned: BTreeMap<String, Box<RawValue>> = changes
            .iter()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| Ok((k.clone(), serde_json::value::to_raw_value(v).map_err(|_| invalid())?)))
            .collect::<Result<_, Failure>>()?;
        let mut result = fields;
        for key in changes.keys() {
            result.remove(key);
        }
        for (key, value) in &owned {
            result.insert(key.clone(), value.as_ref());
        }
        let body = serde_json::to_vec(&result).map_err(|_| invalid())?;
        Ok(Prepared { body: Cow::Owned(body), headers })
    }
}

#[cfg(test)]
#[path = "claude_compat_tests.rs"]
mod tests;
