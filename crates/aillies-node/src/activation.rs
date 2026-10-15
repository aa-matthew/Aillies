//! Reviewed activation and a non-blocking exit gate. No account fallback.
use super::*;
use aillies_codex::restart::{self, ClosedClients, RestartPlan};
use anyhow::ensure;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Activation {
    Codex,
    Claude,
    Quit,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActivationMode {
    #[default]
    Now,
    Later,
    Manual,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationPhase {
    Reviewed,
    Waiting,
    Closing,
    History,
    Settings,
    Reopening,
    Completed,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivationStatus {
    pub id: String,
    pub service: Activation,
    pub on: bool,
    pub mode: ActivationMode,
    pub phase: ActivationPhase,
    pub failed_stage: Option<ActivationPhase>,
    pub error: Option<String>,
    pub open_clients: Vec<restart::OpenClient>,
    pub checked: bool,
    pub reviewed_clients: Vec<restart::OpenClient>,
    pub closed: bool,
    pub recovery_saved: bool,
    pub history_done: bool,
    pub settings_done: bool,
    pub reopen_failed: usize,
    #[serde(default)]
    pub history_skipped: usize,
    #[serde(default)]
    pub history_pending: bool,
}
pub(super) struct ActivationPlan {
    token: String,
    created: Instant,
    service: Activation,
    on: bool,
    clients: Vec<RestartPlan>,
    manual_clients: Vec<restart::Client>,
    mode: ActivationMode,
    scheduled: bool,
    empty_samples: usize,
    route: Destination,
    installed: (bool, bool, bool),
    connected_account: Option<String>,
}
#[derive(Serialize)]
pub struct ActivationPreview {
    pub token: String,
    pub service: Activation,
    pub on: bool,
    pub apps: Vec<&'static str>,
    pub terminals: usize,
    pub manual_close: bool,
    pub chats: Option<usize>,
    pub mode: ActivationMode,
}
#[derive(Debug, Serialize, Default)]
pub struct CleanupReport {
    pub codex: bool,
    pub claude_code: bool,
    pub claude_desktop: bool,
    pub complete: bool,
    pub reopen_failed: usize,
    #[serde(default)]
    pub history_skipped: usize,
    #[serde(default)]
    pub history_pending: bool,
}
impl Node {
    pub fn prepare_activation(&self, service: Activation, on: bool) -> Result<ActivationPreview> {
        self.prepare_activation_with_mode(service, on, ActivationMode::Now)
    }
    pub fn prepare_activation_with_mode(&self, service: Activation, on: bool, mode: ActivationMode) -> Result<ActivationPreview> {
        let _change = self.codex_change.try_lock().map_err(|_| anyhow!("Wait for the provider change to finish."))?;
        self.cancel_activation();
        let state = self.snapshot();
        let installed = (self.codex_config().is_installed(), self.claude_config()?.is_installed(), self.claude_desktop_config()?.is_installed());
        let mode = if !cfg!(target_os = "macos") && mode == ActivationMode::Now { ActivationMode::Manual } else { mode };
        let mut plan = ActivationPlan {
            token: random_hex(24),
            created: Instant::now(),
            service,
            on,
            clients: vec![],
            manual_clients: vec![],
            mode,
            scheduled: false,
            empty_samples: 0,
            route: if service == Activation::Claude { state.claude.destination.clone() } else { state.destination.clone() },
            installed,
            connected_account: state.claude.connected_account.clone(),
        };
        let mut apps = vec![];
        let mut terminals = 0;
        let codex = service == Activation::Codex || service == Activation::Quit && self.codex_config().is_installed();
        let claude = service == Activation::Claude
            || service == Activation::Quit && (self.claude_config()?.is_installed() || self.claude_desktop_config()?.is_installed());
        if service == Activation::Claude && on {
            ensure!(
                self.snapshot().claude.destination != Destination::Local || self.claude_status().connected,
                "Connect a Claude account first, or choose a paired device."
            );
        }
        if codex && cfg!(target_os = "macos") && mode != ActivationMode::Manual {
            let (clients, preview) = if mode == ActivationMode::Later {
                restart::prepare_passive(on, restart::Client::Codex)?
            } else {
                restart::prepare(&self.codex_config().codex_home, on, String::new())?
            };
            plan.clients.push(clients);
            apps.extend(preview.apps);
            terminals += preview.terminals;
        }
        if claude && cfg!(target_os = "macos") && mode != ActivationMode::Manual {
            let (clients, preview) = if mode == ActivationMode::Later {
                restart::prepare_passive(on, restart::Client::Claude)?
            } else {
                restart::prepare_claude(&self.claude_config()?.home, on, String::new())?
            };
            plan.clients.push(clients);
            apps.extend(preview.apps);
            terminals += preview.terminals;
        }
        if !cfg!(target_os = "macos") || mode != ActivationMode::Now {
            if codex {
                plan.manual_clients.push(restart::Client::Codex);
            }
            if claude {
                plan.manual_clients.push(restart::Client::Claude);
            }
        }
        apps.sort();
        apps.dedup();
        if mode == ActivationMode::Manual {
            for c in &plan.manual_clients {
                apps.push(if *c == restart::Client::Codex { "Codex / ChatGPT" } else { "Claude / Claude Code" });
            }
        }
        let preview = ActivationPreview {
            token: plan.token.clone(),
            service,
            on,
            apps,
            terminals,
            manual_close: mode == ActivationMode::Manual,
            chats: if codex { aillies_codex::provider_switch::preview_chat_count(&self.codex_config(), on) } else { None },
            mode,
        };
        *self.activation_status.lock().unwrap() = Some(ActivationStatus {
            id: plan.token.clone(),
            service,
            on,
            mode,
            phase: ActivationPhase::Reviewed,
            failed_stage: None,
            error: None,
            open_clients: vec![],
            checked: false,
            reviewed_clients: vec![],
            closed: false,
            recovery_saved: false,
            history_done: false,
            settings_done: false,
            reopen_failed: 0,
            history_skipped: 0,
            history_pending: false,
        });
        *self.activation.lock().unwrap() = Some(plan);
        Ok(preview)
    }
    pub fn cancel_activation(&self) {
        if self.activation.lock().unwrap().take().is_some() {
            if let Some(view) = self.activation_status.lock().unwrap().as_mut() {
                view.phase = ActivationPhase::Cancelled;
            }
        }
    }
    pub fn cancel_scheduled_activation(&self) -> Result<()> {
        let _change = self.codex_change.try_lock().map_err(|_| anyhow!("The switch is applying. Keep clients closed until it finishes."))?;
        self.cancel_activation();
        Ok(())
    }
    pub fn dismiss_activation_notice(&self) {
        let mut view = self.activation_status.lock().unwrap();
        if view.as_ref().is_some_and(|v| matches!(v.phase, ActivationPhase::Completed | ActivationPhase::Failed | ActivationPhase::Cancelled)) {
            view.take();
        }
    }
    fn activation_bound(&self, plan: &ActivationPlan) -> bool {
        let s = self.snapshot();
        let route = if plan.service == Activation::Claude { &s.claude.destination } else { &s.destination };
        route == &plan.route
            && match route {
                Destination::Local => true,
                Destination::Peer(fp) => s.peer(fp).is_some(),
            }
            && (plan.service != Activation::Claude || s.claude.connected_account == plan.connected_account)
            && (
                self.codex_config().is_installed(),
                self.claude_config().is_ok_and(|c| c.is_installed()),
                self.claude_desktop_config().is_ok_and(|c| c.is_installed()),
            ) == plan.installed
    }
    fn activation_phase(&self, phase: ActivationPhase) {
        if let Some(view) = self.activation_status.lock().unwrap().as_mut() {
            view.phase = phase;
        }
    }
    fn activation_failed(&self, error: &str) {
        if let Some(view) = self.activation_status.lock().unwrap().as_mut() {
            view.failed_stage = Some(view.phase);
            view.phase = ActivationPhase::Failed;
            view.error = Some(error.to_owned());
        }
    }
    pub fn schedule_activation(&self, token: &str, mode: ActivationMode) -> Result<()> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| anyhow!("Cannot start the waiting switch. Try again."))?;
        ensure!(mode != ActivationMode::Now, "Choose when to switch first.");
        let _change = self.codex_change.try_lock().map_err(|_| anyhow!("Wait for the provider change to finish."))?;
        let mut plans = self.activation.lock().unwrap();
        let plan = plans
            .as_mut()
            .filter(|p| p.token == token && !p.scheduled && p.created.elapsed() < Duration::from_secs(120))
            .ok_or_else(|| anyhow!("Review the apps again before scheduling."))?;
        ensure!(plan.service != Activation::Quit, "Disconnect cannot be scheduled.");
        ensure!(self.activation_bound(plan), "The chosen provider or device changed. Review the switch again.");
        plan.mode = mode;
        plan.scheduled = true;
        plan.manual_clients = vec![if plan.service == Activation::Claude { restart::Client::Claude } else { restart::Client::Codex }];
        if let Some(view) = self.activation_status.lock().unwrap().as_mut() {
            view.mode = mode;
            view.phase = ActivationPhase::Waiting;
        }
        let token = token.to_owned();
        let weak = self.me.clone();
        runtime.spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let Some(node) = weak.upgrade() else {
                    break;
                };
                let id = token.clone();
                let keep = tokio::task::spawn_blocking(move || node.poll_scheduled_activation(&id)).await.unwrap_or(false);
                if !keep {
                    break;
                }
            }
        });
        Ok(())
    }
    fn poll_scheduled_activation(&self, token: &str) -> bool {
        self.poll_scheduled_with(token, &restart::open_clients, |p| self.apply_plan(p))
    }
    fn poll_scheduled_with(
        &self,
        token: &str,
        probe: &impl Fn(&[restart::Client]) -> std::result::Result<Vec<restart::OpenClient>, restart::RestartError>,
        apply: impl FnOnce(ActivationPlan) -> Result<CleanupReport>,
    ) -> bool {
        let Ok(_change) = self.codex_change.try_lock() else {
            return true;
        };
        let mut plans = self.activation.lock().unwrap();
        let Some(plan) = plans.as_mut().filter(|p| p.token == token && p.scheduled) else {
            return false;
        };
        if !self.activation_bound(plan) {
            plans.take();
            self.activation_failed("The chosen provider, account or device changed. Review the switch again.");
            return false;
        }
        let open = match probe(&plan.manual_clients) {
            Ok(v) => v,
            Err(_) => {
                plans.take();
                self.activation_failed("Cannot check open clients. No settings were changed.");
                return false;
            }
        };
        if let Some(view) = self.activation_status.lock().unwrap().as_mut() {
            view.checked = true;
            for client in &open {
                if let Some(old) = view.reviewed_clients.iter_mut().find(|c| c.label == client.label) {
                    old.count = old.count.max(client.count);
                } else {
                    view.reviewed_clients.push(client.clone());
                }
            }
            view.open_clients = open.clone();
        }
        if !ready_after_closure(&mut plan.empty_samples, &open) {
            return true;
        }
        let Some(plan) = plans.take() else {
            return false;
        };
        drop(plans);
        if apply(plan).is_err() {
            self.activation_failed("The switch stopped. Keep affected clients closed and review it again. Recovery files were kept.");
        }
        false
    }

    /// Single use, bound to the reviewed inventory. A refusal never silently switches.
    pub fn apply_activation(&self, token: &str) -> Result<CleanupReport> {
        let _change = self.codex_change.try_lock().map_err(|_| anyhow!("Wait for the provider change to finish."))?;
        let mut pending = self.activation.lock().unwrap();
        ensure!(
            pending.as_ref().is_some_and(|p| p.token == token && p.created.elapsed() < Duration::from_secs(120)),
            "Review the apps again before switching."
        );
        let plan = pending.take().ok_or_else(|| anyhow!("Review the apps again before switching."))?;
        drop(pending);
        ensure!(self.activation_bound(&plan), "The chosen provider or device changed. Review the switch again.");
        let result = self.apply_plan(plan);
        if result.is_err() {
            self.activation_failed("The change stopped. Check open clients and review it again. Recovery files were kept.");
        }
        result
    }
    fn apply_plan(&self, plan: ActivationPlan) -> Result<CleanupReport> {
        self.activation_phase(ActivationPhase::Closing);
        restart::check_manual_clients(&plan.manual_clients)?;
        if plan.service == Activation::Quit {
            self.stop_sharing_for_exit();
        }
        let closed: Vec<ClosedClients> = if plan.mode == ActivationMode::Now {
            restart::close_reviewed(plan.clients)?
        } else if plan.mode == ActivationMode::Later {
            restart::passive_desktops(plan.clients)
        } else {
            vec![]
        };
        if let Some(view) = self.activation_status.lock().unwrap().as_mut() {
            view.closed = true;
        }
        self.activation_phase(ActivationPhase::History);
        let mut report = match plan.service {
            Activation::Codex => {
                let result = aillies_codex::provider_switch::switch_provider_with_progress(
                    &self.codex_config(),
                    plan.on,
                    self.loopback_port(),
                    &self.secrets.codex_key,
                    &|p| {
                        *self.codex_progress.lock().unwrap() = Some(p);
                        if let Some(view) = self.activation_status.lock().unwrap().as_mut() {
                            view.recovery_saved |= p.phase == aillies_codex::provider_switch::SwitchPhase::Applying;
                            if p.phase == aillies_codex::provider_switch::SwitchPhase::Settings {
                                view.history_done = true;
                                view.phase = ActivationPhase::Settings;
                            }
                        }
                    },
                );
                *self.codex_progress.lock().unwrap() = None;
                let migration = result.map_err(|error| anyhow!("{error}"))?;
                if let Some(view) = self.activation_status.lock().unwrap().as_mut() {
                    view.history_done = true;
                    view.history_skipped = migration.skipped;
                    view.history_pending = migration.history_pending;
                }
                self.activation_phase(ActivationPhase::Settings);
                self.activate_provider_routing(plan.on)?;
                CleanupReport { complete: true, history_skipped: migration.skipped, history_pending: migration.history_pending, ..Default::default() }
            }
            Activation::Claude => {
                self.activation_phase(ActivationPhase::Settings);
                self.change_claude_clients(plan.on)?;
                self.activate_provider_routing(plan.on)?;
                CleanupReport { complete: true, ..Default::default() }
            }
            Activation::Quit => self.disconnect_settings(),
        };
        if let Some(view) = self.activation_status.lock().unwrap().as_mut() {
            view.settings_done = report.complete;
        }
        self.activation_phase(ActivationPhase::Reopening);
        // Failed cleanup deliberately leaves clients closed. The UI can always exit.
        if report.complete {
            for client in closed {
                report.reopen_failed += usize::from(client.reopen().is_err());
            }
        }
        if let Some(view) = self.activation_status.lock().unwrap().as_mut() {
            view.reopen_failed = report.reopen_failed;
        }
        self.activation_phase(if report.complete { ActivationPhase::Completed } else { ActivationPhase::Failed });
        Ok(report)
    }
    fn change_claude_clients(&self, on: bool) -> Result<()> {
        let code = self.claude_config()?;
        let desktop = self.claude_desktop_config()?;
        if on {
            self.check_claude_desktop_closed()?;
            let code_was_installed = code.is_installed();
            code.install(self.loopback_port(), self.claude_key())?;
            if let Err(error) = desktop.install(self.loopback_port(), self.claude_key()) {
                if !code_was_installed {
                    let _ = code.restore();
                }
                return Err(error);
            }
            Ok(())
        } else {
            self.check_claude_desktop_closed()?;
            let code_result = code.restore();
            let desktop_result = desktop.restore();
            code_result.and(desktop_result)
        }
    }
    /// All services are attempted independently. Receipts remain for unsafe files.
    fn disconnect_settings(&self) -> CleanupReport {
        let codex = if self.codex_config().is_installed() {
            let result = aillies_codex::provider_switch::switch_provider_with_progress(
                &self.codex_config(),
                false,
                self.loopback_port(),
                &self.secrets.codex_key,
                &|p| *self.codex_progress.lock().unwrap() = Some(p),
            );
            *self.codex_progress.lock().unwrap() = None;
            result.is_ok()
        } else {
            true
        };
        let claude_code = self.claude_config().and_then(|c| c.restore()).is_ok();
        let claude_desktop = self.claude_desktop_config().and_then(|c| c.restore()).is_ok();
        CleanupReport {
            codex,
            claude_code,
            claude_desktop,
            complete: codex && claude_code && claude_desktop,
            reopen_failed: 0,
            history_skipped: 0,
            history_pending: false,
        }
    }
    /// Explicit exit gate: no config rewriting or credential reads, even after failure.
    fn stop_sharing_for_exit(&self) {
        let _ = self.update(|s| {
            s.sharing = false;
            s.shared_account = None;
            s.claude.sharing = false;
            s.claude.shared_account = None;
            Ok(())
        });
        self.stop_listener();
    }
    pub fn shutdown_anyway(&self) {
        self.stop_sharing_for_exit();
        self.cancel_activation();
        self.shutdown();
    }
}

