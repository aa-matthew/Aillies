# Features

Every user-facing capability, where it is implemented, and its state.
✅ shipped and tested · 🟡 partial or unverified on real hardware · ⬜ planned

Version: 1.0.0 (router-core-v4-pairing). Current package selection: [RELEASES.md](RELEASES.md).

Test status distinguishes automated/synthetic coverage from physical acceptance; earlier platform observations are in [the verification history](history/STATUS-2026-10-11.md).

## Using another device’s account (requesting side, “A”)

| Feature | What the user gets | Where | State |
|---|---|---|---|
| Codex routing | Codex requests through Aillies run on the selected device’s Codex login; complete replies, reasoning events and images return after successful completion (keepalive comments while waiting) | `aillies-node/src/loopback.rs` (`codex_turn`, `forward`) | ✅ Current smoke: codex-cli 0.161.0 with mock upstream; earlier live evidence in STATUS |
| Use Fast mode | Optional switch for the selected account; B must allow it first. Off sends the standard tier, on requests priority. Preferences are separate per destination | `Node::set_fast_mode`, `request::select_service_tier`; CLI `fast on/off`; home switch | ✅ Cross-device regression test; live entitlement and accounting unverified |
| Image creation tool | Codex discovers `aillies_images.generate_image` when the integration is enabled; the selected device generates the PNG and A saves it under `~/.codex/generated_images/aillies/`. Optional current-model argument; otherwise saved root model. Full PNG decoding, absolute saved path and final-reply Markdown display guidance, new creation only; editing is not exposed by this adapter | `aillies-node/src/image_mcp.rs`, `config.rs`, `/mcp/images` | 🟡 Earlier live build saved a PNG via Mac A → Windows B; v4 verified with complete mock PNG, fresh live approval/image test pending |
| Send through | Pick *This device* or any paired device; applies to the next request through Aillies | `Node::set_destination`; tray menu; home picker | ✅ |
| This device | Use your own Codex login through the same endpoint | `retry::route`, `Node::executor` | ✅ |
| Provider instead of pause | Desktop chooses Normal or Aillies; selecting Aillies explicitly enables a previously paused setup. CLI pause/resume remains available | `Node::activate_provider_routing`, provider switch; window/tray | ✅ Synthetic regression; physical acceptance pending |
| Readiness check | Connection (reachable, latency, address) and account readiness checked separately | `Node::peer_health`, peer `GET /v1/status`; CLI `check` | ✅ |
| Clear failures | Fixed messages for unreachable, sign-in expired, account changed, not approved, busy, sharing off, version mismatch, delivery uncertain | `aillies-core/src/failure.rs` | ✅ |
| Bounded retries | First attempt plus up to three retries for upstream, stream and transient transport/busy failures, on the same device/account/model; stop on cancellation, route/account change or access/local validation refusal. Generation events are buffered until success, with keepalive comments while waiting. Earlier attempts may consume usage; Codex’s own retries remain disabled | `retry.rs`, `loopback.rs`, `peer_server.rs`, `wire::TerminalScanner` | 🟡 Unit + two-node recovery/exhaustion/cancellation/account/image mock checks passed; live acceptance pending |
| Recent warnings | Latest four request/retry notices on Home, newest first; retained through successful turns, dismissal and app restarts. Fixed messages only; full latest-100 diagnostics remain exportable | `diagnostics.rs`, `NodeStatus::recent_warnings`, `app.js` | ✅ Persistence, ordering, latest-four bound, escaping and polling tests |
| Codex integration switch | Writes a backed-up provider and local image MCP server into `~/.codex/config.toml`; turning off removes only Aillies’ lines and keeps later Codex edits | `aillies-codex/src/config.rs` | ✅ (restore merge since alpha.3) |
| Existing-chat provider choice | Quit Codex, choose Normal or Aillies, reopen the same chat. Indexed and valid unindexed active/archived chats preserve original transcript records; updates include provider index, paginated offsets and a provider settings event when needed. Rollout-only legacy homes supported; rollback/recovery and background preparation with progress | `aillies-codex/src/provider_switch.rs`, `Node::switch_codex_provider`, settings selector; CLI `codex on/off --existing-chats` | 🟡 Synthetic installed Codex 0.161.0 list/resume/read verified both directions; physical desktop reopen still to check |
| Traffic diagnostics | Latest 100 incoming turns and 100 upstream attempts, independent of 100 failures. Fixed phase/category/status/timing and shared generated trace, export to Downloads | `diagnostics.rs`, provider `telemetry.rs`, Connection details → Export diagnostics | ✅ Unit + two-node mock verification; export both devices for both sides |

