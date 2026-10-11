//! Session selection follows pinned 9router ce4460e. No content is retained.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const SESSION_HEADERS: &[&str] = &["x-session-id", "session-id", "session_id", "x-amp-thread-id", "x-client-request-id"];

pub fn bounded(value: &str) -> Option<String> {
    let v = value.trim();
    (!v.is_empty() && v.len() <= 256 && v.bytes().all(|b| (32..=126).contains(&b))).then(|| v.to_owned())
}

/// Capture before normalization removes client-only body fields.
pub fn client_session(raw: &[u8], header: Option<&str>) -> Option<String> {
    if let Some(v) = header.and_then(bounded) {
        return Some(v);
    }
    let body: Value = serde_json::from_slice(raw).ok()?;
    let hint = [body.get("prompt_cache_key"), body.get("session_id"), body.get("conversation_id"), body.get("metadata").and_then(|m| m.get("user_id"))]
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find_map(bounded);
    hint
}

struct Entry {
    key: [u8; 32],
    id: String,
    last_used: Instant,
}
#[derive(Default)]
struct Sessions {
    assistant: VecDeque<Entry>,
    account: VecDeque<Entry>,
}

fn cached(entries: &mut VecDeque<Entry>, key: [u8; 32], cap: usize, now: Instant) -> String {
    entries.retain(|e| now.duration_since(e.last_used) < Duration::from_secs(7200));
    if let Some(entry) = entries.iter_mut().find(|e| e.key == key) {
        entry.last_used = now;
        return entry.id.clone();
    }
    if entries.len() >= cap {
        entries.pop_front();
    }
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let id = format!("{}{millis}", uuid::Uuid::new_v4());
    entries.push_back(Entry { key, id: id.clone(), last_used: now });
    id
}

fn assistant_key(body: &Value, account: &str) -> Option<[u8; 32]> {
    let mut text = String::new();
    for item in body.get("input")?.as_array()? {
        if item["role"] != "assistant" {
            continue;
        }
        let mut append = |s: &str| {
            for c in s.chars() {
                if text.encode_utf16().count() >= 50 {
                    break;
                }
                text.push(c);
            }
        };
        if let Some(s) = item["content"].as_str() {
            append(s);
        } else if let Some(blocks) = item["content"].as_array() {
            for block in blocks {
                if let Some(s) = block.get("text").filter(|v| v.as_str().is_some_and(|s| !s.is_empty())).or_else(|| block.get("output")).and_then(Value::as_str)
                {
                    append(s);
                }
            }
        }
        if text.encode_utf16().count() >= 50 {
            break;
        }
    }
    if text.encode_utf16().count() < 50 {
        return None;
    }
    let units: Vec<_> = text.encode_utf16().take(50).collect();
    let text = String::from_utf16_lossy(&units);
    let encoded = serde_json::to_vec(&("codex", account, text)).ok()?;
    Some(Sha256::digest(encoded).into())
}

pub(crate) fn resolve(body: &Value, account: &str, hint: Option<&str>) -> String {
    if let Some(hint) = hint.and_then(bounded) {
        return hint;
    }
    if let Some(key) = body["prompt_cache_key"].as_str().and_then(bounded) {
        return key;
    }
    static STORE: OnceLock<Mutex<Sessions>> = OnceLock::new();
    let mut store = STORE.get_or_init(|| Mutex::new(Sessions::default())).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(key) = assistant_key(body, account) {
        cached(&mut store.assistant, key, 5000, Instant::now())
    } else {
        cached(&mut store.account, Sha256::digest(account.as_bytes()).into(), 1000, Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn header_precedes_cache_key_and_body_hints_are_bounded() {
        assert_eq!(client_session(br#"{"prompt_cache_key":"cache","session_id":"body"}"#, Some("header")), Some("header".into()));
        assert_eq!(client_session(br#"{"session_id":"body"}"#, None), Some("body".into()));
        assert_eq!(client_session(br#"{"conversation_id":"chat"}"#, Some("bad\r\nvalue")), Some("chat".into()));
        assert_eq!(resolve(&json!({"prompt_cache_key":"cache"}), "acct", Some("header")), "header");
    }
    #[test]
    fn session_selection_matches_pinned_router_relationships() {
        let cases: Vec<Value> = serde_json::from_str(include_str!("../tests/9router-sessions.json")).unwrap();
        let mut groups = std::collections::HashMap::new();
        for case in cases {
            let hint = SESSION_HEADERS.iter().find_map(|h| case["headers"][h].as_str().and_then(bounded));
            let raw = serde_json::to_vec(&case["body"]).unwrap();
            let hint = client_session(&raw, hint.as_deref());
            let actual = resolve(&case["body"], case["connectionId"].as_str().unwrap(), hint.as_deref());
            if let Some(expected) = case["expected"].as_str() {
                assert_eq!(actual, expected, "{}", case["name"]);
            } else {
                let group = case["group"].as_u64().unwrap();
                if let Some(previous) = groups.get(&group) {
                    assert_eq!(&actual, previous, "{}", case["name"]);
                } else {
                    assert!(!groups.values().any(|id| id == &actual), "{}", case["name"]);
                    groups.insert(group, actual);
                }
            }
        }
    }

    #[test]
    fn fallback_is_scoped_stable_bounded_and_expires() {
        let body = json!({"input":[{"role":"assistant","content":"a".repeat(50)}]});
        assert_eq!(resolve(&body, "account-a", None), resolve(&body, "account-a", None));
        assert_ne!(resolve(&body, "account-a", None), resolve(&body, "account-b", None));
        let mut entries = VecDeque::new();
        let now = Instant::now();
        let first = cached(&mut entries, [0; 32], 2, now);
        assert_eq!(first, cached(&mut entries, [0; 32], 2, now));
        cached(&mut entries, [1; 32], 2, now);
        cached(&mut entries, [2; 32], 2, now);
        assert_eq!(entries.len(), 2);
        assert_ne!(first, cached(&mut entries, [0; 32], 2, now));
        let later = now + Duration::from_secs(7201);
        cached(&mut entries, [3; 32], 2, later);
        assert_eq!(entries.len(), 1);
    }
}
