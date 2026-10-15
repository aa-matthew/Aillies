//! Window-only coordination and geometry; no routing or account state.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct OpenGate(Arc<AtomicBool>);

pub(crate) struct OpenLease(Arc<AtomicBool>);

impl OpenGate {
    pub(crate) fn acquire(&self) -> Option<OpenLease> {
        self.0.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).ok().map(|_| OpenLease(self.0.clone()))
    }
}

impl Drop for OpenLease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Rect {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

/// Keep a usable piece of the title bar on a live monitor, including negative
/// coordinates and mixed-DPI physical monitor coordinates. No monitors means
/// enumeration is unavailable, not proof that the window should be moved.
pub(crate) fn needs_recenter(window: Rect, monitors: &[Rect]) -> bool {
    !monitors.is_empty()
        && !monitors.iter().any(|m| {
            let width = (window.x + window.width).min(m.x + m.width) - window.x.max(m.x);
            let height = (window.y + window.height.min(32)).min(m.y + m.height) - window.y.max(m.y);
            width >= 64 && height >= 16
        })
}

/// Record a closed code only. Never retain raw native error text or paths.
pub(crate) fn creation_code(message: &str) -> &'static str {
    let bounded: String = message.chars().take(2048).collect::<String>().to_ascii_uppercase();
    for (needle, code) in [
        ("0X80070005", "hresult_80070005"),
        ("0X80070020", "hresult_80070020"),
        ("0X800700B7", "hresult_800700b7"),
        ("0X80004002", "hresult_80004002"),
        ("0X80004005", "hresult_80004005"),
        ("0X80070002", "hresult_80070002"),
    ] {
        if bounded.contains(needle) {
            return code;
        }
    }
    "unclassified"
}

pub(crate) fn panic_component(path: &str) -> &'static str {
    // The compile-time path is used only for classification, never written.
    let path = path.replace('\\', "/");
    for (marker, component) in
        [("/tauri-runtime-wry-", "tauri_runtime_wry"), ("/tauri-", "tauri"), ("/wry-", "wry"), ("/tao-", "tao"), ("/webview2-com-", "webview2_com")]
    {
        if path.contains(marker) {
            return component;
        }
    }
    if path == "src/lib.rs" || path == "src/window_recovery.rs" {
        "aillies_shell"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_diagnostics_discard_messages_and_private_compile_paths() {
        assert_eq!(creation_code("private-user/token prompt 0x80070005"), "hresult_80070005");
        assert_eq!(creation_code("private-user/token prompt arbitrary failure"), "unclassified");
        assert_eq!(creation_code(&("a".repeat(2048) + "0x80070005")), "unclassified");
        assert_eq!(panic_component(r"C:\Users\private-user\registry\tao-0.37.1\src\event_loop.rs"), "tao");
        assert_eq!(panic_component("/Users/private-user/registry/tauri-runtime-wry-2.12.1/src/lib.rs"), "tauri_runtime_wry");
        assert_eq!(panic_component("/private/unknown/path"), "other");
    }

    #[test]
    fn repeated_opens_coalesce_and_failure_or_panic_releases_the_gate() {
        let gate = OpenGate::default();
        let lease = gate.acquire().expect("first request");
        assert!(gate.acquire().is_none());
        drop(lease);
        let result = std::panic::catch_unwind(|| {
            let _lease = gate.acquire().expect("retry after failed creation");
            assert!(gate.acquire().is_none());
            panic!("synthetic window failure");
        });
        assert!(result.is_err());
        assert!(gate.acquire().is_some(), "a later click can recover after panic");
    }

    #[test]
    fn simultaneous_tray_and_second_launch_requests_have_one_creator() {
        let gate = Arc::new(OpenGate::default());
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let (gate, barrier) = (gate.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    let lease = gate.acquire();
                    barrier.wait();
                    lease.is_some()
                })
            })
            .collect();
        assert_eq!(handles.into_iter().map(|h| h.join().unwrap()).filter(|won| *won).count(), 1);
        assert!(gate.acquire().is_some());
    }

    #[test]
    fn disconnected_monitors_and_hidden_title_bars_recenter_but_valid_positions_stay() {
        let primary = Rect { x: 0, y: 0, width: 1920, height: 1080 };
        let left = Rect { x: -2560, y: 0, width: 2560, height: 1440 };
        let w = Rect { x: -1800, y: 100, width: 600, height: 820 };
        assert!(!needs_recenter(w, &[primary, left]));
        assert!(needs_recenter(w, &[primary]));
        assert!(needs_recenter(Rect { y: -800, x: 100, ..w }, &[primary]));
        assert!(needs_recenter(Rect { y: 1070, x: 100, ..w }, &[primary]));
        assert!(!needs_recenter(Rect { y: 100, x: 100, ..w }, &[primary]));
        assert!(!needs_recenter(w, &[]));
        assert!(needs_recenter(Rect { x: i32::MAX as i64, ..w }, &[primary]));
    }
}
