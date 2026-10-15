//! Aillies desktop shell: tray/menu-bar item plus one compact window.
//! All routing and security behavior lives in `aillies-node`.

use aillies_node::node::{NodeStatus, PairStarted, PeerHealth, PeerView};
use aillies_node::{Node, NodeConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

mod window_recovery;
#[cfg(target_os = "windows")]
mod windows_com;
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent, Wry};
use window_recovery::{OpenGate, Rect};

/// Typed lifecycle events cannot carry prompts, upstream errors or panic payloads.
#[derive(Clone, Copy)]
enum LifecycleEvent {
    Starting,
    NodeStarted(u16),
    WindowCreated,
    WindowFailed,
    WindowOpening,
    WindowStepFailed(WindowStep),
    WindowDeferred,
    WindowCreateCode(&'static str),
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    UiStaReady,
    PanicSite(&'static str, u32),
    NodeFailed,
    TrayFailed,
    AppFailed,
    Panic,
}

fn write_lifecycle_log(writer: &mut impl std::io::Write, event: LifecycleEvent) -> std::io::Result<()> {
    let message = match event {
        LifecycleEvent::Starting => format!("starting Aillies {}", env!("CARGO_PKG_VERSION")),
        LifecycleEvent::NodeStarted(port) => format!("node started on 127.0.0.1:{port}"),
        LifecycleEvent::WindowCreated => "window created".into(),
        LifecycleEvent::WindowFailed => "window failed".into(),
        LifecycleEvent::UiStaReady => "UI COM initialized as STA".into(),
        LifecycleEvent::WindowCreateCode(code) => format!("window creation class={code}"),
        LifecycleEvent::PanicSite(component, line) => format!("panic site={component} line={line}"),
        LifecycleEvent::WindowOpening => "window opening on worker".into(),
        LifecycleEvent::WindowDeferred => "window opening deferred until startup completes".into(),
        LifecycleEvent::WindowStepFailed(step) => format!("window failed: {}", step.code()),
        LifecycleEvent::NodeFailed => "node failed".into(),
        LifecycleEvent::TrayFailed => "tray failed".into(),
        LifecycleEvent::AppFailed => "app failed to start".into(),
        LifecycleEvent::Panic => "panic; restart Aillies".into(),
    };
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    writeln!(writer, "{t} {message}")
}

fn log_line(event: LifecycleEvent) {
    let dir = aillies_node::node::default_home();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("desktop.log")) {
        let _ = write_lifecycle_log(&mut f, event);
    }
}

struct AppState {
    node: Option<Arc<Node>>,
    error: Option<String>,
    navigation: Mutex<Option<WindowIntent>>,
}

#[derive(Clone, serde::Serialize, Debug, PartialEq)]
#[serde(tag = "screen", rename_all = "snake_case")]
enum WindowIntent {
    Home,
    Codex { on: Option<bool> },
    Pair,
    Claude,
    ActivateClaude { on: bool },
    Devices,
    Details,
    Warnings,
    Quit,
    Error { message: String },
}

fn navigation_for(id: &str) -> Option<WindowIntent> {
    match id {
        "open" => Some(WindowIntent::Home),
        "provider:normal" => Some(WindowIntent::Codex { on: Some(false) }),
        "provider:aillies" => Some(WindowIntent::Codex { on: Some(true) }),
        "pair" => Some(WindowIntent::Pair),
        "claude" => Some(WindowIntent::Claude),
        "claude:on" => Some(WindowIntent::ActivateClaude { on: true }),
        "claude:off" => Some(WindowIntent::ActivateClaude { on: false }),
        "devices" => Some(WindowIntent::Devices),
        "details" => Some(WindowIntent::Details),
        "warnings" => Some(WindowIntent::Warnings),
        "quit" => Some(WindowIntent::Quit),
        _ => None,
    }
}

#[tauri::command]
fn take_navigation(s: State<'_, AppState>) -> Option<WindowIntent> {
    s.navigation.lock().unwrap().take()
}

#[tauri::command]
async fn prepare_activation(
    s: State<'_, AppState>,
    service: aillies_node::node::Activation,
    on: bool,
    mode: Option<aillies_node::node::ActivationMode>,
) -> R<serde_json::Value> {
    let node = node(&s)?;
    tauri::async_runtime::spawn_blocking(move || {
        node.prepare_activation_with_mode(service, on, mode.unwrap_or_default()).map_err(e).and_then(|r| serde_json::to_value(r).map_err(e))
    })
    .await
    .map_err(|_| "Could not review the apps. Try again.".to_string())?
}
#[tauri::command]
async fn apply_activation(s: State<'_, AppState>, token: String) -> R<serde_json::Value> {
    let node = node(&s)?;
    tauri::async_runtime::spawn_blocking(move || node.apply_activation(&token).map_err(e).and_then(|r| serde_json::to_value(r).map_err(e)))
        .await
        .map_err(|_| "The change stopped. Recovery files were kept.".to_string())?
}
#[tauri::command]
async fn repair_settings_access(s: State<'_, AppState>, id: String) -> R<()> {
    let node = node(&s)?;
    tauri::async_runtime::spawn_blocking(move || node.repair_settings_access(&id).map_err(e))
        .await
        .map_err(|_| "Access repair stopped. Review the access details.".to_string())?
}
#[tauri::command]
fn cancel_activation(s: State<'_, AppState>) -> R<()> {
    node(&s)?.cancel_activation();
    Ok(())
}
#[tauri::command]
async fn schedule_activation(s: State<'_, AppState>, token: String, mode: aillies_node::node::ActivationMode) -> R<()> {
    node(&s)?.schedule_activation(&token, mode).map_err(e)
}
#[tauri::command]
fn cancel_scheduled_activation(s: State<'_, AppState>) -> R<()> {
    node(&s)?.cancel_scheduled_activation().map_err(e)
}
#[tauri::command]
fn dismiss_activation_notice(s: State<'_, AppState>) -> R<()> {
    node(&s)?.dismiss_activation_notice();
    Ok(())
}
#[tauri::command]
fn cancel_invitation(s: State<'_, AppState>) -> R<()> {
    node(&s)?.cancel_invitation();
    Ok(())
}
#[tauri::command]
fn open_automation_settings() -> R<()> {
    #[cfg(target_os = "macos")]
    {
        let result = std::process::Command::new("/usr/bin/open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation").status();
        if result.is_ok_and(|s| s.success()) {
            Ok(())
        } else {
            Err("Open System Settings, Privacy & Security, Automation.".into())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Automation settings are available on Mac.".into())
    }
}

#[tauri::command]
fn quit_anyway(app: AppHandle, s: State<'_, AppState>) {
    if let Some(node) = &s.node {
        node.shutdown_anyway();
    }
    app.exit(0);
}
#[tauri::command]
fn diagnostics(s: State<'_, AppState>) -> R<serde_json::Value> {
    Ok(node(&s)?.diagnostics())
}

#[tauri::command]
fn quit_if_normal(app: AppHandle, s: State<'_, AppState>) -> R<()> {
    node(&s)?.shutdown_if_normal().map_err(e)?;
    app.exit(0);
    Ok(())
}

type R<T> = Result<T, String>;

fn node(s: &State<'_, AppState>) -> R<Arc<Node>> {
    s.node.clone().ok_or_else(|| s.error.clone().unwrap_or_else(|| "Aillies is not running.".into()))
}

fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}

#[tauri::command]
fn startup_error(s: State<'_, AppState>) -> Option<String> {
    s.error.clone()
}

#[tauri::command]
fn status(s: State<'_, AppState>) -> R<NodeStatus> {
    Ok(node(&s)?.status())
}

#[tauri::command]
fn export_diagnostics(s: State<'_, AppState>) -> R<String> {
    Ok(node(&s)?.export_diagnostics().map_err(e)?.to_string_lossy().into_owned())
}

#[tauri::command]
async fn peer_health(s: State<'_, AppState>, fp: String) -> R<PeerHealth> {
    Ok(node(&s)?.peer_health(&fp).await)
}

#[tauri::command]
fn set_destination(app: AppHandle, s: State<'_, AppState>, target: String) -> R<()> {
    node(&s)?.set_destination(&target).map_err(e)?;
    refresh_tray(&app);
    Ok(())
}

#[tauri::command]
fn set_routing(app: AppHandle, s: State<'_, AppState>, on: bool) -> R<()> {
    node(&s)?.set_routing(on).map_err(e)?;
    refresh_tray(&app);
    Ok(())
}

#[tauri::command]
async fn set_sharing(s: State<'_, AppState>, on: bool) -> R<()> {
    node(&s)?.set_sharing(on).await.map_err(e)
}

#[tauri::command]
fn switch_claude_provider(s: State<'_, AppState>, on: bool) -> R<()> {
    node(&s)?.switch_claude_provider(on).map_err(e)
}
#[tauri::command]
async fn switch_claude_desktop_provider(s: State<'_, AppState>, on: bool) -> R<()> {
    let node = node(&s)?;
    tauri::async_runtime::spawn_blocking(move || node.switch_claude_desktop_provider(on))
        .await
        .map_err(|_| "Claude desktop switch stopped. Try again.".to_string())?
        .map_err(e)
}
#[tauri::command]
fn set_claude_destination(s: State<'_, AppState>, target: String) -> R<()> {
    node(&s)?.set_claude_destination(&target).map(|_| ()).map_err(e)
}
#[tauri::command]
async fn set_claude_sharing(s: State<'_, AppState>, on: bool, source: Option<aillies_node::store::ClaudeAccountSource>) -> R<()> {
    let node = node(&s)?;
    // OS credential access can wait for an owner-approved Keychain prompt.
    tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(async {
            match source {
                Some(source) => node.set_claude_sharing_from(on, source).await,
                None => node.set_claude_sharing(on).await,
            }
        })
    })
    .await
    .map_err(|_| "Claude sharing stopped. Try again.".to_string())?
    .map_err(e)
}
#[tauri::command]
async fn begin_claude_connect(s: State<'_, AppState>) -> R<String> {
    node(&s)?.begin_claude_connect().await.map_err(e)
}
#[tauri::command]
fn open_claude_authorization(s: State<'_, AppState>) -> R<()> {
    let url = node(&s)?.claude_authorization_url().map_err(e)?;
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("/usr/bin/open").arg(url).status().map_err(|_| "Open the authorization link in your browser.".to_owned())?;
        if !status.success() {
            return Err("Open the authorization link in your browser.".into());
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = url;
        Err("Connecting Claude accounts is currently available on Mac.".into())
    }
}
#[tauri::command]
fn cancel_claude_connect(s: State<'_, AppState>) -> R<()> {
    node(&s)?.cancel_claude_connect();
    Ok(())
}
#[tauri::command]
async fn finish_claude_connect(s: State<'_, AppState>, code: String) -> R<()> {
    let node = node(&s)?;
    tauri::async_runtime::spawn_blocking(move || tauri::async_runtime::block_on(node.finish_claude_connect(&code)))
        .await
        .map_err(|_| "Claude connection stopped. Connect again.".to_owned())?
        .map_err(e)
}
#[tauri::command]
async fn disconnect_claude_account(s: State<'_, AppState>) -> R<()> {
    let node = node(&s)?;
    tauri::async_runtime::spawn_blocking(move || tauri::async_runtime::block_on(node.disconnect_claude_account()))
        .await
        .map_err(|_| "Claude disconnect stopped. Try again.".to_owned())?
        .map_err(e)
}
#[tauri::command]
fn grant_codex(s: State<'_, AppState>, fp: String, on: bool) -> R<()> {
    node(&s)?.grant_codex(&fp, on).map_err(e)
}
#[tauri::command]
fn grant_claude(s: State<'_, AppState>, fp: String, on: bool) -> R<()> {
    node(&s)?.grant_claude(&fp, on).map_err(e)
}

