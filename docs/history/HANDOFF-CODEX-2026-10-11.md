> Historical snapshot before the 2026-10-11 cleanup. Later decisions and verification supersede earlier statements. Use [current HANDOFF-CODEX](../HANDOFF-CODEX.md) for operative guidance.

# Handoff for Codex — 2026-10-11

Shared rules are in [AGENTS.md](../../AGENTS.md); read it first. This file is a point-in-time handoff and contains no credentials, keys, `auth.json` contents or user data. **UNVERIFIED** marks anything not proven by a test or a direct observation.

## Latest alpha.12 work — 2026-10-11

Eight-digit pairing is implemented in pairing.rs/discovery.rs and Node; decision 47 describes the protocol. mDNS advertises only public metadata while a code is active. SPAKE2 binds both TLS identities; AES-GCM seals the original full invite, then `/v1/pair` consumes its random secret from the same bootstrap requester fingerprint. Approval on both devices remains. Codes are 10-minute, five-execution, in-memory values; never log or advertise codes, hashes, secrets or OAuth credentials. Stored certificates remain the access mechanism. Do not replace this with plaintext short-code comparison, a reusable bearer token or an OpenAI-token exchange.

Default CLI `invite` and desktop show eight digits; `invite --full` and secondary full-code entry/generation preserve private-network/old-device compatibility. Numeric pairing requires alpha.12 on both devices and LAN multicast. `numeric_pairing_via_local_discovery` is opt-in for multicast environments and passed on this Mac using synthetic homes/logins without live inference. Default e2e injects candidates into the same crypto/approval flow, including wrong password/certificate, consumed-code reuse, sharing-off invalidation and full-code compatibility. Physical two-Mac Local Network permission and Windows firewall acceptance remain unverified. Bundled Info.plist must include the purpose text and `_aillies-pair._tcp`; package verification results follow in STATUS.

## Alpha.11 work — 2026-10-11

Provider-first desktop controls and supplied brand are implemented; see decision 46 and STATUS alpha.11. No desktop routing toggle; CLI pause remains. Explicit Aillies activation resumes a paused gate. Tray provider actions use a queued window intent and the existing transaction/restart review. Quit refuses while Aillies is installed or a switch owns the guard, then offers Normal first. Sharing/Fast visibility, direct pairing entry, both-device approval and Claude Upcoming follow the approved wireframes. Source PNGs and reproducible brand-icon script are included. No real clients closed, real history edited or live inference sent during this change. Final checks/package evidence are in STATUS.

## Follow-up on the Mac — 2026-10-11

- `make test lint`: **65 tests** (20 core, 24 codex, 3 node unit, 14 e2e, 4 desktop), workspace and desktop lint/format, and UI syntax. `make smoke` passes with real codex-cli 0.161.0 and the mock. macOS `.app` bundled, installed and running at `~/Applications/Aillies.app`; a new live desktop text request through B completed without test content/local keys in its log. CodeGraph synced and impact/affected results checked against source.
- Config restore now parses TOML values before preserving statement formatting. Regression tests first reproduced the single-quote/tab/comment problem and invalid TOML acceptance. Symlink and failed-write receipt cleanup have tests.
- Forgetting the selected peer previously selected the local account. A failing test reproduced it; now the stale destination fails `no_destination` until the user explicitly chooses another destination.
- B has a default-off **Allow Fast mode** switch; A has default-off **Use Fast mode**, separate per destination. Every incoming peer request enforces B's permission. Old peers omit the capability and cannot enable Fast on A. Mock coverage includes rejection before upstream and revocation without downgrade/replay. Live Fast entitlement/accounting remains unverified.
- Added authenticated loopback MCP `aillies_images.generate_image` to Codex integration. It calls the existing route once and saves a private PNG locally. Creation only; no native image-edit endpoint support. Old installations need integration off/on and a new Codex process/chat. This Mac's config has already been refreshed with a private backup.
- **Live Mac A → Windows B passed:** existing pinned pairing, text request, direct hosted-image stream, new MCP image call, and real codex-cli discovering/invoking the MCP tool and saving a PNG. Images arrive in `response.output_item.done`; the completed output list was empty. Account tokens were never copied. Usage accounting was not observed.
- Codex CLI discovery required a trusted workspace. Its default `never` approval policy blocked this mutating tool until the isolated test configuration explicitly approved that one tool. Production installation leaves Codex approval settings alone; interactive users approve the tool if asked.
- Desktop logging accepts only typed lifecycle events, public version and port; raw errors/panic payloads no longer enter `desktop.log`. Its output contract has a capture test. Tracing is captured during real simulated request/refusal paths and checked for prompts, replies and credentials.
- Browser preview verified both Fast switches and inert `<img onerror>` peer names. This was a live Playwright check, not a new automated browser test in CI.
- B has no remote administration access. The updated Windows build and live offline/sign-out/revoke/accounting/Fast checks remain manual. [UPDATE-DEVICES.md](../UPDATE-DEVICES.md) has concrete steps and a source-only update archive.
- **Not claimed:** green three-OS CI for these uncommitted changes, physical B update, complete desktop acceptance, or mutation runs for every added invariant test. Failing-before-fix evidence exists for the restore/provider-removal/image-discovery regressions; other new invariant cases were run against the guarded implementation.

