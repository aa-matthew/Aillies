> Historical snapshot before the 2026-10-11 cleanup. Later decisions and verification supersede earlier statements. Use [current PRODUCT-SPEC](../PRODUCT-SPEC.md) for operative guidance.

> **Aillies product spec** (formerly the Peer Route 0.9 migration plan). This is the full target. What is built today is tracked in [STATUS.md](../STATUS.md); implementation choices that differ from this text are logged in [DECISIONS.md](../DECISIONS.md) (for example: identity is a per-device certificate, ports are 18522/18544, the UI is plain HTML/JS).

# Aillies 0.9 migration

Status: proposed migration plan after macOS 0.8.3.

## Outcome

Aillies 0.9 becomes a tray-first application for macOS, Ubuntu and Windows. A user chooses a device and keeps using Codex normally. Aillies chooses the best available path:

1. direct LAN connection when both devices can reach each other;
2. an end-to-end encrypted public tunnel when direct access is unavailable;
3. no silent fallback to the requesting device's account.

The selected peer owns the Codex OAuth session and applicable usage. The public relay carries encrypted bytes only. It never receives the peer's OAuth credentials or plaintext generation content.

This release targets the working Codex Responses route. Official ChatGPT desktop account substitution remains outside the 0.9 claim until its application protocol and account boundary are independently verified.

## Minimal product model

The 0.8 role switch is removed. Every installation can send through another device and share its own account at the same time.

The product has four concepts:

- **Device:** a paired Mac, Ubuntu computer or Windows computer.
- **Destination:** the device selected to execute new requests.
- **Path:** direct or public tunnel, selected automatically by default.
- **Sharing:** whether this device accepts requests from approved devices.

The user should not need to understand ports, proxy URLs, certificates, bearer keys, SSE, model adapters or relay regions during ordinary use.

## Everyday UX

The tray is the primary interface:

```text
Aillies                         ● Connected

Send through
  ○ This device
  ● Dailyngocss                    48 ms

Path                               Auto · Tunnel
Routing                            On
──────────────────────────────────────────────
Open Aillies…
Pause routing
Quit
```

Platform placement:

- macOS: menu bar status item; no Dock icon while the main window is closed.
- Ubuntu: AppIndicator-compatible tray item; document the GNOME extension requirement where applicable.
- Windows: notification-area icon.

Closing the main window keeps routing and sharing active. **Quit** stops listeners, tunnels and active requests. Tray state uses three signals only:

- green: connected and ready;
- gray: paused or using this device;
- orange: action required.

## Main window

The normal window contains one compact page:

```text
Aillies                                      ● Ready

Send through       [ Dailyngocss              ▾ ]
                    Connected via tunnel · 48 ms

Routing            [ On ]

Share this device  [ On ]
                    Codex account ready · 2 devices approved

                         [ Pair a device ]

Connection details                                 ›
```

Rules:

- Do not show a model text field on the home screen.
- Do not show a manual generation test after onboarding succeeds.
- Do not expose endpoint or bearer-token instructions in the normal window.
- Do not use separate mode pages for sending and sharing.
- Do not show a dashboard, request history or decorative metrics.
- Put logs, model overrides, relay selection and diagnostics under **Connection details**.

Codex integration becomes a single setting named **Use Aillies with Codex**. Turning it on performs backup, compatibility validation, provider installation and a no-content health check. Turning it off restores the saved configuration while preserving unrelated edits. If automatic merging is unsafe, show one precise recovery action and make no changes.

## First-run onboarding

First run has one primary action:

```text
Use another device's account

Pair a trusted device to use its Codex login securely.

                         [ Pair a device ]

I only want to share this device
```

Pairing flow:

1. Choose **Pair a device** on either computer.
2. Discover nearby devices or enter a short pairing code for a remote device.
3. Compare one short verification code on both devices.
4. Approve on both devices.
5. The sharing device selects its local Codex login if none is configured.
6. Aillies tests identity, path and account readiness separately.
7. The new device becomes the selected destination.

