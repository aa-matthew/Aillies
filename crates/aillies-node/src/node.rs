//! Node orchestration shared by the CLI daemon and the desktop shell.

use crate::identity::Identity;
use crate::peer_client;
use crate::store::{now, random_hex, ClientRecord, Destination, PeerRecord, Secrets, State, Store};
use aillies_codex::config::CodexConfig;
use aillies_codex::{default_auth_path, Executor};
use aillies_core::protocol::{
    self, short_id, valid_name, verification_code, AccountReadiness, Invite, PairRequest, PairState, PeerStatus, DEFAULT_LOOPBACK_PORT, DEFAULT_PEER_PORT,
    INVITE_TTL_SECS, PROTOCOL_VERSION,
};
use aillies_core::{Failure, FailureCode};
use anyhow::{anyhow, bail, Result};
use bytes::Bytes;
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{watch, Semaphore};
use tokio::task::JoinHandle;

pub const MAX_CONCURRENT_TURNS: usize = 4;
const PENDING_TTL: u64 = 600;

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub home: PathBuf,
    pub loopback_port: u16,
    pub peer_port: u16,
    pub codex_auth_path: Option<PathBuf>,
    pub codex_home: Option<PathBuf>,
    /// Loopback-only upstream override for tests.
    pub codex_upstream: Option<String>,
    /// Add 127.0.0.1 to invites (two instances on one machine).
    pub advertise_loopback: bool,
}

pub fn default_home() -> PathBuf {
    if let Some(h) = std::env::var_os("AILLIES_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(h);
    }
    directories::ProjectDirs::from("is", "anyax", "Aillies").map(|d| d.config_dir().to_path_buf()).unwrap_or_else(|| PathBuf::from(".aillies"))
}

fn env_port(name: &str, default: u16) -> u16 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

impl NodeConfig {
    pub fn from_env() -> Self {
        Self {
            home: default_home(),
            loopback_port: env_port("AILLIES_LOOPBACK_PORT", DEFAULT_LOOPBACK_PORT),
            peer_port: env_port("AILLIES_PEER_PORT", DEFAULT_PEER_PORT),
            codex_auth_path: std::env::var_os("AILLIES_CODEX_AUTH").map(PathBuf::from),
            codex_home: None,
            codex_upstream: aillies_codex::executor::upstream_override(std::env::var("AILLIES_CODEX_UPSTREAM").ok()),
            advertise_loopback: std::env::var("AILLIES_ADVERTISE_LOOPBACK").is_ok(),
        }
    }
}

pub fn platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        "linux" => "linux",
        other => other,
    }
}

struct InviteSlot {
    secret: String,
    expires: u64,
}

struct PendingPair {
    client_fp: String,
    name: String,
    platform: String,
    code: String,
    created: u64,
    decision: Option<bool>,
}

