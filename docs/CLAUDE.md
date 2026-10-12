# Claude integration — 1.1 (experimental)

Version 1.1 includes **Connect account**, following OmniRoute’s explicit OAuth authorization-code/PKCE and refresh strategy. One connected account on the executing Mac serves both Desktop Chat and Claude Code. Aillies keeps its own connection in its private app-data folder; the normal desktop/Code login and history are untouched. Earlier private desktop-cache/native Code readers remain explicit legacy CLI sources, never automatic fallbacks.

## Connect and share

On the executing Mac, open **Aillies Share → Claude → Connect account**. Aillies opens Claude’s authorization page in your default browser. Choose/authorize the intended account, then paste the complete one-time `code#state` into Aillies and select **Connect**. Aillies validates the PKCE-bound code, inference scope, account and organization before saving. An authorization can be used once and expires after ten minutes; Cancel discards it. Opening the browser again uses the same pending authorization. If opening fails, the local UI provides the authorization link. Nothing is automatically shared after connecting.

Turn on **Share Claude**, pair devices and explicitly allow Claude for the requester. The sharing Mac may leave its client provider Normal. Changing/disconnecting the account requires sharing off. Disconnect deletes Aillies’ local credential and leaves native logins unchanged; it does not revoke other Claude sessions. A connected account is also available when this Mac is selected locally. Authentication errors never trigger a native-cache/Code/API-key fallback.

Connecting accounts is currently **Mac only**. Windows/Linux can select a paired Mac; their native requester configuration remains physically unverified. This is experimental: native browser authorization, private-file persistence, desktop gateway startup, live Anthropic subscription access/billing and tools require real acceptance. Mock success does not prove live entitlement.

The UI requires a connected account before enabling Aillies with **This device**, or changing the destination back to this device. A requester selecting a paired Mac does not need its own connected account. Legacy native sources remain explicit CLI choices.

## Desktop Chat setup

Update both devices. Connect/share as above on B. On the requester, choose the paired Mac under **Aillies Use → Claude device**. Turn on Claude activation, review affected clients and choose **Quit, switch & reopen**. Claude Code sessions/editors close and stay closed; Claude desktop reopens. The same switch configures both clients. Aillies creates a separate native gateway configuration in `Claude-3p/configLibrary`, with its own private local bearer. It does not use Claude Code’s `settings.json`. Turning Claude activation off removes Aillies-owned settings and restores the previous selection while preserving other edits. Aillies refuses to switch while Claude is running on Mac, or when managed/bootstrap settings would override the local configuration. No apps are closed or reopened automatically.

Gateway-mode chats are local and separate from normal Claude cloud chats. Returning to Normal restores access to the normal account’s cloud history. The account name shown in normal Claude is not evidence of which paired device executes a gateway request. Aillies shows the selected device; it does not replace the owner of existing cloud conversations.

## Claude Code setup

Choose **Claude Code** in the client selector. It uses the same explicitly connected Aillies account; connect/share and allow the requester as above. On the requester select the device and Aillies, then restart Claude Code manually in the original working folder and resume (`claude --resume`). Aillies never edits transcripts, session IDs, project trust or working directories. Selecting a paired device affects the next turn without rewriting history. Do not switch during a turn: a retry stops if its destination changes.

**Normal** restores only Aillies-owned settings, preserving later edits. A private receipt recovers interrupted installs. Keep the same `CLAUDE_CONFIG_DIR` when restoring. Malformed settings, symlinks and conflicting cloud backends refuse without edits. Project/managed settings, command-line settings and editor environments can override user settings. Installed Claude Code 2.1.227 is the compatibility probe; older clients are unverified. Aillies never reopens Terminal or VS Code.

## Request path and ownership

```text
Claude desktop Chat or Claude Code on A
  -> 127.0.0.1 /claude-desktop/v1/messages or /v1/messages (A's dedicated local Claude bearer)
  -> pinned mutual TLS /v1/claude/messages
  -> B checks its explicit Claude grant and sharing state
  -> api.anthropic.com/v1/messages (B’s Claude OAuth Bearer token)
```

