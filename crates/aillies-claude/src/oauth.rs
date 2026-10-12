//! Explicit account connection. No native login/cache is changed or used as fallback.
use crate::auth::{digest, invalid, token_ok, Credential};
use aillies_core::{Failure, FailureCode};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const ORIGIN: &str = "https://api.anthropic.com";
// Public native PKCE client, as used by the inspected OmniRoute implementation.
const CLIENT: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const REDIRECT: &str = "https://platform.claude.com/oauth/code/callback";
const SCOPES: &str = "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers";
const CAP: usize = 65536;
const CONNECT_ERROR: &str = "Claude could not connect this account. Start again and authorize it in your browser.";
const STORAGE_ERROR: &str = "Cannot access the saved Claude account. Check Aillies’ private data folder or connect again.";

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
fn secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok()
}

#[derive(Clone, Serialize, Deserialize)]
struct Connection {
    access_token: String,
    refresh_token: String,
    expires_at: u64,
    account: String,
    organization: String,
    refresh_pending: bool,
}
impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClaudeConnection(<redacted>)")
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}
impl Connection {
    fn identity(&self) -> String {
        digest(&format!("connected:{}:{}", self.account.to_ascii_lowercase(), self.organization.to_ascii_lowercase()))
    }
    fn validate(&self) -> Result<(), Failure> {
        if !token_ok(&self.access_token)
            || !uuid(&self.account)
            || !uuid(&self.organization)
            || self.refresh_token.is_empty()
            || self.refresh_token.len() > 16384
            || !self.refresh_token.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(invalid());
        }
        Ok(())
    }
    fn credential(&self) -> Result<Credential, Failure> {
        self.validate()?;
        if self.refresh_pending || self.expires_at <= now() + 30 {
            return Err(Failure::new(FailureCode::ClaudeLoginExpired));
        }
        Ok(Credential { access_token: self.access_token.clone(), account: self.identity() })
    }
}
#[derive(Clone)]
pub struct AccountStore {
    backend: Backend,
}
#[derive(Clone)]
enum Backend {
    Private(PathBuf),
    Fixture(PathBuf),
}
impl std::fmt::Debug for AccountStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClaudeAccountStore(<redacted>)")
    }
}
impl AccountStore {
    pub fn private(home: &std::path::Path) -> Self {
        Self { backend: Backend::Private(home.join("accounts/claude.json")) }
    }
    /// Metadata only: never probes Keychain or reads tokens during status polling.
    pub fn present(&self) -> bool {
        let path = match &self.backend {
            Backend::Private(path) | Backend::Fixture(path) => path,
        };
        std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    }
    fn private_folder(path: &std::path::Path, create: bool) -> Result<(), Failure> {
        if !path.is_absolute() {
            return Err(invalid());
        }
        let folder = path.parent().ok_or_else(invalid)?;
        let home = folder.parent().ok_or_else(invalid)?;
        for dir in [home, folder] {
            match std::fs::symlink_metadata(dir) {
                Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
                Err(e) if create && e.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir(dir).map_err(|_| invalid())?;
                }
                _ => return Err(invalid()),
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if create {
                std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700)).map_err(|_| invalid())?;
            }
            if std::fs::metadata(folder).map_err(|_| invalid())?.permissions().mode() & 0o077 != 0 {
                return Err(invalid());
            }
            if let Ok(meta) = std::fs::symlink_metadata(path) {
                if meta.permissions().mode() & 0o077 != 0 {
                    return Err(invalid());
                }
            }
            Ok(())
        }
        // Local account connection is Mac-only. Do not create a broadly readable
        // credential file on an unsupported platform without a private ACL backend.
        #[cfg(not(unix))]
        {
            Err(invalid())
        }
    }
    /// Synthetic fixture storage only. Never selected by production environment values.
    pub fn fixture(path: PathBuf) -> Self {
        Self { backend: Backend::Fixture(path) }
    }
    fn load(&self) -> Result<Connection, Failure> {
        let bytes = match &self.backend {
            Backend::Fixture(path) => crate::config::read(path).map_err(|_| invalid())?.ok_or(Failure::new(FailureCode::ClaudeLoginUnavailable))?,
            Backend::Private(path) => {
                if !self.present() {
                    return Err(Failure::new(FailureCode::ClaudeLoginUnavailable));
                }
                Self::private_folder(path, false)?;
                crate::config::read(path).map_err(|_| invalid())?.ok_or_else(invalid)?
            }
        };
        let bytes = Zeroizing::new(bytes);
        if bytes.len() > CAP {
            return Err(invalid());
        }
        let c: Connection = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        c.validate()?;
        Ok(c)
    }
    fn save(&self, c: &Connection) -> Result<(), Failure> {
        let bytes = Zeroizing::new(serde_json::to_vec(c).map_err(|_| invalid())?);
        match &self.backend {
            Backend::Fixture(path) => {
                crate::config::read(path).map_err(|_| invalid())?;
                crate::config::private_write(path, &bytes).map_err(|_| invalid())
            }
            Backend::Private(path) => {
                Self::private_folder(path, true)?;
                crate::config::read(path).map_err(|_| invalid())?;
                crate::config::private_write(path, &bytes).map_err(|_| invalid())
            }
        }
    }
    async fn load_async(&self) -> Result<Connection, Failure> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.load()).await.map_err(|_| invalid())?
    }
    async fn save_async(&self, c: &Connection) -> Result<(), Failure> {
        let store = self.clone();
        let c = c.clone();
        tokio::task::spawn_blocking(move || store.save(&c)).await.map_err(|_| invalid())?
    }
    pub fn read(&self) -> Result<Credential, Failure> {
        self.load()?.credential()
    }
    fn delete(&self) -> Result<(), Failure> {
        match &self.backend {
            Backend::Fixture(path) => {
                if crate::config::read(path).map_err(|_| invalid())?.is_some() {
                    std::fs::remove_file(path).map_err(|_| invalid())?;
                }
                Ok(())
            }
            Backend::Private(path) => {
                if self.present() {
                    Self::private_folder(path, false)?;
                    crate::config::read(path).map_err(|_| invalid())?;
                    std::fs::remove_file(path).map_err(|_| invalid())?;
                }
                Ok(())
            }
        }
    }
}

