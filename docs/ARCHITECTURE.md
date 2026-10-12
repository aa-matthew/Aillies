# Architecture

## Overview

```text
 Device A (requester)                                     Device B (sharer)
┌──────────────────────────────────────┐                ┌──────────────────────────────────────┐
│ Codex ──HTTP──▶ loopback 127.0.0.1:18522               │ peer listener 0.0.0.0:18544          │
│                 /v1/responses  (Bearer codex_key)      │   /v1/pair, /v1/pair/{id}[/cancel]   │
│ aillies CLI ──▶ /control/*     (Bearer control_token)  │   /v1/status                         │
│ desktop app ──▶ Node (in-process)                      │   /v1/codex/responses                │
│                        │                               │            │                         │
│                        └══ HTTP/1.1 in TLS 1.3, mutual, pinned ═══▶ │ Executor ──HTTPS──▶ chatgpt.com/backend-api/codex/responses
│                                                        │            reads ~/.codex/auth.json   │
└──────────────────────────────────────┘                └──────────────────────────────────────┘
                 ◀──────────────── successful attempt buffered; keepalives while waiting ────────────────
```

One **node** runs per device, inside either `aillies run` or the desktop app. A node can send through a paired device and share its own login at the same time.

## Crates

| Crate | Role | Key modules |
|---|---|---|
| `aillies-core` | Pure, platform-neutral logic, with no I/O so the same fixtures run everywhere | `request` (validate/normalize), `wire` (SSE vs JSON detection, terminal-event scanner), `failure` (codes → HTTP + message), `protocol` (invite, verification code, peer status, providers) |
| `aillies-claude` | Claude provider | `auth`, `desktop_auth` (native login readers), `executor`, `config`, `desktop_config` (transactional client settings) |
| `aillies-codex` | Codex provider | `auth` (login reader), `executor` (upstream call and stream), `config` (install/restore `config.toml`), `provider_switch` (offline existing chats), `session` (bounded identities), `telemetry` (typed observations) |
| `aillies-node` | Everything a device does | `node` (orchestration, pairing, settings), `loopback` (Codex + control API), `control`, `peer_server`, `peer_client`, `tls`, `identity`, `store`, `body` |
| `aillies-cli` | `aillies` binary | `main.rs`: `run` starts a `Node`; other commands call `/control/*` |
| `apps/desktop/src-tauri` | Tauri 2 shell | `lib.rs`: tray, on-demand window, IPC commands → `Node` methods |
| `apps/desktop/ui` | Window UI | `app.js` (screens, polling, escaping), `style.css`, `mock.js` (browser preview only) |

Dependency direction: `core` ← `codex` / `claude` ← `node` ← `cli` / `desktop`. Only `node` does networking between devices; `codex` talks to OpenAI and `claude` talks to Anthropic.

## Interfaces

### Loopback API (127.0.0.1:18522)

The `Host` header must be `127.0.0.1`, `localhost` or `[::1]`; other values get 421 (DNS-rebinding guard). Connections from non-loopback addresses are dropped.

| Method and path | Auth | Purpose |
|---|---|---|
| `GET /healthz` | none | `{"ok":true,"product":"Aillies","version":…}` |
| `POST /v1/responses` | `Bearer codex_key` | Codex Responses request → routed per settings |
| `GET /v1/models`, `/claude-desktop/v1/models` | `Bearer claude_key` | Native Claude catalog from the selected executing account; vetted pagination, pinned retries, no fallback |
| `GET /control/diagnostics`, `POST /control/diagnostics/export` | control | Read the bounded request error/warning report or export a private copy to Downloads |
| `GET /control/status` | `Bearer control_token` | `NodeStatus` (state, destination, peers, clients, pending, Codex, last failure) |
| `GET /control/peers/{fp}/health` | control | `PeerHealth`: reachable, latency, address, peer status |
| `POST /control/pairing-code` | control | Create an eight-digit nearby pairing code (turns sharing on) |
| `POST /control/invite` | control | Create a full one-time code for private networks/older devices; replaces active numeric code |
| `POST /control/pair` `{code}` | control | Start pairing → `{id, code, name, platform, short_id}` |
| `POST /control/pair/{id}/confirm` `{wait_secs}` | control | Wait for approval on the other device, then save it |
| `POST /control/pair/{id}/cancel` | control | Withdraw the request on the other device |
| `POST /control/approve/{id}`, `/control/reject/{id}` | control | Decide a pending request (sharing side) |
| `POST /control/destination` `{target}` | control | `local`, fingerprint, short id, or name |
| `POST /control/routing` `{on}`, `/control/sharing` `{on}` | control | Switches |
| `POST /control/codex/enable` `{model?}`, `/control/codex/restore` | control | Codex integration |
| `POST /control/peers/remove`, `/control/clients/revoke` `{target}` | control | Forget a peer, revoke a client |
| `POST /control/name` `{name}`, `/control/dismiss` | control | Rename this device; clear the last failure |

