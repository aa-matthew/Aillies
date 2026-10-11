//! Aillies desktop shell: tray/menu-bar item plus one compact window.
//! All routing and security behavior lives in `aillies-node`.

use aillies_node::node::{NodeStatus, PairStarted, PeerHealth, PeerView};
use aillies_node::{Node, NodeConfig};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, State, WindowEvent, Wry};

/// Desktop startup log (no content, just lifecycle and errors) so a silent
/// failure on someone's machine can be diagnosed: <Aillies home>/desktop.log
fn log_line(msg: &str) {
    use std::io::Write;
    let dir = aillies_node::node::default_home();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("desktop.log")) {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "{t} {msg}");
    }
}

struct AppState {
    node: Option<Arc<Node>>,
    error: Option<String>,
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
async fn create_invite(s: State<'_, AppState>) -> R<String> {
    node(&s)?.create_invite().await.map_err(e)
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
fn codex_enable(s: State<'_, AppState>, model: Option<String>) -> R<()> {
    node(&s)?.codex_enable(model.as_deref()).map_err(e)
}

#[tauri::command]
fn codex_restore(s: State<'_, AppState>) -> R<()> {
    node(&s)?.codex_restore().map_err(e)
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
    match st.state.as_str() {
        "ready" => "Connected".into(),
        "local" => "Using this device".into(),
        "paused" => "Paused".into(),
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

fn build_menu(app: &AppHandle, st: &NodeStatus) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "noop", format!("Aillies — {}", label(st)), false, None::<&str>)?)?;
    if let Some(p) = st.pending.first() {
        menu.append(&MenuItem::with_id(app, "open", format!("{} wants to pair…", p.name), true, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "noop", "Send through", false, None::<&str>)?)?;
    let local = matches!(st.destination, aillies_node::store::Destination::Local);
    menu.append(&CheckMenuItem::with_id(app, "dest:local", "This device", true, local, None::<&str>)?)?;
    for p in &st.peers {
        let checked = matches!(&st.destination, aillies_node::store::Destination::Peer(fp) if fp == &p.fp);
        menu.append(&CheckMenuItem::with_id(app, format!("dest:{}", p.fp), &p.name, true, checked, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "open", "Open Aillies…", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "routing", if st.routing { "Pause routing" } else { "Resume routing" }, true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit Aillies", true, None::<&str>)?)?;
    Ok(menu)
}

fn signature(st: &NodeStatus) -> String {
    format!(
        "{}|{}|{:?}|{}|{}|{:?}",
        st.state,
        st.routing,
        st.destination,
        st.peers.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(","),
        st.pending.len(),
        st.last_failure.as_ref().map(|f| f.code)
    )
}

static LAST_SIG: Mutex<String> = Mutex::new(String::new());

fn refresh_tray(app: &AppHandle) {
    let Some(node) = app.state::<AppState>().node.clone() else { return };
    let st = node.status();
    let sig = signature(&st);
    {
        let mut last = LAST_SIG.lock().unwrap();
        if *last == sig {
            return;
        }
        let had_pending = last.split('|').nth(4).and_then(|n| n.parse::<usize>().ok()).unwrap_or(0);
        if st.pending.len() > had_pending {
            show_window(app);
        }
        *last = sig;
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Ok(menu) = build_menu(app, &st) {
            let _ = tray.set_menu(Some(menu));
        }
        let _ = tray.set_icon(Some(tray_image(&st.state)));
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
                    log_line("window created");
                    w
                }
                Err(err) => {
                    log_line(&format!("window failed: {err}"));
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
    match id {
        "open" => show_window(app),
        "routing" => {
            let on = !node.status().routing;
            let _ = node.set_routing(on);
        }
        "quit" => {
            node.shutdown();
            app.exit(0);
        }
        other => {
            if let Some(target) = other.strip_prefix("dest:") {
                let _ = node.set_destination(target);
            }
        }
    }
    LAST_SIG.lock().unwrap().clear();
    refresh_tray(app);
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
    std::panic::set_hook(Box::new(|info| log_line(&format!("panic: {info}"))));
    log_line(&format!("starting Aillies {}", env!("CARGO_PKG_VERSION")));
    let result = tauri::Builder::default()
        // A second launch (Start menu, desktop icon) just opens the running app's window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let started = tauri::async_runtime::block_on(Node::start(NodeConfig::from_env()));
            let (node, error) = match started {
                Ok(n) => {
                    log_line(&format!("node started on 127.0.0.1:{}", n.loopback_port()));
                    (Some(n), None)
                }
                Err(err) => {
                    log_line(&format!("node failed: {err}"));
                    (None, Some(err.to_string()))
                }
            };
            let first_run = node.as_ref().map(|n| {
                let s = n.status();
                s.peers.is_empty() && !s.sharing
            });
            app.manage(AppState { node, error: error.clone() });
            if let Err(err) = make_tray(app.handle()) {
                log_line(&format!("tray failed: {err}"));
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
            startup_error,
            status,
            peer_health,
            set_destination,
            set_routing,
            set_sharing,
            create_invite,
            start_pairing,
            confirm_pairing,
            cancel_pairing,
            decide_pairing,
            codex_enable,
            codex_restore,
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
        Err(err) => log_line(&format!("app failed to start: {err}")),
    }
}
