//! Read the owner's Claude login; never export, refresh or overwrite it.
use aillies_core::{Failure, FailureCode};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_AUTH: usize = 65536;
/// Match Claude Code's native secure-storage service, including macOS NFC paths.
pub fn keychain_service_name(custom_storage: Option<&Path>) -> String {
    use unicode_normalization::UnicodeNormalization;
    let suffix = custom_storage.map(|path| {
        let normalized: String = path.to_string_lossy().nfc().collect();
        format!("-{}", &digest(&normalized)[..8])
    });
    format!("Claude Code-credentials{}", suffix.unwrap_or_default())
}
pub(crate) fn invalid() -> Failure {
    Failure::new(FailureCode::ClaudeLoginInvalid)
}
pub(crate) fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
pub(crate) fn token_ok(value: &str) -> bool {
    value.starts_with("sk-ant-oat") && value.len() <= 16384 && value.bytes().all(|b| b.is_ascii_graphic())
}
pub(crate) fn read_json(path: &Path, cap: usize) -> Result<Option<Value>, Failure> {
    use std::io::Read;
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > cap as u64 {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path).map_err(|_| invalid())?.take(cap as u64 + 1).read_to_end(&mut bytes).map_err(|_| invalid())?;
    if bytes.len() > cap {
        return Err(invalid());
    }
    serde_json::from_slice(&bytes).map(Some).map_err(|_| invalid())
}

