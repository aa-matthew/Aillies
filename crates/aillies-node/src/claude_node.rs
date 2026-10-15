//! Claude controls live in the node, shared by CLI and desktop shells.
use super::*;
use crate::store::ClaudeAccountSource;
use aillies_claude::{
    auth::{keychain_service_name, Reader},
    Config, Executor as ClaudeExecutor,
};
use aillies_claude::{desktop_auth::DesktopReader, desktop_config::DesktopConfig};
use anyhow::ensure;

pub(super) fn desktop_reader(cfg: &NodeConfig) -> Result<DesktopReader> {
    if let Some(reader) = &cfg.claude_desktop_reader {
        return Ok(reader.clone());
    }
    if let Some(home) = &cfg.claude_home {
        return Ok(DesktopReader::unavailable(home.join("desktop")));
    }
    let dirs = directories::BaseDirs::new().ok_or_else(|| anyhow!("Cannot locate Claude desktop."))?;
    let root = if cfg!(target_os = "windows") { dirs.data_local_dir() } else { dirs.config_dir() };
    Ok(DesktopReader::native(root.join("Claude")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeStatus {
    pub connected: bool,
    #[serde(default)]
    pub reconnect_required: bool,
    pub desktop_installed: bool,
    pub account_source: ClaudeAccountSource,
    pub installed: bool,
    pub sharing: bool,
    pub account: AccountReadiness,
    pub destination: Destination,
    pub destination_name: String,
}
impl Node {
    pub fn claude_key(&self) -> &str {
        &self.secrets.claude_key
    }
    pub fn claude_config(&self) -> Result<Config> {
        let home = self
            .cfg
            .claude_home
            .clone()
            .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()).map(PathBuf::from))
            .or_else(|| directories::BaseDirs::new().map(|d| d.home_dir().join(".claude")))
            .ok_or_else(|| anyhow!("Cannot locate Claude settings."))?;
        Ok(Config { home, receipts: self.cfg.home.clone() })
    }
    fn claude_reader_from(&self, source: ClaudeAccountSource) -> Result<Reader, Failure> {
        let cfg = self.claude_config().map_err(|_| Failure::new(FailureCode::ClaudeLoginUnavailable))?;
        let custom = self.cfg.claude_home.is_some() || std::env::var_os("CLAUDE_CONFIG_DIR").is_some_and(|s| !s.is_empty());
        let mut storage = cfg.home.clone();
        let mut keychain_service = None;
        if self.cfg.claude_home.is_none() {
            let secure = std::env::var_os("CLAUDE_SECURESTORAGE_CONFIG_DIR");
            if let Some(secure) = &secure {
                storage = if secure.is_empty() {
                    directories::BaseDirs::new().ok_or(Failure::new(FailureCode::ClaudeLoginUnavailable))?.home_dir().join(".claude")
                } else {
                    PathBuf::from(secure)
                };
            }
            let custom_storage = match secure {
                Some(s) if s.is_empty() => None,
                Some(_) => Some(storage.as_path()),
                None if custom => Some(cfg.home.as_path()),
                None => None,
            };
            keychain_service = Some(keychain_service_name(custom_storage));
        }
        let metadata = if custom {
            cfg.home.join(".claude.json")
        } else {
            cfg.home.parent().ok_or(Failure::new(FailureCode::ClaudeLoginUnavailable))?.join(".claude.json")
        };
        Ok(Reader {
            connected: (source == ClaudeAccountSource::Connected).then(|| self.claude_oauth.store.clone()),
            desktop: (source == ClaudeAccountSource::Desktop).then(|| self.claude_desktop_reader.clone()),
            home: cfg.home,
            storage,
            receipts: self.cfg.home.clone(),
            metadata,
            keychain_service,
        })
    }
    pub(super) fn claude_readiness(&self, expected: Option<&str>) -> AccountReadiness {
        let source = self.snapshot().claude.account_source;
        // Status polling must not probe either account or prompt for OS access.
        // The explicit Share action and execution paths perform credential checks.
        if !self.snapshot().claude.sharing {
            return AccountReadiness::Failed { code: FailureCode::ClaudeLoginUnavailable };
        }
        match self.claude_reader_from(source).and_then(|r| r.read()) {
            Ok(auth) if expected.is_none_or(|e| e == auth.account) => AccountReadiness::Ready,
            Ok(_) => AccountReadiness::Failed { code: FailureCode::AccountChanged },
            Err(f) => AccountReadiness::Failed { code: f.code },
        }
    }
    pub fn claude_status(&self) -> ClaudeStatus {
        let s = self.snapshot();
        let destination_name = match &s.claude.destination {
            Destination::Local => "This device".into(),
            Destination::Peer(fp) => s.peer(fp).map(|p| p.name.clone()).unwrap_or_else(|| "Unknown device".into()),
        };
        ClaudeStatus {
            connected: s.claude.connected_account.is_some() && self.claude_oauth.store.present(),
            reconnect_required: s.claude.connected_account.is_some() && !self.claude_oauth.store.present(),
            desktop_installed: self.claude_desktop_config().is_ok_and(|c| c.is_installed()),
            account_source: s.claude.account_source,
            installed: self.claude_config().is_ok_and(|c| c.is_installed()),
            sharing: s.claude.sharing,
            account: self.claude_readiness(s.claude.shared_account.as_deref()),
            destination: s.claude.destination,
            destination_name,
        }
    }
    pub fn claude_desktop_config(&self) -> Result<DesktopConfig> {
        let library = if let Some(home) = &self.cfg.claude_home {
            home.join("desktop-configLibrary")
        } else {
            self.claude_desktop_reader.home.parent().ok_or_else(|| anyhow!("Cannot locate Claude desktop configuration."))?.join("Claude-3p/configLibrary")
        };
        Ok(DesktopConfig { library, receipts: self.cfg.home.clone() })
    }
    pub(super) fn check_claude_desktop_closed(&self) -> Result<()> {
        if self.cfg.claude_home.is_none() {
            #[cfg(target_os = "macos")]
            {
                let open = std::process::Command::new("/usr/bin/pgrep")
                    .args(["-x", "Claude"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()?;
                ensure!(open.code() == Some(1), "Quit Claude desktop, then switch here. Your existing account and chats will be kept.");
                let mut managed = vec![PathBuf::from("/Library/Managed Preferences/com.anthropic.claudefordesktop.plist")];
                if let Ok(user) = std::env::var("USER") {
                    managed.push(PathBuf::from("/Library/Managed Preferences").join(user).join("com.anthropic.claudefordesktop.plist"));
                }
                ensure!(
                    !managed.iter().any(|p| std::fs::symlink_metadata(p).is_ok()),
                    "Claude desktop has managed settings. Configure its gateway through your administrator."
                );
            }
            #[cfg(target_os = "windows")]
            for hive in ["HKLM", "HKCU"] {
                let managed = std::process::Command::new("reg")
                    .args(["query", &format!("{hive}\\SOFTWARE\\Policies\\Claude")])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()?;
                ensure!(!managed.success(), "Claude desktop has managed settings. Configure its gateway through your administrator.");
            }
        }
        Ok(())
    }
    pub fn switch_claude_desktop_provider(&self, on: bool) -> Result<()> {
        let _change = self.codex_change.lock().unwrap();
        self.check_claude_desktop_closed()?;
        let cfg = self.claude_desktop_config()?;
        if on {
            cfg.install(self.loopback_port(), self.claude_key())?;
            self.activate_provider_routing(true)
        } else {
            cfg.restore()
        }
    }
    pub fn switch_claude_provider(&self, on: bool) -> Result<()> {
        let _change = self.codex_change.lock().unwrap();
        let cfg = self.claude_config()?;
        if on {
            cfg.install(self.loopback_port(), self.claude_key())?;
            self.activate_provider_routing(true)
        } else {
            cfg.restore()
        }
    }
    pub fn set_claude_destination(&self, target: &str) -> Result<Destination> {
        let s = self.snapshot();
        let dest = if target.eq_ignore_ascii_case("local") {
            Destination::Local
        } else {
            let t = target.to_lowercase().replace('·', "");
            let hits: Vec<_> = s.peers.iter().filter(|p| p.fp == t || (t.len() >= 8 && p.fp.starts_with(&t)) || p.name.eq_ignore_ascii_case(target)).collect();
            match hits.as_slice() {
                [p] => Destination::Peer(p.fp.clone()),
                _ => bail!("Choose one paired device for Claude."),
            }
        };
        self.update(|s| {
            s.claude.destination = dest.clone();
            Ok(())
        })?;
        Ok(dest)
    }
    pub async fn set_claude_sharing(&self, on: bool) -> Result<()> {
        self.set_claude_sharing_from(on, self.snapshot().claude.account_source).await
    }
    pub async fn set_claude_sharing_from(&self, on: bool, source: ClaudeAccountSource) -> Result<()> {
        let _change = self.claude_connection_change.lock().await;
        let current = self.snapshot().claude;
        ensure!(!on || !current.sharing || current.account_source == source, "Turn off Claude sharing before choosing a different account source.");
        let account = if on {
            if source == ClaudeAccountSource::Connected {
                let expected = current.connected_account.as_deref().ok_or_else(|| anyhow!("Connect a Claude account on this device first."))?;
                self.claude_oauth.fresh(expected).await.map_err(|_| anyhow!("Claude account needs to be connected again. Sharing stayed off."))?;
            }
            let reader = self.claude_reader_from(source).map_err(|f| anyhow!(f))?;
            Some(
                reader
                    .check_sharing()
                    .map_err(|error| {
                        anyhow!(if source == ClaudeAccountSource::Connected {
                            "Connect your Claude account again. Sharing stayed off."
                        } else {
                            error.message(source == ClaudeAccountSource::Desktop)
                        })
                    })?
                    .account,
            )
        } else {
            None
        };
        if on && self.peer_port().is_none() {
            self.start_listener().await?;
        }
        self.update(|s| {
            ensure!(!on || !s.claude.sharing || s.claude.account_source == source, "Turn off Claude sharing before choosing a different account source.");
            s.claude.sharing = on;
            s.claude.shared_account = account;
            if on {
                s.claude.account_source = source;
            }
            Ok(())
        })?;
        if !on {
            self.drop_connections();
            if !self.snapshot().sharing {
                self.stop_listener_wait().await;
            }
        }
        Ok(())
    }
    pub fn grant_codex(&self, fp: &str, on: bool) -> Result<()> {
        self.update(|s| {
            let c = s.clients.iter_mut().find(|c| c.fp == fp).ok_or_else(|| anyhow!("Pair this device first."))?;
            c.codex = on;
            Ok(())
        })?;
        self.drop_connections();
        Ok(())
    }
    pub fn grant_claude(&self, fp: &str, on: bool) -> Result<()> {
        self.update(|s| {
            let c = s.clients.iter_mut().find(|c| c.fp == fp).ok_or_else(|| anyhow!("Pair this device first."))?;
            c.claude = on;
            Ok(())
        })?;
        self.drop_connections();
        Ok(())
    }
    pub(crate) async fn claude_executor(&self, shared: bool, desktop_local: bool) -> Result<Arc<ClaudeExecutor>, Failure> {
        let source = if shared {
            self.snapshot().claude.account_source
        } else if self.snapshot().claude.account_source == ClaudeAccountSource::Connected {
            ClaudeAccountSource::Connected
        } else if desktop_local {
            ClaudeAccountSource::Desktop
        } else {
            ClaudeAccountSource::Code
        };
        if source == ClaudeAccountSource::Connected {
            let expected = self.snapshot().claude.connected_account.ok_or(Failure::new(FailureCode::ClaudeLoginUnavailable))?;
            self.claude_oauth.fresh(&expected).await?;
        }
        let reader = self.claude_reader_from(source)?;
        let check = reader.clone();
        let auth = tokio::task::spawn_blocking(move || check.read()).await.map_err(|_| Failure::new(FailureCode::ClaudeLoginUnavailable))??;
        if shared && self.snapshot().claude.shared_account.as_deref() != Some(auth.account.as_str()) {
            return Err(Failure::new(FailureCode::AccountChanged));
        }
        let token = auth.token_digest();
        Ok(Arc::new(ClaudeExecutor::new(reader, auth.account, token, self.cfg.claude_upstream.clone())))
    }
    pub(crate) async fn claude_retry_executor(&self, client: &str, trace: uuid::Uuid, initial_send: bool) -> Result<Arc<ClaudeExecutor>, Failure> {
        // Lookup first: a retry must never refresh/rebind its pinned executor.
        let (existing, generation) = {
            let mut bindings = self.claude_accounts.lock().unwrap();
            (bindings.executor(client, trace, false, || unreachable!()), bindings.generation())
        };
        if existing.is_ok() || !initial_send {
            return existing;
        }
        let executor = self.claude_executor(true, false).await?;
        let mut bindings = self.claude_accounts.lock().unwrap();
        if generation != bindings.generation() {
            return Err(Failure::new(FailureCode::AccountChanged));
        }
        bindings.executor(client, trace, initial_send, || Ok(executor))
    }
    pub async fn begin_claude_connect(&self) -> Result<String> {
        let _change = self.claude_connection_change.lock().await;
        ensure!(!self.snapshot().claude.sharing, "Turn off Claude sharing before connecting an account.");
        ensure!(
            cfg!(target_os = "macos") || self.cfg.claude_home.is_some(),
            "Connecting Claude accounts is currently available on Mac. You can send through a paired Mac."
        );
        Ok(self.claude_oauth.begin())
    }
    pub fn claude_authorization_url(&self) -> Result<String> {
        self.claude_oauth.authorization_url().ok_or_else(|| anyhow!("Connect Claude again. The authorization has expired."))
    }
    pub fn cancel_claude_connect(&self) {
        self.claude_oauth.cancel();
    }
    pub async fn finish_claude_connect(&self, code: &str) -> Result<()> {
        let _change = self.claude_connection_change.lock().await;
        ensure!(!self.snapshot().claude.sharing, "Turn off Claude sharing before connecting an account.");
        let account = self.claude_oauth.finish(code).await?;
        self.update(|s| {
            s.claude.connected_account = Some(account);
            s.claude.account_source = ClaudeAccountSource::Connected;
            Ok(())
        })?;
        self.drop_connections();
        Ok(())
    }
    pub async fn disconnect_claude_account(&self) -> Result<()> {
        let _change = self.claude_connection_change.lock().await;
        ensure!(!self.snapshot().claude.sharing, "Turn off Claude sharing before disconnecting this account.");
        // Fail closed even if the OS refuses deletion. Never select a native fallback.
        self.update(|s| {
            s.claude.connected_account = None;
            s.claude.account_source = ClaudeAccountSource::Connected;
            Ok(())
        })?;
        self.drop_connections();
        self.claude_oauth.disconnect().await
    }
}
