# Changelog

## 0.9.0-alpha.3 — 2026-10-10

- Turning off "Use Aillies with Codex" now works after Codex has changed its own settings (trusted folders, dismissed warnings, a different model). Aillies removes only its own lines and keeps everything else. It still refuses, without changing anything, if the file can't be read safely.
- The "routing paused" error in Codex now says how to turn routing back on.

## 0.9.0-alpha.2 — 2026-10-10

- Desktop: Aillies starts its background service and tray icon before opening the window, so routing works even if the window can't open (for example when Microsoft Edge WebView2 is missing). It then shows a tray tooltip and opens the WebView2 download page.
- Desktop: opening Aillies again (Start menu, desktop icon) brings up the running app's window instead of starting a second copy.
- Desktop: writes a startup log, `desktop.log`, in the Aillies folder (`%APPDATA%\anyax\Aillies\config` on Windows). It holds lifecycle events and errors only, never content.
- Windows installer installs for the current user without an admin prompt.

## 0.9.0-alpha.1 — 2026-10-10

First Aillies build (formerly Peer Route).

- New cross-platform Rust core, Codex provider and device node; the Swift app moved to `legacy/`.
- `aillies` CLI: `run`, `invite`, `pair`, `approve`, `use`, `pause`, `check`, `revoke`, `codex on|off`, and more.
- Pairing with a one-time code and a three-part verification code; approval on both devices.
- Per-device identities over pinned mutual TLS 1.3; per-device revocation, effective immediately.
- Codex integration writes a backed-up provider with retries disabled; restore refuses to overwrite later edits.
- Accepts the tools current Codex sends (`tool_search`, `web_search`), so real Codex sessions work.
- Tauri tray app with the compact window: destination switcher, routing and sharing switches, pairing, approval banner, devices, Codex setting.
- New name and logo.