The local conversation, approvals and tool execution belong to A. Anthropic authentication and usage belong to B. A’s local Claude bearer, control token and Codex key are different; none is forwarded to Anthropic. B’s OAuth token and private account/token digests are never exposed through peer status, diagnostics or responses. Read credentials again for each attempt, requiring the account and token pinned for that turn. Live account entitlement and usage accounting are not established by mock tests.

Native model, system/cache markers, tool IDs/results and signed history retain their exact JSON bytes. Alpha.7 normalizes only reviewed top-level thinking, effort and sampling options; see the compatibility policy below. Preserve the allowed `anthropic-version`, native beta headers and actual client User-Agent; add required `oauth-2025-04-20` and `claude-code-20250219` betas, `x-app: cli` and Bearer authorization. The adapter preserves `?beta=true`. It does not translate to OpenAI, repair signatures, compress history, choose another model/account/device or fall back to a direct route. Claude image generation is not advertised or routed to Codex; native image inputs remain native Messages content.

Supported endpoints are Messages (streaming and finite JSON), `/v1/messages/count_tokens`, and authenticated `GET /v1/models`, over both loopback and peer transport. Desktop Chat uses the `/claude-desktop` prefix; Claude Code uses `/v1/models` directly. Discovery forwards the selected executing account's native Anthropic catalog, including capabilities and pagination metadata, with no synthetic list or account/device fallback. It accepts bounded `limit` (1–1000), one `after_id` or `before_id`, up to three distinct lifecycle filters (`active`, `deprecated`, `retired`), and optional `beta=true`. GET bodies, unknown/duplicate parameters, invalid cursors and malformed catalogs refuse. Both clients require the dedicated local Claude bearer; peer discovery also requires sharing and a Claude grant. Upstream OAuth acceptance/entitlement remains a live acceptance gate.

Alpha.6 enables desktop model discovery by default and removes its overriding static list. Existing exact Aillies-owned profiles are upgraded automatically when the node starts, using a receipt intent that can resume or restore either side of an interrupted write. Externally edited/unsafe profiles refuse without mutation and show a fixed warning. No new UI setting is added. Restart Claude desktop to read its updated discovery configuration. Code exposes the same catalog endpoint without additional user-settings keys; how a particular installed Code version presents that catalog remains client behavior.

The allowed top-level request fields are deliberate; new Claude fields fail closed until reviewed. The current protocol version remains 1, with its reserved Claude provider activated. Upgrade both peers for Claude; older peers cannot execute discovery.

## Connected OAuth lifecycle

The public native PKCE client/redirect/scopes match the pinned OmniRoute source: `claude.ai/oauth/authorize`, S256, random 256-bit verifier/state, manual `platform.claude.com/oauth/code/callback`, `prompt=login`. Claude controls browser authentication/consent; reuse without reauthentication is not promised. Only a complete matching `code#state` is accepted; verifier remains backend-memory only. A newer attempt or cancellation invalidates older exchanges, including a response arriving after cancellation. Codes never enter status, diagnostics, process arguments or disk. Native secrets are not imported or overwritten.

Exchange/refresh use fixed `api.anthropic.com/v1/oauth/token`; authenticated bootstrap on the same origin supplies missing account/org identity. Contradictory or ambiguous identities refuse; no synthetic UUID/account guessing. Scope must include `user:inference`, token type must be Bearer when supplied, expiry/token sizes are bounded. Responses are bounded to 64 KiB; redirects and proxy inheritance are disabled. Errors are static messages; raw upstream bodies/URLs with secrets are not logged.

Connected access/refresh tokens and the refresh-intent marker are stored in `accounts/claude.json` under Aillies’ private home (normally `~/Library/Application Support/is.anyax.Aillies`). The accounts directory is 0700 and file 0600; bounded regular-file/symlink checks and atomic rename with file/directory sync protect updates. This is less protected than Keychain: another process running as the user can read the file. Tokens are never installed in client settings, diagnostics or peer traffic; clients receive only a distinct loopback bearer. Aillies state stores a hashed account/org selection.

