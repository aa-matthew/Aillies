//! Persistent state under the Aillies home directory. Every file is 0600.
//!
//! `state.json`   settings, paired devices, approved devices (no secrets)
//! `secrets.json` local Codex client key and control token
//! TODO(0.9): move secrets into Keychain / Credential Manager / Secret Service.

use anyhow::{Context, Result};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn private_write(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let tmp = dir.join(format!(".{}.tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("f")));
    {
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        use std::io::Write;
        let mut f = o.open(&tmp).with_context(|| format!("write {}", tmp.display()))?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn random_hex(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut b);
    hex::encode(b)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum Destination {
    /// Use this device's own Codex login.
    #[default]
    Local,
    /// Send through a paired device (by fingerprint).
    Peer(String),
}

/// A device this one can send through.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRecord {
    pub fp: String,
    pub name: String,
    pub platform: String,
    pub addrs: Vec<String>,
    pub paired_at: u64,
    #[serde(default)]
    pub fast_mode: bool,
}

fn codex_grant_default() -> bool {
    true
}

/// A device allowed to use this device's account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRecord {
    pub fp: String,
    pub name: String,
    pub platform: String,
    pub approved_at: u64,
    #[serde(default)]
    pub last_used: Option<u64>,
    #[serde(default)]
    pub claude: bool,
    #[serde(default = "codex_grant_default")]
    pub codex: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeState {
    #[serde(default)]
    pub connected_account: Option<String>,
    #[serde(default)]
    pub account_source: ClaudeAccountSource,
    pub sharing: bool,
    pub destination: Destination,
    #[serde(default)]
    pub shared_account: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeAccountSource {
    #[default]
    Code,
    Desktop,
    Connected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub claude: ClaudeState,
    pub version: u32,
    pub name: String,
    pub routing: bool,
    pub sharing: bool,
    #[serde(default)]
    pub allow_fast_mode: bool,
    #[serde(default)]
    pub local_fast_mode: bool,
    pub destination: Destination,
    /// Account id approved for sharing; a different login fails closed.
    pub shared_account: Option<String>,
    pub peers: Vec<PeerRecord>,
    pub clients: Vec<ClientRecord>,
}

impl State {
    fn fresh(name: String) -> Self {
        Self {
            claude: ClaudeState::default(),
            version: 1,
            name,
            routing: true,
            sharing: false,
            destination: Destination::Local,
            shared_account: None,
            peers: vec![],
            clients: vec![],
            allow_fast_mode: false,
            local_fast_mode: false,
        }
    }
    pub fn peer(&self, fp: &str) -> Option<&PeerRecord> {
        self.peers.iter().find(|p| p.fp == fp)
    }
    pub fn client(&self, fp: &str) -> Option<&ClientRecord> {
        self.clients.iter().find(|c| c.fp == fp)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secrets {
    /// Bearer Codex uses on the loopback `/v1/responses`.
    pub codex_key: String,
    #[serde(default)]
    pub claude_key: String,
    /// Bearer the CLI uses on `/control/*`.
    pub control_token: String,
    pub loopback_port: u16,
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    fn state_path(&self) -> PathBuf {
        self.dir.join("state.json")
    }
    pub fn secrets_path(dir: &Path) -> PathBuf {
        dir.join("secrets.json")
    }

    pub fn load_state(&self, default_name: &str) -> Result<State> {
        match std::fs::read(self.state_path()) {
            Ok(b) => {
                let mut state: State = serde_json::from_slice(&b).context("state.json is damaged")?;
                if state.claude.sharing && state.claude.shared_account.is_none() {
                    // Alpha.1 API-key consent never authorizes a native account.
                    state.claude.sharing = false;
                    self.save_state(&state)?;
                }
                Ok(state)
            }
            Err(_) => {
                let s = State::fresh(default_name.to_string());
                self.save_state(&s)?;
                Ok(s)
            }
        }
    }

    pub fn save_state(&self, s: &State) -> Result<()> {
        private_write(&self.state_path(), &serde_json::to_vec_pretty(s)?)
    }

    pub fn load_or_create_secrets(&self, loopback_port: u16) -> Result<Secrets> {
        let p = Self::secrets_path(&self.dir);
        if let Ok(b) = std::fs::read(&p) {
            let mut s: Secrets = serde_json::from_slice(&b).context("secrets.json is damaged")?;
            if s.loopback_port != loopback_port || s.claude_key.is_empty() {
                if s.claude_key.is_empty() {
                    s.claude_key = format!("aillies-claude-{}", random_hex(24));
                }
                s.loopback_port = loopback_port;
                private_write(&p, &serde_json::to_vec_pretty(&s)?)?;
            }
            return Ok(s);
        }
        let s = Secrets {
            claude_key: format!("aillies-claude-{}", random_hex(24)),
            codex_key: format!("aillies-local-{}", random_hex(24)),
            control_token: random_hex(32),
            loopback_port,
        };
        private_write(&p, &serde_json::to_vec_pretty(&s)?)?;
        Ok(s)
    }

    pub fn read_secrets(dir: &Path) -> Result<Secrets> {
        let b = std::fs::read(Self::secrets_path(dir)).context("Aillies has not run on this device yet. Start it with `aillies run` or open the app.")?;
        Ok(serde_json::from_slice(&b)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_state_stays_codex_only_and_tokens_migrate_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_owned());
        let old = serde_json::json!({"version":1,"name":"Old","routing":true,"sharing":false,"destination":{"kind":"local"},"shared_account":null,"peers":[],"clients":[{"fp":"client","name":"Client","platform":"macos","approved_at":1}]});
        private_write(&store.state_path(), &serde_json::to_vec(&old).unwrap()).unwrap();
        let state = store.load_state("unused").unwrap();
        assert!(!state.claude.sharing);
        assert!(!state.clients[0].claude);
        assert!(state.clients[0].codex);
        let old = serde_json::json!({"codex_key":"unchanged","control_token":"control","loopback_port":12345});
        private_write(&Store::secrets_path(dir.path()), &serde_json::to_vec(&old).unwrap()).unwrap();
        let first = store.load_or_create_secrets(12345).unwrap();
        let next = store.load_or_create_secrets(12345).unwrap();
        assert_eq!(first.codex_key, "unchanged");
        assert!(!first.claude_key.is_empty());
        assert_ne!(first.codex_key, first.claude_key);
        assert_eq!(first.claude_key, next.claude_key);
    }
    #[test]
    fn api_key_consent_does_not_authorize_a_native_account() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_owned());
        let state = store.load_state("Test").unwrap();
        let mut old = serde_json::to_value(state).unwrap();
        old["claude"] = serde_json::json!({"sharing":true,"destination":{"kind":"local"},"shared_key_digest":"old-key-digest"});
        private_write(&store.state_path(), &serde_json::to_vec(&old).unwrap()).unwrap();
        let migrated = store.load_state("unused").unwrap();
        assert!(!migrated.claude.sharing);
        assert!(migrated.claude.shared_account.is_none());
        assert!(!store.load_state("unused").unwrap().claude.sharing);
    }
}
