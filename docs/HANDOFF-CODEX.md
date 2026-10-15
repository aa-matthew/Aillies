# Current handoff — Codex/Claude 1.1.0 release preparation

Read [AGENTS.md](../AGENTS.md) first. This is operative guidance for the prepared 1.1.0 release (`router-core-v11-review-fixes`), including retained Codex 1.0 behavior. [STATUS.md](STATUS.md) tracks current acceptance; [RELEASES.md](RELEASES.md) selects the shipped artifacts. Dated investigation and verification notes are retained in [history](history/HANDOFF-CODEX-2026-10-11.md); their earlier policy/coverage claims are not current instructions.

## Release preparation

Use exact tag `codex/claude-stable-1.1` for application 1.1.0; release archive names use `codex-claude-stable-1.1`. Notes live at `docs/releases/codex/claude-stable-1.1.md`. Merge after platform CI passes. Preparation alone does not authorize pushing the tag that publishes. Prior alpha.9 packages remain unchanged. Current runtime is the reviewed/repaired alpha.9 implementation; Claude remains experimental and native/live acceptance is open.

## Current behavior

- Requester owns initial + three retries, same selected device/account/model, with 500/1000/2000 ms backoff and 880-second total deadline. Buffer each attempt up to 64 MiB; only a completed winning response is exposed. Earlier attempts may consume usage. Cancel, route/account changes, validation and access refusals stop. Codex/executor/sharer retry budgets remain zero. Decision 45 supersedes earlier no-replay wording.
- Eight random digits bootstrap the full one-time invite through SPAKE2/AES-GCM bound to both TLS fingerprints. LAN mDNS carries public metadata only. Ten minutes / five PAKE executions; consume or invalidate on replacement/sharing off/shutdown/expiry. Full codes remain for private networks/older peers; compare and approve on both devices. Stored certificates authorize future access; OAuth never crosses pairing.
- Existing chats switch while Codex is closed using a journaled file/config/SQLite transaction. Include valid unindexed rollouts and indexed same-provider replicas; preserve transcript records and update provider settings/projection offsets. Background preparation has no production deadline. Do not skip chats or reintroduce two-second refusal. Recovery uses absolute offsets.
- macOS restart is an explicit, expiring, one-use reviewed action. Validate all inventories before closing eligible clients normally; reopen only affected desktop apps (ChatGPT/Claude), never terminals/editors. Unknown clients, changed inventories or unresolved recovery refuse. Windows/Linux remain manual. Never restart the active user chat while developing.
- Desktop exposes adjacent Codex/Claude activation in Aillies Use and current local accounts in Aillies Share, independent service discovery and conditional Allow Fast. CLI/control retain simple enable/restore and pause/resume. Disconnect & quit attempts independent owned-settings cleanup; unsafe files retain receipts and explicit Quit anyway always remains. Claude Code has a native signed-in account adapter on claude-integration (1.1 alpha); see CLAUDE.md. Claude remains experimental; the published Codex 1.0.0 release remains available.
- Explicit MCP image creation validates a complete PNG, saves one winner and returns a local Markdown reference. Native image_gen routing/editing are separate and not established by these checks. Never bypass approval to probe images.
- Diagnostics: three independent latest-100 rings (incoming, upstream, failures), fixed categories/attempt numbers only; latest four notices on Home. No prompts, outputs, images, tokens, upstream error bodies or private recovery receipts in logs/exports.

## Alpha.9 operative notes

Windows/Linux activation returns manual_close and records affected services in the one-use plan. Apply checks closure, runs settings/history changes and launches nothing; empty cleanup succeeds. Mac lifecycle stays reviewed. Hosted Claude consent/source/identity changes clear bindings and increment a generation; async creation must reject a changed generation. Do not refresh/rebind used retries. UI completion finds Codex/Claude by provider ID and routes to Aillies Use. The exact separate version message was unavailable; do not claim an unknown compatibility error was fixed.

## Alpha.8 operative notes

`activation.rs` is the desktop lifecycle/exit entry point. Reviews are one-use/120 seconds, validate both inventories before any closure and keep failed cleanup closed. Malformed/symlink/history recovery refusals must not be bypassed by deleting receipts; offer explicit exit. Owned edited desktop gateway fields are removed while non-routing user edits survive. Codex history transaction remains intact.

