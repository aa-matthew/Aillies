//! Identify upstream bodies by content rather than trusting the MIME header.

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Pending,
    Sse,
    Json,
    Other,
}

pub fn detect(data: &[u8]) -> Format {
    const BOM: [u8; 3] = [0xef, 0xbb, 0xbf];
    if !data.is_empty() && data.len() < 3 && data == &BOM[..data.len()] {
        return Format::Pending;
    }
    let text = String::from_utf8_lossy(data);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text).trim();
    if text.is_empty() {
        return Format::Pending;
    }
    if text.starts_with("event:") || text.starts_with("data:") || text.starts_with(':') {
        return Format::Sse;
    }
    if text.starts_with('{') || text.starts_with('[') {
        return Format::Json;
    }
    if "event:".starts_with(text) || "data:".starts_with(text) {
        return Format::Pending;
    }
    Format::Other
}

fn event(kind: &str, value: &Value) -> Vec<u8> {
    let mut out = format!("event: {kind}\ndata: ").into_bytes();
    out.extend(serde_json::to_vec(value).unwrap_or_default());
    out.extend_from_slice(b"\n\n");
    out
}

/// Turn a completed Responses JSON document into a finite SSE sequence.
/// Buffered; never pretends tokens arrived incrementally.
pub fn completed_json(data: &[u8]) -> Option<Vec<Vec<u8>>> {
    let response: Value = serde_json::from_slice(data).ok()?;
    if response.get("object")?.as_str()? != "response" || response.get("status")?.as_str()? != "completed" {
        return None;
    }
    let output = response.get("output")?.as_array()?;
    let mut events = Vec::with_capacity(output.len() + 1);
    for (index, item) in output.iter().enumerate() {
        events.push(event("response.output_item.done", &json!({"type": "response.output_item.done", "output_index": index, "item": item})));
    }
    events.push(event("response.completed", &json!({"type": "response.completed", "response": response})));
    Some(events)
}

/// Scan SSE bytes for a terminal event so the requester can tell a finished
/// response from a truncated one.
#[derive(Default)]
pub struct TerminalScanner {
    tail: Vec<u8>,
    pub completed: bool,
}

impl TerminalScanner {
    pub fn feed(&mut self, chunk: &[u8]) {
        if self.completed {
            return;
        }
        self.tail.extend_from_slice(chunk);
        let hay = String::from_utf8_lossy(&self.tail);
        if hay.contains("response.completed") || hay.contains("response.failed") || hay.contains("response.incomplete") {
            self.completed = true;
        }
        let keep = self.tail.len().saturating_sub(64);
        self.tail.drain(..keep);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_formats() {
        assert_eq!(detect(b""), Format::Pending);
        assert_eq!(detect(&[0xef]), Format::Pending);
        assert_eq!(detect(b"ev"), Format::Pending);
        assert_eq!(detect(b"event: response.created\n"), Format::Sse);
        assert_eq!(detect("\u{feff}data: {}".as_bytes()), Format::Sse);
        assert_eq!(detect(b"  {\"a\":1"), Format::Json);
        assert_eq!(detect(b"<html>"), Format::Other);
    }

    #[test]
    fn completed_json_to_events() {
        let doc = br#"{"object":"response","status":"completed","output":[{"type":"message"}]}"#;
        let ev = completed_json(doc).unwrap();
        assert_eq!(ev.len(), 2);
        assert!(String::from_utf8_lossy(&ev[1]).starts_with("event: response.completed"));
        assert!(completed_json(br#"{"object":"response","status":"failed","output":[]}"#).is_none());
        assert!(completed_json(br#"{"error":{"message":"x"}}"#).is_none());
    }

    #[test]
    fn scanner_across_chunks() {
        let mut s = TerminalScanner::default();
        s.feed(b"event: response.compl");
        assert!(!s.completed);
        s.feed(b"eted\ndata: {}\n\n");
        assert!(s.completed);
    }
}
