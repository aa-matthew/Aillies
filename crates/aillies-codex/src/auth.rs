//! Read the Codex ChatGPT login (`~/.codex/auth.json`) at turn time.

use aillies_core::{Failure, FailureCode};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_AUTH_BYTES: u64 = 65536;

#[derive(Clone)]
pub struct CodexOAuth {
    pub access_token: String,
    pub account_id: String,
}

impl std::fmt::Debug for CodexOAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexOAuth").field("account_id", &"<redacted>").finish_non_exhaustive()
    }
}

/// `$CODEX_HOME` or `~/.codex`.
pub fn codex_home() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CODEX_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(p));
    }
    directories::BaseDirs::new().map(|b| b.home_dir().join(".codex"))
}

pub fn default_auth_path() -> Option<PathBuf> {
    codex_home().map(|h| h.join("auth.json"))
}

/// Decode JWT claims without verifying (the upstream verifies). Used only to
/// read expiry and the account id, never to grant anything locally.
pub fn claims(token: &str) -> Map<String, Value> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Map::new();
    }
    let mut s = parts[1].replace('-', "+").replace('_', "/");
    while !s.len().is_multiple_of(4) {
        s.push('=');
    }
    STANDARD.decode(s).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

fn claim_account(c: &Map<String, Value>) -> Option<String> {
    c.get("https://api.openai.com/auth")?.get("chatgpt_account_id")?.as_str().map(str::to_owned)
}

fn now_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

impl CodexOAuth {
    pub fn load(data: &[u8]) -> Result<Self, Failure> {
        Self::load_at(data, now_secs())
    }

    pub fn load_at(data: &[u8], now: f64) -> Result<Self, Failure> {
        let invalid = Failure::new(FailureCode::InvalidAuthFile);
        if data.len() as u64 > MAX_AUTH_BYTES {
            return Err(invalid);
        }
        let root: Map<String, Value> = match serde_json::from_slice(data) {
            Ok(Value::Object(m)) => m,
            _ => return Err(invalid),
        };
        if !matches!(root.get("OPENAI_API_KEY"), None | Some(Value::Null)) {
            return Err(invalid);
        }
        let tokens = root.get("tokens").and_then(Value::as_object).ok_or(invalid.clone())?;
        let access = tokens.get("access_token").and_then(Value::as_str).ok_or(invalid.clone())?;
        if access.is_empty() || access.len() >= 16384 || access.chars().any(char::is_whitespace) {
            return Err(invalid);
        }
        let ac = claims(access);
        if let Some(exp) = ac.get("exp").and_then(Value::as_f64) {
            if exp <= now + 30.0 {
                return Err(Failure::new(FailureCode::OauthExpired));
            }
        }
        let idc = claims(tokens.get("id_token").and_then(Value::as_str).unwrap_or(""));
        let from_claims = claim_account(&ac).or_else(|| claim_account(&idc));
        let account = tokens.get("account_id").and_then(Value::as_str).map(str::to_owned).or_else(|| from_claims.clone()).ok_or(invalid.clone())?;
        let shape_ok = !account.is_empty() && account.len() <= 200 && account.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if !shape_ok || from_claims.as_ref().is_some_and(|c| c != &account) {
            return Err(invalid);
        }
        Ok(Self { access_token: access.to_owned(), account_id: account })
    }

    pub fn read(path: &Path) -> Result<Self, Failure> {
        let unavailable = Failure::new(FailureCode::AuthFileUnavailable);
        let meta = std::fs::metadata(path).map_err(|_| unavailable.clone())?;
        if !meta.is_file() {
            return Err(unavailable);
        }
        if meta.len() > MAX_AUTH_BYTES {
            return Err(Failure::new(FailureCode::InvalidAuthFile));
        }
        let data = std::fs::read(path).map_err(|_| unavailable)?;
        Self::load(&data)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde_json::json;

    pub fn jwt(claims: Value) -> String {
        format!("{}.{}.sig", URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\"}"), URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap()))
    }

    pub fn auth_file(exp: f64, account: &str) -> Vec<u8> {
        let token = jwt(json!({"exp": exp, "https://api.openai.com/auth": {"chatgpt_account_id": account}}));
        serde_json::to_vec(&json!({"OPENAI_API_KEY": null, "tokens": {"access_token": token, "account_id": account, "refresh_token": "r"}})).unwrap()
    }

    #[test]
    fn loads_valid_login() {
        let a = CodexOAuth::load_at(&auth_file(2e9, "acct_1"), 1.7e9).unwrap();
        assert_eq!(a.account_id, "acct_1");
        assert!(!format!("{a:?}").contains("acct_1"));
    }

    #[test]
    fn expired_and_mismatched_fail_closed() {
        assert_eq!(CodexOAuth::load_at(&auth_file(1.7e9 + 10.0, "a"), 1.7e9).unwrap_err().code, FailureCode::OauthExpired);
        let token = jwt(json!({"exp": 2e9, "https://api.openai.com/auth": {"chatgpt_account_id": "other"}}));
        let file = serde_json::to_vec(&json!({"tokens": {"access_token": token, "account_id": "mine"}})).unwrap();
        assert_eq!(CodexOAuth::load_at(&file, 1.7e9).unwrap_err().code, FailureCode::InvalidAuthFile);
    }

    #[test]
    fn api_key_logins_and_garbage_rejected() {
        let f = serde_json::to_vec(&json!({"OPENAI_API_KEY": "sk-x", "tokens": {"access_token": "a.b.c", "account_id": "x"}})).unwrap();
        assert!(CodexOAuth::load_at(&f, 0.0).is_err());
        assert!(CodexOAuth::load_at(b"[]", 0.0).is_err());
        assert!(CodexOAuth::load_at(br#"{"tokens":{"access_token":"has space","account_id":"x"}}"#, 0.0).is_err());
        assert!(CodexOAuth::load_at(br#"{"tokens":{"access_token":"t","account_id":"bad/id"}}"#, 0.0).is_err());
    }

    #[test]
    fn missing_file_is_unavailable() {
        assert_eq!(CodexOAuth::read(Path::new("/nonexistent/auth.json")).unwrap_err().code, FailureCode::AuthFileUnavailable);
    }
}
