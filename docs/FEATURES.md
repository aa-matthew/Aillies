# Features

Every user-facing capability, where it is implemented, and its state.
✅ shipped and tested · 🟡 partial or unverified on real hardware · ⬜ planned

Version: 0.9.0-alpha.3

## Using another device’s account (requesting side, “A”)

| Feature | What the user gets | Where | State |
|---|---|---|---|
| Codex routing | New Codex chats run on the selected device’s Codex login; replies, reasoning events and images stream back | `aillies-node/src/loopback.rs` (`codex_turn`, `forward`) | ✅ Verified with codex-cli 0.162.1 and a mock backend |
| Send through | Pick *This device* or any paired device; applies to new chats | `Node::set_destination`; tray menu; home picker | ✅ |
| This device | Use your own Codex login through the same endpoint | `Node::local_executor` | ✅ |
| Pause / resume routing | One switch stops every request; Codex shows “Routing is paused … Nothing was sent.” | `Node::set_routing`; tray; CLI `pause`/`resume` | ✅ |
| Readiness check | Connection (reachable, latency, address) and account readiness checked separately | `Node::peer_health`, peer `GET /v1/status`; CLI `check` | ✅ |
| Clear failures | Fixed messages for unreachable, sign-in expired, account changed, not approved, busy, sharing off, version mismatch, delivery uncertain | `aillies-core/src/failure.rs` | ✅ |
| No replay | A request is sent once. A cut stream is reported as *may have run*, never resent; Codex retries are disabled | `loopback::forward`, executor, Codex provider settings | ✅ |
| Codex integration switch | Writes a backed-up provider into `~/.codex/config.toml`; turning off removes only Aillies’ lines and keeps later Codex edits | `aillies-codex/src/config.rs` | ✅ (restore merge since alpha.3) |

## Sharing your account (sharing side, “B”)

| Feature | What the user gets | Where | State |
|---|---|---|---|
| Share this device | Approved devices can run requests on this login | `Node::set_sharing`, `peer_server.rs` | ✅ |
| Account pinning | Sharing approves the current Codex account; a different login fails closed until sharing is turned on again | `Executor::credentials` (`AccountChanged`) | ✅ |
| Login read at request time | Expired, missing or API-key logins fail before anything is sent | `aillies-codex/src/auth.rs` | ✅ |
| Concurrency limit | At most 4 requests at once; extra requests get “busy”, nothing sent | `Node::turns` semaphore | ✅ |
| Per-device revoke | Revoking a device takes effect immediately and drops live connections | `Node::revoke_client`, epoch watch | ✅ |
| Approval banner | The window opens and shows the verification code when a device asks to pair | `apps/desktop/ui/app.js` (`home`, `invite`); tray refresh | ✅ |

## Pairing

| Feature | What the user gets | Where | State |
|---|---|---|---|
| One-time pairing code | `aillies1:…` code that works once and expires in 10 minutes | `aillies-core/src/protocol.rs` (`Invite`), `Node::create_invite` | ✅ |
| Verification code | Same three-part code on both screens (e.g. `MINT · 45 · SAGE`), bound to both certificates and the secret | `protocol::verification_code` | ✅ |
| Two-sided approval | Requester confirms codes; sharer approves; either side can cancel or decline | `Node::start_pairing`, `confirm_pairing`, `approve`, `cancel_pairing` | ✅ |
| Same network or Tailscale | Code lists LAN, Tailscale and `.local` addresses; first one presenting the right identity wins | `Node::advertised_addrs`, `peer_client::connect` | ✅ |
| Short codes, pairing across the internet | 6-digit code via rendezvous; relay when there is no direct path | — | ⬜ M6 |
| Nearby discovery (mDNS) | Devices on the LAN appear without a code | — | ⬜ |

## Apps

| Feature | What the user gets | Where | State |
|---|---|---|---|
| CLI | `aillies run/status/share/invite/pair/pending/approve/reject/use/pause/resume/devices/check/revoke/forget/codex/name/dismiss` | `crates/aillies-cli` | ✅ macOS/Linux; ✅ Windows under Wine |
| Menu bar / tray | Status icon (green/gray/orange), destination list, pause, open, quit | `apps/desktop/src-tauri/src/lib.rs` | 🟡 Linux verified; macOS and Windows unverified on hardware |
| Compact window | Welcome, home, pair (enter or show code), verify, readiness, connection details, devices, Codex switch, error and approval banners, dark mode | `apps/desktop/ui` | 🟡 Same as above |
| Survives window problems | Service and tray start before the window; a broken WebView doesn’t stop routing | `show_window` (alpha.2) | ✅ Verified under Wine |
| Single instance | Launching again opens the running app’s window | `tauri-plugin-single-instance` | 🟡 Windows/macOS behavior unverified |
| Startup log | `desktop.log` with lifecycle events and errors only | `log_line` | ✅ |
| Installers | macOS build script → Aillies.app; Windows NSIS setup (current user); Linux .deb/AppImage via `cargo tauri build` | `scripts/`, `release.yml` | 🟡 Unsigned |
| Autostart, auto-update | — | — | ⬜ M8 |

## Platform support

| | macOS 13+ | Windows 10/11 x64 | Ubuntu 22.04+ |
|---|---|---|---|
| CLI | ✅ build script | ✅ cross-built | ✅ |
| Desktop app | 🟡 build script, not yet confirmed | 🟡 installer, not yet confirmed | ✅ |
| Secrets in OS store | ⬜ M7 | ⬜ M7 | ⬜ M7 |

## Providers

| Provider | State |
|---|---|
| Codex (ChatGPT login) | ✅ |
| Claude | ⬜ M9. `Provider::Claude` is reserved in the protocol; see ARCHITECTURE “Adding Claude” |