struct OutgoingPair {
    remote_id: String,
    invite: Invite,
    code: String,
    created: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerView {
    pub fp: String,
    pub short_id: String,
    pub name: String,
    pub platform: String,
    pub addrs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientView {
    pub fp: String,
    pub short_id: String,
    pub name: String,
    pub platform: String,
    pub approved_at: u64,
    pub last_used: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingView {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub code: String,
    pub short_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureView {
    pub code: FailureCode,
    pub message: String,
    pub at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub name: String,
    pub platform: String,
    pub short_id: String,
    pub fingerprint: String,
    pub version: String,
    /// ready | paused | local | action
    pub state: String,
    pub routing: bool,
    pub sharing: bool,
    pub destination: Destination,
    pub destination_name: String,
    pub peers: Vec<PeerView>,
    pub clients: Vec<ClientView>,
    pub pending: Vec<PendingView>,
    pub codex_installed: bool,
    pub local_account: AccountReadiness,
    pub last_failure: Option<FailureView>,
    pub loopback_port: u16,
    pub peer_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerHealth {
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub addr: Option<String>,
    pub status: Option<PeerStatus>,
    pub failure: Option<FailureCode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairStarted {
    pub id: String,
    pub code: String,
    pub name: String,
    pub platform: String,
    pub short_id: String,
}

pub struct Node {
    me: std::sync::Weak<Node>,
    pub(crate) cfg: NodeConfig,
    store: Store,
    state: Mutex<State>,
    pub(crate) secrets: Secrets,
    pub(crate) identity: Identity,
    invites: Mutex<Vec<InviteSlot>>,
    pending: Mutex<HashMap<String, PendingPair>>,
    outgoing: Mutex<HashMap<String, OutgoingPair>>,
    last_failure: Mutex<Option<FailureView>>,
    pub(crate) epoch: watch::Sender<u64>,
    listener: Mutex<Option<(JoinHandle<()>, u16)>>,
    loopback_task: Mutex<Option<JoinHandle<()>>>,
    loopback_port: AtomicU16,
    pub(crate) turns: Arc<Semaphore>,
    host_exec: Mutex<Option<Arc<Executor>>>,
    local_exec: Mutex<Option<Arc<Executor>>>,
}

impl Node {
    /// Load state, then bind the loopback API (and the peer listener if sharing).
    pub async fn start(cfg: NodeConfig) -> Result<Arc<Self>> {
        std::fs::create_dir_all(&cfg.home)?;
        let store = Store::new(cfg.home.clone());
        let hostname = gethostname::gethostname().to_string_lossy().trim_end_matches(".local").to_string();
        let default_name = if valid_name(&hostname) { hostname } else { "My device".to_string() };
        let state = store.load_state(&default_name)?;
        let identity = Identity::load_or_create(&cfg.home)?;
        let (epoch, _) = watch::channel(0u64);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", cfg.loopback_port))
            .await
            .map_err(|e| anyhow!("Cannot open 127.0.0.1:{} ({e}). Is Aillies already running?", cfg.loopback_port))?;
        let port = listener.local_addr()?.port();
        let secrets = store.load_or_create_secrets(port)?;
        let sharing = state.sharing;
        let node = Arc::new_cyclic(|me| Self {
            me: me.clone(),
            cfg,
            store,
            state: Mutex::new(state),
            secrets,
            identity,
            invites: Mutex::new(vec![]),
            pending: Mutex::new(HashMap::new()),
            outgoing: Mutex::new(HashMap::new()),
            last_failure: Mutex::new(None),
            epoch,
            listener: Mutex::new(None),
            loopback_task: Mutex::new(None),
            loopback_port: AtomicU16::new(port),
            turns: Arc::new(Semaphore::new(MAX_CONCURRENT_TURNS)),
            host_exec: Mutex::new(None),
            local_exec: Mutex::new(None),
        });
        let task = tokio::spawn(crate::loopback::serve(node.clone(), listener));
        *node.loopback_task.lock().unwrap() = Some(task);
        if sharing {
            if let Err(e) = node.start_listener().await {
                tracing::warn!("sharing could not start: {e}");
            }
        }
        Ok(node)
    }

    /// Stop listeners and drop active connections.
    pub fn shutdown(&self) {
        self.stop_listener();
        if let Some(t) = self.loopback_task.lock().unwrap().take() {
            t.abort();
        }
    }

    pub fn loopback_port(&self) -> u16 {
        self.loopback_port.load(Ordering::Relaxed)
    }
    pub fn codex_key(&self) -> &str {
        &self.secrets.codex_key
    }
    pub fn control_token(&self) -> &str {
        &self.secrets.control_token
    }
    pub fn fingerprint(&self) -> &str {
        &self.identity.fingerprint
    }
    pub fn peer_port(&self) -> Option<u16> {
        self.listener.lock().unwrap().as_ref().map(|(_, p)| *p)
    }

    pub(crate) fn snapshot(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    fn update<T>(&self, f: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let mut s = self.state.lock().unwrap();
        let mut next = s.clone();
        let out = f(&mut next)?;
        self.store.save_state(&next)?;
        *s = next;
        Ok(out)
    }

    fn auth_path(&self) -> Result<PathBuf> {
        self.cfg.codex_auth_path.clone().or_else(default_auth_path).ok_or_else(|| anyhow!("Cannot locate the Codex login file."))
    }

    fn executor(&self, expected: Option<String>) -> Result<Executor> {
        let path = self.auth_path()?;
        Ok(match &self.cfg.codex_upstream {
            Some(u) => Executor::with_upstream(path, expected, u.clone()),
            None => Executor::new(path, expected),
        })
    }

    pub(crate) fn host_executor(&self) -> Result<Arc<Executor>, Failure> {
        let mut slot = self.host_exec.lock().unwrap();
        if let Some(e) = slot.as_ref() {
            return Ok(e.clone());
        }
        let account = self.snapshot().shared_account.ok_or(Failure::new(FailureCode::AccountNotSelected))?;
        let e = Arc::new(self.executor(Some(account)).map_err(|_| Failure::new(FailureCode::AuthFileUnavailable))?);
        *slot = Some(e.clone());
        Ok(e)
    }

    pub(crate) fn local_executor(&self) -> Result<Arc<Executor>, Failure> {
        let mut slot = self.local_exec.lock().unwrap();
        if let Some(e) = slot.as_ref() {
            return Ok(e.clone());
        }
        let e = Arc::new(self.executor(None).map_err(|_| Failure::new(FailureCode::AuthFileUnavailable))?);
        *slot = Some(e.clone());
        Ok(e)
    }

    pub(crate) fn record_failure(&self, f: &Failure) {
        *self.last_failure.lock().unwrap() = Some(FailureView { code: f.code, message: f.message(), at: now() });
    }
    pub(crate) fn clear_failure(&self) {
        *self.last_failure.lock().unwrap() = None;
    }
    pub fn dismiss_failure(&self) {
        self.clear_failure();
    }

    pub(crate) fn touch_client(&self, fp: &str) {
        let _ = self.update(|s| {
            if let Some(c) = s.clients.iter_mut().find(|c| c.fp == fp) {
                c.last_used = Some(now());
            }
            Ok(())
        });
    }

    // ---------- Status ----------

    pub fn status(&self) -> NodeStatus {
        let s = self.snapshot();
        let local_account = match self.executor(None).and_then(|e| e.preflight().map_err(|f| anyhow!(f))) {
            Ok(()) => AccountReadiness::Ready,
            Err(e) => AccountReadiness::Failed { code: e.downcast_ref::<Failure>().map(|f| f.code).unwrap_or(FailureCode::AuthFileUnavailable) },
        };
        let last_failure = self.last_failure.lock().unwrap().clone();
        let destination_name = match &s.destination {
            Destination::Local => "This device".to_string(),
            Destination::Peer(fp) => s.peer(fp).map(|p| p.name.clone()).unwrap_or_else(|| "Unknown device".into()),
        };
        let state = if last_failure.is_some() {
            "action"
        } else if !s.routing {
            "paused"
        } else if s.destination == Destination::Local {
            "local"
        } else {
            "ready"
        };
        let cutoff = now().saturating_sub(PENDING_TTL);
        let pending = self
            .pending
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, p)| p.decision.is_none() && p.created >= cutoff)
            .map(|(id, p)| PendingView {
                id: id.clone(),
                name: p.name.clone(),
                platform: p.platform.clone(),
                code: p.code.clone(),
                short_id: short_id(&p.client_fp),
            })
            .collect();
        NodeStatus {
            name: s.name.clone(),
            platform: platform().into(),
            short_id: short_id(&self.identity.fingerprint),
            fingerprint: self.identity.fingerprint.clone(),
            version: aillies_core::APP_VERSION.into(),
            state: state.into(),
            routing: s.routing,
            sharing: s.sharing,
            destination: s.destination.clone(),
            destination_name,
            peers: s
                .peers
                .iter()
                .map(|p| PeerView { fp: p.fp.clone(), short_id: short_id(&p.fp), name: p.name.clone(), platform: p.platform.clone(), addrs: p.addrs.clone() })
                .collect(),
            clients: s
                .clients
                .iter()
                .map(|c| ClientView {
                    fp: c.fp.clone(),
                    short_id: short_id(&c.fp),
                    name: c.name.clone(),
                    platform: c.platform.clone(),
                    approved_at: c.approved_at,
                    last_used: c.last_used,
                })
                .collect(),
            pending,
            codex_installed: self.codex_config().is_installed(),
            local_account,
            last_failure,
            loopback_port: self.loopback_port(),
            peer_port: self.peer_port(),
        }
    }

    /// Transport readiness and account readiness, checked separately.
    pub async fn peer_health(&self, fp: &str) -> PeerHealth {
        let Some(peer) = self.snapshot().peer(fp).cloned() else {
            return PeerHealth { reachable: false, latency_ms: None, addr: None, status: None, failure: Some(FailureCode::NoDestination) };
        };
        let Some(mut c) = peer_client::connect(&self.identity, &peer.fp, &peer.addrs).await else {
            return PeerHealth { reachable: false, latency_ms: None, addr: None, status: None, failure: Some(FailureCode::PeerUnavailable) };
        };
        let resp = c.sender.send_request(peer_client::request("GET", "/v1/status", Bytes::new())).await;
        let (status, failure) = match resp {
            Ok(r) if r.status() == 200 => {
                let body = r.into_body().collect().await.map(|b| b.to_bytes()).unwrap_or_default();
                match serde_json::from_slice::<PeerStatus>(&body) {
                    Ok(ps) if ps.protocol == PROTOCOL_VERSION => (Some(ps), None),
                    Ok(_) => (None, Some(FailureCode::ProtocolMismatch)),
                    Err(_) => (None, Some(FailureCode::ProtocolMismatch)),
                }
            }
            Ok(r) => {
                let body = r.into_body().collect().await.map(|b| b.to_bytes()).unwrap_or_default();
                (None, Some(Failure::from_json(&body).map(|f| f.code).unwrap_or(FailureCode::NotApproved)))
            }
            Err(_) => (None, Some(FailureCode::PeerUnavailable)),
        };
        PeerHealth { reachable: true, latency_ms: Some(c.latency_ms), addr: Some(c.addr), status, failure }
    }

    // ---------- Settings ----------

    pub fn set_name(&self, name: &str) -> Result<()> {
        if !valid_name(name) {
            bail!("Use 1–64 characters for the device name.");
        }
        self.update(|s| {
            s.name = name.to_string();
            Ok(())
        })
    }

    pub fn set_routing(&self, on: bool) -> Result<()> {
        self.update(|s| {
            s.routing = on;
            Ok(())
        })
    }

    /// Accepts "local", a fingerprint, a short id or a device name.
    pub fn set_destination(&self, target: &str) -> Result<Destination> {
        let s = self.snapshot();
        let dest = if target.eq_ignore_ascii_case("local") || target.eq_ignore_ascii_case("this device") {
            Destination::Local
        } else {
            let t = target.to_lowercase().replace('·', "");
            let hits: Vec<&PeerRecord> =
                s.peers.iter().filter(|p| p.fp == t || p.fp.starts_with(&t) && t.len() >= 8 || p.name.eq_ignore_ascii_case(target)).collect();
            match hits.as_slice() {
                [one] => Destination::Peer(one.fp.clone()),
                [] => bail!("No paired device matches “{target}”."),
                _ => bail!("More than one device matches “{target}”. Use its id."),
            }
        };
        self.update(|s| {
            s.destination = dest.clone();
            Ok(())
        })?;
        self.clear_failure();
        Ok(dest)
    }

    pub async fn set_sharing(&self, on: bool) -> Result<()> {
        if on {
            let creds = self.executor(None)?.credentials().map_err(|f| anyhow!(f.message().replace("The sharing device", "This device")))?;
            self.update(|s| {
                s.sharing = true;
                s.shared_account = Some(creds.account_id.clone());
                Ok(())
            })?;
            *self.host_exec.lock().unwrap() = None;
            if self.peer_port().is_none() {
                self.start_listener().await?;
            }
        } else {
            self.update(|s| {
                s.sharing = false;
                s.shared_account = None;
                Ok(())
            })?;
            *self.host_exec.lock().unwrap() = None;
            self.stop_listener();
        }
        Ok(())
    }

    async fn start_listener(&self) -> Result<()> {
        let tcp = tokio::net::TcpListener::bind(("0.0.0.0", self.cfg.peer_port))
            .await
            .map_err(|e| anyhow!("Cannot open port {} for sharing ({e}).", self.cfg.peer_port))?;
        let port = tcp.local_addr()?.port();
        let tls = crate::tls::server_config(&self.identity)?;
        let me = self.self_arc();
        let task = tokio::spawn(crate::peer_server::serve(me, tcp, tls));
        *self.listener.lock().unwrap() = Some((task, port));
        Ok(())
    }

    fn stop_listener(&self) {
        if let Some((t, _)) = self.listener.lock().unwrap().take() {
            t.abort();
        }
        self.drop_connections();
    }

    /// Close every live peer connection (sharing off, revocation).
    pub(crate) fn drop_connections(&self) {
        self.epoch.send_modify(|e| *e += 1);
    }

    fn self_arc(&self) -> Arc<Node> {
        self.me.upgrade().expect("node alive")
    }

    // ---------- Codex ----------

    pub fn codex_config(&self) -> CodexConfig {
        let home = self.cfg.codex_home.clone().or_else(aillies_codex::codex_home).unwrap_or_else(|| PathBuf::from(".codex"));
        CodexConfig { codex_home: home, receipt_dir: self.cfg.home.clone() }
    }

    pub fn codex_enable(&self, model: Option<&str>) -> Result<()> {
        self.codex_config().install(self.loopback_port(), &self.secrets.codex_key, model).map_err(|e| anyhow!(e.to_string()))
    }

    pub fn codex_restore(&self) -> Result<()> {
        self.codex_config().restore().map_err(|e| anyhow!(e.to_string()))
    }

    // ---------- Pairing: sharing side ----------

    fn advertised_addrs(&self, port: u16) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(ifaces) = if_addrs::get_if_addrs() {
            let mut v4: Vec<String> = ifaces
                .iter()
                .filter(|i| !i.is_loopback())
                .filter_map(|i| match i.ip() {
                    std::net::IpAddr::V4(ip) if !ip.is_link_local() => Some(format!("{ip}:{port}")),
                    _ => None,
                })
                .collect();
            v4.sort();
            v4.dedup();
            out.extend(v4);
        }
        let host = gethostname::gethostname().to_string_lossy().to_string();
        if !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
            let h = if host.ends_with(".local") { host } else { format!("{host}.local") };
            out.push(format!("{h}:{port}"));
        }
        if self.cfg.advertise_loopback || out.is_empty() {
            out.push(format!("127.0.0.1:{port}"));
        }
        out.truncate(16);
        out
    }

    /// Create a one-time pairing code. Turns sharing on if needed.
    pub async fn create_invite(&self) -> Result<String> {
        if !self.snapshot().sharing {
            self.set_sharing(true).await?;
        }
        let port = self.peer_port().ok_or_else(|| anyhow!("Sharing is not running."))?;
        let secret = random_hex(32);
        {
            let mut inv = self.invites.lock().unwrap();
            let t = now();
            inv.retain(|i| i.expires > t);
            inv.push(InviteSlot { secret: secret.clone(), expires: t + INVITE_TTL_SECS });
        }
        let s = self.snapshot();
        let invite = Invite {
            v: PROTOCOL_VERSION,
            name: s.name,
            platform: platform().into(),
            addrs: self.advertised_addrs(port),
            fp: self.identity.fingerprint.clone(),
            secret,
        };
        Ok(invite.encode())
    }

    /// Peer side of `POST /v1/pair`.
    pub(crate) fn accept_pair_request(&self, client_fp: &str, req: PairRequest) -> Result<PairState, Failure> {
        use subtle::ConstantTimeEq;
        if req.protocol != PROTOCOL_VERSION {
            return Err(Failure::new(FailureCode::ProtocolMismatch));
        }
        if !valid_name(&req.name) || req.platform.len() > 16 {
            return Err(Failure::new(FailureCode::RequestInvalid));
        }
        let t = now();
        let matched = {
            let mut inv = self.invites.lock().unwrap();
            inv.retain(|i| i.expires > t);
            let pos = inv.iter().position(|i| i.secret.len() == req.secret.len() && bool::from(i.secret.as_bytes().ct_eq(req.secret.as_bytes())));
            pos.map(|p| inv.remove(p))
        };
        let Some(slot) = matched else { return Err(Failure::new(FailureCode::NotApproved)) };
        let code = verification_code(&self.identity.fingerprint, client_fp, &slot.secret);
        let id = uuid::Uuid::new_v4().to_string();
        let mut pending = self.pending.lock().unwrap();
        pending.retain(|_, p| p.created + PENDING_TTL > t);
        if pending.len() >= 8 {
            return Err(Failure::new(FailureCode::PeerBusy));
        }
        pending.insert(
            id.clone(),
            PendingPair { client_fp: client_fp.into(), name: req.name, platform: req.platform, code: code.clone(), created: t, decision: None },
        );
        Ok(PairState::Pending { id, code })
    }

    pub(crate) fn pair_poll(&self, client_fp: &str, id: &str) -> PairState {
        let pending = self.pending.lock().unwrap();
        match pending.get(id) {
            Some(p) if p.client_fp != client_fp => PairState::Expired,
            Some(p) if p.created + PENDING_TTL <= now() => PairState::Expired,
            Some(PendingPair { decision: Some(true), .. }) => PairState::Approved,
            Some(PendingPair { decision: Some(false), .. }) => PairState::Rejected,
            Some(p) => PairState::Pending { id: id.into(), code: p.code.clone() },
            None => PairState::Expired,
        }
    }

    pub(crate) fn pair_cancel(&self, client_fp: &str, id: &str) {
        let removed = {
            let mut pending = self.pending.lock().unwrap();
            match pending.get(id) {
                Some(p) if p.client_fp == client_fp => pending.remove(id),
                _ => None,
            }
        };
        if let Some(p) = removed {
            if p.decision == Some(true) {
                let _ = self.revoke_client(&p.client_fp);
            }
        }
    }

    pub fn approve(&self, id: &str, approve: bool) -> Result<()> {
        let rec = {
            let mut pending = self.pending.lock().unwrap();
            let p = pending.get_mut(id).ok_or_else(|| anyhow!("No pairing request with that id."))?;
            if p.created + PENDING_TTL <= now() {
                bail!("That pairing request expired.");
            }
            p.decision = Some(approve);
            ClientRecord { fp: p.client_fp.clone(), name: p.name.clone(), platform: p.platform.clone(), approved_at: now(), last_used: None }
        };
        if approve {
            self.update(|s| {
                s.clients.retain(|c| c.fp != rec.fp);
                s.clients.push(rec);
                Ok(())
            })?;
        }
        Ok(())
    }

    pub fn revoke_client(&self, target: &str) -> Result<()> {
        let t = target.to_lowercase().replace('·', "");
        self.update(|s| {
            let before = s.clients.len();
            s.clients.retain(|c| !(c.fp == t || (t.len() >= 8 && c.fp.starts_with(&t)) || c.name.eq_ignore_ascii_case(target)));
            if s.clients.len() == before {
                bail!("No approved device matches “{target}”.");
            }
            Ok(())
        })?;
        self.drop_connections();
        Ok(())
    }

    // ---------- Pairing: requesting side ----------

    pub async fn start_pairing(&self, code: &str) -> Result<PairStarted> {
        let invite = Invite::decode(code).map_err(|e| anyhow!(e.to_string()))?;
        if invite.fp == self.identity.fingerprint {
            bail!("That pairing code is from this device. Open it on the other device.");
        }
        let mut c = peer_client::connect(&self.identity, &invite.fp, &invite.addrs)
            .await
            .ok_or_else(|| anyhow!("Cannot reach {}. Both devices must be on the same network or a private network such as Tailscale.", invite.name))?;
        let s = self.snapshot();
        let body = serde_json::to_vec(&PairRequest { protocol: PROTOCOL_VERSION, secret: invite.secret.clone(), name: s.name, platform: platform().into() })?;
        let resp =
            c.sender.send_request(peer_client::request("POST", "/v1/pair", Bytes::from(body))).await.map_err(|_| anyhow!("Pairing connection dropped."))?;
        let ok = resp.status() == 200;
        let bytes = resp.into_body().collect().await.map(|b| b.to_bytes()).unwrap_or_default();
        if !ok {
            let f = Failure::from_json(&bytes);
            bail!(match f.map(|f| f.code) {
                Some(FailureCode::NotApproved) => "This pairing code was already used or expired. Create a new one on the other device.".to_string(),
                Some(FailureCode::ProtocolMismatch) => "Update Aillies on both devices.".to_string(),
                _ => "The other device refused pairing.".to_string(),
            });
        }
        let PairState::Pending { id: remote_id, code: remote_code } = serde_json::from_slice(&bytes)? else { bail!("Unexpected pairing reply.") };
        let local_code = verification_code(&invite.fp, &self.identity.fingerprint, &invite.secret);
        if remote_code != local_code {
            bail!("Security check failed: the verification codes differ. Pairing stopped.");
        }
        let id = uuid::Uuid::new_v4().to_string();
        let started = PairStarted {
            id: id.clone(),
            code: local_code.clone(),
            name: invite.name.clone(),
            platform: invite.platform.clone(),
            short_id: short_id(&invite.fp),
        };
        self.outgoing.lock().unwrap().insert(id, OutgoingPair { remote_id, invite, code: local_code, created: now() });
        Ok(started)
    }

    /// The person here confirmed the codes match. Wait for approval on the
    /// other device, then save it and (if nothing else is chosen) select it.
    pub async fn confirm_pairing(&self, id: &str, wait: Duration) -> Result<PeerView> {
        let (remote_id, invite) = {
            let out = self.outgoing.lock().unwrap();
            let p = out.get(id).ok_or_else(|| anyhow!("No pairing in progress with that id."))?;
            if p.created + PENDING_TTL <= now() {
                bail!("Pairing expired. Start again.");
            }
            let _ = &p.code;
            (p.remote_id.clone(), p.invite.clone())
        };
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let state = self.poll_remote(&invite, &remote_id).await;
            match state {
                Some(PairState::Approved) => break,
                Some(PairState::Rejected) => {
                    self.outgoing.lock().unwrap().remove(id);
                    bail!("{} declined the pairing.", invite.name);
                }
                Some(PairState::Expired) => {
                    self.outgoing.lock().unwrap().remove(id);
                    bail!("Pairing expired. Start again.");
                }
                _ => {}
            }
            if tokio::time::Instant::now() >= deadline {
                bail!("Still waiting for approval on {}. Approve there, then confirm again.", invite.name);
            }
            tokio::time::sleep(Duration::from_millis(700)).await;
        }
        self.outgoing.lock().unwrap().remove(id);
        let rec =
            PeerRecord { fp: invite.fp.clone(), name: invite.name.clone(), platform: invite.platform.clone(), addrs: invite.addrs.clone(), paired_at: now() };
        self.update(|s| {
            s.peers.retain(|p| p.fp != rec.fp);
            s.peers.push(rec.clone());
            if s.destination == Destination::Local {
                s.destination = Destination::Peer(rec.fp.clone());
            }
            Ok(())
        })?;
        Ok(PeerView { fp: rec.fp.clone(), short_id: short_id(&rec.fp), name: rec.name, platform: rec.platform, addrs: rec.addrs })
    }

    pub async fn cancel_pairing(&self, id: &str) {
        let p = self.outgoing.lock().unwrap().remove(id);
        if let Some(p) = p {
            if let Some(mut c) = peer_client::connect(&self.identity, &p.invite.fp, &p.invite.addrs).await {
                let _ = c.sender.send_request(peer_client::request("POST", &format!("/v1/pair/{}/cancel", p.remote_id), Bytes::new())).await;
            }
        }
    }

    async fn poll_remote(&self, invite: &Invite, remote_id: &str) -> Option<PairState> {
        let mut c = peer_client::connect(&self.identity, &invite.fp, &invite.addrs).await?;
        let r = c.sender.send_request(peer_client::request("GET", &format!("/v1/pair/{remote_id}"), Bytes::new())).await.ok()?;
        let b = r.into_body().collect().await.ok()?.to_bytes();
        serde_json::from_slice(&b).ok()
    }

    pub fn remove_peer(&self, target: &str) -> Result<()> {
        let t = target.to_lowercase().replace('·', "");
        self.update(|s| {
            let before = s.peers.len();
            s.peers.retain(|p| !(p.fp == t || (t.len() >= 8 && p.fp.starts_with(&t)) || p.name.eq_ignore_ascii_case(target)));
            if s.peers.len() == before {
                bail!("No paired device matches “{target}”.");
            }
            if let Destination::Peer(fp) = &s.destination {
                if s.peer(fp).is_none() {
                    s.destination = Destination::Local;
                }
            }
            Ok(())
        })
    }

    pub(crate) fn peer_status_doc(&self) -> PeerStatus {
        let s = self.snapshot();
        let account = match self.host_executor().and_then(|e| e.preflight()) {
            Ok(()) => AccountReadiness::Ready,
            Err(f) => AccountReadiness::Failed { code: f.code },
        };
        PeerStatus {
            protocol: PROTOCOL_VERSION,
            app_version: aillies_core::APP_VERSION.into(),
            name: s.name,
            platform: platform().into(),
            sharing: s.sharing,
            providers: vec![protocol::ProviderStatus {
                provider: protocol::Provider::Codex,
                capabilities: vec![
                    protocol::Capability::Text,
                    protocol::Capability::Reasoning,
                    protocol::Capability::ImageGeneration,
                    protocol::Capability::Streaming,
                    protocol::Capability::Cancellation,
                ],
                account,
            }],
        }
    }
}
