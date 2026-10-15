//! Offline provider migration. Transcript bytes after session metadata are copied unchanged.
//! A private crash journal coordinates SQLite, rollout files and the config receipt.
use crate::config::{CodexConfig, PROVIDER_ID};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const JOURNAL: &str = "provider-switch.json";
// Preparation runs on a background worker; elapsed time alone is not a failure.
const BUDGET: Option<Duration> = None;
type Result<T> = std::result::Result<T, SwitchError>;
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SwitchError(&'static str);
#[cfg(target_os = "macos")]
impl SwitchError {
    pub(crate) fn fixed_message(&self) -> &'static str {
        self.0
    }
}
const INVALID: SwitchError = SwitchError("Codex history could not be updated safely. Your settings and history are kept.");

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SwitchPhase {
    Scanning,
    Preparing,
    Applying,
    Settings,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwitchProgress {
    pub phase: SwitchPhase,
    pub completed: usize,
    pub total: usize,
}

#[derive(Serialize)]
pub struct SwitchReport {
    pub chats: usize,
    pub skipped: usize,
    pub history_pending: bool,
    pub elapsed_ms: u64,
    pub reopen_required: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    id: String,
    path: PathBuf,
    backup: PathBuf,
    staged: PathBuf,
    source: String,
    target: String,
    #[serde(default)]
    history: Option<HistoryOffsets>,
    #[serde(default = "indexed_default")]
    indexed: bool,
}
fn indexed_default() -> bool {
    true
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryOffsets {
    delta: i64,
    #[serde(default)]
    delimiter_at: Option<i64>,
    turns: Vec<(String, Option<i64>, Option<i64>)>,
    projection: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryIssueCode {
    UnsupportedSchema,
    MissingFile,
    UnsafePath,
    DamagedHeader,
    MetadataMismatch,
    DamagedRecord,
    IncompleteRecord,
    RecordTooLarge,
    InvalidOrdinal,
    InvalidSettings,
    InvalidOffsets,
    IoFailure,
    AmbiguousIndex,
    SettingsFailure,
    RecoveryNeeded,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryStage {
    Database,
    Scan,
    Prepare,
    Commit,
    Recovery,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryIssue {
    pub code: HistoryIssueCode,
    pub stage: HistoryStage,
    pub chat_ref: String,
    pub record: Option<u64>,
    pub at_unix_seconds: u64,
}
fn issue(issues: &mut Vec<HistoryIssue>, path: &Path, code: HistoryIssueCode, stage: HistoryStage, record: Option<u64>) {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(path.as_os_str().as_encoded_bytes());
    let chat_ref = digest.iter().map(|b| format!("{b:02x}")).collect();
    issues.push(HistoryIssue {
        code,
        stage,
        chat_ref,
        record,
        at_unix_seconds: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
    });
}
pub fn migration_diagnostics(config: &CodexConfig) -> Vec<HistoryIssue> {
    let Ok(Some(bytes)) = read_optional(&config.receipt_dir.join("history-diagnostics.json")) else { return vec![] };
    if bytes.len() > 256 * 1024 {
        return vec![];
    }
    let Ok(entries) = serde_json::from_slice::<Vec<HistoryIssue>>(&bytes) else { return vec![] };
    entries
        .into_iter()
        .filter(|e| e.chat_ref.len() == 64 && e.chat_ref.bytes().all(|b| b.is_ascii_hexdigit()) && e.record.is_none_or(|n| n <= 100_000_000))
        .rev()
        .take(100)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
fn save_issues(config: &CodexConfig, issues: &[HistoryIssue]) -> Result<()> {
    if issues.is_empty() {
        return Ok(());
    }
    let mut all = migration_diagnostics(config);
    all.extend_from_slice(issues);
    if all.len() > 100 {
        all.drain(..all.len() - 100);
    }
    let path = config.receipt_dir.join("history-diagnostics.json");
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    private_write(&temp, &serde_json::to_vec(&all).map_err(|_| INVALID)?)?;
    fs::rename(&temp, path).map_err(|_| INVALID)
}
fn database_name(name: &str, prefix: &str) -> Option<u32> {
    let digits = name.strip_prefix(prefix)?.strip_suffix(".sqlite")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}
fn selected_database(home: &Path, prefix: &str) -> Result<Option<String>> {
    if !home.exists() {
        return Ok(None);
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(home).map_err(|_| INVALID)? {
        let entry = entry.map_err(|_| INVALID)?;
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(n) = database_name(&name, prefix) {
            candidates.push((n, name));
        }
    }
    candidates.sort();
    Ok(candidates.pop().map(|(_, name)| name))
}
fn check_named_database(home: &Path, name: &str, prefix: &str) -> Result<()> {
    if database_name(name, prefix).is_none() {
        return Err(INVALID);
    }
    for path in [home.join(name), home.join(format!("{name}-wal")), home.join(format!("{name}-shm"))] {
        if fs::symlink_metadata(path).is_ok_and(|m| !m.file_type().is_file()) {
            return Err(INVALID);
        }
    }
    Ok(())
}
fn has_columns(db: &Connection, alias: &str, table: &str, columns: &[&str]) -> Result<bool> {
    let mut stmt = db.prepare(&format!("PRAGMA {alias}.table_info({table})")).map_err(|_| INVALID)?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1)).map_err(|_| INVALID)?.collect::<std::result::Result<Vec<_>, _>>().map_err(|_| INVALID)?;
    Ok(columns.iter().all(|c| names.iter().any(|n| n == c)))
}
fn issue_code(error: &SwitchError) -> HistoryIssueCode {
    match error.0 {
        "A chat contains a damaged record." => HistoryIssueCode::DamagedRecord,
        "A chat contains an incomplete record." => HistoryIssueCode::IncompleteRecord,
        "A chat record exceeds the migration limit." => HistoryIssueCode::RecordTooLarge,
        "A chat contains an unsupported ordinal." => HistoryIssueCode::InvalidOrdinal,
        "A chat contains unsupported provider settings." => HistoryIssueCode::InvalidSettings,
        "A chat history schema could not preserve offsets." => HistoryIssueCode::UnsupportedSchema,
        "Provider settings could not be updated safely." => HistoryIssueCode::SettingsFailure,
        "History recovery could not be completed. Keep Codex closed." => HistoryIssueCode::RecoveryNeeded,
        "A chat metadata does not match its index." => HistoryIssueCode::MetadataMismatch,
        "A chat's history offsets could not be preserved." => HistoryIssueCode::InvalidOffsets,
        _ => HistoryIssueCode::IoFailure,
    }
}

fn attach_history(db: &Connection, home: &Path, name: &str) -> Result<bool> {
    check_named_database(home, name, "thread_history_")?;
    let path = home.join(name);
    if !path.exists() {
        return Ok(false);
    }
    db.execute("ATTACH DATABASE ?1 AS history", [path.to_str().ok_or(INVALID)?]).map_err(|_| INVALID)?;
    Ok(true)
}

fn snapshot_offsets(db: &Connection, id: &str, delta: i64) -> Result<HistoryOffsets> {
    let mut query =
        db.prepare("SELECT turn_id,rollout_byte_offset,rollout_end_byte_offset FROM history.thread_turns WHERE thread_id=?1").map_err(|_| INVALID)?;
    let turns: Vec<(String, Option<i64>, Option<i64>)> =
        query.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(|_| INVALID)?.collect::<std::result::Result<_, _>>().map_err(|_| INVALID)?;
    use rusqlite::OptionalExtension;
    let projection: Option<i64> = db
        .query_row("SELECT next_rollout_byte_offset FROM history.thread_history_projection_state WHERE thread_id=?1", [id], |r| r.get(0))
        .optional()
        .map_err(|_| INVALID)?;
    for value in turns.iter().flat_map(|(_, a, b)| [*a, *b]).chain(std::iter::once(projection)).flatten() {
        if value < 0 || value > 0 && value.checked_add(delta).is_none_or(|v| v < 0) {
            return Err(SwitchError("A chat's history offsets could not be preserved."));
        }
    }
    Ok(HistoryOffsets { delta, delimiter_at: None, turns, projection })
}

fn write_offsets(db: &Connection, change: &Change, committed: bool) -> Result<()> {
    let Some(offsets) = &change.history else {
        return Ok(());
    };
    let shift = |offset: Option<i64>| -> Result<Option<i64>> {
        offset
            .map(|v| {
                if v < 0 {
                    return Err(INVALID);
                }
                if committed && v > 0 {
                    v.checked_add(offsets.delta).and_then(|n| n.checked_add(i64::from(offsets.delimiter_at == Some(v)))).filter(|v| *v >= 0).ok_or(INVALID)
                } else {
                    Ok(v)
                }
            })
            .transpose()
    };
    for (turn, start, end) in &offsets.turns {
        if db
            .execute(
                "UPDATE history.thread_turns SET rollout_byte_offset=?1,rollout_end_byte_offset=?2 WHERE thread_id=?3 AND turn_id=?4",
                params![shift(*start)?, shift(*end)?, change.id, turn],
            )
            .map_err(|_| INVALID)?
            != 1
        {
            return Err(INVALID);
        }
    }
    if let Some(value) = offsets.projection {
        if db
            .execute(
                "UPDATE history.thread_history_projection_state SET next_rollout_byte_offset=?1 WHERE thread_id=?2",
                params![shift(Some(value))?, change.id],
            )
            .map_err(|_| INVALID)?
            != 1
        {
            return Err(INVALID);
        }
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    changes: Vec<Change>,
    config_before: Option<String>,
    receipt_before: Option<String>,
    on: bool,
    #[serde(default)]
    completed: bool,
    #[serde(default)]
    state_db: Option<String>,
    #[serde(default)]
    history_db: Option<String>,
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() && meta.len() <= 16 * 1024 * 1024 => fs::read(path).map(Some).map_err(|_| INVALID),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        _ => Err(INVALID),
    }
}
fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    write_new(path, bytes, true)
}
fn write_new(path: &Path, bytes: &[u8], sync: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| INVALID)?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| INVALID)?;
    let result = file.write_all(bytes).and_then(|()| if sync { file.sync_all() } else { Ok(()) });
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path);
        return Err(INVALID);
    }
    Ok(())
}
fn restore_file(path: &Path, encoded: &Option<String>) -> Result<()> {
    if let Some(encoded) = encoded {
        let bytes = STANDARD.decode(encoded).map_err(|_| INVALID)?;
        let temp = path.with_extension(format!("restore-{}", uuid::Uuid::new_v4()));
        private_write(&temp, &bytes)?;
        if path.exists() {
            fs::remove_file(path).map_err(|_| INVALID)?;
        }
        fs::rename(temp, path).map_err(|_| INVALID)?;
    } else if path.exists() {
        fs::remove_file(path).map_err(|_| INVALID)?;
    }
    Ok(())
}
fn previous_provider(config: &CodexConfig) -> Result<String> {
    let bytes = if config.is_installed() {
        let receipt: Value = serde_json::from_slice(&fs::read(config.receipt_path()).map_err(|_| INVALID)?).map_err(|_| INVALID)?;
        match receipt["original"].as_str() {
            Some(v) => STANDARD.decode(v).map_err(|_| INVALID)?,
            None => Vec::new(),
        }
    } else {
        read_optional(&config.config_path())?.unwrap_or_default()
    };
    let table: toml::Table = toml::from_str(std::str::from_utf8(&bytes).map_err(|_| INVALID)?).map_err(|_| INVALID)?;
    Ok(table.get("model_provider").and_then(toml::Value::as_str).unwrap_or("openai").to_owned())
}

