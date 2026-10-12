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
pub(super) struct ActivationPlan {
    token: String,
    created: Instant,
    service: Activation,
    on: bool,
    clients: Vec<RestartPlan>,
    manual_clients: Vec<restart::Client>,
}
#[derive(Serialize)]
pub struct ActivationPreview {
    pub token: String,
    pub service: Activation,
    pub on: bool,
    pub apps: Vec<&'static str>,
    pub terminals: usize,
    pub manual_close: bool,
}
#[derive(Debug, Serialize, Default)]
pub struct CleanupReport {
    pub codex: bool,
    pub claude_code: bool,
    pub claude_desktop: bool,
    pub complete: bool,
    pub reopen_failed: usize,
}
impl Node {
    pub fn prepare_activation(&self, service: Activation, on: bool) -> Result<ActivationPreview> {
        let _change = self.codex_change.try_lock().map_err(|_| anyhow!("Wait for the provider change to finish."))?;
        self.activation.lock().unwrap().take();
        let mut plan = ActivationPlan { token: random_hex(24), created: Instant::now(), service, on, clients: vec![], manual_clients: vec![] };
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
        if codex && cfg!(target_os = "macos") {
            let (clients, preview) = restart::prepare(&self.codex_config().codex_home, on, String::new())?;
            plan.clients.push(clients);
            apps.extend(preview.apps);
            terminals += preview.terminals;
        }
        if claude && cfg!(target_os = "macos") {
            let (clients, preview) = restart::prepare_claude(&self.claude_config()?.home, on, String::new())?;
            plan.clients.push(clients);
            apps.extend(preview.apps);
            terminals += preview.terminals;
        }
        if !cfg!(target_os = "macos") {
            if codex {
                plan.manual_clients.push(restart::Client::Codex);
            }
            if claude {
                plan.manual_clients.push(restart::Client::Claude);
            }
        }
        apps.sort();
        apps.dedup();
        let preview = ActivationPreview { token: plan.token.clone(), service, on, apps, terminals, manual_close: !cfg!(target_os = "macos") };
        *self.activation.lock().unwrap() = Some(plan);
        Ok(preview)
    }
    pub fn cancel_activation(&self) {
        self.activation.lock().unwrap().take();
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
        restart::check_manual_clients(&plan.manual_clients)?;
        if plan.service == Activation::Quit {
            self.stop_sharing_for_exit();
        }
        let closed: Vec<ClosedClients> = restart::close_reviewed(plan.clients)?;
        let mut report = match plan.service {
            Activation::Codex => {
                let result = aillies_codex::provider_switch::switch_provider_with_progress(
                    &self.codex_config(),
                    plan.on,
                    self.loopback_port(),
                    &self.secrets.codex_key,
                    &|p| *self.codex_progress.lock().unwrap() = Some(p),
                );
                *self.codex_progress.lock().unwrap() = None;
                result.map_err(|_| anyhow!("Codex could not switch safely. It stayed closed; your history and recovery files were kept."))?;
                self.activate_provider_routing(plan.on)?;
                CleanupReport { complete: true, ..Default::default() }
            }
            Activation::Claude => {
                self.change_claude_clients(plan.on)?;
                self.activate_provider_routing(plan.on)?;
                CleanupReport { complete: true, ..Default::default() }
            }
            Activation::Quit => self.disconnect_settings(),
        };
        // Failed cleanup deliberately leaves clients closed. The UI can always exit.
        if report.complete {
            for client in closed {
                report.reopen_failed += usize::from(client.reopen().is_err());
            }
        }
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
        CleanupReport { codex, claude_code, claude_desktop, complete: codex && claude_code && claude_desktop, reopen_failed: 0 }
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

#[cfg(test)]
mod tests {
    use super::*;
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
            s.claude.destination = Destination::Peer("fixture-peer".into());
            Ok(())
        })
        .unwrap();
        for service in [Activation::Codex, Activation::Claude] {
            for on in [true, false, true] {
                let plan = node.prepare_activation(service, on).unwrap();
                assert!(plan.manual_close && plan.apps.is_empty());
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
