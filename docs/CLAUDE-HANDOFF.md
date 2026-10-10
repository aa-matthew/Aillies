# Peer Route handoff

Updated: 2026-10-10

## Product goal

Peer Route lets computer A use computer B's locally logged-in Codex OAuth account for text generation, streamed responses, reasoning events, and image generation. The user chooses a machine from a menu-bar or tray app and then keeps using Codex normally.

The intended everyday experience is as simple as choosing an audio output device:

1. Install Peer Route on both devices.
2. Pair them with a short, verified flow.
3. Choose the destination device.
4. Turn routing on.
5. Continue in Codex.

The selected peer owns the OAuth session and applicable usage. OAuth material stays on that peer. The requesting machine receives the generated response stream.

## Current product truth

### Working in the native macOS preview

- A SwiftUI menu-bar application builds and runs on Apple Silicon macOS 14+.
- Two Macs can be paired with a private `.peerrelay` file and a pinned TLS identity.
- The sharing Mac selects a local Codex `auth.json` and rereads it for every turn.
- The requesting Mac exposes `POST http://127.0.0.1:18322/v1/responses`.
- Text and image generation checks have been observed succeeding through the peer path.
- Responses can stream as native Responses SSE. A completed JSON response has a buffered compatibility conversion.
- Codex configuration installation and restoration are transactional and preserve unrelated configuration where the merger can prove the edit is safe.
- The peer owns the account and quota for the Responses path. The loopback bearer value is the pairing access key, not an OpenAI API key and not 9router.
- The app has a menu-bar surface, pairing, sharing, machine selection, diagnostics, a network relay, and experimental ChatGPT traffic investigation.

### Not working or not proven

- The official ChatGPT desktop app cannot currently substitute B's account while running locally on A.
- The ChatGPT network relay forwards encrypted traffic but retains A's ChatGPT login and quota.
- ChatGPT desktop conversations, image UI, cookies, and account state are not translated into the peer Responses path.
- No production public rendezvous or blind relay exists yet.
- Ubuntu and Windows native applications do not exist yet.
- The Swift build is ad-hoc signed and not notarized.
- Automatic updates, production installers, per-device host revocation, NAT traversal, and a complete two-Mac acceptance suite remain unfinished.

Never claim official ChatGPT peer-account support until the account boundary is independently verified. Do not silently fall back to A's account, another peer, or a direct OpenAI route.

## Repository map

```text
.
├── CLAUDE.md                         # Entry point for Claude
├── README.md                         # Project overview; includes old prototype history
├── docs/
│   ├── CLAUDE-HANDOFF.md             # This file
│   └── NEXT-VERSION-MIGRATION.md     # 0.9 product and architecture proposal
├── macos/
│   ├── Package.swift                 # Current Swift package
│   ├── README.md                     # Detailed setup, versions, and limitations
│   ├── CHATGPT-ADAPTER.md            # Experimental ChatGPT investigation
│   ├── Sources/PeerRoute/
│   │   ├── App.swift                 # SwiftUI app and menu-bar UI
│   │   ├── RouteStore.swift          # Main application state/orchestration
│   │   ├── Sharing.swift             # Pairing and sharing flows
│   │   ├── TLSBridge.swift           # Pinned peer transport
│   │   ├── TurnExecutor.swift        # Host-side Codex OAuth execution
│   │   ├── TurnWire.swift            # Responses/SSE wire handling
│   │   ├── TurnFailure.swift         # Sanitized failure mapping
│   │   ├── CodexConfig.swift         # Safe Codex provider merge/restore
│   │   ├── DesktopRelay*.swift       # Encrypted ChatGPT traffic relay
│   │   └── ChatGPT*.swift            # Experimental diagnostics/tracing
│   ├── Tests/PeerRouteTests/         # Native fixtures and integration tests
│   └── scripts/build.sh              # App bundle and ZIP build
├── server.mjs, public/, test/        # Older Node/SSH browser prototype
└── package.json                      # Tests for the old prototype
```

The Swift macOS application is the current implementation. Treat the root Node/SSH app as historical reference unless the user explicitly asks to modify it.

## Current protocol and trust boundaries

```text
Codex on A
    │ loopback Responses API
    ▼
Peer Route A
    │ pinned TLS, pairing access key
    ▼
Peer Route B
    │ reads B's local Codex OAuth at turn time
    ▼
chatgpt.com/backend-api/codex/responses
```

Current local endpoint:

```text
POST http://127.0.0.1:18322/v1/responses
Authorization: Bearer <pairing access key>
```

Supported request shape:

- `model`
- full conversation history in `input`
- forced `stream: true`
- forced `store: false`
- image generation through the hosted `image_generation` tool

Rejected or unsupported:

- server conversation references
- redirects and automatic retries
- account fallback
- direct fallback
- WebSocket Responses
- `/v1/images/generations`
- model discovery on the local endpoint

Resource limits currently include a 16 MiB request cap, 64 MiB streamed-output cap, and four concurrent generation requests.

The public-relay design in 0.9 must add end-to-end application encryption between A and B. A hosted relay may see operational metadata and opaque byte counts, but must never receive OAuth tokens, `auth.json`, pairing secrets, prompts, responses, or images in plaintext.

## 0.9 direction

Build one cross-platform tray-first application:

- Tauri 2 for windows, trays, lifecycle, and installers.
- Rust for device identity, pairing, discovery, transport, relay multiplexing, the loopback Responses server, upstream Responses/SSE, and safe configuration transactions.
- Tokio plus Axum or Hyper for asynchronous networking.
- Reqwest with rustls for upstream requests.
- SQLite for non-secret state.
- macOS Keychain, Windows Credential Manager, and Linux Secret Service for secrets.
- Small static TypeScript UI; avoid a frontend framework until the interface needs one.

