//! macOS host operations. No command lines/environments are collected, no force-kill.
use super::*;
use std::{
    collections::BTreeSet,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
};
const INSPECT: RestartError = RestartError("Could not identify every Codex session safely. Close Codex yourself, then switch providers here.");
const REOPEN: RestartError = RestartError("Open ChatGPT to continue your saved chats.");
const LIMIT: usize = 32;
pub(super) struct MacHost;

fn output(command: &mut Command) -> Result<String> {
    let mut child = command.stdin(Stdio::null()).stderr(Stdio::null()).stdout(Stdio::piped()).spawn().map_err(|_| INSPECT)?;
    let stdout = child.stdout.take().ok_or(INSPECT)?;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes).map(|_| bytes)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() < Duration::from_secs(5) => thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let bytes = reader.join().map_err(|_| INSPECT)?.map_err(|_| INSPECT)?;
    if !status.is_some_and(|s| s.success()) || bytes.len() > 4 * 1024 * 1024 {
        return Err(INSPECT);
    }
    String::from_utf8(bytes).map_err(|_| INSPECT)
}
fn run(command: &mut Command) -> Result<()> {
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_| OPEN)?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|_| OPEN)? {
            return if status.success() { Ok(()) } else { Err(OPEN) };
        }
        if start.elapsed() > Duration::from_secs(15) {
            // This only stops the AppleScript/open helper, never the Codex client.
            let _ = child.kill();
            let _ = child.wait();
            return Err(OPEN);
        }
        thread::sleep(Duration::from_millis(100));
    }
}
fn parse_processes(text: &str) -> Result<Vec<Process>> {
    text.lines()
        .map(|line| {
            let mut rest = line.trim();
            let mut fields = Vec::new();
            for _ in 0..9 {
                let end = rest.find(char::is_whitespace).ok_or(INSPECT)?;
                fields.push(&rest[..end]);
                rest = rest[end..].trim_start();
            }
            Ok(Process {
                pid: fields[0].parse().map_err(|_| INSPECT)?,
                parent: fields[1].parse().map_err(|_| INSPECT)?,
                uid: fields[2].parse().map_err(|_| INSPECT)?,
                tty: fields[3].into(),
                started: fields[4..9].join(" "),
                exe: rest.into(),
            })
        })
        .collect()
}
fn processes() -> Result<Vec<Process>> {
    parse_processes(&output(Command::new("/bin/ps").args(["-A", "-o", "pid=,ppid=,uid=,tty=,lstart=,comm="]))?)
}
fn is_codex(p: &Process) -> bool {
    p.exe.file_name().is_some_and(|n| n == "codex" || n == "Codex")
}
fn current_uid() -> Result<u32> {
    output(Command::new("/usr/bin/id").arg("-u"))?.trim().parse().map_err(|_| INSPECT)
}
fn application(p: &Process) -> Option<(PathBuf, &'static str)> {
    let path = p.exe.to_str()?;
    let end = path.find(".app/")? + 4;
    let app = PathBuf::from(&path[..end]);
    let label = match app.file_name()?.to_str()? {
        "ChatGPT.app" => "ChatGPT",
        "Codex.app" => "Codex",
        "Visual Studio Code.app" => "Visual Studio Code",
        "Cursor.app" => "Cursor",
        "Windsurf.app" => "Windsurf",
        _ => return None,
    };
    Some((app, label))
}
fn owning_app(p: &Process, all: &[Process]) -> Option<(Process, PathBuf, &'static str)> {
    let mut current = p;
    let mut found = None;
    for _ in 0..32 {
        if let Some((path, label)) = application(current) {
            found = Some((current.clone(), path, label));
        }
        if current.parent == current.pid {
            break;
        }
        let Some(parent) = all.iter().find(|v| v.pid == current.parent) else {
            break;
        };
        current = parent;
    }
    found
}
fn terminal(p: &Process, home: &Path) -> Result<Target> {
    if p.tty == "??" || !p.exe.is_absolute() || !p.exe.is_file() {
        return Err(INSPECT);
    }
    let listing = output(Command::new("/usr/sbin/lsof").args(["-nP", "-a", "-p", &p.pid.to_string(), "-Fn"]))?;
    let lines: Vec<_> = listing.lines().collect();
    let cwd = lines.windows(2).find_map(|v| (v[0] == "fcwd").then(|| v[1].strip_prefix('n')).flatten()).ok_or(INSPECT)?;
    // Codex's arg0/maintenance lock identifies its data directory without reading its environment.
    let identified = lines
        .iter()
        .filter_map(|v| v.strip_prefix('n'))
        .any(|v| Path::new(v) == home.join(".sqlite-maintenance.lock") || Path::new(v).starts_with(home.join("tmp/arg0")) && v.ends_with("/.lock"));
    let cwd = PathBuf::from(cwd);
    if !identified || !cwd.is_absolute() || !cwd.is_dir() {
        return Err(INSPECT);
    }
    Ok(Target::Terminal { process: p.clone(), cwd, home: home.to_owned() })
}
pub(super) fn prepare(home: &Path, on: bool, token: String) -> Result<(RestartPlan, RestartPreview)> {
    let uid = current_uid()?;
    let all = processes()?;
    let mut codex: Vec<_> = all.iter().filter(|p| is_codex(p)).cloned().collect();
    codex.sort_by_key(|p| p.pid);
    if codex.len() > LIMIT || codex.iter().any(|p| p.uid != uid) {
        return Err(INSPECT);
    }
    let home = home.canonicalize().map_err(|_| INSPECT)?;
    let mut apps = BTreeSet::new();
    let mut targets = Vec::new();
    let mut terminals = 0;
    for p in &codex {
        if let Some((owner, path, label)) = owning_app(p, &all) {
            if owner.uid != uid || !path.is_dir() {
                return Err(INSPECT);
            }
            if apps.insert(path.clone()) {
                targets.push(Target::App { process: owner, path, label });
            }
        } else if p.exe.starts_with(home.join("packages/app-server-daemon/releases")) && p.tty == "??" {
            targets.push(Target::Daemon(p.clone()));
        } else {
            targets.push(terminal(p, &home)?);
            terminals += 1;
        }
    }
    // Quit applications before signalling any remaining daemon or terminal clients.
    targets.sort_by_key(|t| match t {
        Target::App { .. } => 0,
        Target::Terminal { .. } => 1,
        Target::Daemon(_) => 2,
    });
    let labels = targets.iter().filter_map(|t| if let Target::App { label, .. } = t { Some(*label) } else { None }).collect();
    let chatgpt = all.iter().find_map(|p| application(p).filter(|(_, label)| *label == "ChatGPT").map(|(path, _)| path)).or_else(|| {
        [PathBuf::from("/Applications/ChatGPT.app"), PathBuf::from(std::env::var_os("HOME")?).join("Applications/ChatGPT.app")]
            .into_iter()
            .find(|path| path.is_dir())
    });
    Ok((RestartPlan { on, created: Instant::now(), processes: codex, targets, chatgpt }, RestartPreview { token, on, apps: labels, terminals }))
}
fn same_process(p: &Process) -> Result<bool> {
    Ok(processes()?.iter().any(|v| v.pid == p.pid && v.uid == p.uid && v.started == p.started && v.exe == p.exe))
}
impl Host for MacHost {
    fn codex(&mut self) -> Result<Vec<Process>> {
        let mut found: Vec<_> = processes()?.into_iter().filter(is_codex).collect();
        found.sort_by_key(|p| p.pid);
        Ok(found)
    }
    fn stop(&mut self, target: &Target) -> Result<()> {
        let p = target.process();
        if !same_process(p)? {
            return Ok(());
        }
        match target {
            Target::App { path, .. } => {
                run(Command::new("/usr/bin/osascript").args(["-e", "on run argv\ntell application (item 1 of argv) to quit\nend run"]).arg(path))?;
                let start = Instant::now();
                while same_process(p)? {
                    if start.elapsed() > Duration::from_secs(15) {
                        return Err(OPEN);
                    }
                    thread::sleep(Duration::from_millis(200));
                }
                Ok(())
            }
            _ => run(Command::new("/bin/kill").args(["-TERM", &p.pid.to_string()])),
        }
    }
    fn wait_closed(&mut self) -> Result<()> {
        let start = Instant::now();
        let mut empty = false;
        loop {
            if self.codex()?.is_empty() {
                if empty {
                    return Ok(());
                }
                empty = true;
            } else {
                empty = false;
            }
            if start.elapsed() > Duration::from_secs(15) {
                return Err(OPEN);
            }
            thread::sleep(Duration::from_millis(250));
        }
    }
    fn reopen_chatgpt(&mut self, path: &Path) -> Result<()> {
        if path.file_name().is_none_or(|name| name != "ChatGPT.app") || !path.is_dir() {
            return Err(REOPEN);
        }
        run(Command::new("/usr/bin/open").arg(path)).map_err(|_| REOPEN)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_parser_keeps_paths_with_spaces_and_start_identity() {
        let p = parse_processes(" 12 1 501 ?? Sun Oct 11 21:01:03 2026 /Applications/Visual Studio Code.app/Contents/MacOS/Electron").unwrap().remove(0);
        assert_eq!(p.started, "Sun Oct 11 21:01:03 2026");
        assert_eq!(application(&p).unwrap().1, "Visual Studio Code");
        assert!(!is_codex(&p));
        assert!(parse_processes("12 malformed").is_err());
    }
    #[test]
    #[ignore = "read-only inspection of this Mac; never closes or launches anything"]
    fn local_process_preview_is_read_only() {
        let home = crate::codex_home().unwrap();
        let (plan, preview) = prepare(&home, true, "test-only".into()).unwrap();
        assert_eq!(plan.processes.len(), MacHost.codex().unwrap().len());
        println!("Read-only preview: {} apps, {} terminal sessions", preview.apps.len(), preview.terminals);
    }
}
