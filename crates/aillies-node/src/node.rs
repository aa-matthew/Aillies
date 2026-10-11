//! Node orchestration shared by the CLI daemon and the desktop shell.

use crate::diagnostics::{RequestContext, RequestLog};
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
    std::env::consts::OS
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
    pub diagnostics_build: String,
    pub name: String,
    pub platform: String,
    pub short_id: String,
    pub fingerprint: String,
    pub version: String,
    /// ready | paused | local | action
    pub state: String,
    pub routing: bool,
    pub sharing: bool,
    pub allow_fast_mode: bool,
    pub fast_mode: bool,
    pub destination: Destination,
    pub destination_name: String,
    pub peers: Vec<PeerView>,
    pub clients: Vec<ClientView>,
    pub pending: Vec<PendingView>,
    pub codex_installed: bool,
    #[serde(default)]
    pub provider_switch: Option<aillies_codex::provider_switch::SwitchProgress>,
    pub local_account: AccountReadiness,
    pub last_failure: Option<FailureView>,
    #[serde(default)]
    pub recent_warnings: Vec<crate::diagnostics::WarningView>,
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
    numeric_invite: Mutex<Option<crate::pairing::NumericInvite>>,
    pairing_search: Semaphore,
    pending: Mutex<HashMap<String, PendingPair>>,
    outgoing: Mutex<HashMap<String, OutgoingPair>>,
    last_failure: Mutex<Option<FailureView>>,
    request_log: Mutex<RequestLog>,
    retry_accounts: Mutex<crate::retry::AccountBindings>,
    codex_change: Mutex<()>,
    codex_progress: Mutex<Option<aillies_codex::provider_switch::SwitchProgress>>,
    codex_restart: Mutex<Option<(String, aillies_codex::restart::RestartPlan)>>,
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
        let request_log = Mutex::new(RequestLog::load(&cfg.home));
        let node = Arc::new_cyclic(|me| Self {
            me: me.clone(),
            cfg,
            store,
            state: Mutex::new(state),
            secrets,
            identity,
            invites: Mutex::new(vec![]),
            numeric_invite: Mutex::new(None),
            pairing_search: Semaphore::new(1),
            pending: Mutex::new(HashMap::new()),
            outgoing: Mutex::new(HashMap::new()),
            last_failure: Mutex::new(None),
            retry_accounts: Mutex::new(crate::retry::AccountBindings::default()),
            request_log,
            codex_change: Mutex::new(()),
            codex_progress: Mutex::new(None),
            codex_restart: Mutex::new(None),
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

    pub(crate) fn pinned_local_executor(&self) -> Result<Arc<Executor>, Failure> {
        let account = self.local_executor()?.credentials()?.account_id;
        self.executor(Some(account)).map(Arc::new).map_err(|_| Failure::new(FailureCode::AuthFileUnavailable))
    }

    pub(crate) fn host_retry_executor(&self, client: &str, trace: uuid::Uuid, initial_send: bool) -> Result<Arc<Executor>, Failure> {
        self.retry_accounts.lock().unwrap().executor(client, trace, initial_send, || self.host_executor())
    }

    pub fn diagnostics(&self) -> serde_json::Value {
        self.request_log.lock().unwrap().report()
    }

    pub fn export_diagnostics(&self) -> Result<PathBuf> {
        let dirs = directories::UserDirs::new().ok_or_else(|| anyhow!("Cannot find your Downloads folder."))?;
        let dir =
            dirs.download_dir().ok_or_else(|| anyhow!("Cannot find your Downloads folder. Copy request-diagnostics.json from the Aillies data folder."))?;
        crate::diagnostics::export_to(dir, &self.diagnostics())
    }

    pub(crate) fn record_diagnostic(&self, request: RequestContext, f: &Failure) {
        self.request_log.lock().unwrap().record(request, f);
    }

    pub(crate) fn record_retry(&self, request: RequestContext, f: &Failure, retry: u8) {
        self.request_log.lock().unwrap().record_retry(request, f, retry);
    }

    pub(crate) fn record_traffic(&self, request: RequestContext, upstream: bool, event: aillies_codex::telemetry::UpstreamEvent) {
        self.request_log.lock().unwrap().traffic(request, upstream, event);
    }

    pub(crate) fn upstream_observer(&self, request: RequestContext) -> aillies_codex::telemetry::Observer {
        let weak = self.me.clone();
        Arc::new(move |event| {
            if let Some(node) = weak.upgrade() {
                node.record_traffic(request, true, event);
            }
        })
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
            diagnostics_build: crate::diagnostics::DIAGNOSTICS_BUILD.into(),
            name: s.name.clone(),
            platform: platform().into(),
            short_id: short_id(&self.identity.fingerprint),
            fingerprint: self.identity.fingerprint.clone(),
            version: aillies_core::APP_VERSION.into(),
            state: state.into(),
            routing: s.routing,
            sharing: s.sharing,
            allow_fast_mode: s.allow_fast_mode,
            fast_mode: match &s.destination {
                Destination::Local => s.local_fast_mode,
                Destination::Peer(fp) => s.peer(fp).is_some_and(|p| p.fast_mode),
            },
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
            provider_switch: *self.codex_progress.lock().unwrap(),
            local_account,
            last_failure,
            recent_warnings: self.request_log.lock().unwrap().recent_warnings(),
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

    pub fn set_allow_fast_mode(&self, on: bool) -> Result<()> {
        self.update(|s| {
            s.allow_fast_mode = on;
            Ok(())
        })
    }

    pub async fn set_fast_mode(&self, on: bool) -> Result<()> {
        let destination = self.snapshot().destination;
        if on {
            if let Destination::Peer(fp) = &destination {
                let health = self.peer_health(fp).await;
                let allowed = health.status.as_ref().is_some_and(|s| {
                    s.sharing
                        && s.providers.iter().any(|p| p.provider == protocol::Provider::Codex && p.allow_fast_mode && p.account == AccountReadiness::Ready)
                });
                if !allowed {
                    bail!("The selected device must be reachable, ready, and allow Fast mode first.");
                }
            }
        }
        self.update(|s| {
            if s.destination != destination {
                bail!("The selected device changed. Try again.");
            }
            match &destination {
                Destination::Local => s.local_fast_mode = on,
                Destination::Peer(fp) => {
                    let peer = s.peers.iter_mut().find(|p| &p.fp == fp).ok_or_else(|| anyhow!("Choose a paired device first."))?;
                    peer.fast_mode = on;
                }
            }
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
        self.numeric_invite.lock().unwrap().take();
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
        let _change = self.codex_change.lock().unwrap();
        self.codex_restart.lock().unwrap().take();
        self.codex_config().install(self.loopback_port(), &self.secrets.codex_key, model).map_err(|e| anyhow!(e.to_string()))?;
        self.activate_provider_routing(true)
    }

    pub fn codex_restore(&self) -> Result<()> {
        let _change = self.codex_change.lock().unwrap();
        self.codex_restart.lock().unwrap().take();
        self.codex_config().restore().map_err(|e| anyhow!(e.to_string()))
    }

    pub fn switch_codex_provider(&self, on: bool) -> Result<aillies_codex::provider_switch::SwitchReport> {
        let _change = self.codex_change.lock().unwrap();
        self.codex_restart.lock().unwrap().take();
        let result = aillies_codex::provider_switch::switch_provider_with_progress(
            &self.codex_config(),
            on,
            self.loopback_port(),
            &self.secrets.codex_key,
            &|progress| *self.codex_progress.lock().unwrap() = Some(progress),
        )
        .map_err(|e| anyhow!(e.to_string()));
        *self.codex_progress.lock().unwrap() = None;
        let report = result?;
        self.activate_provider_routing(on)?;
        Ok(report)
    }

    fn activate_provider_routing(&self, on: bool) -> Result<()> {
        if on && !self.snapshot().routing {
            self.set_routing(true).map_err(|_| anyhow!("The provider changed, but Aillies could not be enabled. Choose Aillies again."))?;
        }
        Ok(())
    }

    /// Desktop Quit must not leave Codex configured for a stopped local provider.
    pub fn shutdown_if_normal(&self) -> Result<()> {
        let _change = self.codex_change.try_lock().map_err(|_| anyhow!("Wait for the provider change to finish before quitting."))?;
        if self.codex_config().is_installed() {
            bail!("Codex is using Aillies. Switch to Normal before quitting.");
        }
        self.shutdown();
        Ok(())
    }

    /// Read-only preview. Plans stay in memory and are single-use, expiring after two minutes.
    pub fn prepare_codex_restart(&self, on: bool) -> Result<aillies_codex::restart::RestartPreview> {
        let _change = self.codex_change.try_lock().map_err(|_| anyhow!("A provider change is already in progress."))?;
        self.codex_restart.lock().unwrap().take();
        let token = random_hex(24);
        let (plan, preview) = aillies_codex::restart::prepare(&self.codex_config().codex_home, on, token.clone())?;
        *self.codex_restart.lock().unwrap() = Some((token, plan));
        Ok(preview)
    }

    /// Runs inside Aillies, so it survives quitting the Codex app that requested the switch.
    pub fn restart_codex_provider(&self, token: &str) -> Result<aillies_codex::restart::RestartReport> {
        let _change = self.codex_change.try_lock().map_err(|_| anyhow!("A provider change is already in progress."))?;
        let mut pending = self.codex_restart.lock().unwrap();
        if !pending.as_ref().is_some_and(|(expected, _)| expected == token) {
            bail!("Review your Codex sessions again before switching.");
        }
        let (_, plan) = pending.take().ok_or_else(|| anyhow!("Review your Codex sessions again before switching."))?;
        drop(pending);
        let result =
            aillies_codex::restart::switch_and_reopen_with_progress(plan, &self.codex_config(), self.loopback_port(), &self.secrets.codex_key, &|progress| {
                *self.codex_progress.lock().unwrap() = Some(progress)
            })
            .map_err(|e| anyhow!(e.to_string()));
        *self.codex_progress.lock().unwrap() = None;
        let report = result?;
        self.activate_provider_routing(self.codex_config().is_installed())?;
        Ok(report)
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

    async fn prepare_invite(&self) -> Result<Invite> {
        if !self.snapshot().sharing {
            self.set_sharing(true).await?;
        }
        let port = self.peer_port().ok_or_else(|| anyhow!("Sharing is not running."))?;
        Ok(Invite {
            v: PROTOCOL_VERSION,
            name: self.snapshot().name,
            platform: platform().into(),
            addrs: self.advertised_addrs(port),
            fp: self.identity.fingerprint.clone(),
            secret: random_hex(32),
        })
    }

    /// Full invite for private networks and older devices. Never advertised.
    pub async fn create_invite(&self) -> Result<String> {
        let invite = self.prepare_invite().await?;
        let mut inv = self.invites.lock().unwrap();
        let t = now();
        inv.retain(|i| i.expires > t);
        if inv.len() >= 8 {
            inv.remove(0);
        }
        inv.push(InviteSlot { secret: invite.secret.clone(), expires: t + INVITE_TTL_SECS });
        Ok(invite.encode())
    }

    /// Eight cryptographically random digits. Nearby discovery carries no password.
    pub async fn create_pairing_code(&self) -> Result<String> {
        let invite = self.prepare_invite().await?;
        let mut slot = self.numeric_invite.lock().unwrap();
        let port = self.peer_port().filter(|_| self.snapshot().sharing).ok_or_else(|| anyhow!("Turn sharing on, then show a new code."))?;
        let mut numeric = crate::pairing::NumericInvite::new(invite);
        numeric.advertisement = Some(crate::discovery::advertise(self.fingerprint(), port)?);
        let code = numeric.code().to_owned();
        let generation = numeric.generation;
        *slot = Some(numeric);
        let me = self.me.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(INVITE_TTL_SECS)).await;
            if let Some(node) = me.upgrade() {
                let mut slot = node.numeric_invite.lock().unwrap();
                if slot.as_ref().is_some_and(|i| i.generation == generation) {
                    slot.take();
                }
            }
        });
        Ok(code)
    }

    pub async fn create_full_invite(&self) -> Result<String> {
        self.numeric_invite.lock().unwrap().take();
        self.create_invite().await
    }

    pub(crate) fn bootstrap_pairing(&self, client_fp: &str, request: &crate::pairing::BootstrapRequest) -> Result<crate::pairing::BootstrapReply, Failure> {
        if !self.snapshot().sharing {
            return Err(Failure::new(FailureCode::SharingOff));
        }
        self.numeric_invite.lock().unwrap().as_mut().ok_or_else(|| Failure::new(FailureCode::NotApproved))?.bootstrap(client_fp, request)
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
        let numeric = {
            let mut slot = self.numeric_invite.lock().unwrap();
            if slot.as_ref().is_some_and(|i| i.accepts_secret(client_fp, &req.secret)) {
                slot.take().map(|i| InviteSlot { secret: i.invite.secret, expires: t + INVITE_TTL_SECS })
            } else {
                None
            }
        };
        let matched = numeric.or_else(|| {
            let mut inv = self.invites.lock().unwrap();
            inv.retain(|i| i.expires > t);
            let pos = inv.iter().position(|i| i.secret.len() == req.secret.len() && bool::from(i.secret.as_bytes().ct_eq(req.secret.as_bytes())));
            pos.map(|p| inv.remove(p))
        });
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
        if let Some(digits) = protocol::pairing_digits(code) {
            let _search = self.pairing_search.try_acquire().map_err(|_| anyhow!("Pairing is already in progress."))?;
            return tokio::time::timeout(Duration::from_secs(25), async {
                let candidates = crate::discovery::discover().await?;
                let invite = self.resolve_pairing_code(&digits, &candidates).await?;
                self.start_pairing_invite(invite).await
            })
            .await
            .map_err(|_| anyhow!("Nearby pairing timed out. Show a new code or use a full pairing code."))?;
        }
        let invite = Invite::decode(code).map_err(|_| anyhow!("Enter eight digits or a full Aillies pairing code."))?;
        self.start_pairing_invite(invite).await
    }

    /// Alternate discovery backends may supply candidates. They remain untrusted
    /// until password authentication succeeds; this never skips human approval.
    pub async fn start_pairing_with_candidates(&self, code: &str, candidates: &[crate::PairingCandidate]) -> Result<PairStarted> {
        let _search = self.pairing_search.try_acquire().map_err(|_| anyhow!("Pairing is already in progress."))?;
        let digits = protocol::pairing_digits(code).ok_or_else(|| anyhow!("Enter the eight-digit pairing code."))?;
        tokio::time::timeout(Duration::from_secs(25), async {
            let invite = self.resolve_pairing_code(&digits, candidates).await?;
            self.start_pairing_invite(invite).await
        })
        .await
        .map_err(|_| anyhow!("Nearby pairing timed out. Show a new code or use a full pairing code."))?
    }

    async fn resolve_pairing_code(&self, code: &str, candidates: &[crate::PairingCandidate]) -> Result<Invite> {
        use futures_util::StreamExt;
        let mut seen = std::collections::HashSet::new();
        let candidates: Vec<_> = candidates
            .iter()
            .take(crate::discovery::MAX_CANDIDATES)
            .filter(|c| {
                c.fp != self.fingerprint()
                    && protocol::is_fingerprint(&c.fp)
                    && !c.addrs.is_empty()
                    && c.addrs.len() <= 16
                    && c.addrs.iter().all(|a| a.parse::<std::net::SocketAddr>().is_ok())
                    && seen.insert(c.fp.clone())
            })
            .cloned()
            .collect();
        let node = self.self_arc();
        let code = code.to_owned();
        let results: Vec<_> = futures_util::stream::iter(candidates)
            .map(move |candidate| {
                let node = node.clone();
                let code = code.clone();
                async move {
                    let (client, request) = crate::pairing::ClientHandshake::start(&code, node.fingerprint(), &candidate.fp).ok()?;
                    let mut c = peer_client::connect(&node.identity, &candidate.fp, &candidate.addrs).await?;
                    let raw = serde_json::to_vec(&request).ok()?;
                    let response = c.sender.send_request(peer_client::request("POST", "/v1/pair/bootstrap", Bytes::from(raw))).await.ok()?;
                    if response.status() != 200 {
                        return None;
                    }
                    let bytes = crate::body::read_limited(response.into_body(), 35 * 1024).await.ok()?;
                    let reply = serde_json::from_slice(&bytes).ok()?;
                    let mut invite = client.finish(reply).ok()?;
                    // The authenticated connection is the best known path; identity remains pinned.
                    invite.addrs.retain(|addr| addr != &c.addr);
                    invite.addrs.insert(0, c.addr);
                    invite.addrs.truncate(16);
                    Some(invite)
                }
            })
            .buffer_unordered(4)
            .filter_map(|result| async move { result })
            .collect()
            .await;
        let mut found = results.into_iter();
        let invite=found.next().ok_or_else(||anyhow!("Code not found, expired or too many attempts. Keep both devices on the same local network, show a new code, or use a full code for a private network."))?;
        if found.next().is_some() {
            bail!("More than one device matches. Show a new pairing code and try again.");
        }
        Ok(invite)
    }

    async fn start_pairing_invite(&self, invite: Invite) -> Result<PairStarted> {
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
        let rec = PeerRecord {
            fp: invite.fp.clone(),
            name: invite.name.clone(),
            platform: invite.platform.clone(),
            addrs: invite.addrs.clone(),
            paired_at: now(),
            fast_mode: false,
        };
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
            // Keep a missing selection: the next turn fails NoDestination.
            // Only an explicit set_destination may choose the local account.
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
                allow_fast_mode: s.allow_fast_mode,
                supports_pinned_retries: true,
            }],
        }
    }
}

