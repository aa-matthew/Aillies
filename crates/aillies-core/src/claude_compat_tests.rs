use super::*;
use crate::claude::{Endpoint, Headers, Request};
use serde_json::json;

fn prepared(v: &Value) -> (Value, Headers) {
    let raw = serde_json::to_vec(v).unwrap();
    let r = Request::parse(&raw, Endpoint::Messages, Headers::default()).unwrap();
    let p = r.prepare(&raw).unwrap();
    (serde_json::from_slice(&p.body).unwrap(), p.headers)
}
fn base(model: &str) -> Value {
    json!({"model":model,"max_tokens":1024,"messages":[{"role":"user","content":"fixture"}]})
}
#[test]
fn frontier_manual_thinking_becomes_adaptive_without_changing_model_or_history() {
    for model in [
        "claude-opus-4-7",
        "claude-opus-4-8",
        "claude-opus-5",
        "claude-opus-5-5",
        "claude-sonnet-5",
        "claude-sonnet-5-5",
        "claude-haiku-5-5",
        "claude-fable-5",
        "claude-fable-5-1",
        "claude-mythos-5-1",
    ] {
        let mut v = base(model);
        v["thinking"] = json!({"type":"enabled","budget_tokens":32000,"display":"summarized"});
        v["output_config"] = json!({"effort":"max","format":{"type":"json_schema","schema":{"type":"object"}}});
        v["temperature"] = json!(0.5);
        v["top_p"] = json!(0.8);
        v["top_k"] = json!(20);
        let (out, h) = prepared(&v);
        assert_eq!(out["model"], model);
        assert_eq!(out["messages"], v["messages"]);
        assert_eq!(out["thinking"], json!({"type":"adaptive","display":"summarized"}));
        assert_eq!(out["output_config"], v["output_config"]);
        for k in ["temperature", "top_p", "top_k"] {
            assert!(out.get(k).is_none());
        }
        assert!(h.beta.is_none());
    }
}
#[test]
fn no_op_keeps_every_native_byte_and_unknown_models_are_not_guessed() {
    for model in ["claude-sonnet-4-6", "claude-future-99"] {
        let raw = format!(
            r#"{{ "model": "{model}", "max_tokens": 1024, "messages": [{{"role":"user", "content":"keep \\ escapes"}}], "thinking": {{"type":"enabled", "budget_tokens":512}} }}"#
        );
        let r = Request::parse(raw.as_bytes(), Endpoint::Messages, Headers::default()).unwrap();
        let p = r.prepare(raw.as_bytes()).unwrap();
        assert!(matches!(p.body, Cow::Borrowed(_)));
        assert_eq!(p.body.as_ref(), raw.as_bytes());
    }
}
#[test]
fn signed_messages_tools_system_and_cache_bytes_survive_normalization() {
    let raw = br#"{"model":"claude-opus-5-5","max_tokens":1024,"thinking":{"type":"enabled","budget_tokens":1024},"messages": [ {"role":"assistant","content":[{"type":"thinking", "thinking":"keep\u0020text", "signature":"ORIGINAL_SIGNATURE"},{"type":"tool_use","id":"tool1","name":"read","input":{"path":"file"}}]}, {"role":"user","content":[{"type":"tool_result","tool_use_id":"tool1","content":"SECRET_RESULT"}]} ],"system":[ {"type":"text","text":"SECRET_SYSTEM","cache_control":{"type":"ephemeral"}} ],"tools":[ {"name":"read", "input_schema":{"type":"object"}} ]}"#;
    let r = Request::parse(raw, Endpoint::Messages, Headers::default()).unwrap();
    let p = r.prepare(raw).unwrap();
    let old: std::collections::BTreeMap<String, &serde_json::value::RawValue> = serde_json::from_slice(raw).unwrap();
    let new: std::collections::BTreeMap<String, &serde_json::value::RawValue> = serde_json::from_slice(&p.body).unwrap();
    for k in ["messages", "system", "tools", "model"] {
        assert_eq!(old[k].get(), new[k].get());
    }
}
#[test]
fn effort_uses_current_contract_and_preserves_structured_outputs() {
    for model in ["claude-sonnet-5-5", "claude-haiku-5-5", "claude-opus-5-5", "claude-fable-5-1"] {
        let mut v = base(model);
        v["output_config"] = json!({"effort":"max","format":{"type":"json_schema","schema":{"type":"object"}}});
        assert_eq!(prepared(&v).0["output_config"], v["output_config"]);
    }
    for model in ["claude-opus-4-6", "claude-sonnet-4-6"] {
        let mut v = base(model);
        v["output_config"] = json!({"effort":"xhigh"});
        assert_eq!(prepared(&v).0["output_config"]["effort"], "high");
    }
    let mut v = base("claude-haiku-4-5-20251001");
    v["output_config"] = json!({"effort":"high","format":{"type":"json_schema","schema":{"type":"object"}}});
    assert!(prepared(&v).0["output_config"].get("effort").is_none());
    assert_eq!(prepared(&v).0["output_config"]["format"], v["output_config"]["format"]);
}
#[test]
fn disabled_thinking_respects_current_model_contract() {
    for model in ["claude-opus-5-5", "claude-fable-5-1"] {
        let mut v = base(model);
        v["thinking"] = json!({"type":"disabled"});
        assert_eq!(prepared(&v).0["thinking"]["type"], "adaptive");
    }
    let mut v = base("claude-sonnet-5-5");
    v["thinking"] = json!({"type":"disabled"});
    v["output_config"] = json!({"effort":"medium"});
    assert_eq!(prepared(&v).0["thinking"]["type"], "between_tools");
    for model in ["claude-haiku-5-5", "claude-opus-5"] {
        let mut v = base(model);
        v["thinking"] = json!({"type":"disabled"});
        v["output_config"] = json!({"effort":"max"});
        assert_eq!(prepared(&v).0["output_config"]["effort"], "high");
    }
}
#[test]
fn reviewed_fields_and_feature_betas_are_validated_without_blanket_flags() {
    let mut v = base("claude-opus-5-5");
    v["speed"] = json!("fast");
    v["thread"] = json!({"type":"continue","previous_message_id":"msg_01abc"});
    v["cache_control"] = json!({"type":"ephemeral","ttl":"1h"});
    let (out, h) = prepared(&v);
    assert_eq!(out, v);
    let betas = h.beta.unwrap();
    assert!(betas.contains("fast-mode-2026-02-01"));
    assert!(betas.contains("message-threads-2026-08-12"));
    assert!(betas.contains("extended-cache-ttl-2025-04-11"));
    assert!(!betas.contains("skills-2025-10-02"));
    for (k, val) in [
        ("speed", json!("turbo")),
        ("thread", json!({"type":"continue"})),
        ("thread", json!({"type":"create","previous_message_id":"SECRET"})),
        ("cache_control", json!({"type":"permanent"})),
    ] {
        let mut bad = base("claude-opus-5-5");
        bad[k] = val;
        let raw = serde_json::to_vec(&bad).unwrap();
        assert!(Request::parse(&raw, Endpoint::Messages, Headers::default()).is_err());
    }
}
#[test]
fn unsupported_forced_tools_and_fast_models_refuse_before_send() {
    for model in ["claude-opus-5-5", "claude-sonnet-5-5", "claude-fable-5-1"] {
        let mut v = base(model);
        v["tool_choice"] = json!({"type":"tool","name":"read"});
        let raw = serde_json::to_vec(&v).unwrap();
        let r = Request::parse(&raw, Endpoint::Messages, Headers::default()).unwrap();
        let e = r.prepare(&raw).err().unwrap();
        assert_eq!(e.code, FailureCode::ClaudeOptionsUnsupported);
        assert!(!e.may_have_been_delivered());
    }
    let mut v = base("claude-sonnet-5-5");
    v["speed"] = json!("fast");
    let raw = serde_json::to_vec(&v).unwrap();
    let r = Request::parse(&raw, Endpoint::Messages, Headers::default()).unwrap();
    assert_eq!(r.prepare(&raw).err().unwrap().code, FailureCode::ClaudeOptionsUnsupported);
}
#[test]
fn per_message_effort_is_preserved_and_only_supported_models_accept_it() {
    let mut v = base("claude-sonnet-5-5");
    v["messages"].as_array_mut().unwrap().push(json!({"role":"system","content":[],"output_config":{"effort":"max"}}));
    let (out, h) = prepared(&v);
    assert_eq!(out, v);
    assert!(h.beta.unwrap().contains("mid-conversation-output-config-2026-07-01"));
    v["model"] = json!("claude-sonnet-4-6");
    let raw = serde_json::to_vec(&v).unwrap();
    let r = Request::parse(&raw, Endpoint::Messages, Headers::default()).unwrap();
    assert!(r.prepare(&raw).is_err());
}
#[test]
fn normalization_is_idempotent_and_betas_are_deduplicated() {
    let mut v = base("claude-opus-5-5");
    v["thinking"] = json!({"type":"enabled","budget_tokens":1024});
    v["speed"] = json!("fast");
    let raw = serde_json::to_vec(&v).unwrap();
    let r = Request::parse(&raw, Endpoint::Messages, Headers::new("2023-06-01", Some("fast-mode-2026-02-01,effort-2025-11-24")).unwrap()).unwrap();
    let first = r.prepare(&raw).unwrap();
    let next = Request::parse(&first.body, Endpoint::Messages, first.headers.clone()).unwrap().prepare(&first.body).unwrap();
    assert_eq!(first.body, next.body);
    assert_eq!(first.headers.beta, next.headers.beta);
    assert_eq!(first.headers.beta.unwrap().matches("fast-mode-2026-02-01").count(), 1);
}

