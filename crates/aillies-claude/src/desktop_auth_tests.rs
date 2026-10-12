use super::*;
use serde_json::json;

const ACCOUNT: &str = "11111111-1111-4111-8111-111111111111";
const ORG: &str = "22222222-2222-4222-8222-222222222222";
fn entry(token: &str) -> Value {
    json!({"token":token,"expiresAt":u64::MAX,"identityChecked":true})
}
fn key(scope: &str) -> String {
    format!("acct:{ACCOUNT}|89355bc3-cbfd-4382-905b-976645cad410:{ORG}:https://api.anthropic.com:{scope}")
}
#[test]
fn desktop_cache_reuses_account_and_allows_rotation_between_turns() {
    let mut cache = json!({});
    cache[key("user:inference")] = entry("sk-ant-oat01-DESKTOP_FIRST");
    let first = select(&cache, ACCOUNT).unwrap();
    cache[key("user:inference")] = entry("sk-ant-oat01-DESKTOP_ROTATED");
    let next = select(&cache, ACCOUNT).unwrap();
    assert_eq!(first.account, next.account);
    assert_ne!(first.token_digest(), next.token_digest());
    assert!(!format!("{first:?}").contains("DESKTOP_FIRST"));
}
#[test]
fn desktop_cache_refuses_expiry_foreign_accounts_origins_and_ambiguity() {
    let mut cache = json!({});
    let k = key("user:inference");
    cache[&k] = entry("sk-ant-oat01-DESKTOP");
    assert!(select(&cache, ORG).is_err());
    cache[&k]["expiresAt"] = json!(1);
    assert_eq!(select(&cache, ACCOUNT).unwrap_err().code, FailureCode::ClaudeLoginExpired);
    cache[&k] = entry("sk-ant-oat01-DESKTOP");
    cache[k.replace(ORG, "33333333-3333-4333-8333-333333333333")] = entry("sk-ant-oat01-OTHER");
    assert!(select(&cache, ACCOUNT).is_err());
    assert!(select(&json!({k.replace("api.anthropic.com", "evil.example"):entry("sk-ant-oat01-X")}), ACCOUNT).is_err());
    assert!(select(&json!({key("user:profile"):entry("sk-ant-oat01-X")}), ACCOUNT).is_err());
    assert!(select(&json!({k:entry("sk-ant-sid02-COOKIE")}), ACCOUNT).is_err());
}
#[test]
fn desktop_safe_storage_fixture_is_read_only_and_v2_is_authoritative() {
    let dir = tempfile::tempdir().unwrap();
    let reader = DesktopReader::with_test_password(dir.path().into(), b"fixture-password");
    let cache = json!({key("user:inference"):entry("sk-ant-oat01-DESKTOP")});
    write_fixture(&reader, ACCOUNT, &cache);
    let path = reader.home.join("config.json");
    let before = std::fs::read(&path).unwrap();
    assert!(reader.read().is_ok());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut config: Value = serde_json::from_slice(&before).unwrap();
    config["oauth:tokenCacheV2"] = json!("invalid");
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    assert!(reader.read().is_err(), "never fall back to a stale legacy token");
    config["oauth:tokenCacheV2"] = config["oauth:tokenCache"].take();
    config["lastKnownAccountUuid"] = json!(ORG);
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    assert!(reader.read().is_err());
}
#[test]
fn desktop_storage_rejects_wrong_key_plaintext_and_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let reader = DesktopReader::with_test_password(dir.path().into(), b"fixture-password");
    write_fixture(&reader, ACCOUNT, &json!({key("user:inference"):entry("sk-ant-oat01-DESKTOP")}));
    let wrong = DesktopReader::with_test_password(dir.path().into(), b"wrong-password");
    assert!(wrong.read().is_err());
    assert!(decrypt(b"{}", b"fixture-password").is_err());
    #[cfg(unix)]
    {
        std::fs::rename(dir.path().join("config.json"), dir.path().join("original.json")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("original.json"), dir.path().join("config.json")).unwrap();
        assert!(reader.read().is_err());
    }
}

pub(super) fn write_fixture(reader: &DesktopReader, account: &str, cache: &Value) {
    use cbc::cipher::{BlockEncryptMut, KeyIvInit};
    let password = reader.password().unwrap();
    let key = derive(&password);
    let bytes = serde_json::to_vec(cache).unwrap();
    let ciphertext =
        cbc::Encryptor::<aes::Aes128>::new((&*key).into(), (&[b' '; 16]).into()).encrypt_padded_vec_mut::<cbc::cipher::block_padding::Pkcs7>(&bytes);
    let mut payload = b"v10".to_vec();
    payload.extend(ciphertext);
    let config = json!({"lastKnownAccountUuid":account,"oauth:tokenCache":base64::engine::general_purpose::STANDARD.encode(payload)});
    std::fs::create_dir_all(&reader.home).unwrap();
    std::fs::write(reader.home.join("config.json"), serde_json::to_vec(&config).unwrap()).unwrap();
}
