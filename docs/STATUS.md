# Current status

**Release preparation:** 1.1.0 / codex/claude-stable-1.1 · **Build marker:** router-core-v11-review-fixes · **Updated:** 2026-10-12.

This page is the current acceptance summary. [FEATURES.md](FEATURES.md) maps behavior to code; [HANDOFF-CODEX.md](HANDOFF-CODEX.md) contains operative constraints and coverage. The complete earlier verification reports are preserved in [dated history](history/STATUS-2026-10-11.md), including observations from older builds that do not prove current hardware acceptance.

## 1.1.0 release preparation

The owner requested merge and preparation under `codex/claude-stable-1.1`. Version-matched manifests/locks, curated release notes, exact tag validation, flat archive names and desktop license resources are prepared. No publication tag is pushed during preparation. Prior alpha.9 verification and installers below remain their original versions; fresh packages come from the publishing workflow. Decision 61 records the boundary.

## Alpha.9 review repairs

Restore manual window activation and disconnect on Windows/Linux with current affected-client checks; macOS reviewed desktop-only reopening is unchanged. Hosted Claude bindings invalidate on consent/source/account changes, including off/on with identical old credentials, with a generation guard around async creation. Onboarding/pairing now describe both services and resolve provider statuses by ID. The reported additional old-version wording could not be reproduced without its exact message; compatibility/history checks remain intact.

## Alpha.8 redesign

Home now separates Aillies Use and Aillies Share. Codex/Claude activation and approved device dropdowns include own accounts; counts remain informational. Local account statuses, independent Share, conditional Allow Fast and separate pairing/grants are retained. Editable name/version/Diagnostics live in Connection details; four warnings follow it. Full latest 100 logs and export are in Diagnostics.

macOS plans review clients, close normally, change settings and reopen desktop apps only. Terminal/editor sessions stay closed. Disconnect & quit attempts all integration cleanups independently, preserving other edits. Unsafe files retain recovery material and expose Quit anyway. No full-profile equality blocker on cleanup.

Connected Claude tokens use private accounts/claude.json (0700 directory/0600 file, atomic rename and file/directory sync), with serialized refresh and pinned retries unchanged. Less protected than Keychain. Old Keychain selections require explicit reconnect; no automatic credential read/transfer occurs. Development checks use synthetic homes; the user's running clients and real settings/logins remain untouched.

## Current implementation

| Milestone | State |
|---|---|
| M1–M3: shared logic, Codex provider, device node/CLI | Implemented; current automated tests and mock smoke pass |
| M4: compact desktop/tray | Mac and Windows alpha.12 packages built/verified; current physical acceptance incomplete; Linux has earlier build evidence |
| M5: real two-device acceptance | Earlier text/explicit image passed Mac → Windows; current alpha.12 hardware, entitlement/accounting and interruption checks remain |
| M6: internet rendezvous/blind relay | Planned; direct LAN/private reachability required today |
| M7: Aillies OS credential stores | Connected Claude OAuth uses private app-data files by owner request; OS stores for other secrets/platforms remain planned |
| M8: signing/notarization/updater/autostart | Planned; current Mac ad-hoc, Windows unsigned |
| M9: Claude | Connect account OAuth, private storage and refresh for Claude Code/desktop Chat included in 1.1.0 (experimental); live/physical acceptance pending |

Eight-digit nearby pairing uses SPAKE2/AES-GCM, both certificate identities and both approvals; full codes preserve private-network compatibility. Requester-owned initial + three retries buffer each attempt until completion, with same device/account/model and no fallback. Provider migration is transactional and runs in the background without a production preparation cutoff. See [architecture](ARCHITECTURE.md) and decisions 43–47.

## Current verification

