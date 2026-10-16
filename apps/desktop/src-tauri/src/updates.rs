//! Signed whole-app updates. IPC never accepts a URL, package, key or command.
use futures_util::StreamExt;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Clone, serde::Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    Downloading,
    Ready,
    Waiting,
    Installing,
    Error,
    Unsupported,
}
#[derive(Clone, serde::Serialize, Default)]
pub struct View {
    pub phase: Phase,
    pub version: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub ready: bool,
    pub message: Option<&'static str>,
}
#[derive(Default)]
pub struct Updates {
    view: Mutex<View>,
    package: Mutex<Option<(Update, Arc<Vec<u8>>)>>,
    job: Arc<tokio::sync::Mutex<()>>,
    cancel: AtomicBool,
}
impl Updates {
    pub fn view(&self) -> View {
        self.view.lock().unwrap().clone()
    }
    fn fail(&self, message: &'static str) {
        self.view.lock().unwrap().phase = Phase::Error;
        self.view.lock().unwrap().message = Some(message);
        super::log_line(super::LifecycleEvent::UpdateFailed);
    }
    pub fn check(self: &Arc<Self>, app: AppHandle) {
        let Ok(job) = self.job.clone().try_lock_owned() else { return };
        if self.package.lock().unwrap().is_some() {
            return;
        }
        if cfg!(debug_assertions) || !supports_install(&app) {
            *self.view.lock().unwrap() = View { phase: Phase::Unsupported, message: Some("Use the latest installer for this build."), ..Default::default() };
            return;
        }
        *self.view.lock().unwrap() = View { phase: Phase::Checking, ..Default::default() };
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            let _job = job;
            if let Err(message) = state.fetch(&app).await {
                state.fail(message);
            }
        });
    }
    async fn fetch(&self, app: &AppHandle) -> Result<(), &'static str> {
        let updater = app
            .updater_builder()
            .timeout(Duration::from_secs(30))
            .no_proxy()
            .version_comparator(|current, release| aillies_core::update::stable_upgrade(&current.to_string(), &release.version.to_string()))
            .build()
            .map_err(|_| "Couldn’t check for updates. Try again or download the installer.")?;
        let update = updater.check().await.map_err(|_| "Couldn’t check for updates. Try again or download the installer.")?;
        let Some(mut update) = update else {
            *self.view.lock().unwrap() = View { message: Some("You’re up to date."), ..Default::default() };
            return Ok(());
        };
        // A compromised feed cannot redirect even a signed download to an arbitrary origin.
        if !aillies_core::update::trusted_package(
            env!("CARGO_PKG_VERSION"),
            &update.version,
            update.download_url.as_str(),
            std::env::consts::OS,
            std::env::consts::ARCH,
        ) {
            return Err("The update information is invalid. Download the installer.");
        }
        update.timeout = Some(Duration::from_secs(900));
        *self.view.lock().unwrap() = View { phase: Phase::Downloading, version: Some(update.version.clone()), ..Default::default() };
        let client = reqwest::Client::builder()
            .https_only(true)
            .no_proxy()
            .timeout(Duration::from_secs(900))
            .build()
            .map_err(|_| "Couldn’t download the update. Try again.")?;
        let response = client
            .get(update.download_url.clone())
            .send()
            .await
            .map_err(|_| "Couldn’t download the update. Try again.")?
            .error_for_status()
            .map_err(|_| "Couldn’t download the update. Try again.")?;
        let total = response.content_length();
        if total.is_some_and(|n| n > 256 * 1024 * 1024) {
            return Err("The update package is too large. Download the installer.");
        }
        self.view.lock().unwrap().total = total;
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "Couldn’t download the update. Try again.")?;
            if bytes.len().saturating_add(chunk.len()) > 256 * 1024 * 1024 {
                return Err("The update package is too large. Download the installer.");
            }
            bytes.extend_from_slice(&chunk);
            self.view.lock().unwrap().downloaded = bytes.len() as u64;
        }
        // Official verifier authenticates bytes and the announced version. Install rechecks both.
        update.verify(&bytes).map_err(|_| "Couldn’t verify this update. Download the installer.")?;
        *self.package.lock().unwrap() = Some((update, Arc::new(bytes)));
        self.view.lock().unwrap().phase = Phase::Ready;
        self.view.lock().unwrap().ready = true;
        super::log_line(super::LifecycleEvent::UpdateReady);
        Ok(())
    }
    pub fn apply(self: &Arc<Self>, app: AppHandle, node: Arc<aillies_node::Node>) -> Result<(), String> {
        let job = self.job.clone().try_lock_owned().map_err(|_| "Wait for the update operation to finish.")?;
        if self.package.lock().unwrap().is_none() {
            return Err("Check for updates first.".into());
        }
        let pause = node.pause_for_update().map_err(|e| e.to_string())?;
        self.cancel.store(false, Ordering::Release);
        self.view.lock().unwrap().phase = Phase::Waiting;
        self.view.lock().unwrap().message = None;
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            let _job = job;
            let started = std::time::Instant::now();
            while !pause.idle() {
                if state.cancel.load(Ordering::Acquire) || started.elapsed() > Duration::from_secs(900) {
                    state.view.lock().unwrap().phase = Phase::Ready;
                    state.view.lock().unwrap().message = Some("Update postponed. Your current version is still running.");
                    return; // Drop pause; ordinary work resumes.
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            if state.cancel.load(Ordering::Acquire) {
                state.view.lock().unwrap().phase = Phase::Ready;
                return;
            }
            state.view.lock().unwrap().phase = Phase::Installing;
            let work = state.clone();
            let result = tauri::async_runtime::spawn_blocking(move || {
                let package = work.package.lock().unwrap();
                let Some((update, bytes)) = package.as_ref() else { return Err(()) };
                pause.install(|| update.install(bytes.as_slice()).map_err(|_| anyhow_error())).map_err(|_| ())?;
                Ok(pause) // Keep admissions closed until restart.
            })
            .await;
            match result {
                Ok(Ok(_pause)) => app.restart(),
                _ => state.fail("Couldn’t install the update. Retry or download the installer."),
            }
        });
        Ok(())
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}
fn anyhow_error() -> anyhow::Error {
    anyhow::anyhow!("The signed update could not be installed.")
}

