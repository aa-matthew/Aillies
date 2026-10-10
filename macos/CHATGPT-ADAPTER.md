# Official local ChatGPT app: experimental adapter investigation

## Goal

Keep the official ChatGPT app running locally while a selected trusted Mac supplies the ChatGPT account, including text, images, files, and conversation history. Remote desktop and a substitute chat interface do not satisfy this requirement.

## Evidence collected on 2026-10-10

Read-only inspection of the installed `/Applications/ChatGPT.app` version `26.930.61225` and its packaged application code found:

- The Electron app includes both a Codex app-server connection and separate ChatGPT cloud-chat code. Provider configuration for Codex does not establish support for ChatGPT cloud chats.
- `application-network-startup-BEAX-hka.js` maps `CODEX_APP_SERVER_CHATGPT_BASE_URL` to the Codex app-server `chatgpt_base_url` setting. This is not evidence of a global ChatGPT endpoint override.
- `app-initial-69cd8dbddec5.js` contains streaming calls to `/f/conversation` and preparation calls to `/f/conversation/prepare`.
- `main-C7cfj__D.js` includes conversation-history calls, authentication handling, and a DeviceCheck token path with an `x-sentinel-dc` header. The conditions under which this path applies have not been verified live.
- Other packaged modules contain file download/resolution and realtime-service paths. Image-generation conversations therefore cannot be treated as a single Codex `/responses` request.

These are static observations of shipped application code, not a live trace. Code may be unused, gated, or version-specific. No private chats, credentials, browser profiles, or session cookies were inspected. The installed app was not patched or re-signed.

Official configuration documentation describes `chatgpt_base_url` as a login-flow override and describes custom model providers; it does not establish a complete ChatGPT conversation/image proxy contract:
https://learn.chatgpt.com/docs/config-file/config-reference

## Implemented experiment: encrypted connection diagnostics

The native Client Support panel opens a first-stage ChatGPT diagnostic view. It provides an opt-in loopback CONNECT proxy and a copyable app launch command. The app must be fully quit and relaunched by the user; launch arguments passed to an already-running app may have no effect.

The proxy is deliberately separate from the paired Codex relay. It observes destination hosts and TCP outcomes, forwards TLS bytes without decrypting them, caps concurrent connections and header sizes, bounds in-memory observations, and rejects destinations outside its documented HTTPS scope. No system proxy or certificate-trust setting changes occur. Closing the sheet keeps the listener alive; stopping diagnostics interrupts its proxied connections.

Tests cover fragmented CONNECT headers, bytes arriving immediately after the headers, authority validation, HTTPS destination scope, a real loopback TCP tunnel carrying binary data across multiple buffers, and an out-of-scope rejection. This does not verify the official app respects the launch flag.

## Next investigation gates

1. On the actual ChatGPT build, confirm text, image creation, upload/download, reconnect, cancellation, and history produce diagnostic connections. Correlate manually with actions; hostnames alone cannot identify API calls.
2. If paths bypass Electron proxy configuration, identify their supported networking configuration before making system-wide changes. Do not equate missing observations with no traffic.
3. Establish a consented, narrowly scoped way to observe application-level requests and responses. Certificate pinning, OS trust, attestation, and upstream authentication remain unknown. Stop and document incompatibility if ordinary supported trust mechanisms cannot make the experiment work; no attestation bypass is implemented.
4. Establish whether the peer can perform those exact operations through its own authenticated app/session. The bundled Codex OAuth session is not interchangeable with a ChatGPT desktop session.
5. Only after both sides work independently, prototype an explicit endpoint adapter with peer-owned conversation/file IDs, streaming, cancellation, reconnect behavior, and no implicit account fallback. Do not copy the peer's long-lived credentials into the requesting Mac.

## Acceptance before enabling a “ChatGPT supported” switch

- The local official app obtains its account-scoped capabilities through the selected peer, and account identity is verified independently of transport reachability.
- New conversations, text streaming, image generation/render/download, uploads, and history work end to end on two Macs.
- Conversation/file IDs remain associated with their owning account. Switching peers does not silently reuse foreign IDs or repeat a submitted prompt.
- Expired peer login, offline peer, permission denial, client updates, and revocation produce explicit failures without direct-account fallback.
- TLS validation, narrowly scoped access, sensitive-data storage, diagnostic redaction, rollback, and local normal-app restoration are checked.

## Recorded stage-1 result

The user's 2026-10-10 recording showed established diagnostic TCP tunnels to `chatgpt.com`, `ws.chatgpt.com`, `chat.openai.com`, and first-party CDN/content hosts. Separate screenshots showed a successful text response and generated image. This establishes that some official-app traffic reached the loopback diagnostic proxy; it does not establish that all text/image operations traversed it, identify API paths, or verify another account. Google dependencies and one other destination were blocked by the diagnostic allowlist.