### Peer protocol v1 (TCP 18544, TLS 1.3, client certificate required)

| Method and path | Who may call | Purpose |
|---|---|---|
| `POST /v1/pair/bootstrap` `{message}` | any certificate; active numeric code, max five PAKE executions | SPAKE2 reply plus AES-GCM sealed full invite; no approval granted |
| `POST /v1/pair` `{protocol, secret, name, platform}` | any certificate + valid one-time secret; numeric secret also bound to bootstrap certificate | Creates a pending request → `{state:"pending", id, code}` |
| `GET /v1/pair/{id}` | the same certificate | `pending` / `approved` / `rejected` / `expired` |
| `POST /v1/pair/{id}/cancel` | the same certificate | Withdraw; removes an approval if one was given |
| `GET /v1/status` | approved certificate | `PeerStatus`: protocol, version, sharing, providers with capabilities and account readiness |
| `POST /v1/codex/responses` | approved certificate, sharing on | Executes and streams Responses SSE |
| `GET /v1/claude/models` | approved certificate with Claude grant, Claude sharing on | Executes native model discovery once per requester attempt using local Claude credentials |

Requests may carry `x-aillies-protocol: 1`; a mismatch returns 426 `protocol_mismatch`. `PeerStatus.providers` lists Codex and the Claude provider activated by the experimental 1.1 adapter. Claude uses independent sharing/destinations/grants; upgrade both peers for Claude.

## Flows

### Pairing

The desktop and `aillies invite` default to eight cryptographically random digits (including leading zeroes). While the code is active, B advertises a random service name, public certificate fingerprint and host/port using mDNS. A discovers candidates for three seconds, pins each advertised fingerprint, and runs SPAKE2 with both certificate identities and distinct role labels. Only the right password decrypts the AES-256-GCM sealed full invite; the AAD also binds both fingerprints. The ordinary pairing flow below then consumes its random 32-byte secret, bound to the bootstrap requester certificate. Future access is authorized by stored certificate fingerprints, not by an exchanged OpenAI credential or reusable bearer token.

Codes expire after ten minutes; at most five PAKE executions are allowed per code. Successful consumption, replacement, sharing off, shutdown or expiry removes the advertisement. The requester permits one search at a time, at most 16 candidates / 16 addresses each, four concurrent probes and a 25-second total budget. Multiple successful matches refuse. This is LAN discovery, not internet rendezvous. `invite --full` and the secondary full-code UI preserve direct/private-network compatibility and the existing approval flow. No password or secret is persisted or logged by the numeric bootstrap.

```text
B: create_invite ──▶ invite = {name, platform, addrs[], fp_B, secret} (10 min, one use)
A: start_pairing(invite)
   connect to addrs in order; accept only the server cert whose SHA-256 == fp_B
   POST /v1/pair {secret, name_A}    (A’s client cert = A’s identity)
B: consume secret → pending {id, fp_A, code = H(fp_B, fp_A, secret)}
A: checks B’s code == its own code; shows it; person confirms
B: shows the same code; person approves → fp_A added to clients
A: polls GET /v1/pair/{id} until approved → saves B (fp_B, addrs); selects B if nothing else is selected
```

### A Codex request

