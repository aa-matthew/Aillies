# Peer Route for macOS — 0.8.3 preview

Native SwiftUI app for paired Mac routing. No SSH or separate proxy runner.

## 0.8.3 Codex configuration merge

Update A, reconnect to the peer, select your verified text model and choose **Use in Codex…** again. B can remain on 0.8.2 for this fix.

Quoted project/MCP table names and multiline arrays are preserved. The merge scans statement boundaries and normalized table/key paths rather than treating every line beginning with a bracket as an unquoted table header. It updates only the root model/provider, model-discovery flag and newly appended Peer Route provider. Other providers retain their model-catalog URLs. Conflicting providers, duplicate managed keys and unsupported multiline strings still fail before any active configuration is written. Existing private backup and concurrent-edit restore checks remain in place.

## 0.8.2 response format fix

Update **both Macs**, reopen Peer Route and re-enable sharing on B, then retry Text once. Existing pairings remain valid.

The 0.8.1 screenshot establishes an upstream HTTP 200 response with a Content-Type other than `text/event-stream`; its body was not captured, so it does not establish successful generation. The adapter now recognizes SSE from its body prefix even when the MIME header is missing or incorrect, matching the inspected 9router approach. Original SSE bytes continue through the paired TLS route. A completed Responses JSON document is converted into completed-item and completion events; this fallback is buffered, not incremental token streaming. HTML, JSON errors and incomplete JSON responses are rejected without exposing their bodies. No automatic retry, account fallback or prompt replay is added.

Local fixtures exercise byte-fragmented streams with correct, missing and incorrect MIME headers, completed JSON text/image output, and error/HTML rejection. Live resolution still requires retrying against B; no private response body or credentials were collected.

## 0.8.1 rejection fix

Update **both Macs**, quit/reopen Peer Route, and enable sharing again on B. Existing pairings remain valid. Retry Text once, then Image. The model field is remembered after editing.

The v0.8 generation check sent a plain string input. The Codex backend adapter now converts it to typed user message items; fixtures reject string input rather than accepting any request body. The inspected 9router text route also uses a Responses Lite header/prefix for GPT-5.6 Luna and related known Lite text models. That shape is now matched for text; image-tool requests retain the ordinary image adapter shape. These are source-grounded compatibility fixes, not proof of current live model entitlement.

An authenticated `GET /peer-route-status` preflight checks peer adapter version and the selected local OAuth file without inference or revealing credentials/account IDs. It does not test whether OpenAI accepts the token. Older hosts are asked to update. Missing, expired, unreadable and changed accounts get fixed action messages. Upstream rejection statuses are preserved inside sanitized error codes, so HTTP 400, 401, 403 and 429 can be distinguished. HTTP failures export fixed codes and status numbers instead of upstream error bodies. Generation SSE is still relayed as response content; the executor does not log prompts or tokens. The initial screenshot did not retain an HTTP status, so its exact live cause cannot be inferred retrospectively.

## New in 0.8: peer-owned text and image turns

The primary screen now has two modes, **Use another Mac** and **Share this Mac**. Diagnostics and the earlier CLIProxyAPI integration are under **Advanced**.

On B:

1. Select **Share this Mac → Choose auth.json…**. Choose B’s Codex OAuth file, normally `~/.codex/auth.json` (the picker shows hidden files).
2. Enable **Allow my devices**, then **Pair another Mac…** to AirDrop a private pairing to A.

On A:

1. Open the received `.peerrelay` file and confirm matching device codes.
2. Enable **Connect**. Under **Test and integration**, choose a model and run the text or image check. Each check submits one prompt to B’s configured account; image access depends on B’s actual entitlement. The default model is a starting point from the reviewed 9router implementation, not a verified catalog entry for your account.
3. After a successful check, **Use in Codex…** installs a backed-up HTTP Responses provider at `127.0.0.1:18322`. Quit/restart Codex and start a new chat. Restore Codex before switching peers. Local function tools stay on A; the endpoint accepts text and the image-generation hosted tool. End-to-end official Codex tool/image UI behavior has not been tested live.

B rereads its selected file for every turn. It sends only the access token and matching account ID to `https://chatgpt.com/backend-api/codex/responses`. A receives SSE events, including text, available reasoning events, image previews and final image output. B never transmits its OAuth credentials to A. Peer Route does not use or rotate refresh tokens or change Codex’s auth file; Codex must maintain the login. Expired, missing or changed accounts fail closed; select the file again after an intentional account change.