/// Do not edit history underneath a running Codex process, including the desktop app.
/// Read-only bounded estimate for review. The migration revalidates after closure.
/// Never exports transcript records, identifiers, paths or database values.
pub fn preview_chat_count(config: &CodexConfig, on: bool) -> Option<usize> {
    let count = || -> Result<usize> {
        let start = Instant::now();
        let source = if on { previous_provider(config)? } else { PROVIDER_ID.into() };
        let mut rows = Vec::new();
        let mut index = RolloutIndex::default();
        let name = selected_database(&config.codex_home, "state_")?;
        if let Some(name) = name {
            let path = config.codex_home.join(&name);
            check_named_database(&config.codex_home, &name, "state_")?;
            let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|_| INVALID)?;
            db.busy_timeout(Duration::from_millis(50)).map_err(|_| INVALID)?;
            let mut query = db.prepare("SELECT id,rollout_path,model_provider FROM threads LIMIT 20001").map_err(|_| INVALID)?;
            let entries = query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).map_err(|_| INVALID)?;
            for (i, entry) in entries.enumerate() {
                if i >= 20000 || start.elapsed() > Duration::from_secs(1) {
                    return Err(INVALID);
                }
                let (id, path, provider) = entry.map_err(|_| INVALID)?;
                index.ids.insert(id.clone());
                if let Ok(path) = PathBuf::from(path).canonicalize() {
                    index.paths.insert(path);
                }
                index.providers.insert(id.clone(), provider.clone());
                if provider == source {
                    rows.push((id, PathBuf::new(), true));
                }
            }
        }
        collect_unindexed(&config.codex_home.join("sessions"), &mut index, &source, &mut rows, start, Some(Duration::from_secs(1)))?;
        collect_unindexed(&config.codex_home.join("archived_sessions"), &mut index, &source, &mut rows, start, Some(Duration::from_secs(1)))?;
        Ok(rows.len())
    };
    count().ok()
}

pub fn codex_running() -> Result<bool> {
    #[cfg(windows)]
    let output = std::process::Command::new("tasklist").args(["/FO", "CSV", "/NH"]).output();
    #[cfg(not(windows))]
    let output = std::process::Command::new("ps").args(["-A", "-o", "comm="]).output();
    let output = output.map_err(|_| SwitchError("Cannot check whether Codex is closed. No settings were changed."))?;
    if !output.status.success() {
        return Err(INVALID);
    }
    Ok(String::from_utf8_lossy(&output.stdout).lines().any(|line| {
        #[cfg(windows)]
        let line = line.split(',').next().unwrap_or("").trim_matches('"');
        let name = Path::new(line.trim()).file_name().and_then(|s| s.to_str()).unwrap_or("");
        matches!(name, "Codex" | "codex" | "codex.exe" | "Codex.exe")
    }))
}

fn valid_path(home: &Path, path: &Path) -> bool {
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return false;
    }
    let Ok(root) = home.canonicalize() else {
        return false;
    };
    // Use the existing parent even if a crash temporarily left the original missing.
    let Some(parent) = path.parent().and_then(|p| p.canonicalize().ok()) else {
        return false;
    };
    (parent.starts_with(root.join("sessions")) || parent.starts_with(root.join("archived_sessions")))
        && fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file())
}

fn rollback(config: &CodexConfig, journal: &Journal) -> Result<()> {
    for change in journal.changes.iter().rev() {
        if change.backup.exists() {
            if change.path.exists() {
                fs::remove_file(&change.path).map_err(|_| INVALID)?;
            }
            fs::rename(&change.backup, &change.path).map_err(|_| INVALID)?;
        }
        if change.staged.exists() {
            fs::remove_file(&change.staged).map_err(|_| INVALID)?;
        }
    }
    restore_file(&config.config_path(), &journal.config_before)?;
    restore_file(&config.receipt_path(), &journal.receipt_before)
}
fn cleanup(config: &CodexConfig, journal: &Journal) -> Result<()> {
    for change in &journal.changes {
        for path in [&change.backup, &change.staged] {
            if path.exists() {
                fs::remove_file(path).map_err(|_| INVALID)?;
            }
        }
    }
    fs::remove_file(config.receipt_dir.join(JOURNAL)).map_err(|_| INVALID)
}

/// Failed restart flows must not reopen writers while recovery is still pending.
#[cfg(target_os = "macos")]
pub(crate) fn recovery_pending(config: &CodexConfig) -> bool {
    !matches!(config.receipt_dir.join(JOURNAL).symlink_metadata(), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
}

/// Recovery is deliberately explicit and only runs after Codex is closed.
fn recover(config: &CodexConfig) -> Result<()> {
    let Some(bytes) = read_optional(&config.receipt_dir.join(JOURNAL))? else {
        return Ok(());
    };
    let journal: Journal = serde_json::from_slice(&bytes).map_err(|_| INVALID)?;
    for c in &journal.changes {
        if !valid_path(&config.codex_home, &c.path) && !valid_path(&config.codex_home, &c.backup) {
            return Err(INVALID);
        }
        if c.backup.parent() != c.path.parent()
            || c.staged.parent() != c.path.parent()
            || !c.backup.file_name().is_some_and(|n| n.to_string_lossy().starts_with(".aillies-old-"))
            || !c.staged.file_name().is_some_and(|n| n.to_string_lossy().starts_with(".aillies-new-"))
        {
            return Err(INVALID);
        }
        for path in [&c.path, &c.backup, &c.staged] {
            if fs::symlink_metadata(path).is_ok_and(|m| !m.file_type().is_file()) {
                return Err(INVALID);
            }
        }
    }
    let mut recovery_db = if !journal.changes.iter().any(|c| c.indexed || c.history.is_some()) {
        None
    } else {
        let name = journal.state_db.as_deref().unwrap_or("state_5.sqlite");
        check_named_database(&config.codex_home, name, "state_")?;
        let db = Connection::open(config.codex_home.join(name)).map_err(|_| INVALID)?;
        if journal.changes.iter().any(|c| c.history.is_some())
            && !attach_history(&db, &config.codex_home, journal.history_db.as_deref().unwrap_or("thread_history_1.sqlite"))?
        {
            return Err(INVALID);
        }
        Some(db)
    };
    let recovery_tx = recovery_db.as_mut().map(|db| db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)).transpose().map_err(|_| INVALID)?;
    let committed = if !journal.changes.iter().any(|c| c.indexed) {
        // File-only migrations have no provider-index commit marker.
        journal.completed
    } else {
        let db = recovery_tx.as_ref().ok_or(INVALID)?;
        let mut target = 0;
        let mut source = 0;
        for c in journal.changes.iter().filter(|c| c.indexed) {
            let current: String = db.query_row("SELECT model_provider FROM threads WHERE id=?1", [&c.id], |r| r.get(0)).map_err(|_| INVALID)?;
            if current == c.target {
                target += 1;
            } else if current == c.source {
                source += 1;
            } else {
                return Err(INVALID);
            }
        }
        if source > 0 && target > 0 {
            return Err(INVALID);
        }
        target > 0
    };
    if !committed {
        rollback(config, &journal)?;
    }
    if let Some(tx) = recovery_tx {
        // WAL databases can commit separately after a crash. Restore absolute offsets
        // using the provider index as the commit marker; never apply a delta twice.
        for change in &journal.changes {
            write_offsets(&tx, change, committed)?;
        }
        tx.commit().map_err(|_| INVALID)?;
    }
    cleanup(config, &journal)
}

