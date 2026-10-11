//! Aillies desktop shell: tray/menu-bar item plus one compact window.
//! All routing and security behavior lives in `aillies-node`.

use aillies_node::node::{NodeStatus, PairStarted, PeerHealth, PeerView};
use aillies_node::{Node, NodeConfig};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent, Wry};

/// Typed lifecycle events cannot carry prompts, upstream errors or panic payloads.
#[derive(Clone, Copy)]
enum LifecycleEvent {
    Starting,
    NodeStarted(u16),
    WindowCreated,
    WindowFailed,
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
fn set_allow_fast_mode(s: State<'_, AppState>, on: bool) -> R<()> {
    node(&s)?.set_allow_fast_mode(on).map_err(e)
}

#[tauri::command]
async fn set_fast_mode(s: State<'_, AppState>, on: bool) -> R<()> {
    node(&s)?.set_fast_mode(on).await.map_err(e)
}

#[tauri::command]
async fn create_invite(s: State<'_, AppState>) -> R<String> {
    node(&s)?.create_pairing_code().await.map_err(e)
}

#[tauri::command]
async fn create_full_invite(s: State<'_, AppState>) -> R<String> {
    node(&s)?.create_full_invite().await.map_err(e)
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
        return "Normal provider".into();
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
    items.push(TrayEntry::Submenu {
        text: format!("Codex provider: {}", if st.codex_installed { "Aillies" } else { "Normal" }),
        items: vec![entry("provider:normal", "Normal…", Some(!st.codex_installed)), entry("provider:aillies", "Aillies…", Some(st.codex_installed))],
    });
    if st.codex_installed {
        let mut devices = vec![entry(
            "dest:local",
            if st.platform == "macos" { "This Mac" } else { "This device" },
            Some(matches!(st.destination, aillies_node::store::Destination::Local)),
        )];
        for p in &st.peers {
            devices.push(entry(&format!("dest:{}", p.fp), &p.name, Some(matches!(&st.destination, aillies_node::store::Destination::Peer(fp) if fp == &p.fp))));
        }
        items.push(TrayEntry::Submenu { text: format!("Send through: {}", st.destination_name), items: devices });
        items.push(entry("fast", "Use Fast mode", Some(st.fast_mode)));
    }
    items.push(TrayEntry::Separator);
    items.push(entry("sharing", if st.platform == "macos" { "Share this Mac" } else { "Share this device" }, Some(st.sharing)));
    if st.sharing {
        items.push(entry("allow-fast", "Allow Fast mode", Some(st.allow_fast_mode)));
    }
    items.push(entry("pair", "Pair a device…", None));
    items.push(TrayEntry::Separator);
    if !st.recent_warnings.is_empty() {
        items.push(entry("warnings", format!("Recent warnings ({})…", st.recent_warnings.len().min(4)), None));
    }
    items.push(entry("open", "Open Aillies…", None));
    items.push(TrayEntry::Separator);
    items.push(entry("quit", "Quit Aillies", None));
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
    format!("{:?}", (tray_entries(st), st.pending.len()))
}

static LAST_SIG: Mutex<(String, usize)> = Mutex::new((String::new(), 0));

fn refresh_tray(app: &AppHandle) {
    let Some(node) = app.state::<AppState>().node.clone() else { return };
    let st = node.status();
    let sig = signature(&st);
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

/// The window is created on demand, after the node and tray are running, so
/// routing keeps working even if the system web view is missing or broken.
fn show_window(app: &AppHandle) {
    let window = match app.get_webview_window("main") {
        Some(w) => w,
        None => {
            let built = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
                .title("Aillies")
                .inner_size(480.0, 640.0)
                .min_inner_size(420.0, 520.0)
                .center()
                .visible(false)
                .build();
            match built {
                Ok(w) => {
                    let handle = w.clone();
                    w.on_window_event(move |event| {
                        // Closing keeps routing and sharing running; Quit stops them.
                        if let WindowEvent::CloseRequested { api, .. } = event {
                            api.prevent_close();
                            let _ = handle.hide();
                        }
                    });
                    log_line(LifecycleEvent::WindowCreated);
                    w
                }
                Err(_) => {
                    log_line(LifecycleEvent::WindowFailed);
                    if let Some(tray) = app.tray_by_id(TRAY_ID) {
                        let _ = tray.set_tooltip(Some("Aillies is running, but its window can't open. Install Microsoft Edge WebView2 Runtime."));
                    }
                    #[cfg(target_os = "windows")]
                    let _ = std::process::Command::new("cmd").args(["/C", "start", "", "https://go.microsoft.com/fwlink/p/?LinkId=2124703"]).spawn();
                    return;
                }
            }
        }
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn on_menu(app: &AppHandle, id: &str) {
    let Some(node) = app.state::<AppState>().node.clone() else {
        if id == "quit" {
            app.exit(0);
        }
        return;
    };
    if let Some(intent) = navigation_for(id) {
        if id == "quit" && node.shutdown_if_normal().is_ok() {
            app.exit(0);
            return;
        }
        open_intent(app, intent);
        return;
    }
    let handle = app.clone();
    let id = id.to_owned();
    tauri::async_runtime::spawn(async move {
        let result = match id.as_str() {
            "sharing" => node.set_sharing(!node.status().sharing).await,
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
    *app.state::<AppState>().navigation.lock().unwrap() = Some(intent);
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
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .build(app)
}

pub fn run() {
    std::panic::set_hook(Box::new(|_| log_line(LifecycleEvent::Panic)));
    log_line(LifecycleEvent::Starting);
    let result = tauri::Builder::default()
        // A second launch (Start menu, desktop icon) just opens the running app's window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let started = tauri::async_runtime::block_on(Node::start(NodeConfig::from_env()));
            let (node, error) = match started {
                Ok(n) => {
                    log_line(LifecycleEvent::NodeStarted(n.loopback_port()));
                    (Some(n), None)
                }
                Err(err) => {
                    log_line(LifecycleEvent::NodeFailed);
                    (None, Some(err.to_string()))
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
            if error.is_some() || first_run == Some(true) {
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
            startup_error,
            export_diagnostics,
            status,
            peer_health,
            set_destination,
            set_routing,
            set_sharing,
            set_fast_mode,
            set_allow_fast_mode,
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
        Ok(app) => app.run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { api, code: None, .. } = event {
                api.prevent_exit();
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
        assert!(active.contains("Send through") && active.contains("Use Fast mode"));
        assert!(!active.contains("routing") && !active.contains("Allow Fast mode"));
        st.sharing = true;
        assert!(format!("{:?}", tray_entries(&st)).contains("Allow Fast mode"));
        st.codex_installed = false;
        let normal = format!("{:?}", tray_entries(&st));
        assert!(normal.contains("Normal provider") && normal.contains("Pair a device"));
        assert!(!normal.contains("Send through") && !normal.contains("Use Fast mode"));
        assert_eq!(navigation_for("provider:normal"), Some(WindowIntent::Codex { on: Some(false) }));
        assert_eq!(navigation_for("provider:aillies"), Some(WindowIntent::Codex { on: Some(true) }));
        assert_eq!(navigation_for("pair"), Some(WindowIntent::Pair));
        assert_eq!(navigation_for("quit"), Some(WindowIntent::Quit));
        assert_eq!(navigation_for("dest:local"), None);
        assert_eq!(serde_json::to_value(WindowIntent::Codex { on: Some(false) }).unwrap(), serde_json::json!({"screen":"codex","on":false}));
    }
}
