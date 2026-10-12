//! Reuse a locally cached desktop OAuth token. Never mint, refresh, or read cookies.
//! Format observed in Claude desktop 2.31226.1; unknown formats fail closed.
use crate::auth::{digest, invalid, read_json, token_ok, Credential};
use aillies_core::{Failure, FailureCode};
use base64::Engine;
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct DesktopReader {
    pub home: PathBuf,
    native: bool,
    password: Arc<Mutex<Option<Zeroizing<Vec<u8>>>>>,
}
impl std::fmt::Debug for DesktopReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DesktopReader(<redacted>)")
    }
}
impl DesktopReader {
    pub fn native(home: PathBuf) -> Self {
        Self { home, native: true, password: Arc::new(Mutex::new(None)) }
    }
    /// Explicit synthetic dependency injection; no environment variable loads a key.
    pub fn with_test_password(home: PathBuf, password: &[u8]) -> Self {
        Self { home, native: false, password: Arc::new(Mutex::new(Some(Zeroizing::new(password.to_vec())))) }
    }
    pub fn unavailable(home: PathBuf) -> Self {
        Self { home, native: false, password: Arc::new(Mutex::new(None)) }
    }
    pub fn read(&self) -> Result<Credential, Failure> {
        let unavailable = || Failure::new(FailureCode::ClaudeLoginUnavailable);
        let root = read_json(&self.home.join("config.json"), 8 * 1024 * 1024)?.ok_or_else(unavailable)?;
        let account = root["lastKnownAccountUuid"].as_str().filter(|a| valid_id(a)).ok_or_else(unavailable)?;
        // V2 is authoritative when present, including null/malformed/empty values.
        let cached = root.get("oauth:tokenCacheV2").or_else(|| root.get("oauth:tokenCache")).and_then(Value::as_str).ok_or_else(unavailable)?;
        if cached.len() > 4 * 1024 * 1024 {
            return Err(invalid());
        }
        let encrypted = base64::engine::general_purpose::STANDARD.decode(cached).map_err(|_| invalid())?;
        let password = self.password()?;
        let bytes = decrypt(&encrypted, &password)?;
        let cache: Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        select(&cache, account)
    }
    fn password(&self) -> Result<Zeroizing<Vec<u8>>, Failure> {
        let mut cached = self.password.lock().map_err(|_| invalid())?;
        if cached.is_none() && self.native {
            *cached = Some(native_password()?);
        }
        cached.clone().ok_or(Failure::new(FailureCode::ClaudeLoginUnavailable))
    }
}
fn valid_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok()
}
fn derive(password: &[u8]) -> Zeroizing<[u8; 16]> {
    let mut key = Zeroizing::new([0; 16]);
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password, b"saltysalt", 1003, &mut *key);
    key
}
fn decrypt(bytes: &[u8], password: &[u8]) -> Result<Zeroizing<Vec<u8>>, Failure> {
    let ciphertext = bytes.strip_prefix(b"v10").filter(|b| !b.is_empty() && b.len() % 16 == 0).ok_or_else(invalid)?;
    let key = derive(password);
    cbc::Decryptor::<aes::Aes128>::new((&*key).into(), (&[b' '; 16]).into())
        .decrypt_padded_vec_mut::<cbc::cipher::block_padding::Pkcs7>(ciphertext)
        .map(Zeroizing::new)
        .map_err(|_| invalid())
}
fn select(cache: &Value, account: &str) -> Result<Credential, Failure> {
    let entries = cache.as_object().ok_or_else(invalid)?;
    let mut candidates = Vec::new();
    for (key, value) in entries {
        let Some((who, rest)) = key.strip_prefix("acct:").and_then(|s| s.split_once('|')) else { continue };
        if !who.eq_ignore_ascii_case(account) {
            continue;
        }
        let Some((identity, scopes)) = rest.split_once(":https://api.anthropic.com:") else { continue };
        let Some((client, org)) = identity.split_once(':') else { return Err(invalid()) };
        if !valid_id(client) || !valid_id(org) {
            return Err(invalid());
        }
        if !scopes.split_whitespace().any(|s| s == "user:inference") || value.is_null() {
            continue;
        }
        let preferred = client == "89355bc3-cbfd-4382-905b-976645cad410" && scopes == "user:inference";
        candidates.push((preferred, org, value));
    }
    if candidates.iter().any(|c| c.0) {
        candidates.retain(|c| c.0);
    }
    // Never guess among organizations or different scoped credentials.
    let [(.., org, value)] = candidates.as_slice() else { return Err(invalid()) };
    let token = value["token"].as_str().filter(|s| token_ok(s)).ok_or_else(invalid)?;
    let expiry = value["expiresAt"].as_u64().ok_or_else(invalid)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    if u128::from(expiry) <= now + 30000 {
        return Err(Failure::new(FailureCode::ClaudeLoginExpired));
    }
    Ok(Credential { access_token: token.to_owned(), account: digest(&format!("desktop:{}:{}", account.to_ascii_lowercase(), org.to_ascii_lowercase())) })
}
#[cfg(target_os = "macos")]
fn native_password() -> Result<Zeroizing<Vec<u8>>, Failure> {
    use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
    let mut search = ItemSearchOptions::new();
    search.class(ItemClass::generic_password()).service("Claude Safe Storage").account("Claude").load_data(true).limit(2);
    let found = search.search().map_err(|_| Failure::new(FailureCode::ClaudeLoginUnavailable))?;
    match found.as_slice() {
        [SearchResult::Data(bytes)] if !bytes.is_empty() && bytes.len() <= 4096 => Ok(Zeroizing::new(bytes.clone())),
        _ => Err(invalid()),
    }
}
#[cfg(not(target_os = "macos"))]
fn native_password() -> Result<Zeroizing<Vec<u8>>, Failure> {
    Err(Failure::new(FailureCode::ClaudeLoginUnavailable))
}

#[cfg(test)]
#[path = "desktop_auth_tests.rs"]
mod tests;
