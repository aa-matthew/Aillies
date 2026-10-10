# Aillies status

The single place to track progress. Update it in the same commit as the work.

**Version:** 0.9.0-alpha.2 · **Last verified:** 2026-10-10 (Linux container, see the log below)

## Milestones

| # | Milestone | State |
|---|---|---|
| M1 | Shared Rust core: request validation, SSE detection, failure codes, protocol, verification code | ✅ Done |
| M2 | Codex provider: login file reader, streaming executor, transactional Codex config install/restore | ✅ Done |
| M3 | Node: identity, two-sided pairing, pinned mutual TLS, loopback + control API, `aillies` CLI | ✅ Done |
| M4 | Desktop tray app (Tauri 2) with the compact window | 🟡 Runs on Linux; Windows installer built (not yet run on real Windows); macOS build pending |
| M5 | Live acceptance on two real devices with real Codex logins | ⬜ Next |
| M6 | Pairing and routing without a shared network (rendezvous + blind relay) | ⬜ Planned |
| M7 | Secrets in Keychain / Credential Manager / Secret Service | ⬜ Planned |
| M8 | Signed installers, notarization, auto-update, autostart | ⬜ Planned |
| M9 | Claude provider | ⬜ Later. `Provider::Claude` is already reserved in the protocol |

## M5 acceptance checklist (do this next)

Run on two real machines (for example Matthew’s Mac = A, Dailyngocss = B). Record results in the log.

- [ ] B: `aillies run`, `aillies invite`; A: `aillies run`, `aillies pair …`; codes match on both screens
- [ ] A: `aillies check` shows *reachable* and *Codex account ready*
- [ ] A: `aillies codex on`, restart Codex, new chat: a text reply arrives
- [ ] Image generation from Codex on A completes
- [ ] B’s Codex usage changes, A’s doesn’t
- [ ] Stop `aillies` on B: Codex on A shows “selected device is unreachable. Nothing was sent.” once
- [ ] Sign out of Codex on B: A shows the sign-in message; nothing is sent from A’s account
- [ ] `aillies revoke` on B: A is refused immediately
- [ ] `aillies codex off` on A restores the original `~/.codex/config.toml` byte for byte
- [ ] Desktop app: tray status colors, switching destination, pairing from the window, approve banner on B

## Verification log

| Date | Where | What | Result |
|---|---|---|---|
| 2026-10-10 | Linux container | `cargo test --workspace`: 32 tests (core 15, codex 15, two-node end-to-end 2) | ✅ Pass |
| 2026-10-10 | Linux container | Two `aillies run` daemons + mock OpenAI backend: CLI pairing with verification code, `aillies check` | ✅ Pass |
| 2026-10-10 | Linux container | Real `codex exec` (codex-cli 0.162.1) on A, which has no login → B → mock backend with B’s account | ✅ Reply received, one upstream call, B’s account id used |
| 2026-10-10 | Linux container | Sharing off on B → Codex on A shows the Aillies error, exactly one attempt, no retry | ✅ Pass |
| 2026-10-10 | Linux → Windows cross-build | `aillies.exe` (x64) and NSIS installer `Aillies_0.9.0-alpha.1_x64-setup.exe` built with `scripts/build-windows-from-linux.sh` | ✅ Built |
| 2026-10-10 | Wine (Linux) | Windows `aillies.exe` as A paired with a Linux node B, routed a turn using B's account, `codex on` wrote config | ✅ Pass |
| 2026-10-10 | Linux container | Desktop app `cargo build` (Tauri 2, webkit2gtk 4.1); window UI rendered in Chromium with the preview mock | ✅ Pass |

Not yet proven: the Windows desktop app on real Windows (needs WebView2), live OpenAI entitlement and quota accounting, the macOS/Windows desktop builds, NAT traversal, behavior after a Codex backend change.

## Known gaps and risks

- **Same network required.** Invites list LAN, Tailscale and `.local` addresses. Without one, pairing fails (M6).
- **Secrets are 0600 files** in the Aillies home folder, not the OS credential store (M7).
- **Codex backend drift.** Aillies follows Codex’s private ChatGPT backend contract (headers, the Responses Lite shape, allowed tool types). If Codex changes it, requests fail closed with `request_invalid` or `upstream_rejected`. Recheck with `scripts/smoke.sh` after Codex updates.
- **Token refresh** is left to Codex on the sharing device. If B doesn’t open Codex for a long time its login can expire; A then sees the sign-in message.
- **One node per device.** The CLI daemon and the desktop app both bind 127.0.0.1:18522; run one.
- **Windows desktop not yet seen running.** First report: “installed, nothing happened.” alpha.2 starts the service and tray before the window, single-instance, and writes `desktop.log` (`%APPDATA%\\anyax\\Aillies\\config`). Waiting on that log.
- **Unsigned builds.** macOS will ask the user to allow the app in Privacy & Security until M8.
