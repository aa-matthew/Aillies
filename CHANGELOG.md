# Changelog

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
