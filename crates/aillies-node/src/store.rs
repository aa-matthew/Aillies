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

/// A device allowed to use this device's account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRecord {
    pub fp: String,
    pub name: String,
    pub platform: String,
    pub approved_at: u64,
    #[serde(default)]
    pub last_used: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
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
            Ok(b) => serde_json::from_slice(&b).context("state.json is damaged"),
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
            if s.loopback_port != loopback_port {
                s.loopback_port = loopback_port;
                private_write(&p, &serde_json::to_vec_pretty(&s)?)?;
            }
            return Ok(s);
        }
        let s = Secrets { codex_key: format!("aillies-local-{}", random_hex(24)), control_token: random_hex(32), loopback_port };
        private_write(&p, &serde_json::to_vec_pretty(&s)?)?;
        Ok(s)
    }

    pub fn read_secrets(dir: &Path) -> Result<Secrets> {
        let b = std::fs::read(Self::secrets_path(dir)).context("Aillies has not run on this device yet. Start it with `aillies run` or open the app.")?;
        Ok(serde_json::from_slice(&b)?)
    }
}