fn ready_after_closure(samples: &mut usize, open: &[restart::OpenClient]) -> bool {
    if open.is_empty() {
        *samples += 1;
    } else {
        *samples = 0;
    }
    *samples >= 2
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn passive_switch_needs_two_closed_samples_and_resets_on_reopen() {
        let mut samples = 0;
        assert!(!ready_after_closure(&mut samples, &[]));
        assert!(!ready_after_closure(&mut samples, &[restart::OpenClient { label: "Codex".into(), count: 1 }]));
        assert!(!ready_after_closure(&mut samples, &[]));
        assert!(ready_after_closure(&mut samples, &[]));
    }
    #[tokio::test]
    async fn scheduled_switch_is_single_use_cancellable_and_route_bound() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = NodeConfig::from_env();
        cfg.home = dir.path().join("node");
        cfg.codex_home = Some(dir.path().join("codex"));
        cfg.codex_auth_path = Some(dir.path().join("missing"));
        cfg.claude_home = Some(dir.path().join("claude"));
        cfg.loopback_port = 0;
        cfg.peer_port = 0;
        let node = Node::start(cfg).await.unwrap();
        let pending = || ActivationPlan {
            token: "test".into(),
            created: Instant::now(),
            service: Activation::Codex,
            on: true,
            clients: vec![],
            manual_clients: vec![restart::Client::Codex],
            mode: ActivationMode::Later,
            scheduled: true,
            empty_samples: 0,
            route: Destination::Local,
            installed: (false, false, false),
            connected_account: None,
        };
        *node.activation.lock().unwrap() = Some(pending());
        let open = |_: &[restart::Client]| Ok(vec![restart::OpenClient { label: "Codex".into(), count: 1 }]);
        assert!(node.poll_scheduled_with("test", &open, |_| panic!("must not apply while open")));
        let closed = |_: &[restart::Client]| Ok(vec![]);
        assert!(node.poll_scheduled_with("test", &closed, |_| panic!("one sample is not enough")));
        let mut applied = false;
        assert!(!node.poll_scheduled_with("test", &closed, |p| {
            assert_eq!(p.mode, ActivationMode::Later);
            applied = true;
            Ok(CleanupReport::default())
        }));
        assert!(applied);
        assert!(!node.poll_scheduled_with("test", &closed, |_| panic!("single use")));
        *node.activation.lock().unwrap() = Some(pending());
        node.cancel_scheduled_activation().unwrap();
        assert!(!node.poll_scheduled_with("test", &closed, |_| panic!("cancelled")));
        *node.activation.lock().unwrap() = Some(pending());
        node.update(|s| {
            s.destination = Destination::Peer("different".into());
            Ok(())
        })
        .unwrap();
        assert!(!node.poll_scheduled_with("test", &closed, |_| panic!("route changed")));
        assert!(!node.codex_config().is_installed());
        node.shutdown();
    }
    #[cfg(not(target_os = "macos"))]
    #[tokio::test]
    async fn manually_closed_clients_can_activate_restore_and_disconnect() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = NodeConfig::from_env();
        cfg.home = dir.path().join("aillies");
        cfg.codex_home = Some(dir.path().join("codex"));
        cfg.codex_auth_path = Some(dir.path().join("missing-auth.json"));
        cfg.claude_home = Some(dir.path().join("claude"));
        cfg.loopback_port = 0;
        cfg.peer_port = 0;
        let node = Node::start(cfg).await.unwrap();
        // No account access or requests: only exercise the manual settings path.
        node.update(|s| {
            s.peers.push(crate::store::PeerRecord {
                fp: "fixture-peer".into(),
                name: "Fixture".into(),
                platform: "macos".into(),
                addrs: vec![],
                paired_at: 0,
                fast_mode: false,
            });
            s.claude.destination = Destination::Peer("fixture-peer".into());
            Ok(())
        })
        .unwrap();
        for service in [Activation::Codex, Activation::Claude] {
            for on in [true, false, true] {
                let plan = node.prepare_activation(service, on).unwrap();
                assert!(plan.manual_close);
                assert_eq!(plan.mode, ActivationMode::Manual);
                assert_eq!(plan.terminals, 0);
                assert_eq!(plan.apps, vec![if service == Activation::Codex { "Codex / ChatGPT" } else { "Claude / Claude Code" }]);
                assert!(node.apply_activation(&plan.token).unwrap().complete);
                assert!(node.apply_activation(&plan.token).is_err());
                let installed = if service == Activation::Codex { node.codex_config().is_installed() } else { node.claude_config().unwrap().is_installed() };
                assert_eq!(installed, on);
            }
        }
        let plan = node.prepare_activation(Activation::Quit, false).unwrap();
        assert!(node.apply_activation(&plan.token).unwrap().complete);
        assert!(!node.codex_config().is_installed());
        assert!(!node.claude_config().unwrap().is_installed());
        assert!(!node.claude_desktop_config().unwrap().is_installed());
        let empty = node.prepare_activation(Activation::Quit, false).unwrap();
        assert!(node.apply_activation(&empty.token).unwrap().complete);
        node.shutdown();
    }
    #[tokio::test]
    async fn malformed_cleanup_keeps_receipt_but_never_traps_exit() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = NodeConfig::from_env();
        cfg.home = dir.path().join("aillies");
        cfg.codex_home = Some(dir.path().join("codex"));
        cfg.codex_auth_path = Some(dir.path().join("missing-auth.json"));
        cfg.claude_home = Some(dir.path().join("claude"));
        cfg.loopback_port = 0;
        cfg.peer_port = 0;
        let node = Node::start(cfg).await.unwrap();
        let code = node.claude_config().unwrap();
        code.install(node.loopback_port(), node.claude_key()).unwrap();
        std::fs::write(code.home.join("settings.json"), b"not JSON").unwrap();
        let result = node.disconnect_settings();
        assert!(!result.complete);
        assert!(!result.claude_code);
        assert!(result.claude_desktop);
        assert!(code.is_installed());
        assert_eq!(std::fs::read(code.home.join("settings.json")).unwrap(), b"not JSON");
        node.shutdown_anyway();
        assert!(node.loopback_task.lock().unwrap().is_none());

        // An old selected connection must not resume sharing or silently read
        // a different credential source when its new private file is absent.
        let mut old = node.snapshot();
        old.claude.account_source = crate::store::ClaudeAccountSource::Connected;
        old.claude.connected_account = Some("previous-connection".into());
        old.claude.shared_account = old.claude.connected_account.clone();
        old.claude.sharing = true;
        node.store.save_state(&old).unwrap();
        let reopened = Node::start(node.cfg.clone()).await.unwrap();
        let status = reopened.claude_status();
        assert!(!status.sharing && !status.connected && status.reconnect_required);
        assert_eq!(reopened.snapshot().claude.connected_account, old.claude.connected_account);
        assert!(reopened.peer_port().is_none());
        reopened.shutdown();
    }
}
