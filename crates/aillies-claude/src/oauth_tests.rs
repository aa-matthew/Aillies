use super::*;

fn connection() -> Connection {
    Connection {
        access_token: "sk-ant-oat01-SYNTHETIC".into(),
        refresh_token: "refresh-SYNTHETIC".into(),
        expires_at: now() + 3600,
        account: "11111111-1111-4111-8111-111111111111".into(),
        organization: "22222222-2222-4222-8222-222222222222".into(),
        refresh_pending: false,
    }
}
#[test]
fn pkce_is_random_bound_expiring_and_redacted() {
    let a = Pending::new();
    let b = Pending::new();
    assert_ne!(a.state, b.state);
    assert_ne!(*a.verifier, *b.verifier);
    let url = reqwest::Url::parse(&a.url()).unwrap();
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(q["code_challenge"], URL_SAFE_NO_PAD.encode(Sha256::digest(a.verifier.as_bytes())));
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["redirect_uri"], REDIRECT);
    assert!(!a.url().contains(a.verifier.as_str()));
    assert!(!format!("{a:?}").contains(a.state.as_str()));
    assert!(a.code(&format!("code#{}", a.state)).is_ok());
    assert!(a.code("code#wrong").is_err());
    assert!(a.code("https://evil.example/code").is_err());
    assert!(a.code("code").is_err());
    let old = Pending { at: Instant::now() - Duration::from_secs(601), ..Pending::new() };
    assert!(old.code(&format!("code#{}", old.state)).is_err());
}
#[test]
fn private_store_checks_identity_expiry_and_refresh_uncertainty() {
    let dir = tempfile::tempdir().unwrap();
    let store = AccountStore::fixture(dir.path().join("account.json"));
    assert!(store.read().is_err());
    let mut c = connection();
    store.save(&c).unwrap();
    let first = store.read().unwrap();
    c.access_token = "sk-ant-oat01-ROTATED".into();
    store.save(&c).unwrap();
    assert_eq!(first.account, store.read().unwrap().account);
    assert_ne!(first.token_digest(), store.read().unwrap().token_digest());
    c.refresh_pending = true;
    store.save(&c).unwrap();
    assert_eq!(store.read().unwrap_err().code, FailureCode::ClaudeLoginExpired);
    c.refresh_pending = false;
    c.expires_at = now();
    store.save(&c).unwrap();
    assert_eq!(store.read().unwrap_err().code, FailureCode::ClaudeLoginExpired);
    c = connection();
    c.organization = "unknown".into();
    store.save(&c).unwrap();
    assert!(store.read().is_err());
    assert!(!format!("{c:?} {store:?}").contains("SYNTHETIC"));
    store.delete().unwrap();
    assert!(store.read().is_err());
}
#[cfg(unix)]
#[test]
fn synthetic_store_refuses_symlinks_and_is_private() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("other");
    std::fs::write(&target, b"unchanged").unwrap();
    let path = dir.path().join("account.json");
    symlink(&target, &path).unwrap();
    let store = AccountStore::fixture(path.clone());
    assert!(store.save(&connection()).is_err());
    assert!(store.read().is_err());
    assert!(store.delete().is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"unchanged");
    std::fs::remove_file(&path).unwrap();
    store.save(&connection()).unwrap();
    assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
}
#[cfg(unix)]
#[test]
fn production_store_is_private_atomic_and_never_uses_keychain() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let store = AccountStore::private(dir.path());
    assert!(!store.present());
    store.save(&connection()).unwrap();
    assert!(store.present());
    let folder = dir.path().join("accounts");
    let path = folder.join("claude.json");
    assert_eq!(std::fs::metadata(&folder).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    let mut rotated = connection();
    rotated.access_token = "sk-ant-oat01-ROTATED".into();
    store.save(&rotated).unwrap();
    assert_eq!(store.read().unwrap().access_token, rotated.access_token);
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
    store.delete().unwrap();
    std::fs::remove_dir(&folder).unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    symlink(elsewhere.path(), &folder).unwrap();
    assert!(store.save(&connection()).is_err());
    assert!(store.read().is_err());
    assert!(std::fs::read_dir(elsewhere.path()).unwrap().next().is_none());
}
#[test]
fn token_response_requires_inference_and_verified_identity() {
    let valid = serde_json::json!({"access_token":"sk-ant-oat01-SYNTHETIC", "refresh_token":"refresh-SYNTHETIC", "token_type":"Bearer", "expires_in":3600,"scope":"user:profile user:inference", "account":{"uuid":"11111111-1111-4111-8111-111111111111"}, "organization":{"uuid":"22222222-2222-4222-8222-222222222222"}});
    let mut value = valid.clone();
    assert!(parse_tokens(&value, None).is_ok());
    value["scope"] = "user:profile".into();
    assert!(parse_tokens(&value, None).is_err());
    value = valid.clone();
    value["expires_in"] = 0.into();
    assert!(parse_tokens(&value, None).is_err());
    value = valid.clone();
    value["token_type"] = "Basic".into();
    assert!(parse_tokens(&value, None).is_err());
    value = valid.clone();
    value["refresh_token"] = serde_json::Value::Null;
    assert!(parse_tokens(&value, None).is_err());
    value = valid;
    value["scope"] = serde_json::Value::Null;
    value["refresh_token"] = serde_json::Value::Null;
    assert!(parse_tokens(&value, Some(&connection())).is_ok());
    value["organization"]["uuid"] = "33333333-3333-4333-8333-333333333333".into();
    assert!(parse_tokens(&value, Some(&connection())).is_err());
}
#[test]
fn mock_origins_are_strictly_parsed_loopback() {
    for bad in [
        "https://evil.example",
        "http://127.0.0.1.evil:1",
        "http://user@127.0.0.1:9",
        "http://127.0.0.1:9/path",
        "http://127.0.0.1:9/?secret",
        "http://127.0.0.1",
        "http://10.0.0.1:9",
    ] {
        assert_eq!(origin(Some(bad)), ORIGIN);
    }
    assert_eq!(origin(Some("http://127.0.0.1:1234")), "http://127.0.0.1:1234");
}