#[tauri::command]
fn set_allow_fast_mode(s: State<'_, AppState>, on: bool) -> R<()> {
    node(&s)?.set_allow_fast_mode(on).map_err(e)
}

#[tauri::command]
async fn set_fast_mode(s: State<'_, AppState>, on: bool) -> R<()> {
    node(&s)?.set_fast_mode(on).await.map_err(e)
}

#[tauri::command]
async fn create_invite(s: State<'_, AppState>, permissions: Option<aillies_node::node::PairPermissions>) -> R<String> {
    let node = node(&s)?;
    match permissions {
        Some(p) => node.create_scoped_pairing(p, false).await.map_err(e),
        None => node.create_pairing_code().await.map_err(e),
    }
}

#[tauri::command]
async fn create_full_invite(s: State<'_, AppState>, permissions: Option<aillies_node::node::PairPermissions>) -> R<String> {
    let node = node(&s)?;
    match permissions {
        Some(p) => node.create_scoped_pairing(p, true).await.map_err(e),
        None => node.create_full_invite().await.map_err(e),
    }
}

#[tauri::command]
async fn start_pairing(s: State<'_, AppState>, code: String) -> R<PairStarted> {
    node(&s)?.start_pairing(&code).await.map_err(e)
}

#[tauri::command]
async fn confirm_pairing(s: State<'_, AppState>, id: String) -> R<PeerView> {
    node(&s)?.confirm_pairing(&id, Duration::from_secs(180)).await.map_err(e)
}

