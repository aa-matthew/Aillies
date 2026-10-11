//! Explicit desktop restart around the existing offline migration. Never submits a turn.
#[cfg(any(target_os = "macos", test))]
use crate::provider_switch::SwitchReport;
use serde::Serialize;
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
const CHANGED: RestartError = RestartError("Codex sessions changed. Review them again before switching.");
#[cfg(any(target_os = "macos", test))]
const OPEN: RestartError = RestartError("Codex is still open. Finish your work and close it, then try again. The provider was not switched.");
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
#[cfg(any(target_os = "macos", test))]
pub struct RestartPlan {
    on: bool,
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
    pub reopen_failed: usize,
}
#[cfg(any(target_os = "macos", test))]
trait Host {
    fn codex(&mut self) -> Result<Vec<Process>>;
    fn stop(&mut self, target: &Target) -> Result<()>;
    fn wait_closed(&mut self) -> Result<()>;
    fn reopen_chatgpt(&mut self, path: &std::path::Path) -> Result<()>;
}

#[cfg(any(target_os = "macos", test))]
enum MigrationError {
    Refused(&'static str),
    RecoveryNeeded,
}

#[cfg(any(target_os = "macos", test))]
fn execute(host: &mut impl Host, plan: RestartPlan, migrate: impl FnOnce(bool) -> std::result::Result<SwitchReport, MigrationError>) -> Result<RestartReport> {
    if plan.created.elapsed() > Duration::from_secs(120) || host.codex()? != plan.processes {
        return Err(CHANGED);
    }
    let closed = (|| {
        for target in &plan.targets {
            host.stop(target)?;
        }
        host.wait_closed()
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
    switched.map(|r| RestartReport { chats: r.chats, reopen_failed })
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
    fn process(pid: u32) -> Process {
        Process { pid, parent: 1, uid: 501, tty: "ttys001".into(), started: "stable".into(), exe: "/bin/codex".into() }
    }
    fn plan() -> RestartPlan {
        let p = process(10);
        RestartPlan {
            on: true,
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
        fn wait_closed(&mut self) -> Result<()> {
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
        Ok(SwitchReport { chats: 7, elapsed_ms: 3, reopen_required: true })
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
            Ok(SwitchReport { chats: 2, elapsed_ms: 1, reopen_required: true })
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
