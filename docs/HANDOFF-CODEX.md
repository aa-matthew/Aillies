# Current handoff — Codex 1.0

Read [AGENTS.md](../AGENTS.md) first. This is operative guidance for 1.0.0 (`router-core-v4-pairing`). [STATUS.md](STATUS.md) tracks current acceptance; [RELEASES.md](RELEASES.md) selects the shipped artifacts. Dated investigation and verification notes are retained in [history](history/HANDOFF-CODEX-2026-10-11.md); their earlier policy/coverage claims are not current instructions.

## Current behavior

- Requester owns initial + three retries, same selected device/account/model, with 500/1000/2000 ms backoff and 880-second total deadline. Buffer each attempt up to 64 MiB; only a completed winning response is exposed. Earlier attempts may consume usage. Cancel, route/account changes, validation and access refusals stop. Codex/executor/sharer retry budgets remain zero. Decision 45 supersedes earlier no-replay wording.
- Eight random digits bootstrap the full one-time invite through SPAKE2/AES-GCM bound to both TLS fingerprints. LAN mDNS carries public metadata only. Ten minutes / five PAKE executions; consume or invalidate on replacement/sharing off/shutdown/expiry. Full codes remain for private networks/older peers; compare and approve on both devices. Stored certificates authorize future access; OAuth never crosses pairing.
- Existing chats switch while Codex is closed using a journaled file/config/SQLite transaction. Include valid unindexed rollouts and indexed same-provider replicas; preserve transcript records and update provider settings/projection offsets. Background preparation has no production deadline. Do not skip chats or reintroduce two-second refusal. Recovery uses absolute offsets.
- macOS restart is an explicit, expiring, one-use reviewed action. Close eligible clients normally; open only ChatGPT. Unknown clients, changed inventories or unresolved recovery refuse. Windows/Linux remain manual. Never restart the active user chat while developing.
- Desktop exposes one provider transaction path, independent sharing and conditional Allow Fast. CLI/control retain simple enable/restore and pause/resume. Quit requires Normal. Claude Code is Upcoming.
- Explicit MCP image creation validates a complete PNG, saves one winner and returns a local Markdown reference. Native image_gen routing/editing are separate and not established by these checks. Never bypass approval to probe images.
- Diagnostics: three independent latest-100 rings (incoming, upstream, failures), fixed categories/attempt numbers only; latest four notices on Home. No prompts, outputs, images, tokens, upstream error bodies or private recovery receipts in logs/exports.

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

Latest full cleanup checks: 136 default Rust cases, UI behavior, workspace/desktop fmt/Clippy and JS syntax; real Codex CLI → two local nodes → mock smoke. Existing tray tests decode all three retained 44px icons. No new live inference or user process/config/history changes. Package/build evidence is recorded in STATUS.

Next acceptance: physical alpha.12 Windows install/WebView2/tray/firewall, numeric pairing on two real devices, macOS restart/save-dialog/large-history flow, live 5.6/image approval/display, Fast entitlement and correct account usage. Current live capability must be checked after updating both devices, with normal approvals.

Optional synthetic probes: `cargo test -p aillies-codex installed_codex_lists -- --ignored --nocapture`; `cargo test -p aillies-node --test e2e numeric_pairing_via_local_discovery -- --ignored --nocapture`. Neither should use the user's real account/history. No automatic rerun of old failed chats.

## Cleanup boundary

Unused wrappers and old desktop enable/restore IPC were removed after caller/graph inspection; progress-aware implementations and Node/control methods remain. Six unused tray variants and two unused app PNGs were removed; the generator now emits only configured/runtime outputs. Master artwork, both lockfiles, fixtures/generator/probe, default instruction text and mock.js remain.

Peer Route sources, investigation notes and the complete legacy working directory are privately preserved; see [archive index](ARCHIVES.md). Dated verification/product/design evidence remains under history. Do not publish the private archive or select old installers automatically.

## Codex stable publication

The owner explicitly authorized `codex-stable-1.0` (application 1.0.0) as stable with limitations disclosed. Decision 51 records the signing/promotion exception; no runtime invariant changes. The release workflow gates publishing on all selected platform checks/builds and attaches only explicit packages, tagged source and SHA256SUMS. Physical/live acceptance remains open; do not treat the stable label as new evidence.