#[tauri::command]
async fn cancel_pairing(s: State<'_, AppState>, id: String) -> R<()> {
    node(&s)?.cancel_pairing(&id).await;
    Ok(())
}

#[tauri::command]
fn decide_pairing(s: State<'_, AppState>, id: String, approve: bool) -> R<()> {
    node(&s)?.approve(&id, approve).map_err(e)
}

#[tauri::command]
async fn prepare_codex_restart(s: State<'_, AppState>, on: bool) -> R<serde_json::Value> {
    let node = node(&s)?;
    tauri::async_runtime::spawn_blocking(move || node.prepare_codex_restart(on).map_err(e).and_then(|r| serde_json::to_value(r).map_err(e)))
        .await
        .map_err(|_| "Could not review Codex sessions.".to_string())?
}

#[tauri::command]
async fn restart_codex_provider(s: State<'_, AppState>, token: String) -> R<serde_json::Value> {
    let node = node(&s)?;
    tauri::async_runtime::spawn_blocking(move || node.restart_codex_provider(&token).map_err(e).and_then(|r| serde_json::to_value(r).map_err(e)))
        .await
        .map_err(|_| "Check the provider in Aillies before reopening Codex.".to_string())?
}

#[tauri::command]
async fn switch_codex_provider(s: State<'_, AppState>, on: bool) -> R<serde_json::Value> {
    let node = node(&s)?;
    tauri::async_runtime::spawn_blocking(move || node.switch_codex_provider(on).map_err(e).and_then(|r| serde_json::to_value(r).map_err(e)))
        .await
        .map_err(|_| "Could not switch Codex providers.".to_string())?
}

#[tauri::command]
fn revoke_client(s: State<'_, AppState>, fp: String) -> R<()> {
    node(&s)?.revoke_client(&fp).map_err(e)
}

#[tauri::command]
fn remove_peer(s: State<'_, AppState>, fp: String) -> R<()> {
    node(&s)?.remove_peer(&fp).map_err(e)
}

#[tauri::command]
fn set_name(s: State<'_, AppState>, name: String) -> R<()> {
    node(&s)?.set_name(&name).map_err(e)
}

#[tauri::command]
fn dismiss_failure(s: State<'_, AppState>) -> R<()> {
    node(&s)?.dismiss_failure();
    Ok(())
}

// ---------- Tray ----------

const TRAY_ID: &str = "aillies";

