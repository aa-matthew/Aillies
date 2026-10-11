# Handoff for Codex — 2026-10-11

Shared rules are in [AGENTS.md](../AGENTS.md); read it first. This file is a point-in-time handoff and contains no credentials, keys, `auth.json` contents or user data. **UNVERIFIED** marks anything not proven by a test or a direct observation.

## State at handoff

- Version 0.9.0-alpha.3 plus unreleased fixes (see [CHANGELOG](../CHANGELOG.md)).
- Your review’s three findings were confirmed by failing tests and are now fixed:

| # | Finding | Confirmed by (failed before the fix) | Fix |
|---|---|---|---|
| 1 | `upstream_override` accepted `http://127.0.0.1:80@example.com/…` | [`override_rejects_userinfo_and_lookalike_hosts`](../crates/aillies-codex/src/executor.rs#L279) | Parse with `reqwest::Url`: `http`, loopback host, explicit non-default port, no user info or fragment ([DECISIONS #14](DECISIONS.md)) |
| 2 | `TerminalScanner` treated any `response.completed` text as completion | [`scanner_ignores_terminal_names_inside_content`](../crates/aillies-core/src/wire.rs#L180), [`scanner_terminal_by_data_type_crlf_and_byte_fragments`](../crates/aillies-core/src/wire.rs#L197), and end to end in [`e2e.rs` “Regression: a stream that ends early”](../crates/aillies-node/tests/e2e.rs#L215) | Incremental SSE parser using the `event:` field or the top-level `type`; 4 KiB line cap ([`scanner_handles_huge_terminal_payloads`](../crates/aillies-core/src/wire.rs#L210)) ([DECISIONS #15](DECISIONS.md)) |
| 3 | `make test lint` skipped the desktop shell | `Makefile` and CI inspected | `make test` = core + desktop; `make lint` = fmt + clippy for both + UI JS check; CI desktop job now runs fmt, clippy, tests and build; 3 desktop unit tests added ([DECISIONS #16](DECISIONS.md)) |

- Checks (Linux container, 2026-10-11): 41 tests pass (core 18, codex 18, two-node e2e 2, desktop 3); fmt and clippy `-D warnings` clean for the workspace and desktop; `node --check` clean for the UI; `scripts/smoke.sh` passes with real codex-cli 0.162.1 and a mock backend. **Not yet rerun on the Mac.**

## Unresolved bugs and reproduction

| # | Bug | Reproduce | Status |
|---|---|---|---|
| B1 | Under Wine, creating the window blocks the main thread when WebView2 is missing, so the tray menu stops responding (the node keeps serving) | `scripts/build-windows-from-linux.sh` → `WINEPREFIX=$PWD/wpx wine Aillies.exe` under `xvfb-run`. `desktop.log` shows `node started` but never `window created` or `window failed`; `curl 127.0.0.1:18522/healthz` answers | Verified under Wine. **UNVERIFIED** on real Windows without WebView2 (might fail fast instead) |
| B2 | Linux without a D-Bus session: single-instance doesn’t work, so a second launch runs setup, fails to bind 18522 and opens a “couldn’t start” window | `xvfb-run` with no `dbus-launch`; start `aillies-desktop` twice | Verified in container. Normal desktops have D-Bus. macOS/Windows **UNVERIFIED** |
| B3 | A friend’s Windows PC: alpha.1 installed, launching showed nothing | Unknown. alpha.2 adds `desktop.log` at `%APPDATA%\anyax\Aillies\config` | Root cause **UNVERIFIED**; alpha.2/alpha.3 not yet reported back |
| B4 | Cosmetic: with no TOML tables in `config.toml`, install appends root keys after the trailing blank line | Install into a file containing only `model = "x"\n`, then inspect the output | Valid TOML; low priority |
| B5 | UX: users read gray “Using this device” as broken, and Pause blocks *This device* too (one user hit `routing_paused` in Codex) | Pause in the tray, then send from Codex | Matches spec ([DECISIONS #22–23](DECISIONS.md)); needs a product call |
| B6 | Pairing confirm waits up to 180 s with no cancel button in the window | Start pairing in the window, then don’t approve on B | Minor |

Environment, not code: on the user’s Mac the repo branch is still `master` (GitHub target is `main`), and `.git/stale-*.lock` files are left from sessions without delete permission. Both are safe to clean.

## Failed approaches (don’t repeat)

| Tried | Why it failed | What works |
|---|---|---|
| Prefix check for the upstream override | Accepts user-info tricks (finding 1) | Parse the URL |
| Substring search for terminal events | Content can contain event names (finding 2) | SSE parser |
| Byte-equality guard on Codex restore | The Codex app edits `config.toml` itself (trusted projects, notices), so restore always refused | Structural `merge_restore` ([config.rs](../crates/aillies-codex/src/config.rs)) |
| `diffy` three-way merge for restore | Codex appends at EOF next to Aillies’ appended block, which conflicts every time | Structural removal of Aillies’ own lines |
| `rustup target add` for macOS/Windows cross-builds | `static.rust-lang.org` blocked from the build sandbox | Ubuntu `rustc-1.91` + `rust-1.91-src` + `-Zbuild-std` + mingw (Windows); build macOS on a Mac |
| Ubuntu std sources as-is for Windows | Debian strips the `windows-targets` dependency and ships no `rsbegin.o`/`rsend.o` | Patch `std/Cargo.toml`; compile `rtstartup` objects ([script](../scripts/build-windows-from-linux.sh)) |
| `cargo tauri build --target x86_64-pc-windows-gnu` | tauri-cli asks rustup for installed targets, which linked toolchains can’t answer | Build with cargo, then `cargo tauri bundle` with a rustup shim |
| WebView2 `embedBootstrapper` | Download blocked during bundling | Default download bootstrapper |
| Testing the NSIS installer under wine32 | Silent install produced nothing | Inspect the payload with `7z l`; test on real Windows |
| Swift-era tool allowlist (`function`, `custom`, `image_generation`) | codex-cli 0.162.1 also sends `tool_search` and `web_search`, so every real turn got `request_invalid` | Allowlist from captured real requests ([DECISIONS #6](DECISIONS.md)) |
| `pkill -f "aillies run"` in scripts | Matches the invoking shell, which then gets killed | `pgrep -x aillies` |
| `codex exec` in scripts without stdin redirect | Hangs waiting for stdin | `< /dev/null` |
| Installing Rust on the user’s Mac via the script | `~/.cargo` and `~/.rustup` were root-owned, so rustup looped on renames | Script reuses Rust 1.87+ and fixes ownership with `sudo chown` |

## Assumptions now recorded

Previously only implicit; added to [DECISIONS.md](DECISIONS.md) #14–25:

- model default (#17)
- Codex client-identity headers `0.159.0` (#18, **UNVERIFIED** live)
- no system proxy upstream (#19)
- LAN addresses in invites (#20)
- invite turns sharing on, and first pairing auto-selects (#21)
- Pause blocks all destinations (#22)
- gray for this device (#23)
- desktop lifecycle (#24)
- per-user Windows installer (#25)

## Mocks vs real

| Behavior | Mock / simulated | Real |
|---|---|---|
| Codex request shape accepted, streaming, error display, no retry | Mock OpenAI backend | **Real codex-cli 0.162.1** (container) |
| Real OpenAI backend, account entitlement, quota accounting, Lite header, image generation | Mock only | **Never tested** |
| Two separate physical devices over LAN/Tailscale | Two nodes on one host via loopback | **Never tested** (milestone M5) |
| Pairing, verification code, revoke, sharing off, expired/changed login, mid-stream and early-end drops | e2e test + smoke script | Not on real devices |
| macOS desktop app | — | User’s Mac: app launches, window renders, Codex switch writes config, real Codex app reached the loopback endpoint (`routing_paused` shown) |
| macOS menu-bar icon colors, single instance | Unit tests only (icons decode) | **UNVERIFIED** |
| Windows CLI | Wine: pairs with a Linux node, routes a mock turn, `codex on` | **Never on real Windows** |
| Windows desktop app | Wine: node and tray start, window blocks (B1) | alpha.1 on a real PC failed silently (B3) |
| Linux desktop app | Xvfb: node, IPC and window render | No real desktop session |
| Codex config restore after Codex edits | Synthetic edits in unit tests | Not with a config the real Codex app edited |

## Recommended next task: close the invariant test gaps

Every invariant in AGENTS.md should have a test that fails if it breaks. This needs no hardware.

**Acceptance criteria:**

1. Add tests for each “missing” row below, in the module or `tests/e2e.rs` as noted.
2. Each new test fails when the guarded check is removed. Show this once per test by temporarily disabling the check, and note it in the PR.
3. `make test lint` and `make smoke` pass on macOS; CI is green on all three OSes.
4. `docs/STATUS.md` verification log is updated; no behavior change, no new dependencies unless justified in DECISIONS.

After that, the next milestone is **M5: live acceptance on two real devices** (human-run checklist in [STATUS.md](STATUS.md)). Its pass criteria: a text and an image turn from Codex on A, B’s usage moves and A’s doesn’t, plus the offline, sign-out and revoke cases behave as listed.

## Invariant coverage

| Invariant (AGENTS.md) | Existing coverage | Missing |
|---|---|---|
| 1 No fallback | [e2e sharing off / A has no login](../crates/aillies-node/tests/e2e.rs#L177), [routing paused](../crates/aillies-node/tests/e2e.rs#L170) | Destination peer removed mid-session → `no_destination` (`Node::remove_peer` + turn) |
| 2 No replay | [mid-stream drop](../crates/aillies-node/tests/e2e.rs#L201), [early end](../crates/aillies-node/tests/e2e.rs#L215), hit counts throughout `e2e.rs`; [Codex retries 0](../crates/aillies-codex/src/config.rs#L542) | Peer connection reset after the request is written → `delivery_uncertain`, one attempt (`loopback::forward` send error path) |
| 3 Credentials stay home | Upstream sees B’s account ([e2e](../crates/aillies-node/tests/e2e.rs#L115)); A without a login still works | Assert no bytes of B’s access token appear in anything A receives (responses, `/control/status`, peer status) |
| 4 No content in logs or errors | [envelope has no upstream body](../crates/aillies-codex/src/executor.rs#L251), [envelope bounds](../crates/aillies-core/src/failure.rs#L166) | Capture `tracing` output during an e2e turn and assert the prompt, output and token are absent; same for `desktop.log` |
| 5 Fingerprint identity, two-sided pairing | [one-time invite, code match, approve](../crates/aillies-node/tests/e2e.rs#L115), [reject](../crates/aillies-node/tests/e2e.rs#L244), [code binds both sides](../crates/aillies-core/src/protocol.rs#L221), [revoke](../crates/aillies-node/tests/e2e.rs#L228) | Pinned TLS rejects a different server cert at a known address; cancel withdraws approval (`pair_cancel`); expired invite (TTL); `protocol_mismatch` header (426) |
| 6 Transactional Codex config | [round trip and conflict](../crates/aillies-codex/src/config.rs#L574), [keep Codex edits](../crates/aillies-codex/src/config.rs#L592), [refuse unparseable](../crates/aillies-codex/src/config.rs#L617), [ambiguous configs](../crates/aillies-codex/src/config.rs#L563) | Refuse a symlinked `config.toml`; install failure after writing the receipt removes the receipt |
| 7 Security logic in crates | Structural (shells call `Node`) | A lint or review rule only; no test practical |
| 8 Loopback stays loopback | [override parsing](../crates/aillies-codex/src/executor.rs#L279) | Bad `Host` header → 421; `/control/*` with the Codex key → 401; `/v1/responses` with the control token → 401 |
| 9 No ChatGPT desktop claim | Docs only | n/a |
| Limits | — | Peer busy at 5 concurrent turns → `peer_busy` 429 with no upstream call; request over 16 MiB → 400 |
| UI escaping | — | Peer name `<img onerror>` renders inert (`esc()` in `apps/desktop/ui/app.js`), via Playwright on `index.html` + `mock.js` |
