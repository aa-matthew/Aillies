use super::*;
use serde_json::json;

#[test]
fn native_keychain_service_matches_default_and_normalized_custom_paths() {
    assert_eq!(keychain_service_name(None), "Claude Code-credentials");
    let composed = Path::new("/tmp/Claudé");
    let decomposed = Path::new("/tmp/Claude\u{301}");
    let expected = format!("Claude Code-credentials-{}", &digest("/tmp/Claudé")[..8]);
    assert_eq!(keychain_service_name(Some(composed)), expected);
    assert_eq!(keychain_service_name(Some(decomposed)), expected);
}

fn fixture() -> (tempfile::TempDir, Reader) {
    let dir = tempfile::tempdir().unwrap();
    let reader = Reader {
        connected: None,
        desktop: None,
        home: dir.path().join("claude"),
        storage: dir.path().join("claude"),
        receipts: dir.path().join("aillies"),
        metadata: dir.path().join("claude/.claude.json"),
        keychain_service: None,
    };
    std::fs::create_dir_all(&reader.home).unwrap();
    (dir, reader)
}
fn credentials(reader: &Reader, token: &str, account: &str, expiry: u64) {
    std::fs::write(
        reader.home.join(".credentials.json"),
        serde_json::to_vec(&json!({"claudeAiOauth":{"accessToken":token,"expiresAt":expiry,"scopes":["user:inference"],"accountUuid":account}})).unwrap(),
    )
    .unwrap();
}
#[test]
fn native_login_rotation_keeps_account_but_changes_attempt_binding() {
    let (_dir, reader) = fixture();
    credentials(&reader, "sk-ant-oat01-SECRET_FIRST", "account-a", u64::MAX);
    let first = reader.read().unwrap();
    credentials(&reader, "sk-ant-oat01-SECRET_ROTATED", "account-a", u64::MAX);
    let second = reader.read().unwrap();
    assert_eq!(first.account, second.account);
    assert_ne!(first.token_digest(), second.token_digest());
    assert!(!format!("{first:?}").contains("SECRET"));
    credentials(&reader, "sk-ant-oat01-SECRET_OTHER", "account-b", u64::MAX);
    assert_ne!(first.account, reader.read().unwrap().account);
}
#[test]
fn settings_oauth_survives_gateway_install_and_restore_without_key_copy() {
    let (_dir, reader) = fixture();
    let initial = json!({"env":{"ANTHROPIC_AUTH_TOKEN":"sk-ant-oat01-SECRET_CONFIGURED","ANTHROPIC_BASE_URL":"https://api.anthropic.com"},"theme":"dark"});
    std::fs::write(reader.home.join("settings.json"), serde_json::to_vec(&initial).unwrap()).unwrap();
    let before = reader.read().unwrap();
    let config = crate::Config { home: reader.home.clone(), receipts: reader.receipts.clone() };
    config.install(18522, "aillies-claude-synthetic").unwrap();
    assert_eq!(before.token_digest(), reader.read().unwrap().token_digest());
    let current = std::fs::read_to_string(reader.home.join("settings.json")).unwrap();
    assert!(!current.contains("SECRET_CONFIGURED"));
    config.restore().unwrap();
    assert_eq!(before.token_digest(), reader.read().unwrap().token_digest());
}
#[test]
fn invalid_expired_or_alternate_credentials_never_fall_back() {
    let (_dir, reader) = fixture();
    credentials(&reader, "sk-ant-oat01-SECRET_NATIVE", "account-a", u64::MAX);
    for env in [
        json!({"ANTHROPIC_AUTH_TOKEN":"sk-ant-api03-WRONG"}),
        json!({"ANTHROPIC_AUTH_TOKEN":"sk-ant-oat01-SECRET","ANTHROPIC_BASE_URL":"https://other.example"}),
        json!({"CLAUDE_CODE_USE_BEDROCK":"1"}),
    ] {
        std::fs::write(reader.home.join("settings.json"), serde_json::to_vec(&json!({"env":env})).unwrap()).unwrap();
        assert_eq!(reader.read().unwrap_err().code, FailureCode::ClaudeLoginInvalid);
    }
    std::fs::remove_file(reader.home.join("settings.json")).unwrap();
    credentials(&reader, "sk-ant-oat01-SECRET_EXPIRED", "account-a", 1);
    assert_eq!(reader.read().unwrap_err().code, FailureCode::ClaudeLoginExpired);
    std::fs::write(reader.home.join(".credentials.json"), b"not json").unwrap();
    assert_eq!(reader.read().unwrap_err().code, FailureCode::ClaudeLoginInvalid);
    std::fs::remove_file(reader.home.join(".credentials.json")).unwrap();
    assert_eq!(reader.read().unwrap_err().code, FailureCode::ClaudeLoginUnavailable);
}
#[test]
fn native_account_metadata_pins_identity_and_no_account_uses_token_digest() {
    let (_dir, reader) = fixture();
    std::fs::write(
        reader.home.join(".credentials.json"),
        br#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-SECRET","expiresAt":9999999999999,"scopes":["user:inference"]}}"#,
    )
    .unwrap();
    let no_account = reader.read().unwrap();
    std::fs::write(&reader.metadata, br#"{"oauthAccount":{"accountUuid":"account-a","organizationUuid":"org-a"}}"#).unwrap();
    let with_account = reader.read().unwrap();
    assert_ne!(no_account.account, with_account.account);
    std::fs::write(&reader.metadata, br#"{"oauthAccount":{"accountUuid":"account-b","organizationUuid":"org-a"}}"#).unwrap();
    assert_ne!(with_account.account, reader.read().unwrap().account);
}
#[cfg(unix)]
#[test]
fn credential_symlinks_are_refused() {
    let (_dir, reader) = fixture();
    std::os::unix::fs::symlink("/nonexistent", reader.home.join(".credentials.json")).unwrap();
    assert_eq!(reader.read().unwrap_err().code, FailureCode::ClaudeLoginInvalid);
}

#[test]
fn configured_oauth_token_is_masked_for_gateway_and_restored() {
    let (_dir, reader) = fixture();
    std::fs::write(reader.home.join("settings.json"), br#"{"env":{"CLAUDE_CODE_OAUTH_TOKEN":"sk-ant-oat01-SECRET_SETUP"}}"#).unwrap();
    let before = reader.read().unwrap();
    let config = crate::Config { home: reader.home.clone(), receipts: reader.receipts.clone() };
    config.install(18522, "aillies-claude-synthetic").unwrap();
    let installed: Value = serde_json::from_slice(&std::fs::read(reader.home.join("settings.json")).unwrap()).unwrap();
    assert_eq!(installed["env"]["CLAUDE_CODE_OAUTH_TOKEN"], "");
    assert_eq!(before.token_digest(), reader.read().unwrap().token_digest());
    config.restore().unwrap();
    assert_eq!(before.token_digest(), reader.read().unwrap().token_digest());
}

#[test]
fn alpha1_restore_receipt_remains_recoverable() {
    let (_dir, reader) = fixture();
    let config = crate::Config { home: reader.home.clone(), receipts: reader.receipts.clone() };
    config.install(18522, "aillies-claude-synthetic").unwrap();
    let path = reader.receipts.join("claude-restore.json");
    let mut receipt: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for field in ["before", "installed"] {
        receipt[field].as_object_mut().unwrap().remove("CLAUDE_CODE_OAUTH_TOKEN");
    }
    std::fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    config.restore().unwrap();
    let settings: Value = serde_json::from_slice(&std::fs::read(reader.home.join("settings.json")).unwrap()).unwrap();
    assert!(settings["env"].get("ANTHROPIC_BASE_URL").is_none());
}

#[test]
fn sharing_check_explains_settings_conflicts_without_exposing_values() {
    let (_dir, reader) = fixture();
    credentials(&reader, "sk-ant-oat01-SECRET_NATIVE", "account-a", u64::MAX);
    for (env, expected) in [
        (json!({"ANTHROPIC_BASE_URL":"https://SECRET_GATEWAY.example"}), LoginIssue::Gateway),
        (json!({"ANTHROPIC_API_KEY":"sk-ant-api03-SECRET_KEY"}), LoginIssue::ApiKey),
        (json!({"CLAUDE_CODE_USE_BEDROCK":"1"}), LoginIssue::Backend),
        (json!({"ANTHROPIC_AUTH_TOKEN":"sk-ant-oat01-SECRET_ONE","CLAUDE_CODE_OAUTH_TOKEN":"sk-ant-oat01-SECRET_TWO"}), LoginIssue::ConflictingTokens),
        (json!({"ANTHROPIC_AUTH_TOKEN":"SECRET_UNSUPPORTED_TOKEN"}), LoginIssue::TokenFormat),
    ] {
        std::fs::write(reader.home.join("settings.json"), serde_json::to_vec(&json!({"env":env})).unwrap()).unwrap();
        let error = reader.check_sharing().err().unwrap();
        assert_eq!(error.issue, expected);
        assert_eq!(reader.read().unwrap_err().code, FailureCode::ClaudeLoginInvalid);
        let message = error.message(false);
        assert!(!message.contains("SECRET") && !message.contains("sk-ant"));
        assert!(!format!("{error:?}").contains("SECRET"));
    }
    std::fs::write(reader.home.join("settings.json"), b"invalid").unwrap();
    assert_eq!(reader.check_sharing().err().unwrap().issue, LoginIssue::Settings);
}
