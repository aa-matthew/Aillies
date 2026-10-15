use super::*;
use std::{
    ffi::{CStr, CString},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    process::{Command, Stdio},
};
fn identity_home(identity: &str) -> Result<(u32, PathBuf), AccessCode> {
    let uid = identity.parse::<u32>().map_err(|_| AccessCode::RepairFailed)?;
    if uid == 0 {
        return Err(AccessCode::RepairFailed);
    }
    let mut storage = vec![0u8; 16384];
    let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut found = std::ptr::null_mut();
    let result = unsafe { libc::getpwuid_r(uid, entry.as_mut_ptr(), storage.as_mut_ptr().cast(), storage.len(), &mut found) };
    if result != 0 || found.is_null() {
        return Err(AccessCode::RepairFailed);
    }
    let entry = unsafe { entry.assume_init() };
    if entry.pw_dir.is_null() {
        return Err(AccessCode::RepairFailed);
    }
    let home = PathBuf::from(std::ffi::OsStr::from_bytes(unsafe { CStr::from_ptr(entry.pw_dir) }.to_bytes()));
    if !home.is_absolute() || home.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(AccessCode::RepairFailed);
    }
    Ok((uid, home))
}
pub(super) fn current_identity() -> Result<(String, PathBuf), AccessCode> {
    let identity = unsafe { libc::getuid() }.to_string();
    let (_, home) = identity_home(&identity)?;
    Ok((identity, home))
}
fn open_target(home: &Path, target: AccessTarget) -> Result<fs::File, AccessCode> {
    use std::path::Component;
    let path = home.join(target.relative());
    let parts: Vec<_> = path
        .components()
        .filter_map(|p| match p {
            Component::Normal(s) => Some(s),
            _ => None,
        })
        .collect();
    let mut parent = fs::File::open("/").map_err(|_| AccessCode::RepairFailed)?;
    for (i, part) in parts.iter().enumerate() {
        let name = CString::new(part.as_bytes()).map_err(|_| AccessCode::UnsafePath)?;
        let directory = i + 1 < parts.len() || target.directory();
        let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK | if directory { libc::O_DIRECTORY } else { 0 };
        let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(access_code(&io::Error::last_os_error()));
        }
        parent = unsafe { fs::File::from_raw_fd(fd) };
    }
    let meta = parent.metadata().map_err(|_| AccessCode::RepairFailed)?;
    if meta.is_dir() != target.directory() || (!target.directory() && (!meta.is_file() || meta.nlink() != 1)) {
        return Err(AccessCode::UnsafePath);
    }
    Ok(parent)
}
fn repair_in(home: &Path, uid: u32, targets: &[AccessTarget]) -> Result<(), AccessCode> {
    // Preflight all handles before any changes. No content is read, and neither
    // symlinks nor hard-linked files can redirect privileged metadata changes.
    let mut files = Vec::new();
    for target in targets {
        let file = open_target(home, *target)?;
        let owner = file.metadata().map_err(|_| AccessCode::RepairFailed)?.uid();
        if owner != uid && owner != 0 {
            return Err(AccessCode::UnsafePath);
        }
        files.push((file, *target, owner));
    }
    for (file, target, owner) in files {
        if owner != uid && unsafe { libc::fchown(file.as_raw_fd(), uid, !0) } != 0 {
            return Err(access_code(&io::Error::last_os_error()));
        }
        if unsafe { libc::fchmod(file.as_raw_fd(), if target.directory() { 0o700 } else { 0o600 }) } != 0 {
            return Err(access_code(&io::Error::last_os_error()));
        }
    }
    Ok(())
}
pub(super) fn repair(request: &RepairRequest) -> Result<(), AccessCode> {
    if !request.valid() {
        return Err(AccessCode::RepairFailed);
    }
    let (uid, home) = identity_home(&request.identity)?;
    let caller = unsafe { libc::geteuid() };
    if caller != 0 && caller != uid {
        return Err(AccessCode::RepairFailed);
    }
    repair_in(&home, uid, &request.targets)
}
#[cfg(target_os = "macos")]
fn shell_quote(value: &std::ffi::OsStr) -> Result<String, AccessCode> {
    let value = value.to_str().ok_or(AccessCode::UnsafePath)?;
    if value.contains(['\0', '\n', '\r']) {
        return Err(AccessCode::UnsafePath);
    }
    Ok(format!("'{}'", value.replace('\'', "'\"'\"'")))
}
pub(super) fn elevate(request: &RepairRequest) -> Result<(), AccessCode> {
    let exe = std::env::current_exe().map_err(|_| AccessCode::RepairFailed)?;
    let payload = encoded(request)?;
    #[cfg(target_os = "macos")]
    {
        let command = format!("{} {} {}", shell_quote(exe.as_os_str())?, shell_quote(HELPER_FLAG.as_ref())?, shell_quote(payload.as_ref())?);
        let script="on run argv\ntry\ndo shell script (item 1 of argv) with administrator privileges with prompt \"Aillies needs permission to restore your access to its settings files. Config contents and chats will not be changed.\"\nreturn \"ok\"\non error number n\nif n is -128 then return \"cancelled\"\nreturn \"failed\"\nend try\nend run";
        let output = Command::new("/usr/bin/osascript")
            .args(["-e", script, "--", &command])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| if e.kind() == io::ErrorKind::NotFound { AccessCode::AuthenticationUnavailable } else { AccessCode::RepairFailed })?;
        match output.stdout.as_slice() {
            b"ok\n" => Ok(()),
            b"cancelled\n" => Err(AccessCode::RepairCancelled),
            _ => Err(AccessCode::RepairFailed),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let status = Command::new("/usr/bin/pkexec")
            .arg("--disable-internal-agent")
            .arg(exe)
            .args([HELPER_FLAG, &payload])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| if e.kind() == io::ErrorKind::NotFound { AccessCode::AuthenticationUnavailable } else { AccessCode::RepairFailed })?;
        match status.code() {
            Some(0) => Ok(()),
            Some(126) => Err(AccessCode::RepairCancelled),
            Some(127) => Err(AccessCode::AuthenticationFailed),
            _ => Err(AccessCode::RepairFailed),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_command_quoting_keeps_paths_literal() {
        let literal = "/Applications/Aillies's app; $(printf injected)";
        let quoted = shell_quote(literal.as_ref()).unwrap();
        let output = Command::new("/bin/sh").args(["-c", &format!("printf '%s' {quoted}")]).output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, literal.as_bytes());
        assert_eq!(shell_quote("bad\npath".as_ref()).unwrap_err(), AccessCode::UnsafePath);
    }
    #[test]
    fn scoped_metadata_repair_preserves_content_and_rejects_links_before_changes() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().canonicalize().unwrap();
        fs::create_dir(home.join(".codex")).unwrap();
        let file = home.join(".codex/config.toml");
        fs::write(&file, b"PRIVATE_CONTENT").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o400)).unwrap();
        let uid = unsafe { libc::getuid() };
        repair_in(&home, uid, &[AccessTarget::CodexConfig]).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"PRIVATE_CONTENT");
        assert_eq!(fs::metadata(&file).unwrap().mode() & 0o777, 0o600);
        let outside = home.join("outside");
        fs::write(&outside, b"UNTOUCHED").unwrap();
        fs::remove_file(&file).unwrap();
        symlink(&outside, &file).unwrap();
        assert!(repair_in(&home, uid, &[AccessTarget::CodexFolder, AccessTarget::CodexConfig]).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"UNTOUCHED");
        fs::remove_file(&file).unwrap();
        fs::hard_link(&outside, &file).unwrap();
        assert_eq!(repair_in(&home, uid, &[AccessTarget::CodexConfig]).unwrap_err(), AccessCode::UnsafePath);
    }
}