struct Pending {
    verifier: Zeroizing<String>,
    state: String,
    at: Instant,
}
impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClaudeAuthorization(<redacted>)")
    }
}
impl Pending {
    fn new() -> Self {
        Self { verifier: Zeroizing::new(secret()), state: secret(), at: Instant::now() }
    }
    fn url(&self) -> String {
        let mut url = reqwest::Url::parse("https://claude.ai/oauth/authorize").expect("fixed URL");
        url.query_pairs_mut().extend_pairs([
            ("code", "true"),
            ("client_id", CLIENT),
            ("response_type", "code"),
            ("redirect_uri", REDIRECT),
            ("scope", SCOPES),
            ("code_challenge", &URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes()))),
            ("code_challenge_method", "S256"),
            ("state", &self.state),
            ("prompt", "login"),
        ]);
        url.into()
    }
    fn code<'a>(&self, raw: &'a str) -> anyhow::Result<&'a str> {
        let (code, state) = raw.trim().split_once('#').ok_or_else(|| anyhow::anyhow!("Paste the complete code shown by Claude, including its # suffix."))?;
        anyhow::ensure!(
            self.at.elapsed() < Duration::from_secs(600)
                && state == self.state
                && !code.is_empty()
                && code.len() <= 4096
                && code.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
            "This authorization code is invalid or expired. Connect again."
        );
        Ok(code)
    }
}
#[derive(Clone)]
pub struct OAuth {
    pub store: AccountStore,
    pending: Arc<Mutex<Option<Pending>>>,
    gate: Arc<tokio::sync::Mutex<()>>,
    origin: String,
    client: reqwest::Client,
    generation: Arc<AtomicU64>,
}
fn origin(candidate: Option<&str>) -> String {
    candidate
        .and_then(|s| {
            let u = reqwest::Url::parse(s).ok()?;
            let local =
                u.host_str().is_some_and(|h| h == "localhost" || h.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()));
            (local
                && u.scheme() == "http"
                && u.port().is_some()
                && u.username().is_empty()
                && u.password().is_none()
                && u.query().is_none()
                && u.fragment().is_none()
                && u.path() == "/")
                .then(|| u.as_str().trim_end_matches('/').to_owned())
        })
        .unwrap_or_else(|| ORIGIN.into())
}
impl OAuth {
    pub fn new(store: AccountStore, test_origin: Option<&str>) -> Self {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(25))
            .build()
            .expect("OAuth HTTP client");
        Self {
            store,
            pending: Arc::new(Mutex::new(None)),
            gate: Arc::new(tokio::sync::Mutex::new(())),
            origin: origin(test_origin),
            client,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }
    pub fn begin(&self) -> String {
        let mut pending = self.pending.lock().unwrap();
        self.generation.fetch_add(1, Ordering::SeqCst);
        let p = Pending::new();
        let url = p.url();
        *pending = Some(p);
        url
    }
    pub fn cancel(&self) {
        let mut pending = self.pending.lock().unwrap();
        self.generation.fetch_add(1, Ordering::SeqCst);
        *pending = None;
    }
    pub fn authorization_url(&self) -> Option<String> {
        self.pending.lock().unwrap().as_ref().filter(|p| p.at.elapsed() < Duration::from_secs(600)).map(Pending::url)
    }
    async fn response(&self, call: reqwest::RequestBuilder) -> Result<Value, Failure> {
        let response = call.send().await.map_err(|_| Failure::new(FailureCode::ClaudeUnavailable))?;
        if !response.status().is_success() {
            return Err(Failure::new(FailureCode::ClaudeLoginInvalid));
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Zeroizing::new(Vec::new());
        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| invalid())?;
            if bytes.len().saturating_add(chunk.len()) > CAP {
                return Err(invalid());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| invalid())
    }
    pub async fn finish(&self, code: &str) -> anyhow::Result<String> {
        let _guard = self.gate.lock().await;
        let (generation, pending) = {
            let mut slot = self.pending.lock().unwrap();
            (
                self.generation.load(Ordering::SeqCst),
                slot.take().ok_or_else(|| anyhow::anyhow!("Connect Claude first. Authorization attempts can only be used once."))?,
            )
        };
        let code = pending.code(code)?;
        let body = Zeroizing::new(serde_json::to_vec(
            &json!({"grant_type":"authorization_code", "code":code,"state":pending.state,"client_id":CLIENT,"redirect_uri":REDIRECT,"code_verifier":pending.verifier.as_str()}),
        )?);
        let mut value = self
            .response(self.client.post(format!("{}/v1/oauth/token", self.origin)).header("content-type", "application/json").body(body.to_vec()))
            .await
            .map_err(|_| anyhow::anyhow!(CONNECT_ERROR))?;
        // Missing identity must be verified with the authenticated bootstrap endpoint.
        if identity(&value).is_none() {
            let token = value["access_token"].as_str().filter(|t| token_ok(t)).ok_or_else(|| anyhow::anyhow!(CONNECT_ERROR))?;
            let bootstrap = self
                .response(self.client.get(format!("{}/api/claude_cli/bootstrap", self.origin)).bearer_auth(token).header("anthropic-beta", "oauth-2025-04-20"))
                .await
                .map_err(|_| anyhow::anyhow!(CONNECT_ERROR))?;
            let account = &bootstrap["oauth_account"];
            for (field, bootstrap_field) in [("account", "account_uuid"), ("organization", "organization_uuid")] {
                if let Some(existing) = value[field]["uuid"].as_str() {
                    anyhow::ensure!(
                        account[bootstrap_field].as_str().is_some_and(|id| existing.eq_ignore_ascii_case(id)),
                        "Claude account identity could not be verified. Connect again."
                    );
                }
            }
            value["account"] = json!({"uuid":account["account_uuid"]});
            value["organization"] = json!({"uuid":account["organization_uuid"]});
        }
        let c = parse_tokens(&value, None).map_err(|_| anyhow::anyhow!(CONNECT_ERROR))?;
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let _commit = this.pending.lock().unwrap();
            anyhow::ensure!(this.generation.load(Ordering::SeqCst) == generation, "Claude connection was cancelled. Start again when ready.");
            this.store.save(&c).map_err(|_| anyhow::anyhow!(STORAGE_ERROR))?;
            Ok(c.identity())
        })
        .await
        .map_err(|_| anyhow::anyhow!(STORAGE_ERROR))?
    }
    /// Refresh before pinning a new turn. Never call this inside an attempt retry.
    pub async fn fresh(&self, expected: &str) -> Result<(), Failure> {
        let _guard = self.gate.lock().await;
        let mut old = self.store.load_async().await?;
        if old.identity() != expected {
            return Err(Failure::new(FailureCode::AccountChanged));
        }
        if old.refresh_pending {
            return Err(Failure::new(FailureCode::ClaudeLoginExpired));
        }
        if old.expires_at > now() + 300 {
            return Ok(());
        }
        // A rotated refresh token can be consumed even if the response is lost.
        // Persist intent first; uncertain/failed refresh requires reconnect, never replay.
        old.refresh_pending = true;
        self.store.save_async(&old).await?;
        let body = Zeroizing::new(
            url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs([("grant_type", "refresh_token"), ("refresh_token", old.refresh_token.as_str()), ("client_id", CLIENT)])
                .finish(),
        );
        let value = self
            .response(
                self.client
                    .post(format!("{}/v1/oauth/token", self.origin))
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("anthropic-beta", "oauth-2025-04-20")
                    .body(body.as_bytes().to_vec()),
            )
            .await
            .map_err(|_| Failure::new(FailureCode::ClaudeLoginExpired))?;
        let c = parse_tokens(&value, Some(&old))?;
        self.store.save_async(&c).await?;
        Ok(())
    }
    pub async fn disconnect(&self) -> anyhow::Result<()> {
        let _guard = self.gate.lock().await;
        self.cancel();
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || store.delete()).await.map_err(|_| anyhow::anyhow!(STORAGE_ERROR))?.map_err(|_| anyhow::anyhow!(STORAGE_ERROR))
    }
}
fn identity(value: &Value) -> Option<(&str, &str)> {
    Some((value["account"]["uuid"].as_str().filter(|v| uuid(v))?, value["organization"]["uuid"].as_str().filter(|v| uuid(v))?))
}
fn parse_tokens(value: &Value, old: Option<&Connection>) -> Result<Connection, Failure> {
    if value["token_type"].as_str().is_some_and(|v| !v.eq_ignore_ascii_case("bearer")) {
        return Err(invalid());
    }
    if let Some(scope) = value.get("scope").filter(|s| !s.is_null()) {
        if !scope.as_str().is_some_and(|s| s.split_whitespace().any(|v| v == "user:inference")) {
            return Err(invalid());
        }
    } else if old.is_none() {
        return Err(invalid());
    }
    let (account, organization) = match identity(value) {
        Some(ids) => ids,
        None if old.is_some() && value.get("account").is_none() && value.get("organization").is_none() => {
            let c = old.ok_or_else(invalid)?;
            (&*c.account, &*c.organization)
        }
        _ => return Err(invalid()),
    };
    if old.is_some_and(|c| !c.account.eq_ignore_ascii_case(account) || !c.organization.eq_ignore_ascii_case(organization)) {
        return Err(Failure::new(FailureCode::AccountChanged));
    }
    let duration = value["expires_in"].as_u64().filter(|s| (31..=31_536_000).contains(s)).ok_or_else(invalid)?;
    let c = Connection {
        access_token: value["access_token"].as_str().ok_or_else(invalid)?.into(),
        refresh_token: value["refresh_token"].as_str().or_else(|| old.map(|c| c.refresh_token.as_str())).ok_or_else(invalid)?.into(),
        expires_at: now().checked_add(duration).ok_or_else(invalid)?,
        account: account.into(),
        organization: organization.into(),
        refresh_pending: false,
    };
    c.validate()?;
    Ok(c)
}
#[cfg(test)]
#[path = "oauth_tests.rs"]
mod tests;