An installation can send through another device and share its own account at the same time. Remove the old mutually exclusive “Use another Mac / Share this Mac” mental model.

Connection policy:

1. Direct LAN when available.
2. Existing private network address such as Tailscale, WireGuard, or ZeroTier.
3. User-configured self-hosted Peer Route relay.
4. Peer Route hosted blind relay.

`Auto` chooses the best usable path. A path change may reconnect transport but must never replay a prompt automatically or authorize a different device.

## Minimal UX direction

Design reference: [macOS 27 Community UI kit](https://www.figma.com/design/tTS5NlPIiYCNxPiJ6qNJ82/macOS-27--Community-?node-id=131-8996&p=f).

The latest interactive wireframe is stored outside the repository in the shared Codex visualization directory:

```text
/Users/mtbhust/.codex/visualizations/2026/10/09/01a1218c-4a49-7c23-b739-2a0dd4c3e639/peer-route-platform-wireframes.html
```

It contains macOS, Windows, and Ubuntu variants for the tray, home, onboarding, pairing, connection details, self-hosted relay, approved devices, Codex integration, and failures.

Visual direction:

- Use the macOS 27 kit as inspiration for rounded, layered system surfaces and restrained depth.
- Keep the app compact and lean. Reduce explanatory copy after onboarding.
- Use native platform placement: macOS menu bar, Windows notification area, Ubuntu AppIndicator.
- Preserve each platform's window controls and conventions instead of putting macOS chrome everywhere.
- Normal operation should fit in the tray popover.
- Advanced networking and diagnostics belong behind one details entry.
- Use three status colors only: green ready, gray paused/local, orange action required.

Target tray content:

```text
Peer Route                         ● Connected

Using
  Dailyngocss                         48 ms ›
────────────────────────────────────────────
Pause routing
Settings…
```

Target main window:

```text
● Connected
Dailyngocss · Tunnel · 48 ms

Use                 Dailyngocss                 ▾
Route Codex         On                          ●
Share this device   2 devices                   ●

                              Details  Pair device
```

Do not add dashboards, request history, decorative metrics, model fields on the home screen, proxy terminology, certificate setup, ports, bearer-token instructions, or generation-test controls to the everyday UI.

## Behavioral requirements

- New destination selection applies to new conversations. Existing conversation ownership must not be silently moved between accounts.
- Never silently retry a submitted prompt on another peer.
- Treat uncertain delivery as uncertain and require an explicit retry.
- Verify transport readiness and peer-account readiness separately.
- Pairing approval must identify both device identities; short codes are only a human verification aid.
- Secrets belong in platform credential stores with least-privilege file permissions for any unavoidable receipts.
- B must read and validate its local OAuth source at execution time. Missing, expired, unreadable, or intentionally changed accounts fail closed.
- Error messages exposed to A must be sanitized fixed categories and status numbers, never upstream response bodies or credential material.
- Keep prompts, reasoning, images, and OAuth tokens out of logs.
- Restoring Codex configuration must refuse to overwrite independent edits made after Peer Route installed its provider.

## Recommended next implementation slice

Do not begin with the public service or a complete UI rewrite. First create a shared Rust core while keeping the proven Swift path available for comparison:

1. Add a Rust workspace with `peer-core`, `peer-turns`, `peer-transport`, `peer-config`, and `peer-platform` crates.
2. Port the versioned request validation, SSE framing, sanitized failures, and configuration fixtures before moving UI code.
3. Define a versioned A↔B protocol with capability negotiation for text, reasoning events, and image generation.
4. Implement device identity and pairing records with explicit per-device revocation.
5. Add the loopback Responses server and a mock host executor; prove streaming and cancellation locally.
6. Add direct transport with pinned identities and no fallback/replay.
7. Wrap the shared core in a minimal Tauri tray shell on macOS.
8. Only after the local path is stable, add rendezvous and an opaque relay.
9. Add Windows and Ubuntu packaging after the shared core has platform-neutral tests.

Keep scope tight. A useful first cross-platform milestone is: two local app instances pair, A sends a mocked Responses turn to B, B streams text and an image fixture back, the tray shows the selected peer, and stopping B produces a clear failure without replay.

## Verification baseline

Validated on 2026-10-10:

```sh
cd macos
swift test
# 28 tests passed, 0 failures

cd ..
npm test
# 3 tests passed, 0 failures
```

Native build:

```sh
cd macos
bash scripts/build.sh
```

The automated upstream is mocked. Passing tests do not prove live OpenAI entitlement, exact quota accounting, public-relay security, or official ChatGPT desktop compatibility.

## Definition of done for the next release

- One tray-first app builds on macOS, Windows, and Ubuntu.
- Pairing works without SSH, AirDrop, or typing IP addresses.
- A user can select an approved online peer and route a new Codex conversation through it.
- Text, reasoning events, image generation, streaming, cancellation, and bounded failures are covered by shared fixtures.
- The selected peer's OAuth remains local to that peer.
- Direct and relayed paths share the same end-to-end authenticated session.
- No prompt replay or account fallback occurs during path failure.
- Per-device revocation works.
- Codex enable/restore is transactional on all platforms.
- Everyday UI matches the compact tray and main-window direction above.
- Official ChatGPT desktop peer-account support is still labeled unsupported unless separately proven.

