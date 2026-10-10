# Architecture

## Processes

One **node** runs per device, inside either `aillies run` or the desktop app. A node can send through a paired device and share its own login at the same time.

```text
             ┌──────────────── device A ────────────────┐            ┌──────────── device B ────────────┐
Codex ──HTTP─▶ loopback 127.0.0.1:18522                  │            │ peer listener 0.0.0.0:18544     │
             │   /v1/responses  (Bearer codex_key)       │──mTLS────▶ │   /v1/pair, /v1/status,          │──HTTPS──▶ chatgpt.com/backend-api/codex/responses
CLI ──HTTP──▶ │   /control/*     (Bearer control_token)   │  pinned    │   /v1/codex/responses            │          (B’s ~/.codex/auth.json)
             └───────────────────────────────────────────┘            └──────────────────────────────────┘
```

## Crates

- **aillies-core:** no I/O. `request::normalize` (allowlist of fields and tools, history-only, forces `stream:true` and `store:false`, Responses Lite shape for Lite models), `wire::detect` (SSE vs JSON by body, not MIME), `failure` (fixed codes → HTTP status + message; never upstream bodies), `protocol` (invite codes, verification code, peer status, provider/capability enums).
- **aillies-codex:** `CodexOAuth::read` (rejects API-key logins, expired tokens and account mismatches), `Executor` (no redirects, proxy, cookies or retries; connect failure = nothing sent, any later failure = *delivery uncertain*), `config` (statement-level TOML merge that never reserializes, receipt-based restore that refuses to overwrite later edits).
- **aillies-node:** `identity` (P-256 self-signed cert; identity = SHA-256 fingerprint), `tls` (TLS 1.3 only; server pinned by fingerprint; client certs required, authorized per request against the approved list), `peer_server`, `peer_client`, `loopback`, `control`, `store` (state.json + secrets.json, 0600), `node` (orchestration used by CLI and desktop).
- **aillies-cli** and **apps/desktop**: thin shells over `Node`.

## Pairing

1. B (`create_invite`) turns sharing on and makes a one-time invite: name, platform, candidate addresses, B’s fingerprint, 32-byte secret. Expires in 10 minutes.
2. A connects to the first address that presents B’s fingerprint, sends the secret plus its own name; its client certificate is A’s identity.
3. B consumes the secret and records a pending request. Both sides compute `verification_code(B_fp, A_fp, secret)` and show it. A also checks that B’s returned code equals its own.
4. A’s person confirms; B’s person approves. A polls until approved, then saves B. B saves A as an approved client only when its person approves.

Cancelling on A withdraws the request on B. Rejection or expiry leaves no record on either side.

## A Codex request

1. Codex POSTs to `127.0.0.1:18522/v1/responses` with the local key from `config.toml`.
2. Routing paused → `routing_paused`. Destination *This device* → local executor with A’s own login.
3. Destination peer → connect (pinned) → on failure `peer_unavailable` (nothing sent). Send once. A write or read error before the response → `delivery_uncertain`.
4. B checks: approved client, sharing on, 4 concurrent turns max, body ≤ 16 MiB, then reads its login, checks it is the account approved when sharing started, normalizes the request and calls upstream.
5. SSE bytes stream back unchanged; a completed JSON response becomes a finite SSE sequence. A watches for a terminal event; a cut stream is recorded as *delivery uncertain*.

Revoking a client or turning sharing off drops every live peer connection.

## Adding Claude later

The protocol already carries `Provider::{Codex, Claude}` in peer status. Add an `aillies-claude` crate with its own login reader and executor, a peer route `/v1/claude/messages`, a loopback route Claude Code can target, and a provider field on destinations. The pairing, transport and failure model stay the same.