Connected Claude OAuth now uses private accounts/claude.json with atomic refresh intent/token persistence,0700 directory and 0600 file. This is less protected than Keychain. Do not automatically access old Keychain items: absent private storage with a retained selection stops sharing and shows Reconnect. Production Connect remains Mac-only; fixtures never access OS credentials. Native legacy sources remain explicit CLI/control compatibility only.

Editable device names propagate through approved pinned status as bounded metadata; fingerprints/grants are unchanged. Status advertises only services actively shared and granted to that requester. The UI overview/rename/discovery/diagnostics/activation review/incomplete cleanup were exercised with the loopback mock; screenshot is dist/verification/aillies-use-share-alpha8.png. Native process shutdown, save dialogs, actual OAuth and physical/live inference remain unverified; never restart this active chat during development.

## Current invariant coverage

| Invariant | Evidence in current source | Remaining limit |
|---|---|---|
| No account/device/model fallback | e2e `forgetting_selected_peer_never_selects_local_account`, `retry_stops_on_destination_change_without_local_fallback`, account pinning/refusal tests | Physical offline/account-change acceptance |
| Bounded authorized retries | e2e recovery/exhaustion, cancellation, reset-after-write, partial-content suppression and valid-image-only winner tests; retry-scope unit bounds | Live upstream interruption/usage accounting |
| Credentials stay home | e2e `status_and_response_never_expose_sharer_credentials`, mock account assertions, encrypted bootstrap tests | No physical packet-capture claim |
| No content in logs/errors | e2e `tracing_never_contains_request_response_or_credentials`; typed lifecycle, closed-enum diagnostics/persistence tests | Broader physical acceptance |
| Fingerprint identity and approvals | wrong pinned certificate, numeric PAKE tampering/password/expiry/guess bounds, both approvals and one-time use tests | Physical Mac/Windows multicast/firewall flow |
| Transactional config/history | provider-switch rollback/recovery/offset/unindexed/replica/progress tests; earlier opt-in installed-Codex synthetic probe | Real shutdown/large-history/sleep-wake acceptance |
| Security in crates | desktop IPC delegates to Node; requester/peer/crypto/config logic remains in crates | Ongoing review constraint |
| Loopback isolation | e2e Host/key boundaries, protocol mismatch; MCP wrong key/control key/Origin checks | Physical network acceptance |
| No official ChatGPT desktop substitution claim | README/AGENTS/product scope | Documentation constraint |

## Verification and next work