```text
Codex → A /v1/responses: authenticate, validate, capture route/model/account scope
  routing off / missing destination / invalid input → fixed refusal
  local → A's executor; peer → pinned mutual TLS to selected B
  requester controller → initial attempt + at most 3 retries, same scope
    B: certificate, sharing/account/Fast/version checks and concurrency bound
    executor: read local auth.json, normalize, send once to chatgpt.com
      non-200 → fixed upstream_rejected(status); transport → fixed failure
      SSE → TerminalScanner; completed JSON → finite SSE sequence
    A: buffer each attempt (≤64 MiB), emit fixed keepalive comments while waiting
      success → release only the completed winning attempt to Codex
      retryable failure → discard partial content, back off, recheck scope
      cancelled / scope changed / access or local validation refused → stop
      exhausted → fixed HTTP error, or response.failed SSE after headers
```

### Requester retry and warning policy

Decision 45 authorizes an initial attempt plus three retries, with 500/1000/2000 ms backoff and an 880-second logical-turn deadline. The executor, sharing node and Codex do not add nested retries. Account/device/model fallback is forbidden. Current access, peer capability and route selection are checked before further sends. Earlier attempts may consume usage even though failed partial content is suppressed. Cancellation drops collection and backoff. Forced image calls additionally validate a complete PNG before choosing the winner and saving one file.

The peer link carries attempt (1–4), initial-send boolean and generated trace/session metadata. B binds client fingerprint + trace to its original account executor in a private 1,024-entry, 900-second map. Missing bindings after restart/expiry refuse retries; confirmed pre-send/busy failures may establish their first binding later. Local execution pins its account too. Only bounded metadata crosses this path; OAuth stays on the executor's device.

Diagnostics record one incoming logical turn on A, individual hosted/upstream attempts on B and fixed retry notices. Home shows the newest four notices; success and dismissal preserve latest-100 failure history. After a possibly sent attempt, a later pre-send refusal must not imply that the entire logical turn sent nothing.

### Codex integration

`install`: refuse symlinks and non-UTF-8; locate TOML statements without reserializing; set root `model_provider = "aillies"`, keep the user’s `model` (or default `gpt-5.5`), set `features.api_key_model_discovery = false`, append `[model_providers.aillies]` (base URL, local key, `wire_api = "responses"`, retries 0). Write the receipt (original + installed) first, then the config, both atomically and 0600.

`restore`: if the file is unchanged, write back the original byte for byte. Otherwise `merge_restore` removes the Aillies table and marker, puts back the original `model_provider` / `model` / discovery lines, and drops a `[features]` table Aillies created, while keeping every other edit (Codex adds trusted projects and notices on its own). If parsing is unsafe, nothing changes and the receipt stays.

## Failure codes

| Code | HTTP | Meaning | May have run on the sharing account? |
|---|---|---|---|
| `request_invalid` | 400 | Request shape not accepted | No |
| `unauthorized` | 401 | Wrong local key | No |
| `not_approved` | 401 | Device not (or no longer) approved | No |
| `routing_paused` | 503 | Routing switched off | No |
| `no_destination` | 503 | Selected device missing | No |
| `peer_unavailable` | 503 | Device unreachable | No |
| `sharing_off` | 503 | Sharing off on the device | No |
| `account_not_selected`, `invalid_auth_file`, `oauth_expired`, `auth_file_unavailable`, `account_changed` | 503 | Sharing account not usable | No |
| `peer_busy` | 429 | 4 requests already running | No |
| `protocol_mismatch` | 426 | Incompatible versions | No |
| `upstream_rejected` | 502 / 429 | OpenAI returned a non-200 status (kept as a number only) | Rejected by OpenAI |
| `unexpected_upstream_format`, `unexpected_json_response` | 502 | 200 without a usable stream | Possibly |
| `upstream_unavailable` | 502 | Couldn’t connect to OpenAI | No |
| `stream_failed`, `delivery_uncertain` | 502 / failed SSE after headers | Broke after sending | **Possibly**. Requester may retry up to three times; see alpha.10 |

The table describes one attempt. A logical turn may include earlier possibly delivered attempts; warnings preserve that uncertainty. Messages are fixed strings in `failure.rs`, wrapped OpenAI-style (`{"error":{"code","message","type":"aillies_error"}}`) so Codex displays them.

## State

All files live in the Aillies home folder and are 0600:

