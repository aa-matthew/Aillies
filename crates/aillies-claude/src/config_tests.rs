use super::*;
use serde_json::json;

#[test]
fn install_restore_preserves_login_history_and_later_edits() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("claude");
    std::fs::create_dir_all(home.join("projects")).unwrap();
    let transcript = home.join("projects/session.jsonl");
    std::fs::write(&transcript, b"original history").unwrap();
    let settings = home.join("settings.json");
    let original = json!({"permissions":{"defaultMode":"default"},"env":{"ANTHROPIC_BASE_URL":"https://old.example","KEEP":"yes"}});
    std::fs::write(&settings, serde_json::to_vec(&original).unwrap()).unwrap();
    let cfg = Config { home, receipts: dir.path().join("aillies") };
    cfg.install(12345, "local-secret").unwrap();
    cfg.install(12345, "local-secret").unwrap();
    let mut installed: Value = serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
    assert_eq!(installed["env"]["CLAUDE_CODE_MAX_RETRIES"], "0");
    assert_eq!(installed["env"]["ANTHROPIC_API_KEY"], "");
    installed["theme"] = json!("dark");
    installed["env"]["KEEP"] = json!("later");
    std::fs::write(&settings, serde_json::to_vec(&installed).unwrap()).unwrap();
    cfg.restore().unwrap();
    let restored: Value = serde_json::from_slice(&std::fs::read(settings).unwrap()).unwrap();
    assert_eq!(restored["env"]["ANTHROPIC_BASE_URL"], "https://old.example");
    assert_eq!(restored["env"]["KEEP"], "later");
    assert_eq!(restored["theme"], "dark");
    assert!(restored["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
    assert_eq!(std::fs::read(transcript).unwrap(), b"original history");
}

#[test]
fn malformed_settings_and_managed_backends_fail_without_changes() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config { home: dir.path().join("claude"), receipts: dir.path().join("aillies") };
    std::fs::create_dir_all(&cfg.home).unwrap();
    for bytes in [b"not json".as_slice(), br#"{"env":{"CLAUDE_CODE_USE_BEDROCK":"1"}}"#] {
        std::fs::write(cfg.home.join("settings.json"), bytes).unwrap();
        assert!(cfg.install(12345, "secret").is_err());
        assert_eq!(std::fs::read(cfg.home.join("settings.json")).unwrap(), bytes);
        assert!(!cfg.is_installed());
    }
}

#[test]
fn restore_keeps_user_changes_to_managed_values() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config { home: dir.path().join("claude"), receipts: dir.path().join("aillies") };
    cfg.install(12345, "secret").unwrap();
    let path = cfg.home.join("settings.json");
    let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["env"]["ANTHROPIC_BASE_URL"] = json!("https://later.example");
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    cfg.restore().unwrap();
    let value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(value["env"]["ANTHROPIC_BASE_URL"], "https://later.example");
}

#[test]
fn restore_refuses_damaged_or_different_home_and_recovers_an_interrupted_install() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config { home: dir.path().join("claude"), receipts: dir.path().join("aillies") };
    cfg.install(12345, "secret").unwrap();
    let receipt = std::fs::read(cfg.receipt()).unwrap();
    let path = cfg.home.join("settings.json");
    std::fs::write(&path, b"damaged").unwrap();
    assert!(cfg.restore().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"damaged");
    assert_eq!(std::fs::read(cfg.receipt()).unwrap(), receipt);
    // A crash after writing the receipt but before writing settings leaves original bytes.
    std::fs::write(&path, b"{}").unwrap();
    let different = Config { home: dir.path().join("different"), receipts: cfg.receipts.clone() };
    assert!(different.restore().is_err());
    cfg.restore().unwrap();
    assert!(!path.exists());
    assert!(!cfg.receipt().exists());
}

#[cfg(unix)]
#[test]
fn settings_symlink_is_refused_without_touching_its_target() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config { home: dir.path().join("claude"), receipts: dir.path().join("aillies") };
    std::fs::create_dir_all(&cfg.home).unwrap();
    let target = dir.path().join("target.json");
    std::fs::write(&target, b"{}").unwrap();
    std::os::unix::fs::symlink(&target, cfg.home.join("settings.json")).unwrap();
    assert!(cfg.install(12345, "secret").is_err());
    assert_eq!(std::fs::read(target).unwrap(), b"{}");
    assert!(!cfg.receipt().exists());
}