Current reviewed alpha.9 checks passed 206 default Rust cases, UI, strict lint and real-Codex-to-mock smoke. Version 1.1.0 PR [CI 38235371730](https://github.com/aa-matthew/Aillies/actions/runs/38235371730) passed all six macOS/Windows/Ubuntu workspace and desktop jobs, including native Windows tests and desktop builds. Existing tray tests decode all three retained 44px icons. No new live inference or user process/config/history changes. Package/build evidence is recorded in STATUS.

Next acceptance: physical alpha.12 Windows install/WebView2/tray/firewall, numeric pairing on two real devices, macOS restart/save-dialog/large-history flow, live 5.6/image approval/display, Fast entitlement and correct account usage. Current live capability must be checked after updating both devices, with normal approvals.

Optional synthetic probes: `cargo test -p aillies-codex installed_codex_lists -- --ignored --nocapture`; `cargo test -p aillies-node --test e2e numeric_pairing_via_local_discovery -- --ignored --nocapture`. Neither should use the user's real account/history. No automatic rerun of old failed chats.

## Cleanup boundary

Unused wrappers and old desktop enable/restore IPC were removed after caller/graph inspection; progress-aware implementations and Node/control methods remain. Six unused tray variants and two unused app PNGs were removed; the generator now emits only configured/runtime outputs. Master artwork, both lockfiles, fixtures/generator/probe, default instruction text and mock.js remain.

Peer Route sources, investigation notes and the complete legacy working directory are privately preserved; see [archive index](ARCHIVES.md). Dated verification/product/design evidence remains under history. Do not publish the private archive or select old installers automatically.

## Codex stable publication

The owner explicitly authorized `codex-stable-1.0` (application 1.0.0) as stable with limitations disclosed. Decision 51 records the signing/promotion exception; no runtime invariant changes. The release workflow gates publishing on all selected platform checks/builds and attaches only explicit packages, tagged source and SHA256SUMS. Physical/live acceptance remains open; do not treat the stable label as new evidence.


## Claude branch handoff

Read [CLAUDE.md](CLAUDE.md) and decisions 52–54. Decision 53 supersedes the rejected API-key-only design. Native Messages/token counts, independent provider state/grants, owner account/token pinning, transactional settings and shared retry/diagnostic machinery are implemented. The Claude CLI remains the local session owner; no history/provider-owner rewriting or automatic editor restart. Existing clients remain Codex-only, and Claude-only pairing grants no Codex access. Upgrade both peers.

Read the owner’s configured OAuth token or native macOS Keychain / `.credentials.json` login, with `.claude.json` account metadata. Only fixed Anthropic HTTPS receives the Bearer token. Preserve native request bytes and client User-Agent, merge required OAuth/Claude Code betas. Account changes require new sharing consent; token refresh between turns for the same native account is accepted, while any token/account change during a retry stops. No API-key/direct/device fallback. No auto refresh or native credential edits. Alpha.1 sharing consent resets on upgrade; pairings and grants survive. Its private API-key file is ignored.

Explicit `NodeConfig.claude_home` test roots disable Keychain access. Default checks and the installed-client probe use synthetic credentials and mock inference only. Actual Keychain access, live Anthropic entitlement/betas/tool use and physical-device acceptance remain open. Defer Claude HTTP200 until a winning attempt completes: the installed client otherwise resends empty failed streams even with `CLAUDE_CODE_MAX_RETRIES=0`. Unknown native SSE errors fail closed. Final alpha.2 verification/package evidence is recorded in STATUS. No production settings, history or running client is changed during development.

Alpha.2 final verification: 160 default Rust cases, UI behavior, fmt/Clippy, JS syntax and isolated real-Codex-to-mock smoke passed. Installed Claude Code 2.1.227 mock probe passed separately. Historical alpha.2 selection was `Aillies_1.1.0-alpha.2_claude-account-r2_aarch64.dmg`; earlier alpha.1 and initial alpha.2 packages are superseded. Exact mounted verification/hash is in STATUS. `AILLIES_TEST_CLAUDE_HOME` disables native Keychain for synthetic CLI smoke roots; the harness checks unavailable Claude accounts before routing.


### Desktop direct-login extension

Alpha.3 separates Desktop Chat from Code settings and sharing account source. Native gateway profile is target-bound and restored transactionally without login/cloud-history edits. Mac switch refuses running Claude and managed/bootstrap configs. No automatic app/terminal/editor reopen. Desktop chats in gateway mode are local and separate from cloud chats; the old normal account label is not a route indicator.

DesktopReader supports the installed Mac Safe Storage v10 cache format, exact Keychain service/account, active UUID/org/scope/expiry validation and V2-authoritative selection. No cookies/token mint/refresh/Code fallback. Existing inference cache required; a web-only login may not have it. Test DI is explicit and synthetic; no native Keychain/user settings/processes were accessed. Desktop total budget is 240 seconds to fit gateway silent-stream timeout. Physically verify Keychain permission, native desktop profile load and actual request/entitlement before claiming live support. Windows/Linux desktop owner credential reuse is unavailable.


Final alpha.3 verification: all 170 default Rust cases passed (Claude21, Codex56, core35, node20, e2e33, desktop5), plus UI behavior, workspace/desktop fmt and strict Clippy, JS syntax, and isolated real codex-cli 0.161.0 mock smoke. The separate installed Claude Code 2.1.227 two-node mock probe passed. The first combined run exposed a desktop IPC call to private `snapshot`; that wiring was corrected, then affected desktop/UI checks, all lint and smoke passed. Desktop fixture tests were rerun after adding guarded-Quit coverage. No live inference, real cache/Keychain access, user config/history changes or running-app replacement occurred.

Selected experimental installer: `dist/macos/Aillies_1.1.0-alpha.3_claude-desktop_aarch64.dmg`, SHA256 `6851ea7b6bfd0b256a774b1e5316b2d184bae0b0a2e1c5a28279776bdf06c8d5`. Apple Silicon / macOS13+; release build, mounted DMG checksum, deep/strict ad-hoc signature, exact executable and ICNS comparisons passed. The package includes updated crypto notices. It is not notarized. Prior installers and the delivered Codex stable release are preserved. UI mock selection/sharing/device status was visually verified (`dist/verification/claude-desktop-alpha3.png`); this is not proof of native desktop startup/live account acceptance. CodeGraph was refreshed and whitespace checks passed.


### Alpha.4 follow-up

The user reported the generic account failure while Code/Normal/local was selected, though the intended surface is desktop Chat. Do not infer the exact native credential problem or say the picker defect caused their selection. No real credentials/config were inspected. Local sharing now has fixed reason-specific messages (`LoginIssue`/`LoginError`) without values, preserving request FailureCodes and no fallback. Both account sources are unprobed during status polling while sharing is off. UI preserves active selects during polling, shows source/Not shared, offers explicit Desktop Chat from Code errors and removes the duplicate Share error toast. Synthetic regression tests prove these behaviors; actual desktop cache/Keychain/live request acceptance remains open.


Alpha.4 package: `dist/macos/Aillies_1.1.0-alpha.4_account-checks_aarch64.dmg`, SHA256 `e280b1fec3c293e999f925ee4f337654deb197ae46d3ae300f261d69cf48f88d`. Release build and mounted DMG checksum, deep/strict ad-hoc signature, executable and icon comparisons passed. Previous installers remain intact. The isolated installed Claude Code 2.1.227 mock probe passed again; no live account or native Keychain was used. The UI mock verifies Desktop Chat defaults, explicit sharing source and Not shared state (`dist/verification/claude-account-check-alpha4.png`). Real desktop cache/Keychain/live request acceptance is still pending; the build does not claim to repair the user's unknown credential problem.


Final alpha.4 validation: `RUST_TEST_THREADS=4 make test lint smoke` completed successfully with 171 default Rust tests (Claude22, Codex56, core35, node20, e2e33, desktop5), UI regressions, strict workspace/desktop lint, JS syntax and real-Codex-to-mock smoke. The separate installed-Claude mock probe passed. CodeGraph and whitespace checks were refreshed. All credential tests used synthetic roots; no actual Keychain/login/history/settings or running app was changed. This verifies the feedback/selection fix and refusal boundaries, not the user's unknown native credential failure.


### Alpha.5 operative override

Decision 56 supersedes the no-new-login restriction of 54 for the main UI. Explicit Connect account uses OmniRoute-style browser OAuth/PKCE and manual code#state, saves Aillies-owned credentials in its own Mac Keychain item and renews before new turns. Both client types use that selected account. Legacy native readers remain explicit CLI/control sources, with no native fallback after connected-source failure/disconnect. Never test actual user login/Keychain/settings/running applications. The generic desktop-cache cause is still unknown and is not repaired by this change.

Exchange/refresh bodies/errors stay private and bounded; fixed origin, redirects/proxies off, genuine account/org required. Pending flow one use/ten minutes; cancellation/replacement invalidates in-flight commit. Sharing off required to connect/disconnect. Refresh intent persists before send; failed/uncertain refresh or save blocks until reconnect. Mutex serializes renewal; pinned retry bindings are reused before any fresh-token path. Native login stays unchanged. Mac-only Connect UX; other platforms can send through a paired Mac. Browser/Keychain/native desktop/live entitlement are still acceptance gates. Final verification/package evidence follows.

Final alpha.5 checks passed: 179 default Rust tests (Claude27, Codex56, core35, node20, e2e36, desktop5), UI regressions, workspace/desktop fmt/Clippy, JS syntax and real-Codex-to-mock smoke; separate installed Claude Code 2.1.227 two-node mock probe passed. New OAuth tests include cancellation during exchange, one-use state, bootstrap identity, inference scope, concurrent renewal, rotated refresh persistence, uncertain refresh blocking and no token rebinding during retries. UI guards local activation without Connect; paired requesters need no local connection. Mock browser screenshots are in `dist/verification/claude-connect-form-alpha5.png` and `claude-connected-alpha5.png`. CodeGraph and whitespace checks were refreshed.

Selected experimental package: `dist/macos/Aillies_1.1.0-alpha.5_claude-oauth_aarch64.dmg`, SHA256 `1903bbf4337b71dd1c743dd22aae412c56121ff3cf13c2b7754afc23f4fd84cb`. Final release rebuild, mounted checksum, deep/strict ad-hoc signature, executable and icon comparisons passed. Not notarized; previous installers/stable release preserved. No native authorization, real Keychain access, live inference, user settings/history changes or running-app replacement occurred. Next acceptance is the user completing Connect and testing the native desktop gateway/live entitlement; mock success does not establish that acceptance.

### Alpha.6 operative addition

Decision 57 adds default GET model discovery for Desktop Chat/Code. Authenticated loopback endpoints route native models through the selected account and pinned peer path, with vetted queries, bounded native-response validation, existing grants and the requester retry budget. No fake/static catalog or credential/device fallback. Desktop profiles enable discovery and omit inferenceModels. Startup upgrades only exact owned profiles with a pending receipt, preserving normal selections/logins/history; restore accepts both known crash states. No new UI button or automatic app restart. Native picker/live OAuth acceptance remains open; final verification/package evidence follows.

Alpha.6 package: `dist/macos/Aillies_1.1.0-alpha.6_claude-models_aarch64.dmg`, SHA256 `ae890df241e1c05bafbbd689c41d60461ec417d3a9d64e914a988a80c30120b6`. Final release rebuild, mounted DMG checksum, deep/strict ad-hoc signature and exact executable/icon comparisons passed. Prior installers/stable release remain intact. Not notarized. Development used synthetic account/settings roots only; no real Keychain, login, history, live inference or running app was changed. Native discovery-picker startup and live OAuth model-list acceptance remain unverified.

Final alpha.6 verification: `RUST_TEST_THREADS=4 make test lint smoke` passed with 186 default Rust tests (Claude29, Codex56, core37, node20, e2e39, desktop5), UI regressions, workspace/desktop fmt and strict Clippy, JS syntax and isolated real-Codex mock smoke. The installed Claude Code 2.1.227 mock probe passed separately. Final-source model routing tests passed again after removing redundant body parsing; profile tests additionally verify resume and restore at both journal crash points. Authentication, peer grants, native metadata/pagination, query/body refusal, bounded retry/recovery/exhaustion, malformed-catalog privacy and startup upgrade passed using synthetic homes only. CodeGraph impact/affected review and final sync plus whitespace checks completed. No live/native catalog acceptance claim.

### Claude frontier compatibility — alpha.7

Decision58 adds a pure core adapter to the shared Claude executor. Both clients/local/peer routes use reviewed thinking/effort/sampling normalization and shape-required beta headers, while model/system/tools/signed-history raw JSON stays exact. Discovery remains enabled without a new button. Incompatible forced tools/fast/system directives refuse with a fixed nonretryable400; no semantic fallback. Current rules and reference URLs are in CLAUDE.md. Unknown future models get no guessed model rules; HTTP429 and live entitlement remain separate.

Package verified: `dist/macos/Aillies_1.1.0-alpha.7_claude-frontier_aarch64.dmg`, SHA256 `f13252accc8f50533c86bbee6619541d2d7aff401294be17df604bf12161df35`. Apple Silicon/macOS13+, deep/strict ad-hoc signature, mounted checksum and exact executable/icon comparisons passed. Prior installers are preserved. Not notarized. No actual Keychain, login, settings, history, live inference or running app was touched.

Final alpha.7 verification: `RUST_TEST_THREADS=4 make test lint smoke` passed with 200 default Rust tests (Claude29, Codex56, core48, node21, e2e41, desktop5), UI regression checks, workspace/desktop fmt and strict Clippy, JS syntax and isolated real-Codex-to-mock smoke. The installed Claude Code2.1.227 peer gateway probe passed separately. Core cases cover exact raw signed-history preservation, no-op bytes, current/legacy effort, unsupported semantic refusal, conditional betas and idempotence. Two-node fixtures bridge discovery to ten reviewed modern model IDs on both client prefixes, local/peer routes, OAuth owner headers and unchanged native thinking/signature/tool SSE. Retry/finite JSON/token-count/refusal cases pass, and closed diagnostic models never retain content. CodeGraph impact/affected review was verified against source and whitespace checks passed. These checks do not establish native/live frontier model entitlement, thread acceptance or billing.


### Aillies Use/Share — alpha.8 package

`dist/macos/Aillies_1.1.0-alpha.8_use-share_aarch64.dmg`, SHA256 `917f8183df3bd099a7b1567834ca84bb08a4949db2f776f1c64e9d86c27b6ebd`. Apple Silicon/macOS13+, ad-hoc signed and not notarized. Release build, read-only mounted DMG checksum, deep/strict signature and exact executable/icon comparisons passed. Earlier installers and stable selection remain intact. This package was built without installing/launching it or changing the user's real accounts/settings/history/running clients.

The overview, editable name, service discovery, Diagnostics, activation review/Cancel and incomplete-cleanup exit were exercised in the loopback browser mock. Actual app/save-dialog shutdown, browser OAuth, physical peers and live Claude entitlement remain acceptance tasks.


Alpha.8 final validation (2026-10-12): `RUST_TEST_THREADS=4 make test lint smoke` passed with 204 default Rust cases (Claude30, Codex58, core48, node22, e2e41, desktop5), UI contract checks, strict workspace/desktop lint, JS syntax and real-Codex-to-isolated-mock smoke. The installed Claude Code2.1.227 peer mock probe passed separately. The augmented missing-private-connection/reconnect plus unsafe-exit regression also passed in isolation. CodeGraph and whitespace checks were refreshed. All account/settings tests used synthetic roots; no actual Keychain, login, history, live inference or running user app was changed.

Verified installer copied to `/Users/mtbhust/Downloads/Aillies_1.1.0-alpha.8_use-share_aarch64.dmg` with matching SHA256. Experimental candidate remains separate from stable; package verification does not establish native save-dialog/restart, browser OAuth or live Claude entitlement acceptance.


### Alpha.9 activation/retry repairs — packages and targeted checks

Decision60 repairs the two review findings. A pre-fix Code→Desktop source-change regression sent four upstream attempts instead of one; final targeted/e2e regression stops after the original attempt and allows a fresh trace under new consent. Same-account off/on also invalidates retries. Async executor creation is guarded by binding generation. No native account/settings/process was changed.

Windows/Linux manual activation no longer calls the unsupported restart engine. Final-source synthetic Linux x64 container test passed Codex/Claude activation/restoration, one-use apply, disconnect and empty cleanup. Windows node test executables cross-compiled; Windows workspace/all-target strict Clippy passed. Installed Claude Code2.1.227 isolated peer mock probe passed separately. Native Windows execution/install and physical/live acceptance remain open. Onboarding/pairing now show both providers by ID and return to Aillies Use; the user's exact additional version message was unavailable, so compatibility checks remain unchanged.

Verified packages (also copied byte-identically to `/Users/mtbhust/Downloads`):

- `dist/macos/Aillies_1.1.0-alpha.9_review-fixes_aarch64.dmg`, SHA256 `c69aab7fc283278870b59a445e37e5b285599f191f651bcb61127b1ddfae4bf3`,8118439bytes. Apple Silicon/macOS13+, ad-hoc signed/not notarized. Mounted checksum, deep/strict signature, executable/icon comparisons passed.
- `dist/windows/Aillies_1.1.0-alpha.9_review-fixes_x64-setup.exe`, SHA256 `402a67a9e4651ebdad97ba51c2d7ab80d2a0f6ef07740f3a2d3e09c2fdff5961`,5523250bytes. Unsigned x64 NSIS, cross-built with cargo-xwin. Archive integrity/extraction, AMD64 PE/version, exactly three expected UNK→NSS bundle-marker bytes and all nine license files/notices passed. A temporary bundle resource map includes notices/licenses; first packaging config used the wrong source root and failed before bundling, then was corrected. No failed package is selected.

Prior artifacts and stable selection remain preserved. Neither installer was installed/launched, and real login/history/settings/Keychain/running user clients were untouched. Final full source-check result is recorded below after completion.


Alpha.9 final validation: `RUST_TEST_THREADS=4 make test lint smoke` exited successfully. All206 default Rust cases passed (Claude30, Codex60, core48, node22, e2e41, desktop5), plus UI contracts, workspace/desktop fmt and strict Clippy, JS syntax and real Codex0.161.0 routing/account/offline checks against isolated nodes/mock upstream. E2e41 completed in456.84seconds. Separate installed Claude Code2.1.227 peer mock probe passed. Final-source Linux x64 manual activation/restore/disconnect regression passed in an isolated network-disabled container; Windows node test binaries compiled and workspace/all-target Clippy passed. No native Windows execution or live inference claim. CodeGraph impact/affected/current status and Git whitespace checks passed. Installer verification and matching Downloads copies are recorded above. All requested fixes are included in both alpha.9 packages; the separate unspecified version message remains unconfirmed.


### Version 1.1.0 preparation verification

Final local `make test lint smoke` passed206 Rust cases, UI, three release identity cases, strict lint and isolated real-Codex mock routing/account/offline checks. All six PR platform CI jobs passed; the final documentation update is checked again before merge. Actionlint, resource resolution, release-note links, CodeGraph and whitespace checks passed. Prior installer evidence remains version-specific; release preparation does not relabel alpha packages, push the publishing tag, access real accounts or restart clients.


### Designed-flow completion — 2026-10-14

`activation.rs` now owns passive scheduling and status; the UI and Tauri shell delegate. The pending worker is weak-node/in-memory, polls static client metadata on a blocking worker, uses two closed samples and a final closure check, pins direction/route/connected account/installed state, and applies once under codex_change. Never call native apply against this coding chat while verifying. Fake probe/apply callbacks test waiting, cancellation, route changes and single use; the existing non-Mac isolated test covers real synthetic config mutation. Native app closure/reopen stays manual acceptance.

`PairPermissions` stays executor-local in InviteSlot/NumericInvite/PendingPair and is saved atomically with client approval. Desktop scoped invites explicitly start selected sharing and grant only their mask. Full and deterministic numeric e2e coverage tests both service grants before/after approval. Legacy CLI/control behavior remains; new metadata is not an upstream credential. Client-side checkboxes are presentation only.

The UI includes switch timing cards, waiting counts and closed labels, recovery options, timeline, countdown, readiness shortcut and numbered OAuth. Codex’s desktop reopening still means ChatGPT; terminals/editors never reopen. Manual mode never reopens apps. Later mode survives hiding the window, but not node restart. The preview harness uses fake timers only; production progress is node status. Final verification is recorded in STATUS.


### Iroh candidate — 2026-10-15

1.1.1-alpha.1 / router-core-v12-iroh adds a persistent private Iroh key, default n0 discovery/relay, and Iroh addresses in full invites. Preserve the certificate-pinned TLS layer; Iroh identity/address is transport only. All production peer HTTP paths share this adapter, including Claude and model discovery. Incoming sharing-off refusal, bounded streams/handshakes, epoch cancellation and existing per-request grants remain. Do not emit invitation secrets or Iroh private keys in diagnostics.

Eight digits stay nearby-only. Full codes are the cross-network UX; no Iroh signup/configuration button. Previously paired direct-only records need a new full-code pairing to acquire Iroh addressing. Default fixture nodes explicitly disable discovery/relay; optional public probe uses synthetic accounts/mock inference. Physical internet/relay-only acceptance is separate. Published 1.1.0 artifacts remain immutable; this new candidate includes the prior completed redesigned flows. No user clients/logins/settings were touched while building.

Selected Iroh candidate installer: `dist/macos/Aillies_1.1.1-alpha.1_iroh-redesign_aarch64.dmg`, SHA256 `c6b337408c1a78e48f033e7ac57c442f2f047b53362868d780ee576352abd191`; verified and copied to Downloads without installation. Public relay acceptance remains blocked by TLS access denial in this execution environment.

Final verification: `RUST_TEST_THREADS=4 make test lint smoke` exited 0. The initial full suite passed 214 Rust cases; the final added relay-hint unit and Iroh-only shutdown regression also passed, bringing unique verified coverage to 215 cases (Claude30, Codex62, core48, node27, e2e43, desktop5). UI contracts, three release-identity cases, workspace/desktop fmt and strict Clippy, JS syntax and isolated real-Codex mock routing/account/offline checks passed. Full e2e43 took 491.56 seconds. The optional public-infrastructure probe failed separately as documented; it is not included in passing default counts. Final source was rebuilt after the synchronous-Quit fix, packaged without installing/launching it, and copied byte-identically to Downloads. CodeGraph is current and Git whitespace checks passed.


### Alpha.2 relay fix

Use the explicit current production relay map in internet.rs. The old0.92 defaults contain retired extra-iroh hostnames; do not restore them or disable TLS checks. The alpha.1 blocked-network conclusion was incomplete: current official hosts are reachable and the public probe passed after this correction. Keep the local readiness boolean (no keys/addresses), full-code format on New code, and existing direct/private path. Public probe is still opt-in and uses fixture accounts/mock inference; physical cross-network acceptance remains separate. Final checks/package evidence follows in STATUS.

Alpha.2 Mac candidate: `dist/macos/Aillies_1.1.1-alpha.2_iroh-relay-fix_aarch64.dmg`, SHA256 `fcb7059bd36584da892a875bb4c8bd481a14576072e3aa4c4fbaa0d95193ac39`, 13414772bytes. Mounted checksum, deep/strict ad-hoc signature, executable/icon comparisons passed; byte-identical copy in Downloads. Apple Silicon/macOS13+, not notarized. No actual app installation/restart or account/config migration occurred. Final-source public Iroh full-code pairing/request/revocation probe passed again in 6.80seconds. Physical cross-network acceptance remains separate.

Alpha.2 final verification: `RUST_TEST_THREADS=4 make test lint smoke` exited 0 with 216 default Rust cases (Claude30, Codex62, core48, node28, e2e43, desktop5), UI contracts, three release-identity checks, workspace/desktop fmt and strict Clippy, JS syntax and isolated real-Codex mock routing/account/offline checks. E2e43 took 565.98seconds. The separate final-source public Iroh full-code pairing/request/revocation probe passed (6.80seconds), giving 217 verified Rust cases including that opt-in probe. Build/version identity, CodeGraph and Git whitespace checks passed. The packaged alpha.2 was not installed or launched; native user accounts/settings and the running alpha.1 app were preserved.


### Alpha.3 history migration

Decision66 authorizes partial preparation, never guessed/implicit account rerouting. Skip a whole thread group if any replica fails; originals and skipped index rows must remain unchanged. Required column capabilities, highest numbered database selection, pinned journal filenames and backward defaults govern recovery. Unknown newest indexes mean no history edits and a new-chat-only warning. SQLite schemas are never altered. Metadata provider fields and cloned settings only; message bytes remain original. Optional delimiter offset journals preserve valid unterminated records.

History diagnostics are private latest100 closed code/stage/hash/record/timestamp entries, redacted on load/export; do not add raw paths, IDs, contents or raw exceptions. Node export/status, activation reports, CLI and legacy restart carry pending counts. The UI warns to use a new Aillies chat until unsupported old histories are repaired; recent warnings remain four. All verification is synthetic; preserve the installed running app and actual history. Final check/package evidence follows in STATUS.

Native probe caught stale surviving cache offsets when an unindexed rollout switched and Codex rebuilt its index. A unique unindexed group now snapshots/shifts the surviving supported cache, while indexed replicas leave canonical offsets alone. Preserve this regression; unsupported caches block unindexed groups as well. The probe waits boundedly for native listing discovery and uses unreachable loopback only.

If history cache exists with no state database available for a safe attached transaction, preserve unindexed groups instead of guessing or creating a replacement state schema. Covered by orphaned_history_cache_preserves_unindexed_rollout.

Broad checks reproduced a rapid sharing off/on port-rebind race: aborting a listener schedules its destruction but did not await it. Async Codex/Claude sharing-off now awaits the aborted listener before returning; synchronous shutdown remains nonblocking. The existing Claude account/source/retry-binding end-to-end regression reproduces the rapid restart.

Final alpha.3 verification: `make test` passed227 default Rust cases (Claude30, Codex72, core48, node28, e2e44, desktop5), UI contracts and three release-identity cases. The final-source optional installed-Codex app-server list/resume/provider round-trip probe also passed (5.46seconds, isolated home/unreachable loopback, no live inference), giving228 verified Rust cases including that probe. Full e2e44 took394.47seconds. The combined run reached a desktop test-fixture spacing check; formatting was corrected and `make lint smoke` then passed, including both workspace/desktop strict Clippy, fmt, JS syntax and real-Codex mock routing/account/offline smoke. The only source edit after the selected release builds was whitespace in a cfg(test) fixture. Earlier failed port/native-cache probes are documented above and their regression repairs are included in both selected installers. CodeGraph and Git whitespace checks passed; release version identity is1.1.1-alpha.3. Native Windows installation and actual user-history migration remain separate acceptance; the installed app/settings/accounts/history were not changed.

Final omitted-effort source verification: `RUST_TEST_THREADS=4 make test lint smoke` exited0. Passed231 default Rust cases (Claude30, Codex72, core51, node28, e2e45, desktop5), UI contracts, three release-identity cases, workspace/desktop fmt and strict Clippy, JS syntax and real-Codex routing/account/offline smoke against synthetic nodes/mock upstream. Full e2e45 took388.49seconds. Focused14 compatibility tests and the two-node default-effort matrix also passed separately. No live inference, actual credentials/settings/history or installed apps were changed. CodeGraph and whitespace checks passed. Source publication includes the completed application changes on codex/onboarding-redesign; unrelated marketing/review-document work remains local. Previously delivered installers stay unchanged.