fn tray_image(state: &str) -> Image<'static> {
    let bytes: &'static [u8] = match state {
        "ready" => include_bytes!("../icons/tray-ready@2x.png"),
        "action" => include_bytes!("../icons/tray-action@2x.png"),
        _ => include_bytes!("../icons/tray-idle@2x.png"),
    };
    Image::from_bytes(bytes).expect("tray icon")
}

fn label(st: &NodeStatus) -> String {
    if !st.codex_installed {
        return if st.claude.installed || st.claude.desktop_installed {
            if !st.routing || st.last_failure.is_some() {
                "Action required"
            } else {
                "Claude uses Aillies"
            }
        } else {
            "Normal provider"
        }
        .into();
    }
    match st.state.as_str() {
        "ready" => "Connected".into(),
        "local" => "Using this device".into(),
        "paused" => "Action required".into(),
        _ => st.last_failure.as_ref().map(|f| short_failure(&f.code)).unwrap_or_else(|| "Action required".into()),
    }
}

fn short_failure(code: &aillies_core::FailureCode) -> String {
    use aillies_core::FailureCode::*;
    match code {
        PeerUnavailable => "Device unreachable",
        OauthExpired => "Sign-in expired on device",
        DeliveryUncertain | StreamFailed => "Last request may not have arrived",
        NotApproved => "No longer approved",
        SharingOff => "Sharing is off on device",
        _ => "Action required",
    }
    .into()
}

#[derive(Debug, PartialEq)]
enum TrayEntry {
    Item { id: String, text: String, enabled: bool, checked: Option<bool> },
    Submenu { text: String, items: Vec<TrayEntry> },
    Separator,
}

fn entry(id: &str, text: impl Into<String>, checked: Option<bool>) -> TrayEntry {
    TrayEntry::Item { id: id.into(), text: text.into(), enabled: true, checked }
}

fn tray_entries(st: &NodeStatus) -> Vec<TrayEntry> {
    let mut items = vec![TrayEntry::Item { id: "noop".into(), text: format!("Aillies — {}", label(st)), enabled: false, checked: None }];
    if let Some(p) = st.pending.first() {
        items.push(entry("open", format!("{} wants to pair…", p.name), None));
    }
    items.push(TrayEntry::Separator);
    items.push(entry(if st.codex_installed { "provider:normal" } else { "provider:aillies" }, "Activate Codex…", Some(st.codex_installed)));
    let claude = st.claude.installed || st.claude.desktop_installed;
    items.push(entry(if claude { "claude:off" } else { "claude:on" }, "Activate Claude…", Some(claude)));
    if st.codex_installed {
        items.push(entry("fast", "Use Fast mode", Some(st.fast_mode)));
    }
    items.push(TrayEntry::Separator);
    let mut accounts = vec![entry("sharing", "Codex", Some(st.sharing))];
    if st.sharing {
        accounts.push(entry("allow-fast", "Allow Fast mode", Some(st.allow_fast_mode)));
    }
    let mut claude_share =
        entry("claude-sharing", if st.claude.connected { "Claude · Connected" } else { "Claude · Connect account first" }, Some(st.claude.sharing));
    if let TrayEntry::Item { enabled, .. } = &mut claude_share {
        *enabled = st.claude.connected || st.claude.sharing;
    }
    accounts.push(claude_share);
    items.push(TrayEntry::Submenu { text: if st.platform == "macos" { "Share this Mac" } else { "Share this device" }.into(), items: accounts });
    items.push(entry("pair", "Pair a device…", None));
    items.push(entry("devices", "Manage paired devices…", None));
    items.push(entry("details", "Connection details…", None));
    items.push(TrayEntry::Separator);
    if !st.recent_warnings.is_empty() {
        items.push(entry("warnings", format!("Recent warnings ({})…", st.recent_warnings.len().min(4)), None));
    }
    items.push(entry("open", "Open Aillies…", None));
    items.push(TrayEntry::Separator);
    items.push(entry("quit", "Disconnect & quit…", None));
    items
}

fn native_entry(app: &AppHandle, item: &TrayEntry) -> tauri::Result<MenuItemKind<Wry>> {
    Ok(match item {
        TrayEntry::Item { id, text, enabled, checked: Some(checked) } => {
            MenuItemKind::Check(CheckMenuItem::with_id(app, id.clone(), text, *enabled, *checked, None::<&str>)?)
        }
        TrayEntry::Item { id, text, enabled, checked: None } => MenuItemKind::MenuItem(MenuItem::with_id(app, id.clone(), text, *enabled, None::<&str>)?),
        TrayEntry::Separator => MenuItemKind::Predefined(PredefinedMenuItem::separator(app)?),
        TrayEntry::Submenu { text, items } => {
            let submenu = Submenu::new(app, text, true)?;
            for item in items {
                submenu.append(&native_entry(app, item)?)?;
            }
            MenuItemKind::Submenu(submenu)
        }
    })
}

fn build_menu(app: &AppHandle, st: &NodeStatus) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    for item in tray_entries(st) {
        menu.append(&native_entry(app, &item)?)?;
    }
    Ok(menu)
}

fn signature(st: &NodeStatus) -> String {
    format!("{:?}", (tray_entries(st), st.pending.len(), st.activation.as_ref().map(|a| (&a.id, a.phase))))
}