The local endpoint is `POST http://127.0.0.1:18322/v1/responses`, authenticated with the pairing’s `accessKey` as `Authorization: Bearer …`. It accepts full history in `input`, forces `stream:true` and `store:false`, and rejects server conversation references. A text request has `model` and `input`; image requests additionally supply `tools:[{"type":"image_generation","output_format":"png"}]` and `tool_choice:{"type":"image_generation"}`. Response format is native Responses SSE, not 9router’s custom progress/done wrapper. `/v1/images/generations`, model discovery and WebSocket Responses are not implemented on this endpoint.

Both peer network and turn traffic use the same pinned TLS listener on B:18444. The HTTP gateway binds only to A’s loopback interface. Request bodies are capped at 16 MiB, streamed output at 64 MiB, concurrent generation requests at four; redirects, retries, account fallback and direct fallback are disabled. Pausing or stopping sharing cancels active connections. Pairing files authorize account use once the host selects an account: transfer privately, compare device codes and delete transferred copies. Existing v0.7 pairings remain usable, so revoke old pairings before granting a different set of devices account access.

**Official ChatGPT desktop still uses A’s account through the separate encrypted network relay. Translating these Responses turns into the official ChatGPT conversation/image UI remains unimplemented.** The generation check verifies the peer API only; it is not proof of ChatGPT desktop integration or exact quota accounting.