## Stage 2 implementation

Version 0.3 adds a native Chromium NetLog preparation/import/export flow using Electron's documented launch flags. The user relaunches ChatGPT manually, performs a short test, and quits it to finish the log. The trace runs without the earlier restricted proxy. The app locally derives a sanitized request list using event type constants and same-source response headers; uncorrelated HTTP/2-session headers are deliberately ignored.

Only allowlisted request metadata is exported. Query strings, path identifiers, arbitrary subdomains, headers, bodies, source IDs and timestamps are discarded. Raw traces are created in a private folder and stay on the user's Mac for manual deletion. Tests inject credentials, cookies, prompts, signed URLs and unrelated sites to verify they do not enter the exported representation.

References: https://www.electronjs.org/docs/latest/api/command-line-switches and https://www.electronjs.org/docs/latest/api/net-log/

Current result: recorded official-app host-level traffic observed; native API-metadata tracing implemented and locally tested; live NetLog capture, application-level compatibility, image behavior through a peer, and peer-account substitution remain unverified.


## Live NetLog result and importer fixes (0.3.1)

The user supplied a complete NetLog on 2026-10-10. The local sanitizer examined 199,537 events and derived 1,215 scoped request starts without hitting the event/request caps. It observed HTTP 200 JSON responses for `/backend-api/f/conversation/prepare`, conversation history, image bootstrap, and scoped file-library requests; HTTP 101 upgrades at `ws.chatgpt.com`; and HTTP 200 image responses under redacted `/images/…` paths. There were no observed `/backend-api/f/conversation` requests or directly labeled SSE responses. This suggests a separate messaging path deserves investigation; it does not prove which WebSocket carries generation output.

The original raw file exceeded the initial importer size cap. Version 0.3.1 permits completed files up to 128 MB while retaining the 200,000-event and 5,000-request caps. A real Chromium `URL_REQUEST_START_JOB` BEGIN/END pair also exposed a correlation bug: the END event has no URL and must not clear the active request. A regression test now verifies response correlation survives this event. The corrected sanitized report records response-byte events for 1,189 requests, coarse network errors for 12, and no such outcome for 14. Counts cover the whole app trace, not just the two test prompts.

Only sanitized metadata was emitted for inspection. The raw log was not copied to the repository. The derived report is ignored by Git. WebSocket frame payloads, conversation request/response bodies, account ownership, and peer-account substitution remain unverified. Successful image responses alone do not establish that every generated-image operation traversed a peer.

## Protocol investigation and frame-shape tracing (0.4)

The installed package inspected in this stage is version `26.1007.21159`; it differs from the earlier inspected package. `chatgpt-protocol-evidence.json` records its archive SHA-256, fixed search evidence, and module names. Reproduce that read-only inspection with `python3 scripts/inspect-chatgpt.py`. The scanner reads shipped JS and Info.plist only, executes no application code, and emits no source excerpts or private-profile contents.

### Static pub/sub contract

The shipped `app-shared` module contains these structures (placeholders describe types, not captured user values):

- Outbound commands are batched in an array of `{id: number, command: object}` envelopes. Opening the connection sends a `connect` command with `presence` state. Foreground/background changes send presence updates.
- Incoming array entries can be `{id: number, reply: unknown}` command replies or `{type: "message", topic_id: string, payload: unknown, offset?: string}` topic messages.
- Subscription state includes `topic_id`, `recovered`, optional `last_offset`, and optional catch-up message entries.
- Incoming object message types include `conversation-update`, `conversation-history-update`, `conversation-turn-handoff-control`, `voice-work-continuation-ready`, and `voice-conversation-commit`. The client routes these to the conversations topic.
- The source contains reconnect/recovery and topic subscriptions. A WebSocket connection can therefore carry notification and synchronization traffic; it must not be equated with a prompt-generation stream.

These are definitions from the shipped code. The NetLog does not establish that a particular JSON envelope or type was actually sent during the user's test. The scanner is version-specific evidence, not a supported public API contract.

### Static conversation HTTP path and identity boundaries

The renderer still contains `streamPost('/f/conversation', …)` and an SSE decoder with a `[DONE]` marker. It delegates HTTP fetching to the desktop `httpFetch` service. The native host service obtains backend authentication, may rewrite a URL to the authenticated workspace's backend origin, and checks account/user consistency; a mismatch can abort with `Authenticated principal changed` or `Authenticated workspace changed`. It also contains messaging-room endpoints and separately scoped integrity/DeviceCheck handling.