fn supports_install(_app: &AppHandle) -> bool {
    #[cfg(target_os = "linux")]
    {
        _app.env().appimage.is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plugin_accepts_signed_package_but_refuses_tampering_and_version_replay() {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        let key = include_str!("../tests/fixtures/updater/public.key").trim();
        context
            .config_mut()
            .plugins
            .0
            .insert("updater".into(), serde_json::json!({"pubkey":key,"endpoints":["https://example.invalid/latest.json"],"requireSignedVersion":true}));
        let app = tauri::test::mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(context).unwrap();
        let updater = app.updater_builder().target("fixture").build().unwrap();
        let manifest = serde_json::json!({"version":"99.0.0","platforms":{"fixture":{"url":"https://example.invalid/package","signature":include_str!("../tests/fixtures/updater/payload.sig").trim()}}});
        let update = updater.restore_update(manifest.clone()).unwrap().unwrap();
        let bytes = include_bytes!("../tests/fixtures/updater/payload");
        update.verify(bytes).unwrap();
        assert!(update.verify(b"tampered payload").is_err());
        let mut replay = manifest;
        replay["version"] = serde_json::Value::from("100.0.0");
        assert!(updater.restore_update(replay).unwrap().unwrap().verify(bytes).is_err());
    }
}

#[cfg(test)]
mod release_verification {
    use super::*;
    #[test]
    #[ignore = "requires an explicitly selected built updater package; verification only, never installation"]
    fn verify_built_updater_package() {
        let path = std::path::PathBuf::from(std::env::var_os("AILLIES_VERIFY_UPDATE_PACKAGE").expect("explicit updater package"));
        let cfg: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert("updater".into(), cfg["plugins"]["updater"].clone());
        let app = tauri::test::mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(context).unwrap();
        let updater = app.updater_builder().target("fixture").version_comparator(|_, _| true).build().unwrap();
        let signature = std::fs::read_to_string(format!("{}.sig", path.display())).unwrap();
        let manifest = serde_json::json!({"version":cfg["version"],"platforms":{"fixture":{"url":"https://example.invalid/verification-only","signature":signature.trim()}}});
        let update = updater.restore_update(manifest).unwrap().unwrap();
        update.verify(&std::fs::read(path).unwrap()).unwrap();
    }
}