| File | Contents |
|---|---|
| `identity.cert.der`, `identity.key.der` | P-256 self-signed certificate and key; fingerprint = SHA-256 of the cert |
| `state.json` | Name, routing, sharing, destination, `shared_account`, peers (fp, name, platform, addrs), clients (fp, name, platform, approved_at, last_used) |
| `secrets.json` | `codex_key`, `control_token`, `loopback_port` |
| `codex-restore.json` | Receipt while the Codex integration is on |
| `request-diagnostics.json` | Latest 100 request errors/warnings; fixed codes and allowlisted metadata, no content |
| `traffic-diagnostics.json` | Independent latest-100 incoming/upstream observations; fixed phases, counts and timing only |
| `desktop.log` | Typed lifecycle events and public version/port only |

Home folder: `AILLIES_HOME`, else `~/Library/Application Support/is.anyax.Aillies` (macOS), `~/.config/aillies` (Linux), `%APPDATA%\anyax\Aillies\config` (Windows). Invites and pending requests live in memory only.

## Security model

| Threat | Mitigation |
|---|---|
| Impostor sharer on the network | Invite carries B’s fingerprint; TLS accepts only that certificate |
| Impostor requester | Needs the one-time secret *and* approval on B; the verification code exposes a swapped key |
| Stolen invite after use | Secret is consumed on first use; expires in 10 minutes |
| Revoked device keeps using the account | Authorization is checked on every request; revocation drops live connections |
| Malicious web page calling the loopback API | Loopback bind, `Host` check, bearer tokens (Codex key ≠ control token) |
| Token leakage | Tokens never leave B; no logging; errors are fixed codes; the upstream override is a parsed loopback URL (no user info, explicit port) |
| Duplicate work/usage on retries | User-authorized maximum of four attempts; same route/account/model, one requester budget, buffered generation events and cancellation guards. Upstream work can still be duplicated. Codex/executor/peer do not add retries |
| Peer-supplied strings in the UI | Escaped before rendering; strict CSP in the desktop window |

Out of scope today: hiding traffic metadata from the local network, protecting a fully compromised device, OS-store secrets (M7).

## Limits

Request ≤ 16 MiB · response ≤ 64 MiB · 4 concurrent requests per sharer · 64 peer connections · 4 s connect timeout per address · 10 s TLS handshake · upstream connect 15 s, total 15 min · pairing request TTL 10 min · ≤ 8 pending pairings.

## Desktop shell

At start: start the `Node`, then the tray, then (first run, an error, or a pairing request) the window. The window is created on demand so routing survives a broken WebView. Closing the window hides it. Disconnect & quit uses reviewed client closure and independent integration cleanup before stopping; incomplete cleanup retains recovery files and offers explicit Quit anyway. A 1.5 s loop refreshes the tray (icon color, menu, tooltip) from `Node::status()`. The UI polls `status` every 2 s and checks peer health every 15 s while home is visible.

## Claude native adapter (1.1, experimental)

Claude Code sends native Messages to authenticated loopback `/v1/messages` or `/v1/messages/count_tokens`. The node chooses its separate Claude destination, sends vetted version/beta headers over pinned mTLS `/v1/claude/messages[/count_tokens]`, and checks the client's explicit Claude grant on each hosted request. The default executor reads the explicitly connected private Claude OAuth store; legacy native readers remain available only through explicit CLI/control sources. It sends only to `https://api.anthropic.com`; redirects/proxies are disabled. OpenAI OAuth remains restricted to chatgpt.com.

The shared requester controller owns initial + three attempts; Claude account/token bindings are separate from Codex’s account bindings. Raw Messages history, tools, model and cache markers are preserved. Alpha.7 adds a pure capability adapter for reviewed top-level thinking/effort/sampling edits and shape-required betas; no-op bodies retain every byte. Signed history never changes; see CLAUDE.md and decision58. Only a framed native terminal event exposes a winning stream. HTTP/SSE access/validation errors stop; fixed errors suppress upstream bodies. Claude defers HTTP 200 until success so client empty-stream recovery cannot multiply retries; a 900-second client timeout covers the requester budget. Finite Messages and token-count JSON are validated. Diagnostics use the same rings with a closed provider field.

Claude user settings get a target-bound private restore receipt; only the installed environment subset is changed/restored. No transcript/session/folder migration occurs. Restart/resume manually. Account OAuth uses Bearer authorization, OAuth/Claude Code beta headers and the actual client User-Agent. Aillies never refreshes native source logins. Alpha.5 renews only its own explicitly connected Claude OAuth before pinning a new turn. Automatic editor settings and Claude image generation are unsupported. [CLAUDE.md](CLAUDE.md) gives ownership, configuration, test and reference details.