The exact HTTP path used for the recorded text/image turns is still not established. Finding a legacy streaming path in a newer package does not establish that it was active in the earlier recording. The source does not establish a supported mechanism to substitute a peer account across all these stateful boundaries. No principal-check, attestation, or certificate-validation bypass is implemented.

### Implemented bounded trace extension

The native trace importer now includes WebSocket frame-header metadata from Default-mode NetLog: direction, opcode category (text/binary/continuation/ping/pong/close/reserved), a coarse payload-size band, and optional FIN/masking booleans. It never reads or exports frame payloads. The export keeps at most 100 socket records and 5,000 frame shapes, in addition to the previous event/request caps. Frame order is preserved within each derived record; source IDs and timestamps are omitted.

Known first-party URLs are redacted with the same endpoint policy as HTTP. Socket URLs omitted by NetLog are explicitly marked unattributed. Frame sources without a matched socket record are exported as a separate app-wide aggregate; they are not guessed from nearby source IDs or handshake timing and may overlap connection records. Explicitly out-of-scope socket sources are excluded. Missing data is not interpreted as absence of traffic.

The public endpoint vocabulary also recognizes `celsius`, `messaging`, `rooms`, `messages`, and other fixed shipped route segments, while identifiers and query values remain redacted. The existing recording contains HTTP 200 requests to `/backend-api/celsius/ws/user`. Its WebSocket payload content and relationship to generation output remain unknown.

Tests inject prompt text, image bytes, cookies, token-bearing URLs, unrelated hosts, missing socket URLs, mismatched sources, invalid frame lengths, and excessive frame counts. No injected payload value is included in the derived export. This stage reprocesses the existing trace and requires no repeated model inference.

Current limit: default NetLog provides frame headers, not JSON payloads. Live JSON message-type tracing would need an additional app-provided instrumentation interface or an explicitly scoped debugging mechanism. None has been established by this investigation. The app is not patched, no broad debugger is exposed, and ChatGPT peer-account routing remains unimplemented.

## Implemented transport boundary (0.5)

The native **Encrypted network relay** now forwards scoped CONNECT tunnels through a selected Mac over pinned TLS. This carries the official app's existing inner TLS without decrypting it. Relay access uses a separate private pairing key, not a ChatGPT session. The middleman does not need a ChatGPT login. Host identities are loaded into memory only on macOS 15+, with no trust installation. Codex OAuth sharing remains a separate adapter and pairing format.

Tests establish raw binary transport, peer authentication, certificate pin rejection, both endpoint scope checks, and live tunnel shutdown. The app must honor its launch proxy flag for a particular network path to use this transport; all-path enforcement and two-Mac text/image behavior remain unverified. The current local ChatGPT identity remains authoritative, so this implementation does not achieve the original peer-account goal. No account substitution or token extraction is included.

## Scoped TLS compatibility experiment (0.6)

The next executable gate is now available in the native **TLS compatibility experiment** panel. It prepares a private, separate mitmproxy 12.2.3 CA and a locked Python helper. The user must explicitly trust the public CA certificate in the login keychain, launch ChatGPT with the loopback proxy, run one harmless text/image turn, stop, remove that trust, and reopen ChatGPT normally. Peer Route does not modify trust, patch the application, bypass certificate checks, quit an active app, or extract its saved credentials. No TLS session key export or shared CA private key is needed for the existing A-to-B encrypted transport.

Only exact `chatgpt.com:443` and `ws.chatgpt.com:443` connections are selected for local TLS interception. Other hosts pass through without decryption. The optional probe is separate from ports 18317/18318/18319 and binds `127.0.0.1:18320`. It keeps upstream certificate verification enabled. If ChatGPT rejects this ordinary local CA trust mechanism, the compatibility experiment stops there; no pinning bypass is provided.

This stage still executes with **A's current account**. It records a bounded, sanitized schema report: redacted path templates, HTTP method/status, authentication *header names present*, and fixed-vocabulary JSON field/type shapes from SSE and WebSocket messages. Request bodies are streamed without inspection. Response text, image data, cookie/token values, arbitrary key names, identifiers, query values and timestamps are not exported. SSE buffers are bounded at 64 KiB; WebSocket shape parsing is limited to messages no larger than 64 KiB. Larger messages are marked opaque. Export is capped by event count and total size, with private atomic file writes. Compressed SSE may remain opaque. Raw flow files and TLS key logs are not written.