static LAST_ACTIVATION_NOTICE: Mutex<String> = Mutex::new(String::new());

static LAST_SIG: Mutex<(String, usize)> = Mutex::new((String::new(), 0));

fn refresh_tray(app: &AppHandle) {
    let Some(node) = app.state::<AppState>().node.clone() else { return };
    let st = node.status();
    let sig = signature(&st);
    if let Some(a) = &st.activation {
        if a.service != aillies_node::node::Activation::Quit
            && a.mode != aillies_node::node::ActivationMode::Now
            && matches!(a.phase, aillies_node::node::ActivationPhase::Completed | aillies_node::node::ActivationPhase::Failed)
        {
            let notice = format!("{}:{:?}", a.id, a.phase);
            let mut last = LAST_ACTIVATION_NOTICE.lock().unwrap();
            if *last != notice {
                *last = notice;
                show_window(app);
                let _ = app.emit("aillies:navigate", ());
            }
        }
    }
    {
        let mut last = LAST_SIG.lock().unwrap();
        if last.0 == sig {
            return;
        }
        let had_pending = last.1;
        if st.pending.len() > had_pending {
            show_window(app);
        }
        *last = (sig, st.pending.len());
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Ok(menu) = build_menu(app, &st) {
            let _ = tray.set_menu(Some(menu));
        }
        let _ = tray.set_icon(Some(tray_image(if !st.codex_installed {
            "local"
        } else if !st.routing {
            "action"
        } else {
            &st.state
        })));
        let _ = tray.set_tooltip(Some(format!("Aillies — {} · {}", label(&st), st.destination_name)));
    }
}

#[derive(Clone, Copy)]
enum WindowStep {
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    Runtime,
    Create,
    Position,
    Restore,
    Show,
    Focus,
    Panic,
}

impl WindowStep {
    fn code(self) -> &'static str {
        match self {
            Self::Runtime => "webview_runtime_unavailable",
            Self::Create => "create",
            Self::Position => "position",
            Self::Restore => "restore",
            Self::Show => "show",
            Self::Focus => "focus",
            Self::Panic => "worker_panic",
        }
    }
}

static WINDOW_GATE: LazyLock<OpenGate> = LazyLock::new(OpenGate::default);
static WINDOW_DEFERRED: AtomicBool = AtomicBool::new(false);

/// WebView2 creation must not block a Windows menu/event callback. All entry
/// points share one worker, including first launch and single-instance reopen.
fn show_window(app: &AppHandle) {
    WINDOW_DEFERRED.store(true, Ordering::Release);
    if app.try_state::<AppState>().is_none() {
        log_line(LifecycleEvent::WindowDeferred);
        return;
    }
    WINDOW_DEFERRED.store(false, Ordering::Release);
    let Some(lease) = WINDOW_GATE.acquire() else { return };
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        log_line(LifecycleEvent::WindowOpening);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| present_window(&handle)));
        let failure = match result {
            Ok(Ok(())) => return,
            Ok(Err(step)) => step,
            Err(_) => WindowStep::Panic,
        };
        log_line(LifecycleEvent::WindowStepFailed(failure));
        window_failure_notice(&handle, failure);
    });
}

fn present_window(app: &AppHandle) -> Result<(), WindowStep> {
    let window = match app.get_webview_window("main") {
        Some(w) => w,
        None => {
            #[cfg(target_os = "windows")]
            if tauri::webview_version().is_err() {
                return Err(WindowStep::Runtime);
            }
            let builder = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
                .title("Aillies")
                .inner_size(600.0, 820.0)
                .min_inner_size(420.0, 520.0)
                .center()
                .visible(false);
            // A new UI-only profile avoids carrying a locked/broken legacy
            // WebView profile forward. Never rename or remove old browser data;
            // node accounts, pairing state and chat history live elsewhere.
            #[cfg(target_os = "windows")]
            let builder = builder
                // Tao's native OLE drop target is unused by Aillies and is the
                // exact RPC_E_CHANGED_MODE panic site in the reported build.
                .drag_and_drop(false)
                .disable_drag_drop_handler()
                .data_directory(app.path().app_local_data_dir().map_err(|_| WindowStep::Create)?.join("ui-webview-v2"));
            let w = builder.build().map_err(|error| {
                log_line(LifecycleEvent::WindowCreateCode(window_recovery::creation_code(&error.to_string())));
                WindowStep::Create
            })?;
            let handle = w.clone();
            w.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = handle.hide();
                }
            });
            log_line(LifecycleEvent::WindowCreated);
            w
        }
    };
    // Best-effort restoration must not prevent show() if Windows rejects a
    // minimized-state query or the display topology is changing.
    if window.unminimize().is_err() {
        log_line(LifecycleEvent::WindowStepFailed(WindowStep::Restore));
    }
    let recover_position = || -> Result<(), WindowStep> {
        let position = window.outer_position().map_err(|_| WindowStep::Position)?;
        let size = window.outer_size().map_err(|_| WindowStep::Position)?;
        let monitors = window.available_monitors().map_err(|_| WindowStep::Position)?;
        let rects: Vec<_> = monitors
            .iter()
            .map(|m| Rect { x: i64::from(m.position().x), y: i64::from(m.position().y), width: i64::from(m.size().width), height: i64::from(m.size().height) })
            .collect();
        if window_recovery::needs_recenter(
            Rect { x: i64::from(position.x), y: i64::from(position.y), width: i64::from(size.width), height: i64::from(size.height) },
            &rects,
        ) {
            window.center().map_err(|_| WindowStep::Position)?;
        }
        Ok(())
    };
    if let Err(step) = recover_position() {
        log_line(LifecycleEvent::WindowStepFailed(step));
    }
    window.show().map_err(|_| WindowStep::Show)?;
    // Foreground restrictions can deny focus even though the window is visible.
    if window.set_focus().is_err() {
        log_line(LifecycleEvent::WindowStepFailed(WindowStep::Focus));
    }
    Ok(())
}