Old Aillies Keychain connections are not read or migrated automatically. A selected account with no private file disables sharing and shows Reconnect. Explicit reconnect creates the private connection and leaves the old Keychain item alone; no transfer prompt is required by Aillies. A future explicit transfer could require one final OS prompt. Synthetic `NodeConfig.claude_home` roots use a fixture-only private file and disable real Keychain; never point test homes at live data. Production Windows/Linux local connection storage remains unsupported.

Before a new turn (or Share), renew within five minutes of expiry under a per-node refresh mutex. Persist a refresh-intent marker before sending because rotation may consume a refresh token even when the response is lost. Save new access/rotated refresh credentials before use; absent rotated refresh token preserves the prior one. Account/org must remain identical. Any unsuccessful, uncertain or unsaved refresh leaves the connection blocked until reconnect; it is never silently replayed. Each logical turn pins the resulting token; retries never refresh/rebind. Concurrent new turns serialize refresh. Status polling never starts an exchange/refresh and makes no credential reads while sharing is off. A connection badge indicates saved connection metadata, not verified live entitlement.

## Legacy explicit native sources and client settings

The following readers are retained for explicit `--source desktop|code` compatibility. The new Connect account UI does not invoke them.
Desktop sharing reads the normal Claude `config.json`, matches `lastKnownAccountUuid`, and decrypts `oauth:tokenCacheV2` (legacy cache only when V2 is absent) with the exact local `Claude Safe Storage` / `Claude` Keychain item. It validates account, organization, fixed API origin, inference scope and expiry. Prefer the native pure-inference desktop client entry; otherwise require a single inference candidate. Ambiguity refuses. Only the selected account’s access token is used; refresh tokens/cookies are ignored. Unknown ciphertext formats refuse. The Safe Storage password is cached only in process memory, while account/token data is re-read each attempt. No login files or Keychain items are written.

Claude Code uses its independent reader. The executing device reads an explicitly configured OAuth token from the original user-settings environment (`CLAUDE_CODE_OAUTH_TOKEN` or `ANTHROPIC_AUTH_TOKEN`). Otherwise it reads Claude Code’s native macOS Keychain item, or its `.credentials.json` when the item is absent/non-Mac. Native credentials use `claudeAiOauth.accessToken`, `expiresAt` and `scopes`; `.claude.json` supplies account/organization metadata when present. Expired, malformed, conflicting or denied credentials fail closed. Explicit API-key/alternate-backend settings are refused; there is no API-key fallback.

The default native Keychain service is `Claude Code-credentials`, matched to the OS username. Custom config/secure-storage roots use Claude Code’s eight-character SHA256 suffix of the NFC-normalized path. `CLAUDE_SECURESTORAGE_CONFIG_DIR` overrides credential storage; an explicitly empty override selects default storage. Test-only explicit Node config roots disable Keychain reads entirely.

Aillies never refreshes or overwrites native login data. Let Claude Code on the owner’s device renew expired login. Native same-account token refresh works between turns; changing account requires enabling sharing again. Token/account changes during a retry stop immediately. Setup tokens without reliable account metadata bind conservatively to their token digest and require fresh consent after rotation.

Requester `settings.json` contains the loopback base URL and a separate local `ANTHROPIC_AUTH_TOKEN`. Aillies masks configured owner OAuth and disables alternate backends/client retries only in its owned settings subset. The original values remain in a private restore receipt; restore preserves later edits. Native Keychain/credential files are unchanged. Do not publish the receipt or login files. Project/managed/shell overrides may supersede user settings; use the intended CLI environment. Alpha.1 API-key files are unused, and upgrading disables old Claude sharing consent while retaining pairings/grants.

## Retries, completion and diagnostics

Aillies reuses one requester controller: initial attempt plus at most three retries, 500/1000/2000 ms backoff, 240-second desktop turn deadline (below the native gateway’s five-minute silent-stream timeout) or 880-second Claude Code deadline and 64 MiB response cap. The same selected device, Claude account, token and model remain pinned. The sharing executor performs each attempt once. Installed Claude settings disable client retries and request compression; responses include `x-should-retry: false`. Authentication, permission and validation refusals stop, including framed native SSE errors. Cancellation and route/account/token changes stop further attempts. Earlier attempts can consume usage.