## Sharing your account (sharing side, “B”)

| Feature | What the user gets | Where | State |
|---|---|---|---|
| Allow Fast mode | Owner opt-in, off by default; every incoming turn is checked before upstream. Older peers that omit this capability cannot enable Fast on A | `Node::set_allow_fast_mode`, `peer_server.rs`; CLI `allow-fast on/off`; home switch | ✅ Mock two-device test, including permission revocation |
| Share this device | Approved devices can run requests on this login | `Node::set_sharing`, `peer_server.rs` | ✅ |
| Account pinning | Sharing approves the current Codex account; a different login fails closed until sharing is turned on again | `Executor::credentials` (`AccountChanged`) | ✅ |
| Login read at request time | Expired, missing or API-key logins fail before anything is sent | `aillies-codex/src/auth.rs` | ✅ |
| Concurrency limit | At most 4 requests at once; extra requests get “busy”, nothing sent | `Node::turns` semaphore | ✅ |
| Per-device revoke | Revoking a device takes effect immediately and drops live connections | `Node::revoke_client`, epoch watch | ✅ |
| Approval banner | The window opens and shows the verification code when a device asks to pair | `apps/desktop/ui/app.js` (`home`, `invite`); tray refresh | ✅ |

## Pairing

| Feature | What the user gets | Where | State |
|---|---|---|---|
| Eight-digit pairing code | Random eight digits, leading zeroes preserved; nearby discovery and authenticated encrypted exchange; 10 minutes and at most five password attempts | `protocol::pairing_digits`, `Node::create_pairing_code`, `pairing.rs`, `discovery.rs` | ✅ Local automated discovery verified; physical two-Mac permission flow pending |
| Full pairing code | `aillies1:…` alternative for private networks and older devices; one use, 10 minutes | `Invite`, `Node::create_full_invite`; CLI `invite --full` | ✅ |
| Verification code | Same three-part code on both screens (e.g. `MINT · 45 · SAGE`), bound to both certificates and the secret | `protocol::verification_code` | ✅ |
| Two-sided approval | Requester confirms codes; sharer approves; either side can cancel or decline | `Node::start_pairing`, `confirm_pairing`, `approve`, `cancel_pairing` | ✅ |
| Same network or Tailscale | Code lists LAN, Tailscale and `.local` addresses; first one presenting the right identity wins | `Node::advertised_addrs`, `peer_client::connect` | ✅ |
| Pairing across the internet | Rendezvous and relay when there is no direct path | — | ⬜ M6 |
| Nearby discovery (mDNS) | Finds devices with an active code; no code, secret or account token advertised | `discovery.rs`; `_aillies-pair._tcp.local.` | ✅ Code and both approvals remain required |

## Apps

| Feature | What the user gets | Where | State |
|---|---|---|---|
| CLI | `aillies run/status/share/fast/allow-fast/invite/pair/pending/approve/reject/use/pause/resume/devices/check/revoke/forget/codex/name/dismiss` | `crates/aillies-cli` | 🟡 Current Mac tests; Windows alpha.12 cross-built, older CLI exercised under Wine; current Windows runtime pending |
| Menu bar / tray | New A mark in green/gray/orange; provider/device menus, sharing, conditional Allow Fast, pairing, warnings, open and guarded quit | `apps/desktop/src-tauri/src/lib.rs` | 🟡 Current Mac unit/mock preview; current native tray acceptance on each platform pending |
| Compact window | Welcome, home, pair (code entry first, secondary show code), verify, readiness, connection details, devices, Codex switch, error and approval banners, dark mode | `apps/desktop/ui` | 🟡 Same as above |
| Survives window problems | Service and tray start before the window; a broken WebView doesn’t stop routing | `show_window` (alpha.2) | 🟡 Earlier Wine service observation; current WebView2 failure acceptance pending |
| Single instance | Launching again opens the running app’s window | `tauri-plugin-single-instance` | 🟡 Windows/macOS behavior unverified |
| Request diagnostics | Latest 100 request errors/warnings, persisted across restarts; Connection details → Export diagnostics or `aillies diagnostics` saves a private JSON file to Downloads. Fixed codes and allowlisted metadata only | `aillies-node/src/diagnostics.rs`, Node export, control API, desktop IPC | ✅ Unit/two-node tests and earlier physical Windows export; current build acceptance pending |
| Startup log | `desktop.log` accepts only typed lifecycle events and public version/port values; raw errors and panic payloads excluded | `log_line` | ✅ |
| Installers | Alpha.12 Apple Silicon DMG and Windows x64 NSIS; Linux packaging via Tauri/CI | `scripts/`, `release.yml`; [artifact list](RELEASES.md) | 🟡 Mac/Windows payloads verified; unsigned, Windows physical install pending |
| Autostart, auto-update | — | — | ⬜ M8 |

