use super::*;
use serde_json::json;
fn fixture() -> (tempfile::TempDir, DesktopConfig) {
    let dir = tempfile::tempdir().unwrap();
    let config = DesktopConfig { library: dir.path().join("configLibrary"), receipts: dir.path().join("aillies") };
    (dir, config)
}
#[test]
fn desktop_switch_restores_previous_selection_and_preserves_later_configs() {
    let (_dir, cfg) = fixture();
    std::fs::create_dir_all(&cfg.library).unwrap();
    let previous = uuid::Uuid::new_v4().to_string();
    let before = json!({"appliedId":previous,"entries":[{"id":previous,"name":"Work"}],"other":"keep"});
    private_write(&cfg.meta(), &serde_json::to_vec(&before).unwrap()).unwrap();
    cfg.install(18522, "LOCAL_ONLY").unwrap();
    cfg.install(18522, "LOCAL_ONLY").unwrap();
    let installed_bytes = std::fs::read(cfg.meta()).unwrap();
    assert!(cfg.install(18523, "LOCAL_ONLY").is_err());
    assert!(cfg.install(18522, "CHANGED_TOKEN").is_err());
    assert_eq!(std::fs::read(cfg.meta()).unwrap(), installed_bytes);
    let mut current = object(read(&cfg.meta()).unwrap().as_deref()).unwrap();
    let installed = current["appliedId"].as_str().unwrap();
    let profile: Value = serde_json::from_slice(&std::fs::read(cfg.library.join(format!("{installed}.json"))).unwrap()).unwrap();
    assert_eq!(profile["inferenceProvider"], "gateway");
    assert_eq!(profile["inferenceGatewayBaseUrl"], "http://127.0.0.1:18522/claude-desktop");
    assert_eq!(profile["inferenceGatewayApiKey"], "LOCAL_ONLY");
    assert_eq!(profile["disableDeploymentModeChooser"], true);
    assert_eq!(profile["modelDiscoveryEnabled"], true);
    assert!(profile.get("inferenceModels").is_none(), "static models must not override discovery");
    current["entries"].as_array_mut().unwrap().push(json!({"id":uuid::Uuid::new_v4(),"name":"Later"}));
    current.insert("later".into(), json!(true));
    private_write(&cfg.meta(), &serde_json::to_vec(&current).unwrap()).unwrap();
    cfg.restore().unwrap();
    let restored = object(read(&cfg.meta()).unwrap().as_deref()).unwrap();
    assert_eq!(restored["appliedId"], previous);
    assert_eq!(restored["entries"].as_array().unwrap().len(), 2);
    assert_eq!(restored["later"], true);
    assert!(!cfg.is_installed());
}
#[test]
fn desktop_restore_recovers_partial_install_without_changing_other_selection() {
    let (_dir, cfg) = fixture();
    cfg.install(18522, "LOCAL_ONLY").unwrap();
    let receipt = std::fs::read(cfg.receipt()).unwrap();
    // Crash after receipt, before profile/meta writes.
    std::fs::remove_file(cfg.meta()).unwrap();
    let r: Receipt = serde_json::from_slice(&receipt).unwrap();
    std::fs::remove_file(cfg.profile(&r.id).unwrap()).unwrap();
    cfg.restore().unwrap();
    assert!(!cfg.meta().exists());
    cfg.install(18522, "LOCAL_ONLY").unwrap();
    let mut current = object(read(&cfg.meta()).unwrap().as_deref()).unwrap();
    current.insert("appliedId".into(), json!(""));
    private_write(&cfg.meta(), &serde_json::to_vec(&current).unwrap()).unwrap();
    cfg.restore().unwrap();
    assert_eq!(object(read(&cfg.meta()).unwrap().as_deref()).unwrap()["appliedId"], "");
}
#[test]
fn desktop_restore_removes_owned_routing_from_edited_profile_and_keeps_user_fields() {
    let (_dir, cfg) = fixture();
    std::fs::create_dir_all(&cfg.library).unwrap();
    private_write(&cfg.meta(), b"not JSON").unwrap();
    assert!(cfg.install(18522, "secret").is_err());
    assert!(!cfg.receipt().exists());
    std::fs::remove_file(cfg.meta()).unwrap();
    cfg.install(18522, "secret").unwrap();
    let r: Receipt = serde_json::from_slice(&std::fs::read(cfg.receipt()).unwrap()).unwrap();
    let mut changed = r.installed.clone();
    changed["deploymentDisplayName"] = json!("Changed");
    changed["userSetting"] = json!("keep");
    private_write(&cfg.profile(&r.id).unwrap(), &serde_json::to_vec(&changed).unwrap()).unwrap();
    cfg.restore().unwrap();
    let kept: Value = serde_json::from_slice(&std::fs::read(cfg.profile(&r.id).unwrap()).unwrap()).unwrap();
    assert_eq!(kept["deploymentDisplayName"], "Changed");
    assert_eq!(kept["userSetting"], "keep");
    assert!(kept.get("inferenceGatewayBaseUrl").is_none());
    assert!(kept.get("inferenceGatewayApiKey").is_none());
    assert!(!cfg.is_installed());
    #[cfg(unix)]
    {
        let (_dir, other) = fixture();
        std::os::unix::fs::symlink(&cfg.library, &other.library).unwrap();
        assert!(other.install(18522, "secret").is_err());
    }
}
#[test]
fn desktop_discovery_upgrade_is_journaled_and_restore_accepts_both_crash_points() {
    for profile_updated in [false, true] {
        let (_dir, cfg) = fixture();
        cfg.install(18522, "LOCAL_ONLY").unwrap();
        let mut r = cfg.load_receipt().unwrap().unwrap();
        r.installed["modelDiscoveryEnabled"] = false.into();
        r.installed["inferenceModels"] = json!(["claude-old"]);
        private_write(&cfg.profile(&r.id).unwrap(), &serde_json::to_vec(&r.installed).unwrap()).unwrap();
        private_write(&cfg.receipt(), &serde_json::to_vec(&r).unwrap()).unwrap();
        assert!(cfg.upgrade_discovery().unwrap());
        let upgraded = cfg.load_receipt().unwrap().unwrap();
        assert_eq!(upgraded.installed["modelDiscoveryEnabled"], true);
        assert!(upgraded.installed.get("inferenceModels").is_none());
        assert!(!cfg.upgrade_discovery().unwrap());
        // Crash after upgrade intent, before or after the profile replacement.
        r.pending = Some(upgraded.installed.clone());
        private_write(&cfg.receipt(), &serde_json::to_vec(&r).unwrap()).unwrap();
        if !profile_updated {
            private_write(&cfg.profile(&r.id).unwrap(), &serde_json::to_vec(&r.installed).unwrap()).unwrap();
        }
        assert!(cfg.upgrade_discovery().unwrap(), "resume either known crash state");
        assert_eq!(cfg.load_receipt().unwrap().unwrap().installed["modelDiscoveryEnabled"], true);
        // Recreate the same interruption and verify restore independently.
        private_write(&cfg.receipt(), &serde_json::to_vec(&r).unwrap()).unwrap();
        if !profile_updated {
            private_write(&cfg.profile(&r.id).unwrap(), &serde_json::to_vec(&r.installed).unwrap()).unwrap();
        }
        cfg.restore().unwrap();
        assert!(!cfg.is_installed());
    }
}
#[test]
fn desktop_discovery_upgrade_refuses_external_edits_without_mutation() {
    let (_dir, cfg) = fixture();
    cfg.install(18522, "LOCAL_ONLY").unwrap();
    let mut r = cfg.load_receipt().unwrap().unwrap();
    r.installed["modelDiscoveryEnabled"] = false.into();
    private_write(&cfg.receipt(), &serde_json::to_vec(&r).unwrap()).unwrap();
    let before = std::fs::read(cfg.receipt()).unwrap();
    // The profile no longer matches the original owned receipt.
    assert!(cfg.upgrade_discovery().is_err());
    assert_eq!(std::fs::read(cfg.receipt()).unwrap(), before);
}