Generation is buffered until a framed `message_stop`; failed/partial attempts are discarded. Claude defers HTTP 200 until the winning attempt completes: the installed client otherwise replays empty failed streams even with its retry setting disabled. Failures return a fixed HTTP envelope with `x-should-retry:false`. Owned `API_TIMEOUT_MS=900000` accommodates the 880-second budget. Codex retains its comment keepalives. `tool_use` is a successful response termination: Claude Code executes the tool locally. A framed `error` becomes a fixed, content-free failure. Error text from Anthropic is never copied into logs or client failures. Finite JSON and token counts are verified before returning them.

The existing three independent latest-100 rings now include a closed provider category and bounded numeric metadata. Home retains the latest four fixed warnings. No request/response bodies, signed thinking, headers or credentials enter these journals.

## Developer commands

```sh
# On B: after connecting in Aillies, share that connection.
aillies claude share on --source connected
# Legacy explicit native sources remain: --source desktop or --source code.
# Authenticated local control also exposes claude/connect/start, /finish, /cancel and claude/disconnect.
# Never pass authorization codes in process arguments or paste them in logs.
# Pair with the existing invite/pair commands, then grant explicitly on B.
aillies claude grant FULL_REQUESTER_FINGERPRINT on
# On A:
aillies claude use DEVICE_NAME_OR_FINGERPRINT
aillies claude on --desktop
# To restore A's ordinary desktop configuration:
aillies claude off --desktop
# Omit --desktop for Claude Code settings.
```

`CLAUDE_CONFIG_DIR` selects the user settings/history root. `AILLIES_CLAUDE_UPSTREAM` is test-only: a parsed HTTP loopback origin with an explicit port, no credentials/query/fragment/path. Otherwise the executor uses only the fixed HTTPS Anthropic origin, with redirects and proxies disabled. Test homes must be synthetic. `AILLIES_TEST_CLAUDE_HOME` gives CLI smoke daemons an explicit isolated root and disables native Keychain lookup; ordinary custom user roots should use `CLAUDE_CONFIG_DIR`.

Opt-in installed-client probe (no live inference, no user login/history):

```sh
cargo test -p aillies-node --test e2e installed_claude_cli -- --ignored --nocapture
```

The probe runs installed Claude Code against two synthetic nodes and a mock Anthropic server, with other outbound HTTP routed to an unavailable local proxy. It checks a completed answer, authentication refusal, HTTP 503, interrupted SSE and native SSE errors without multiplying Aillies' retry budget. Desktop tests use independently OpenSSL-generated encrypted fixtures and an explicit synthetic Safe Storage password; no real Keychain access. Default unit/e2e tests cover native bytes, grants, distinct local keys, diagnostics, account/token/destination pinning, JSON/token-count endpoints and transactional settings restoration. Physical Macs, live Anthropic model/beta access, billing and real tool/long-thinking sessions remain acceptance work.

## Router references

Source review used 9router `ce4460ef79382bfddb4aa5fc0ff9f3cb0d5f95a8` and OmniRoute `7f3cbb046b3706bf0bbc047b7eaa0cbada8d9e50`. Adopt the narrow provider adapter, native Messages passthrough and OmniRoute’s account import/Bearer OAuth header approach. Reviewed OmniRoute sources include `claudeAuthFile.ts`, `claudeAuthImport.ts`, provider `claude.ts`, `claudeIdentity.ts` and its Claude CLI settings generator. Keep Aillies' own authorization, fixed errors and single retry owner. Do not copy their account fallback, signed-thinking rewrites, body logging or hidden recovery retries.