pub fn switch_provider_with_progress(config: &CodexConfig, on: bool, port: u16, key: &str, progress: &dyn Fn(SwitchProgress)) -> Result<SwitchReport> {
    if codex_running()? {
        return Err(SwitchError("Quit Codex, then switch providers here. Your chats will be preserved."));
    }
    let phase = std::cell::Cell::new(HistoryStage::Recovery);
    let result = switch_closed(config, on, port, key, BUDGET, true, &|p| {
        phase.set(match p.phase {
            SwitchPhase::Scanning => HistoryStage::Database,
            SwitchPhase::Preparing => HistoryStage::Prepare,
            SwitchPhase::Applying | SwitchPhase::Settings => HistoryStage::Commit,
        });
        progress(p);
    });
    if let Err(error) = &result {
        let mut issues = Vec::new();
        issue(&mut issues, &config.codex_home, issue_code(error), phase.get(), None);
        let _ = save_issues(config, &issues);
    }
    result
}

fn read_header(reader: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut header = Vec::new();
    reader.take(1024 * 1024 + 1).read_until(b'\n', &mut header).map_err(|_| INVALID)?;
    if header.len() > 1024 * 1024 || !header.ends_with(b"\n") {
        return Err(INVALID);
    }
    Ok(header)
}

/// Codex also replays provider choices from thread_settings_applied events when
/// rebuilding an index. Preserve every original record and append an updated
/// settings event, copying all permissions and other settings without alteration.
#[allow(clippy::too_many_arguments)]
fn copy_history_with_line(
    reader: &mut impl BufRead,
    output: &mut impl Write,
    id: &str,
    target: &str,
    mut ordinal: Option<u64>,
    start: Instant,
    budget: Option<Duration>,
    line_number: &mut u64,
) -> Result<bool> {
    let mut latest = None;
    let mut boundary_added = false;
    let mut line = Vec::new();
    let mut tail_newline = true;
    loop {
        if budget.is_some_and(|limit| start.elapsed() > limit) {
            return Err(SwitchError("History preparation stopped. Your settings and history were not changed."));
        }
        line.clear();
        let n = reader.take(16 * 1024 * 1024 + 1).read_until(b'\n', &mut line).map_err(|_| INVALID)?;
        if n == 0 {
            break;
        }
        *line_number += 1;
        if n > 16 * 1024 * 1024 {
            return Err(SwitchError("A chat record exceeds the migration limit."));
        }
        tail_newline = line.ends_with(b"\n");
        if line.iter().all(u8::is_ascii_whitespace) {
            output.write_all(&line).map_err(|_| INVALID)?;
            continue;
        }
        let value: Value = serde_json::from_slice(&line).map_err(|_| SwitchError("A chat contains a damaged record."))?;
        if let Some(n) = value.get("ordinal").filter(|n| !n.is_null()) {
            let n = n.as_u64().ok_or(SwitchError("A chat contains an unsupported ordinal."))?;
            ordinal = Some(ordinal.map_or(n, |old| old.max(n)));
        }
        if value["type"] == "event_msg" && value["payload"]["type"] == "thread_settings_applied" {
            if value["payload"].get("thread_id").is_some_and(|thread| !thread.is_null() && thread != id)
                || !value["payload"]["thread_settings"].is_object()
                || value["payload"]["thread_settings"].get("model_provider_id").is_some_and(|p| !p.is_null() && !p.is_string())
            {
                return Err(SwitchError("A chat contains unsupported provider settings."));
            }
            latest = Some(value);
        }
        output.write_all(&line).map_err(|_| INVALID)?;
    }
    if let Some(mut event) = latest {
        if event["payload"]["thread_settings"]["model_provider_id"] != target {
            event["payload"]["thread_settings"]["model_provider_id"] = Value::String(target.to_owned());
            if let Some(ordinal) = ordinal {
                event["ordinal"] = Value::from(ordinal.checked_add(1).ok_or(SwitchError("A chat contains an unsupported ordinal."))?);
            }
            if !tail_newline {
                boundary_added = true;
                output.write_all(b"\n").map_err(|_| INVALID)?;
            }
            serde_json::to_writer(&mut *output, &event).map_err(|_| INVALID)?;
            output.write_all(b"\n").map_err(|_| INVALID)?;
        }
    }
    Ok(boundary_added)
}

#[cfg(test)]
fn copy_history(
    reader: &mut impl BufRead,
    output: &mut impl Write,
    id: &str,
    target: &str,
    ordinal: Option<u64>,
    start: Instant,
    budget: Option<Duration>,
) -> Result<()> {
    copy_history_with_line(reader, output, id, target, ordinal, start, budget, &mut 0).map(|_| ())
}

type Rollout = (String, PathBuf, bool);
#[derive(Default)]
struct RolloutIndex {
    paths: std::collections::HashSet<PathBuf>,
    ids: std::collections::HashSet<String>,
    providers: std::collections::HashMap<String, String>,
}