| Evidence | Result and scope |
|---|---|
| 1.1.0 cross-platform PR CI | [Run 38235371730](https://github.com/aa-matthew/Aillies/actions/runs/38235371730), source 0839797: all six macOS/Windows/Ubuntu workspace and desktop jobs passed, including native Windows tests and desktop builds; physical/live acceptance remains open |
| Activation/retry alpha.9 source checks | 206 default Rust cases, UI, strict lint and Codex mock smoke passed; separate Claude mock probe, Windows cross-compile/lint and Linux manual activation regression passed; native/live acceptance remains open |
| Aillies Use/Share alpha.8 source checks | 204 default Rust cases, new UI contract, strict lint, Codex mock smoke and installed-Claude mock probe passed; native/live acceptance remains open |
| Claude alpha.7 source checks | 200 default Rust cases, UI, strict lint, isolated real-Codex mock smoke and separate installed-Claude peer probe passed; native/live frontier acceptance remains open |
| Claude alpha.6 source checks | 186 default Rust cases, UI, strict lint, isolated real-Codex mock smoke and separate installed-Claude mock probe passed; native/live catalog acceptance remains open |
| Claude alpha.5 source checks | 179 default Rust cases, UI behavior, workspace/desktop fmt/Clippy, JS syntax and isolated real-Codex mock smoke passed; installed Claude Code mock probe passed separately |
| Claude alpha.2 source checks | 160 default Rust cases, UI behavior, fmt/Clippy, JS syntax and isolated real-Codex mock smoke passed; installed-Claude mock probe passed separately |
| Alpha.12 source checks | 136 default Rust cases (core32, provider56, node18, e2e25, desktop5), UI behavior, fmt/Clippy and JS syntax passed |
| Local multicast probe | One explicit synthetic-home test passed; 137 verified Rust cases for alpha.12. Same-host discovery does not establish physical firewall/permissions |
| Real Codex → two-node mock | codex-cli 0.161.0 smoke passed; selected mock account and offline fail-closed checks |
| Mac package | Apple Silicon macOS13+; mounted signature/checksum/executable/icon comparison passed; Bonjour/Local Network purpose verified |
| Windows package | x64 NSIS integrity/extraction, PE64 version and expected three-byte Tauri bundle patch verified; licenses match; no physical install claim |
| Earlier live evidence | Older build: pinned Mac → Windows text and explicit MCP PNG creation/display path, earlier Windows diagnostic export; usage/Fast and current model acceptance remain unverified |
| Cleanup verification | 136 default Rust cases, UI, lint, mock smoke and desktop rebuild pass. Retained icon bytes preserved; three 44px tray states decode. Mounted package, Markdown links, archive manifests and immutable release hashes also pass |

Current shipped artifact selection and exact checksums are in [RELEASES.md](RELEASES.md). Cleanup does not replace delivered installers or their immutable source snapshot. Older installers/provenance remain stored but are excluded from current selection.

## Remaining acceptance

Run with normal approvals after updating both devices; record build/OS and new evidence here.

- [ ] Install alpha.12 on physical Windows; exercise WebView2 startup, native tray, single instance, diagnostic export and Private-profile firewall prompts.
- [ ] Pair two physical Macs and a Mac/Windows pair using eight digits; check Local Network/multicast permission, wrong/expired code and both approvals. Verify full-code private-network alternative.
- [ ] Verify current text and explicit MCP image creation/first-reply display, including GPT-5.6 Sol and approval-review paths. Native image_gen routing/editing is not established by MCP success.
- [ ] Observe sharing-account usage and requesting-account isolation; exercise actual Fast entitlement and owner-permission changes.
- [ ] Exercise offline, sign-out/account change, revoke, interrupted response and cancellation on real devices; confirm at most four attempts on the same scope and accurate uncertainty notices.
- [ ] Exercise macOS reviewed Codex/Claude quit/switch/reopen with save dialogs and larger histories; only affected desktop apps reopen. Verify project/sidebar references and original messages survive. Windows/Linux use manual close/reopen.
- [ ] Exercise native window failures, sleep/wake, Disconnect & quit / Quit anyway and later config edits/restoration on supported platforms.

The [first direct-network release gate](PRODUCT-SPEC.md#first-release-acceptance-gate) defines promotion requirements. Relay and OS stores remain later milestones. Claude remains experimental in the prepared 1.1.0 release. The published Codex 1.0.0 release remains available.

## Known limits

- LAN multicast is needed for eight digits; full codes still need direct/private reachability. No internet rendezvous/relay exists.
- Codex's private ChatGPT backend may drift. Recheck normalization/allowed tools with mock smoke after a Codex update; normal approvals remain required.
- Codex and explicit legacy Claude native readers do not refresh their source logins. Alpha.5 renews only Aillies-owned connected Claude OAuth; uncertain refresh requires reconnect. Account changes fail closed until sharing is explicitly enabled again.
- Buffering delays generation display and cannot undo upstream work/usage from earlier attempts. Codex/executor/sharer do not add nested retries.
- CLI daemon and desktop use the same loopback port; run one node per device. Aillies secrets remain private files; connected Claude tokens are private files; legacy native readers run only when explicitly selected.
- Current hardware/runtime verification is narrower than automated source/package checks. Current 1.1.0 source passed all six platform CI jobs; native image_gen and physical/live acceptance remain unverified.

## Repository cleanup — 2026-10-11

Removed two unused Rust wrappers and two unused desktop-only enable/restore commands after source/CodeGraph caller review; progress-aware implementations and CLI/control methods remain. Removed eight unused icon outputs and changed the brand generator so it does not recreate them. Preserved all configured app/tray/window artwork, masters, lockfiles, fixtures/generator/probe, default instructions and mock preview.

Retired legacy sources and two investigation documents into a verified private backup; the complete 596 MiB working directory was moved intact, not discarded. The source-only backup has 70 files including pre-edit documents/assets. Historical product/status/handoff/feature/architecture snapshots and wireframes remain in docs/history. [ARCHIVES.md](ARCHIVES.md) explains recovery and publication boundaries. Decisions and changelog history were preserved.


### Final cleanup validation

`RUST_TEST_THREADS=4 make test lint` passed all 136 default Rust cases, UI behavior, fmt/Clippy and JS syntax; the 25 two-node cases completed in 90.81 seconds. `make smoke` passed using real codex-cli 0.161.0 against isolated nodes and a mock upstream. No live inference was submitted.

The desktop app rebuilt successfully. A separate verification-only Mac DMG passed checksum, mounted strict/deep signature and executable/ICNS comparisons: `dist/verification/Aillies_0.9.0-alpha.12_cleanup-check_aarch64.dmg`, SHA256 `a27be482dcf93fac06c08288da73eb85e08d46b7075d47dfd5f4699148f0d520`. This ad-hoc package is excluded from delivered release selection; native tray interaction was not exercised.

All 70 backup members match the per-file manifest; all 42 tracked legacy members match the intact archived working directory. Retained app/tray/UI/brand artwork matches its pre-cleanup bytes. Local Markdown file targets resolve, CodeGraph was refreshed after removals, and all three selected immutable release files match their recorded sizes/checksums. No current installer/source ZIP was replaced. Local validation receipts are under `dist/verification/`.

### Git ignore hygiene — 2026-10-12

Added scoped browser verification output, dependency/Python caches, environment values and Windows metadata exclusions. Checked 17 excluded paths and 14 retained source/template paths with Git's ignore matcher; all 12 existing browser artifacts are excluded and no tracked file matches the rules. No files were deleted or untracked. `RUST_TEST_THREADS=4 make test lint` passed all 136 default Rust cases, UI behavior, fmt/Clippy and JS syntax. Networking/configuration behavior did not change; no new smoke run was needed.

### First GitHub CI repair — 2026-10-12

[Run 38158461286](https://github.com/aa-matthew/Aillies/actions/runs/38158461286) on the first published alpha.12 commit exposed two independent issues: Rust 1.99 rejects the public unit error/redundant OS match, and Windows automatic CRLF checkout changes the embedded default instructions, failing the exact 9router request fixture. The local stable compiler had been Rust 1.97.

Return a fixed, data-free body-read error without changing caller failures or caps, use the OS constant directly, and pin only the embedded instruction file to LF via `.gitattributes`. A synthetic `core.autocrlf=true` checkout reproduced CRLF before the rule and preserves exact LF bytes after it. Full local checks passed all 136 default Rust cases, UI behavior, fmt/Clippy and JS syntax; routing smoke passed with real Codex and mock upstream. Workspace all-target Clippy also passed with installed Rust 1.99. Follow-up Actions runs are authoritative for current cross-platform CI; these checks do not establish physical-device acceptance.

### Codex stable 1.0 preparation — 2026-10-12

The owner explicitly authorized stable publication as `codex-stable-1.0` with limitations disclosed (decision 51). Application manifests and both lockfiles use 1.0.0. Runtime behavior remains the reviewed alpha.12 core plus the CI fixes. Signing/notarization and physical/live acceptance above remain incomplete. The release workflow validates versions, runs platform checks/builds, then publishes explicit packages, source and SHA256 checksums in one final job. Curated notes are in [releases/codex-stable-1.0.md](releases/codex-stable-1.0.md).

Final local 1.0.0 validation: `RUST_TEST_THREADS=4 make test lint smoke` passed all 136 default Rust cases, UI behavior, workspace/desktop formatting and Clippy, JS syntax, and real codex-cli 0.161.0 → isolated two-node mock routing/offline checks. No live inference was submitted. The release workflow passed actionlint 1.7.7 and Git whitespace checks. Cross-platform release packages are produced/checked by the tag workflow; these local results do not complete physical/live acceptance.


### Superseded Claude alpha.1 candidate - 2026-10-12

This API-key-only candidate was rejected by the owner and is superseded by the account-authentication correction below. Its checks and artifact are retained as historical evidence, not the current installer.

Added native Messages/token counting, API-key ownership on the executor, separate Claude destinations/sharing/grants, target-bound transactional settings receipts and a compact desktop/tray entry. Shared retry ownership, terminal framing and diagnostics retain Codex coverage. Existing approvals migrate as Codex-only; Claude-only pairing does not implicitly grant Codex. No real login, history, running client or installed application was changed during development.

Final `RUST_TEST_THREADS=4 make test lint smoke` passed all 151 default Rust cases (core35, Codex56, Claude7, node19, e2e29, desktop5), UI behavior, workspace/desktop formatting and Clippy, JS syntax and real codex-cli 0.161.0 routing through isolated nodes and a mock upstream. The first full run caught a stale Codex-only log-label assertion; its provider-neutral replacement passed both the focused privacy check and full rerun. Installed Claude Code 2.1.227 also passed the opt-in two-node mock probe: native completion, HTTP401, HTTP503, interrupted SSE and native SSE API errors stayed within the single initial-plus-three retry budget. Authentication/permission/validation SSE refusals and cancellation are covered by default e2e cases. No live inference was submitted.

Rebuilt the Apple Silicon macOS13+ desktop app and packaged `dist/macos/Aillies_1.1.0-alpha.1_claude-integration_aarch64.dmg`. Mounted strict/deep signature, executable/icon comparisons and checksum verification passed; SHA256 `1e652499d9fb0e12d49f2e3477cd67bbfd2250235e96cff0930171135d95c95f`. This is an ad-hoc signed, unnotarized experimental package, separate from the delivered Codex stable release. Mock settings received a browser visual check; the installed Aillies app and real user settings/history were untouched. Physical devices, live Anthropic betas/models/billing, real tool use and long signed-thinking sessions remain unverified. [CLAUDE.md](CLAUDE.md) documents setup and boundaries.


### Claude account correction — 2026-10-12

The owner rejected the API-key-only interpretation. Alpha.2 reads the executing device’s signed-in Claude login from native macOS Keychain / credential files, or an explicitly configured OAuth token in the original settings environment. Follow OmniRoute’s Bearer auth and required OAuth/Claude Code beta headers, preserve the actual client User-Agent and native Messages bytes. No refresh, identity cloaking, credential transfer or fallback. Private native account identity permits same-account token rotation between turns; every turn pins the token across retries. Alpha.1 sharing consent resets on upgrade while pairings/grants survive. The UI replaces key entry with account readiness. Decision 53 supersedes decision 52’s authentication restriction.

First alpha.2 package (superseded by r2 below): `dist/macos/Aillies_1.1.0-alpha.2_claude-account_aarch64.dmg`, SHA256 `9bf75e999747ffa01e9aa8769030575023a296a2f4f101a6c203d223cf6b765f`. Mounted checksum, strict/deep signature and exact executable/icon comparisons passed. Ad-hoc signed and unnotarized; no installed app was replaced. The corrected mock settings screen was visually checked (`dist/verification/claude-account-settings.jpg`). The installed Claude Code 2.1.227 synthetic two-node probe passed native completion and refusal/interruption/error retry cases, asserting owner OAuth Bearer authorization and no API-key header. No real Keychain, login, history or settings was used during verification. Actual Keychain permissions, live Anthropic entitlement/betas/usage/tools and physical-device acceptance remain open.


Final alpha.2 package includes isolated CLI smoke roots: `dist/macos/Aillies_1.1.0-alpha.2_claude-account-r2_aarch64.dmg`, SHA256 `0dd13331ed9e9c7f582337e5300cd29031d887410895786bc3a2eec83b027cf0`. Rebuild and mounted checksum/signature/executable/icon checks passed. The first full run passed tests/lint but was stopped before smoke node startup when review found the Codex smoke also needed isolated Claude storage. The corrected harness sets explicit test homes (native Keychain disabled) and asserts unavailable Claude accounts before Codex routing. Earlier candidate installers remain preserved and are not the current selection.

Final `RUST_TEST_THREADS=4 make test lint smoke` passed all 160 default Rust cases (core35, Codex56, Claude14, node20, e2e30, desktop5), UI behavior, workspace/desktop fmt/Clippy and JS syntax. Real codex-cli 0.161.0 completed selected-account routing and offline fail-closed checks against synthetic nodes/mock inference, with both Claude accounts explicitly isolated/unavailable. The separate installed Claude Code 2.1.227 probe passed. CodeGraph was refreshed and `git diff --check` passed. No live inference was submitted.


### Claude desktop Chat — alpha.3 candidate

Desktop Chat now has its own native gateway configuration and direct reuse of an existing inference OAuth cache on the sharing Mac. No relogin UX, cookies, token mint/refresh or Code fallback. Native login/history remains unchanged; gateway chats are local and separate from normal cloud history. Claude must be closed to switch on Mac; managed/bootstrap config refuses. UI distinguishes clients, uses a real Share action, preserves errors and shows the selected destination. Desktop turns use a 240-second budget; Code retains 880 seconds.

Three new two-node/local fixture tests pass, covering owner-cache preservation, explicit grants, absent/expired/account-changed cache, between-turn rotation and retry stop. Unit tests cover observed cache crypto/selection and reversible native config. Actual Keychain permission, native desktop startup/gateway request and live Anthropic entitlement remain unverified. A normal desktop web-only login may lack an inference cache; this candidate refuses instead of claiming it can share every login. Direct desktop credential reuse is Mac-only.


Final alpha.3 verification: all 170 default Rust cases passed (Claude21, Codex56, core35, node20, e2e33, desktop5), plus UI behavior, workspace/desktop fmt and strict Clippy, JS syntax, and isolated real codex-cli 0.161.0 mock smoke. The separate installed Claude Code 2.1.227 two-node mock probe passed. The first combined run exposed a desktop IPC call to private `snapshot`; that wiring was corrected, then affected desktop/UI checks, all lint and smoke passed. Desktop fixture tests were rerun after adding guarded-Quit coverage. No live inference, real cache/Keychain access, user config/history changes or running-app replacement occurred.

Selected experimental installer: `dist/macos/Aillies_1.1.0-alpha.3_claude-desktop_aarch64.dmg`, SHA256 `6851ea7b6bfd0b256a774b1e5316b2d184bae0b0a2e1c5a28279776bdf06c8d5`. Apple Silicon / macOS13+; release build, mounted DMG checksum, deep/strict ad-hoc signature, exact executable and ICNS comparisons passed. The package includes updated crypto notices. It is not notarized. Prior installers and the delivered Codex stable release are preserved. UI mock selection/sharing/device status was visually verified (`dist/verification/claude-desktop-alpha3.png`); this is not proof of native desktop startup/live account acceptance. CodeGraph was refreshed and whitespace checks passed.


### Local Claude sharing errors and selection stability — alpha.4

The reported screenshot selects Claude Code, Provider Normal, This device, with sharing off and a generic invalid-login message. It does not establish a desktop-cache or paired-device failure. Exact native cause is unknown: no real account/settings/Keychain was inspected. Code previously collapsed gateway/API-key/backend/token/settings failures into one message, and status polling replaced focused native dropdowns. The latter is a reproducible UI defect, not proof it caused this user's selection.

Share now reports a fixed local reason and the selected client, with no credential/settings values. Request-path failures retain their existing fixed codes and refusal behavior. Invalid Code settings never fall back to native Code or desktop credentials; an explicit Desktop Chat action changes the source before a new Share attempt. UI shows Not shared for unchecked accounts, preserves active pickers across polling and displays one persistent Share error. Both Claude credential sources are left untouched by status polling while sharing is off. Synthetic tests cover each settings category, redaction, selection preservation and Code-gateway refusal with independent desktop sharing. Native desktop/Keychain/live acceptance remains pending.


Alpha.4 package: `dist/macos/Aillies_1.1.0-alpha.4_account-checks_aarch64.dmg`, SHA256 `e280b1fec3c293e999f925ee4f337654deb197ae46d3ae300f261d69cf48f88d`. Release build and mounted DMG checksum, deep/strict ad-hoc signature, executable and icon comparisons passed. Previous installers remain intact. The isolated installed Claude Code 2.1.227 mock probe passed again; no live account or native Keychain was used. The UI mock verifies Desktop Chat defaults, explicit sharing source and Not shared state (`dist/verification/claude-account-check-alpha4.png`). Real desktop cache/Keychain/live request acceptance is still pending; the build does not claim to repair the user's unknown credential problem.


Final alpha.4 validation: `RUST_TEST_THREADS=4 make test lint smoke` completed successfully with 171 default Rust tests (Claude22, Codex56, core35, node20, e2e33, desktop5), UI regressions, strict workspace/desktop lint, JS syntax and real-Codex-to-mock smoke. The separate installed-Claude mock probe passed. CodeGraph and whitespace checks were refreshed. All credential tests used synthetic roots; no actual Keychain/login/history/settings or running app was changed. This verifies the feedback/selection fix and refusal boundaries, not the user's unknown native credential failure.


### Claude Connect account — alpha.5

Owner authorized explicit OmniRoute-style OAuth onboarding instead of depending on the unknown desktop-cache failure. Connect on Mac through browser PKCE/manual one-time code, save in Aillies’ own Keychain, then explicitly share/allow devices. Both native clients use the selected connected account. Native logins/history stay untouched; no automatic native/API/account fallback. Serialized pre-turn refresh persists rotated credentials; uncertainty blocks until reconnect; retries stay pinned. Windows/Linux local account connection remains unavailable. Native browser/Keychain access, physical desktop load, live Anthropic entitlement/usage and tools remain unverified. Verification/package results follow after final checks.

Final alpha.5 validation: `RUST_TEST_THREADS=4 make test lint smoke` passed with 179 default Rust tests (Claude27, Codex56, core35, node20, e2e36, desktop5), UI behavior, strict workspace/desktop lint and JS syntax. The installed Claude Code 2.1.227 mock probe passed separately. Synthetic tests exercise PKCE/state/expiry/cancellation, missing identity bootstrap, scope refusal, private storage, concurrent refresh, rotation/uncertain-refresh refusal, pinned retries, both client routes, peer grants and redacted diagnostics. The UI requires Connect before local provider/destination activation and preserves a pending authorization form across polling. Browser mock Connect/Disconnect and connected state were visually verified; no real authorization or Keychain was used.

Experimental Apple Silicon installer: `dist/macos/Aillies_1.1.0-alpha.5_claude-oauth_aarch64.dmg`, SHA256 `1903bbf4337b71dd1c743dd22aae412c56121ff3cf13c2b7754afc23f4fd84cb`. Final release rebuild, mounted DMG checksum, deep/strict ad-hoc signature, executable and icon comparisons passed. It is not notarized. Prior installers remain intact. UI evidence: `dist/verification/claude-connect-form-alpha5.png` and `claude-connected-alpha5.png` (mock only). Actual browser authorization, native Keychain access, desktop gateway startup and live Anthropic account/model/tool acceptance still need user testing. No user login/history/settings or running application was changed.

### Default Claude model discovery — alpha.6

Native GET model catalogs now route for both clients, with selected-account OAuth, peer grants, validated pagination and existing pinned retries. Desktop discovery is enabled by default; exact owned profiles automatically upgrade through a pending receipt, without a new button. Edited profiles refuse without mutation. Final verification and package evidence follow below. Native picker loading and live OAuth catalog access remain unverified.

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


### Codex/Claude 1.1.0 preparation verification — 2026-10-12

Final `RUST_TEST_THREADS=4 make test lint smoke` exited0 on version1.1.0:206 default Rust cases (Claude30, Codex60, core48, node22, e2e41, desktop5), UI contracts, three release-identity regression cases, workspace/desktop fmt and strict Clippy, JS syntax and real Codex0.161.0 against isolated two-node/mock routing, account and offline fail-closed checks. E2e completed in473.70seconds. Release validation accepts the exact owner tag with flat archive names; actionlint, desktop resource resolution, release-note links, CodeGraph sync and Git whitespace checks passed. PR CI run38235371730 passed all six native platform workspace/desktop jobs. The final evidence/documentation update is checked again before merge. No live inference, real login/settings/history or running-user-client changes. No publishing tag or new release installer was produced; prior packages remain immutable.
