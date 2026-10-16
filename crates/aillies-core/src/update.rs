//! Whole-app update selection, independent of networking and UI.
pub fn stable_upgrade(current: &str, next: &str) -> bool {
    match (semver::Version::parse(current), semver::Version::parse(next)) {
        (Ok(current), Ok(next)) => next.pre.is_empty() && next > current,
        _ => false,
    }
}
pub fn trusted_package(current: &str, version: &str, address: &str, os: &str, arch: &str) -> bool {
    if !stable_upgrade(current, version) {
        return false;
    }
    let Ok(url) = url::Url::parse(address) else {
        return false;
    };
    let name = match (os, arch) {
        ("macos", "aarch64") => format!("Aillies_{version}_aarch64.app.tar.gz"),
        ("windows", "x86_64") => format!("Aillies_{version}_x64-setup.exe"),
        ("linux", "x86_64") => format!("Aillies_{version}_amd64.AppImage"),
        _ => return false,
    };
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path() == format!("/aa-matthew/Aillies/releases/download/v{version}/{name}")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_new_stable_matching_platform_from_the_fixed_release_origin_is_allowed() {
        let path = "/aa-matthew/Aillies/releases/download/v1.2.0/Aillies_1.2.0_x64-setup.exe";
        let url = format!("https://github.com{path}");
        assert!(trusted_package("1.1.3", "1.2.0", &url, "windows", "x86_64"));
        for address in [
            format!("http://github.com{path}"),
            format!("https://github.com.evil.test{path}"),
            format!("https://user@github.com{path}"),
            format!("{url}?token=x"),
            format!("{url}#x"),
            url.replace("v1.2.0/", "v1.1.3/"),
        ] {
            assert!(!trusted_package("1.1.3", "1.2.0", &address, "windows", "x86_64"));
        }
        assert!(!trusted_package("1.1.3", "1.2.0", &url, "macos", "aarch64"));
        assert!(!stable_upgrade("1.2.0", "1.1.3"));
        assert!(!stable_upgrade("1.2.0", "1.2.0"));
        assert!(!stable_upgrade("1.1.3", "1.2.0-alpha.1"));
        assert!(stable_upgrade("1.2.0-alpha.1", "1.2.0"));
    }
}