fn collect_unindexed(dir: &Path, index: &mut RolloutIndex, source: &str, rows: &mut Vec<Rollout>, start: Instant, budget: Option<Duration>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    if !fs::symlink_metadata(dir).map_err(|_| INVALID)?.is_dir() {
        return Err(INVALID);
    }
    for entry in fs::read_dir(dir).map_err(|_| INVALID)? {
        if budget.is_some_and(|limit| start.elapsed() > limit) {
            return Err(SwitchError("History preparation stopped. Your settings and history were not changed."));
        }
        let entry = entry.map_err(|_| INVALID)?;
        let kind = entry.file_type().map_err(|_| INVALID)?;
        if kind.is_symlink() {
            return Err(INVALID);
        }
        if kind.is_dir() {
            collect_unindexed(&entry.path(), index, source, rows, start, budget)?;
        } else if entry.path().extension().is_some_and(|e| e == "jsonl") {
            if !kind.is_file() {
                return Err(INVALID);
            }
            let path = entry.path().canonicalize().map_err(|_| INVALID)?;
            if index.paths.contains(&path) {
                continue;
            }
            let header = read_header(&mut BufReader::new(fs::File::open(&path).map_err(|_| INVALID)?))?;
            let value: Value = serde_json::from_slice(&header).map_err(|_| INVALID)?;
            let id = value["payload"]["id"].as_str().filter(|id| !id.is_empty() && id.len() <= 256).ok_or(INVALID)?;
            if value["type"] != "session_meta" {
                return Err(INVALID);
            }
            if !index.ids.insert(id.to_owned()) {
                // Codex's index chooses the canonical rollout. Other same-provider
                // copies are migrated as files, without touching its offsets again.
                if !index.providers.get(id).is_some_and(|provider| value["payload"]["model_provider"] == *provider) {
                    return Err(INVALID);
                }
            }
            // Leave other providers alone. No invented index rows or transcript events.
            if value["payload"]["model_provider"] == source {
                rows.push((id.to_owned(), path, false));
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect_unindexed_safe(
    dir: &Path,
    index: &mut RolloutIndex,
    source: &str,
    rows: &mut Vec<Rollout>,
    start: Instant,
    budget: Option<Duration>,
    issues: &mut Vec<HistoryIssue>,
    blocked: &mut std::collections::HashSet<String>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    if !fs::symlink_metadata(dir).map_err(|_| INVALID)?.is_dir() {
        return Err(INVALID);
    }
    for entry in fs::read_dir(dir).map_err(|_| INVALID)? {
        if budget.is_some_and(|limit| start.elapsed() > limit) {
            return Err(INVALID);
        }
        let entry = entry.map_err(|_| INVALID)?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|_| INVALID)?;
        if meta.is_dir() {
            collect_unindexed_safe(&path, index, source, rows, start, budget, issues, blocked)?;
            continue;
        }
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        if !meta.is_file() {
            issue(issues, &path, HistoryIssueCode::UnsafePath, HistoryStage::Scan, None);
            continue;
        }
        let canonical = path.canonicalize().map_err(|_| INVALID)?;
        if index.paths.contains(&canonical) {
            continue;
        }
        let parse = (|| -> Result<Value> {
            let mut reader = BufReader::new(fs::File::open(&path).map_err(|_| INVALID)?);
            let header = read_header(&mut reader)?;
            serde_json::from_slice(&header).map_err(|_| INVALID)
        })();
        let value = match parse {
            Ok(v) => v,
            Err(_) => {
                issue(issues, &path, HistoryIssueCode::DamagedHeader, HistoryStage::Scan, Some(1));
                continue;
            }
        };
        let Some(id) = value["payload"]["id"].as_str().filter(|s| !s.is_empty() && s.len() <= 256) else {
            issue(issues, &path, HistoryIssueCode::DamagedHeader, HistoryStage::Scan, Some(1));
            continue;
        };
        if value["type"] != "session_meta" {
            issue(issues, &path, HistoryIssueCode::DamagedHeader, HistoryStage::Scan, Some(1));
            continue;
        }
        let Some(provider) = value["payload"]["model_provider"].as_str() else {
            blocked.insert(id.to_owned());
            issue(issues, &path, HistoryIssueCode::MetadataMismatch, HistoryStage::Scan, Some(1));
            continue;
        };
        if !index.ids.insert(id.to_owned()) && index.providers.get(id).is_none_or(|p| p != provider) {
            blocked.insert(id.to_owned());
            issue(issues, &path, HistoryIssueCode::AmbiguousIndex, HistoryStage::Scan, Some(1));
            continue;
        }
        if provider == source {
            rows.push((id.to_owned(), path, false));
        }
    }
    Ok(())
}

fn switch_closed(
    config: &CodexConfig,
    on: bool,
    port: u16,
    key: &str,
    budget: Option<Duration>,
    check_process: bool,
    progress: &dyn Fn(SwitchProgress),
) -> Result<SwitchReport> {
    recover(config).map_err(|_| SwitchError("History recovery could not be completed. Keep Codex closed."))?;
    let start = Instant::now();
    progress(SwitchProgress { phase: SwitchPhase::Scanning, completed: 0, total: 0 });
    if !on && !config.is_installed() {
        return Ok(SwitchReport { chats: 0, skipped: 0, history_pending: false, elapsed_ms: 0, reopen_required: false });
    }
    let previous = previous_provider(config)?;
    let (source, target) = if on { (previous, PROVIDER_ID.to_owned()) } else { (PROVIDER_ID.to_owned(), previous) };
    let mut journal = Journal {
        state_db: None,
        history_db: None,
        on,
        completed: false,
        changes: Vec::new(),
        config_before: read_optional(&config.config_path())?.map(|b| STANDARD.encode(b)),
        receipt_before: read_optional(&config.receipt_path())?.map(|b| STANDARD.encode(b)),
    };
    let mut issues = Vec::new();
    journal.state_db = selected_database(&config.codex_home, "state_")?;
    journal.history_db = selected_database(&config.codex_home, "thread_history_")?;
    let dbpath = journal.state_db.as_ref().map(|name| config.codex_home.join(name));
    let mut index_unsupported = false;
    let mut db = if let Some(name) = &journal.state_db {
        check_named_database(&config.codex_home, name, "state_")?;
        match Connection::open_with_flags(config.codex_home.join(name), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(db) if has_columns(&db, "main", "threads", &["id", "rollout_path", "model_provider"]).unwrap_or(false) => {
                drop(db);
                Some(Connection::open(config.codex_home.join(name)).map_err(|_| INVALID)?)
            }
            _ => {
                index_unsupported = true;
                issue(&mut issues, dbpath.as_ref().ok_or(INVALID)?, HistoryIssueCode::UnsupportedSchema, HistoryStage::Database, None);
                None
            }
        }
    } else {
        None
    };
    let mut history_unsupported = db.is_none() && journal.history_db.is_some();
    let has_history = match (&db, &journal.history_db) {
        (Some(db), Some(name)) => {
            let attached = attach_history(db, &config.codex_home, name).is_ok();
            let supported = attached
                && has_columns(db, "history", "thread_turns", &["thread_id", "turn_id", "rollout_byte_offset", "rollout_end_byte_offset"]).unwrap_or(false)
                && has_columns(db, "history", "thread_history_projection_state", &["thread_id", "next_rollout_byte_offset"]).unwrap_or(false);
            history_unsupported = !supported;
            supported
        }
        _ => false,
    };
    let transaction = match db.as_mut() {
        Some(db) => {
            db.busy_timeout(Duration::from_millis(100)).map_err(|_| INVALID)?;
            Some(
                db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .map_err(|_| SwitchError("Codex history is busy. Close Codex and try again."))?,
            )
        }
        None => None,
    };
    let mut rows: Vec<Rollout> = Vec::new();
    let mut index = RolloutIndex::default();
    let mut blocked_ids = std::collections::HashSet::new();
    if let Some(tx) = &transaction {
        let mut statement = tx.prepare("SELECT id,rollout_path,model_provider FROM threads").map_err(|_| INVALID)?;
        let entries = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).map_err(|_| INVALID)?;
        for entry in entries {
            let (id, path, provider) = match entry {
                Ok(row) => row,
                Err(_) => {
                    index_unsupported = true;
                    issue(&mut issues, dbpath.as_ref().ok_or(INVALID)?, HistoryIssueCode::MetadataMismatch, HistoryStage::Database, None);
                    rows.clear();
                    break;
                }
            };
            let path = PathBuf::from(path);
            if !index.ids.insert(id.clone()) {
                blocked_ids.insert(id.clone());
                issue(&mut issues, &path, HistoryIssueCode::AmbiguousIndex, HistoryStage::Database, None);
                continue;
            }
            match path.canonicalize() {
                Ok(path) => {
                    if !index.paths.insert(path) {
                        return Err(INVALID);
                    }
                    index.providers.insert(id.clone(), provider.clone());
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && provider != source => {}
                Err(_) => {
                    blocked_ids.insert(id.clone());
                    issue(&mut issues, &path, HistoryIssueCode::MissingFile, HistoryStage::Scan, None);
                }
            }
            if provider == source && source != target {
                rows.push((id, path, true));
            }
        }
    }
    if !index_unsupported && source != target {
        collect_unindexed_safe(&config.codex_home.join("sessions"), &mut index, &source, &mut rows, start, budget, &mut issues, &mut blocked_ids)?;
        collect_unindexed_safe(&config.codex_home.join("archived_sessions"), &mut index, &source, &mut rows, start, budget, &mut issues, &mut blocked_ids)?;
    }
    let total = rows.len();
    progress(SwitchProgress { phase: SwitchPhase::Preparing, completed: 0, total });
    let mut groups = std::collections::BTreeMap::<String, Vec<Rollout>>::new();
    for row in rows {
        groups.entry(row.0.clone()).or_default().push(row);
    }
    let mut completed = 0;
    let mut skipped_groups = 0usize;
    for (id, group) in groups {
        let before = journal.changes.len();
        let group_len = group.len();
        let mut record = 1u64;
        let mut failed = None;
        for (_, path, indexed) in group {
            record = 1;
            if blocked_ids.contains(&id) {
                failed = Some((path, SwitchError("A chat file could not be read.")));
                break;
            }
            let result = (|| -> Result<()> {
                if !valid_path(&config.codex_home, &path) {
                    return Err(SwitchError("A chat path could not be updated safely."));
                }
                let file = fs::File::open(&path).map_err(|_| INVALID)?;
                let meta = file.metadata().map_err(|_| INVALID)?;
                let mut reader = BufReader::new(file);
                let header = read_header(&mut reader)?;
                let mut value: Value = serde_json::from_slice(&header).map_err(|_| SwitchError("A chat contains a damaged header."))?;
                if value["type"] != "session_meta" || value["payload"]["id"] != id || value["payload"]["model_provider"] != source {
                    return Err(SwitchError("A chat metadata does not match its index."));
                }
                if value["payload"]
                    .get("history_mode")
                    .is_some_and(|m| !m.is_null() && (!m.is_string() || m.as_str().is_some_and(|mode| mode != "legacy" && !has_history)))
                    || history_unsupported
                {
                    return Err(SwitchError("A chat history schema could not preserve offsets."));
                }
                if value.get("ordinal").is_some_and(|n| !n.is_null() && n.as_u64().is_none()) {
                    return Err(SwitchError("A chat contains an unsupported ordinal."));
                }
                let old_header_len = header.len() as i64;
                value["payload"]["model_provider"] = Value::String(target.clone());
                let mut header = serde_json::to_vec(&value).map_err(|_| INVALID)?;
                header.push(b'\n');
                let token = uuid::Uuid::new_v4();
                let parent = path.parent().ok_or(INVALID)?;
                let change = Change {
                    id: id.clone(),
                    indexed,
                    path: path.clone(),
                    backup: parent.join(format!(".aillies-old-{token}")),
                    staged: parent.join(format!(".aillies-new-{token}")),
                    source: source.clone(),
                    target: target.clone(),
                    history: if has_history && (indexed || !index.providers.contains_key(&id)) {
                        Some(
                            snapshot_offsets(
                                transaction.as_ref().ok_or(INVALID)?,
                                value["payload"]["id"].as_str().ok_or(INVALID)?,
                                header.len() as i64 - old_header_len,
                            )
                            .map_err(|_| SwitchError("A chat's history offsets could not be preserved."))?,
                        )
                    } else {
                        None
                    },
                };
                journal.changes.push(change);
                let change = journal.changes.last_mut().ok_or(INVALID)?;
                write_new(&change.staged, &header, false)?;
                let mut output = fs::OpenOptions::new().append(true).open(&change.staged).map_err(|_| INVALID)?;
                let boundary_added =
                    copy_history_with_line(&mut reader, &mut output, &change.id, &target, value["ordinal"].as_u64(), start, budget, &mut record)?;
                if let Some(offsets) = &mut change.history {
                    let old_len = i64::try_from(meta.len()).map_err(|_| INVALID)?;
                    if offsets
                        .turns
                        .iter()
                        .flat_map(|(_, a, b)| [*a, *b])
                        .chain(std::iter::once(offsets.projection))
                        .flatten()
                        .any(|v| v > 0 && (v < old_header_len || v > old_len))
                    {
                        return Err(SwitchError("A chat's history offsets could not be preserved."));
                    }
                    if boundary_added {
                        offsets.delimiter_at = Some(old_len);
                    }
                }
                output.sync_all().map_err(|_| INVALID)?;
                if let Ok(modified) = meta.modified() {
                    output.set_times(fs::FileTimes::new().set_modified(modified)).map_err(|_| INVALID)?;
                }

                Ok(())
            })();
            if let Err(error) = result {
                failed = Some((path, error));
                break;
            }
        }
        if let Some((path, error)) = failed {
            skipped_groups += 1;
            for change in journal.changes.drain(before..) {
                let _ = fs::remove_file(change.staged);
            }
            let code = match error.0 {
                "A chat contains a damaged header." => HistoryIssueCode::DamagedHeader,
                "A chat path could not be updated safely." => HistoryIssueCode::UnsafePath,
                "A chat file could not be read." => HistoryIssueCode::MissingFile,
                _ => issue_code(&error),
            };
            issue(&mut issues, &path, code, HistoryStage::Prepare, Some(record));
        }
        completed += group_len;
        progress(SwitchProgress { phase: SwitchPhase::Preparing, completed, total });
    }
    // Diagnostics must be durable before allowing a partially completed migration.
    if save_issues(config, &issues).is_err() {
        for change in &journal.changes {
            let _ = fs::remove_file(&change.staged);
        }
        return Err(SwitchError("History warnings could not be saved. No settings were changed."));
    }
    if budget.is_some_and(|limit| start.elapsed() > limit) || (check_process && codex_running()?) {
        for c in &journal.changes {
            let _ = fs::remove_file(&c.staged);
        }
        return Err(SwitchError("Close Codex and try again. No settings were changed."));
    }
    if let Err(e) = private_write(&config.receipt_dir.join(JOURNAL), &serde_json::to_vec(&journal).map_err(|_| INVALID)?) {
        for c in &journal.changes {
            let _ = fs::remove_file(&c.staged);
        }
        return Err(e);
    }
    progress(SwitchProgress { phase: SwitchPhase::Applying, completed: 0, total });
    let apply = (|| -> Result<()> {
        for (completed, c) in journal.changes.iter().enumerate() {
            fs::rename(&c.path, &c.backup).map_err(|_| INVALID)?;
            fs::rename(&c.staged, &c.path).map_err(|_| INVALID)?;
            if let Some(tx) = &transaction {
                write_offsets(tx, c, true)?;
                if c.indexed
                    && tx
                        .execute("UPDATE threads SET model_provider=?1 WHERE id=?2 AND model_provider=?3", params![c.target, c.id, c.source])
                        .map_err(|_| INVALID)?
                        != 1
                {
                    return Err(INVALID);
                }
            }
            progress(SwitchProgress { phase: SwitchPhase::Applying, completed: completed + 1, total });
        }
        progress(SwitchProgress { phase: SwitchPhase::Settings, completed: total, total });
        if on {
            if !config.is_installed() {
                config.install(port, key, None).map_err(|_| SwitchError("Provider settings could not be updated safely."))?;
            }
        } else {
            config.restore().map_err(|_| SwitchError("Provider settings could not be updated safely."))?;
        }
        Ok(())
    })();
    if let Err(e) = apply {
        rollback(config, &journal).map_err(|_| SwitchError("The switch stopped. Recovery files were kept; keep Codex closed and try again."))?;
        cleanup(config, &journal)?;
        return Err(e);
    }
    if let Some(tx) = transaction {
        if tx.commit().is_err() {
            rollback(config, &journal)?;
            cleanup(config, &journal)?;
            return Err(INVALID);
        }
    }
    if !journal.changes.iter().any(|c| c.indexed) {
        journal.completed = true;
        let pending = config.receipt_dir.join(format!(".provider-switch-{}", uuid::Uuid::new_v4()));
        let complete = private_write(&pending, &serde_json::to_vec(&journal).map_err(|_| INVALID)?)
            .and_then(|()| fs::rename(&pending, config.receipt_dir.join(JOURNAL)).map_err(|_| INVALID));
        if let Err(e) = complete {
            let _ = fs::remove_file(pending);
            recover(config).map_err(|_| SwitchError("The switch stopped. Recovery files were kept; keep Codex closed and try again."))?;
            return Err(e);
        }
    }
    let report = SwitchReport {
        history_pending: !issues.is_empty(),
        skipped: skipped_groups
            + issues
                .iter()
                .filter(|i| matches!(i.stage, HistoryStage::Scan) && matches!(i.code, HistoryIssueCode::DamagedHeader | HistoryIssueCode::UnsafePath))
                .count(),
        chats: journal.changes.iter().map(|c| &c.id).collect::<std::collections::HashSet<_>>().len(),
        elapsed_ms: start.elapsed().as_millis() as u64,
        reopen_required: true,
    };
    // A committed switch is successful even if cleanup must finish next time.
    let _ = cleanup(config, &journal);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    fn pending_recovery_is_detected_without_reading_journal_contents() {
        let (dir, config, _, _) = fixture();
        fs::create_dir_all(&config.receipt_dir).unwrap();
        assert!(!recovery_pending(&config));
        fs::write(config.receipt_dir.join(JOURNAL), b"invalid journal").unwrap();
        assert!(recovery_pending(&config));
        fs::remove_file(config.receipt_dir.join(JOURNAL)).unwrap();
        fs::create_dir(config.receipt_dir.join(JOURNAL)).unwrap();
        assert!(recovery_pending(&config));
        drop(dir);
    }
    fn fixture() -> (tempfile::TempDir, CodexConfig, PathBuf, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("codex");
        let sessions = home.join("sessions/2026/10/11");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(home.join("config.toml"), b"model_provider=\"openai\"\nmodel=\"gpt-5.6-sol\"\n").unwrap();
        let path = sessions.join("rollout-synthetic.jsonl");
        let tail = b"{\"type\":\"event_msg\",\"payload\":{\"text\":\"SECRET_HISTORY\"}}\n".to_vec();
        let mut data = b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"synthetic\",\"model_provider\":\"openai\"}}\n".to_vec();
        data.extend(&tail);
        fs::write(&path, data).unwrap();
        let db = Connection::open(home.join("state_5.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY,rollout_path TEXT,model_provider TEXT);").unwrap();
        db.execute("INSERT INTO threads VALUES('synthetic',?1,'openai')", [path.to_str().unwrap()]).unwrap();
        let config = CodexConfig { codex_home: home, receipt_dir: dir.path().join("aillies") };
        (dir, config, path, tail)
    }
    #[test]
    fn damaged_chat_is_preserved_while_compatible_chat_switches() {
        let (_dir, cfg, good, _) = fixture();
        let bad = good.parent().unwrap().join("rollout-damaged.jsonl");
        let bytes = b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"damaged\",\"model_provider\":\"openai\"}}\nPRIVATE_BROKEN_RECORD\n";
        fs::write(&bad, bytes).unwrap();
        let db = Connection::open(cfg.codex_home.join("state_5.sqlite")).unwrap();
        db.execute("INSERT INTO threads VALUES('damaged',?1,'openai')", [bad.to_str().unwrap()]).unwrap();
        let report = switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        assert_eq!(report.chats, 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(fs::read(&bad).unwrap(), bytes);
        assert_eq!(db.query_row("SELECT model_provider FROM threads WHERE id='damaged'", [], |r| r.get::<_, String>(0)).unwrap(), "openai");
        let diagnostics = serde_json::to_string(&migration_diagnostics(&cfg)).unwrap();
        assert!(diagnostics.contains("damaged_record"));
        assert_eq!(migration_diagnostics(&cfg).last().unwrap().record, Some(2));
        assert!(!diagnostics.contains("PRIVATE_BROKEN_RECORD"));
        assert!(!diagnostics.contains(bad.to_str().unwrap()));
        assert!(cfg.is_installed());
        let again = switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        assert_eq!(again.chats, 0);
        assert_eq!(again.skipped, 1);
    }

    #[test]
    fn legacy_null_fields_and_complete_tail_preserve_every_original_byte() {
        let event = serde_json::json!({"type":"event_msg","ordinal":null,"payload":{"type":"thread_settings_applied","thread_id":null,"thread_settings":{"model_provider_id":null,"approval_policy":"never","future_setting":{"keep":true}}}});
        let bytes = serde_json::to_vec(&event).unwrap();
        let mut output = Vec::new();
        copy_history(&mut std::io::Cursor::new(&bytes), &mut output, "synthetic", "aillies", None, Instant::now(), BUDGET).unwrap();
        assert_eq!(&output[..bytes.len()], &bytes);
        assert_eq!(output[bytes.len()], b'\n');
        let appended: Value = serde_json::from_slice(&output[bytes.len() + 1..]).unwrap();
        assert_eq!(appended["payload"]["thread_settings"]["approval_policy"], "never");
        assert_eq!(appended["payload"]["thread_settings"]["future_setting"], event["payload"]["thread_settings"]["future_setting"]);
        assert_eq!(appended["payload"]["thread_settings"]["model_provider_id"], "aillies");
    }

    #[test]
    fn damaged_replica_keeps_all_files_of_that_chat_unchanged() {
        let (_dir, cfg, path, _) = fixture();
        let before = fs::read(&path).unwrap();
        let replica = path.parent().unwrap().join("rollout-replica.jsonl");
        let mut bad = before.clone();
        bad.extend_from_slice(b"BROKEN_RECORD\n");
        fs::write(&replica, &bad).unwrap();
        let result = switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        assert_eq!(result.chats, 0);
        assert_eq!(result.skipped, 1);
        assert_eq!(fs::read(path).unwrap(), before);
        assert_eq!(fs::read(replica).unwrap(), bad);
    }

    #[test]
    fn history_warning_log_is_bounded_private_and_content_free() {
        let (_dir, cfg, path, _) = fixture();
        let mut issues = Vec::new();
        for n in 0..110 {
            issue(&mut issues, &path, HistoryIssueCode::DamagedRecord, HistoryStage::Prepare, Some(n));
        }
        save_issues(&cfg, &issues).unwrap();
        let entries = migration_diagnostics(&cfg);
        assert_eq!(entries.len(), 100);
        assert_eq!(entries[0].record, Some(10));
        assert_eq!(entries[99].record, Some(109));
        assert!(!serde_json::to_string(&entries).unwrap().contains(path.to_str().unwrap()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(cfg.receipt_dir.join("history-diagnostics.json")).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn valid_unterminated_settings_record_preserves_paginated_resume_boundary() {
        let (_dir, cfg, path, _) = fixture();
        let event = serde_json::json!({"type":"event_msg","payload":{"type":"thread_settings_applied","thread_settings":{"model_provider_id":"openai"}}});
        let mut original = fs::read(&path).unwrap();
        original.extend(serde_json::to_vec(&event).unwrap());
        fs::write(&path, &original).unwrap();
        let db = Connection::open(cfg.codex_home.join("thread_history_1.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE thread_turns(thread_id TEXT,turn_id TEXT,rollout_byte_offset INTEGER,rollout_end_byte_offset INTEGER);CREATE TABLE thread_history_projection_state(thread_id TEXT,next_rollout_byte_offset INTEGER);").unwrap();
        db.execute("INSERT INTO thread_history_projection_state VALUES('synthetic',?1)", [original.len() as i64]).unwrap();
        drop(db);
        assert_eq!(switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap().chats, 1);
        let db = Connection::open(cfg.codex_home.join("thread_history_1.sqlite")).unwrap();
        let next: i64 = db.query_row("SELECT next_rollout_byte_offset FROM thread_history_projection_state", [], |r| r.get(0)).unwrap();
        let bytes = fs::read(path).unwrap();
        let appended: Value = serde_json::from_slice(&bytes[next as usize..]).unwrap();
        assert_eq!(appended["payload"]["thread_settings"]["model_provider_id"], "aillies");
    }

    #[test]
    fn unsupported_database_preserves_all_history_and_allows_new_chat_configuration() {
        let (_dir, cfg, path, _) = fixture();
        let before = fs::read(&path).unwrap();
        let unknown = cfg.codex_home.join("state_99.sqlite");
        fs::write(&unknown, b"DAMAGED_DB").unwrap();
        let report = switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        assert_eq!(report.chats, 0);
        assert!(report.history_pending);
        assert_eq!(fs::read(path).unwrap(), before);
        assert_eq!(fs::read(unknown).unwrap(), b"DAMAGED_DB");
        assert!(cfg.is_installed());
    }

    #[test]
    fn unsupported_offset_schema_preserves_index_and_transcript() {
        let (_dir, cfg, path, _) = fixture();
        let before = fs::read(&path).unwrap();
        let history = Connection::open(cfg.codex_home.join("thread_history_7.sqlite")).unwrap();
        history.execute_batch("CREATE TABLE old_history(id TEXT);").unwrap();
        drop(history);
        let result = switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        assert_eq!(result.chats, 0);
        assert_eq!(result.skipped, 1);
        assert_eq!(fs::read(path).unwrap(), before);
        let db = Connection::open(cfg.codex_home.join("state_5.sqlite")).unwrap();
        assert_eq!(db.query_row("SELECT model_provider FROM threads", [], |r| r.get::<_, String>(0)).unwrap(), "openai");
    }

    #[test]
    fn obsolete_database_files_do_not_block_newer_compatible_index() {
        let (_dir, cfg, _, _) = fixture();
        fs::write(cfg.codex_home.join("state_4.sqlite"), b"OBSOLETE_DATABASE").unwrap();
        fs::rename(cfg.codex_home.join("state_5.sqlite"), cfg.codex_home.join("state_6.sqlite")).unwrap();
        assert_eq!(switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap().chats, 1);
        assert_eq!(fs::read(cfg.codex_home.join("state_4.sqlite")).unwrap(), b"OBSOLETE_DATABASE");
        assert_eq!(switch_closed(&cfg, false, 18522, "local-key", BUDGET, false, &|_| {}).unwrap().chats, 1);
    }

    #[test]
    fn switching_multiline_config_preserves_history_content() {
        let (_dir, cfg, path, tail) = fixture();
        let mut original = fs::read_to_string(cfg.config_path()).unwrap();
        original.push_str("instructions = \"\"\"\n[model_providers.aillies]\n# Preserve this literal history-like text.\n\"\"\"\n");
        fs::write(cfg.config_path(), &original).unwrap();
        let original_header: Value = serde_json::from_slice(fs::read(&path).unwrap().split(|b| *b == b'\n').next().unwrap()).unwrap();
        let first = switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        assert_eq!(first.chats, 1);
        assert!(first.elapsed_ms < 2000);
        let data = fs::read(&path).unwrap();
        let newline = data.iter().position(|b| *b == b'\n').unwrap();
        assert_eq!(&data[newline + 1..], tail);
        let header: Value = serde_json::from_slice(&data[..newline]).unwrap();
        assert_eq!(header["payload"]["model_provider"], "aillies");
        let db = Connection::open(cfg.codex_home.join("state_5.sqlite")).unwrap();
        assert_eq!(db.query_row("SELECT model_provider FROM threads", [], |r| r.get::<_, String>(0)).unwrap(), "aillies");
        drop(db);
        switch_closed(&cfg, false, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        assert!(!cfg.is_installed());
        let restored_history = fs::read(path).unwrap();
        let newline = restored_history.iter().position(|b| *b == b'\n').unwrap();
        assert_eq!(&restored_history[newline + 1..], tail);
        assert_eq!(serde_json::from_slice::<Value>(&restored_history[..newline]).unwrap(), original_header);
        assert_eq!(fs::read_to_string(cfg.config_path()).unwrap(), original);
    }
    #[test]
    fn interrupted_switch_recovers_and_config_failure_rolls_back() {
        let (_dir, cfg, path, _) = fixture();
        let old = fs::read(&path).unwrap();
        let original = fs::read(cfg.config_path()).unwrap();
        let backup = path.with_file_name(".aillies-old-test");
        let staged = path.with_file_name(".aillies-new-test");
        fs::rename(&path, &backup).unwrap();
        fs::write(&path, b"interrupted replacement").unwrap();
        let journal = Journal {
            state_db: None,
            history_db: None,
            on: true,
            completed: false,
            changes: vec![Change {
                id: "synthetic".into(),
                indexed: true,
                path: path.clone(),
                backup,
                staged,
                source: "openai".into(),
                target: "aillies".into(),
                history: None,
            }],
            config_before: Some(STANDARD.encode(&original)),
            receipt_before: None,
        };
        private_write(&cfg.receipt_dir.join(JOURNAL), &serde_json::to_vec(&journal).unwrap()).unwrap();
        fs::write(cfg.config_path(), b"broken temporary configuration").unwrap();
        recover(&cfg).unwrap();
        assert_eq!(fs::read(&path).unwrap(), old);
        assert_eq!(fs::read(cfg.config_path()).unwrap(), original);
        // A config parse failure after staging must roll the transcript and index back.
        fs::write(cfg.config_path(), b"model_provider='openai'\n[model_providers.aillies]\nname='user-owned'\n").unwrap();
        assert!(switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).is_err());
        assert_eq!(fs::read(path).unwrap(), old);
        let db = Connection::open(cfg.codex_home.join("state_5.sqlite")).unwrap();
        assert_eq!(db.query_row("SELECT model_provider FROM threads", [], |r| r.get::<_, String>(0)).unwrap(), "openai");
    }

    #[test]
    fn paginated_offsets_follow_header_size_and_recovery_does_not_shift_twice() {
        let (_dir, cfg, path, _) = fixture();
        let before = fs::read(&path).unwrap();
        let boundary = before.iter().position(|b| *b == b'\n').unwrap() as i64 + 1;
        let history = Connection::open(cfg.codex_home.join("thread_history_1.sqlite")).unwrap();
        history.execute_batch("CREATE TABLE thread_turns(thread_id TEXT,turn_id TEXT,rollout_byte_offset INTEGER,rollout_end_byte_offset INTEGER); CREATE TABLE thread_history_projection_state(thread_id TEXT,next_rollout_byte_offset INTEGER);").unwrap();
        history.execute("INSERT INTO thread_turns VALUES('synthetic','turn',?1,?2)", params![boundary, before.len() as i64]).unwrap();
        history.execute("INSERT INTO thread_history_projection_state VALUES('synthetic',?1)", [before.len() as i64]).unwrap();
        drop(history);
        switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        let after = fs::read(&path).unwrap();
        let delta = after.len() as i64 - before.len() as i64;
        let history = Connection::open(cfg.codex_home.join("thread_history_1.sqlite")).unwrap();
        let offset: i64 = history.query_row("SELECT rollout_byte_offset FROM thread_turns", [], |r| r.get(0)).unwrap();
        assert_eq!(&after[offset as usize..], &before[boundary as usize..]);
        drop(history);
        let journal = Journal {
            state_db: None,
            history_db: None,
            on: true,
            completed: false,
            changes: vec![Change {
                id: "synthetic".into(),
                indexed: true,
                path: path.clone(),
                backup: path.with_file_name(".aillies-old-offset"),
                staged: path.with_file_name(".aillies-new-offset"),
                source: "openai".into(),
                target: "aillies".into(),
                history: Some(HistoryOffsets {
                    delimiter_at: None,
                    delta,
                    turns: vec![("turn".into(), Some(boundary), Some(before.len() as i64))],
                    projection: Some(before.len() as i64),
                }),
            }],
            config_before: None,
            receipt_before: None,
        };
        private_write(&cfg.receipt_dir.join(JOURNAL), &serde_json::to_vec(&journal).unwrap()).unwrap();
        recover(&cfg).unwrap();
        let history = Connection::open(cfg.codex_home.join("thread_history_1.sqlite")).unwrap();
        assert_eq!(history.query_row("SELECT rollout_byte_offset FROM thread_turns", [], |r| r.get::<_, i64>(0)).unwrap(), offset);
        assert_eq!(
            history.query_row("SELECT next_rollout_byte_offset FROM thread_history_projection_state", [], |r| r.get::<_, i64>(0)).unwrap(),
            after.len() as i64
        );
        drop(history);
        switch_closed(&cfg, false, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        let history = Connection::open(cfg.codex_home.join("thread_history_1.sqlite")).unwrap();
        assert_eq!(
            history.query_row("SELECT next_rollout_byte_offset FROM thread_history_projection_state", [], |r| r.get::<_, i64>(0)).unwrap(),
            fs::metadata(path).unwrap().len() as i64
        );
    }

    #[test]
    fn unindexed_chats_switch_without_fabricating_index_rows() {
        let (_dir, cfg, path, tail) = fixture();
        let extra = cfg.codex_home.join("archived_sessions/rollout-unindexed.jsonl");
        fs::create_dir_all(extra.parent().unwrap()).unwrap();
        let data = fs::read_to_string(&path).unwrap().replace("synthetic", "unindexed");
        fs::write(&extra, &data).unwrap();
        let unrelated = path.with_file_name("rollout-custom.jsonl");
        let custom = data.replace("unindexed", "custom").replace("openai", "custom-provider");
        fs::write(&unrelated, &custom).unwrap();
        assert_eq!(switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap().chats, 2);
        let after = fs::read(&extra).unwrap();
        let boundary = after.iter().position(|b| *b == b'\n').unwrap() + 1;
        assert_eq!(&after[boundary..], tail);
        assert_eq!(serde_json::from_slice::<Value>(&after[..boundary]).unwrap()["payload"]["model_provider"], "aillies");
        assert_eq!(fs::read(&unrelated).unwrap(), custom.as_bytes());
        let db = Connection::open(cfg.codex_home.join("state_5.sqlite")).unwrap();
        assert_eq!(db.query_row("SELECT COUNT(*) FROM threads", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
        drop(db);
        assert_eq!(switch_closed(&cfg, false, 18522, "local-key", BUDGET, false, &|_| {}).unwrap().chats, 2);
        assert!(fs::read_to_string(&extra).unwrap().contains("\"model_provider\":\"openai\""));
        // Older CLI homes can have rollout files without a state database at all.
        fs::remove_file(cfg.codex_home.join("state_5.sqlite")).unwrap();
        assert_eq!(switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap().chats, 2);
        assert!(!cfg.codex_home.join("state_5.sqlite").exists());
        switch_closed(&cfg, false, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
    }

    #[test]
    fn provider_settings_event_preserves_history_and_every_other_setting() {
        let (_dir, cfg, path, _) = fixture();
        let settings = serde_json::json!({"type":"event_msg","ordinal":7,"payload":{
            "type":"thread_settings_applied","thread_id":"synthetic","thread_settings":{
                "model_provider_id":"openai","model":"gpt-5.6-sol","approval_policy":"never",
                "permission_profile":{"type":"managed","network":"restricted"},"cwd":"synthetic-cwd"
            }
        }});
        let mut event = serde_json::to_vec(&settings).unwrap();
        event.push(b'\n');
        fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(&event).unwrap();
        let original = fs::read(&path).unwrap();
        let tail = &original[original.iter().position(|b| *b == b'\n').unwrap() + 1..];
        switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        let after = fs::read(&path).unwrap();
        let boundary = after.iter().position(|b| *b == b'\n').unwrap() + 1;
        assert_eq!(&after[boundary..boundary + tail.len()], tail);
        let appended: Value = serde_json::from_slice(&after[boundary + tail.len()..]).unwrap();
        let mut expected = settings.clone();
        expected["ordinal"] = Value::from(8);
        expected["payload"]["thread_settings"]["model_provider_id"] = Value::String("aillies".into());
        assert_eq!(appended, expected);
        switch_closed(&cfg, false, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        let appended: Value = serde_json::from_str(after.lines().last().unwrap()).unwrap();
        assert_eq!(appended["ordinal"], 9);
        assert_eq!(appended["payload"]["thread_settings"], settings["payload"]["thread_settings"]);
    }

    #[test]
    fn older_settings_event_without_thread_id_preserves_every_other_setting() {
        let event = serde_json::json!({"type":"event_msg","ordinal":3,"payload":{
            "type":"thread_settings_applied","thread_settings":{"model_provider_id":"openai",
            "approval_policy":"never","permission_profile":{"type":"managed","network":"restricted"}}
        }});
        let mut bytes = serde_json::to_vec(&event).unwrap();
        bytes.push(b'\n');
        let mut output = Vec::new();
        copy_history(&mut std::io::Cursor::new(&bytes), &mut output, "synthetic", "aillies", None, Instant::now(), BUDGET).unwrap();
        assert_eq!(&output[..bytes.len()], &bytes);
        let appended: Value = serde_json::from_slice(&output[bytes.len()..]).unwrap();
        let mut expected = event;
        expected["ordinal"] = Value::from(4);
        expected["payload"]["thread_settings"]["model_provider_id"] = Value::from("aillies");
        assert_eq!(appended, expected);
    }

    #[test]
    fn settings_for_another_chat_or_malformed_provider_refuse() {
        {
            let thread = Value::from("other-chat");
            let event = serde_json::json!({"type":"event_msg","payload":{
                "type":"thread_settings_applied","thread_id":thread,"thread_settings":{"model_provider_id":"openai"}
            }});
            let mut bytes = serde_json::to_vec(&event).unwrap();
            bytes.push(b'\n');
            assert!(copy_history(&mut std::io::Cursor::new(bytes), &mut Vec::new(), "synthetic", "aillies", None, Instant::now(), BUDGET).is_err());
        }
    }

    #[test]
    fn file_only_recovery_rolls_back_until_completion_is_durable() {
        for completed in [false, true] {
            let (_dir, cfg, path, _) = fixture();
            fs::remove_file(cfg.codex_home.join("state_5.sqlite")).unwrap();
            let original = fs::read(&path).unwrap();
            let config_before = fs::read(cfg.config_path()).unwrap();
            let backup = path.with_file_name(".aillies-old-recovery");
            fs::rename(&path, &backup).unwrap();
            let replacement = String::from_utf8(original.clone()).unwrap().replace("openai", "aillies");
            fs::write(&path, &replacement).unwrap();
            cfg.install(18522, "local-key", None).unwrap();
            let journal = Journal {
                state_db: None,
                history_db: None,
                on: true,
                completed,
                changes: vec![Change {
                    id: "synthetic".into(),
                    indexed: false,
                    path: path.clone(),
                    backup,
                    staged: path.with_file_name(".aillies-new-recovery"),
                    source: "openai".into(),
                    target: "aillies".into(),
                    history: None,
                }],
                config_before: Some(STANDARD.encode(config_before)),
                receipt_before: None,
            };
            private_write(&cfg.receipt_dir.join(JOURNAL), &serde_json::to_vec(&journal).unwrap()).unwrap();
            recover(&cfg).unwrap();
            assert_eq!(cfg.is_installed(), completed);
            assert_eq!(fs::read(&path).unwrap(), if completed { replacement.into_bytes() } else { original });
            assert!(!cfg.codex_home.join("state_5.sqlite").exists());
            assert!(!cfg.receipt_dir.join(JOURNAL).exists());
        }
    }

    #[test]
    fn orphaned_history_cache_preserves_unindexed_rollout() {
        let (_dir, cfg, path, _) = fixture();
        let original = fs::read(&path).unwrap();
        fs::remove_file(cfg.codex_home.join("state_5.sqlite")).unwrap();
        fs::write(cfg.codex_home.join("thread_history_1.sqlite"), b"unknown cache").unwrap();
        let report = switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        assert_eq!(report.chats, 0);
        assert_eq!(report.skipped, 1);
        assert!(report.history_pending);
        assert_eq!(fs::read(path).unwrap(), original);
        assert_eq!(fs::read(cfg.codex_home.join("thread_history_1.sqlite")).unwrap(), b"unknown cache");
    }

    #[test]
    fn unindexed_chat_preserves_existing_cached_offsets_both_directions() {
        let (_dir, cfg, path, _) = fixture();
        let original = fs::read(&path).unwrap();
        let boundary = original.iter().position(|b| *b == b'\n').unwrap() as i64 + 1;
        let db = Connection::open(cfg.codex_home.join("state_5.sqlite")).unwrap();
        db.execute("DELETE FROM threads", []).unwrap();
        drop(db);
        let history = Connection::open(cfg.codex_home.join("thread_history_1.sqlite")).unwrap();
        history.execute_batch("CREATE TABLE thread_turns(thread_id TEXT,turn_id TEXT,rollout_byte_offset INTEGER,rollout_end_byte_offset INTEGER); CREATE TABLE thread_history_projection_state(thread_id TEXT,next_rollout_byte_offset INTEGER);").unwrap();
        history.execute("INSERT INTO thread_turns VALUES('synthetic','turn',?1,?2)", params![boundary, original.len() as i64]).unwrap();
        history.execute("INSERT INTO thread_history_projection_state VALUES('synthetic',?1)", [original.len() as i64]).unwrap();
        drop(history);
        for on in [true, false] {
            let report = switch_closed(&cfg, on, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
            assert_eq!(report.chats, 1);
            assert!(!report.history_pending);
            let after = fs::read(&path).unwrap();
            let history = Connection::open(cfg.codex_home.join("thread_history_1.sqlite")).unwrap();
            let offset: i64 = history.query_row("SELECT rollout_byte_offset FROM thread_turns", [], |r| r.get(0)).unwrap();
            assert_eq!(&after[offset as usize..], &original[boundary as usize..]);
        }
    }

    #[test]
    fn indexed_replicas_switch_together_without_reapplying_canonical_offsets() {
        let (_dir, cfg, path, _) = fixture();
        let replica = path.with_file_name("rollout-replica.jsonl");
        let mut copied = fs::read(&path).unwrap();
        copied.extend_from_slice(b"{\"type\":\"event_msg\",\"payload\":{\"text\":\"extra preserved history\"}}\n");
        fs::write(&replica, &copied).unwrap();
        let original = fs::read(&path).unwrap();
        let boundary = original.iter().position(|b| *b == b'\n').unwrap() as i64 + 1;
        let history = Connection::open(cfg.codex_home.join("thread_history_1.sqlite")).unwrap();
        history.execute_batch("CREATE TABLE thread_turns(thread_id TEXT,turn_id TEXT,rollout_byte_offset INTEGER,rollout_end_byte_offset INTEGER); CREATE TABLE thread_history_projection_state(thread_id TEXT,next_rollout_byte_offset INTEGER);").unwrap();
        history.execute("INSERT INTO thread_turns VALUES('synthetic','turn',?1,?2)", params![boundary, original.len() as i64]).unwrap();
        history.execute("INSERT INTO thread_history_projection_state VALUES('synthetic',?1)", [original.len() as i64]).unwrap();
        drop(history);
        assert_eq!(switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap().chats, 1);
        let after = fs::read(&path).unwrap();
        let history = Connection::open(cfg.codex_home.join("thread_history_1.sqlite")).unwrap();
        let offset: i64 = history.query_row("SELECT rollout_byte_offset FROM thread_turns", [], |r| r.get(0)).unwrap();
        assert_eq!(&after[offset as usize..], &original[boundary as usize..]);
        drop(history);
        let after_replica = fs::read(&replica).unwrap();
        let replica_boundary = after_replica.iter().position(|b| *b == b'\n').unwrap() + 1;
        assert_eq!(&after_replica[replica_boundary..], &copied[boundary as usize..]);
        assert_eq!(serde_json::from_slice::<Value>(&after_replica[..replica_boundary]).unwrap()["payload"]["model_provider"], "aillies");
        assert_eq!(switch_closed(&cfg, false, 18522, "local-key", BUDGET, false, &|_| {}).unwrap().chats, 1);
        for (file, before) in [(&path, &original), (&replica, &copied)] {
            let after = fs::read(file).unwrap();
            let n = after.iter().position(|b| *b == b'\n').unwrap() + 1;
            assert_eq!(serde_json::from_slice::<Value>(&after[..n]).unwrap()["payload"]["model_provider"], "openai");
            assert_eq!(&after[n..], &before[boundary as usize..]);
        }
    }

    #[test]
    fn duplicate_history_is_refused_and_bulk_archived_chats_are_preserved() {
        let (_dir, cfg, path, tail) = fixture();
        let extra = path.with_file_name("rollout-unindexed.jsonl");
        fs::copy(&path, &extra).unwrap();
        let db = Connection::open(cfg.codex_home.join("state_5.sqlite")).unwrap();
        db.execute("DELETE FROM threads", []).unwrap();
        drop(db);
        let skipped = switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        assert_eq!(skipped.chats, 0);
        assert!(skipped.history_pending);
        fs::remove_file(extra).unwrap();
        let db = Connection::open(cfg.codex_home.join("state_5.sqlite")).unwrap();
        db.execute("INSERT INTO threads VALUES('synthetic',?1,'openai')", [path.to_str().unwrap()]).unwrap();
        for n in 1..100 {
            let id = format!("synthetic-{n}");
            let dir = if n % 2 == 0 { cfg.codex_home.join("archived_sessions") } else { path.parent().unwrap().to_owned() };
            fs::create_dir_all(&dir).unwrap();
            let file = dir.join(format!("rollout-{id}.jsonl"));
            let mut bytes = serde_json::to_vec(&serde_json::json!({"type":"session_meta","payload":{"id":id,"model_provider":"openai"}})).unwrap();
            bytes.push(b'\n');
            bytes.extend_from_slice(&tail);
            fs::write(&file, bytes).unwrap();
            db.execute("INSERT INTO threads VALUES(?1,?2,'openai')", params![id, file.to_str().unwrap()]).unwrap();
        }
        drop(db);
        let report = switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        assert_eq!(report.chats, 100);
        eprintln!("100 synthetic chats: {} ms", report.elapsed_ms);
        switch_closed(&cfg, false, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
    }

    #[test]
    #[ignore = "requires installed Codex and Python; isolated home, no live inference"]
    fn installed_codex_lists_and_resumes_migrated_history() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = CodexConfig { codex_home: dir.path().join("codex"), receipt_dir: dir.path().join("aillies") };
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/codex-provider-probe.py");
        let probe = |mode: &str| {
            assert!(std::process::Command::new("python3").arg(&script).arg(&cfg.codex_home).arg(mode).status().unwrap().success());
        };
        probe("create");
        let first = switch_closed(&cfg, true, 18599, "synthetic-local-key", BUDGET, false, &|_| {}).unwrap();
        assert_eq!(first.chats, 1);
        probe("aillies");
        switch_closed(&cfg, false, 18599, "synthetic-local-key", BUDGET, false, &|_| {}).unwrap();
        probe("normal");
        // Reproduce a valid rollout that lost only its state-index row.
        let db = Connection::open(cfg.codex_home.join("state_5.sqlite")).unwrap();
        db.execute("DELETE FROM threads", []).unwrap();
        drop(db);
        assert_eq!(switch_closed(&cfg, true, 18599, "synthetic-local-key", BUDGET, false, &|_| {}).unwrap().chats, 1);
        probe("aillies");
        switch_closed(&cfg, false, 18599, "synthetic-local-key", BUDGET, false, &|_| {}).unwrap();
        probe("normal");
    }

    #[test]
    fn review_count_is_read_only_and_includes_indexed_chats() {
        let (_dir, cfg, path, _) = fixture();
        let before = fs::read(&path).unwrap();
        assert_eq!(preview_chat_count(&cfg, true), Some(1));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(!cfg.is_installed());
    }

    #[test]
    fn background_switch_reports_preparation_before_commit_without_content() {
        let (_dir, cfg, path, _) = fixture();
        let before = fs::read(&path).unwrap();
        let observations = std::cell::RefCell::new(Vec::new());
        let report = switch_closed(&cfg, true, 18522, "synthetic-local-key", BUDGET, false, &|progress| {
            if matches!(progress.phase, SwitchPhase::Scanning | SwitchPhase::Preparing) {
                assert_eq!(fs::read(&path).unwrap(), before);
                assert!(!cfg.is_installed());
            }
            observations.borrow_mut().push(progress);
        })
        .unwrap();
        assert_eq!(report.chats, 1);
        let observations = observations.into_inner();
        assert_eq!(observations.first().unwrap().phase, SwitchPhase::Scanning);
        assert!(observations.iter().any(|p| p.phase == SwitchPhase::Preparing && p.completed == 1 && p.total == 1));
        assert_eq!(observations.last().unwrap().phase, SwitchPhase::Settings);
        assert_eq!(observations.last().unwrap().completed, 1);
        for progress in observations {
            let value = serde_json::to_value(progress).unwrap();
            assert_eq!(value.as_object().unwrap().len(), 3);
            assert!(value["phase"].is_string() && value["completed"].is_u64() && value["total"].is_u64());
        }
    }

    #[test]
    fn history_preparation_continues_after_two_seconds() {
        let bytes = b"{\"type\":\"event_msg\",\"payload\":{\"text\":\"synthetic history\"}}\n";
        let mut output = Vec::new();
        copy_history(&mut std::io::Cursor::new(bytes), &mut output, "synthetic", "aillies", None, Instant::now() - Duration::from_secs(3), BUDGET).unwrap();
        assert_eq!(&output, bytes);
    }

    #[test]
    fn timeout_aborts_and_bad_metadata_is_preserved() {
        let (_dir, cfg, path, _) = fixture();
        let old = fs::read(&path).unwrap();
        let oldconfig = fs::read(cfg.config_path()).unwrap();
        assert!(switch_closed(&cfg, true, 18522, "local-key", Some(Duration::ZERO), false, &|_| {}).is_err());
        assert_eq!(fs::read(&path).unwrap(), old);
        assert_eq!(fs::read(cfg.config_path()).unwrap(), oldconfig);
        let changed = String::from_utf8(old).unwrap().replace("session_meta", "unknown_meta");
        fs::write(&path, &changed).unwrap();
        let report = switch_closed(&cfg, true, 18522, "local-key", BUDGET, false, &|_| {}).unwrap();
        assert_eq!(report.chats, 0);
        assert_eq!(report.skipped, 1);
        assert_eq!(fs::read(path).unwrap(), changed.as_bytes());
    }
}
