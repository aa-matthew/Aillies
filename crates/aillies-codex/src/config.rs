//! Transactional Codex `config.toml` install/restore.
//!
//! Ported from Peer Route 0.8.3. We never parse-and-reserialize the user's
//! file: statements are located and only root `model_provider`/`model`, the
//! discovery flag and our appended provider table change. Anything ambiguous
//! fails before writing. Restore refuses to clobber later independent edits.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const PROVIDER_ID: &str = "aillies";
pub const DEFAULT_MODEL: &str = "gpt-5.5";
const RECEIPT: &str = "codex-restore.json";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} No changes were made.")]
pub struct ConfigError(pub String);

fn err<T>(m: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError(m.into()))
}

pub fn quoted(v: &str) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "\"\"".into())
}

#[derive(Debug, Clone)]
struct Statement {
    start: usize,
    end: usize,
    path: Vec<String>,
    header: bool,
    array_table: bool,
}

fn key_path(raw: &str) -> Result<Vec<String>, ConfigError> {
    let chars: Vec<char> = raw.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut out = Vec::new();
    let ws = |i: &mut usize| {
        while *i < n && chars[*i].is_whitespace() {
            *i += 1;
        }
    };
    loop {
        ws(&mut i);
        if i >= n {
            return err("Unsupported TOML key.");
        }
        let start = i;
        if chars[i] == '"' || chars[i] == '\'' {
            let q = chars[i];
            i += 1;
            let mut closed = false;
            while i < n {
                if q == '"' && chars[i] == '\\' {
                    i += 2;
                    continue;
                }
                if chars[i] == q {
                    i += 1;
                    closed = true;
                    break;
                }
                i += 1;
            }
            if !closed || i > n {
                return err("Unclosed TOML key.");
            }
            let token: String = chars[start..i].iter().collect();
            if q == '\'' {
                out.push(token[1..token.len() - 1].to_string());
            } else {
                match serde_json::from_str::<String>(&token) {
                    Ok(s) => out.push(s),
                    Err(_) => return err("Unsupported escape in TOML key."),
                }
            }
        } else {
            while i < n && chars[i].is_ascii() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '-') {
                i += 1;
            }
            if i == start {
                return err("Unsupported TOML key.");
            }
            out.push(chars[start..i].iter().collect());
        }
        ws(&mut i);
        if i == n {
            return Ok(out);
        }
        if chars[i] != '.' {
            return err("Unsupported TOML key.");
        }
        i += 1;
    }
}

fn statements(lines: &[&str]) -> Result<Vec<Statement>, ConfigError> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut first = 0;
    let mut depth: i32 = 0;
    let mut quote: Option<char> = None;
    let mut multiline = false;
    let mut escaped = false;
    for (li, line) in lines.iter().enumerate() {
        if current.is_empty() {
            first = li;
        }
        let mut clean = String::new();
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            i += 1;
            if let Some(active) = quote {
                clean.push(c);
                if active == '"' && escaped {
                    escaped = false;
                } else if active == '"' && c == '\\' {
                    escaped = true;
                } else if c == active {
                    if !multiline {
                        quote = None;
                    } else if chars.get(i) == Some(&active) && chars.get(i + 1) == Some(&active) {
                        // Valid TOML permits one or two quote characters immediately
                        // before the closing delimiter. The TOML parser validates the
                        // run; all of it belongs to this value, never to a new key.
                        while chars.get(i) == Some(&active) {
                            clean.push(active);
                            i += 1;
                        }
                        quote = None;
                        multiline = false;
                    }
                }
            } else {
                if c == '#' {
                    break;
                }
                clean.push(c);
                match c {
                    '"' | '\'' => {
                        quote = Some(c);
                        multiline = chars.get(i) == Some(&c) && chars.get(i + 1) == Some(&c);
                        if multiline {
                            clean.push(c);
                            clean.push(c);
                            i += 2;
                        }
                    }
                    '[' | '{' => depth += 1,
                    ']' | '}' => depth -= 1,
                    _ => {}
                }
                if depth < 0 {
                    return err("Unbalanced TOML value.");
                }
            }
        }
        if quote.is_some() && !multiline {
            return err("Unclosed TOML string.");
        }
        current.push_str(&clean);
        current.push('\n');
        // A backslash before a multiline newline trims whitespace; it does
        // not escape the first character of the next physical line.
        escaped = false;
        if depth > 0 || quote.is_some() {
            continue;
        }
        let text = current.trim().to_string();
        current.clear();
        if text.is_empty() {
            continue;
        }
        if text.starts_with('[') {
            let array = text.starts_with("[[");
            let w = if array { 2 } else { 1 };
            if !text.ends_with(if array { "]]" } else { "]" }) || text.len() < 2 * w {
                return err("Invalid TOML table.");
            }
            let raw = &text[w..text.len() - w];
            result.push(Statement { start: first, end: li, path: key_path(raw)?, header: true, array_table: array });
        } else {
            let mut active: Option<char> = None;
            let mut esc = false;
            let mut split = None;
            for (idx, c) in text.char_indices() {
                if let Some(q) = active {
                    if q == '"' && esc {
                        esc = false;
                    } else if q == '"' && c == '\\' {
                        esc = true;
                    } else if c == q {
                        active = None;
                    }
                } else if c == '"' || c == '\'' {
                    active = Some(c);
                } else if c == '=' {
                    split = Some(idx);
                    break;
                }
            }
            let Some(split) = split else { return err("Unsupported TOML statement.") };
            result.push(Statement { start: first, end: li, path: key_path(&text[..split])?, header: false, array_table: false });
        }
    }
    if depth != 0 || quote.is_some() {
        return err("Unclosed TOML value.");
    }
    Ok(result)
}