#[cfg(test)]
mod invariant_tests {
    use super::*;
    #[tokio::test]
    async fn explicit_provider_activation_recovers_pause_and_shutdown_requires_normal() {
        let dir = tempfile::tempdir().unwrap();
        let node = Node::start(NodeConfig {
            home: dir.path().join("node"),
            loopback_port: 0,
            peer_port: 0,
            codex_auth_path: Some(dir.path().join("missing")),
            codex_home: Some(dir.path().join("codex")),
            codex_upstream: None,
            advertise_loopback: true,
        })
        .await
        .unwrap();
        node.set_routing(false).unwrap();
        node.codex_enable(None).unwrap();
        assert!(node.snapshot().routing, "an explicit activation must recover a previously paused setup");
        assert!(node.shutdown_if_normal().is_err());
        assert!(reqwest::Client::new().get(format!("http://127.0.0.1:{}/", node.loopback_port())).send().await.is_ok());
        node.set_routing(false).unwrap();
        node.activate_provider_routing(true).unwrap();
        assert!(node.snapshot().routing);
        node.set_routing(false).unwrap();
        node.activate_provider_routing(false).unwrap();
        node.codex_restore().unwrap();
        assert!(!node.snapshot().routing, "Normal never changes the routing setting");
        let guard = node.codex_change.lock().unwrap();
        assert!(node.shutdown_if_normal().is_err(), "never exit during a provider transaction");
        drop(guard);
        node.shutdown_if_normal().unwrap();
    }

