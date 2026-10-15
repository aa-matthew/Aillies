use super::*;
use std::{
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{
        Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED},
        Registry::*,
        Threading::*,
    },
    UI::{Shell::*, WindowsAndMessaging::SW_HIDE},
};
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Local(*mut std::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
unsafe fn wide_string(value: *const u16) -> Result<String, AccessCode> {
    if value.is_null() {
        return Err(AccessCode::RepairFailed);
    }
    let mut n = 0;
    while n < 32768 && unsafe { *value.add(n) } != 0 {
        n += 1;
    }
    if n == 32768 {
        return Err(AccessCode::RepairFailed);
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(value, n) }).map_err(|_| AccessCode::RepairFailed)
}
fn sid_string(sid: PSID) -> Result<String, AccessCode> {
    let mut pointer = null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut pointer) } == 0 {
        return Err(AccessCode::RepairFailed);
    }
    let _memory = Local(pointer.cast());
    unsafe { wide_string(pointer) }
}
pub(super) fn current_identity() -> Result<(String, PathBuf), AccessCode> {
    let mut raw = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(AccessCode::RepairFailed);
    }
    let token = Handle(raw);
    let mut size = 0;
    unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut size) };
    if size == 0 || size > 65536 {
        return Err(AccessCode::RepairFailed);
    }
    let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
    if unsafe { GetTokenInformation(token.0, TokenUser, buffer.as_mut_ptr().cast(), size, &mut size) } == 0 {
        return Err(AccessCode::RepairFailed);
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let identity = sid_string(user.User.Sid)?;
    let mut path = null_mut();
    if unsafe { SHGetKnownFolderPath(&FOLDERID_Profile, 0, null_mut(), &mut path) } < 0 {
        return Err(AccessCode::RepairFailed);
    }
    let home = unsafe { wide_string(path) };
    unsafe { CoTaskMemFree(path.cast()) };
    Ok((identity, PathBuf::from(home?)))
}
fn open(path: &Path, access: u32) -> Result<fs::File, AccessCode> {
    let name = wide(path.as_os_str());
    let handle = unsafe {
        CreateFileW(name.as_ptr(), access, FILE_SHARE_READ, null(), OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT, null_mut())
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(access_code(&io::Error::last_os_error()));
    }
    let file = unsafe { fs::File::from_raw_handle(handle) };
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    if unsafe { GetFileInformationByHandle(handle, info.as_mut_ptr()) } == 0 {
        return Err(AccessCode::RepairFailed);
    }
    let info = unsafe { info.assume_init() };
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 && info.nNumberOfLinks != 1) {
        return Err(AccessCode::UnsafePath);
    }
    Ok(file)
}
fn pin_parents(path: &Path) -> Result<Vec<fs::File>, AccessCode> {
    let mut pins = Vec::new();
    let mut current = PathBuf::new();
    for component in path.parent().ok_or(AccessCode::UnsafePath)?.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(AccessCode::UnsafePath);
        }
        current.push(component);
        if current.has_root() {
            pins.push(open(&current, FILE_READ_ATTRIBUTES)?);
        }
    }
    Ok(pins)
}
fn grant(file: &fs::File, identity: &str, admin: Option<&str>, directory: bool) -> Result<(), AccessCode> {
    let mut owner = null_mut();
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    let handle = file.as_raw_handle();
    if unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    } != ERROR_SUCCESS
    {
        return Err(AccessCode::RepairFailed);
    }
    let _descriptor = Local(descriptor);
    let owner = sid_string(owner)?;
    if owner != identity && Some(owner.as_str()) != admin && owner != "S-1-5-18" && owner != "S-1-5-32-544" {
        return Err(AccessCode::UnsafePath);
    }
    // A null DACL already grants access to everyone. Never manufacture one or
    // replace that policy as an incidental repair; leave it for manual review.
    if dacl.is_null() {
        return Err(AccessCode::UnsafePath);
    }
    let identity = wide(identity.as_ref());
    let mut sid = null_mut();
    if unsafe { ConvertStringSidToSidW(identity.as_ptr(), &mut sid) } == 0 {
        return Err(AccessCode::RepairFailed);
    }
    let _sid = Local(sid);
    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_GENERIC_READ | FILE_GENERIC_WRITE | if directory { FILE_GENERIC_EXECUTE | FILE_DELETE_CHILD } else { DELETE },
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W { TrusteeForm: TRUSTEE_IS_SID, TrusteeType: TRUSTEE_IS_USER, ptstrName: sid.cast(), ..Default::default() },
    };
    let mut updated = null_mut();
    if unsafe { SetEntriesInAclW(1, &entry, dacl, &mut updated) } != ERROR_SUCCESS {
        return Err(AccessCode::RepairFailed);
    }
    let _updated = Local(updated.cast());
    let result = unsafe { SetSecurityInfo(handle, SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, null_mut(), null_mut(), updated, null()) };
    if result != ERROR_SUCCESS {
        return Err(access_code(&io::Error::from_raw_os_error(result as i32)));
    }
    if !directory {
        let mut basic = FILE_BASIC_INFO::default();
        if unsafe {
            GetFileInformationByHandleEx(handle, FileBasicInfo, (&mut basic as *mut FILE_BASIC_INFO).cast(), std::mem::size_of::<FILE_BASIC_INFO>() as u32)
        } == 0
        {
            return Err(access_code(&io::Error::last_os_error()));
        }
        if basic.FileAttributes & FILE_ATTRIBUTE_READONLY != 0 {
            basic.FileAttributes &= !FILE_ATTRIBUTE_READONLY;
            if basic.FileAttributes == 0 {
                basic.FileAttributes = FILE_ATTRIBUTE_NORMAL;
            }
            if unsafe {
                SetFileInformationByHandle(handle, FileBasicInfo, (&basic as *const FILE_BASIC_INFO).cast(), std::mem::size_of::<FILE_BASIC_INFO>() as u32)
            } == 0
            {
                return Err(access_code(&io::Error::last_os_error()));
            }
        }
    }
    Ok(())
}
fn elevated() -> bool {
    let mut raw = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return false;
    }
    let token = Handle(raw);
    let mut info = TOKEN_ELEVATION::default();
    let mut size = 0;
    (unsafe {
        GetTokenInformation(token.0, TokenElevation, (&mut info as *mut TOKEN_ELEVATION).cast(), std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut size)
    }) != 0
        && info.TokenIsElevated != 0
}
fn profile_for(identity: &str) -> Result<PathBuf, AccessCode> {
    if !identity.starts_with("S-") || !identity.bytes().all(|b| b.is_ascii_digit() || b == b'S' || b == b'-') {
        return Err(AccessCode::UnsafePath);
    }
    let key = wide(format!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\ProfileList\\{identity}").as_ref());
    let name = wide("ProfileImagePath".as_ref());
    let mut buffer = vec![0u16; 32768];
    let mut bytes = (buffer.len() * 2) as u32;
    if unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND,
            null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    } != ERROR_SUCCESS
    {
        return Err(AccessCode::UnsafePath);
    }
    let mut profile = unsafe { wide_string(buffer.as_ptr()) }?;
    // Expand only the OS system-drive placeholder using a native known folder,
    // never variables from an elevated or attacker-controlled environment.
    if profile.to_ascii_lowercase().starts_with("%systemdrive%") {
        let mut windows = null_mut();
        if unsafe { SHGetKnownFolderPath(&FOLDERID_Windows, 0, null_mut(), &mut windows) } < 0 {
            return Err(AccessCode::RepairFailed);
        }
        let folder = unsafe { wide_string(windows) };
        unsafe { CoTaskMemFree(windows.cast()) };
        let folder = PathBuf::from(folder?);
        let drive = folder.components().next().ok_or(AccessCode::UnsafePath)?.as_os_str().to_string_lossy().to_string();
        profile = format!("{drive}{}", &profile[13..]);
    }
    let home = PathBuf::from(profile);
    if !home.is_absolute() || home.as_os_str().to_string_lossy().contains('%') {
        return Err(AccessCode::UnsafePath);
    }
    Ok(home)
}
pub(super) fn repair(request: &RepairRequest) -> Result<(), AccessCode> {
    if !request.valid() {
        return Err(AccessCode::RepairFailed);
    }
    let (caller, home) = current_identity()?;
    let (home, admin) = if caller == request.identity {
        (home, None)
    } else if elevated() {
        (profile_for(&request.identity)?, Some(caller.as_str()))
    } else {
        return Err(AccessCode::PermissionDenied);
    };
    repair_in(&home, &request.identity, admin, &request.targets)
}
fn repair_in(home: &Path, identity: &str, admin: Option<&str>, targets: &[AccessTarget]) -> Result<(), AccessCode> {
    let mut pins = Vec::new();
    let mut files = Vec::new();
    for target in targets {
        let path = home.join(target.relative());
        pins.extend(pin_parents(&path)?);
        let file = open(&path, READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES)?;
        if file.metadata().map_err(|_| AccessCode::RepairFailed)?.is_dir() != target.directory() {
            return Err(AccessCode::UnsafePath);
        }
        files.push((file, *target));
    }
    for (file, target) in files {
        grant(&file, identity, admin, target.directory())?;
    }
    drop(pins);
    Ok(())
}
pub(super) fn elevate(request: &RepairRequest) -> Result<(), AccessCode> {
    let exe = std::env::current_exe().map_err(|_| AccessCode::RepairFailed)?;
    let binary = wide(exe.as_os_str());
    let verb = wide("runas".as_ref());
    let parameters = wide(format!("{} {}", HELPER_FLAG, encoded(request)?).as_ref());
    std::thread::spawn(move || {
        if unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) } < 0 {
            return Err(AccessCode::RepairFailed);
        }
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe { CoUninitialize() };
            }
        }
        let _apartment = Apartment;
        let mut launch = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpVerb: verb.as_ptr(),
            lpFile: binary.as_ptr(),
            lpParameters: parameters.as_ptr(),
            nShow: SW_HIDE,
            ..Default::default()
        };
        if unsafe { ShellExecuteExW(&mut launch) } == 0 {
            return Err(if unsafe { GetLastError() } == ERROR_CANCELLED { AccessCode::RepairCancelled } else { AccessCode::RepairFailed });
        }
        if launch.hProcess.is_null() {
            return Err(AccessCode::RepairFailed);
        }
        let process = Handle(launch.hProcess);
        if unsafe { WaitForSingleObject(process.0, 300_000) } != WAIT_OBJECT_0 {
            return Err(AccessCode::RepairFailed);
        }
        let mut code = 1;
        if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 || code != 0 {
            return Err(AccessCode::RepairFailed);
        }
        Ok(())
    })
    .join()
    .map_err(|_| AccessCode::RepairFailed)?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_repair_keeps_content_and_refuses_hard_links() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        fs::create_dir(home.join(".codex")).unwrap();
        let file = home.join(".codex/config.toml");
        fs::write(&file, b"PRIVATE_CONFIG").unwrap();
        let (identity, _) = current_identity().unwrap();
        repair_in(home, &identity, None, &[AccessTarget::CodexConfig]).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"PRIVATE_CONFIG");
        fs::hard_link(&file, home.join("outside")).unwrap();
        assert_eq!(repair_in(home, &identity, None, &[AccessTarget::CodexConfig]).unwrap_err(), AccessCode::UnsafePath);
    }
    #[test]
    fn unknown_profile_cannot_redirect_the_repair() {
        let (identity, _) = current_identity().unwrap();
        let request = RepairRequest { identity: format!("{identity}-1"), targets: vec![AccessTarget::CodexConfig] };
        assert!(repair(&request).is_err());
    }
}
