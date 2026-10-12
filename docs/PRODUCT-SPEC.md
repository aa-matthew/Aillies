# Product scope and roadmap

Aillies lets Codex on one computer use the selected paired computer's local Codex login. The owning device reads its credentials and calls OpenAI; credentials remain there. This document separates the initial direct-network release gate from later milestones. [FEATURES.md](FEATURES.md) describes implemented behavior; [STATUS.md](STATUS.md) records what is actually verified.

## Initial direct-network release scope

One node per device can request and share simultaneously. Pair over LAN using eight digits or use a full code across a reachable private network. Both-device verification/approval establishes pinned certificate trust. Choose Normal or Aillies for Codex, select the execution device, and opt into Fast only when the sharing owner permits it. Keep provider controls and advanced details compact; do not expose credentials or protocol internals in normal use.

Requests have one requester-owned initial + three retry budget on the same device/account/model. Failed partial generation is suppressed; completed responses are returned with keepalives while waiting. Earlier attempts may consume usage. Cancel, scope changes and access/local validation refusals stop. No fallback or account rotation. The explicit image MCP adapter creates/saves complete PNGs with normal approvals; native image_gen/editing are outside that adapter's claim.

Existing-chat provider migration requires closed Codex clients and preserves original records through transactional config/file/index updates. Background preparation reports progress without a short production timeout. macOS may offer reviewed quit/switch/reopen, opening only ChatGPT. Windows/Linux close/reopen manually. Disconnect & quit attempts owned-settings cleanup independently, preserves other edits and offers Quit anyway for unsafe files; sharing is independent. Claude activation reviews affected clients and reopens only desktop apps on macOS.

## First-release acceptance gate

These are requirements to promote a tested direct-network release, not claims that every box is already complete. STATUS owns the checked/unchecked evidence.

- New users pair on the supported OS combinations, compare matching codes and approve on both devices. Wrong codes/certificates, expiry, reuse and revocation refuse. Windows/macOS network permissions and private-network full-code fallback are exercised physically.
- The selected account completes text and explicit MCP image creation with normal Codex approvals and a displayed saved PNG. Usage is charged to the sharing account; Fast entitlement and owner permission are verified.
- Offline, expired/changed login, cancellation and access revocation fail closed. Bounded retries remain on the same account/device/model, with no mixed partial output or nested retry budget and accurate uncertainty/usage notices.
- New/existing Codex chats work after provider switching; history, unrelated config and project/folder references survive. Recovery is tested; unknown schemas/unsafe paths refuse without partial application.
- Native tray/window, single instance, save dialogs, startup/WebView failures, sleep/wake and deliberate quit are exercised on each supported platform. Closing the window keeps the node running; deliberate quit does not strand Codex on a stopped provider.
- Loopback authentication/Host checks, pinned transport, credential isolation, escaped peer strings and content-free diagnostics/logging pass automated tests. Export contains only bounded metadata, never recovery/config secrets.
- `make test lint` and relevant mock smoke pass. Packages have explicit architecture/version/source provenance, verified payloads/checksums and accurate signing status. Stable public distribution normally requires the M8 signing/notarization work. The owner explicitly authorized `codex-stable-1.0` as a stable publication with unsigned/ad-hoc installers and pending acceptance disclosed (decision 51); this exception does not complete M8 or the remaining acceptance checks.

## Later milestones

| Milestone | Future work | Outside initial direct-network gate |
|---|---|---|
| M6 | Internet rendezvous and blind encrypted relay; remote short-code pairing, relay/private/manual path choices | No relay/tunnel service is implemented today |
| M7 | Keychain, Credential Manager and Secret Service; transactional import of legacy identities where compatible | Current secrets use private files; no automatic Swift-era import promise |
| M8 | Native signing/notarization, updater, autostart and stable installer lifecycle | Current Mac/Windows alpha packages disclose unsigned/ad-hoc status |
| M9 | Claude Code / desktop Chat provider and transport/config adapters | Explicit Mac OAuth Connect account, owned refresh and native Code/desktop gateway adapters included in 1.1.0 (experimental); live/physical acceptance pending |

A future relay must carry opaque peer-encrypted payloads and must never receive OAuth credentials or generation content. It may observe bounded transport metadata. Pairing identity, authorization, revocation and account selection must survive path changes; introducing a relay cannot silently expand retries or fallback policy.

## Historical proposal

The [original 0.9 migration proposal](history/PRODUCT-SPEC-2026-10-11.md) preserves the detailed relay/credential-store/import plan and early wireframes. Its proposed crate layout, pause controls, retry policy and all-features acceptance list were superseded by implementation and decisions 43–47. Treat it as design evidence, not current contributor instructions or implemented capabilities.

Official ChatGPT desktop account substitution, general-purpose VPN/proxying, OAuth refresh, load balancing and quota-based account routing remain outside the supported product scope.
