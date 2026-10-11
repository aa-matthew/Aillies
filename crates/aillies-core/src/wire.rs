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

/// Events after which a Responses stream is finished (successfully or not).
pub const TERMINAL_EVENTS: [&str; 3] = ["response.completed", "response.failed", "response.incomplete"];

/// Longest line prefix kept while parsing. Event names and the leading
/// `{"type": …}` of a data payload fit easily; huge payloads (images) are
/// never buffered whole.
const LINE_KEEP: usize = 4096;

/// Incremental SSE parser that detects a real terminal event, so the
/// requester can tell a finished response from a cut one. It follows the SSE
/// framing rules (fields per line, `\r\n` or `\n`, dispatch on a blank line,
/// `:` comments) and works on arbitrarily fragmented chunks. Text inside a
/// payload that merely mentions an event name doesn't count.
#[derive(Default)]
pub struct TerminalScanner {
    line: Vec<u8>,
    line_truncated: bool,
    event: Option<String>,
    first_data: Option<(Vec<u8>, bool)>,
    pub completed: bool,
}

impl TerminalScanner {
    pub fn feed(&mut self, chunk: &[u8]) {
        let mut rest = chunk;
        while !self.completed && !rest.is_empty() {
            let (segment, ended) = match rest.iter().position(|&b| b == b'\n') {
                Some(i) => (&rest[..i], true),
                None => (rest, false),
            };
            let room = LINE_KEEP.saturating_sub(self.line.len());
            if segment.len() > room {
                self.line.extend_from_slice(&segment[..room]);
                self.line_truncated = true;
            } else {
                self.line.extend_from_slice(segment);
            }
            rest = if ended { &rest[segment.len() + 1..] } else { &[] };
            if ended {
                self.end_line();
            }
        }
    }

    fn end_line(&mut self) {
        let mut line = std::mem::take(&mut self.line);
        let truncated = std::mem::replace(&mut self.line_truncated, false);
        if !truncated && line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.is_empty() {
            self.dispatch();
            return;
        }
        if line[0] == b':' {
            return; // comment
        }
        let (field, value) = match line.iter().position(|&b| b == b':') {
            Some(i) => {
                let v = &line[i + 1..];
                (&line[..i], v.strip_prefix(b" ").unwrap_or(v))
            }
            None => (&line[..], &[][..]),
        };
        match field {
            b"event" if !truncated => self.event = Some(String::from_utf8_lossy(value).into_owned()),
            b"data" if self.first_data.is_none() => self.first_data = Some((value.to_vec(), truncated)),
            _ => {}
        }
    }

    fn dispatch(&mut self) {
        let event = self.event.take();
        let data = self.first_data.take();
        let name = event.or_else(|| data.and_then(|(bytes, truncated)| data_type(&bytes, truncated)));
        if name.is_some_and(|n| TERMINAL_EVENTS.contains(&n.as_str())) {
            self.completed = true;
        }
    }
}

/// The `type` of a JSON data payload. Complete payloads are parsed; a
/// truncated one is read only if it starts with `{"type":"…"`, which is how
/// Responses events are serialized.
fn data_type(bytes: &[u8], truncated: bool) -> Option<String> {
    if !truncated {
        if let Ok(Value::Object(m)) = serde_json::from_slice::<Value>(bytes) {
            return m.get("type").and_then(Value::as_str).map(str::to_owned);
        }
    }
    let text = String::from_utf8_lossy(bytes);
    let rest = text.trim_start().strip_prefix('{')?.trim_start().strip_prefix("\"type\"")?.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
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
    fn scanner_ignores_terminal_names_inside_content() {
        // Regression: model output that mentions the event name must not count as completion.
        let mut s = TerminalScanner::default();
        s.feed(b"event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"the response.completed event\"}\n\n");
        assert!(!s.completed);
        s.feed(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"response.failed and response.incomplete\"}\n\n");
        assert!(!s.completed);
        s.feed(b": comment response.completed\n\n");
        assert!(!s.completed);
        // An event line naming it without the blank-line terminator hasn't been dispatched yet.
        s.feed(b"event: response.completed\n");
        assert!(!s.completed);
        s.feed(b"data: {}\n\n");
        assert!(s.completed);
    }

    #[test]
    fn scanner_terminal_by_data_type_crlf_and_byte_fragments() {
        let stream = b"data: {\"type\":\"response.created\"}\r\n\r\ndata: {\"type\":\"response.failed\",\"response\":{}}\r\n\r\n";
        let mut s = TerminalScanner::default();
        for (i, b) in stream.iter().enumerate() {
            s.feed(std::slice::from_ref(b));
            if i + 1 < stream.len() {
                assert!(!s.completed, "completed too early at byte {i}");
            }
        }
        assert!(s.completed);
    }

    #[test]
    fn scanner_handles_huge_terminal_payloads() {
        // response.completed can carry whole images; detection must not depend on buffering it all.
        let mut s = TerminalScanner::default();
        s.feed(b"data: {\"type\":\"response.completed\",\"response\":{\"output\":\"");
        let big = vec![b'A'; 3 * 1024 * 1024];
        for chunk in big.chunks(65536) {
            s.feed(chunk);
        }
        s.feed(b"\"}}\n\n");
        assert!(s.completed);
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