    #[tokio::test]
    async fn restart_requires_a_current_confirmation_and_rejects_concurrent_changes() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("codex");
        std::fs::create_dir_all(&home).unwrap();
        let config = home.join("config.toml");
        std::fs::write(&config, b"model_provider=\"openai\"\n").unwrap();
        let node = Node::start(NodeConfig {
            home: dir.path().join("node"),
            loopback_port: 0,
            peer_port: 0,
            codex_auth_path: Some(dir.path().join("missing")),
            codex_home: Some(home),
            codex_upstream: None,
            advertise_loopback: true,
        })
        .await
        .unwrap();
        assert!(node.restart_codex_provider("unknown").err().unwrap().to_string().contains("Review your Codex sessions"));
        let guard = node.codex_change.lock().unwrap();
        assert!(node.prepare_codex_restart(true).err().unwrap().to_string().contains("already in progress"));
        assert!(node.restart_codex_provider("unknown").err().unwrap().to_string().contains("already in progress"));
        let progress =
            aillies_codex::provider_switch::SwitchProgress { phase: aillies_codex::provider_switch::SwitchPhase::Preparing, completed: 2, total: 12 };
        *node.codex_progress.lock().unwrap() = Some(progress);
        assert_eq!(node.status().provider_switch, Some(progress));
        // Progress is readable while migration owns the change lock. Older status remains readable.
        let mut status = serde_json::to_value(node.status()).unwrap();
        status.as_object_mut().unwrap().remove("provider_switch");
        assert!(serde_json::from_value::<NodeStatus>(status).unwrap().provider_switch.is_none());
        *node.codex_progress.lock().unwrap() = None;
        drop(guard);
        assert_eq!(std::fs::read(config).unwrap(), b"model_provider=\"openai\"\n");
        assert!(node.codex_restart.lock().unwrap().is_none());
        node.shutdown();
    }

    #[tokio::test]
    async fn expired_invite_cannot_be_consumed_or_approved() {
        let dir = tempfile::tempdir().unwrap();
        let node = Node::start(NodeConfig {
            home: dir.path().join("node"),
            loopback_port: 0,
            peer_port: 0,
            codex_auth_path: Some(dir.path().join("missing")),
            codex_home: Some(dir.path().join("codex")),
            codex_upstream: None,
            advertise_loopback: true,
        })
        .await
        .unwrap();
        let secret = "test-expired-secret".to_string();
        node.invites.lock().unwrap().push(InviteSlot { secret: secret.clone(), expires: now().saturating_sub(1) });
        let fp = "a".repeat(64);
        let req = PairRequest { protocol: PROTOCOL_VERSION, secret, name: "client".into(), platform: "macos".into() };
        assert_eq!(node.accept_pair_request(&fp, req).unwrap_err().code, FailureCode::NotApproved);
        assert!(node.status().pending.is_empty());
        assert!(node.status().clients.is_empty());
        node.shutdown();
    }
}