The real local fixture establishes CA-based TLS interception, SSE delivery before completion, unchanged authentication forwarding, unchanged binary bytes and secret-free reports without contacting OpenAI or modifying system trust. It does not establish ChatGPT compatibility or peer-account execution. Python 3.12+ is an additional prerequisite for this optional experiment; the native app installs hash-locked helper dependencies on explicit request. No independently managed proxy process is required.

B-account execution remains gated on independently verifying B's authorized session and an adapter that A's app accepts. The official Sign in with ChatGPT plan-usage route supports streamed Responses requests but explicitly excludes the image-generation tool and does not target ChatGPT backend-api endpoints. It cannot be used as proof of desktop image support:

- https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference
- https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations

The next user-run test is **CA acceptance plus live generation path/schema observation**. No peer-account toggle has been enabled, and no unverified conversation or integrity-header rewriting is implemented.

## Supplied TLS schema report and retention fix (0.6.1)

The supplied report contains 1,200 events and explicitly reports truncation: 606 requests, 555 responses and 39 WebSocket message shapes. It includes ten HTTP 200 image responses, nine WebSocket upgrades, and conversation preparation responses of HTTP 403 and subsequently HTTP 200. It contains no retained SSE response/event or conversation submission. This confirms successful ordinary local CA interception for some official-app traffic, but neither image generation nor full generation-path coverage nor peer-account execution. Image downloads can also be thumbnails or other existing assets. The 403 response does not independently identify its cause.

Startup asset/background events exhausted the report's global event budget. Version 0.6.1 filters ordinary asset traffic, bounds background events to 300 of 1,200 slots, reserves capacity for generation, image, SSE and WebSocket evidence, and aggregates repeated stream shapes with occurrence counts. It also counts fixed known CONNECT destinations without expanding the two-host TLS interception scope. The report includes separate background-limit and important-event-loss metadata. A regression test floods background events before generation and verifies the generation evidence survives. The already truncated report cannot recover discarded events; a short fresh text/image run is needed. The prepared helper/CA can be reused; no new trust installation is performed by this update.

## Follow-up capture and next gate

The follow-up report contains 337 rows, two successful conversation-preparation responses, fourteen HTTP 200 image responses, and WebSocket shapes. It reports background limiting but zero important events dropped. No conversation submission or SSE response/event was identified. Image downloads and preparation remain insufficient to establish generation capture. The stopped listener and disabled peer-account flag are expected for this completed local experiment.

Read-only inspection of the installed app's shipped `app-initial` code finds `chat.openai.com` in a telemetry intake URL (`/ces/v1/telemetry/intake`). This is a concrete alternative explanation for the three observed CONNECTs, not proof that every connection to that host was telemetry. It does not justify expanding TLS interception to that host as the next generation experiment.

The shipped desktop HTTP service delegates requests to `applicationNetwork.fetch` or an upload-progress request path after authentication and principal checks. This establishes a service boundary, not whether its runtime transport honors the Chromium proxy switch. The next gate is to identify that networking implementation and correlate one explicitly bounded generation action with proxy coverage. Missing capture alone must not be called proof of bypass. Do not broaden trust scope or implement speculative credential/header rewriting. B-account execution still needs independent verification of B's authorized execution path, peer-owned conversation IDs, streaming and cancellation before an official-app adapter can be enabled.

## Networking implementation and targeted coverage update (0.6.2)

Further shipped-code inspection resolves `applicationNetwork.fetch` to Electron `net.fetch`. A separate durable/cloud app-server transport uses the Node `ws` implementation with a fixed `wss://codex-cloud-backend.chatgpt.com/` origin and system/default CA lists. Its cloud connection does not use the explicit SOCKS agent supplied for some other host kinds. Static code does not establish whether the user's captured generation turns used this transport, but it identifies a network path that requires separate coverage.

The generic `CODEX_APP_SERVER_WS_URL` override was considered and rejected: the cloud-specific selector reads the fixed host configuration directly, whereas the generic override affects another host kind. No URL-rewriting bridge or application patch is shipped. The existing cloud endpoint, authentication and principal checks are preserved.

Version 0.6.2 adds an experimental process-scoped Node environment-proxy option to both the native relay launch and TLS capture launch, using Node's documented built-in proxy support. Both proxy-variable cases are set to avoid an inherited lowercase variable taking precedence. Only local destinations are excluded. The installed framework contains the corresponding built-in Node support; a standalone Node 24.14 local HTTPS-upgrade fixture verifies streaming through CONNECT and failure without direct fallback. Embedded-runtime behavior remains unverified. See https://nodejs.org/api/http.html#built-in-proxy-support .