fn window_failure_notice(app: &AppHandle, step: WindowStep) {
    log_line(LifecycleEvent::WindowFailed);
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some("Aillies could not open its window. Choose Open Aillies to retry; see desktop.log."));
    }
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONWARNING, MB_OK, MB_YESNO};
        let (message, flags) = if matches!(step, WindowStep::Runtime) {
            ("Aillies needs Microsoft Edge WebView2 Runtime to open its window. Your accounts and pairings are kept. Open Microsoft's runtime download page?", MB_YESNO)
        } else {
            ("Aillies could not open its window. Choose Open Aillies in the tray to retry, or quit and reopen Aillies. Details are in desktop.log. Your accounts and pairings are kept.", MB_OK)
        };
        let title: Vec<u16> = "Aillies".encode_utf16().chain(Some(0)).collect();
        let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        // Fixed UTF-16 strings, no account data or raw WebView errors.
        let answer = unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), flags | MB_ICONWARNING) };
        if matches!(step, WindowStep::Runtime) && answer == IDYES {
            let _ = std::process::Command::new("cmd").args(["/C", "start", "", "https://go.microsoft.com/fwlink/p/?LinkId=2124703"]).spawn();
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = step;
}

fn on_menu(app: &AppHandle, id: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        show_window(app);
        return;
    };
    let Some(node) = state.node.clone() else {
        if id == "quit" {
            app.exit(0);
        } else {
            show_window(app);
        }
        return;
    };
    if let Some(intent) = navigation_for(id) {
        open_intent(app, intent);
        return;
    }
    let handle = app.clone();
    let id = id.to_owned();
    tauri::async_runtime::spawn(async move {
        let result = match id.as_str() {
            "sharing" => node.set_sharing(!node.status().sharing).await,
            "claude-sharing" => node.set_claude_sharing_from(!node.status().claude.sharing, aillies_node::store::ClaudeAccountSource::Connected).await,
            "fast" => node.set_fast_mode(!node.status().fast_mode).await,
            "allow-fast" => node.set_allow_fast_mode(!node.status().allow_fast_mode),
            other => match other.strip_prefix("dest:") {
                Some(target) => node.set_destination(target).map(|_| ()),
                None => return,
            },
        };
        if let Err(error) = result {
            open_intent(&handle, WindowIntent::Error { message: error.to_string() });
        }
        LAST_SIG.lock().unwrap().0.clear();
        refresh_tray(&handle);
    });
}

fn open_intent(app: &AppHandle, intent: WindowIntent) {
    if let Some(state) = app.try_state::<AppState>() {
        *state.navigation.lock().unwrap_or_else(|e| e.into_inner()) = Some(intent);
    }
    show_window(app);
    let _ = app.emit("aillies:navigate", ());
}

