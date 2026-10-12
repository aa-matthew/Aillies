//! Own only a small set of user-level environment values. Never touch transcripts.
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const OWNED: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_MAX_RETRIES",
    "API_TIMEOUT_MS",
    "CLAUDE_CODE_GZIP_REQUEST_BODIES",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
];

pub struct Config {
    pub home: PathBuf,
    pub receipts: PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u32,
    target: PathBuf,
    absent: bool,
    env_absent: bool,
    before: BTreeMap<String, Option<Value>>,
    installed: BTreeMap<String, Value>,
}

pub(crate) fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    use std::io::Read;
    match std::fs::symlink_metadata(path) {
        Ok(m) => {
            ensure!(m.is_file() && !m.file_type().is_symlink() && m.len() <= 1024 * 1024, "Claude settings need attention. No changes were made.");
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => bail!("Cannot read Claude settings. No changes were made."),
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024 * 1024, "Claude settings are too large.");
    Ok(Some(bytes))
}
pub fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or_else(|| anyhow::anyhow!("Cannot locate the settings folder."))?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".{}.{}.aillies-tmp", path.file_name().and_then(|s| s.to_str()).unwrap_or("settings"), uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    let result = (|| -> Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
fn settings(bytes: Option<&[u8]>) -> Result<Map<String, Value>> {
    let value = match bytes {
        Some(bytes) => serde_json::from_slice(bytes).map_err(|_| anyhow::anyhow!("Claude settings are not valid JSON. No changes were made."))?,
        None => json!({}),
    };
    let mut object = value.as_object().cloned().ok_or_else(|| anyhow::anyhow!("Claude settings must be an object. No changes were made."))?;
    if let Some(env) = object.get("env") {
        ensure!(env.is_object(), "Claude environment settings need attention. No changes were made.");
    }
    object.entry("env").or_insert_with(|| json!({}));
    Ok(object)
}
impl Config {
    /// Effective original upstream auth while our local gateway token is installed.
    /// Never expose this map through IPC, status or diagnostics.
    pub(crate) fn auth_env(&self) -> Result<Map<String, Value>> {
        let path = self.home.join("settings.json");
        let bytes = read(&path)?;
        let object = settings(bytes.as_deref())?;
        let mut env = object["env"].as_object().cloned().ok_or_else(|| anyhow::anyhow!("Claude settings need attention."))?;
        if let Some(bytes) = read(&self.receipt())? {
            let receipt: Receipt = serde_json::from_slice(&bytes)?;
            ensure!(receipt.version == 1 && receipt.target == path, "Claude settings folder changed.");
            for (key, value) in &receipt.installed {
                if env.get(key) == Some(value) {
                    match receipt.before.get(key).and_then(Option::as_ref) {
                        Some(before) => {
                            env.insert(key.clone(), before.clone());
                        }
                        None => {
                            env.remove(key);
                        }
                    }
                }
            }
        }
        Ok(env)
    }
    fn receipt(&self) -> PathBuf {
        self.receipts.join("claude-restore.json")
    }
    pub fn is_installed(&self) -> bool {
        self.receipt().exists()
    }
    pub fn install(&self, port: u16, token: &str) -> Result<()> {
        let path = self.home.join("settings.json");
        ensure!(path.is_absolute(), "Claude settings need an absolute home folder.");
        let bytes = read(&path)?;
        let mut object = settings(bytes.as_deref())?;
        let env_absent = bytes.as_ref().is_none_or(|b| serde_json::from_slice::<Value>(b).ok().is_none_or(|v| v.get("env").is_none()));
        let env = object.get_mut("env").and_then(Value::as_object_mut).ok_or_else(|| anyhow::anyhow!("Claude settings need attention."))?;
        for backend in ["CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX", "CLAUDE_CODE_USE_FOUNDRY"] {
            ensure!(
                !env.get(backend).and_then(Value::as_str).is_some_and(|v| v == "1" || v == "true"),
                "Turn off Claude's alternate backend before using Aillies."
            );
        }
        let installed: BTreeMap<String, Value> = [
            ("ANTHROPIC_BASE_URL", json!(format!("http://127.0.0.1:{port}"))),
            ("ANTHROPIC_AUTH_TOKEN", json!(token)),
            ("ANTHROPIC_API_KEY", json!("")),
            ("CLAUDE_CODE_OAUTH_TOKEN", json!("")),
            ("CLAUDE_CODE_MAX_RETRIES", json!("0")),
            ("API_TIMEOUT_MS", json!("900000")),
            ("CLAUDE_CODE_GZIP_REQUEST_BODIES", json!("0")),
            ("CLAUDE_CODE_USE_BEDROCK", json!("0")),
            ("CLAUDE_CODE_USE_VERTEX", json!("0")),
            ("CLAUDE_CODE_USE_FOUNDRY", json!("0")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        let receipt = if let Some(b) = read(&self.receipt())? {
            let receipt: Receipt =
                serde_json::from_slice(&b).map_err(|_| anyhow::anyhow!("Claude's restore receipt needs attention. No changes were made."))?;
            ensure!(receipt.version == 1 && receipt.target == path && receipt.installed == installed, "Restore Claude's previous Aillies settings first.");
            ensure!(installed.iter().all(|(k, v)| env.get(k) == Some(v)), "Claude settings changed. Switch to Normal before enabling again.");
            return Ok(());
        } else {
            Receipt {
                version: 1,
                target: path.clone(),
                absent: bytes.is_none(),
                env_absent,
                before: installed.keys().map(|k| (k.clone(), env.get(k).cloned())).collect(),
                installed,
            }
        };
        for (key, value) in &receipt.installed {
            env.insert(key.clone(), value.clone());
        }
        private_write(&self.receipt(), &serde_json::to_vec_pretty(&receipt)?)?;
        // Receipt precedes mutation: restore also repairs a failed install.
        private_write(&path, &serde_json::to_vec_pretty(&object)?).map_err(|_| anyhow::anyhow!("Could not save Claude settings. Switch to Normal to recover."))
    }
    pub fn restore(&self) -> Result<()> {
        let Some(bytes) = read(&self.receipt())? else { return Ok(()) };
        let receipt: Receipt =
            serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("Claude's restore receipt needs attention. No changes were made."))?;
        ensure!(
            receipt.version == 1
                && receipt.before.keys().eq(receipt.installed.keys())
                && (receipt.installed.len() == OWNED.len()
                    || receipt.installed.len() == OWNED.len() - 1 && !receipt.installed.contains_key("CLAUDE_CODE_OAUTH_TOKEN"))
                && receipt.installed.keys().all(|k| OWNED.contains(&k.as_str())),
            "Claude's restore receipt needs attention."
        );
        let path = self.home.join("settings.json");
        ensure!(path.is_absolute(), "Claude settings need an absolute home folder.");
        ensure!(receipt.target == path, "Claude settings folder changed. Restore using the original CLAUDE_CONFIG_DIR.");
        let current = read(&path)?;
        if current.is_some() {
            let mut object = settings(current.as_deref())?;
            let env = object.get_mut("env").and_then(Value::as_object_mut).ok_or_else(|| anyhow::anyhow!("Claude settings need attention."))?;
            for (key, installed) in &receipt.installed {
                if env.get(key) == Some(installed) {
                    match &receipt.before[key] {
                        Some(value) => {
                            env.insert(key.clone(), value.clone());
                        }
                        None => {
                            env.remove(key);
                        }
                    }
                }
            }
            if receipt.env_absent && env.is_empty() {
                object.remove("env");
            }
            if receipt.absent && object.is_empty() {
                std::fs::remove_file(path)?;
            } else {
                private_write(&path, &serde_json::to_vec_pretty(&object)?)?;
            }
        }
        std::fs::remove_file(self.receipt())?;
        Ok(())
    }
}
#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