Pairing must work through the public rendezvous service, so AirDrop, SSH, IP addresses and shared LAN access are optional conveniences rather than requirements.

## Connection behavior

**Auto** is the default and should satisfy almost every user:

```text
Auto
  ├─ direct LAN available → pinned direct connection
  └─ otherwise            → public encrypted tunnel
```

Advanced connection choices:

- **Auto:** direct first, public tunnel fallback.
- **Direct only:** never contact a public relay for request transport.
- **Public tunnel:** force the selected relay for testing or restrictive networks.

Auto evaluates usable paths in this order:

1. local network address discovered by Aillies;
2. configured private-network address, including Tailscale, WireGuard or ZeroTier;
3. configured self-hosted Aillies relay;
4. Aillies hosted relay.

Only reachability changes across these paths. Device identity, peer authorization, application encryption and the selected account remain the same. A path change never authorizes a different device.

### Connection details

Provider setup lives under **Connection details**, not onboarding or the tray:

```text
Connection details

Path policy                    [ Auto                     ▾ ]

Private network
  Tailscale                    Detected
  Dailyngocss address          dailyngocss.example.ts.net

Relay
  Provider                     [ Aillies hosted       ▾ ]
  Region                       [ Automatic               ▾ ]

Other options
  › Use a self-hosted relay
  › Add a manual device address
```

Supported setup types:

- **Aillies hosted:** the default rendezvous and blind relay; no hostname entry.
- **Tailscale:** use the peer's MagicDNS name or Tailscale IP as a private direct address. Both devices must already run and authorize Tailscale. Aillies does not manage the tailnet, login or ACLs. Tailscale may choose a direct, peer-relay or DERP path internally; Aillies still authenticates and encrypts its own peer session.
- **Self-hosted Aillies relay:** connect both devices outbound to a compatible relay at a user-owned HTTPS/WSS domain such as `relay.example.com`. The setup validates HTTPS, protocol version and the relay identity before saving.
- **Manual device address:** connect directly to a hostname/IP and port provided by the user. This covers ordinary VPNs, WireGuard, ZeroTier, port forwarding and private DNS without adding a provider-specific integration.

A custom domain by itself is not a tunnel. It must resolve either to the peer's reachable listener for a manual direct path or to a running Aillies relay service for a relayed path.

Self-hosted relay setup stays compact:

```text
Self-hosted relay

Relay URL          [ wss://relay.example.com             ]
Enrollment token   [ Stored securely                     ]

                    [ Test connection ]  [ Save ]
```

The enrollment token, when required by the operator, is stored in the platform credential store and is used only to admit the device to that relay. It is separate from device pairing and Codex OAuth. A successful relay test proves transport compatibility only; it does not prove peer availability or account readiness.

Custom Tailscale DERP configuration is deliberately not reproduced in Aillies. Users who operate a custom DERP server configure it in their tailnet policy; Aillies simply uses the Tailscale address exposed by the operating system.

Changing paths may reconnect transport but must not resubmit a prompt automatically. If a request's delivery state is uncertain, report it as uncertain and let the user decide whether to retry.

## Public service boundary

The hosted service has two deliberately small roles:

### Rendezvous

- publishes short-lived online presence for paired device identities;
- exchanges connection candidates and relay availability;
- delivers pairing approval messages;
- stores no OAuth token, prompt, response or conversation history.

### Blind relay

- accepts outbound connections from both devices on port 443;
- forwards multiplexed opaque frames between an authorized device pair;
- applies connection, bandwidth and abuse limits;
- can observe operational metadata such as device pseudonyms, timing, region and byte counts;
- cannot decrypt application payloads.

Application encryption terminates only on A and B. TLS to the relay is an outer transport layer, not the security boundary for prompts or credentials. Pairing establishes device identity keys and the peers derive fresh session keys for each connection.

