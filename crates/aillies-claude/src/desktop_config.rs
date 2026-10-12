//! Native desktop config library. Own a separate entry; never edit login/history.
use crate::config::{private_write, read};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub struct DesktopConfig {
    pub library: PathBuf,
    pub receipts: PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u32,
    target: PathBuf,
    id: String,
    before: Option<Value>,
    installed: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending: Option<Value>,
}
fn discovery(mut profile: Value) -> Result<Value> {
    let object = profile.as_object_mut().ok_or_else(|| anyhow::anyhow!("Claude desktop restore receipt needs attention."))?;
    object.insert("modelDiscoveryEnabled".into(), true.into());
    object.remove("inferenceModels");
    Ok(profile)
}
fn matches_profile(r: &Receipt, bytes: &[u8]) -> bool {
    serde_json::from_slice::<Value>(bytes).is_ok_and(|v| v == r.installed || r.pending.as_ref() == Some(&v))
}
fn object(bytes: Option<&[u8]>) -> Result<Map<String, Value>> {
    let value = match bytes {
        Some(b) => serde_json::from_slice(b).map_err(|_| anyhow::anyhow!("Claude desktop configuration needs attention. No changes were made."))?,
        None => json!({"appliedId":"","entries":[]}),
    };
    let object = value.as_object().cloned().ok_or_else(|| anyhow::anyhow!("Claude desktop configuration must be an object."))?;
    let entries = object.get("entries").and_then(Value::as_array).ok_or_else(|| anyhow::anyhow!("Claude desktop configurations need attention."))?;
    let selected = object.get("appliedId").and_then(Value::as_str).ok_or_else(|| anyhow::anyhow!("Claude desktop selection needs attention."))?;
    let mut ids = std::collections::HashSet::new();
    ensure!(
        entries.iter().all(|e| e["id"].as_str().is_some_and(|id| uuid::Uuid::parse_str(id).is_ok() && ids.insert(id)))
            && (selected.is_empty() || ids.contains(selected)),
        "Claude desktop selection needs attention."
    );
    ensure!(!object.contains_key("hybridPointer"), "Claude desktop is managed by a bootstrap connection. Use its configuration window.");
    Ok(object)
}
impl DesktopConfig {
    fn meta(&self) -> PathBuf {
        self.library.join("_meta.json")
    }
    fn receipt(&self) -> PathBuf {
        self.receipts.join("claude-desktop-restore.json")
    }
    fn profile(&self, id: &str) -> Result<PathBuf> {
        ensure!(uuid::Uuid::parse_str(id).is_ok(), "Claude desktop restore receipt needs attention.");
        Ok(self.library.join(format!("{id}.json")))
    }
    fn check(&self) -> Result<()> {
        ensure!(self.library.is_absolute(), "Claude desktop needs an absolute configuration folder.");
        // Refuse redirected parent paths before creating files.
        for parent in self.library.ancestors().take(2) {
            if let Ok(meta) = std::fs::symlink_metadata(parent) {
                ensure!(meta.is_dir() && !meta.file_type().is_symlink(), "Claude desktop configuration folder needs attention.");
            }
        }
        Ok(())
    }
    pub fn is_installed(&self) -> bool {
        self.receipt().exists()
    }
    fn load_receipt(&self) -> Result<Option<Receipt>> {
        let Some(bytes) = read(&self.receipt())? else { return Ok(None) };
        let r: Receipt = serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("Claude desktop restore receipt needs attention."))?;
        ensure!(r.version == 1 && r.target == self.library, "Restore Claude desktop using its original configuration folder.");
        self.profile(&r.id)?;
        ensure!(r.pending.as_ref().is_none_or(|p| discovery(r.installed.clone()).ok().as_ref() == Some(p)), "Claude desktop restore receipt needs attention.");
        Ok(Some(r))
    }
    /// Upgrade only the exact Aillies-owned profile. A receipt intent makes either
    /// side of an interrupted replacement safe to resume or restore.
    pub fn upgrade_discovery(&self) -> Result<bool> {
        let Some(mut r) = self.load_receipt()? else { return Ok(false) };
        self.check()?;
        object(read(&self.meta())?.as_deref())?;
        let profile = read(&self.profile(&r.id)?)?.ok_or_else(|| anyhow::anyhow!("Claude desktop configuration needs attention."))?;
        ensure!(matches_profile(&r, &profile), "Aillies' Claude desktop configuration was edited. No changes were made.");
        let desired = discovery(r.installed.clone())?;
        if desired == r.installed && r.pending.is_none() {
            return Ok(false);
        }
        r.pending = Some(desired);
        private_write(&self.receipt(), &serde_json::to_vec_pretty(&r)?)?;
        private_write(&self.profile(&r.id)?, &serde_json::to_vec_pretty(&r.pending)?)?;
        r.installed = r.pending.take().ok_or_else(|| anyhow::anyhow!("Claude desktop configuration needs attention."))?;
        private_write(&self.receipt(), &serde_json::to_vec_pretty(&r)?)?;
        Ok(true)
    }
    pub fn install(&self, port: u16, token: &str) -> Result<()> {
        self.check()?;
        let bytes = read(&self.meta())?;
        let mut meta = object(bytes.as_deref())?;
        if let Some(r) = self.load_receipt()? {
            ensure!(
                meta["appliedId"] == r.id
                    && r.installed["inferenceGatewayBaseUrl"] == format!("http://127.0.0.1:{port}/claude-desktop")
                    && r.installed["inferenceGatewayApiKey"] == token
                    && read(&self.profile(&r.id)?)?.is_some_and(|b| matches_profile(&r, &b)),
                "Claude desktop settings changed. Switch to Normal before enabling again."
            );
            self.upgrade_discovery()?;
            return Ok(());
        }
        let id = uuid::Uuid::new_v4().to_string();
        ensure!(read(&self.profile(&id)?)?.is_none(), "Claude desktop configuration already exists.");
        let installed = json!({"deploymentDisplayName":"Aillies","inferenceProvider":"gateway","inferenceGatewayBaseUrl":format!("http://127.0.0.1:{port}/claude-desktop"),"inferenceGatewayApiKey":token,"inferenceGatewayAuthScheme":"bearer","inferenceCredentialKind":"static","disableDeploymentModeChooser":true,"chatTabEnabled":true,"coworkTabEnabled":false,"isClaudeCodeForDesktopEnabled":false,"modelDiscoveryEnabled":true});
        let r = Receipt {
            version: 1,
            target: self.library.clone(),
            id: id.clone(),
            before: bytes.map(|b| serde_json::from_slice(&b)).transpose()?,
            installed,
            pending: None,
        };
        meta.get_mut("entries")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow::anyhow!("Claude desktop configurations need attention."))?
            .push(json!({"id":id,"name":"Aillies"}));
        meta.insert("appliedId".into(), json!(id));
        private_write(&self.receipt(), &serde_json::to_vec_pretty(&r)?)?;
        private_write(&self.profile(&r.id)?, &serde_json::to_vec_pretty(&r.installed)?)?;
        private_write(&self.meta(), &serde_json::to_vec_pretty(&meta)?)?;
        Ok(())
    }
    pub fn restore(&self) -> Result<()> {
        self.check()?;
        let Some(r) = self.load_receipt()? else { return Ok(()) };
        let path = self.profile(&r.id)?;
        let profile = read(&path)?;
        // The receipt owns this UUID, not the entire current profile. Remove our
        // gateway even after edits, retaining new or changed non-routing values.
        let kept_profile = if let Some(bytes) = &profile {
            let mut value: Map<String, Value> =
                serde_json::from_slice(bytes).map_err(|_| anyhow::anyhow!("Claude desktop profile cannot be read safely. Recovery files were kept."))?;
            for (key, installed) in r.installed.as_object().ok_or_else(|| anyhow::anyhow!("Claude desktop restore receipt needs attention."))? {
                if key.starts_with("inferenceGateway") || key == "inferenceProvider" || key == "inferenceCredentialKind" || value.get(key) == Some(installed) {
                    value.remove(key);
                }
            }
            (!value.is_empty()).then(|| serde_json::to_vec_pretty(&value)).transpose()?
        } else {
            None
        };
        let current = read(&self.meta())?;
        if current.is_some() {
            let mut meta = object(current.as_deref())?;
            if meta["appliedId"] == r.id {
                let previous = r.before.as_ref().and_then(|b| b.get("appliedId")).and_then(Value::as_str).unwrap_or("");
                let present = meta["entries"].as_array().is_some_and(|entries| entries.iter().any(|e| e["id"] == previous));
                meta.insert("appliedId".into(), json!(if present { previous } else { "" }));
            }
            if let Some(entries) = meta.get_mut("entries").and_then(Value::as_array_mut) {
                entries.retain(|e| e["id"] != r.id);
            }
            // Validate restoration before any mutation, including externally removed selections.
            let encoded = serde_json::to_vec_pretty(&meta)?;
            object(Some(&encoded))?;
            if r.before.is_none() && meta == object(None)? {
                std::fs::remove_file(self.meta())?;
            } else {
                private_write(&self.meta(), &encoded)?;
            }
        }
        if let Some(bytes) = kept_profile {
            private_write(&path, &bytes)?;
        } else if profile.is_some() {
            std::fs::remove_file(path)?;
        }
        std::fs::remove_file(self.receipt())?;
        Ok(())
    }
}
#[cfg(test)]
#[path = "desktop_config_tests.rs"]
mod tests;