## Platform support

| | macOS 13+ | Windows 10/11 x64 | Ubuntu 22.04+ |
|---|---|---|---|
| CLI | ✅ automated tests/build | 🟡 cross-build history; alpha.12 runtime pending | 🟡 earlier tests; current hardware pending |
| Desktop app | 🟡 alpha.12 package/unit/mock checks; native acceptance pending | 🟡 alpha.12 installer verified; physical acceptance pending | 🟡 earlier build; current acceptance pending |
| Secrets in OS store | ⬜ M7 | ⬜ M7 | ⬜ M7 |

## Providers

| Provider | State |
|---|---|
| Codex (ChatGPT login) | ✅ |
| Claude | ⬜ M9. `Provider::Claude` is reserved in the protocol; see ARCHITECTURE “Adding Claude” |

## Current upstream compatibility

Both devices use the v4 request formatter/session resolver and pinned `codex_cli_rs/0.159.0` identity. Model/effort and Lite/ordinary behavior are covered by the pinned 9router fixtures; current live model/backend acceptance is not implied. See [UPSTREAM-COMPATIBILITY.md](UPSTREAM-COMPATIBILITY.md). V2/v3 formatter and marker descriptions are [historical](history/FEATURES-2026-10-11.md).

Embedded default instructions are checked out with LF endings on every platform, preserving fixture-identical request bytes even when Windows enables automatic CRLF conversion.

Native OpenAI `image_gen`, image editing and native image endpoint forwarding are separate from the explicit `aillies_images.generate_image` MCP adapter and remain unverified/unsupported by that adapter. Normal Codex approvals remain required.

## macOS quit, switch and reopen (since alpha.8)

The provider selector offers a compact review when Codex is still running, followed by **Quit, switch & reopen** or Cancel. It names the apps that will close (including an editor when it owns Codex) and terminal count. Finish work/save editor changes first. Aillies quits normally, verifies shutdown, runs the history-preserving transaction and opens only ChatGPT. Editors and terminal Codex sessions stay closed. A previously quit ChatGPT is found in standard Applications locations. The chosen provider stays selected while awaiting confirmation or switching; Cancel/failure restores the actual provider. No unfinished action is resumed automatically. Unknown clients refuse; recovery failures keep Codex closed. Windows/Linux still use manual quit/reopen. Implementation: `aillies-codex/src/restart.rs` and `restart/macos.rs`, Node plan/execute methods, thin desktop IPC, `app.js`.

Existing-chat switching includes indexed same-provider rollout copies without deleting/merging history or applying canonical paginated offsets twice. Older provider-setting events without thread IDs are supported within their validated rollout. Conflicting copies or ambiguous unindexed histories still stop safely.

Alpha.9 removes the two-second history preparation cutoff. The provider row shows scanning, preparation counts and saving status while the existing background worker runs. The provider choice stays pending until commit; errors remain visible. Clients must stay closed, and transactional rollback/recovery, record bounds and path/schema checks remain.

## Current provider and pairing controls

Home and the tray replace routing pause/resume with the existing transactional Normal ↔ Aillies provider switch. Use Aillies with groups Codex and disabled Claude Code — Upcoming. Selecting Aillies explicitly resumes a previously paused connection; selecting Normal leaves peer sharing independent. Allow Fast mode appears only while Share this device is on. Pair a device goes directly to eight-digit entry for nearby devices, with a secondary full-code alternative. The encrypted bootstrap leads into matching-code approval on both devices; OpenAI tokens stay home. Tray provider actions open the same pending selection/restart review, and Quit requires Normal before stopping the node. The user-supplied silver A and wordmark are preserved in assets/brand/source, with derived platform icons generated by scripts/brand-icons.mjs. Physical native tray/restart and fresh live image acceptance remain unverified.