/// Produce the new config text. `model` overrides the root model; otherwise an
/// existing root model is kept and `DEFAULT_MODEL` is used only when absent.
pub fn render(original: &str, port: u16, key: &str, model: Option<&str>) -> Result<String, ConfigError> {
    if original.contains('\r') {
        return err("This configuration needs a manual merge.");
    }
    if key.is_empty() || key.contains('\n') || port < 1024 || model.is_some_and(|m| m.is_empty() || m.len() > 100) {
        return err("Missing model or local client key.");
    }
    let _: toml::Table = toml::from_str(original).map_err(|_| ConfigError("Invalid Codex TOML; fix it before enabling Aillies.".into()))?;
    let lines: Vec<&str> = if original.is_empty() { Vec::new() } else { original.split('\n').collect() };
    let entries = statements(&lines)?;
    let headers: Vec<&Statement> = entries.iter().filter(|e| e.header).collect();
    let root_end = headers.first().map(|h| h.start).unwrap_or(lines.len());
    let roots: Vec<&Statement> = entries.iter().filter(|e| !e.header && e.start < root_end).collect();

    let mut context: Vec<String> = Vec::new();
    for e in &entries {
        if e.header {
            context = e.path.clone();
        }
        let full: Vec<String> = if e.header { e.path.clone() } else { context.iter().chain(e.path.iter()).cloned().collect() };
        let ours = full.len() >= 2 && full[0] == "model_providers" && full[1] == PROVIDER_ID;
        let discovery_table = e.header && full.len() >= 2 && full[0] == "features" && full[1] == "api_key_model_discovery";
        let image_tool = full.len() >= 2 && full[0] == "mcp_servers" && full[1] == "aillies_images";
        if ours || discovery_table || image_tool {
            return err("An existing Aillies provider or conflicting discovery table requires a manual merge.");
        }
    }
    let bad_root = roots.iter().any(|r| {
        let head = r.path.first().map(String::as_str).unwrap_or("");
        ((head == "model" || head == "model_provider") && r.path.len() != 1) || head == "features" || head == "model_providers" || head == "mcp_servers"
    });
    if bad_root {
        return err("An inline or dotted provider/features setting requires a manual merge.");
    }

    let mut replacements: HashMap<usize, String> = HashMap::new();
    let mut insertions: HashMap<usize, Vec<String>> = HashMap::new();

    let provider_matches: Vec<&&Statement> = roots.iter().filter(|r| r.path == ["model_provider"]).collect();
    let model_matches: Vec<&&Statement> = roots.iter().filter(|r| r.path == ["model"]).collect();
    for (name, matches) in [("model_provider", &provider_matches), ("model", &model_matches)] {
        if matches.len() > 1 || matches.iter().any(|m| m.start != m.end) {
            return err(format!("Duplicate or multiline {name} setting requires a manual merge."));
        }
    }
    let provider_line = format!("model_provider = {}", quoted(PROVIDER_ID));
    match provider_matches.first() {
        Some(m) => {
            replacements.insert(m.start, provider_line);
        }
        None => insertions.entry(root_end).or_default().push(provider_line),
    }
    match (model_matches.first(), model) {
        (Some(m), Some(chosen)) => {
            replacements.insert(m.start, format!("model = {}", quoted(chosen)));
        }
        (Some(_), None) => {}
        (None, chosen) => insertions.entry(root_end).or_default().push(format!("model = {}", quoted(chosen.unwrap_or(DEFAULT_MODEL)))),
    }

    let features: Vec<&&Statement> = headers.iter().filter(|h| h.path == ["features"]).collect();
    if features.len() > 1 || features.iter().any(|f| f.array_table) {
        return err("Invalid or duplicate features table.");
    }
    if let Some(feature) = features.first() {
        let end = headers.iter().find(|h| h.start > feature.start).map(|h| h.start).unwrap_or(lines.len());
        let settings: Vec<&Statement> = entries
            .iter()
            .filter(|e| !e.header && e.start > feature.start && e.start < end && e.path.first().map(String::as_str) == Some("api_key_model_discovery"))
            .collect();
        if settings.len() > 1 || settings.iter().any(|s| s.path.len() != 1 || s.start != s.end) {
            return err("Ambiguous model discovery setting.");
        }
        match settings.first() {
            Some(s) => {
                replacements.insert(s.start, "api_key_model_discovery = false".into());
            }
            None => insertions.entry(feature.end + 1).or_default().push("api_key_model_discovery = false".into()),
        }
    }

    let mut out: Vec<String> = Vec::new();
    for i in 0..=lines.len() {
        if let Some(ins) = insertions.get(&i) {
            out.extend(ins.iter().cloned());
        }
        if i < lines.len() {
            out.push(replacements.get(&i).cloned().unwrap_or_else(|| lines[i].to_string()));
        }
    }
    if features.is_empty() {
        out.extend(["".into(), "[features]".into(), "api_key_model_discovery = false".into()]);
    }
    out.extend([
        String::new(),
        "# Managed by Aillies. Turn off \"Use Aillies with Codex\" to restore your previous settings.".into(),
        format!("[model_providers.{PROVIDER_ID}]"),
        "name = \"Aillies\"".into(),
        format!("base_url = \"http://127.0.0.1:{port}/v1\""),
        format!("experimental_bearer_token = {}", quoted(key)),
        "wire_api = \"responses\"".into(),
        "requires_openai_auth = false".into(),
        "supports_websockets = false".into(),
        "# Aillies owns the retry budget; disable additional Codex retries.".into(),
        "request_max_retries = 0".into(),
        "stream_max_retries = 0".into(),
        "stream_idle_timeout_ms = 300000".into(),
        String::new(),
        "[mcp_servers.aillies_images]".into(),
        format!("url = \"http://127.0.0.1:{port}/mcp/images\""),
        "tool_timeout_sec = 900".into(),
        "[mcp_servers.aillies_images.http_headers]".into(),
        format!("Authorization = {}", quoted(&format!("Bearer {key}"))),
        String::new(),
    ]);
    Ok(out.join("\n"))
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    original: Option<String>,
    installed: String,
}