Official references: [Claude gateways](https://code.claude.com/docs/en/llm-gateway-connect), [gateway protocol](https://code.claude.com/docs/en/llm-gateway-protocol), [settings](https://code.claude.com/docs/en/settings), [local sessions](https://code.claude.com/docs/en/how-claude-code-works), [native streaming](https://platform.claude.com/docs/en/build-with-claude/streaming), [authentication terms](https://code.claude.com/docs/en/legal-and-compliance).

Desktop references: [native configuration](https://claude.com/docs/third-party/claude-desktop/configuration), [gateway protocol](https://claude.com/docs/third-party/claude-desktop/gateway), [local Chat ownership](https://claude.com/docs/third-party/claude-desktop/overview). Installed app source was inspected statically; no user cache or Keychain was read.

Crypto compatibility reference: [Electron 44.4.3 native Safe Storage implementation](https://github.com/electron/electron/blob/v44.4.3/patches/chromium/revert_oscrypt_remove_sync_backend.patch). `v10` AES-128-CBC / PBKDF2-HMAC-SHA1 parameters are used only to read the observed native cache; unsupported formats refuse.


Legacy alpha.4 local Share errors distinguish a malformed settings/restore receipt, a foreign gateway, alternate cloud backend, API-key setup, conflicting OAuth tokens and unsupported configured tokens. Values are never shown. Native failures identify Code versus desktop and do not claim an exact cause where validation cannot tell. The alpha.5 UI uses Connect account for either client instead of guessing a native source. On a sharing Mac, Provider can remain Normal while Share is on; only the requester needs its client provider set to Aillies. Polling does not inspect Claude credentials while sharing is off.

## Frontier model compatibility — alpha.7

Discovery remains automatic on both client endpoints. The executing-device adapter prepares each native Messages/token-count request; it never replaces the selected model. Reviewed native IDs and dated snapshots cover Opus 4.7/4.8/5/5.5, Sonnet 5/5.5, Haiku 5.5, Fable 5/5.1 and Mythos 5/5.1. An unknown future model passes without guessed model rules; discovery visibility is not proof of inference entitlement.

For adaptive-only models, convert manual enabled thinking to adaptive, remove manual budgets and incompatible sampling fields. Always-thinking models normalize disabled to adaptive; Sonnet 5.5 normalizes disabled to between_tools. Preserve display/binding controls where valid, or refuse unsupported combinations before upstream execution. Map unsupported legacy effort to the supported level, remove only effort on older models without it, and retain structured output format. All five current effort levels, including Sonnet 5.5 max, remain available. Disabled/between_tools caps at high on models that require it. Never rewrite a historical per-message effort directive to make a request pass.

Support reviewed speed, native thread, top-level ephemeral cache, system messages, per-message effort and turn-scoped system fields. Add only shape-required beta flags (fast, threads, one-hour cache, per-message effort, clear_at, inline tools, display updates and binding controls), retaining native negotiated betas. Plain system messages and summarized/omitted display need no extra beta. Do not inject long-context entitlement flags. Fast is accepted only on reviewed supporting models; unsupported fast/forced-tool combinations return a fixed HTTP400 without model/tool-choice substitution. Adaptive forced tools remain available on models that support them.

Unchanged fields use borrowed raw JSON slices: model, system, tools and messages preserve exact escapes, whitespace, signatures and tool IDs. A no-op retains the complete original body. Native SSE thinking/signature/tool deltas and terminal tool_use pass unchanged after successful buffering; retries use identical normalization and never mix responses. Diagnostics contain closed model IDs and fast metadata only, never arbitrary model strings/content. Rate-limit HTTP429 is still an account/upstream refusal and is not repaired by this adapter.

Implementation: `aillies-core/src/claude_compat.rs`, used by the shared Claude executor for both clients and local/peer execution. Reference: [OmniRoute pinned core](https://github.com/diegosouzapw/OmniRoute/blob/7f3cbb046b3706bf0bbc047b7eaa0cbada8d9e50/open-sse/handlers/chatCore/upstreamBody.ts); current Anthropic [thinking](https://platform.claude.com/docs/en/build-with-claude/thinking), [effort](https://platform.claude.com/docs/en/build-with-claude/effort), [fast](https://platform.claude.com/docs/en/build-with-claude/fast-mode) and [system messages](https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages). Native thread gating follows the pinned router's inspected native-client evidence; live thread acceptance remains unverified. No identity cloak, history signature repair, account/model fallback or error-driven stripping is imported.