#[test]
fn system_history_uses_only_requested_feature_betas_and_effective_default_effort() {
    let mut v = base("claude-haiku-5-5");
    v["thinking"] = json!({"type":"disabled"});
    v["messages"].as_array_mut().unwrap().push(json!({"role":"system","content":[],"output_config":{"effort":"medium"}}));
    let (out, h) = prepared(&v);
    assert_eq!(out, v);
    assert!(!h.beta.unwrap().contains("context-1m"));
    v["messages"][1] = json!({"role":"system","content":"updated instruction"});
    assert!(prepared(&v).1.beta.is_none());
    v["model"] = json!("claude-sonnet-5");
    let raw = serde_json::to_vec(&v).unwrap();
    let r = Request::parse(&raw, Endpoint::Messages, Headers::default()).unwrap();
    assert_eq!(r.prepare(&raw).err().unwrap().code, FailureCode::ClaudeOptionsUnsupported);
}

#[test]
fn forced_tools_work_with_adaptive_models_that_support_them() {
    for model in ["claude-opus-4-7", "claude-opus-5", "claude-sonnet-5", "claude-haiku-5-5", "claude-fable-5"] {
        let mut v = base(model);
        v["thinking"] = json!({"type":"adaptive"});
        v["tool_choice"] = json!({"type":"tool","name":"read"});
        assert_eq!(prepared(&v).0, v);
    }
    let mut v = base("claude-opus-5-5");
    v["thinking"] = json!({"type":"adaptive","display":"updates","block_binding":{"prefix_mismatch_behavior":"error"}});
    let beta = prepared(&v).1.beta.unwrap();
    assert!(beta.contains("thinking-display-updates-2026-08-18"));
    assert!(beta.contains("thinking-binding-controls-2026-08-01"));
}