The earlier handoff below is preserved as historical evidence. Its “never tested” and “missing” cells are superseded by this follow-up and the coverage table below.

## Current invariant coverage

| Invariant | Added evidence | Remaining limit |
|---|---|---|
| No fallback | `forgetting_selected_peer_never_selects_local_account`; sharing-off and image-route refusals | Physical offline/sign-out cases on B |
| No replay | `reset_after_request_is_written_is_uncertain_and_never_replayed`; live revocation and previous stream-drop hit counts | Physical network reset |
| Credentials stay home | `status_and_response_never_expose_sharer_credentials`, B account assertions in text/image routes | No physical packet-capture claim |
| No content in logs/errors | `tracing_never_contains_request_response_or_credentials`; desktop `lifecycle_log_accepts_only_fixed_events_and_public_port`; fixed upstream failure tests | Live desktop text log checked; broader desktop acceptance remains |
| Fingerprint and pairing | Wrong certificate, protocol mismatch, cancellation, expired invite tests | Fresh physical pairing/revocation acceptance |
| Config transaction | Quoting/comments, invalid TOML, later edits, MCP ownership, symlink refusal, failed receipt cleanup; actual Mac config refresh | Concurrent editor crash/race stress |
| Shells thin | Fast commands call Node; image route lives in node crate | Review constraint |
| Loopback boundaries | `loopback_host_and_token_boundaries`; image endpoint tests wrong key/control key/browser Origin | Browser-origin check applies additionally to MCP |
| No official ChatGPT desktop claim | Documentation constraint unchanged | n/a |
| Limits and escaping | Fifth live request, oversized body, Playwright malicious peer name | Browser test is manual, not CI |

## State at handoff

- Version 0.9.0-alpha.3 plus unreleased fixes (see [CHANGELOG](../../CHANGELOG.md)).
- Your review’s three findings were confirmed by failing tests and are now fixed:

| # | Finding | Confirmed by (failed before the fix) | Fix |
|---|---|---|---|
| 1 | `upstream_override` accepted `http://127.0.0.1:80@example.com/…` | [`override_rejects_userinfo_and_lookalike_hosts`](../../crates/aillies-codex/src/executor.rs#L279) | Parse with `reqwest::Url`: `http`, loopback host, explicit non-default port, no user info or fragment ([DECISIONS #14](../DECISIONS.md)) |
| 2 | `TerminalScanner` treated any `response.completed` text as completion | [`scanner_ignores_terminal_names_inside_content`](../../crates/aillies-core/src/wire.rs#L180), [`scanner_terminal_by_data_type_crlf_and_byte_fragments`](../../crates/aillies-core/src/wire.rs#L197), and end to end in [`e2e.rs` “Regression: a stream that ends early”](../../crates/aillies-node/tests/e2e.rs#L215) | Incremental SSE parser using the `event:` field or the top-level `type`; 4 KiB line cap ([`scanner_handles_huge_terminal_payloads`](../../crates/aillies-core/src/wire.rs#L210)) ([DECISIONS #15](../DECISIONS.md)) |
| 3 | `make test lint` skipped the desktop shell | `Makefile` and CI inspected | `make test` = core + desktop; `make lint` = fmt + clippy for both + UI JS check; CI desktop job now runs fmt, clippy, tests and build; 3 desktop unit tests added ([DECISIONS #16](../DECISIONS.md)) |

- Checks (Linux container, 2026-10-11): 41 tests pass (core 18, codex 18, two-node e2e 2, desktop 3); fmt and clippy `-D warnings` clean for the workspace and desktop; `node --check` clean for the UI; `scripts/smoke.sh` passes with real codex-cli 0.162.1 and a mock backend. **Not yet rerun on the Mac.**

## Unresolved bugs and reproduction

| # | Bug | Reproduce | Status |
|---|---|---|---|
| B1 | Under Wine, creating the window blocks the main thread when WebView2 is missing, so the tray menu stops responding (the node keeps serving) | `scripts/build-windows-from-linux.sh` → `WINEPREFIX=$PWD/wpx wine Aillies.exe` under `xvfb-run`. `desktop.log` shows `node started` but never `window created` or `window failed`; `curl 127.0.0.1:18522/healthz` answers | Verified under Wine. **UNVERIFIED** on real Windows without WebView2 (might fail fast instead) |
| B2 | Linux without a D-Bus session: single-instance doesn’t work, so a second launch runs setup, fails to bind 18522 and opens a “couldn’t start” window | `xvfb-run` with no `dbus-launch`; start `aillies-desktop` twice | Verified in container. Normal desktops have D-Bus. macOS/Windows **UNVERIFIED** |
| B3 | A friend’s Windows PC: alpha.1 installed, launching showed nothing | Unknown. alpha.2 adds `desktop.log` at `%APPDATA%\anyax\Aillies\config` | Root cause **UNVERIFIED**; alpha.2/alpha.3 not yet reported back |
| B4 | Cosmetic: with no TOML tables in `config.toml`, install appends root keys after the trailing blank line | Install into a file containing only `model = "x"\n`, then inspect the output | Valid TOML; low priority |
| B5 | UX: users read gray “Using this device” as broken, and Pause blocks *This device* too (one user hit `routing_paused` in Codex) | Pause in the tray, then send from Codex | Matches spec ([DECISIONS #22–23](../DECISIONS.md)); needs a product call |
| B6 | Pairing confirm waits up to 180 s with no cancel button in the window | Start pairing in the window, then don’t approve on B | Minor |

Environment, not code: on the user’s Mac the repo branch is still `master` (GitHub target is `main`), and `.git/stale-*.lock` files are left from sessions without delete permission. Both are safe to clean.

## Failed approaches (don’t repeat)

| Tried | Why it failed | What works |
|---|---|---|
| Prefix check for the upstream override | Accepts user-info tricks (finding 1) | Parse the URL |
| Substring search for terminal events | Content can contain event names (finding 2) | SSE parser |
| Byte-equality guard on Codex restore | The Codex app edits `config.toml` itself (trusted projects, notices), so restore always refused | Structural `merge_restore` ([config.rs](../../crates/aillies-codex/src/config.rs)) |
| `diffy` three-way merge for restore | Codex appends at EOF next to Aillies’ appended block, which conflicts every time | Structural removal of Aillies’ own lines |
| `rustup target add` for macOS/Windows cross-builds | `static.rust-lang.org` blocked from the build sandbox | Ubuntu `rustc-1.91` + `rust-1.91-src` + `-Zbuild-std` + mingw (Windows); build macOS on a Mac |
| Ubuntu std sources as-is for Windows | Debian strips the `windows-targets` dependency and ships no `rsbegin.o`/`rsend.o` | Patch `std/Cargo.toml`; compile `rtstartup` objects ([script](../../scripts/build-windows-from-linux.sh)) |
| `cargo tauri build --target x86_64-pc-windows-gnu` | tauri-cli asks rustup for installed targets, which linked toolchains can’t answer | Build with cargo, then `cargo tauri bundle` with a rustup shim |
| WebView2 `embedBootstrapper` | Download blocked during bundling | Default download bootstrapper |
| Testing the NSIS installer under wine32 | Silent install produced nothing | Inspect the payload with `7z l`; test on real Windows |
| Swift-era tool allowlist (`function`, `custom`, `image_generation`) | codex-cli 0.162.1 also sends `tool_search` and `web_search`, so every real turn got `request_invalid` | Allowlist from captured real requests ([DECISIONS #6](../DECISIONS.md)) |
| `pkill -f "aillies run"` in scripts | Matches the invoking shell, which then gets killed | `pgrep -x aillies` |
| `codex exec` in scripts without stdin redirect | Hangs waiting for stdin | `< /dev/null` |
| Installing Rust on the user’s Mac via the script | `~/.cargo` and `~/.rustup` were root-owned, so rustup looped on renames | Script reuses Rust 1.87+ and fixes ownership with `sudo chown` |

## Assumptions now recorded

Previously only implicit; added to [DECISIONS.md](../DECISIONS.md) #14–25:

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

After that, the next milestone is **M5: live acceptance on two real devices** (human-run checklist in [STATUS.md](../STATUS.md)). Its pass criteria: a text and an image turn from Codex on A, B’s usage moves and A’s doesn’t, plus the offline, sign-out and revoke cases behave as listed.

## Invariant coverage

| Invariant (AGENTS.md) | Existing coverage | Missing |
|---|---|---|
| 1 No fallback | [e2e sharing off / A has no login](../../crates/aillies-node/tests/e2e.rs#L177), [routing paused](../../crates/aillies-node/tests/e2e.rs#L170) | Destination peer removed mid-session → `no_destination` (`Node::remove_peer` + turn) |
| 2 No replay | [mid-stream drop](../../crates/aillies-node/tests/e2e.rs#L201), [early end](../../crates/aillies-node/tests/e2e.rs#L215), hit counts throughout `e2e.rs`; [Codex retries 0](../../crates/aillies-codex/src/config.rs#L542) | Peer connection reset after the request is written → `delivery_uncertain`, one attempt (`loopback::forward` send error path) |
| 3 Credentials stay home | Upstream sees B’s account ([e2e](../../crates/aillies-node/tests/e2e.rs#L115)); A without a login still works | Assert no bytes of B’s access token appear in anything A receives (responses, `/control/status`, peer status) |
| 4 No content in logs or errors | [envelope has no upstream body](../../crates/aillies-codex/src/executor.rs#L251), [envelope bounds](../../crates/aillies-core/src/failure.rs#L166) | Capture `tracing` output during an e2e turn and assert the prompt, output and token are absent; same for `desktop.log` |
| 5 Fingerprint identity, two-sided pairing | [one-time invite, code match, approve](../../crates/aillies-node/tests/e2e.rs#L115), [reject](../../crates/aillies-node/tests/e2e.rs#L244), [code binds both sides](../../crates/aillies-core/src/protocol.rs#L221), [revoke](../../crates/aillies-node/tests/e2e.rs#L228) | Pinned TLS rejects a different server cert at a known address; cancel withdraws approval (`pair_cancel`); expired invite (TTL); `protocol_mismatch` header (426) |
| 6 Transactional Codex config | [round trip and conflict](../../crates/aillies-codex/src/config.rs#L574), [keep Codex edits](../../crates/aillies-codex/src/config.rs#L592), [refuse unparseable](../../crates/aillies-codex/src/config.rs#L617), [ambiguous configs](../../crates/aillies-codex/src/config.rs#L563) | Refuse a symlinked `config.toml`; install failure after writing the receipt removes the receipt |
| 7 Security logic in crates | Structural (shells call `Node`) | A lint or review rule only; no test practical |
| 8 Loopback stays loopback | [override parsing](../../crates/aillies-codex/src/executor.rs#L279) | Bad `Host` header → 421; `/control/*` with the Codex key → 401; `/v1/responses` with the control token → 401 |
| 9 No ChatGPT desktop claim | Docs only | n/a |
| Limits | — | Peer busy at 5 concurrent turns → `peer_busy` 429 with no upstream call; request over 16 MiB → 400 |
| UI escaping | — | Peer name `<img onerror>` renders inert (`esc()` in `apps/desktop/ui/app.js`), via Playwright on `index.html` + `mock.js` |

## Request diagnostics update (2026-10-11)

- Added a persistent 100-entry request error/warning journal and Downloads export in Connection details and the CLI. Content-free closed-enum metadata includes the model, effort, effective tier, route, Lite/image flags, counts, fixed failure code and upstream status. Both requester and sharer observe failures; successful requests and Dismiss preserve history.
- Added fixed Service Unavailable wording for upstream HTTP 503.
- `make test lint` passed on macOS: 74 tests, workspace/desktop fmt and Clippy, UI JavaScript syntax. `make smoke` passed with real Codex CLI against a two-node mock. New tests cover retention/reload, private export, invalid input sanitization, disk failure, both request directions, local routing, stream failures, control authentication, paused routing and protocol mismatches.
- Chrome preview verified the export action and success feedback. The in-app browser checker was unavailable due to its local native-module signature.
- Windows x64 NSIS diagnostic installer built with cargo-xwin from a source-only public-preview staging copy plus these changes. It preserves the public preview's `aillies` upstream identity; 25 provider tests passed in that staging copy, including the identity assertion. Installer payload checked for export support and the new 503 message. Missing MSVC PDB linker warnings do not prevent the release build.
- Physical Windows install/export and current live upstream 400/503 causes remain unverified. Logging begins only after installation. See [DIAGNOSTICS-WINDOWS.md](../DIAGNOSTICS-WINDOWS.md).

## 9router compatibility review (2026-10-11)

- Reviewed decolua/9router ce4460ef79382bfddb4aa5fc0ff9f3cb0d5f95a8. Corrected ordinary/Lite model and hosted-search selection, lifted saved tool prefixes, validated embedded tools, and aligned final upstream standard-tier/parallel-call fields and reasoning includes. Preserved explicit model/effort, history, account selection, credential isolation and no-retry policy.
- Failing-before-fix tests reproduced three core differences and the Astra final-wire difference. `make test lint` passed: 80 tests (26 core, 26 provider, 7 node unit, 17 e2e, 4 desktop), formatting, Clippy and UI syntax. `make smoke` passed with real Codex against two local nodes and a mock upstream. CodeGraph impact/affected analysis checked against source.
- Built unsigned Windows x64 request-compat-v2 NSIS candidate from separate public-preview staging. Public identity remains aillies/0.9.0-alpha.3. Its 27 provider tests passed, including identity header verification. Extracted installer payload contains the new diagnostics marker and matches the built executable except the three-byte NSIS bundle marker. Source and checksums accompany it under dist/windows.
- User screenshot confirms physical Windows diagnostics export works. It shows two Astra/low upstream 400s, not their exact cause. Live acceptance of the compatibility candidate and physical installation remain unverified. See UPSTREAM-COMPATIBILITY.md.

- macOS follow-up: built the public-identity request-compat-v2 Apple Silicon DMG; hdiutil checksum verified, mounted read-only and confirmed the packaged executable contains the new build marker. Saved with SHA256 under dist/macos. Not installed or live-tested; not notarized.

## Router core v3 (2026-10-11)

- User requested 9router core alignment. Added request defaults/aliases/effort mapping, function tool/schema cleanup, bounded client session hints and stable upstream session/cache keys. New installers use the pinned codex_cli_rs/0.159.0 identity, superseding the previous public-preview identity for this build. See UPSTREAM-COMPATIBILITY.md for retained differences.
- `make test` passed 86 tests: core 29, provider 29, node unit 7, e2e 17, desktop 4. The 13-case differential fixture test executes expected bodies generated from pinned 9router's transformRequest. Fixture regeneration produces identical bytes. Schema/alias/default regressions failed before implementation. Session/header tests verify cache consistency, unique per-request IDs and no raw account ID in generated keys; two-node Astra test verifies the session hint survives the peer link.
- `make lint` passed after a byte-string style correction. `make smoke` passed with real Codex and a two-node mock. No live inference was submitted or replayed by this work. CodeGraph impact and affected tests were inspected; index refreshed.
- Mac Apple Silicon and Windows x64 installers are built from source-only staging with identical request core; runtime accounts/state were excluded. Final artifact verification is recorded alongside their checksums. Physical install and live OpenAI acceptance are unverified. Diagnostics identify router-core-v3; old entries remain retained.

- Final router-core-v3 artifacts: Windows x64 NSIS payload matches the built executable apart from the three-byte bundle marker; Mac Apple Silicon DMG checksum is valid and its mounted executable equals the release binary. Packaged client identity/version and diagnostics export symbols verified. Both staged request cores byte-match tested source. Windows optimized the short diagnostics marker into code, so literal string search alone is not a version check. Installers have SHA256 files and a source archive in dist. No physical/live acceptance claim.

## Mac AirDrop packaging correction (2026-10-11)

The user reported the transferred app was damaged. Inspection of the original router-core-v3 DMG showed a valid image checksum but `codesign --verify --deep --strict` failed: code has no resources but signature indicates they must be present. The executable only had its linker-generated ad-hoc signature, with Info.plist unbound and no sealed bundle resources. Earlier payload/checksum verification did not catch this packaging defect.

Repaired a copy of the app with full bundle ad-hoc signing and repackaged it as Aillies_0.9.0-alpha.3_router-core-v3_macfix_aarch64.dmg. Both pre-packaging and mounted-DMG codesign strict/deep verification now pass, as does the DMG checksum. The request implementation is unchanged. This is Apple Silicon only and remains unnotarized; the receiving Mac may require Privacy & Security → Open Anyway. No claim of Developer ID signing or physical AirDrop verification. Prefer macfix over the earlier Mac DMG. Future Mac packaging must verify the full bundle signature, not just the executable and image checksum.

## Router core v4 (2026-10-11)

See STATUS.md and UPSTREAM-COMPATIBILITY.md for verification, scope and remaining live checks. Version alpha.4, diagnostics marker router-core-v4. Important new files: provider_switch.rs (SQLite + rollout/config coordination), session.rs (bounded Codex resolver), telemetry.rs (no content-capable fields), diagnostics.rs (three independent rings), ui-settings-test.mjs (bridge behavior). Codex uses paginated history and byte offsets in thread_history_1.sqlite; changing session metadata alone silently invalidates those offsets. Recovery must set absolute old/new values, never blindly reapply deltas. The installed Codex isolated regression is opt-in: `cargo test -p aillies-codex installed_codex_lists -- --ignored --nocapture`; it makes no live inference and never reads the user's home.

User accepted quitting/reopening Codex. Do not kill the active Codex app to migrate this chat. Existing chats are updated only when the user invokes the new provider selector or `codex on/off --existing-chats` while closed. Root-provider scope, unknown schema/unindexed refusal and two-second preparation budget are deliberate. Private provider-switch recovery receipts contain config backups/paths; never include them in diagnostic exports. Update both paired apps before live verification; never replay an earlier failed request automatically. Image approvals remain Codex's normal approval flow.

Mac packages must seal the whole bundle and verify the mounted payload. `scripts/package-macos.sh` packages an already built app with ad-hoc signing and strict/deep verification. It does not install or notarize anything. Both builds must retain codex_cli_rs/0.159.0; do not revive old public-preview identity rewrites.

Final v4 checks: 101 Rust default cases + settings behavior pass; installed-Codex optional case also passed (102 total verified Rust cases). Workspace/desktop lint and mock smoke pass. Mac HFS+ mounted checksum/signature/payload verification passes; Windows NSIS extracted payload differs by three expected bundle-marker bytes. No fresh live 5.6/approval/image acceptance and no physical Windows v4 install claim. Source and artifact provenance are under dist; use router-core-v4 alpha.4 on both devices.

## Alpha.5 unindexed history follow-up (2026-10-11)

Supersedes alpha.4 blanket unindexed refusal: valid unique rollouts are file-only migration changes; no index rows are invented. Root-provider scope remains. Codex replays thread_settings_applied events when rebuilding missing index rows, so header-only migration is insufficient. Preserve original records and append the latest settings event with only provider ID/next ordinal changed. Settings permissions are copied intact. Projection offsets point to the appended event; do not shift them by its length. File-only journals use a completed flag; mixed/indexed journals still use SQLite provider state. Unsupported schemas, duplicate IDs, malformed records, external/symlink paths and preparation over two seconds still refuse. Record limit is 16 MiB. Optional installed-Codex test now removes the index row after a full round trip and verifies list/resume/read both ways again. Never run this against the user's real history.

Alpha.5 checks: 104 Rust default tests plus UI settings behavior and lint passed; installed Codex optional case also passed. Mock smoke passed. Release marker router-core-v4-historyfix, app version 0.9.0-alpha.5. Request core/client identity are unchanged from v4. No installed app or user config/history was modified.

Alpha.5 artifacts are verified under dist/macos and dist/windows, named router-core-v4-historyfix. Use scripts/package-macos.sh with full bundle signing/checksum/mounted-payload checks; Windows payload comparison allows only the three expected NSIS marker bytes. No physical Windows run or live upstream verification performed.

## Alpha.6 image display follow-up (2026-10-11)

MCP image results now provide an absolute saved path plus ready-to-display Markdown and final-reply guidance. Follow-up display reuses the saved image. This fixes a presentation-guidance gap shown by the user's screenshot; no live first-reply rendering claim. Separate local safe journal entries show hosted image success with GPT-5.6 Sol and a later HTTP 400 with model category other; exact client/model still unconfirmed. Never silently replace the requested model or replay either request. App version alpha.6, marker router-core-v4-image-display; alpha.5 history correction remains included.

Alpha.6 checks: 104 Rust default tests plus UI behavior, lint and mock smoke passed. Initial smoke startup raced its fixed sleep before inference; it now polls authenticated control readiness with a bounded wait. Verified Apple Silicon DMG at dist/macos/Aillies_0.9.0-alpha.6_router-core-v4-image-display_aarch64.dmg; full-bundle mounted signature/checksum/payload comparison passes, still ad-hoc/unnotarized. No Windows alpha.6 installer built for this Mac-specific report. No live first-reply image display or separate upstream HTTP 400 fix claim. No installed app/config/history changed.

## Alpha.7 explicit macOS restart flow (2026-10-11)

User requested adding Quit, switch & reopen to Aillies, not closing sessions during development. New `restart.rs` / `restart/macos.rs` hold OS operations and orchestration; Node serializes and stores expiring one-use plans, desktop IPC only delegates. Review is read-only. Unknown/background Codex clients refuse; known app ownership and terminal working-dir/home locks must be identifiable. App quit honors save dialogs, terminal/known daemon receive SIGTERM once, no force quit. Existing closed-process checks and migration transaction remain. Terminal reopen uses `resume --all` pickers, never a prompt or approval flags. No exact per-terminal session UUID was available from safe process metadata. Do not claim automatic exact-session restoration or idle detection. Check Automation permission on a real installed build; live shutdown was deliberately not exercised against this active conversation. Mac-only feature; Windows/Linux stay manual. Recovery pending after failed migration must prevent reopening clients. No process paths/commands/environments enter diagnostic exports.

## Alpha.8 provider switch correction (2026-10-11)

Supersedes alpha.7 reopen scope: only ChatGPT opens, even when it was quit before review. No terminal/editor/Codex reopen commands remain. Pending UI target stays selected through review/polling/execution; Cancel or failure returns to actual provider. Never run the new action against this active chat during development.

The supplied recording also shows an actual migration refusal. Read-only local checks found seven extra rollout copies of indexed openai chats and older settings events missing thread_id. Indexed same-provider replicas now migrate as file-only changes in the same journal; offsets belong only to the indexed canonical file and reports count unique chat IDs. Duplicate files without an indexed authority or with conflicting providers still refuse. Missing legacy settings thread_id inherits file scope, present null/foreign IDs still refuse. No history is discarded or raw contents logged. User history and processes were inspected read-only, never migrated/restarted. Verify real shutdown/save prompts/Automation and user-history switching manually after installation.

Alpha.8 validation: 118 default Rust tests plus UI, fmt/Clippy/syntax and mock smoke passed. Installed Codex isolated list/resume/reindex/history check passed. New marker router-core-v4-switchfix, version0.9.0-alpha.8. Browser preview remains blocked by its local classic-level native module signature; no browser visual acceptance claim. Mac bundle/DMG verification and exact hashes are saved with dist artifacts. No Windows alpha.8 installer, no user process shutdown or live inference performed.

Alpha.8 Mac artifact: dist/macos/Aillies_0.9.0-alpha.8_router-core-v4-switchfix_aarch64.dmg. Full-bundle strict/deep ad-hoc signature passed before packaging and on the mounted read-only DMG; image checksum and signed staged/mounted executable comparison passed. Actual bundled Info.plist version and ChatGPT-only Automation description verified. Still unnotarized; no installed app was replaced.

## Alpha.9 background preparation (2026-10-11)

Supersedes prior two-second preparation limit: a real screenshot demonstrated the hard cutoff still blocked larger history. Production passes no deadline; optional deadlines remain in private test paths for deterministic cleanup tests. Do not reintroduce a short cutoff or skip histories to meet performance targets. Background workers already existed. New SwitchProgress/phase types carry only counters, with transient Node status and provider-row rendering; no diagnostic persistence or thread IDs/content. Progress status must not take codex_change; it remains readable while preparation runs. Errors stay visible. Existing close guard before apply, transaction and recovery remain. Test/package results are recorded with alpha.9 artifacts; never exercise real shutdown from this active chat.

Alpha.9 verification: `make test lint` passed 120 default Rust tests (core31, provider56, node11, e2e18, desktop4), UI behavior, workspace/desktop fmt and Clippy, and JavaScript syntax. The new elapsed-clock regression failed before removing the cutoff; progress rendering also failed before implementation. Preparation progress precedes commit and contains only fixed phases/numeric counters. Focused Node test verifies progress remains readable while the change lock is held and old status without the new optional field deserializes. Installed Codex isolated list/resume/reindex/history checks passed both directions. `make smoke` passed with real Codex, two local nodes and mock upstream. CodeGraph sync/impact/affected results checked against source.

Built Apple Silicon alpha.9 DMG with marker router-core-v4-background. Full-bundle strict/deep ad-hoc signing, mounted DMG checksum and signed executable comparison passed. Actual bundled version and ChatGPT-only Automation description verified. Still unnotarized. No installed app, user history/config or running user session was changed; actual large-history migration and shutdown/reopen remain manual acceptance. No new live image/OpenAI request or Windows alpha.9 installer. UI verified with the fake bridge; no fresh browser visual acceptance claim.

## Alpha.10 retries and recent warnings (2026-10-11)

The user explicitly chose retries even after OpenAI errors/interrupted responses. Decision 45/updated AGENTS.md supersede the previous no-replay policy for requester logical turns only. Initial + three retries, 500/1000/2000 ms backoff, 880-second total deadline. Core/executor/peer one-send behavior and Codex retry=0 remain. `retry.rs` holds generation events until completion and sends keepalive comments, so generation text is delayed. Final failure after headers is fixed failed SSE. Client cancellation drops the pending future and backoff. Local validation/login/policy refusals stop. Never add a second budget in the sharing node or assistant tool loop.

Account scopes: local expected-account executor; host in-memory (client fingerprint, generated trace) bindings, max1,024, TTL900 seconds. Numeric attempt and initial-send boolean are fixed peer metadata. A missing used binding refuses after restart/expiry. Confirmed pre-send/busy failures can establish a first binding later. `supports_pinned_retries` defaults false for older peers; update both devices. Same device/model/history/tier/session, no account fallback, credentials stay home.

Forced MCP image responses validate complete PNGs before becoming the winner; only one file is saved. This does not route native image_gen or guarantee current model/backend acceptance. Latest four fixed notices are derived from the private persisted failure ring; success/dismiss do not erase them. Each attempt has a numeric identifier in traffic diagnostics. No raw bodies, account IDs, tokens or sessions enter notices/logs. Verification and package evidence follow below.


Alpha.10 verification: `RUST_TEST_THREADS=4 make test lint` passed 129 default Rust tests (core31, provider56, node14, e2e24, desktop4), UI behavior, workspace/desktop fmt and Clippy, and JavaScript syntax. Two explicit installed-Codex probes remain opt-in and were not rerun. New retry regressions failed before implementation. Coverage includes successful recovery, four-attempt exhaustion without nested retries, cancellation during backoff, destination/account changes, revocation, partial-content suppression, valid-image-only saving, bounded account scopes and persistent escaped latest-four notices. `make smoke` passed real Codex CLI 0.161.0 through two local nodes and a mock upstream, with account and offline checks. CodeGraph was synchronized and impact/affected results checked against source. No live OpenAI request or native image_gen routing acceptance was performed.

Apple Silicon alpha.10 artifact: dist/macos/Aillies_0.9.0-alpha.10_router-core-v4-retry_aarch64.dmg. Full-bundle strict/deep ad-hoc signature verified before packaging and on the mounted read-only DMG; checksum and signed payload comparison passed. Bundled version and ChatGPT-only Automation description verified. SHA256: c11ed66262f09dc3042807d594affc32ac4ec11ab36cafa64a2989660b2c7302. Still unnotarized. Update both paired devices for account-pinned retries. No installed app, user process, configuration or history was changed; no Windows alpha.10 installer built. UI verified through the fake bridge, without a fresh browser visual acceptance run.

Alpha.11 final verification: RUST_TEST_THREADS=4 make test lint passed **131 default Rust tests** (core31, provider56, node15, e2e24, desktop5), settings/tray interaction tests, fmt, Clippy and UI syntax. The two-node suite completed in 362.64 seconds. make smoke passed real Codex CLI 0.161.0 → two local nodes → mock upstream. CodeGraph refreshed and impact/affected checked against source; git diff --check passed. Browser mock preview checked Home, conditional Fast permission, direct pairing and dark provider layout. Package signature/checksum/payload/icon verification passed. No real-user app/config/history changes, live inference, physical native tray/restart acceptance or Windows alpha.11 installer.

Mac artifact: dist/macos/Aillies_0.9.0-alpha.11_router-core-v4-design_aarch64.dmg. SHA256: def9a5f17bcf4ff669854f40b7073423bdc664c3905f25da7c8fc20e19d214fa. Matching source archive and install/provenance files are under dist.


Alpha.12 final verification: 136 default Rust cases plus one explicit same-host mDNS case passed (137 verified), UI behavior, lint and real Codex CLI→two-node mock smoke. Browser mock entry/grouped-code screenshots are under output/playwright. Mounted DMG signature/checksum/executable/icon checks passed. Info.plist includes the Bonjour service and local-network purpose. Artifact: dist/macos/Aillies_0.9.0-alpha.12_router-core-v4-pairing_aarch64.dmg; SHA256 532cdb1476974755e2db9a1e1de2f9e08256bf00dba2dd5ad8f86be49293c605. Physical numeric pairing, Windows firewall and live inference were not exercised. Source archive and provenance are under dist.


Alpha.12 Windows follow-up: x64 NSIS now built from the immutable 120-file alpha.12 source ZIP; no runtime-source changes. `7zz t`, extracted PE64/version, exact three-byte UNK→NSS Tauri patch and license comparisons pass. Artifact: dist/windows/Aillies_0.9.0-alpha.12_router-core-v4-pairing_x64-setup.exe; SHA256 f34720854b45f78efb8221c02756ef54c7d0396fe502b56e8f0c938bcd1ca42c. Build/install notes accompany it. Physical Windows/WebView2/firewall/numeric pairing and live inference remain unverified; Windows provider switch is still manual. This supersedes the earlier alpha.12 “no Windows installer” statement.