The implementation follows the wire behavior inspected in [9router’s Codex image adapter](https://github.com/decolua/9router/blob/ce4460ef79382bfddb4aa5fc0ff9f3cb0d5f95a8/open-sse/handlers/imageProviders/codex.js), with original Swift code. This is an experimental Codex backend adapter, distinct from the documented public ChatGPT-plan API. Backend compatibility and account entitlement require live verification. No credentials, prompts, reasoning or images are logged by the executor; ephemeral sessions disable cookies and caching.

Local verification covers pinned peer TLS, host-side credentials, arbitrarily fragmented text/partial-image/final-image SSE, incorrect pairing keys, missing and changed host accounts, auth-file validation, and safe config generation. The upstream is mocked; no live inference or two-Mac test has been performed.

Build/install: `cd macos && bash scripts/build.sh`. Extract `dist/Peer-Route-macOS.zip`, quit the previous Peer Route, and replace it in Applications. Apple Silicon macOS 14+; sharing requires macOS 15+. The preview is ad-hoc signed, not notarized.

## Earlier 0.7 network-relay setup (historical UI)


Install Peer Route on both Macs and open **Connect your Macs…**.

1. On B: choose **Share this Mac**, click **Allow my devices to use this Mac**, then **Share pairing via AirDrop…** and choose A in Apple's recipient picker.
2. On A: choose **Use another Mac**. Accept the AirDrop transfer and open the received `.peerrelay` file. Peer Route imports the address, certificate pin and relay key; no hostname/IP or password typing is required. If needed, click **Open a received pairing file…**.
3. Compare the displayed device codes on both Macs, then click **Codes match — trust this Mac** on A. Click **Connect**, fully quit ChatGPT yourself, then **Launch ChatGPT with routing**.

Nearby sharing Macs appear through Bonjour. Discovery broadcasts only the machine name, public certificate fingerprint and service version; it never distributes the relay key or establishes trust. The displayed code is the first 64 bits of the certificate fingerprint; the full fingerprint remains available in advanced host settings. Imported keys stay in Keychain. Routing refuses an unconfirmed certificate even from the tray. A received file can still be imported when Bonjour discovery is unavailable. AirDrop requires both Macs nearby with AirDrop enabled; Bonjour discovery generally requires the same local network. Manual file export and address settings remain under Advanced for VPN use.

Remove the received pairing file after import because it contains the relay key. The temporary sender copy is removed after AirDrop reports completion/failure, or before the next transfer if the previous process was interrupted. Code confirmation persists across restarts; forgetting a device removes its trust confirmation. Revocation currently replaces the sharing key for all devices.

This onboarding is implemented and compiled, but AirDrop, Bonjour discovery and actual ChatGPT text/image behavior still need a two-Mac test. It makes **network relay** setup easier. **It does not enable B's ChatGPT account or quota.**

## Build

```sh
swift test
bash scripts/build.sh
open "dist/Peer Route.app"
```

Requires macOS 14+ and Xcode command-line tools; hosting the desktop network relay requires macOS 15+. The build script fetches the pinned engine for the current architecture, verifies its published SHA-256 checksum, bundles its MIT license, builds Swift, signs and verifies the app, and creates `dist/Peer-Route-macOS.zip`. The delivered build is Apple Silicon. Set `PEER_ROUTE_SIGN_IDENTITY` for Developer ID signing. Default ad-hoc signing is for local use; external notarized distribution is not complete.

## Serving Mac

1. Open **Share this Mac**.
2. Choose **Sign in with ChatGPT** and finish Codex OAuth in your browser. This creates a dedicated OAuth session; it does not import ChatGPT desktop cookies.
3. Set a reachable `.local` hostname or LAN IPv4 address.
4. Enable sharing. The bundled engine listens with TLS on TCP port 18443; macOS may request incoming network access.
5. Export a private `.peerroute` pairing file and transfer it privately to your other Mac. It contains an access key: treat it as a password.

The UI refuses another sign-in if the private OAuth directory already contains an account. Use only one account there for deterministic routing.

## Requesting Mac

1. Choose **Pair a machine** and import the pairing file.
2. Compare the certificate SHA-256 fingerprint shown on both Macs.
3. Connect, select a model, and run **Test model response**. This sends a small prompt and consumes model usage.
4. Quit Codex, choose **Enable in Codex**, then restart Codex and start a new chat. Tools execute locally; model traffic uses the peer account adapter.

The local endpoint binds only to 127.0.0.1:18317. Each connection is transported over TLS with the peer certificate pinned and validated. No system-trust certificate is installed. Both apps must remain running. Same-LAN or existing private-VPN reachability is needed; NAT traversal and router configuration are not implemented. Remote proxy management is disabled.

## Pairing and revocation

This preview uses pairing files, not short codes or two-sided approval. Client keys are saved in Keychain. The host engine also needs its key in a private YAML config. Exported pairing files grant account access, so keep them private. **Revoke all pairings** stops the server and rotates the key; all old files stop working. Per-device revocation is not implemented. Removing a peer locally does not revoke its key on the host.

## Codex configuration

Enable writes a custom Responses provider in `~/.codex/config.toml`, preserving sandbox/approval settings and `auth.json`, and saves a private restore receipt. The config contains the proxy client key with owner-only permissions; OAuth tokens remain on the host. The merger refuses ambiguous TOML shapes. Restore refuses to overwrite independent edits made after installation.

Restore before switching peers, models, or keys; connect/test/enable again and restart Codex. Conversation ownership is not tracked or migrated. Pause/quit blocks the configured endpoint and drops local connections without local-account fallback. Reachability is checked every 15 seconds. Already-delivered upstream work is not guaranteed to cancel. Automatic reconnection is not implemented.

## Verification and limits

`swift test` covers configuration preservation, ambiguity rejection, private permissions, backup/restore, symlinks, concurrent edits, hostname/fingerprint validation, and real-engine transport integration. Temporary TLS identities verify a valid pin, HTTP 401 for a wrong key, and rejection of a wrong certificate. Tests use no OAuth credentials, no model inference, and no real Codex configuration.

Real two-Mac OAuth inference and desktop behavior still require a live sign-in/test. Voice, native history, websocket streaming and cancellation have not been individually verified. The account label is user-provided; response success does not independently establish the account email. Start-at-login, updater, notarization, and full ChatGPT desktop support are not implemented.

## Credits

[CLIProxyAPI 8.0.23](https://github.com/router-for-me/CLIProxyAPI/releases/tag/v8.0.23), MIT license bundled in the app. [Codex integration guide](https://github.com/router-for-me/CLIProxyAPIDocs/blob/main/docs/en/agent-client/codex.md), [official provider configuration](https://learn.chatgpt.com/docs/config-file/config-advanced).

## ChatGPT desktop experimental investigation

Open **Investigate ChatGPT desktop** in Client Support. This first-stage diagnostic adapter runs an HTTP CONNECT listener on `127.0.0.1:18318`. Start it, fully quit ChatGPT yourself, copy the displayed command, and launch ChatGPT with its Electron proxy flag. Use harmless text and image-generation prompts to observe which destination hosts establish tunnels. The launch flag may be ignored by some network paths; this is not verified live.

The adapter passes encrypted bytes directly to the destination and records only a bounded, in-memory list of destination hosts, attempt counts, and TCP outcomes. It does not decrypt TLS, install a root certificate, record HTTP headers or prompts, retrieve cookies, or use the peer account. It allows only port 443 on `chatgpt.com`, `openai.com`, `oaistatic.com`, `oaiusercontent.com`, `auth0.com`, and their subdomains. Other destinations are rejected, so this is a scoped diagnostic rather than a transparent replacement for normal networking. Diagnostics stay running when the sheet closes; stop or quit Peer Route to close its tunnels.

To restore normal networking, fully quit ChatGPT, stop diagnostics, and reopen ChatGPT normally without the flag. If this conversation is hosted in ChatGPT itself, finish or save your work before quitting. Do not launch a second copy to avoid disrupting your active profile.

See [ChatGPT adapter investigation](CHATGPT-ADAPTER.md) for evidence, limits, and the account-routing acceptance criteria. **No ChatGPT peer-account support is claimed by this diagnostic build.**


## ChatGPT API metadata trace (stage 2)

The recorded stage-1 test showed TCP tunnels to ChatGPT, its websocket host, and content hosts. It did not expose API paths or prove peer-account routing. Stage 2 uses Electron's documented `--log-net-log` switch, not TLS interception.

1. Open **Trace ChatGPT API metadata** under Client Support and choose **Prepare trace**.
2. Copy the generated command. It selects `Default` capture mode and a unique file in the private Application Support folder.
3. Fully quit ChatGPT yourself, stop the stage-1 proxy, then run the command. It deliberately omits the old proxy flag so third-party dependencies are not blocked by that filter.
4. Send one harmless text prompt and create one test image. Keep the session short.
5. Fully quit ChatGPT to finish writing JSON. In Peer Route, **Import and sanitize NetLog**. Choose the generated file under `~/Library/Application Support/Peer Route/private-traces/`.
6. Inspect the metadata list and **Export sanitized report**. Share only that exported report. Reopen ChatGPT normally without tracing flags.

Raw NetLogs may contain sensitive URLs and metadata even in Default mode. They remain in the private folder for manual removal through Finder; do not upload them. The export is built from a strict allowlist: request method, generalized first-party hostname, redacted path template, directly correlated status/type, and coarse network outcome. It omits query strings, path identifiers, request/response bodies, header values, credentials, source IDs, and timestamps. Unknown path components become `:redacted`.

The parser accepts completed Chromium NetLogs up to 128 MB, examines at most 200,000 events and retains at most 5,000 request starts. It labels SSE from `text/event-stream`, image response types from a finite MIME allowlist, and WebSocket upgrades from status 101 only when those response headers use the same source as the request. It does not infer HTTP/2-session mappings, decode WebSocket messages, read response content, or establish upstream account identity. Missing status/type means unobserved. Protocol paths outside Chromium may not appear at all.

This build is tested against synthetic NetLogs with injected secrets and the existing real transport integration. Live NetLog capture in the installed ChatGPT build still needs the manual test above.

References: [Electron command-line switches](https://www.electronjs.org/docs/latest/api/command-line-switches), [Electron NetLog](https://www.electronjs.org/docs/latest/api/net-log/), [Chromium NetLog capture guide](https://new.chromium.org/for-testers/providing-network-details/).

## WebSocket frame shapes and static protocol evidence (0.4)

In **Trace ChatGPT API metadata**, import an existing completed NetLog and turn on **Show WebSocket frame shapes**. No fresh capture is required for a log that already has `WEBSOCKET_*_FRAME_HEADER` events. The export includes direction, opcode category, coarse size band, and FIN/masking flags. It excludes payloads, credentials, query strings, source IDs, and timestamps. Unmatched sources and omitted URLs are labeled **unattributed**; do not assume they belong to prompt generation or count them as additional physical sockets. The importer limits frame metadata to 5,000 entries and socket metadata to 100 records.

`python3 scripts/inspect-chatgpt.py` emits a fixed, read-only search manifest of shipped package code. The supplied `chatgpt-protocol-evidence.json` identifies the inspected package by version and SHA-256. It accesses no chats, cookies, tokens, or user profiles, and does not modify or execute the installed app. See `CHATGPT-ADAPTER.md` for the pub/sub envelope definitions and the separate desktop HTTP/account-routing path found in that code. Static definitions are not live message-type observations.

This build does not expose a debugger, install an interception certificate, capture WebSocket payloads, or enable peer-account sharing. The remaining blocker is establishing the actual generation protocol and a compatible peer-account boundary; more host-only captures cannot resolve it.

## ChatGPT encrypted network relay (0.5)

This is a **transport-only relay**, separate from the Codex OAuth adapter above. The official ChatGPT app continues running locally and authenticating with its own account. The selected middleman carries encrypted traffic to OpenAI. It does **not** supply its ChatGPT account or remove the need to sign in on the requesting Mac.

1. Install this build on both Macs. Open **Encrypted network relay** under Client Support.
2. On the middleman, enter a reachable `.local` hostname or LAN IPv4 address and choose **Allow paired devices to relay traffic**. Hosting requires macOS 15 or later. Allow incoming connections if macOS asks; the listener uses TCP port 18444.
3. Export its private `.peerrelay` pairing file and transfer it privately to the requesting Mac. Import it there and compare certificate SHA-256 fingerprints shown on both machines. This file is separate from Codex's `.peerroute` pairing.
4. Choose the middleman and **Test peer and enable routing**. This verifies pinned TLS and relay access, without sending any AI prompt. It opens a CONNECT gateway on `127.0.0.1:18319`.
5. Save work, fully quit ChatGPT yourself, and run the displayed launch command. Peer Route never quits your active ChatGPT conversation automatically.
6. Try text and image generation. Both apps must stay running. Some ChatGPT network paths may ignore the proxy flag, so a successful peer test does not establish that every app request uses this route.

The middleman authenticates a private relay bearer key inside pinned TLS. It opens TCP tunnels only to HTTPS port 443 on the documented ChatGPT/OpenAI, asset, and Auth0 domains. Inner TLS bytes pass unchanged: HTTP streaming, images and WebSockets need no application protocol translation. No prompts, HTTP authorization headers, cookies or response content are inspected or logged. The certificate private key is stored in a private host directory; its imported identity stays in process memory without adding a certificate to system trust or Keychain. Pairing keys are kept in Keychain. The certificate expires after 365 days; renewing a certificate requires newly verified pairings. There is no automatic certificate rotation.

The implementation bounds headers, buffering, concurrent tunnels and handshake time; each pump applies backpressure. Stopping sharing or routing closes established tunnels. A failed peer check stops routing, with no direct connection fallback or automatic prompt replay inside the relay. Network paths that ignore the app's proxy setting are outside this guarantee. **Revoke all relay pairings** replaces the shared key and closes existing host connections. Per-device revocation, discovery, short-code pairing, reconnect and NAT traversal are not implemented.

To return to normal networking, fully quit ChatGPT, pause routing, and reopen ChatGPT normally without the launch flag. Existing sessions are not migrated when selecting another peer. This preview requires disabling the old route and relaunching ChatGPT after a switch.

Integration tests create two encrypted loopback endpoints and a binary echo destination. They cover valid and incorrect keys, incorrect certificate pins, identity reload, early binary bytes after CONNECT, multi-buffer forwarding, scope rejection, directional EOF without losing response bytes, and live tunnel closure when the host stops. They use no OpenAI credentials or inference. A real two-Mac ChatGPT text/image test remains required; no claim of full desktop compatibility or peer-account sharing is made.

## Local TLS compatibility experiment (0.6)

Open **TLS compatibility experiment…** under Client Support. This optional research helper decrypts only `chatgpt.com:443`, `ws.chatgpt.com:443` and `codex-cloud-backend.chatgpt.com:443` locally, streams the original bytes onward, and writes a sanitized protocol-shape report. **It still uses the current local account; peer-account sharing is not implemented.** It is intended to establish the live text/image protocol before implementing account substitution.

The native panel installs pinned, hash-verified mitmproxy dependencies into a private helper environment. Python 3.12+ must be installed; choose its absolute executable path. Preparing the helper generates a separate private CA, without installing it into trust. Reveal only the public `mitmproxy-ca-cert.cer`, open it in Keychain Access, add it to the login keychain and explicitly trust it for SSL for this short experiment. Never transfer the CA private key or leave experimental trust enabled afterward.

Start the probe, fully quit ChatGPT yourself, and use the displayed launch command. Send one harmless text prompt and generate one harmless image. Fully quit ChatGPT, stop the probe, remove the experimental CA from Keychain Access, and reopen ChatGPT normally. The app does not kill your active ChatGPT session, patch the official app, disable certificate validation or change system proxy settings. If the client rejects the CA, stop and remove trust; pinning is not bypassed. Other hostnames are tunneled without decryption. The helper listens only on `127.0.0.1:18320`.

The sanitized report is `~/Library/Application Support/Peer Route/experimental-tls/schema-report.json`. It exports fixed JSON schema vocabulary and known message categories, header-name presence, redacted paths, and HTTP status. It excludes prompts, text/image responses, credentials, arbitrary JSON key/value strings, IDs, query values and timestamps. No raw flows or TLS session keys are saved. Request bodies are streamed without inspection; compressed SSE can remain opaque. Closing the panel keeps the probe running; stop it explicitly after the test. This experiment does not supply B's account.

Run its tests with a prepared helper environment:

```sh
python3 -m venv experimental/.venv
experimental/.venv/bin/python -m pip install --require-hashes -r experimental/requirements-lock.txt
experimental/.venv/bin/python -m unittest discover -s experimental/tests -v
```

The local TLS fixture uses a disposable CA, simulated SSE and binary responses, and injected dummy secrets. It verifies delivery and sanitized export without an OpenAI request or system trust change. Live ChatGPT CA acceptance and B-account execution remain unverified. See `CHATGPT-ADAPTER.md` for the precise next gate.

### Report retention update (0.6.1)

Ordinary asset traffic is filtered from TLS schema reports. Background metadata can occupy at most 300 event slots; generation requests, image/SSE responses, stream shapes, TLS failures and known CONNECT destinations retain reserved capacity. Identical stream shapes are aggregated with `occurrences`. Reports expose `backgroundLimited` and `importantEventsDropped` alongside the overall truncation flag. This avoids startup traffic exhausting the report before a short generation test. A previously truncated report cannot recover missing events. Replace the app and reuse the existing prepared helper/certificate for a short fresh test, following the same certificate-trust cleanup instructions afterward.

### Node transport coverage and native launch (0.6.2)

Install the ZIP by extracting it and replacing **Peer Route.app** in Applications after quitting the old Peer Route app. Existing pairings and prepared helper/certificate files stay in Application Support. The app is ad-hoc signed, not notarized.

For normal encrypted transport, open **Encrypted network relay**, enable sharing on B, export/import its private `.peerrelay` pairing, compare fingerprints and enable routing on A. Leave **Include Node networking (experimental)** checked. Save your work and fully quit ChatGPT yourself, then click **Launch ChatGPT with routing**. It refuses to launch over a running ChatGPT instance. No diagnostic CA is needed for the encrypted network relay. Both Macs must be reachable on the same LAN or an existing private VPN; hosting requires macOS 15+.

The launch adds Node's `NODE_USE_ENV_PROXY=1` and matching uppercase/lowercase HTTP/HTTPS proxy variables alongside the Chromium proxy flag. Loopback destinations bypass Node proxying. These process-scoped settings can also affect Node child processes launched by ChatGPT. They do not alter system proxy settings, account identity, app-server endpoints or application files. To undo them, fully quit ChatGPT and reopen it normally. A stopped relay leaves its proxied connections unavailable; coverage of paths that ignore proxy settings is not enforced.

For protocol research, the TLS panel has a separate **Launch ChatGPT with capture** button and the same Node option. Its disclosed TLS scope now also includes exact `codex-cloud-backend.chatgpt.com:443`, the fixed cloud WebSocket origin found in the shipped package. The report recognizes a finite list of JSON-RPC method labels such as `turn/start` and `item/agentMessage/delta`; unknown method values remain redacted. This does not prove those methods are used for a specific ChatGPT turn. Follow the same explicit test-CA trust and cleanup steps.

Validation: 19 native tests and seven Python/runtime tests passed, including a real Node 24.14 HTTPS upgrade through a local CONNECT proxy, incremental response frames and failure without direct fallback when the proxy stops. The installed framework also contains Node environment-proxy support. This is supporting implementation evidence, not a live test of the embedded runtime or full ChatGPT text/image coverage. No two-Mac account test has been completed. **B's ChatGPT account/quota is still unsupported.**

The optional Node fixture requires Node 24.5+ and can be run with `PEER_ROUTE_NODE_FIXTURE=/absolute/path/to/node` when invoking the Python test suite. Node is a test dependency only; it is not added to the application bundle. Reference: https://nodejs.org/api/http.html#built-in-proxy-support
