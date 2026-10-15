//! Explicit desktop restart around the existing offline migration. Never submits a turn.
#[cfg(any(target_os = "macos", test))]
use crate::provider_switch::SwitchReport;
use serde::{Deserialize, Serialize};
#[cfg(any(target_os = "macos", test))]
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, RestartError>;
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct RestartError(pub(crate) &'static str);
#[cfg(any(target_os = "macos", test))]
const CHANGED: RestartError = RestartError("Code sessions changed. Review them again before switching.");
#[cfg(any(target_os = "macos", test))]
const OPEN: RestartError = RestartError("A client is still open. Finish your work and close it, then try again. The provider was not switched.");
#[cfg(not(target_os = "macos"))]
const UNSUPPORTED: RestartError = RestartError("Close Codex yourself on this system, then switch providers here.");

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct Process {
    pid: u32,
    parent: u32,
    uid: u32,
    tty: String,
    started: String,
    exe: PathBuf,
}
#[cfg(any(target_os = "macos", test))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    App { process: Process, path: PathBuf, label: &'static str },
    Terminal { process: Process, cwd: PathBuf, home: PathBuf },
    Daemon(Process),
}
#[cfg(any(target_os = "macos", test))]
impl Target {
    #[cfg(target_os = "macos")]
    fn process(&self) -> &Process {
        match self {
            Self::App { process, .. } | Self::Terminal { process, .. } | Self::Daemon(process) => process,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Client {
    Codex,
    Claude,
}
/// Public waiting metadata contains static labels and counts, never PIDs or paths.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenClient {
    pub label: String,
    pub count: usize,
}
pub fn open_clients(clients: &[Client]) -> Result<Vec<OpenClient>> {
    if clients.is_empty() {
        return Ok(vec![]);
    }
    #[cfg(target_os = "macos")]
    {
        macos::open_clients(clients)
    }
    #[cfg(not(target_os = "macos"))]
    {
        #[cfg(windows)]
        let output = std::process::Command::new("tasklist").args(["/FO", "CSV", "/NH"]).output();
        #[cfg(not(windows))]
        let output = std::process::Command::new("ps").args(["-A", "-o", "comm="]).output();
        let output = output.map_err(|_| RestartError("Cannot check open clients. No settings were changed."))?;
        if !output.status.success() || output.stdout.len() > 4 * 1024 * 1024 {
            return Err(RestartError("Cannot check open clients. No settings were changed."));
        }
        Ok(summarize_manual(&String::from_utf8_lossy(&output.stdout), cfg!(windows), clients))
    }
}
#[cfg(any(not(target_os = "macos"), test))]
fn summarize_manual(listing: &str, windows: bool, clients: &[Client]) -> Vec<OpenClient> {
    let mut counts = std::collections::BTreeMap::new();
    for line in listing.lines() {
        let label = if clients.contains(&Client::Codex) && manual_running(line, windows, Client::Codex) {
            if line.to_ascii_lowercase().contains("chatgpt") {
                "ChatGPT"
            } else {
                "Codex"
            }
        } else if clients.contains(&Client::Claude) && manual_running(line, windows, Client::Claude) {
            "Claude"
        } else {
            continue;
        };
        *counts.entry(label).or_insert(0) += 1;
    }
    counts.into_iter().map(|(label, count)| OpenClient { label: label.into(), count }).collect()
}
/// Passive closure never terminates or launches anything on any platform.
pub fn check_manual_clients(clients: &[Client]) -> Result<()> {
    if !open_clients(clients)?.is_empty() {
        return Err(RestartError("Close the affected desktop apps and code sessions, then try again. No settings were changed."));
    }
    Ok(())
}
#[cfg(any(not(target_os = "macos"), test))]
fn manual_running(listing: &str, windows: bool, client: Client) -> bool {
    listing.lines().any(|line| {
        let line = if windows { line.split(',').next().unwrap_or("").trim_matches('"') } else { line.trim() };
        let name = line.rsplit(['/', '\\']).next().unwrap_or("").to_ascii_lowercase();
        match client {
            Client::Codex => matches!(name.as_str(), "codex" | "codex.exe" | "chatgpt" | "chatgpt.exe"),
            Client::Claude => matches!(name.as_str(), "claude" | "claude.exe") || line.contains("/.local/share/claude/versions/"),
        }
    })
}
#[cfg(any(target_os = "macos", test))]
pub struct RestartPlan {
    on: bool,
    client: Client,
    created: Instant,
    processes: Vec<Process>,
    targets: Vec<Target>,
    chatgpt: Option<PathBuf>,
}
#[derive(Serialize)]
pub struct RestartPreview {
    pub token: String,
    pub on: bool,
    pub apps: Vec<&'static str>,
    pub terminals: usize,
}
#[derive(Serialize)]
pub struct RestartReport {
    pub chats: usize,
    pub skipped: usize,
    pub history_pending: bool,
    pub reopen_failed: usize,
}
#[cfg(any(target_os = "macos", test))]
trait Host {
    fn codex(&mut self) -> Result<Vec<Process>>;
    fn clients(&mut self, client: Client) -> Result<Vec<Process>> {
        let _ = client;
        self.codex()
    }
    fn stop(&mut self, target: &Target) -> Result<()>;
    fn wait_closed(&mut self, client: Client) -> Result<()>;
    fn reopen_chatgpt(&mut self, path: &std::path::Path) -> Result<()>;
}

#[cfg(any(target_os = "macos", test))]
enum MigrationError {
    Refused(&'static str),
    RecoveryNeeded,
}

#[cfg(any(target_os = "macos", test))]
fn execute(host: &mut impl Host, plan: RestartPlan, migrate: impl FnOnce(bool) -> std::result::Result<SwitchReport, MigrationError>) -> Result<RestartReport> {
    if plan.created.elapsed() > Duration::from_secs(120) || host.clients(plan.client)? != plan.processes {
        return Err(CHANGED);
    }
    let closed = (|| {
        for target in &plan.targets {
            host.stop(target)?;
        }
        host.wait_closed(plan.client)
    })();
    let switched = match closed {
        Err(error) => Err(error),
        Ok(()) => match migrate(plan.on) {
            Ok(report) => Ok(report),
            Err(MigrationError::Refused(message)) => Err(RestartError(message)),
            Err(MigrationError::RecoveryNeeded) => {
                return Err(RestartError("The switch stopped. Keep Codex closed and try again in Aillies. Recovery files were kept."))
            }
        },
    };
    // Only ChatGPT is reopened, including after an ordinary refusal. Recovery stays closed.
    let reopen_failed = usize::from(plan.chatgpt.as_ref().is_some_and(|path| host.reopen_chatgpt(path).is_err()));
    switched.map(|r| RestartReport { chats: r.chats, skipped: r.skipped, history_pending: r.history_pending, reopen_failed })
}

#[cfg(not(any(target_os = "macos", test)))]
pub struct RestartPlan;
#[cfg(target_os = "macos")]
mod macos;

pub fn prepare(home: &std::path::Path, on: bool, token: String) -> Result<(RestartPlan, RestartPreview)> {
    #[cfg(target_os = "macos")]
    {
        macos::prepare(home, on, token)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (home, on, token);
        Err(UNSUPPORTED)
    }
}
/// Passive review captures only desktop reopen paths and public client labels.
/// It never identifies terminal homes or prepares termination targets.
pub fn prepare_passive(on: bool, client: Client) -> Result<(RestartPlan, RestartPreview)> {
    #[cfg(target_os = "macos")]
    {
        macos::prepare_passive(on, client)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (on, client);
        Err(UNSUPPORTED)
    }
}
/// Reviewed closure shared by activation and disconnect; never force-kills clients.
pub struct ClosedClients {
    #[cfg(target_os = "macos")]
    desktop: Option<PathBuf>,
}
pub fn prepare_claude(home: &std::path::Path, on: bool, token: String) -> Result<(RestartPlan, RestartPreview)> {
    #[cfg(target_os = "macos")]
    {
        macos::prepare_client(home, on, token, Client::Claude)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (home, on, token);
        Err(UNSUPPORTED)
    }
}
#[cfg(any(target_os = "macos", test))]
fn close_plans(host: &mut impl Host, plans: Vec<RestartPlan>) -> Result<Vec<Option<PathBuf>>> {
    // Validate the entire review before closing anything. An editor can own both
    // clients; closing its first service must not invalidate the second plan.
    for plan in &plans {
        if plan.created.elapsed() > Duration::from_secs(120) || host.clients(plan.client)? != plan.processes {
            return Err(CHANGED);
        }
    }
    for plan in &plans {
        for target in &plan.targets {
            host.stop(target)?;
        }
    }
    for plan in &plans {
        host.wait_closed(plan.client)?;
    }
    Ok(plans.into_iter().map(|p| p.chatgpt).collect())
}
pub fn close_reviewed(plans: Vec<RestartPlan>) -> Result<Vec<ClosedClients>> {
    if plans.is_empty() {
        return Ok(vec![]);
    }
    #[cfg(target_os = "macos")]
    {
        Ok(close_plans(&mut macos::MacHost, plans)?.into_iter().map(|desktop| ClosedClients { desktop }).collect())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = plans;
        Err(UNSUPPORTED)
    }
}
/// Extract only the reviewed desktop reopen paths; no process shutdown occurs.
pub fn passive_desktops(plans: Vec<RestartPlan>) -> Vec<ClosedClients> {
    #[cfg(target_os = "macos")]
    {
        plans.into_iter().map(|p| ClosedClients { desktop: p.chatgpt }).collect()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = plans;
        vec![]
    }
}
impl ClosedClients {
    /// Reopen only the service's desktop app, never terminals or editors.
    pub fn reopen(self) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            if let Some(path) = self.desktop {
                macos::MacHost.reopen_chatgpt(&path)?;
            }
        }
        Ok(())
    }
}

pub fn switch_and_reopen_with_progress(
    plan: RestartPlan,
    config: &crate::config::CodexConfig,
    port: u16,
    key: &str,
    progress: &dyn Fn(crate::provider_switch::SwitchProgress),
) -> Result<RestartReport> {
    #[cfg(target_os = "macos")]
    {
        execute(&mut macos::MacHost, plan, |on| {
            crate::provider_switch::switch_provider_with_progress(config, on, port, key, progress).map_err(|error| {
                if crate::provider_switch::recovery_pending(config) {
                    MigrationError::RecoveryNeeded
                } else {
                    MigrationError::Refused(error.fixed_message())
                }
            })
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (plan, config, port, key, progress);
        Err(UNSUPPORTED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn waiting_summary_counts_only_named_clients_and_hides_paths() {
        let clients = summarize_manual(
            "/usr/bin/codex\n/usr/bin/codex\n/Applications/ChatGPT.app/Contents/MacOS/ChatGPT\n/usr/bin/claude\n/usr/bin/bash",
            false,
            &[Client::Codex],
        );
        assert_eq!(clients.iter().map(|c| c.count).sum::<usize>(), 3);
        let json = serde_json::to_string(&clients).unwrap();
        assert!(json.contains("ChatGPT"));
        assert!(!json.contains("/usr") && !json.contains("claude") && !json.contains("bash"));
    }
    #[test]
    fn manual_close_detects_both_services_without_closing_other_apps() {
        let windows = "\"Code.exe\",\"10\"\n\"CLAUDE.EXE\",\"11\"\n\"ChatGPT.exe\",\"12\"";
        assert!(manual_running(windows, true, Client::Codex));
        assert!(manual_running(windows, true, Client::Claude));
        assert!(!manual_running("\"Code.exe\",\"10\"", true, Client::Codex));
        assert!(manual_running("/home/test/.local/share/claude/versions/2.1.227", false, Client::Claude));
        assert!(manual_running("/usr/bin/codex", false, Client::Codex));
        assert!(!manual_running("/usr/bin/aillies\n/usr/bin/claude", false, Client::Codex));
    }
    #[test]
    fn empty_review_needs_no_native_restart_support() {
        assert!(close_reviewed(vec![]).unwrap().is_empty());
    }
    fn process(pid: u32) -> Process {
        Process { pid, parent: 1, uid: 501, tty: "ttys001".into(), started: "stable".into(), exe: "/bin/codex".into() }
    }
    fn plan() -> RestartPlan {
        let p = process(10);
        RestartPlan {
            on: true,
            client: Client::Codex,
            created: Instant::now(),
            processes: vec![p.clone()],
            targets: vec![Target::App { process: p, path: "/Applications/ChatGPT.app".into(), label: "ChatGPT" }],
            chatgpt: Some("/Applications/ChatGPT.app".into()),
        }
    }
    #[derive(Default)]
    struct Fake {
        changed: bool,
        blocked: bool,
        stop_fails: bool,
        reopen_fails: bool,
        events: Vec<&'static str>,
    }
    impl Host for Fake {
        fn codex(&mut self) -> Result<Vec<Process>> {
            Ok(vec![process(if self.changed { 11 } else { 10 })])
        }
        fn stop(&mut self, _: &Target) -> Result<()> {
            self.events.push("stop");
            if self.stop_fails {
                Err(OPEN)
            } else {
                Ok(())
            }
        }
        fn wait_closed(&mut self, _: Client) -> Result<()> {
            self.events.push("wait");
            if self.blocked {
                Err(OPEN)
            } else {
                Ok(())
            }
        }
        fn reopen_chatgpt(&mut self, path: &std::path::Path) -> Result<()> {
            assert_eq!(path, std::path::Path::new("/Applications/ChatGPT.app"));
            self.events.push("reopen");
            if self.reopen_fails {
                Err(OPEN)
            } else {
                Ok(())
            }
        }
    }
    fn migrated(on: bool) -> std::result::Result<SwitchReport, MigrationError> {
        assert!(on);
        Ok(SwitchReport { chats: 7, skipped: 0, history_pending: false, elapsed_ms: 3, reopen_required: true })
    }
    #[test]
    fn combined_close_checks_all_reviews_before_any_stop_and_never_reopens() {
        let mut host = Fake::default();
        let mut claude = plan();
        claude.client = Client::Claude;
        claude.chatgpt = Some("/Applications/Claude.app".into());
        let result = close_plans(&mut host, vec![plan(), claude]).unwrap();
        assert_eq!(host.events, vec!["stop", "stop", "wait", "wait"]);
        assert_eq!(result, vec![Some("/Applications/ChatGPT.app".into()), Some("/Applications/Claude.app".into())]);
        let mut host = Fake::default();
        let mut stale = plan();
        stale.created = Instant::now() - Duration::from_secs(121);
        assert!(close_plans(&mut host, vec![plan(), stale]).is_err());
        assert!(host.events.is_empty());
        let mut host = Fake { blocked: true, ..Default::default() };
        assert!(close_plans(&mut host, vec![plan()]).is_err());
        assert_eq!(host.events, vec!["stop", "wait"]);
    }
    #[test]
    fn restart_waits_before_migration_and_reopens_without_a_prompt() {
        let mut h = Fake::default();
        let r = execute(&mut h, plan(), migrated).unwrap();
        assert_eq!(r.chats, 7);
        assert_eq!(r.reopen_failed, 0);
        assert_eq!(h.events, ["stop", "wait", "reopen"]);
    }
    #[test]
    fn changed_or_expired_plan_does_not_close_anything() {
        let mut h = Fake { changed: true, ..Default::default() };
        assert!(execute(&mut h, plan(), |_| panic!("must not migrate")).is_err());
        assert!(h.events.is_empty());
        let mut p = plan();
        p.created -= Duration::from_secs(121);
        let mut h = Fake::default();
        assert!(execute(&mut h, p, |_| panic!("must not migrate")).is_err());
        assert!(h.events.is_empty());
    }
    #[test]
    fn refusal_reopens_clients_and_never_migrates() {
        for stop_fails in [true, false] {
            let mut h = Fake { blocked: true, stop_fails, ..Default::default() };
            assert!(execute(&mut h, plan(), |_| panic!("must not migrate")).is_err());
            assert_eq!(h.events.last(), Some(&"reopen"));
        }
    }
    #[test]
    fn migration_failure_reopens_only_when_recovery_is_not_pending() {
        let mut h = Fake::default();
        assert!(execute(&mut h, plan(), |_| Err(MigrationError::Refused("Fixed migration refusal"))).is_err());
        assert_eq!(h.events, ["stop", "wait", "reopen"]);
        let mut h = Fake::default();
        let error = execute(&mut h, plan(), |_| Err(MigrationError::RecoveryNeeded)).err().unwrap();
        assert!(error.0.contains("Keep Codex closed"));
        assert_eq!(h.events, ["stop", "wait"]);
    }
    #[test]
    fn confirmed_provider_direction_is_preserved() {
        let mut p = plan();
        p.on = false;
        let mut h = Fake::default();
        execute(&mut h, p, |on| {
            assert!(!on);
            Ok(SwitchReport { chats: 2, skipped: 0, history_pending: false, elapsed_ms: 1, reopen_required: true })
        })
        .unwrap();
    }
    #[test]
    fn app_and_daemon_targets_share_the_shutdown_barrier() {
        let mut p = plan();
        p.targets.push(Target::App { process: process(12), path: "/Applications/Codex.app".into(), label: "Codex" });
        p.targets.push(Target::Daemon(process(13)));
        p.targets.push(Target::Terminal { process: process(14), cwd: "/tmp/project".into(), home: "/tmp/codex".into() });
        p.targets.push(Target::App { process: process(15), path: "/Applications/Visual Studio Code.app".into(), label: "Visual Studio Code" });
        let mut h = Fake::default();
        execute(&mut h, p, migrated).unwrap();
        assert_eq!(h.events, ["stop", "stop", "stop", "stop", "stop", "wait", "reopen"]);
    }
    #[test]
    fn already_closed_chatgpt_reopens_after_terminal_shutdown() {
        let mut p = plan();
        p.targets = vec![Target::Terminal { process: process(10), cwd: "/tmp".into(), home: "/tmp/codex".into() }];
        let mut h = Fake::default();
        execute(&mut h, p, migrated).unwrap();
        assert_eq!(h.events, ["stop", "wait", "reopen"]);
        let mut p = plan();
        p.chatgpt = None;
        let mut h = Fake::default();
        execute(&mut h, p, migrated).unwrap();
        assert_eq!(h.events, ["stop", "wait"]);
    }

    #[test]
    fn reopen_failure_is_separate_from_successful_switch() {
        let mut h = Fake { reopen_fails: true, ..Default::default() };
        let r = execute(&mut h, plan(), migrated).unwrap();
        assert_eq!(r.chats, 7);
        assert_eq!(r.reopen_failed, 1);
    }
}
