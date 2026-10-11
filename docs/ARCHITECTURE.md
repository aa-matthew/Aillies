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
                 ◀──────────────── Responses SSE streamed back unchanged ────────────────
```

One **node** runs per device, inside either `aillies run` or the desktop app. A node can send through a paired device and share its own login at the same time.

## Crates

| Crate | Role | Key modules |
|---|---|---|
| `aillies-core` | Pure, platform-neutral logic, with no I/O so the same fixtures run everywhere | `request` (validate/normalize), `wire` (SSE vs JSON detection, terminal-event scanner), `failure` (codes → HTTP + message), `protocol` (invite, verification code, peer status, providers) |
| `aillies-codex` | Codex provider | `auth` (login reader), `executor` (upstream call and stream), `config` (install/restore `config.toml`) |
| `aillies-node` | Everything a device does | `node` (orchestration, pairing, settings), `loopback` (Codex + control API), `control`, `peer_server`, `peer_client`, `tls`, `identity`, `store`, `body` |
| `aillies-cli` | `aillies` binary | `main.rs`: `run` starts a `Node`; other commands call `/control/*` |
| `apps/desktop/src-tauri` | Tauri 2 shell | `lib.rs`: tray, on-demand window, IPC commands → `Node` methods |
| `apps/desktop/ui` | Window UI | `app.js` (screens, polling, escaping), `style.css`, `mock.js` (browser preview only) |

Dependency direction: `core` ← `codex` ← `node` ← `cli` / `desktop`. Only `node` does networking between devices; only `codex` talks to OpenAI.

## Interfaces

### Loopback API (127.0.0.1:18522)

The `Host` header must be `127.0.0.1`, `localhost` or `[::1]`; other values get 421 (DNS-rebinding guard). Connections from non-loopback addresses are dropped.

| Method and path | Auth | Purpose |
|---|---|---|
| `GET /healthz` | none | `{"ok":true,"product":"Aillies","version":…}` |
| `POST /v1/responses` | `Bearer codex_key` | Codex Responses request → routed per settings |
| `GET /control/status` | `Bearer control_token` | `NodeStatus` (state, destination, peers, clients, pending, Codex, last failure) |
| `GET /control/peers/{fp}/health` | control | `PeerHealth`: reachable, latency, address, peer status |
| `POST /control/invite` | control | Create a one-time pairing code (turns sharing on) |
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
| `POST /v1/pair` `{protocol, secret, name, platform}` | any certificate + valid one-time secret | Creates a pending request → `{state:"pending", id, code}` |
| `GET /v1/pair/{id}` | the same certificate | `pending` / `approved` / `rejected` / `expired` |
| `POST /v1/pair/{id}/cancel` | the same certificate | Withdraw; removes an approval if one was given |
| `GET /v1/status` | approved certificate | `PeerStatus`: protocol, version, sharing, providers with capabilities and account readiness |
| `POST /v1/codex/responses` | approved certificate, sharing on | Executes and streams Responses SSE |

Requests may carry `x-aillies-protocol: 1`; a mismatch returns 426 `protocol_mismatch`. `PeerStatus.providers` lists `codex` today; `claude` is reserved.

## Flows

### Pairing

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
Codex ─▶ A /v1/responses
  routing off              → 503 routing_paused
  destination = local      → local Executor (A’s own login)
  destination = peer
     connect (pinned)      → fail: 503 peer_unavailable (nothing sent)
     send once             → write/read error before a response: 502 delivery_uncertain
B /v1/codex/responses
     not approved → 401 · sharing off → 503 · 4 in flight → 429 · body > 16 MiB → 400
     Executor: read auth.json → expired/missing/other account → 503 (nothing sent)
               normalize request → invalid → 400 (nothing sent)
               POST chatgpt.com (no redirects, proxy, cookies or retries)
               connect error → upstream_unavailable · later error → delivery_uncertain
               non-200 → upstream_rejected(status)
               body detected as SSE → stream bytes unchanged (≤ 64 MiB)
               completed JSON → finite SSE sequence · anything else → rejected
A streams to Codex; an incremental SSE parser (TerminalScanner) watches for a real terminal
  event (event: field or top-level type); a stream that ends or breaks without one → last_failure = delivery_uncertain
```

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
| `stream_failed`, `delivery_uncertain` | 502 | Broke after sending | **Possibly**. Never retried |

Messages are fixed strings in `failure.rs`, wrapped OpenAI-style (`{"error":{"code","message","type":"aillies_error"}}`) so Codex displays them.

## State

All files live in the Aillies home folder and are 0600:

| File | Contents |
|---|---|
| `identity.cert.der`, `identity.key.der` | P-256 self-signed certificate and key; fingerprint = SHA-256 of the cert |
| `state.json` | Name, routing, sharing, destination, `shared_account`, peers (fp, name, platform, addrs), clients (fp, name, platform, approved_at, last_used) |
| `secrets.json` | `codex_key`, `control_token`, `loopback_port` |
| `codex-restore.json` | Receipt while the Codex integration is on |
| `desktop.log` | Desktop lifecycle and errors only |

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
| Double spending on retries | No retries in Aillies or Codex; possibly-sent failures are labeled |
| Peer-supplied strings in the UI | Escaped before rendering; strict CSP in the desktop window |

Out of scope today: hiding traffic metadata from the local network, protecting a fully compromised device, OS-store secrets (M7).

## Limits

Request ≤ 16 MiB · response ≤ 64 MiB · 4 concurrent requests per sharer · 64 peer connections · 4 s connect timeout per address · 10 s TLS handshake · upstream connect 15 s, total 15 min · pairing request TTL 10 min · ≤ 8 pending pairings.

## Desktop shell

At start: start the `Node`, then the tray, then (first run, an error, or a pairing request) the window. The window is created on demand so routing survives a broken WebView. Closing the window hides it; Quit shuts down the node. A 1.5 s loop refreshes the tray (icon color, menu, tooltip) from `Node::status()`. The UI polls `status` every 2 s and checks peer health every 15 s while home is visible.

## Adding Claude

1. New crate `aillies-claude`: login reader + executor with the same `TurnStream` and `Failure` contract.
2. Peer route `/v1/claude/messages`; advertise `Provider::Claude` with capabilities in `PeerStatus`.
3. Loopback route that Claude Code can target, plus a config installer like `aillies-codex::config`.
4. Add a provider to destinations (per-provider “Send through”).

Pairing, transport, revocation and failure semantics stay unchanged.

## Testing

| Layer | Where |
|---|---|
| Pure logic | `#[cfg(test)]` in `aillies-core` and `aillies-codex` (mock upstream for the executor) |
| Two devices | `crates/aillies-node/tests/e2e.rs`: pair, route text + image, pause, sharing off, expired/changed login, mid-stream drop, revoke, restore |
| End to end with real Codex | `scripts/smoke.sh` (two daemons, mock backend, `codex exec`) |
| Desktop shell | `apps/desktop/src-tauri/src/lib.rs` tests (tray labels, icons, menu refresh signature); `make test`/`make lint` include it |
| Platforms | CI on macOS/Windows/Ubuntu; Windows CLI exercised under Wine |