#[derive(Clone)]
pub struct Reader {
    pub connected: Option<crate::oauth::AccountStore>,
    pub desktop: Option<crate::desktop_auth::DesktopReader>,
    pub home: PathBuf,
    pub storage: PathBuf,
    pub receipts: PathBuf,
    pub metadata: PathBuf,
    /// None for synthetic homes; tests must never open the user's Keychain.
    pub keychain_service: Option<String>,
}
pub struct Credential {
    pub access_token: String,
    /// Private, hashed account/source identity; never part of status or peer data.
    pub account: String,
}
/// Closed local control reasons. Never carry settings values or credential bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginIssue {
    Settings,
    Backend,
    Gateway,
    ApiKey,
    ConflictingTokens,
    TokenFormat,
    Native,
}
#[derive(Debug)]
pub struct LoginError {
    pub failure: Failure,
    pub issue: LoginIssue,
}
impl From<Failure> for LoginError {
    fn from(failure: Failure) -> Self {
        Self { failure, issue: LoginIssue::Native }
    }
}
impl LoginError {
    fn settings(issue: LoginIssue) -> Self {
        Self { failure: invalid(), issue }
    }
    pub fn message(&self, desktop: bool) -> &'static str {
        use LoginIssue::*;
        match self.issue {
            Settings => "Claude Code settings or Aillies' restore receipt cannot be read safely on this device. No settings were changed.",
            Backend => "Claude Code on this device uses a cloud backend instead of a Claude account. Desktop Chat has a separate account source.",
            Gateway => "Claude Code settings on this device point to another gateway. Aillies cannot share that gateway as a Claude account. Choose Desktop Chat to share the desktop app's existing account.",
            ApiKey => "Claude Code settings on this device use an API key. Account sharing requires a Claude account. Choose Desktop Chat to share the desktop app's existing account.",
            ConflictingTokens => "Claude Code settings on this device contain two authentication tokens. Choose one account source in its settings before sharing.",
            TokenFormat => "Claude Code settings on this device contain a token that Aillies cannot use as a Claude account. Choose Desktop Chat to share the desktop app's existing account.",
            Native => match (desktop, self.failure.code) {
                (true, FailureCode::ClaudeLoginExpired) => "The cached desktop login on this device has expired. Open normal Claude so it can renew its existing login, then retry sharing.",
                (true, FailureCode::ClaudeLoginUnavailable) => "Aillies cannot read an existing desktop inference login on this device. A desktop sign-in alone may not create this cache; macOS may also require Safe Storage access. Sharing stayed off.",
                (true, _) => "Aillies cannot safely use this device's desktop inference cache. Its format, account or organization could not be verified. Sharing stayed off; Claude Code was not used instead.",
                (false, FailureCode::ClaudeLoginExpired) => "The Claude Code login on this device has expired. Open Claude Code to renew its existing login, then retry sharing.",
                (false, FailureCode::ClaudeLoginUnavailable) => "Aillies cannot read a Claude Code login on this device. Signing in to the desktop Chat app does not sign in Claude Code. Choose Desktop Chat to use that existing account.",
                (false, _) => "Aillies cannot validate this device's saved Claude Code login, inference permission or account metadata. Sharing stayed off. Desktop Chat has a separate account source.",
            },
        }
    }
}
impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClaudeCredential(<redacted>)")
    }
}
impl Credential {
    pub fn token_digest(&self) -> String {
        digest(&self.access_token)
    }
}
impl Reader {
    pub fn read(&self) -> Result<Credential, Failure> {
        self.check_sharing().map_err(|error| error.failure)
    }
    pub fn check_sharing(&self) -> Result<Credential, LoginError> {
        if let Some(store) = &self.connected {
            return store.read().map_err(Into::into);
        }
        if let Some(reader) = &self.desktop {
            return reader.read().map_err(Into::into);
        }
        let env =
            crate::Config { home: self.home.clone(), receipts: self.receipts.clone() }.auth_env().map_err(|_| LoginError::settings(LoginIssue::Settings))?;
        for backend in ["CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX", "CLAUDE_CODE_USE_FOUNDRY"] {
            if env.get(backend).and_then(Value::as_str).is_some_and(|s| s == "1" || s == "true") {
                return Err(LoginError::settings(LoginIssue::Backend));
            }
        }
        if env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str).is_some_and(|s| !s.is_empty() && s.trim_end_matches('/') != "https://api.anthropic.com") {
            return Err(LoginError::settings(LoginIssue::Gateway));
        }
        if env.get("ANTHROPIC_API_KEY").and_then(Value::as_str).is_some_and(|s| !s.is_empty()) {
            return Err(LoginError::settings(LoginIssue::ApiKey));
        }
        let configured: Vec<_> = ["CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_AUTH_TOKEN"]
            .into_iter()
            .filter_map(|k| env.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()))
            .collect();
        if configured.len() > 1 {
            return Err(LoginError::settings(LoginIssue::ConflictingTokens));
        }
        if let Some(token) = configured.first() {
            if !token_ok(token) {
                return Err(LoginError::settings(LoginIssue::TokenFormat));
            }
            // Setup tokens have no trustworthy account metadata. Pin the token,
            // not an unrelated saved native account; rotation needs new consent.
            return Ok(Credential { access_token: (*token).to_owned(), account: digest(&format!("settings:{token}")) });
        }
        let (root, source) = match self.keychain()? {
            Some(v) => (v, "keychain"),
            None => (read_json(&self.storage.join(".credentials.json"), MAX_AUTH)?.ok_or(Failure::new(FailureCode::ClaudeLoginUnavailable))?, "credentials"),
        };
        self.native(root, source).map_err(Into::into)
    }
    fn native(&self, root: Value, source: &str) -> Result<Credential, Failure> {
        let oauth = root.get("claudeAiOauth").and_then(Value::as_object).ok_or_else(invalid)?;
        let token = oauth.get("accessToken").and_then(Value::as_str).filter(|s| token_ok(s)).ok_or_else(invalid)?;
        let expiry = oauth.get("expiresAt").and_then(Value::as_u64).ok_or_else(invalid)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
        if u128::from(expiry) <= now + 30000 {
            return Err(Failure::new(FailureCode::ClaudeLoginExpired));
        }
        if !oauth.get("scopes").and_then(Value::as_array).is_some_and(|s| s.iter().any(|s| s == "user:inference")) {
            return Err(invalid());
        }
        let metadata = read_json(&self.metadata, 8 * 1024 * 1024)?.unwrap_or(Value::Null);
        let saved = &metadata["oauthAccount"];
        let inline = oauth.get("accountUuid").and_then(Value::as_str);
        let account = saved["accountUuid"].as_str();
        if inline.zip(account).is_some_and(|(a, b)| a != b) {
            return Err(invalid());
        }
        let id = inline.or(account);
        if id.is_some_and(|s| s.is_empty() || s.len() > 200 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))) {
            return Err(invalid());
        }
        let org = saved["organizationUuid"].as_str().unwrap_or("");
        if org.len() > 200 {
            return Err(invalid());
        }
        let account = digest(&format!("{source}:{}:{org}", id.unwrap_or(token)));
        Ok(Credential { access_token: token.to_owned(), account })
    }
    #[cfg(target_os = "macos")]
    fn keychain(&self) -> Result<Option<Value>, Failure> {
        use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
        let Some(service) = &self.keychain_service else { return Ok(None) };
        let mut search = ItemSearchOptions::new();
        search.class(ItemClass::generic_password()).service(service).load_data(true).limit(2);
        if let Ok(user) = std::env::var("USER") {
            search.account(&user);
        }
        let found = match search.search() {
            Ok(items) => items,
            Err(e) if e.code() == -25300 => return Ok(None),
            Err(_) => return Err(Failure::new(FailureCode::ClaudeLoginUnavailable)),
        };
        match found.as_slice() {
            [SearchResult::Data(bytes)] if bytes.len() <= MAX_AUTH => serde_json::from_slice(bytes).map(Some).map_err(|_| invalid()),
            [] => Ok(None),
            _ => Err(invalid()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    fn keychain(&self) -> Result<Option<Value>, Failure> {
        Ok(None)
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
