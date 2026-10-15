//! Settings-only access checks and explicitly requested native permission repair.
//! No config content, credentials, arbitrary paths or raw OS errors enter diagnostics.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
#[cfg(unix)]
#[path = "permissions/unix.rs"]
mod platform;
#[cfg(windows)]
#[path = "permissions/windows.rs"]
mod platform;

pub const HELPER_FLAG: &str = "--aillies-repair-settings-access";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessTarget {
    CodexFolder,
    CodexConfig,
    CodexReceipt,
    AilliesFolder,
    AilliesState,
    ClaudeFolder,
    ClaudeSettings,
    ClaudeReceipt,
    ClaudeDesktopFolder,
    ClaudeDesktopMeta,
    ClaudeDesktopReceipt,
}
impl AccessTarget {
    fn relative(self) -> PathBuf {
        let app = if cfg!(target_os = "macos") {
            "Library/Application Support/is.anyax.Aillies"
        } else if cfg!(windows) {
            "AppData/Roaming/anyax/Aillies/config"
        } else {
            ".config/aillies"
        };
        let desktop = if cfg!(target_os = "macos") {
            "Library/Application Support/Claude-3p/configLibrary"
        } else if cfg!(windows) {
            "AppData/Local/Claude-3p/configLibrary"
        } else {
            ".config/Claude-3p/configLibrary"
        };
        match self {
            Self::CodexFolder => ".codex".into(),
            Self::CodexConfig => ".codex/config.toml".into(),
            Self::ClaudeFolder => ".claude".into(),
            Self::ClaudeSettings => ".claude/settings.json".into(),
            Self::AilliesFolder => app.into(),
            Self::AilliesState => Path::new(app).join("state.json"),
            Self::CodexReceipt => Path::new(app).join("codex-restore.json"),
            Self::ClaudeReceipt => Path::new(app).join("claude-restore.json"),
            Self::ClaudeDesktopReceipt => Path::new(app).join("claude-desktop-restore.json"),
            Self::ClaudeDesktopFolder => desktop.into(),
            Self::ClaudeDesktopMeta => Path::new(desktop).join("_meta.json"),
        }
    }
    fn directory(self) -> bool {
        matches!(self, Self::CodexFolder | Self::ClaudeFolder | Self::AilliesFolder | Self::ClaudeDesktopFolder)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessOperation {
    Inspect,
    Read,
    Write,
    CreateTemporary,
    RemoveTemporary,
    Repair,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessCode {
    PermissionDenied,
    ReadOnlyFilesystem,
    UnsafePath,
    IoFailure,
    RepairStarted,
    AccessRepaired,
    RepairCancelled,
    RepairFailed,
    AuthenticationUnavailable,
    AuthenticationFailed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionIssue {
    pub target: AccessTarget,
    pub operation: AccessOperation,
    pub code: AccessCode,
    pub can_repair: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessEvent {
    pub at_unix_seconds: u64,
    pub service: Activation,
    pub target: AccessTarget,
    pub operation: AccessOperation,
    pub code: AccessCode,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairRequest {
    identity: String,
    targets: Vec<AccessTarget>,
}
impl RepairRequest {
    fn valid(&self) -> bool {
        !self.identity.is_empty()
            && self.identity.len() <= 256
            && !self.targets.is_empty()
            && self.targets.len() <= 11
            && self.targets.iter().enumerate().all(|(i, t)| !self.targets[..i].contains(t))
    }
}
fn access_code(error: &io::Error) -> AccessCode {
    match error.kind() {
        io::ErrorKind::PermissionDenied => AccessCode::PermissionDenied,
        io::ErrorKind::ReadOnlyFilesystem => AccessCode::ReadOnlyFilesystem,
        _ => AccessCode::IoFailure,
    }
}
fn safe_components(path: &Path) -> io::Result<()> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => return Err(io::ErrorKind::InvalidInput.into()),
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    Ok(())
}
fn probe(path: &Path, directory: bool) -> Result<(), (AccessOperation, AccessCode)> {
    safe_components(path)
        .map_err(|e| (AccessOperation::Inspect, if e.kind() == io::ErrorKind::InvalidInput { AccessCode::UnsafePath } else { access_code(&e) }))?;
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() == directory && (directory || meta.is_file()) => {
            if !directory {
                fs::File::open(path).map_err(|e| (AccessOperation::Read, access_code(&e)))?;
                fs::OpenOptions::new().write(true).open(path).map_err(|e| (AccessOperation::Write, access_code(&e)))?;
                return Ok(());
            }
        }
        Ok(_) => return Err((AccessOperation::Inspect, AccessCode::UnsafePath)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if !directory {
                return Ok(());
            }
        }
        Err(e) => return Err((AccessOperation::Inspect, access_code(&e))),
    }
    let parent = path.ancestors().find(|p| p.is_dir()).ok_or((AccessOperation::Inspect, AccessCode::IoFailure))?;
    let temp = parent.join(format!(".aillies-access-{}", uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    drop(options.open(&temp).map_err(|e| (AccessOperation::CreateTemporary, access_code(&e)))?);
    fs::remove_file(temp).map_err(|e| (AccessOperation::RemoveTemporary, access_code(&e)))
}
impl Node {
    fn access_targets(&self, service: Activation) -> Vec<(AccessTarget, PathBuf)> {
        let mut out = vec![(AccessTarget::AilliesFolder, self.cfg.home.clone()), (AccessTarget::AilliesState, self.cfg.home.join("state.json"))];
        if service != Activation::Claude {
            let config = self.codex_config();
            out.extend([
                (AccessTarget::CodexFolder, config.codex_home.clone()),
                (AccessTarget::CodexConfig, config.config_path()),
                (AccessTarget::CodexReceipt, self.cfg.home.join("codex-restore.json")),
            ]);
        }
        if service != Activation::Codex {
            if let Ok(config) = self.claude_config() {
                out.extend([
                    (AccessTarget::ClaudeFolder, config.home.clone()),
                    (AccessTarget::ClaudeSettings, config.home.join("settings.json")),
                    (AccessTarget::ClaudeReceipt, self.cfg.home.join("claude-restore.json")),
                ]);
            }
            if let Ok(config) = self.claude_desktop_config() {
                out.extend([
                    (AccessTarget::ClaudeDesktopFolder, config.library.clone()),
                    (AccessTarget::ClaudeDesktopMeta, config.library.join("_meta.json")),
                    (AccessTarget::ClaudeDesktopReceipt, self.cfg.home.join("claude-desktop-restore.json")),
                ]);
            }
        }
        out
    }
    fn scan_access(&self, service: Activation) -> Vec<PermissionIssue> {
        let home = platform::current_identity().ok().map(|(_, h)| h);
        self.access_targets(service)
            .into_iter()
            .filter_map(|(target, path)| {
                let (operation, code) = probe(&path, target.directory()).err()?;
                let can_repair = code == AccessCode::PermissionDenied
                    && path.exists()
                    && home.as_ref().is_some_and(|h| path == h.join(target.relative()) && path.ancestors().all(|p| !p.is_symlink()));
                Some(PermissionIssue { target, operation, code, can_repair })
            })
            .collect()
    }
    fn record_access(&self, service: Activation, issue: &PermissionIssue, code: AccessCode) {
        self.request_log.lock().unwrap().access(AccessEvent {
            at_unix_seconds: crate::store::now(),
            service,
            target: issue.target,
            operation: issue.operation,
            code,
        });
    }
    pub(super) fn capture_access_failure(&self, service: Activation) -> bool {
        let issues = self.scan_access(service);
        if issues.is_empty() {
            return false;
        }
        for issue in &issues {
            self.record_access(service, issue, issue.code);
        }
        if let Some(view) = self.activation_status.lock().unwrap().as_mut() {
            view.failed_stage = Some(ActivationPhase::Settings);
            view.phase = ActivationPhase::Failed;
            view.error = Some("Settings access is blocked. Review the access details, then try again.".into());
            view.permission_issues = issues;
        }
        true
    }
    pub(super) fn ensure_settings_access(&self, service: Activation) -> Result<()> {
        anyhow::ensure!(!self.capture_access_failure(service), "Settings access is blocked. Grant access, then review the switch again.");
        Ok(())
    }
    /// Explicit desktop action. IDs bind repair to the visible failed activation.
    /// No path, executable or script is accepted from IPC.
    pub fn repair_settings_access(&self, id: &str) -> Result<()> {
        let _change = self.codex_change.try_lock().map_err(|_| anyhow!("Wait for the provider change to finish."))?;
        let view = self
            .activation_status
            .lock()
            .unwrap()
            .clone()
            .filter(|v| v.id == id && v.phase == ActivationPhase::Failed)
            .ok_or_else(|| anyhow!("Review the failed switch before granting access."))?;
        let issues = self.scan_access(view.service);
        let targets: Vec<_> = issues.iter().filter(|i| i.can_repair).map(|i| i.target).collect();
        anyhow::ensure!(!targets.is_empty(), "No settings paths are eligible for automatic repair. Export diagnostics for individual help.");
        let (identity, _) = platform::current_identity().map_err(|_| anyhow!("Cannot identify the current user. No access was changed."))?;
        let request = RepairRequest { identity, targets };
        for issue in issues.iter().filter(|i| i.can_repair) {
            self.record_access(view.service, issue, AccessCode::RepairStarted);
        }
        // First attempt only metadata repair as the existing user.
        let result = request_access(&request, platform::repair, platform::elevate);
        let remaining = self.scan_access(view.service);
        let code = match result {
            Ok(()) if !remaining.iter().any(|i| request.targets.contains(&i.target)) => AccessCode::AccessRepaired,
            Err(AccessCode::RepairCancelled) => AccessCode::RepairCancelled,
            Err(AccessCode::AuthenticationUnavailable) => AccessCode::AuthenticationUnavailable,
            Err(AccessCode::AuthenticationFailed) => AccessCode::AuthenticationFailed,
            _ => AccessCode::RepairFailed,
        };
        for issue in issues.iter().filter(|i| i.can_repair) {
            self.record_access(view.service, issue, code);
        }
        if let Some(status) = self.activation_status.lock().unwrap().as_mut().filter(|s| s.id == id) {
            status.permission_issues = remaining.clone();
            status.error = Some(
                if code == AccessCode::RepairCancelled {
                    "Access request cancelled. Keep your current provider or try again."
                } else if code == AccessCode::AuthenticationUnavailable {
                    "The system authentication helper is unavailable. Export diagnostics for setup help."
                } else if code == AccessCode::AuthenticationFailed {
                    "System authentication did not complete. Check your authentication agent or administrator approval."
                } else if !remaining.is_empty() {
                    "Some settings are still blocked. Export diagnostics for individual help."
                } else {
                    "Write access is ready. Review the switch and try again."
                }
                .into(),
            );
        }
        anyhow::ensure!(code == AccessCode::AccessRepaired && remaining.is_empty(), "Access repair did not complete. Review the access details.");
        Ok(())
    }
}
/// Must run before initializing the node, runtime or desktop app.
pub fn helper_entry() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(HELPER_FLAG)) {
        return None;
    }
    let result = (|| {
        let arg = args.next().and_then(|s| s.into_string().ok()).ok_or(())?;
        if args.next().is_some() || arg.len() > 8192 {
            return Err(());
        }
        let bytes = URL_SAFE_NO_PAD.decode(arg).map_err(|_| ())?;
        let request: RepairRequest = serde_json::from_slice(&bytes).map_err(|_| ())?;
        if !request.valid() {
            return Err(());
        }
        platform::repair(&request).map_err(|_| ())
    })();
    Some(if result.is_ok() { 0 } else { 1 })
}
fn request_access(
    request: &RepairRequest,
    normal: impl FnOnce(&RepairRequest) -> Result<(), AccessCode>,
    elevated: impl FnOnce(&RepairRequest) -> Result<(), AccessCode>,
) -> Result<(), AccessCode> {
    match normal(request) {
        Err(AccessCode::PermissionDenied) => elevated(request),
        result => result,
    }
}
fn encoded(request: &RepairRequest) -> Result<String, AccessCode> {
    if !request.valid() {
        return Err(AccessCode::RepairFailed);
    }
    serde_json::to_vec(request).map(|s| URL_SAFE_NO_PAD.encode(s)).map_err(|_| AccessCode::RepairFailed)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repair_payload_is_closed_and_contains_no_paths_or_commands() {
        assert!(serde_json::from_str::<RepairRequest>(r#"{"identity":"1","targets":["../../auth.json"]}"#).is_err());
        assert!(!RepairRequest { identity: "1".into(), targets: vec![AccessTarget::CodexConfig, AccessTarget::CodexConfig] }.valid());
        let issue =
            PermissionIssue { target: AccessTarget::CodexConfig, operation: AccessOperation::Write, code: AccessCode::PermissionDenied, can_repair: false };
        let text = serde_json::to_string(&issue).unwrap();
        assert!(!text.contains('/'));
        assert!(text.contains("permission_denied"));
    }
    #[test]
    fn only_observed_denial_may_request_elevation_and_cancellation_is_preserved() {
        let request = RepairRequest { identity: "1".into(), targets: vec![AccessTarget::CodexConfig] };
        for result in [Ok(()), Err(AccessCode::IoFailure), Err(AccessCode::UnsafePath), Err(AccessCode::ReadOnlyFilesystem)] {
            assert_eq!(request_access(&request, |_| result, |_| panic!("Unexpected administrator prompt")), result);
        }
        assert_eq!(request_access(&request, |_| Err(AccessCode::PermissionDenied), |_| Err(AccessCode::RepairCancelled)), Err(AccessCode::RepairCancelled));
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn activation_denial_is_logged_before_clients_or_history_change() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = NodeConfig::from_env();
        cfg.home = dir.path().join("app");
        cfg.codex_home = Some(dir.path().join("codex"));
        cfg.claude_home = Some(dir.path().join("claude"));
        cfg.codex_auth_path = Some(dir.path().join("missing-auth"));
        cfg.loopback_port = 0;
        cfg.peer_port = 0;
        cfg.advertise_loopback = true;
        let config = cfg.codex_home.as_ref().unwrap().join("config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, b"model='gpt-5.5'\n# PRIVATE_VALUE\n").unwrap();
        fs::set_permissions(&config, fs::Permissions::from_mode(0o400)).unwrap();
        let node = Node::start(cfg).await.unwrap();
        assert!(node.prepare_activation_with_mode(Activation::Codex, true, ActivationMode::Manual).is_err());
        let view = node.status().activation.unwrap();
        assert!(!view.closed);
        assert_eq!(view.failed_stage, Some(ActivationPhase::Settings));
        assert!(view.permission_issues.iter().any(|i| i.target == AccessTarget::CodexConfig && i.code == AccessCode::PermissionDenied && !i.can_repair));
        let report = node.diagnostics();
        assert!(report["settings"].as_array().unwrap().iter().any(|e| e["code"] == "permission_denied"));
        assert!(!report.to_string().contains("PRIVATE_VALUE"));
        assert!(!report.to_string().contains(&dir.path().display().to_string()));
        assert!(node.repair_settings_access("wrong-plan").is_err());
        assert!(node.repair_settings_access(&view.id).is_err());
        assert_eq!(fs::read(&config).unwrap(), b"model='gpt-5.5'\n# PRIVATE_VALUE\n");
        node.shutdown();
    }
    #[cfg(unix)]
    #[test]
    fn access_probe_is_read_only_for_content_and_refuses_links() {
        use std::os::unix::{
            fs::MetadataExt,
            fs::{symlink, PermissionsExt},
        };
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let file = root.join("config.toml");
        fs::write(&file, b"PRIVATE_CONFIG").unwrap();
        assert!(probe(&file, false).is_ok());
        assert_eq!(fs::read(&file).unwrap(), b"PRIVATE_CONFIG");
        symlink(&file, root.join("linked")).unwrap();
        assert_eq!(probe(&root.join("linked"), false).unwrap_err().1, AccessCode::UnsafePath);
        fs::set_permissions(&file, fs::Permissions::from_mode(0o400)).unwrap();
        if fs::metadata(&file).unwrap().uid() != 0 {
            assert_eq!(probe(&file, false).unwrap_err(), (AccessOperation::Write, AccessCode::PermissionDenied));
        }
        assert_eq!(fs::read(&file).unwrap(), b"PRIVATE_CONFIG");
    }
}
