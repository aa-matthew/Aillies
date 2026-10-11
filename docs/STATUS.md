# Current status

**Version:** 0.9.0-alpha.12 · **Build marker:** router-core-v4-pairing · **Updated:** 2026-10-12.

This page is the current acceptance summary. [FEATURES.md](FEATURES.md) maps behavior to code; [HANDOFF-CODEX.md](HANDOFF-CODEX.md) contains operative constraints and coverage. The complete earlier verification reports are preserved in [dated history](history/STATUS-2026-10-11.md), including observations from older builds that do not prove current hardware acceptance.

## Current implementation

| Milestone | State |
|---|---|
| M1–M3: shared logic, Codex provider, device node/CLI | Implemented; current automated tests and mock smoke pass |
| M4: compact desktop/tray | Mac and Windows alpha.12 packages built/verified; current physical acceptance incomplete; Linux has earlier build evidence |
| M5: real two-device acceptance | Earlier text/explicit image passed Mac → Windows; current alpha.12 hardware, entitlement/accounting and interruption checks remain |
| M6: internet rendezvous/blind relay | Planned; direct LAN/private reachability required today |
| M7: OS credential stores | Planned; private files today |
| M8: signing/notarization/updater/autostart | Planned; current Mac ad-hoc, Windows unsigned |
| M9: Claude | Planned; UI Upcoming and protocol reservation only |

Eight-digit nearby pairing uses SPAKE2/AES-GCM, both certificate identities and both approvals; full codes preserve private-network compatibility. Requester-owned initial + three retries buffer each attempt until completion, with same device/account/model and no fallback. Provider migration is transactional and runs in the background without a production preparation cutoff. See [architecture](ARCHITECTURE.md) and decisions 43–47.

## Current verification

| Evidence | Result and scope |
|---|---|
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
- [ ] Exercise macOS reviewed quit/switch/reopen with save dialogs and larger histories; only ChatGPT reopens. Verify project/sidebar references and original messages survive. Windows/Linux use manual close/reopen.
- [ ] Exercise native window failures, sleep/wake, deliberate guarded Quit and later config edits/restoration on supported platforms.

The [first direct-network release gate](PRODUCT-SPEC.md#first-release-acceptance-gate) defines promotion requirements. Relay, OS stores and Claude are later milestones, not implemented release claims.

## Known limits

- LAN multicast is needed for eight digits; full codes still need direct/private reachability. No internet rendezvous/relay exists.
- Codex's private ChatGPT backend may drift. Recheck normalization/allowed tools with mock smoke after a Codex update; normal approvals remain required.
- Aillies does not refresh OAuth. The sharing device's Codex renews its login. Account changes fail closed until sharing is explicitly enabled again.
- Buffering delays generation display and cannot undo upstream work/usage from earlier attempts. Codex/executor/sharer do not add nested retries.
- CLI daemon and desktop use the same loopback port; run one node per device. Secrets are private files, not OS-store entries.
- Current hardware/runtime verification is narrower than automated source/package checks. No broad three-platform CI or native image_gen claim is made.

## Repository cleanup — 2026-10-11

Removed two unused Rust wrappers and two unused desktop-only enable/restore commands after source/CodeGraph caller review; progress-aware implementations and CLI/control methods remain. Removed eight unused icon outputs and changed the brand generator so it does not recreate them. Preserved all configured app/tray/window artwork, masters, lockfiles, fixtures/generator/probe, default instructions and mock preview.

Retired legacy sources and two investigation documents into a verified private backup; the complete 596 MiB working directory was moved intact, not discarded. The source-only backup has 70 files including pre-edit documents/assets. Historical product/status/handoff/feature/architecture snapshots and wireframes remain in docs/history. [ARCHIVES.md](ARCHIVES.md) explains recovery and publication boundaries. Decisions and changelog history were preserved.


### Final cleanup validation

`RUST_TEST_THREADS=4 make test lint` passed all 136 default Rust cases, UI behavior, fmt/Clippy and JS syntax; the 25 two-node cases completed in 90.81 seconds. `make smoke` passed using real codex-cli 0.161.0 against isolated nodes and a mock upstream. No live inference was submitted.

The desktop app rebuilt successfully. A separate verification-only Mac DMG passed checksum, mounted strict/deep signature and executable/ICNS comparisons: `dist/verification/Aillies_0.9.0-alpha.12_cleanup-check_aarch64.dmg`, SHA256 `a27be482dcf93fac06c08288da73eb85e08d46b7075d47dfd5f4699148f0d520`. This ad-hoc package is excluded from delivered release selection; native tray interaction was not exercised.

All 70 backup members match the per-file manifest; all 42 tracked legacy members match the intact archived working directory. Retained app/tray/UI/brand artwork matches its pre-cleanup bytes. Local Markdown file targets resolve, CodeGraph was refreshed after removals, and all three selected immutable release files match their recorded sizes/checksums. No current installer/source ZIP was replaced. Local validation receipts are under `dist/verification/`.

### Git ignore hygiene — 2026-10-12

Added scoped browser verification output, dependency/Python caches, environment values and Windows metadata exclusions. Checked 17 excluded paths and 14 retained source/template paths with Git's ignore matcher; all 12 existing browser artifacts are excluded and no tracked file matches the rules. No files were deleted or untracked. `RUST_TEST_THREADS=4 make test lint` passed all 136 default Rust cases, UI behavior, fmt/Clippy and JS syntax. Networking/configuration behavior did not change; no new smoke run was needed.