The local TLS experiment now explicitly discloses and selects the exact cloud hostname on port 443 in addition to its original two hosts. It retains upstream validation and fixed-vocabulary schema redaction; known JSON-RPC method categories can be counted without exporting prompts, tokens, IDs or arbitrary method names. Ordinary relay mode still carries encrypted TLS bytes and requires no diagnostic CA.

The next live gate is one bounded launch using the native Node-aware capture option, checking for cloud CONNECT/upgrade/method evidence and ordinary text/image behavior. If the app ignores the option, this build must not claim that route is covered. Even successful coverage would retain A's identity. B-account execution remains a separate unresolved adapter requirement, with no peer-account switch enabled.

## B-account implementation preflight

The subsequent local preflight found no saved peer pairing and no Peer Route host OAuth session. The available schema report is still the 337-row capture: no cloud-host CONNECT, conversation submission or SSE response. Consequently there is no reachable, authorized B session against which to validate an account adapter, nor a live generation contract to translate. These are missing test prerequisites, not evidence that account substitution works or that it is impossible.

Current official Sign in with ChatGPT documentation was checked again. The documented plan-usage route supports streaming Responses requests but excludes image generation; its sign-in integration does not grant access to ChatGPT conversations. It must not be represented as a complete official-desktop account adapter. References:

- https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations
- https://developers.openai.com/cookbook/articles/sign-in-with-chatgpt

Next required user setup: run v0.7 on B, enable sharing and AirDrop its pairing to A; open it and confirm the matching device code. Pairing authorizes the network relay only. B still needs an independently authorized generation/session path before account execution can be enabled. A new release must not relabel existing transport as B-account sharing, silently fall back to A, or charge API billing while claiming ChatGPT-plan usage. No speculative cookie, integrity-token or conversation-ID rewriting was added during this preflight.


## 0.8 peer turn executor

Implemented an original Swift executor using the Codex backend request/event contract inspected in decolua/9router commit ce4460ef79382bfddb4aa5fc0ff9f3cb0d5f95a8. It references a user-selected B-side Codex auth.json via a local bookmark, rereads access credentials per turn, checks expiry and account consistency, and never uses the refresh token. It exposes authenticated HTTP Responses over existing pinned peer TLS, with a loopback gateway on A:18322. Text and image SSE are relayed with backpressure; the test UI validates completed text or a decodable final image. HTTP-only Codex provider configuration reuses private backup/restore and disables unsupported model discovery/WebSockets. Full input history is required; server item IDs are removed while opaque reasoning content is preserved.

The main UI and tray were simplified; pairing, account selection and generation checks no longer require opening diagnostics. Bounds, no redirects/retries/fallback, host-side credentials and missing/changed-account rejection are locally tested with a mock upstream. No B is available for live testing. This does not implement the official ChatGPT conversation or cloud JSON-RPC UI adapter, does not reuse desktop cookies, and does not establish image entitlement or quota behavior on any real account. The network-only ChatGPT launch remains explicitly separate.


## 0.8.1 generation rejection investigation

A live screenshot showed a peer connected but a text check rejected using gpt-5.6-luna. The v0.8 error discarded all upstream/local status details. A bounded, read-only local diagnostic could not access the pairing key, so no diagnostic request or inference was sent; the original live failure cause remains unconfirmed.

Source review found the native check passed a string input where 9router normalizes Codex input to message arrays. The stricter mock now validates actual outgoing request bodies (including URLProtocol body streams). Also implemented the known GPT-5.6 Luna text Responses Lite header/prefix from inspected 9router source, an authenticated no-inference peer/login-file preflight, and fixed-code errors preserving upstream HTTP status without copying upstream bodies. Existing mock tests cover plaintext input normalization, Lite text versus ordinary image shapes, no-account hosts, ready/missing/changed local login state, and HTTP 400 redaction. B must be updated and live generation retried before declaring this screenshot resolved. Official ChatGPT UI translation remains unfinished.


## 0.8.2 HTTP 200 format investigation

The v0.8.1 screenshot maps specifically to the successful-HTTP/non-SSE-MIME guard in TurnExecutor. This proves the backend responded HTTP 200, not that the model generated a successful answer. No body was captured. Read-only review of the same pinned 9router source showed its executor/image adapter parses SSE without requiring a particular MIME header.

Removed that MIME gate in favor of bounded body-prefix detection. SSE preserves original bytes; a fully completed Responses JSON object is converted to finite completed-item/completed events without claiming incremental output. Other JSON, HTML and incomplete results fail with fixed safe errors. The pinned TLS fixture covers missing/wrong MIME and completed JSON text, plus sanitized JSON-error/HTML rejection. No live B test has been run for this release. Official ChatGPT UI account translation remains unfinished.