The public host must never receive B's `auth.json`, access token, refresh token, pairing secret or Codex configuration. B reads its selected OAuth source locally for each turn, matching the current fail-closed account behavior.

## Target architecture

```text
Codex on A
    │ loopback Responses API
    ▼
Aillies A ───────── direct pinned TLS ─────────┐
    │                                             │
    └── encrypted outbound tunnel ── Public relay │
                                      │           │
                                      └───────────┤
                                                  ▼
                                             Aillies B
                                                  │ local OAuth
                                                  ▼
                                                OpenAI
```

Implementation stack:

- Tauri 2 for the window, tray, lifecycle and installers.
- Rust for pairing, discovery, transport, tunnel multiplexing, Responses translation and configuration transactions.
- Tokio for async runtime.
- Axum or Hyper for the loopback Responses server.
- Reqwest with rustls for upstream HTTP and SSE.
- SQLite for non-secret device and state records.
- Platform credential adapters for macOS Keychain, Windows Credential Manager and Linux Secret Service.
- TypeScript with a small static UI. Avoid a frontend state framework until the interface requires one.

Platform-specific code is limited to credential storage, autostart, app detection, native tray details and installer/signing integration. Request routing and security behavior must remain in shared Rust crates.

Suggested workspace:

```text
aillies/
├── crates/
│   ├── peer-core/       # identities, pairing and shared types
│   ├── peer-transport/  # direct and relayed sessions
│   ├── peer-turns/      # Responses validation and SSE
│   ├── peer-config/     # transactional Codex configuration
│   └── peer-platform/   # credential and lifecycle traits
├── app/
│   ├── src/             # minimal TypeScript UI
│   └── src-tauri/       # Tauri commands and tray
├── relay/               # hosted rendezvous and blind relay
└── fixtures/            # versioned protocol and security fixtures
```

## Migration phases

### Phase 0 — freeze the working contract

- Tag the 0.8.3 source and keep its downloadable macOS build.
- Convert current Swift fixtures into language-neutral request, SSE, TLS and failure-code fixtures.
- Record the pairing-file and restore-receipt schemas with explicit versions.
- Add tests proving prompts, tokens and upstream error bodies are absent from logs.

Exit gate: the existing macOS text and image checks remain reproducible without live credentials.

### Phase 1 — shared Rust core

- Implement request normalization, Lite-model behavior, completed-JSON fallback and SSE forwarding in Rust.
- Implement OAuth-file validation without refresh-token use.
- Implement bounded concurrency, cancellation, backpressure and sanitized failures.
- Run Swift and Rust against the same fixtures.

Exit gate: Rust output and failure semantics match the accepted 0.8.3 contract.

### Phase 2 — Tauri shell and tray

- Implement the compact home window and tray menu.
- Keep direct LAN pairing and transport working on macOS first.
- Add transactional Codex enable/restore using semantic TOML editing.
- Import existing macOS pairings and trust records without exposing their secrets to the UI.

Exit gate: a current macOS user can upgrade, retain a pairing, enable Codex, complete a turn, restore Codex and downgrade safely.

### Phase 3 — public tunnel

- Deploy a single-region development rendezvous and blind relay.
- Add remote short-code pairing with two-sided verification.
- Implement outbound-only tunnel establishment and reconnect.
- Add Auto, Direct only and Public tunnel path policy.
- Add Tailscale/private-network address detection without taking control of third-party VPN configuration.
- Add validated self-hosted relay URLs and manual device addresses under Connection details.
- Document relay metadata and retention.

Exit gate: two devices behind separate NATs complete text and image turns; stopping B or revoking the pairing terminates access; the relay cannot decrypt fixture payloads.

### Phase 4 — Ubuntu and Windows

- Implement platform credential stores and tray behavior.
- Build Ubuntu `.deb` and AppImage artifacts.
- Build Windows MSI or setup executable.
- Validate autostart, sleep/wake, firewall prompts and clean uninstall.
- Test mixed pairs: Mac↔Ubuntu, Mac↔Windows and Ubuntu↔Windows.