## Testing

| Layer | Where |
|---|---|
| Pure logic | `#[cfg(test)]` in `aillies-core` and `aillies-codex` (mock upstream for the executor) |
| Two devices | `crates/aillies-node/tests/e2e.rs`: pair, route text + image, pause, sharing off, expired/changed login, mid-stream drop, revoke, restore |
| End to end with real Codex | `scripts/smoke.sh` (two daemons, mock backend, `codex exec`) |
| Desktop shell | `apps/desktop/src-tauri/src/lib.rs` tests (tray labels, icons, menu refresh signature); `make test`/`make lint` include it |
| Platforms | CI on macOS/Windows/Ubuntu; Windows CLI exercised under Wine |

## Optional Fast tier and explicit image tool

The sharer advertises `allow_fast_mode`, default false. The requester stores a separate Fast preference for each destination and sets `priority` or `default` on its outgoing request. The sharing node validates the tier and current owner permission before calling the executor. No permission rejection triggers a downgrade or retry.

Codex integration also registers a Streamable HTTP MCP server on the existing loopback listener at `/mcp/images`, with the local Codex bearer key. It exposes only `generate_image`. Browser Origin requests are refused, in addition to normal Host and token checks. The tool submits a logical Responses turn through the shared requester retry controller, so the selected identity, routing pause, Fast enforcement, concurrency and revocation behavior are shared with text. A bounded, completed SSE result yields a PNG saved privately on A; raw error bodies and model text never become error messages. This adapter does not forward Codex's native `/images/generations` or `/images/edits` endpoints. Codex's own tool trust and approval checks still apply.

## Historical v3 preparation

The v3 cache/session resolver and one-send logical-turn policy are historical. They were replaced by v4 session resolution and decision 45's requester retries. See [the dated architecture snapshot](history/ARCHITECTURE-2026-10-11.md) for that implementation; use [UPSTREAM-COMPATIBILITY.md](UPSTREAM-COMPATIBILITY.md) for the current resolver and supported scope.

## Router core v4

See UPSTREAM-COMPATIBILITY.md for exact resolver scope and deliberate differences. Peer inference requests can carry `x-aillies-session` (printable ASCII ≤256 bytes) and `x-aillies-trace` (UUID); incoming arbitrary headers never traverse the peer link. Explicit body cache keys remain independent. `traffic-diagnostics.json` stores two private 100-entry rings; `request-diagnostics.json` retains failures separately. Exports never include the private provider-switch recovery journal or Codex configuration.

`POST /control/codex/switch` with `{on: boolean}` calls `Node::switch_codex_provider` on a blocking worker. It serializes against config enable/restore and refuses while Codex runs. A closed-process switch stages rollout metadata, snapshots paginated offsets, holds SQLite write transactions, journals recovery data, swaps files and updates config before committing provider/index changes. Valid unique unindexed rollouts are included as file-only changes, without fabricating index rows. Existing records remain intact; the latest provider settings event is appended with the selected provider when needed so index rebuilding agrees. File-only journals record completion durably, while indexed/mixed changes retain the SQLite provider commit marker. A subsequent switch recovers an interrupted operation before preparing new changes. No inference is submitted by a provider switch. The macOS app offers an explicit reviewed quit/switch/reopen action; it never executes shutdown merely from choosing a provider.

### Explicit macOS restart for provider changes

Desktop → Node read-only restart preview → in-memory single-use plan → user action → Node background worker → normal app quit / one SIGTERM per terminal/known daemon → global closed-process check → existing provider transaction → open only ChatGPT (including from standard Applications locations if already quit). Aillies survives closing the calling Codex app. Plans bind the provider direction and snapshot process identity; unexpected clients stop the flow. No config/history is written while clients remain. A failed transaction with a recovery receipt keeps clients closed. Process metadata is transient, never diagnostics. The UI holds pending provider choice separately from committed status. Indexed same-provider rollout replicas are file-only changes; only the canonical indexed file carries paginated offsets. Older settings records can omit thread_id; present IDs must match. See decisions 42–43.