fn make_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let menu = Menu::with_items(
        app,
        &[&MenuItem::with_id(app, "open", "Open Aillies…", true, None::<&str>)?, &MenuItem::with_id(app, "quit", "Quit Aillies", true, None::<&str>)?],
    )?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tray_image("idle"))
        .icon_as_template(false)
        .tooltip("Aillies")
        .menu(&menu)
        .show_menu_on_left_click(!cfg!(target_os = "windows"))
        .on_tray_icon_event(|tray, event| {
            if cfg!(target_os = "windows")
                && matches!(
                    event,
                    tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, button_state: tauri::tray::MouseButtonState::Up, .. }
                )
            {
                show_window(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .build(app)
}

fn start_on_worker<T: Send + 'static>(task: impl std::future::Future<Output = Result<T, String>> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::block_on(tauri::async_runtime::spawn(task))
        .unwrap_or_else(|_| Err("The connection service stopped during startup. Restart Aillies.".into()))
}

pub fn run() {
    std::panic::set_hook(Box::new(|info| {
        log_line(LifecycleEvent::Panic);
        if let Some(site) = info.location() {
            log_line(LifecycleEvent::PanicSite(window_recovery::panic_component(site.file()), site.line()));
        }
    }));
    // Initialize before logging/path discovery, Tauri setup or node startup.
    // Hold this thread-bound guard until the UI event loop has exited.
    #[cfg(target_os = "windows")]
    let _ui_com = match windows_com::UiApartment::sta() {
        Ok(apartment) => apartment,
        Err(_) => {
            log_line(LifecycleEvent::Starting);
            log_line(LifecycleEvent::WindowCreateCode("startup_com_initialization_failed"));
            windows_com::show_startup_failure();
            return;
        }
    };
    log_line(LifecycleEvent::Starting);
    #[cfg(target_os = "windows")]
    log_line(LifecycleEvent::UiStaReady);
    let result = tauri::Builder::default()
        // A second launch (Start menu, desktop icon) just opens the running app's window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Do not let node/WinRT initialization choose the UI thread's COM
            // apartment. Even synchronous NodeConfig work runs on the worker.
            let started = start_on_worker(async { Node::start(NodeConfig::from_env()).await.map_err(|error| error.to_string()) });
            let (node, error) = match started {
                Ok(n) => {
                    log_line(LifecycleEvent::NodeStarted(n.loopback_port()));
                    (Some(n), None)
                }
                Err(err) => {
                    log_line(LifecycleEvent::NodeFailed);
                    (None, Some(err))
                }
            };
            let first_run = node.as_ref().map(|n| {
                let s = n.status();
                s.peers.is_empty() && !s.sharing
            });
            app.manage(AppState { node, error: error.clone(), navigation: Mutex::new(None) });
            if make_tray(app.handle()).is_err() {
                log_line(LifecycleEvent::TrayFailed);
            }
            let reopen_requested = WINDOW_DEFERRED.swap(false, Ordering::AcqRel);
            if error.is_some() || first_run == Some(true) || reopen_requested {
                show_window(app.handle());
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    refresh_tray(&handle);
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            take_navigation,
            quit_if_normal,
            prepare_activation,
            apply_activation,
            repair_settings_access,
            cancel_activation,
            schedule_activation,
            cancel_scheduled_activation,
            open_automation_settings,
            dismiss_activation_notice,
            cancel_invitation,
            quit_anyway,
            diagnostics,
            startup_error,
            export_diagnostics,
            status,
            peer_health,
            set_destination,
            set_routing,
            set_sharing,
            set_fast_mode,
            set_allow_fast_mode,
            switch_claude_provider,
            switch_claude_desktop_provider,
            set_claude_destination,
            set_claude_sharing,
            begin_claude_connect,
            open_claude_authorization,
            cancel_claude_connect,
            finish_claude_connect,
            disconnect_claude_account,
            grant_claude,
            grant_codex,
            create_invite,
            create_full_invite,
            start_pairing,
            confirm_pairing,
            cancel_pairing,
            decide_pairing,
            switch_codex_provider,
            prepare_codex_restart,
            restart_codex_provider,
            revoke_client,
            remove_peer,
            set_name,
            dismiss_failure
        ])
        .build(tauri::generate_context!());
    match result {
        // Keep running in the tray when the window is closed.
        Ok(app) => app.run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, code: None, .. } = event {
                api.prevent_exit();
                if app.state::<AppState>().node.is_some() {
                    open_intent(app, WindowIntent::Quit);
                } else {
                    app.exit(0);
                }
            }
        }),
        Err(_) => log_line(LifecycleEvent::AppFailed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aillies_core::protocol::AccountReadiness;
    use aillies_core::FailureCode;
    use aillies_node::node::FailureView;
    use aillies_node::store::Destination;

    #[test]
    fn node_startup_future_runs_off_the_ui_caller_and_panic_is_a_fixed_failure() {
        let caller = std::thread::current().id();
        let worker = start_on_worker(async { Ok(std::thread::current().id()) }).unwrap();
        assert_ne!(caller, worker);
        let failure = start_on_worker::<()>(async { panic!("synthetic private startup payload") });
        assert_eq!(failure.unwrap_err(), "The connection service stopped during startup. Restart Aillies.");
    }

    #[test]
    fn window_diagnostics_are_fixed_stage_codes_without_error_payloads() {
        let mut bytes = Vec::new();
        for step in [WindowStep::Runtime, WindowStep::Create, WindowStep::Position, WindowStep::Restore, WindowStep::Show, WindowStep::Focus, WindowStep::Panic]
        {
            write_lifecycle_log(&mut bytes, LifecycleEvent::WindowStepFailed(step)).unwrap();
        }
        let text = String::from_utf8(bytes).unwrap();
        let codes: Vec<_> = text.lines().map(|l| l.split_once("window failed: ").unwrap().1).collect();
        assert_eq!(codes, ["webview_runtime_unavailable", "create", "position", "restore", "show", "focus", "worker_panic"]);
    }

    #[test]
    fn lifecycle_log_accepts_only_fixed_events_and_public_port() {
        let mut log = Vec::new();
        for event in [
            LifecycleEvent::Starting,
            LifecycleEvent::NodeStarted(18522),
            LifecycleEvent::WindowCreated,
            LifecycleEvent::WindowFailed,
            LifecycleEvent::NodeFailed,
            LifecycleEvent::TrayFailed,
            LifecycleEvent::AppFailed,
            LifecycleEvent::Panic,
        ] {
            write_lifecycle_log(&mut log, event).unwrap();
        }
        let text = String::from_utf8(log).unwrap();
        assert_eq!(text.lines().count(), 8);
        for line in text.lines() {
            let (_, event) = line.split_once(' ').unwrap();
            assert!([
                format!("starting Aillies {}", env!("CARGO_PKG_VERSION")),
                "node started on 127.0.0.1:18522".into(),
                "window created".into(),
                "window failed".into(),
                "node failed".into(),
                "tray failed".into(),
                "app failed to start".into(),
                "panic; restart Aillies".into()
            ]
            .contains(&event.to_string()));
        }
    }

    fn status(state: &str, failure: Option<FailureCode>) -> NodeStatus {
        NodeStatus {
            internet_ready: false,
            history_warnings: vec![],
            claude: aillies_node::node::ClaudeStatus {
                reconnect_required: false,
                connected: false,
                desktop_installed: false,
                account_source: aillies_node::store::ClaudeAccountSource::Code,
                installed: false,
                sharing: false,
                account: AccountReadiness::Ready,
                destination: Destination::Local,
                destination_name: "This device".into(),
            },
            recent_warnings: Vec::new(),
            diagnostics_build: "router-core-v4-pairing".into(),
            allow_fast_mode: false,
            fast_mode: false,
            name: "A".into(),
            platform: "macos".into(),
            short_id: "0000·0000".into(),
            fingerprint: "0".repeat(64),
            version: "test".into(),
            state: state.into(),
            routing: state != "paused",
            sharing: false,
            destination: Destination::Local,
            destination_name: "This device".into(),
            peers: vec![],
            clients: vec![],
            pending: vec![],
            codex_installed: true,
            provider_switch: None,
            activation: None,
            local_account: AccountReadiness::Ready,
            last_failure: failure.map(|code| FailureView { code, message: String::new(), at: 0 }),
            loopback_port: 18522,
            peer_port: None,
        }
    }

    #[test]
    fn tray_labels_follow_the_three_states() {
        assert_eq!(label(&status("ready", None)), "Connected");
        assert_eq!(label(&status("local", None)), "Using this device");
        assert_eq!(label(&status("paused", None)), "Action required");
        assert_eq!(label(&status("action", Some(FailureCode::PeerUnavailable))), "Device unreachable");
        assert_eq!(label(&status("action", Some(FailureCode::DeliveryUncertain))), "Last request may not have arrived");
        assert_eq!(label(&status("action", Some(FailureCode::ProtocolMismatch))), "Action required");
    }

    #[test]
    fn tray_icons_decode_for_every_state() {
        for s in ["ready", "local", "paused", "action"] {
            let img = tray_image(s);
            assert_eq!(img.width(), 44);
            assert_eq!(img.height(), 44);
        }
    }

    #[test]
    fn signature_changes_when_the_menu_must_change() {
        let base = signature(&status("ready", None));
        assert_eq!(base, signature(&status("ready", None)));
        assert_ne!(base, signature(&status("paused", None)));
        assert_ne!(base, signature(&status("action", Some(FailureCode::OauthExpired))));
        let mut pending = status("ready", None);
        pending.pending.push(aillies_node::node::PendingView {
            permissions: Default::default(),
            id: "1".into(),
            name: "B".into(),
            platform: "linux".into(),
            code: "X".into(),
            short_id: "1".into(),
        });
        assert_ne!(base, signature(&pending));
        let first_pending = signature(&pending);
        pending.pending.push(pending.pending[0].clone());
        assert_ne!(first_pending, signature(&pending));
        let mut changed = status("ready", None);
        changed.sharing = true;
        assert_ne!(base, signature(&changed));
        let sharing = signature(&changed);
        changed.allow_fast_mode = true;
        assert_ne!(sharing, signature(&changed));
        changed = status("ready", None);
        changed.fast_mode = true;
        assert_ne!(base, signature(&changed));
        changed = status("ready", None);
        changed.codex_installed = false;
        assert_ne!(base, signature(&changed));
    }

    #[test]
    fn tray_matches_provider_sharing_and_navigation_contract() {
        let mut st = status("ready", None);
        let active = format!("{:?}", tray_entries(&st));
        assert!(active.contains("Activate Codex") && active.contains("Activate Claude") && active.contains("Use Fast mode"));
        assert!(!active.contains("routing") && !active.contains("Allow Fast mode"));
        st.sharing = true;
        assert!(format!("{:?}", tray_entries(&st)).contains("Allow Fast mode"));
        st.codex_installed = false;
        let normal = format!("{:?}", tray_entries(&st));
        assert!(normal.contains("Activate Codex") && normal.contains("Pair a device") && normal.contains("Disconnect & quit"));
        assert!(!normal.contains("Use Fast mode"));
        assert_eq!(navigation_for("provider:normal"), Some(WindowIntent::Codex { on: Some(false) }));
        assert_eq!(navigation_for("provider:aillies"), Some(WindowIntent::Codex { on: Some(true) }));
        assert_eq!(navigation_for("pair"), Some(WindowIntent::Pair));
        assert_eq!(navigation_for("claude"), Some(WindowIntent::Claude));
        st.claude.installed = true;
        assert_eq!(label(&st), "Claude uses Aillies");
        assert!(format!("{:?}", tray_entries(&st)).contains("claude:off"));
        st.claude.installed = false;
        st.claude.desktop_installed = true;
        assert_eq!(label(&st), "Claude uses Aillies");
        assert!(format!("{:?}", tray_entries(&st)).contains("claude:off"));
        assert_eq!(navigation_for("quit"), Some(WindowIntent::Quit));
        assert_eq!(navigation_for("dest:local"), None);
        assert_eq!(serde_json::to_value(WindowIntent::Codex { on: Some(false) }).unwrap(), serde_json::json!({"screen":"codex","on":false}));
    }
}
