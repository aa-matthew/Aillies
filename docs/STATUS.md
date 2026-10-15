# Current status


**Release target:** 1.1.3 · **Latest published:** [1.1.2](https://github.com/aa-matthew/Aillies/releases/tag/v1.1.2) · **Build marker:** router-core-v22-settings-access · **Updated:** 2026-10-16.

Version 1.1.3 preserves valid multiline Codex settings and adds explicit settings write-access recovery for macOS, Windows and Linux. Local checks and synthetic recovery tests passed; native CI and publication are pending. [Direct downloads](RELEASES.md) target the new release; existing published assets remain unchanged.

This page is the current acceptance summary. [FEATURES.md](FEATURES.md) maps behavior to code; [HANDOFF-CODEX.md](HANDOFF-CODEX.md) contains operative constraints and coverage. The complete earlier verification reports are preserved in [dated history](history/STATUS-2026-10-11.md), including observations from older builds that do not prove current hardware acceptance.

## Historical 1.1.0 release preparation

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
| M4: desktop/tray and onboarding | 1.1.1 packages published for Mac, Windows and Linux; native three-platform CI passed; physical acceptance remains scenario-specific |
| M5: real two-device acceptance | Earlier text/explicit image passed Mac → Windows; owner reported Mac/Windows testing for 1.1.0; current detailed hardware, entitlement/accounting and interruption checks remain |
| M6: internet rendezvous/blind relay | 1.1.1 ships Iroh full codes and current relay hosts; public readiness probe passed; physical internet acceptance remains; internet short codes are unimplemented |
| M7: Aillies OS credential stores | Connected Claude OAuth uses private app-data files by owner request; OS stores for other secrets/platforms remain planned |
| M8: signing/notarization/updater/autostart | Planned; current Mac ad-hoc, Windows unsigned |
| M9: Claude | Connect account OAuth, private storage and refresh for Claude Code/desktop Chat included in 1.1.0 (experimental); live/physical acceptance pending |

Eight-digit nearby pairing uses SPAKE2/AES-GCM, both certificate identities and both approvals; full codes preserve private-network compatibility. Requester-owned initial + three retries buffer each attempt until completion, with same device/account/model and no fallback. Provider migration is transactional and runs in the background without a production preparation cutoff. See [architecture](ARCHITECTURE.md) and decisions 43–47.

## Current verification

| Evidence | Result and scope |
|---|---|
| 1.1.1 published release | [Release run 38719184644](https://github.com/aa-matthew/Aillies/actions/runs/38719184644), [main CI 38719107585](https://github.com/aa-matthew/Aillies/actions/runs/38719107585) and [PR CI 38717975961](https://github.com/aa-matthew/Aillies/actions/runs/38717975961) passed; 231 default Rust cases, UI/release checks, strict lint and isolated Codex mock smoke passed locally; package checks do not imply live acceptance |
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

- [ ] Install current 1.1.1 on physical Windows; exercise WebView2 startup, native tray, single instance, diagnostic export and Private-profile firewall prompts.
- [ ] Pair two physical Macs and a Mac/Windows pair using eight digits; check Local Network/multicast permission, wrong/expired code and both approvals. Verify full-code private-network alternative.
- [ ] Verify current text and explicit MCP image creation/first-reply display, including GPT-5.6 Sol and approval-review paths. Native image_gen routing/editing is not established by MCP success.
- [ ] Observe sharing-account usage and requesting-account isolation; exercise actual Fast entitlement and owner-permission changes.
- [ ] Exercise offline, sign-out/account change, revoke, interrupted response and cancellation on real devices; confirm at most four attempts on the same scope and accurate uncertainty notices.
- [ ] Exercise macOS reviewed Codex/Claude quit/switch/reopen with save dialogs and larger histories; only affected desktop apps reopen. Verify project/sidebar references and original messages survive. Windows/Linux use manual close/reopen.
- [ ] Exercise native window failures, sleep/wake, Disconnect & quit / Quit anyway and later config edits/restoration on supported platforms.

The [first direct-network release gate](PRODUCT-SPEC.md#first-release-acceptance-gate) defines promotion requirements. Iroh full-code relay support is included in 1.1.1; OS-store work remains a later milestone. Claude remains experimental. Published 1.1.0 and Codex 1.0.0 artifacts remain available.

## Known limits

- LAN multicast is needed for eight digits; new full codes include Iroh internet connectivity. Public n0 discovery/relay availability and network policy can affect reachability.
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


### Window/onboarding redesign — 2026-10-14

Adapted the owner's exported 18-screen design on `codex/onboarding-redesign`: Use/Share cards, role-based onboarding, invitation setup, approval/readiness, service-aware picker, activation review/progress, Claude Connect, paired-device review and incomplete cleanup. Editable name and four warnings/full diagnostics remain. Scheduled switching and pre-approval per-device service selection are omitted because the current node does not implement them. Protocol/auth/config/retry logic is unchanged. Existing documentation/marketing edits were preserved. `RUST_TEST_THREADS=4 make test lint` passed all 206 default Rust cases, synthetic UI behavior, three release identity checks, formatting, workspace/desktop Clippy and JS syntax. E2e completed in 386.04 seconds. Browser mock checks covered dark/light Home, 600×820 and 420×520 layouts (no horizontal overflow), first launch, setup/invite, grouped leading-zero code, verification/readiness, picker keyboard navigation and activation cancellation. Screenshots are local under `dist/verification/onboarding-redesign/`. CodeGraph and whitespace checks passed. No networking/executor/config change required a new smoke run; native/physical acceptance is not implied by these synthetic previews.


### Completed missing design flows — 2026-10-14

Implemented node-owned Later/Manual switching, waiting/cancellation and completion reveal, invitation-bound Codex/Claude grants for full/numeric codes, chat-count review and activation timeline, guided recovery, live countdown, post-pairing activation shortcut and numbered OAuth steps. Codex keeps ChatGPT-only reopening; Manual does not launch apps. No native accounts/settings/apps were changed during development. Scoped two-node and injected scheduler regressions pass; full final checks and browser evidence are recorded below after completion. Earlier omission notes describe the first layout pass, not this completed behavior.


Final designed-flow validation: `RUST_TEST_THREADS=4 make test lint smoke` passed all 211 default Rust cases (Claude30, Codex62, core48, node24, e2e42, desktop5), synthetic UI contracts, three release identity checks, strict workspace/desktop lint and real installed Codex → isolated two-node mock routing/account/offline checks. The final controller regression passed again after updating waiting metadata/cancellation. Browser mocks verified all three timing choices, delayed waiting and completion, service-bound invitation choices, an advancing countdown, numbered masked OAuth steps and no horizontal overflow at 420×520. Screenshots are in `dist/verification/onboarding-redesign/`. No real account/login/Keychain, user config/history, native client shutdown or live inference was exercised. Native dialogs, delayed actual desktop reopening and OS permission behavior still require physical acceptance.


## Iroh redesign candidate — 2026-10-15

Version 1.1.1-alpha.1 / router-core-v12-iroh includes all completed onboarding flows and built-in Iroh full-code connectivity. Iroh-only synthetic two-node pairing/request/revocation and wrong-certificate refusal passed; stable private identity, file permissions/symlink refusal, relay-hint parsing and updated UI contract passed. Credentials remain fixture-local and inference uses a loopback mock.

The optional public-discovery probe failed in this execution environment: all three configured production relay hosts resolved, but independent HTTPS probes returned TLS access denied. Iroh likewise reported failed HTTPS probes. Relay hints and bounded initial readiness mitigate publication races; direct/private-network full codes remain available when a relay cannot connect. No public-relay, physical cross-network or live inference acceptance is claimed. Update both devices and pair again with a full code for internet testing. Final check and installer evidence follows after completion.

Mac candidate built and sealed from final source: `dist/macos/Aillies_1.1.1-alpha.1_iroh-redesign_aarch64.dmg`; SHA256 `c6b337408c1a78e48f033e7ac57c442f2f047b53362868d780ee576352abd191`, 13414129 bytes. Mounted DMG integrity, deep/strict ad-hoc signature and exact executable/icon comparisons passed. A byte-identical copy is in Downloads. Apple Silicon/macOS13+, not notarized. Existing published installers and the running installed app were not replaced.

Final verification: `RUST_TEST_THREADS=4 make test lint smoke` exited 0. The initial full suite passed 214 Rust cases; the final added relay-hint unit and Iroh-only shutdown regression also passed, bringing unique verified coverage to 215 cases (Claude30, Codex62, core48, node27, e2e43, desktop5). UI contracts, three release-identity cases, workspace/desktop fmt and strict Clippy, JS syntax and isolated real-Codex mock routing/account/offline checks passed. Full e2e43 took 491.56 seconds. The optional public-infrastructure probe failed separately as documented; it is not included in passing default counts. Final source was rebuilt after the synchronous-Quit fix, packaged without installing/launching it, and copied byte-identically to Downloads. CodeGraph is current and Git whitespace checks passed.


## Iroh relay repair — 2026-10-15

Alpha.1's public-probe failure was traced to Iroh0.92's bundled retired relay hostnames, not a general inability to reach current Iroh infrastructure. Current official AP/NA/EU hosts returned HTTPS200; an explicit current-host relay map fixed the previously failing public full-code pairing/request/revocation probe (6.76seconds, synthetic account/mock backend). Alpha.2 / router-core-v13-relay-fix includes a real readiness warning and full-format New code. Prior failed probe evidence is preserved above; alpha.1 is superseded for internet pairing. Physical cross-network acceptance remains open. Final checks/package evidence follows.

Alpha.2 Mac candidate: `dist/macos/Aillies_1.1.1-alpha.2_iroh-relay-fix_aarch64.dmg`, SHA256 `fcb7059bd36584da892a875bb4c8bd481a14576072e3aa4c4fbaa0d95193ac39`, 13414772bytes. Mounted checksum, deep/strict ad-hoc signature, executable/icon comparisons passed; byte-identical copy in Downloads. Apple Silicon/macOS13+, not notarized. No actual app installation/restart or account/config migration occurred. Final-source public Iroh full-code pairing/request/revocation probe passed again in 6.80seconds. Physical cross-network acceptance remains separate.

Alpha.2 final verification: `RUST_TEST_THREADS=4 make test lint smoke` exited 0 with 216 default Rust cases (Claude30, Codex62, core48, node28, e2e43, desktop5), UI contracts, three release-identity checks, workspace/desktop fmt and strict Clippy, JS syntax and isolated real-Codex mock routing/account/offline checks. E2e43 took 565.98seconds. The separate final-source public Iroh full-code pairing/request/revocation probe passed (6.80seconds), giving 217 verified Rust cases including that opt-in probe. Build/version identity, CodeGraph and Git whitespace checks passed. The packaged alpha.2 was not installed or launched; native user accounts/settings and the running alpha.1 app were preserved.


## Per-chat history migration candidate — 2026-10-15

1.1.1-alpha.3 / router-core-v14-history preserves damaged/unsupported chat groups, stages compatible groups and reports partial completion explicitly. Schema capability checks replace the whole-folder generation blocker; journal recovery pins actual selected database names. Latest100 content-free history diagnostics include fixed reason/stage, hashed reference and record number, accessible in the existing authenticated export/History tab and four recent warnings. No actual user history/account/settings or running apps were changed during development. Focused synthetic preservation, old/new database, replica, null-field, offset, recovery and diagnostic privacy tests pass; final checks/installers follow below.

The installed-Codex isolated probe caught a surviving history-cache offset bug after a missing index row. Fixed unique unindexed cache updates and added a round-trip regression; the real Codex app-server probe then passed both provider directions, list/resume and retained transcript assertions (4.70seconds). An orphan cache without an available state index now preserves the rollout/cache unchanged with an unsupported-schema warning; its focused regression passed. No actual user history was used.

Broad checks reproduced a rapid sharing off/on port-rebind race: aborting a listener schedules its destruction but did not await it. Async Codex/Claude sharing-off now awaits the aborted listener before returning; synchronous shutdown remains nonblocking. The existing Claude account/source/retry-binding end-to-end regression reproduces the rapid restart.

Selected final alpha.3 packages (prior intermediate packages are not selected):

- `/Users/mtbhust/Downloads/Aillies_1.1.1-alpha.3_safe-history-final_aarch64.dmg` (13449125bytes), SHA256 `fc3a788565351ce034da347bc2f3bb2e87c4cbdb71fe2ee1b201117ab1a2c3a6`.
- `/Users/mtbhust/Downloads/Aillies_1.1.1-alpha.3_safe-history-final_x64-setup.exe` (8005214bytes), SHA256 `22af45546f2bf110cc59b7cee781a1553d14b84662559d2816408e286d1d5b9c`.

Mac Apple Silicon/macOS13+: release build, mounted DMG checksum, deep/strict ad-hoc signature, exact executable/icon comparisons passed; not notarized. Windowsx64: cargo-xwin/MSVC cross-build and NSIS archive integrity passed; extracted AMD64 PE/version/build marker, exact bundled executable apart from the three-byte Tauri UNK→NSS bundle marker and all12 license files/notices verified. Windows is unsigned and was not installed/executed natively. Both selected installers were copied byte-identically to Downloads; no actual installed app was replaced or launched.

Final alpha.3 verification: `make test` passed227 default Rust cases (Claude30, Codex72, core48, node28, e2e44, desktop5), UI contracts and three release-identity cases. The final-source optional installed-Codex app-server list/resume/provider round-trip probe also passed (5.46seconds, isolated home/unreachable loopback, no live inference), giving228 verified Rust cases including that probe. Full e2e44 took394.47seconds. The combined run reached a desktop test-fixture spacing check; formatting was corrected and `make lint smoke` then passed, including both workspace/desktop strict Clippy, fmt, JS syntax and real-Codex mock routing/account/offline smoke. The only source edit after the selected release builds was whitespace in a cfg(test) fixture. Earlier failed port/native-cache probes are documented above and their regression repairs are included in both selected installers. CodeGraph and Git whitespace checks passed; release version identity is1.1.1-alpha.3. Native Windows installation and actual user-history migration remain separate acceptance; the installed app/settings/accounts/history were not changed.


## Claude missing-effort default — 2026-10-15

Unreleased source adds medium output effort to generation/test requests that omit it, only for reviewed effort-capable models. Both client entry points and local/peer routes share the same adapter; explicit top-level/history directives and signed content remain intact. Token counting/discovery and unsupported/unknown model capabilities are unchanged. Focused compatibility and two-node checks precede full verification below. Prior alpha.3 installer hashes remain immutable; no new installer is claimed for this source patch.

Final omitted-effort source verification: `RUST_TEST_THREADS=4 make test lint smoke` exited0. Passed231 default Rust cases (Claude30, Codex72, core51, node28, e2e45, desktop5), UI contracts, three release-identity cases, workspace/desktop fmt and strict Clippy, JS syntax and real-Codex routing/account/offline smoke against synthetic nodes/mock upstream. Full e2e45 took388.49seconds. Focused14 compatibility tests and the two-node default-effort matrix also passed separately. No live inference, actual credentials/settings/history or installed apps were changed. CodeGraph and whitespace checks passed. Source publication includes the completed application changes on codex/onboarding-redesign; unrelated marketing/review-document work remains local. Previously delivered installers stay unchanged.


## UI/onboarding1.1.1 release — 2026-10-15

CI run38716470149 passed all three desktop jobs and macOS workspace tests. Windows/Linux workspace tests failed the same outdated empty-app-list assertion in manual activation; the redesigned preview intentionally names clients to close. The repaired regression checks manual mode, service labels and zero automatic terminal closures while retaining settings/replay/cleanup assertions. Version1.1.1 / router-core-v15-onboarding includes the completed redesign, guided/scoped pairing, Iroh relay correction, safe history migration and Claude medium-effort default. Publication is gated by [PR#3](https://github.com/aa-matthew/Aillies/pull/3) and the existing tag release workflow. Version-matched artifacts/checksums will be attached to [v1.1.1](https://github.com/aa-matthew/Aillies/releases/tag/v1.1.1); no earlier asset is replaced.


## Published 1.1.1 and README refresh — 2026-10-15

[PR3](https://github.com/aa-matthew/Aillies/pull/3) merged and [v1.1.1](https://github.com/aa-matthew/Aillies/releases/tag/v1.1.1) published after all native CI/release jobs passed. Current source is tagged at 6cb3efbeae90583d9b114d696ddf0d88a582ad9d. Release assets, checksums and the previous 1.1.0 release remain immutable.

README now presents current status, direct latest/previous desktop downloads, a routing diagram, fresh synthetic app previews and reused existing launch artwork/storyboard frames. No new HyperFrames film was rendered. Demo captures contain no real login, pairing secret or conversation. Ten distinct README download links (and every repeated asset URL in the catalog/curated notes) matched published GitHub release metadata; relative file links resolved. The GitHub Markdown preview loaded all ten images with no page-width overflow at 675px. Both media files decoded successfully; the H.264 preview is 960×540/24fps and exactly8seconds. Existing release descriptions were extended with direct downloads, preserving their original notes and asset lists. This does not add native/live acceptance.


## Windows window recovery candidate — 2026-10-15

1.1.2-alpha.1 / router-core-v16-window addresses the reported 1.1.1 tray-only startup with panic/window-failed events. All window creation runs on a serialized worker, Windows left-click opens the window, second launches coalesce, and startup callbacks defer safely. Off-screen title bars recenter; restore/geometry/focus problems are logged without suppressing show. Open Aillies remains available after node startup failure. Native Windows messages distinguish unavailable WebView2 from generic window creation failure and require Yes before opening Microsoft's runtime download. Accounts, pairings and provider configuration are unchanged.

Focused desktop tests passed before final checks. Full source checks and installer verification follow below. No native Windows UI execution or actual user-app/account/settings change is claimed.


Windows candidate built and statically verified: `dist/windows/Aillies_1.1.2-alpha.1_window-recovery_x64-setup.exe`, 8002684bytes, SHA256 `ecb4c34b921d99babdf4009a1b8545f33c8d7fab6270e60d1b2779152538a73c`. The byte-identical Downloads copy is `Aillies_1.1.2-alpha.1_window-recovery_x64-setup.exe`. Cargo-xwin/MSVC release build and NSIS integrity/extraction passed. Extracted AMD64 PE, exact candidate version/build marker, worker/native-dialog diagnostics and all12 license files/notices verified. Payload matches the release executable apart from Tauri's expected three-byte UNK→NSS bundle marker. Unsigned; native Windows tray/WebView2/save-dialog/monitor behavior still needs the friend's physical acceptance. Existing published assets were not replaced.


Final window-recovery verification: `RUST_TEST_THREADS=4 make test lint` exited0, passing235 default Rust cases (Claude30, Codex72, core51, node28, e2e45, desktop9), UI contracts, three release-identity checks, workspace/desktop formatting and strict Clippy. E2e45 completed in389.81seconds. New desktop regressions exercise concurrent-open coalescing, panic/failure gate release, disconnected/negative-coordinate monitor geometry and content-free stage codes. Windows-only native APIs compiled in the release cross-build; NSIS verification and identical Downloads checksum passed. No networking/executor/Codex-settings behavior changed, so no new smoke run was required. CodeGraph and Git whitespace checks passed. Native Windows UI execution remains the friend's next acceptance step.


## Windows alpha.1 physical failure / alpha.2 recovery candidate

The friend reported alpha.1 logs with node startup, worker entry, panic, and `window failed: create`; no successful window event. The availability probe passed, but that does not establish a healthy WebView2 environment. Alpha.1's fixes did not resolve this physical machine. Root cause remains unconfirmed.

Alpha.2 (router-core-v17-webview-profile) retains the worker/recovery fixes and selects a separate stable Aillies UI WebView profile, preserving old browser data and all node accounts/pairings/history. Only UI-local preferences may start fresh. It adds closed panic-component/line and native creation/HRESULT categories without raw messages, source paths or content. This is a conservative profile-isolation repair and improved evidence, not a claim of native acceptance. Final checks/package evidence follows.


Alpha.2 Windows candidate: `dist/windows/Aillies_1.1.2-alpha.2_webview-recovery_x64-setup.exe`, 7999816bytes, SHA256 `5afe02490371edcb8b457b9409c7250633157d347e55da189752de175449c7e0`. Identical Downloads copy and checksum sidecar saved. Cargo-xwin/MSVC release build, NSIS integrity/extraction, AMD64 PE/version/build marker, new UI-profile/panic/native-error code paths, exact binary comparison (expected three-byte bundle marker only) and12 license files/notices passed. Unsigned; native Windows window creation is not yet confirmed. Alpha.1 and published installers remain unchanged.


Final alpha.2 checks: `RUST_TEST_THREADS=4 make test lint` exited0 with236 default Rust tests (including10 desktop tests), UI contracts, three release-identity cases, formatting and strict workspace/desktop Clippy. The added diagnostic regression discards private compile paths and arbitrary native messages, exposing closed codes only. Windows release/NSIS verification and runtime-source/Downloads hashes match. CodeGraph and Git whitespace checks passed. No real credentials, native-client configuration, accounts, pairings or histories were changed during development. A clean UI profile is an isolation repair, not a demonstrated root cause; the friend's alpha.2 physical retest remains required.


## Windows alpha.2 physical failure / alpha.3 COM repair

Alpha.2 also failed on the friend's Windows PC. Its redacted panic location (`tao line=1145`) resolves to the RPC_E_CHANGED_MODE panic in pinned Tao0.37.1 Windows native file-drop initialization: OLE requires STA but the thread already has an incompatible COM apartment. This establishes the failing operation, not which earlier library selected the conflicting apartment. Browser-profile isolation did not resolve it.

Alpha.3 / router-core-v18-windows-sta establishes STA before startup, moves node/config initialization to a runtime worker and disables unused native/WebView file-drop handlers. A thread-bound guard balances successful COM initialization when the UI exits. No native account/settings/pairing/history edits or additional profile reset. The focused worker regression passed; full checks and Windows package evidence follow. Native physical acceptance remains pending.


Alpha.3 Windows candidate: `dist/windows/Aillies_1.1.2-alpha.3_windows-sta_x64-setup.exe`, 7978258bytes, SHA256 `91f0d6e03c4fa4907bbc9134fb6c37c630906c6c3023f4b6e5967859bb246596`. An identical copy and checksum are in Downloads. Cargo-xwin/MSVC release and Windows test-target checks passed, including type-checking the two Windows-only COM tests (not executing them). NSIS archive integrity/extraction, AMD64 PE/version/build marker, STA/import/profile markers, expected three-byte bundle patch and all12 license files/notices verified. Installer is unsigned. Previous installers remain unchanged; native Windows UI acceptance remains pending.


Final alpha.3 verification: `RUST_TEST_THREADS=4 make test lint smoke` exited0 with237 default Rust tests (Claude30, Codex72, core51, node28, e2e45, desktop11), UI contracts, three release-identity cases, formatting and strict workspace/desktop Clippy. E2e45 completed in489.92seconds. Real installed Codex routed through isolated synthetic nodes/mock upstream; selected-account and offline fail-closed smoke passed. Windows release build and cross-target test check passed; the two Windows-only COM tests were compiled, not run. NSIS verification, runtime-source hashes and identical Downloads checksum passed. CodeGraph index and Git whitespace checks passed. No real installed apps, native accounts/configuration, pairings or histories were changed. Physical Windows window creation remains pending on the friend's machine.


## Claude effective-effort diagnostics — source update

The owner reported unspecified effort despite the shipped medium default. Diagnostics had recorded only the incoming body. Upstream events now capture effort from the actual prepared Claude body before send; UI distinguishes incoming/upstream values and old logs show Not recorded. Two-node regression checks compare exported effort with the mock upstream body for local/peer, Code/Desktop, streaming/nonstreaming and missing/explicit effort. Persistence/privacy/backward-compatibility regression and UI assertions added. Final verification follows; existing alpha.3 installers remain their original Windows-repair source.


## Release target1.1.2 — sender defaults and visible internet pairing

The owner authorized commit/publication of sender-side medium enforcement, diagnostic clarity, Windows startup repairs and prominent internet pairing. Missing/null/literal unspecified Claude generation effort is filled before local/peer dispatch; outgoing diagnostics verify what the sender forwarded. The expanded two-node matrix passed for local/peer, Code/Desktop, streaming/JSON and missing/null/unspecified/explicit inputs. Pure tests cover all-model sender defaulting and exact signed-history bytes. Internet/full-code pairing is primary; Same private network selects eight digits. Source marker router-core-v20-sender-effort. Full local checks, native CI and publication evidence follow. Physical Windows UI and live Claude entitlement remain separate acceptance.

## 1.1.2 final source verification — 2026-10-15

`RUST_TEST_THREADS=4 make test lint smoke` passed: 239 default Rust tests (Claude30, Codex72, core52, node29, e2e45, desktop11), UI contract, three release identity tests, workspace/desktop formatting and strict Clippy, JS syntax, and isolated installed-Codex mock smoke. Sender/local/peer regressions verify medium reaches the sharing node before preparation, explicit effort remains intact and diagnostics retain incoming/outgoing/prepared values. CodeGraph sync/impact and whitespace checks passed. Synthetic browser previews at 420px and 600px confirmed Internet default, visible full-code entry/copy and Same private network. No real settings/accounts or installed app were changed. Native Windows COM tests run in CI; physical window behavior and live account acceptance remain unverified.

## Multiline TOML repair candidate — 2026-10-16

1.1.3-alpha.1 / router-core-v21-multiline-toml removes the blanket multiline-string rejection in transactional Codex settings. Preserve raw string content, comments and later edits; validate complete TOML and retain conflict guards. Regressions cover fake managed headers inside strings, escaped delimiters, closing quote runs, line continuations, nested values, invalid-input refusal and switching both ways with exact config restoration and unchanged history content. Permission denials from the client report remain separate. No real account, config, history, pairings or running app is modified by development verification. Final checks/package evidence follows.

Final verification (2026-10-16): `RUST_TEST_THREADS=4 make test lint smoke` exited 0: 241 default Rust cases (Claude30, Codex74, core52, node29, e2e45, desktop11), UI contracts, three release-identity tests, workspace/desktop fmt and strict Clippy, JS syntax and isolated installed-Codex mock routing/account/offline checks passed. The focused multiline config/history regressions passed; full e2e45 took390.93seconds. CodeGraph sync/impact and Git whitespace checks passed. Built Apple Silicon macOS13+ installer `dist/macos/Aillies_1.1.3-alpha.1_multiline-toml_aarch64.dmg`, SHA256 `5bb7f49574abf090e38297e156d864b251c87e791e7c4b078e36c543a3955ef8`; mounted checksum, strict/deep ad-hoc signature and executable/icon comparisons passed. Byte-identical Downloads copy verified. Not notarized, not installed or launched; physical friend-device acceptance and its separate permissions issue remain. Existing published installers remain unchanged.

## Cross-platform settings access recovery candidate — 2026-10-16

1.1.3-alpha.2 / router-core-v22-settings-access adds permission preflight, a bounded private Settings access diagnostic log and explicit native approval on macOS/Windows/Linux (decision73). Current-user repair runs first; only an observed denial permits escalation. Helper scope is fixed default settings metadata; custom paths/links/policy remain guarded. Main app and normal provider switching never run elevated. Config contents/history are not edited during repair. This candidate includes the multiline TOML fix. Final checks and physical authentication acceptance are recorded separately below.

Final access-recovery verification (2026-10-16): `RUST_TEST_THREADS=4 make test lint smoke` exited0 with247 default Rust cases (Claude30, Codex74, core52, node35, e2e45, desktop11), UI contract, release identity checks, strict workspace/desktop lint and isolated installed-Codex mock smoke. Final Node unit run passed36 including the added literal-shell-quoting regression, for248 unique verified cases; full e2e45 took389.47seconds. Windows workspace/all-target cross-check and strict Clippy passed; Linux x64 musl CLI built. These are cross-compilation results, not native Windows/Linux runtime execution. The extra Windows desktop cross-check was stopped after the workspace check to avoid blocking required host checks; it is not passing native desktop evidence. AppleScript approval code compiled without execution. CLI and bundled desktop helper entrypoints rejected malformed payloads with no node/UI/app-data initialization. Synthetic browser preview at420/600px and three-platform UI contracts verify explicit repair then review, without privileged OS calls. No real OS approval prompt, account or live settings were exercised. CodeGraph sync/impact and whitespace checks passed.

Mac candidate: `dist/macos/Aillies_1.1.3-alpha.2_settings-access_aarch64.dmg`, SHA256 `e62467f0db1b22164b9cc536a73c1192fefb1974f98d30d14f81a7b929aeb9ab`. Mounted checksum, strict/deep ad-hoc signature and executable/icon comparison passed; Downloads copy is byte-identical. Not notarized, not installed. The native authentication dialog and actual ACL/policy behavior on each supported OS remain physical acceptance; custom/default-path restrictions and Exit anyway remain. No commit, push or publication was requested for this follow-up.

Release authorization (2026-10-16): owner requested immediate publication after checks. Promote the verified alpha.2 changes to 1.1.3, run version-matched checks, open a PR and gate merge/publication on native CI. Preserve unrelated design work and all existing installers. Native privilege prompts have not been exercised on physical devices.