### Background history preparation (alpha.9)

Provider migration runs on the existing blocking worker without an elapsed-time cutoff. Optional preparation deadlines exist only for deterministic failure-injection tests. The crate reports a closed phase enum and numeric completed/total file counts through a callback. Node holds these transiently behind a separate mutex, exposes them on the authenticated local status view, and clears them when the operation returns. Status polling never takes the provider-change lock. No file paths, IDs, messages, config or recovery data enter progress/diagnostics. The UI shows progress in the provider row. Process guards before preparation and before applying, staging, record bounds, journals, rollback and recovery remain.

## Desktop provider/navigation flow (alpha.11)

Native tray actions for provider, pairing, warnings and Quit store a single in-memory window intent and emit a navigation event. The window drains it through IPC (polling also handles a not-yet-created window), deferring during provider/pairing review. Provider intents call the existing Node transaction and restart-plan APIs. A successful explicit Aillies activation resumes a persisted paused routing gate. Normal selection leaves the gate, destination and sharing preferences unchanged. Desktop activation/exit uses Node::prepare_activation/apply_activation under the provider-change mutex. Both service inventories are validated before stopping any client. Complete cleanup uses shutdown_if_normal; failed cleanup/process review exposes shutdown_anyway after explicit user choice. Security/config logic remains in crates.

Desktop IPC exposes the transactional provider switch/restart flow. Simple config enable/restore remains available through CLI/control APIs; it is not a second desktop provider path.


### Desktop Chat extension — alpha.3

Desktop Chat config is separate from CLI env settings: a UUID profile plus `_meta.json` in native `Claude-3p/configLibrary`, with a private restore receipt. It points to `/claude-desktop/v1/messages[/count_tokens]` using the distinct local Claude bearer. Managed/bootstrap configurations refuse; the Mac app must be closed before switching. Normal desktop login/history is not modified. Gateway chat storage is local and separate from normal cloud history.

`ClaudeState.account_source` selects Code or Desktop when the owner enables sharing. DesktopReader uses the existing normal desktop config cache and exact native Safe Storage Keychain item (Mac only), no cookies/mint/refresh. Active account/org identity is hashed locally; each executor pins the current token. The desktop route chooses desktop credentials for local execution; peer execution uses the owner’s explicitly selected source. There is no cross-source fallback. Reads are fresh per attempt; process-memory Safe Storage password caching avoids repeated access prompts. Native ciphertext format changes fail closed. Requester turns on the desktop prefix use a 240-second total budget; CLI retains 880 seconds. Mock fixtures prove routing/auth/config behavior, not native app startup or live entitlement.


### Explicit Claude OAuth connection — alpha.5

`oauth.rs` is the connected source for both clients, independent of their gateway settings. The local control API exposes authenticated POST `claude/connect/start`, `claude/connect/finish {code}`, `claude/connect/cancel` and `claude/disconnect`; desktop IPC delegates to Node. Random PKCE/state and one-use pending flow live in memory for ten minutes. Exchange/bootstrap/refresh only contact fixed api.anthropic.com, with parsed synthetic-loopback exceptions and no redirect/proxy inheritance. Native credentials are not imported or edited.

Connected credentials and refresh-intent marker live in Aillies’ private accounts/claude.json (accounts 0700/file 0600), with atomic rename and file/directory sync. This is less protected than Keychain. Old Keychain selections are never read automatically; missing private storage disables sharing and requires explicit reconnect. State stores connected account/org digest and explicit account source. Windows/Linux native connection storage is unavailable. Fixture storage is selected only with explicit synthetic Node roots. A per-node refresh gate serializes renewal before new executor bindings; a persisted intent marker blocks uncertain/failed refresh until reconnect. Attempts re-read and compare pinned account/token; retries never renew. Connect/disconnect require sharing off; disconnect clears selection before deleting the secret. There is no fallback to legacy native sources.


Alpha.9 manual activation: Windows/Linux store affected service identities in the one-use activation plan; apply checks current process names before any settings edit. No restart engine or app launch runs there. Empty cleanup plans are valid. On macOS inventory validation and normal closure remain unchanged. Hosted Claude account bindings are cleared on committed consent/source/identity changes; a generation check rejects async creation that crosses an invalidation, preserving fail-closed retries.