fn private_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.aillies-tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("file")));
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        use std::io::Write;
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// Take Aillies' own changes out of `current` and keep everything else,
/// including edits Codex made after install (trusted projects, dismissed
/// notices, a different model). `installed` is what Aillies wrote and
/// `original` what was there before. Returns None when that can't be done
/// unambiguously.
pub fn merge_restore(installed: &str, original: &str, current: &str) -> Option<String> {
    let current_values: toml::Table = toml::from_str(current).ok()?;
    let _: toml::Table = toml::from_str(installed).ok()?;
    let _: toml::Table = toml::from_str(original).ok()?;
    let split = |t: &str| -> Vec<String> {
        if t.is_empty() {
            vec![]
        } else {
            t.split('\n').map(str::to_owned).collect()
        }
    };
    let (inst_l, orig_l, cur_l) = (split(installed), split(original), split(current));
    fn refs(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }
    let (inst_s, orig_s, cur_s) = (statements(&refs(&inst_l)).ok()?, statements(&refs(&orig_l)).ok()?, statements(&refs(&cur_l)).ok()?);

    let root_end = |st: &[Statement], n: usize| st.iter().find(|e| e.header).map(|h| h.start).unwrap_or(n);
    let root_line = |st: &[Statement], lines: &[String], key: &str, n: usize| -> Option<(usize, String)> {
        let end = root_end(st, n);
        st.iter().find(|e| !e.header && e.start < end && e.path == [key]).map(|e| (e.start, lines[e.start].clone()))
    };
    let table_key = |st: &[Statement], lines: &[String], table: &[&str], key: &str| -> Option<(usize, String)> {
        let mut ctx: Vec<String> = vec![];
        for e in st {
            if e.header {
                ctx = e.path.clone();
            } else if ctx == table && e.path == [key] {
                return Some((e.start, lines[e.start].clone()));
            }
        }
        None
    };

    let mut delete = vec![false; cur_l.len()];
    let mut replace: HashMap<usize, String> = HashMap::new();

    // 1. Our provider table (plus its marker comment and the blank line before it).
    if let Some(ours_hdr) = cur_s.iter().position(|e| e.header && e.path.len() >= 2 && e.path[0] == "model_providers" && e.path[1] == PROVIDER_ID) {
        for (pos, entry) in cur_s.iter().enumerate().skip(ours_hdr) {
            if entry.header && entry.path.len() >= 2 && entry.path[0] == "model_providers" && entry.path[1] == PROVIDER_ID {
                let end = cur_s.iter().skip(pos + 1).find(|e| e.header).map(|e| e.start).unwrap_or(cur_l.len());
                for d in delete.iter_mut().take(end).skip(entry.start) {
                    *d = true;
                }
            }
        }
        let mut i = cur_s[ours_hdr].start;
        if i > 0 && cur_l[i - 1].starts_with("# Managed by Aillies") {
            i -= 1;
            delete[i] = true;
        }
        if i > 0 && cur_l[i - 1].trim().is_empty() {
            delete[i - 1] = true;
        }
    }

    // The image MCP server is owned only by receipts that installed it.
    if inst_s.iter().any(|e| e.header && e.path == ["mcp_servers", "aillies_images"]) {
        for (pos, entry) in cur_s.iter().enumerate() {
            if entry.header && entry.path.len() >= 2 && entry.path[0] == "mcp_servers" && entry.path[1] == "aillies_images" {
                let end = cur_s.iter().skip(pos + 1).find(|e| e.header).map(|e| e.start).unwrap_or(cur_l.len());
                for d in delete.iter_mut().take(end).skip(entry.start) {
                    *d = true;
                }
            }
        }
    }

    // 2. Root model_provider.
    if let Some((idx, _)) = root_line(&cur_s, &cur_l, "model_provider", cur_l.len()) {
        if current_values.get("model_provider").and_then(toml::Value::as_str) == Some(PROVIDER_ID) {
            match root_line(&orig_s, &orig_l, "model_provider", orig_l.len()) {
                Some((_, o)) => {
                    replace.insert(idx, o);
                }
                None => delete[idx] = true,
            }
        }
    }

    // 3. Root model: undo only if Codex hasn't changed it since.
    if let (Some((idx, line)), Some((_, inst))) = (root_line(&cur_s, &cur_l, "model", cur_l.len()), root_line(&inst_s, &inst_l, "model", inst_l.len())) {
        if line == inst {
            match root_line(&orig_s, &orig_l, "model", orig_l.len()) {
                Some((_, o)) => {
                    replace.insert(idx, o);
                }
                None => delete[idx] = true,
            }
        }
    }

    // 4. features.api_key_model_discovery and a [features] table we created.
    if let Some((idx, _)) = table_key(&cur_s, &cur_l, &["features"], "api_key_model_discovery") {
        let current = current_values.get("features").and_then(|v| v.get("api_key_model_discovery"));
        let installed_values: toml::Table = toml::from_str(installed).ok()?;
        if current != installed_values.get("features").and_then(|v| v.get("api_key_model_discovery")) {
            // A later user replacement is no longer our value.
        } else {
            let orig_has_table = orig_s.iter().any(|e| e.header && e.path == ["features"]);
            match table_key(&orig_s, &orig_l, &["features"], "api_key_model_discovery") {
                Some((_, o)) => {
                    replace.insert(idx, o);
                }
                None => {
                    delete[idx] = true;
                    if !orig_has_table {
                        if let Some(h) = cur_s.iter().position(|e| e.header && e.path == ["features"]) {
                            let others = cur_s.iter().skip(h + 1).take_while(|e| !e.header).any(|e| e.start != idx);
                            if !others {
                                let hl = cur_s[h].start;
                                delete[hl] = true;
                                if hl > 0 && cur_l[hl - 1].trim().is_empty() {
                                    delete[hl - 1] = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let mut out: Vec<String> = Vec::new();
    for (i, line) in cur_l.iter().enumerate() {
        if delete[i] {
            continue;
        }
        out.push(replace.get(&i).cloned().unwrap_or_else(|| line.clone()));
    }
    while out.len() > 1 && out[out.len() - 1].is_empty() && out[out.len() - 2].is_empty() {
        out.pop();
    }
    let merged = out.join("\n");
    let merged_values: toml::Table = toml::from_str(&merged).ok()?;
    let still_ours = merged_values.get("model_provider").and_then(toml::Value::as_str) == Some(PROVIDER_ID)
        || merged_values.get("model_providers").and_then(toml::Value::as_table).is_some_and(|t| t.contains_key(PROVIDER_ID));
    (!still_ours).then_some(merged)
}

pub struct CodexConfig {
    pub codex_home: PathBuf,
    pub receipt_dir: PathBuf,
}

impl CodexConfig {
    pub fn config_path(&self) -> PathBuf {
        self.codex_home.join("config.toml")
    }
    pub(crate) fn receipt_path(&self) -> PathBuf {
        self.receipt_dir.join(RECEIPT)
    }

    /// Use the same model as Codex; never silently replace an invalid setting.
    pub fn selected_model(&self) -> Result<String, ConfigError> {
        let text = match std::fs::read_to_string(self.config_path()) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(_) => return err("Cannot read Codex config."),
        };
        let values: toml::Table = toml::from_str(&text).map_err(|_| ConfigError("Invalid Codex TOML.".into()))?;
        match values.get("model") {
            None => Ok(DEFAULT_MODEL.into()),
            Some(toml::Value::String(model)) if !model.is_empty() && model.len() <= 100 => Ok(model.clone()),
            _ => err("Invalid Codex model setting."),
        }
    }

    pub fn is_installed(&self) -> bool {
        self.receipt_path().exists()
    }

    pub fn install(&self, port: u16, key: &str, model: Option<&str>) -> Result<(), ConfigError> {
        if self.is_installed() {
            return err("Aillies is already installed in Codex. Restore it first.");
        }
        let path = self.config_path();
        let original = match std::fs::symlink_metadata(&path) {
            Ok(m) if !m.file_type().is_file() => return err("Refusing to replace a linked or non-regular Codex config file."),
            Ok(_) => Some(std::fs::read(&path).map_err(|_| ConfigError("Cannot read Codex config.".into()))?),
            Err(_) => None,
        };
        let text = match &original {
            Some(b) => String::from_utf8(b.clone()).map_err(|_| ConfigError("Codex configuration is not UTF-8.".into()))?,
            None => String::new(),
        };
        let installed = render(&text, port, key, model)?;
        let receipt = Receipt { original: original.as_ref().map(|b| STANDARD.encode(b)), installed: STANDARD.encode(installed.as_bytes()) };
        let receipt_bytes = serde_json::to_vec(&receipt).map_err(|_| ConfigError("Cannot save backup.".into()))?;
        private_write(&self.receipt_path(), &receipt_bytes).map_err(|_| ConfigError("Cannot save the Codex backup.".into()))?;
        if private_write(&path, installed.as_bytes()).is_err() {
            let _ = std::fs::remove_file(self.receipt_path());
            return err("Cannot write Codex config.");
        }
        Ok(())
    }

    pub fn restore(&self) -> Result<(), ConfigError> {
        let raw = std::fs::read(self.receipt_path()).map_err(|_| ConfigError("Aillies is not installed in Codex.".into()))?;
        let receipt: Receipt = serde_json::from_slice(&raw).map_err(|_| ConfigError("The Codex backup is damaged; restore manually.".into()))?;
        let installed = STANDARD.decode(receipt.installed).map_err(|_| ConfigError("The Codex backup is damaged.".into()))?;
        let original = match receipt.original {
            Some(o) => Some(STANDARD.decode(o).map_err(|_| ConfigError("The Codex backup is damaged.".into()))?),
            None => None,
        };
        let path = self.config_path();
        let current = match std::fs::symlink_metadata(&path) {
            Ok(m) if m.file_type().is_file() => Some(std::fs::read(&path).map_err(|_| ConfigError("Cannot read Codex config.".into()))?),
            Ok(_) => return err("Refusing to replace a linked or non-regular Codex config file."),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return std::fs::remove_file(self.receipt_path()).map_err(|_| ConfigError("Cannot clear the Codex receipt.".into()));
            }
            Err(_) => return err("Cannot read Codex config."),
        };
        let restored: Option<Vec<u8>> = if current.as_deref() == Some(installed.as_slice()) || current == original {
            original
        } else {
            // Codex edits config.toml itself (trusted projects, dismissed
            // notices, model changes). Keep those edits: three-way merge with
            // the installed file as the common base, so only Aillies' own
            // lines are taken out.
            let to_text = |b: &[u8]| String::from_utf8(b.to_vec()).ok();
            let (Some(base), Some(ours)) = (to_text(&installed), to_text(original.as_deref().unwrap_or(b""))) else {
                return err("Codex configuration is not UTF-8; merge it manually.");
            };
            let Some(theirs) = current.as_deref().and_then(to_text) else {
                return err("Codex config.toml was removed or is unreadable; nothing to restore.");
            };
            match merge_restore(&base, &ours, &theirs) {
                Some(text) => (!text.trim().is_empty() || original.is_some()).then(|| text.into_bytes()),
                None => {
                    return err(format!(
                        "Codex settings changed in the same lines Aillies manages. Your backup is kept at {}; merge it manually.",
                        self.receipt_path().display()
                    ))
                }
            }
        };
        match restored {
            Some(o) => private_write(&path, &o).map_err(|_| ConfigError("Cannot write Codex config.".into()))?,
            None => {
                let _ = std::fs::remove_file(&path);
            }
        }
        std::fs::remove_file(self.receipt_path()).map_err(|_| ConfigError("Restored, but could not remove the backup receipt.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_tolerates_removed_or_edited_owned_tables_and_keeps_replaced_values() {
        let before = "model_provider = \"openai\"\nmodel = \"gpt-5.5\"\n";
        let installed = render(before, 18522, "LOCAL_ONLY", None).unwrap();
        let edited = installed
            .replace("api_key_model_discovery = false", "api_key_model_discovery = true")
            .replace("wire_api = \"responses\"", "wire_api = \"responses\"\nuser_option = true");
        let restored = merge_restore(&installed, before, &edited).unwrap();
        assert!(!restored.contains("LOCAL_ONLY"));
        assert!(restored.contains("api_key_model_discovery = true"));
        assert!(restored.contains("model_provider = \"openai\""));
        let removed = edited.split("# Managed by Aillies").next().unwrap();
        let restored = merge_restore(&installed, before, removed).unwrap();
        assert!(restored.contains("model_provider = \"openai\""));
    }

    #[test]
    fn empty_config_gets_provider() {
        let out = render("", 18522, "k", None).unwrap();
        assert!(out.contains("model_provider = \"aillies\"\nmodel = \"gpt-5.5\""));
        assert!(out.contains("[model_providers.aillies]"));
        assert!(out.contains("base_url = \"http://127.0.0.1:18522/v1\""));
        assert!(out.contains("stream_max_retries = 0"));
        assert!(out.contains("api_key_model_discovery = false"));
    }

    #[test]
    fn preserves_unrelated_content_and_existing_model() {
        let original = "# my config\nmodel = \"gpt-5.6-luna\"\nmodel_provider = \"openai\"\napproval_policy = \"on-request\"\n\n[mcp_servers.\"my.server\"]\ncommand = \"x\"\nargs = [\n  \"a\",\n  \"b\", # note\n]\n\n[features]\nfoo = true\n";
        let out = render(original, 18522, "k", None).unwrap();
        assert!(out.contains("# my config\nmodel = \"gpt-5.6-luna\"\nmodel_provider = \"aillies\""));
        assert!(out.contains("[mcp_servers.\"my.server\"]\ncommand = \"x\"\nargs = [\n  \"a\",\n  \"b\", # note\n]"));
        assert!(out.contains("[features]\napi_key_model_discovery = false\nfoo = true"));
        let chosen = render(original, 18522, "k", Some("gpt-5.5")).unwrap();
        assert!(chosen.contains("model = \"gpt-5.5\""));
    }

    #[test]
    fn refuses_ambiguous_configs() {
        assert!(render("x = \"\"\"\nunclosed\n", 18522, "k", None).is_err());
        assert!(render("[model_providers.aillies]\nname=\"x\"\n", 18522, "k", None).is_err());
        assert!(render("model = \"a\"\nmodel = \"b\"\n", 18522, "k", None).is_err());
        assert!(render("model_providers.x.name = \"a\"\n", 18522, "k", None).is_err());
        assert!(render("[features.api_key_model_discovery]\n", 18522, "k", None).is_err());
        assert!(render("a = [\n1,\n", 18522, "k", None).is_err());
        assert!(render("", 80, "k", None).is_err());
    }

    #[test]
    fn multiline_values_preserve_bytes_and_later_edits_on_restore() {
        let original = concat!(
            "model = \"gpt-5.5\"\n",
            "instructions = \"\"\"\nKeep # and = inside text.\n[model_providers.aillies]\n",
            "model_provider = 'not-a-setting'\nEscaped \\\" quotes and \\\n  continuation\n\"\"\"\n",
            "literal = '''\n[mcp_servers.aillies_images]\n# literal \\\n'''\n",
            "nested = [\"\"\"\n[features]\n\"\"\", '''one'''', '''two''''', {value = '''three'''}]\n",
            "# a comment containing \"\"\" and '''\n",
            "[features]\nother = true\n"
        );
        let dir = tempfile::tempdir().unwrap();
        let cfg = CodexConfig { codex_home: dir.path().join("codex"), receipt_dir: dir.path().join("state") };
        std::fs::create_dir_all(&cfg.codex_home).unwrap();
        std::fs::write(cfg.config_path(), original).unwrap();
        cfg.install(18522, "LOCAL_ONLY", None).unwrap();
        let installed = std::fs::read_to_string(cfg.config_path()).unwrap();
        assert!(installed.contains(original.split("[features]").next().unwrap()));
        let mut edited = installed.replace("Keep # and = inside text.", "Later edit # and = inside text.");
        edited.push_str("\n[projects.'local-project']\ntrust_level = 'trusted'\n");
        std::fs::write(cfg.config_path(), &edited).unwrap();
        cfg.restore().unwrap();
        let restored = std::fs::read_to_string(cfg.config_path()).unwrap();
        assert_eq!(
            restored,
            format!(
                "{}\n[projects.'local-project']\ntrust_level = 'trusted'\n",
                original.replace("Keep # and = inside text.", "Later edit # and = inside text.")
            )
        );
        assert!(!restored.contains("LOCAL_ONLY"));
        cfg.install(18522, "LOCAL_ONLY", None).unwrap();
        cfg.restore().unwrap();
        assert_eq!(std::fs::read_to_string(cfg.config_path()).unwrap(), restored);
    }

    #[test]
    fn multiline_delimiters_and_escapes_do_not_create_statements() {
        for value in [
            "\"\"\"\ntext with \\\"\"\" not a delimiter\n\"\"\"",
            "\"\"\"\ntext ending in a quote\"\"\"\"",
            "\"\"\"\ntext ending in two quotes\"\"\"\"\"",
            "\"\"\"\nline continuation \\\n\"\"\"",
            "'''\nbackslash is literal \\\n'''",
            "\"contains ''' in a basic string\"",
        ] {
            let original = format!("model = 'gpt-5.5'\nnotes = {value}\n# ''' and \"\"\" in a comment\n");
            let installed = render(&original, 18522, "LOCAL_ONLY", None).unwrap();
            let current = format!("{installed}\n[later]\nkeep = true\n");
            let restored = merge_restore(&installed, &original, &current).unwrap();
            assert!(restored.starts_with(&original));
            assert!(restored.contains("[later]\nkeep = true"));
            assert!(!restored.contains("LOCAL_ONLY"));
        }
    }

    #[test]
    fn install_restore_round_trip_and_conflict_guard() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = CodexConfig { codex_home: dir.path().join(".codex"), receipt_dir: dir.path().join("state") };
        std::fs::create_dir_all(&cfg.codex_home).unwrap();
        std::fs::write(cfg.config_path(), "model = \"gpt-5.5\"\n").unwrap();
        cfg.install(18522, "key", None).unwrap();
        assert!(cfg.is_installed());
        assert!(cfg.install(18522, "key", None).is_err());
        cfg.restore().unwrap();
        assert_eq!(std::fs::read_to_string(cfg.config_path()).unwrap(), "model = \"gpt-5.5\"\n");

        cfg.install(18522, "key", None).unwrap();
        std::fs::write(cfg.config_path(), "edited by user\n").unwrap();
        assert!(cfg.restore().is_err());
        assert_eq!(std::fs::read_to_string(cfg.config_path()).unwrap(), "edited by user\n");
    }

    #[test]
    fn restore_keeps_edits_codex_made_later() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = CodexConfig { codex_home: dir.path().join(".codex"), receipt_dir: dir.path().join("state") };
        std::fs::create_dir_all(&cfg.codex_home).unwrap();
        let original = "model = \"gpt-5.5\"\napproval_policy = \"on-request\"\n";
        std::fs::write(cfg.config_path(), original).unwrap();
        cfg.install(18522, "key", None).unwrap();
        // What the Codex app does on its own: trust a project, dismiss a notice.
        let mut text = std::fs::read_to_string(cfg.config_path()).unwrap();
        // Codex (toml_edit) changes values in place and appends new tables.
        text = text.replace("approval_policy = \"on-request\"", "approval_policy = \"never\"");
        text.push_str("\n[notice]\nhide_full_access_warning = true\n\n[projects.\"/Users/me/app\"]\ntrust_level = \"trusted\"\n");
        std::fs::write(cfg.config_path(), &text).unwrap();
        cfg.restore().unwrap();
        let out = std::fs::read_to_string(cfg.config_path()).unwrap();
        assert!(!out.contains("aillies"), "{out}");
        assert!(out.contains("model = \"gpt-5.5\""));
        assert!(out.contains("approval_policy = \"never\""));
        assert!(!out.contains("[features]"));
        assert!(out.contains("hide_full_access_warning = true"));
        assert!(out.contains("trust_level = \"trusted\""));
        assert!(!cfg.is_installed());
    }

    #[test]
    fn restore_refuses_what_it_cannot_parse() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = CodexConfig { codex_home: dir.path().join(".codex"), receipt_dir: dir.path().join("state") };
        std::fs::create_dir_all(&cfg.codex_home).unwrap();
        std::fs::write(cfg.config_path(), "model = \"gpt-5.5\"\n").unwrap();
        cfg.install(18522, "key", None).unwrap();
        let mut text = std::fs::read_to_string(cfg.config_path()).unwrap();
        text.push_str("notes = '''\nunclosed\n");
        std::fs::write(cfg.config_path(), &text).unwrap();
        assert!(cfg.restore().is_err());
        assert_eq!(std::fs::read_to_string(cfg.config_path()).unwrap(), text);
        assert!(cfg.is_installed());
    }

    #[test]
    fn restore_handles_provider_quotes_tabs_and_comments() {
        for provider in ["model_provider = 'aillies'", "model_provider\t=\t\"aillies\" # selected"] {
            let dir = tempfile::tempdir().unwrap();
            let cfg = CodexConfig { codex_home: dir.path().join("codex"), receipt_dir: dir.path().join("state") };
            std::fs::create_dir_all(&cfg.codex_home).unwrap();
            let original = "model_provider = \"previous\"\nmodel = \"gpt-5.5\"\n";
            std::fs::write(cfg.config_path(), original).unwrap();
            cfg.install(18522, "key", None).unwrap();
            let current = std::fs::read_to_string(cfg.config_path()).unwrap().replace("model_provider = \"aillies\"", provider);
            std::fs::write(cfg.config_path(), current).unwrap();
            cfg.restore().unwrap();
            let restored = std::fs::read_to_string(cfg.config_path()).unwrap();
            assert!(restored.contains("model_provider = \"previous\""), "{restored}");
            assert!(!restored.contains("aillies"), "{restored}");
        }
    }

    #[test]
    fn restore_preserves_a_later_provider_selection() {
        let original = "model = \"gpt-5.5\"\n";
        let installed = render(original, 18522, "key", None).unwrap();
        let current = installed.replace("model_provider = \"aillies\"", "model_provider = 'other'");
        let restored = merge_restore(&installed, original, &current).unwrap();
        assert!(restored.contains("model_provider = 'other'"));
        assert!(!restored.contains("aillies"));
    }

    #[test]
    fn restore_refuses_invalid_toml_values() {
        let original = "model = \"gpt-5.5\"\n";
        let installed = render(original, 18522, "key", None).unwrap();
        let current = installed.replace("model_provider = \"aillies\"", "model_provider = aillies");
        assert!(merge_restore(&installed, original, &current).is_none());
    }

    #[test]
    fn image_tool_is_installed_and_removed_without_touching_other_mcp_servers() {
        let original = "model = \"gpt-5.5\"\n[mcp_servers.other]\nurl = \"http://localhost:9000/mcp\"\n";
        let installed = render(original, 18522, "key", None).unwrap();
        assert!(installed.contains("[mcp_servers.aillies_images]"));
        assert!(installed.contains("http://127.0.0.1:18522/mcp/images"));
        let current = format!("{installed}\n[notice]\nhide_warning = true\n");
        let restored = merge_restore(&installed, original, &current).unwrap();
        assert!(!restored.contains("aillies"));
        assert!(restored.contains("[mcp_servers.other]"));
        assert!(restored.contains("hide_warning = true"));
    }

    #[cfg(unix)]
    #[test]
    fn linked_config_is_refused_without_changing_target() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = CodexConfig { codex_home: dir.path().join("codex"), receipt_dir: dir.path().join("state") };
        std::fs::create_dir_all(&cfg.codex_home).unwrap();
        let target = dir.path().join("target.toml");
        std::fs::write(&target, "model = 'gpt-5.5'\n").unwrap();
        std::os::unix::fs::symlink(&target, cfg.config_path()).unwrap();
        assert!(cfg.install(18522, "key", None).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "model = 'gpt-5.5'\n");
        assert!(!cfg.is_installed());
    }

    #[test]
    fn failed_config_write_removes_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = CodexConfig { codex_home: dir.path().join("codex"), receipt_dir: dir.path().join("state") };
        std::fs::create_dir_all(cfg.codex_home.join(".config.toml.aillies-tmp")).unwrap();
        assert!(cfg.install(18522, "key", None).is_err());
        assert!(!cfg.is_installed());
        assert!(!cfg.config_path().exists());
    }

    #[test]
    fn missing_config_restores_to_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = CodexConfig { codex_home: dir.path().join(".codex"), receipt_dir: dir.path().join("state") };
        cfg.install(18522, "key", Some("gpt-5.5")).unwrap();
        assert!(cfg.config_path().exists());
        cfg.restore().unwrap();
        assert!(!cfg.config_path().exists());
    }
}
