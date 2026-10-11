//! Bounded, content-free request diagnostics. No arbitrary request strings survive.

use crate::{
    body::{self, Body},
    node::Node,
    store::{now, private_write},
};
use aillies_core::{wire::TerminalScanner, Failure, FailureCode};
use http_body_util::BodyExt;
use hyper::Response;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const LIMIT: usize = 100;
const MAX_LOG_BYTES: u64 = 256 * 1024;
const LOG_NAME: &str = "request-diagnostics.json";
pub const DIAGNOSTICS_BUILD: &str = "router-core-v4-pairing";

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Route {
    Loopback,
    Local,
    Peer,
    Hosted,
}

// Closed enums are intentional: even model/effort fields can contain user content.
#[derive(Clone, Copy, Serialize, Deserialize)]
enum Model {
    #[serde(rename = "gpt-6.1-sol")]
    Sol61,
    #[serde(rename = "gpt-6-astra")]
    Astra6,
    #[serde(rename = "gpt-6-sol")]
    Sol6,
    #[serde(rename = "gpt-6-luna")]
    Luna6,
    #[serde(rename = "gpt-5.6-sol")]
    Sol56,
    #[serde(rename = "gpt-5.6-terra")]
    Terra56,
    #[serde(rename = "gpt-5.6-luna")]
    Luna56,
    #[serde(rename = "gpt-5.5")]
    Gpt55,
    #[serde(rename = "other")]
    Other,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Effort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
    Other,
    Unspecified,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Tier {
    Default,
    Auto,
    Priority,
    Fast,
    Other,
    Unspecified,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestContext {
    #[serde(skip, default = "std::time::Instant::now")]
    received: std::time::Instant,
    #[serde(default)]
    pub(crate) trace_id: uuid::Uuid,
    #[serde(default = "first_attempt")]
    pub(crate) attempt: u8,
    #[serde(skip, default = "initial_send")]
    pub(crate) initial_send: bool,
    #[serde(default)]
    rejection: Option<aillies_core::request::RequestRejection>,
    route: Route,
    model: Model,
    reasoning_effort: Effort,
    service_tier: Tier,
    lite: bool,
    image_tool: bool,
    input_items: usize,
    tools: usize,
}

impl RequestContext {
    pub(crate) fn with_attempt(mut self, attempt: u8) -> Self {
        self.attempt = attempt;
        self
    }
    pub(crate) fn with_fast(mut self, fast: bool) -> Self {
        self.service_tier = if fast { Tier::Priority } else { Tier::Default };
        self
    }

    pub(crate) fn from_raw(route: Route, raw: &[u8]) -> Self {
        let v: Value = serde_json::from_slice(raw).unwrap_or(Value::Null);
        let normalized = aillies_core::request::normalize(raw);
        let model_value = normalized.as_ref().ok().map(|n| json!(n.model)).or_else(|| v.get("model").cloned());
        let model = model_value.and_then(|v| serde_json::from_value(v).ok()).unwrap_or(Model::Other);
        let effort = v.get("reasoning").and_then(|v| v.get("effort"));
        let reasoning_effort = effort.map(|v| serde_json::from_value(v.clone()).unwrap_or(Effort::Other)).unwrap_or(Effort::Unspecified);
        let tools = v.get("tools").and_then(Value::as_array);
        let embedded =
            v.get("input").and_then(Value::as_array).into_iter().flatten().filter(|i| i["type"] == "additional_tools").filter_map(|i| i["tools"].as_array());
        let mut all_tools = tools.into_iter().chain(embedded).flatten();
        let image_tool = all_tools.clone().any(|t| t["type"] == "image_generation");
        let tool_count = all_tools.by_ref().map(count_tool).sum();
        let lite = v.as_object().is_some_and(aillies_core::request::uses_lite);
        let service_tier = v.get("service_tier").map(|v| serde_json::from_value(v.clone()).unwrap_or(Tier::Other)).unwrap_or(Tier::Unspecified);
        Self {
            received: std::time::Instant::now(),
            trace_id: uuid::Uuid::new_v4(),
            attempt: 1,
            initial_send: true,
            rejection: normalized.err().map(|e| e.reason()),
            route,
            service_tier,
            model,
            reasoning_effort,
            lite,
            image_tool,
            input_items: v.get("input").map(|v| v.as_array().map(Vec::len).unwrap_or(usize::from(v.is_string()))).unwrap_or(0),
            tools: tool_count,
        }
    }
}

fn first_attempt() -> u8 {
    1
}

fn initial_send() -> bool {
    true
}

fn count_tool(tool: &Value) -> usize {
    1 + tool.get("tools").and_then(Value::as_array).into_iter().flatten().map(count_tool).sum::<usize>()
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Level {
    Warning,
    Error,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    at_unix_seconds: u64,
    level: Level,
    code: FailureCode,
    upstream_status: Option<u16>,
    request: RequestContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    retry: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarningView {
    pub at: u64,
    pub code: FailureCode,
    pub message: String,
    pub retry: Option<u8>,
}

pub(crate) struct RequestLog {
    path: PathBuf,
    entries: VecDeque<Entry>,
    persistence_ok: bool,
    traffic: TrafficLog,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrafficLog {
    incoming: VecDeque<TrafficEntry>,
    upstream: VecDeque<TrafficEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrafficEntry {
    at_unix_seconds: u64,
    request: RequestContext,
    event: aillies_codex::telemetry::UpstreamEvent,
}

impl RequestLog {
    pub(crate) fn load(home: &Path) -> Self {
        let path = home.join(LOG_NAME);
        let mut entries: VecDeque<Entry> = VecDeque::new();
        let mut persistence_ok = true;
        if path.exists() {
            let loaded = (|| -> anyhow::Result<VecDeque<Entry>> {
                anyhow::ensure!(!std::fs::symlink_metadata(&path)?.file_type().is_symlink(), "Invalid diagnostic file.");
                let mut bytes = Vec::new();
                std::fs::File::open(&path)?.take(MAX_LOG_BYTES + 1).read_to_end(&mut bytes)?;
                anyhow::ensure!(bytes.len() as u64 <= MAX_LOG_BYTES, "Diagnostic file too large.");
                Ok(serde_json::from_slice(&bytes)?)
            })();
            match loaded {
                Ok(v) => entries = v,
                Err(_) => persistence_ok = false,
            }
        }
        while entries.len() > LIMIT {
            entries.pop_front();
        }
        let traffic_path = home.join("traffic-diagnostics.json");
        let mut traffic = if traffic_path.exists() {
            (|| -> Option<TrafficLog> {
                if std::fs::symlink_metadata(&traffic_path).ok()?.file_type().is_symlink() {
                    return None;
                }
                let mut bytes = Vec::new();
                std::fs::File::open(&traffic_path).ok()?.take(MAX_LOG_BYTES * 2 + 1).read_to_end(&mut bytes).ok()?;
                if bytes.len() as u64 > MAX_LOG_BYTES * 2 {
                    return None;
                }
                let mut log: TrafficLog = serde_json::from_slice(&bytes).ok()?;
                while log.incoming.len() > LIMIT {
                    log.incoming.pop_front();
                }
                while log.upstream.len() > LIMIT {
                    log.upstream.pop_front();
                }
                Some(log)
            })()
            .unwrap_or_else(|| {
                persistence_ok = false;
                TrafficLog::default()
            })
        } else {
            TrafficLog::default()
        };
        for entry in traffic.incoming.iter_mut().chain(traffic.upstream.iter_mut()) {
            use aillies_codex::telemetry::Phase;
            if matches!(entry.event.phase, Phase::Received | Phase::Sending | Phase::HeadersReceived | Phase::Receiving) {
                entry.event.phase = Phase::DeliveryUncertain;
                entry.event.failure = Some(FailureCode::DeliveryUncertain);
            }
        }
        Self { path, entries, persistence_ok, traffic }
    }

    pub(crate) fn record(&mut self, request: RequestContext, failure: &Failure) {
        self.record_at(request, failure, now());
    }

    pub(crate) fn record_retry(&mut self, request: RequestContext, failure: &Failure, retry: u8) {
        self.record_entry(request, failure, now(), Some(retry.min(crate::retry::MAX_RETRIES)));
    }

    pub(crate) fn recent_warnings(&self) -> Vec<WarningView> {
        self.entries
            .iter()
            .rev()
            .take(4)
            .map(|entry| WarningView {
                at: entry.at_unix_seconds,
                code: entry.code,
                message: Failure { code: entry.code, upstream_status: entry.upstream_status }.message(),
                retry: entry.retry,
            })
            .collect()
    }

    fn record_at(&mut self, request: RequestContext, failure: &Failure, at_unix_seconds: u64) {
        self.record_entry(request, failure, at_unix_seconds, None);
    }

    fn record_entry(&mut self, request: RequestContext, failure: &Failure, at_unix_seconds: u64, retry: Option<u8>) {
        let level = match (retry, failure.code) {
            (Some(_), _) => Level::Warning,
            (_, FailureCode::UpstreamRejected | FailureCode::UnexpectedUpstreamFormat | FailureCode::UnexpectedJsonResponse | FailureCode::StreamFailed) => {
                Level::Error
            }
            _ => Level::Warning,
        };
        self.entries.push_back(Entry { at_unix_seconds, level, code: failure.code, upstream_status: failure.upstream_status, request, retry });
        while self.entries.len() > LIMIT {
            self.entries.pop_front();
        }
        self.persistence_ok &= serde_json::to_vec_pretty(&self.entries).ok().is_some_and(|bytes| private_write(&self.path, &bytes).is_ok());
    }

    pub(crate) fn report(&self) -> Value {
        json!({"schema_version":2, "app_version":aillies_core::APP_VERSION, "diagnostics_build":DIAGNOSTICS_BUILD,
            "upstream_originator":aillies_codex::executor::CLIENT_ORIGINATOR,
            "upstream_client_version":aillies_codex::executor::CLIENT_VERSION,
            "platform":crate::node::platform(), "capacity":LIMIT, "persistence_ok":self.persistence_ok,
            "entries":self.entries,"incoming":self.traffic.incoming,"upstream":self.traffic.upstream})
    }

    pub(crate) fn traffic(&mut self, request: RequestContext, upstream: bool, mut event: aillies_codex::telemetry::UpstreamEvent) {
        let entries = if upstream { &mut self.traffic.upstream } else { &mut self.traffic.incoming };
        if let Some(entry) = entries.iter_mut().find(|e| e.request.trace_id == request.trace_id && e.request.attempt == request.attempt) {
            if matches!(event.phase, aillies_codex::telemetry::Phase::Received) {
                return;
            }
            event.status = event.status.or(entry.event.status);
            event.first_byte_ms = event.first_byte_ms.or(entry.event.first_byte_ms);
            entry.event = event;
        } else if matches!(event.phase, aillies_codex::telemetry::Phase::Received | aillies_codex::telemetry::Phase::Sending) {
            entries.push_back(TrafficEntry { at_unix_seconds: now(), request, event });
            while entries.len() > LIMIT {
                entries.pop_front();
            }
        }
        let path = self.path.with_file_name("traffic-diagnostics.json");
        self.persistence_ok &= serde_json::to_vec_pretty(&self.traffic).ok().is_some_and(|b| private_write(&path, &b).is_ok());
    }
}

/// Create a new private file without changing permissions of the user's Downloads folder.
pub(crate) fn export_to(dir: &Path, report: &Value) -> anyhow::Result<PathBuf> {
    let path = dir.join(format!("Aillies-diagnostics-{}-{}.json", now(), uuid::Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(report)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|_| anyhow::anyhow!("Cannot save diagnostics in Downloads. Copy request-diagnostics.json from the Aillies data folder."))?;
    if file.write_all(&bytes).and_then(|_| file.sync_all()).is_err() {
        let _ = std::fs::remove_file(&path);
        anyhow::bail!("Could not finish saving diagnostics. Check free disk space.");
    }
    Ok(path)
}

/// Observe only fixed failure envelopes and SSE terminal types. Bytes are forwarded unchanged.
/// This never executes a request or retries one.
pub(crate) async fn observe_response(node: Arc<Node>, response: Response<Body>, request: RequestContext) -> Response<Body> {
    incoming(&node, request, aillies_codex::telemetry::Phase::Received, None, 0);
    let started = request.received;
    node.record_traffic(
        request,
        false,
        aillies_codex::telemetry::UpstreamEvent {
            phase: aillies_codex::telemetry::Phase::HeadersReceived,
            status: Some(response.status().as_u16()),
            elapsed_ms: started.elapsed().as_millis() as u64,
            first_byte_ms: None,
            failure: None,
        },
    );
    let (parts, body) = response.into_parts();
    if parts.status != 200 {
        // Every caller supplies an Aillies-generated fixed failure envelope, never an upstream body.
        let bytes = match http_body_util::Limited::new(body, 4096).collect().await {
            Ok(b) => b.to_bytes(),
            Err(_) => {
                let f = Failure::new(FailureCode::DeliveryUncertain);
                node.record_diagnostic(request, &f);
                incoming(&node, request, aillies_codex::telemetry::Phase::DeliveryUncertain, Some(f.code), 0);
                return body::failure(&f);
            }
        };
        let f = Failure::from_json(&bytes).unwrap_or(Failure::new(FailureCode::DeliveryUncertain));
        node.record_diagnostic(request, &f);
        incoming(&node, request, aillies_codex::telemetry::Phase::Failed, Some(f.code), started.elapsed().as_millis() as u64);
        return Response::from_parts(parts, body::full(bytes));
    }
    let observer = StreamObserver { node, request, scanner: TerminalScanner::default(), recorded: false, started, first_byte: false };
    let stream = futures_util::stream::unfold((body, observer, false), |(mut body, mut observer, done)| async move {
        if done {
            return None;
        }
        loop {
            match body.frame().await {
                Some(Ok(frame)) => {
                    if let Ok(bytes) = frame.into_data() {
                        if !observer.first_byte {
                            observer.first_byte = true;
                            let elapsed = observer.started.elapsed().as_millis() as u64;
                            observer.node.record_traffic(
                                observer.request,
                                false,
                                aillies_codex::telemetry::UpstreamEvent {
                                    phase: aillies_codex::telemetry::Phase::Receiving,
                                    status: Some(200),
                                    elapsed_ms: elapsed,
                                    first_byte_ms: Some(elapsed),
                                    failure: None,
                                },
                            );
                        }
                        observer.scanner.feed(&bytes);
                        if observer.scanner.unsuccessful {
                            observer.fail(FailureCode::StreamFailed);
                        } else if observer.scanner.completed && !observer.recorded {
                            observer.recorded = true;
                            incoming(
                                &observer.node,
                                observer.request,
                                aillies_codex::telemetry::Phase::Completed,
                                None,
                                observer.started.elapsed().as_millis() as u64,
                            );
                        }
                        return Some((Ok(bytes), (body, observer, false)));
                    }
                }
                Some(Err(e)) => {
                    observer.fail(FailureCode::DeliveryUncertain);
                    return Some((Err(e), (body, observer, true)));
                }
                None => {
                    if !observer.scanner.completed {
                        observer.fail(FailureCode::DeliveryUncertain);
                    }
                    return None;
                }
            }
        }
    });
    Response::from_parts(parts, body::stream(stream))
}

struct StreamObserver {
    node: Arc<Node>,
    request: RequestContext,
    scanner: TerminalScanner,
    recorded: bool,
    started: std::time::Instant,
    first_byte: bool,
}
impl StreamObserver {
    fn fail(&mut self, code: FailureCode) {
        if !self.recorded {
            self.node.record_diagnostic(self.request, &Failure::new(code));
            let phase = if code == FailureCode::DeliveryUncertain {
                aillies_codex::telemetry::Phase::DeliveryUncertain
            } else {
                aillies_codex::telemetry::Phase::Failed
            };
            incoming(&self.node, self.request, phase, Some(code), self.started.elapsed().as_millis() as u64);
            self.recorded = true;
        }
    }
}

pub(crate) fn incoming(node: &Node, request: RequestContext, phase: aillies_codex::telemetry::Phase, failure: Option<FailureCode>, elapsed_ms: u64) {
    node.record_traffic(request, false, aillies_codex::telemetry::UpstreamEvent { phase, status: None, elapsed_ms, first_byte_ms: None, failure });
}

/// Covers cancellation while connecting or waiting for headers, before a response stream exists.
pub(crate) struct PendingIncoming {
    node: Arc<Node>,
    request: RequestContext,
    armed: bool,
}
impl PendingIncoming {
    pub(crate) fn new(node: Arc<Node>, request: RequestContext) -> Self {
        incoming(&node, request, aillies_codex::telemetry::Phase::Received, None, 0);
        Self { node, request, armed: true }
    }
    pub(crate) fn handoff(mut self) {
        self.armed = false;
    }
}
impl Drop for PendingIncoming {
    fn drop(&mut self) {
        if self.armed {
            let failure = Failure::new(FailureCode::DeliveryUncertain);
            self.node.record_diagnostic(self.request, &failure);
            incoming(
                &self.node,
                self.request,
                aillies_codex::telemetry::Phase::DeliveryUncertain,
                Some(failure.code),
                self.request.received.elapsed().as_millis() as u64,
            );
        }
    }
}
impl Drop for StreamObserver {
    fn drop(&mut self) {
        if !self.scanner.completed {
            self.fail(FailureCode::DeliveryUncertain);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use aillies_core::{Failure, FailureCode};

    #[test]
    fn latest_four_notices_are_fixed_and_retained_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = RequestLog::load(dir.path());
        let context = RequestContext::from_raw(Route::Peer, br#"{"model":"SECRET_MODEL","input":"SECRET_PROMPT"}"#);
        for index in 0..7 {
            log.record_at(context, &Failure::upstream(400 + index), u64::from(index));
        }
        let warnings = RequestLog::load(dir.path()).recent_warnings();
        assert_eq!(warnings.len(), 4);
        assert_eq!(warnings[0].at, 6);
        assert_eq!(warnings[3].at, 3);
        assert!(!serde_json::to_string(&warnings).unwrap().contains("SECRET"));
        log.record_retry(context, &Failure::upstream(503), 1);
        assert_eq!(RequestLog::load(dir.path()).recent_warnings()[0].retry, Some(1));
        assert_eq!(log.report()["entries"][7]["level"], "warning");
    }

    #[test]
    fn traffic_journals_keep_100_requests_not_100_events_and_reload_safely() {
        use aillies_codex::telemetry::{Phase, UpstreamEvent};
        let dir = tempfile::tempdir().unwrap();
        let mut log = RequestLog::load(dir.path());
        let event = |phase| UpstreamEvent { phase, status: Some(200), elapsed_ms: 4, first_byte_ms: Some(1), failure: None };
        let mut first = None;
        for _ in 0..105 {
            let context = RequestContext::from_raw(Route::Local, br#"{"model":"gpt-5.6-sol","input":"SECRET_PROMPT"}"#);
            first.get_or_insert(context);
            log.traffic(context, false, event(Phase::Received));
            log.traffic(context, true, event(Phase::Sending));
            log.traffic(context, true, event(Phase::HeadersReceived));
            log.traffic(context, true, event(Phase::Completed));
            log.traffic(context, false, event(Phase::Completed));
        }
        // Finishing an evicted request must not reinsert it over a newer one.
        log.traffic(first.unwrap(), true, event(Phase::Completed));
        let report = RequestLog::load(dir.path()).report();
        assert_eq!(report["incoming"].as_array().unwrap().len(), 100);
        assert_eq!(report["upstream"].as_array().unwrap().len(), 100);
        assert_eq!(report["upstream"][99]["event"]["phase"], "completed");
        assert_eq!(report["incoming"][99]["request"]["trace_id"], report["upstream"][99]["request"]["trace_id"]);
        assert!(!report.to_string().contains("SECRET"));
        assert_eq!(report["entries"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn rejection_and_embedded_tool_counts_are_content_free() {
        let context = RequestContext::from_raw(
            Route::Peer,
            br#"{"model":"gpt-5.6-sol","input":[{"type":"additional_tools","tools":[{"type":"mcp","server_url":"SECRET"}]}]}"#,
        );
        let value = serde_json::to_value(context).unwrap();
        assert_eq!(value["tools"], 1);
        assert_eq!(value["rejection"], "unsupported_tool");
        assert!(!value.to_string().contains("SECRET"));
    }

    #[test]
    fn keeps_latest_100_across_restart_and_does_not_clear_on_export() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = RequestLog::load(dir.path());
        for i in 0..105 {
            let context = RequestContext::from_raw(Route::Local, br#"{"model":"gpt-6.1-sol","reasoning":{"effort":"low"}}"#);
            log.record_at(context, &Failure::upstream(400), i);
        }
        let log = RequestLog::load(dir.path());
        let report = log.report();
        assert_eq!(report["entries"].as_array().unwrap().len(), 100);
        assert_eq!(report["entries"][0]["at_unix_seconds"], 5);
        assert_eq!(report["entries"][99]["at_unix_seconds"], 104);
        assert_eq!(report["entries"][99]["request"]["model"], "gpt-6.1-sol");
        assert_eq!(report["entries"][99]["upstream_status"], 400);
        assert_eq!(log.report(), report);
    }

    #[test]
    fn arbitrary_strings_cannot_enter_persisted_or_exported_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = RequestLog::load(dir.path());
        let raw = br#"{"model":"SECRET_MODEL","input":"SECRET_PROMPT","instructions":"SECRET_INSTRUCTIONS","reasoning":{"effort":"SECRET_EFFORT"},"tools":[{"type":"SECRET_TOOL","description":"SECRET_DESCRIPTION"}],"client_metadata":{"token":"SECRET_TOKEN"}}"#;
        log.record_at(RequestContext::from_raw(Route::Peer, raw), &Failure::new(FailureCode::RequestInvalid), 1);
        let disk = std::fs::read_to_string(dir.path().join("request-diagnostics.json")).unwrap();
        assert!(!disk.contains("SECRET"));
        assert!(!log.report().to_string().contains("SECRET"));
        assert_eq!(log.report()["entries"][0]["request"]["model"], "other");
        // A tampered file cannot smuggle strings or unknown properties into an export.
        let poisoned = disk.replace("\"model\": \"other\"", "\"model\": \"SECRET_MODEL\"");
        std::fs::write(dir.path().join("request-diagnostics.json"), poisoned).unwrap();
        assert!(RequestLog::load(dir.path()).report()["entries"].as_array().unwrap().is_empty());
    }

    #[test]
    fn export_creates_distinct_complete_private_files() {
        let dir = tempfile::tempdir().unwrap();
        let log = RequestLog::load(dir.path());
        let first = export_to(dir.path(), &log.report()).unwrap();
        let second = export_to(dir.path(), &log.report()).unwrap();
        assert_ne!(first, second);
        assert_eq!(serde_json::from_slice::<Value>(&std::fs::read(first.clone()).unwrap()).unwrap(), log.report());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(first).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn persistence_failure_keeps_memory_and_is_visible_without_raw_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = RequestLog::load(dir.path());
        std::fs::create_dir(dir.path().join("request-diagnostics.json")).unwrap();
        log.record_at(RequestContext::from_raw(Route::Local, b"{}"), &Failure::upstream(400), 1);
        assert_eq!(log.report()["persistence_ok"], false);
        assert_eq!(log.report()["entries"].as_array().unwrap().len(), 1);
    }
}