Exit gate: every pair combination passes the same protocol suite and a live opt-in smoke test.

### Phase 5 — release transition

- Make Tauri/Rust the default 0.9 application.
- Keep the Swift build available as 0.8.3 during the preview window.
- Remove the old browser/SSH prototype from the primary setup guide.
- Do not remove 0.8 pairing import until one stable release after 0.9.

## Existing-user migration

On macOS, migration is transactional:

1. Detect the 0.8 Application Support directory, Keychain entries and restore receipt.
2. If Codex is currently configured for Aillies, offer **Restore and upgrade**.
3. Restore the original Codex configuration while preserving independent semantic changes.
4. Import paired-device identities into the new database.
5. Store new secrets in Keychain and mark imported legacy material read-only.
6. Enable the 0.9 provider only after its local health check succeeds.
7. Keep a migration journal so an interrupted upgrade can resume or roll back.

Do not silently migrate when the current Codex configuration and receipt disagree. Show the exact conflicting setting category without printing credentials or configuration contents.

Pairing material is device-specific. A pairing imported on a Mac is not copied automatically to Ubuntu or Windows. Pair the new installation as another approved device.

## Protocol versioning

- Preserve the current loopback endpoint shape: `POST /v1/responses`.
- Introduce a versioned peer handshake independent of the application version.
- Negotiate capabilities for text, image, model aliases, maximum sizes and transport features.
- A newer client may connect to an older peer only when the required capability is explicitly advertised.
- Never infer account or image entitlement from connectivity alone.
- Never switch accounts, peers or paths during an in-flight turn.

## Security requirements

- Device identity uses a non-exportable platform credential where available.
- Pairing requires two-sided approval and a human-comparable code.
- Every direct connection pins the paired device identity.
- Every relayed connection is encrypted end to end above relay TLS.
- Public relay compromise must not expose OAuth credentials or generation content.
- Revocation is per device and closes active sessions.
- Logs contain fixed event names, safe error codes and coarse transport metrics only.
- No prompt, response, reasoning, image, authorization header, pairing key or raw upstream error body is logged.
- No automatic prompt retry after a transport ambiguity.
- The loopback API remains bound to `127.0.0.1` and requires a local client credential.

## Release and CI

Use a native CI matrix rather than treating cross-compilation as the release path:

```text
macOS runner   → universal or architecture-specific app, DMG, signing, notarization
Windows runner → MSI/setup executable and code signing
Ubuntu runner  → DEB and AppImage
```

Each platform runs shared Rust tests, platform credential tests, config transaction tests, tray lifecycle tests and an encrypted relay fixture. Release promotion requires artifact signatures and a software bill of materials.

## 0.9 acceptance criteria

The preview is ready only when all of the following are true:

- A first-time user can pair remote devices without SSH, IP addresses or manual certificates.
- The tray can select a peer, pause routing, open the window and quit on all three platforms.
- Auto uses direct LAN when available and the public tunnel otherwise.
- Tailscale/private-network and self-hosted relay paths can be configured without exposing them in the everyday tray flow.
- A custom relay is rejected before saving when HTTPS, relay identity or protocol compatibility validation fails.
- The selected peer's configured account completes verified text and image checks.
- Codex can start a new routed chat after one integration toggle and restart.
- Restoring Codex preserves unrelated configuration edits.
- The relay cannot decrypt test payloads and receives no OAuth material.
- Per-device revocation stops both direct and relayed sessions.
- A failed or uncertain request is never silently replayed or sent through another account.
- The UI communicates destination device, account readiness, path and action-required state without exposing implementation details.

## Deferred beyond 0.9

- Official ChatGPT desktop account substitution.
- Conversation ownership or migration between accounts.
- General-purpose system VPN or arbitrary traffic proxying.
- Relay-side prompt history, analytics or content moderation.
- Automatic OAuth refresh-token handling by Aillies.
- Multi-peer load balancing or quota-based routing.
