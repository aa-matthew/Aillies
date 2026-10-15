# Features

Every user-facing capability, where it is implemented, and its state.
✅ shipped and tested · 🟡 partial or unverified on real hardware · ⬜ planned

Prepared release: 1.1.0 / codex/claude-stable-1.1 (router-core-v11-review-fixes); published Codex stable remains 1.0.0. Current package selection: [RELEASES.md](RELEASES.md).

All six PR CI jobs passed for version 1.1.0 on macOS, Windows and Ubuntu (workspace tests/lint and desktop tests/lint/build). Test status distinguishes automated/synthetic coverage from physical acceptance; earlier platform observations are in [the verification history](history/STATUS-2026-10-11.md).

## Using another device’s account (requesting side, “A”)

| Feature | What the user gets | Where | State |
|---|---|---|---|
| Codex routing | Codex requests through Aillies run on the selected device’s Codex login; complete replies, reasoning events and images return after successful completion (keepalive comments while waiting) | `aillies-node/src/loopback.rs` (`codex_turn`, `forward`) | ✅ Current smoke: codex-cli 0.161.0 with mock upstream; earlier live evidence in STATUS |
| Use Fast mode | Optional switch for the selected account; B must allow it first. Off sends the standard tier, on requests priority. Preferences are separate per destination | `Node::set_fast_mode`, `request::select_service_tier`; CLI `fast on/off`; home switch | ✅ Cross-device regression test; live entitlement and accounting unverified |
| Image creation tool | Codex discovers `aillies_images.generate_image` when the integration is enabled; the selected device generates the PNG and A saves it under `~/.codex/generated_images/aillies/`. Optional current-model argument; otherwise saved root model. Full PNG decoding, absolute saved path and final-reply Markdown display guidance, new creation only; editing is not exposed by this adapter | `aillies-node/src/image_mcp.rs`, `config.rs`, `/mcp/images` | 🟡 Earlier live build saved a PNG via Mac A → Windows B; v4 verified with complete mock PNG, fresh live approval/image test pending |
| Send through | Pick *This device* or any paired device; applies to the next request through Aillies | `Node::set_destination`; tray menu; home picker | ✅ |
| This device | Use your own Codex login through the same endpoint | `retry::route`, `Node::executor` | ✅ |
| Provider instead of pause | Desktop chooses Normal or Aillies; selecting Aillies explicitly enables a previously paused setup. CLI pause/resume remains available | `Node::activate_provider_routing`, provider switch; window/tray | ✅ Synthetic regression; physical acceptance pending |
| Readiness check | Connection (reachable, latency, address) and account readiness checked separately | `Node::peer_health`, peer `GET /v1/status`; CLI `check` | ✅ |
| Clear failures | Fixed messages for unreachable, sign-in expired, account changed, not approved, busy, sharing off, version mismatch, delivery uncertain | `aillies-core/src/failure.rs` | ✅ |
| Bounded retries | First attempt plus up to three retries for upstream, stream and transient transport/busy failures, on the same device/account/model; stop on cancellation, route/account change or access/local validation refusal. Generation events are buffered until success, with keepalive comments while waiting. Earlier attempts may consume usage; Codex’s own retries remain disabled | `retry.rs`, `loopback.rs`, `peer_server.rs`, `wire::TerminalScanner` | 🟡 Unit + two-node recovery/exhaustion/cancellation/account/image mock checks passed; live acceptance pending |
| Recent warnings | Latest four request/retry notices on Home, newest first; retained through successful turns, dismissal and app restarts. Fixed messages only; full latest-100 diagnostics remain exportable | `diagnostics.rs`, `NodeStatus::recent_warnings`, `app.js` | ✅ Persistence, ordering, latest-four bound, escaping and polling tests |
| Codex integration switch | Writes a backed-up provider and local image MCP server into `~/.codex/config.toml`; turning off removes only Aillies’ lines and keeps later Codex edits | `aillies-codex/src/config.rs` | ✅ (restore merge since alpha.3) |
| Existing-chat provider choice | Quit Codex, choose Normal or Aillies, reopen the same chat. Indexed and valid unindexed active/archived chats preserve original transcript records; updates include provider index, paginated offsets and a provider settings event when needed. Rollout-only legacy homes supported; rollback/recovery and background preparation with progress | `aillies-codex/src/provider_switch.rs`, `Node::switch_codex_provider`, settings selector; CLI `codex on/off --existing-chats` | 🟡 Synthetic installed Codex 0.161.0 list/resume/read verified both directions; physical desktop reopen still to check |
| Traffic diagnostics | Latest 100 incoming turns and 100 upstream attempts, independent of 100 failures. Fixed phase/category/status/timing and shared generated trace, export to Downloads | `diagnostics.rs`, provider `telemetry.rs`, Connection details → Export diagnostics | ✅ Unit + two-node mock verification; export both devices for both sides |

## Sharing your account (sharing side, “B”)

| Feature | What the user gets | Where | State |
|---|---|---|---|
| Allow Fast mode | Owner opt-in, off by default; every incoming turn is checked before upstream. Older peers that omit this capability cannot enable Fast on A | `Node::set_allow_fast_mode`, `peer_server.rs`; CLI `allow-fast on/off`; home switch | ✅ Mock two-device test, including permission revocation |
| Share this device | Approved devices can run requests on this login | `Node::set_sharing`, `peer_server.rs` | ✅ |
| Account pinning | Sharing approves the current Codex account; a different login fails closed until sharing is turned on again | `Executor::credentials` (`AccountChanged`) | ✅ |
| Login read at request time | Expired, missing or API-key logins fail before anything is sent | `aillies-codex/src/auth.rs` | ✅ |
| Concurrency limit | At most 4 requests at once; extra requests get “busy”, nothing sent | `Node::turns` semaphore | ✅ |
| Per-device revoke | Revoking a device takes effect immediately and drops live connections | `Node::revoke_client`, epoch watch | ✅ |
| Approval banner | The window opens and shows the verification code when a device asks to pair | `apps/desktop/ui/app.js` (`home`, `invite`); tray refresh | ✅ |

## Pairing

| Feature | What the user gets | Where | State |
|---|---|---|---|
| Eight-digit pairing code | Random eight digits, leading zeroes preserved; nearby discovery and authenticated encrypted exchange; 10 minutes and at most five password attempts | `protocol::pairing_digits`, `Node::create_pairing_code`, `pairing.rs`, `discovery.rs` | ✅ Local automated discovery verified; physical two-Mac permission flow pending |
| Full pairing code | `aillies1:…` alternative for private networks and older devices; one use, 10 minutes | `Invite`, `Node::create_full_invite`; CLI `invite --full` | ✅ |
| Verification code | Same three-part code on both screens (e.g. `MINT · 45 · SAGE`), bound to both certificates and the secret | `protocol::verification_code` | ✅ |
| Two-sided approval | Requester confirms codes; sharer approves; either side can cancel or decline | `Node::start_pairing`, `confirm_pairing`, `approve`, `cancel_pairing` | ✅ |
| Same network or Tailscale | Code lists LAN, Tailscale and `.local` addresses; first one presenting the right identity wins | `Node::advertised_addrs`, `peer_client::connect` | ✅ |
| Pairing across the internet | Rendezvous and relay when there is no direct path | — | ⬜ M6 |
| Nearby discovery (mDNS) | Finds devices with an active code; no code, secret or account token advertised | `discovery.rs`; `_aillies-pair._tcp.local.` | ✅ Code and both approvals remain required |

## Apps

| Feature | What the user gets | Where | State |
|---|---|---|---|
| CLI | `aillies run/status/share/fast/allow-fast/invite/pair/pending/approve/reject/use/pause/resume/devices/check/revoke/forget/codex/name/dismiss` | `crates/aillies-cli` | 🟡 Current Mac tests; Windows alpha.12 cross-built, older CLI exercised under Wine; current Windows runtime pending |
| Menu bar / tray | New A mark in green/gray/orange; provider/device menus, sharing, conditional Allow Fast, pairing, warnings, open and guarded quit | `apps/desktop/src-tauri/src/lib.rs` | 🟡 Current Mac unit/mock preview; current native tray acceptance on each platform pending |
| Compact window | Welcome, home, pair (code entry first, secondary show code), verify, readiness, connection details, devices, Codex switch, error and approval banners, dark mode | `apps/desktop/ui` | 🟡 Same as above |
| Survives window problems | Service and tray start before the window; a broken WebView doesn’t stop routing | `show_window` (alpha.2) | 🟡 Earlier Wine service observation; current WebView2 failure acceptance pending |
| Single instance | Launching again opens the running app’s window | `tauri-plugin-single-instance` | 🟡 Windows/macOS behavior unverified |
| Request diagnostics | Latest 100 request errors/warnings, persisted across restarts; Connection details → Export diagnostics or `aillies diagnostics` saves a private JSON file to Downloads. Fixed codes and allowlisted metadata only | `aillies-node/src/diagnostics.rs`, Node export, control API, desktop IPC | ✅ Unit/two-node tests and earlier physical Windows export; current build acceptance pending |
| Startup log | `desktop.log` accepts only typed lifecycle events and public version/port values; raw errors and panic payloads excluded | `log_line` | ✅ |
| Installers | Alpha.12 Apple Silicon DMG and Windows x64 NSIS; Linux packaging via Tauri/CI | `scripts/`, `release.yml`; [artifact list](RELEASES.md) | 🟡 Mac/Windows payloads verified; unsigned, Windows physical install pending |
| Autostart, auto-update | — | — | ⬜ M8 |

## Platform support

| | macOS 13+ | Windows 10/11 x64 | Ubuntu 22.04+ |
|---|---|---|---|
| CLI | ✅ automated tests/build | 🟡 cross-build history; alpha.12 runtime pending | 🟡 earlier tests; current hardware pending |
| Desktop app | 🟡 alpha.12 package/unit/mock checks; native acceptance pending | 🟡 alpha.12 installer verified; physical acceptance pending | 🟡 earlier build; current acceptance pending |
| Secrets in OS store | ⬜ M7 | ⬜ M7 | ⬜ M7 |

## Providers

| Provider | State |
|---|---|
| Codex (ChatGPT login) | ✅ |
| Claude | Experimental M9 account adapter included in the prepared 1.1.0 release; see CLAUDE.md |

## Current upstream compatibility

Both devices use the v4 request formatter/session resolver and pinned `codex_cli_rs/0.159.0` identity. Model/effort and Lite/ordinary behavior are covered by the pinned 9router fixtures; current live model/backend acceptance is not implied. See [UPSTREAM-COMPATIBILITY.md](UPSTREAM-COMPATIBILITY.md). V2/v3 formatter and marker descriptions are [historical](history/FEATURES-2026-10-11.md).

Embedded default instructions are checked out with LF endings on every platform, preserving fixture-identical request bytes even when Windows enables automatic CRLF conversion.

Native OpenAI `image_gen`, image editing and native image endpoint forwarding are separate from the explicit `aillies_images.generate_image` MCP adapter and remain unverified/unsupported by that adapter. Normal Codex approvals remain required.

## macOS quit, switch and reopen (since alpha.8)

The provider selector offers a compact review when Codex is still running, followed by **Quit, switch & reopen** or Cancel. It names the apps that will close (including an editor when it owns Codex) and terminal count. Finish work/save editor changes first. Aillies quits normally, verifies shutdown, runs the history-preserving transaction and opens only ChatGPT. Editors and terminal Codex sessions stay closed. A previously quit ChatGPT is found in standard Applications locations. The chosen provider stays selected while awaiting confirmation or switching; Cancel/failure restores the actual provider. No unfinished action is resumed automatically. Unknown clients refuse; recovery failures keep Codex closed. Windows/Linux still use manual quit/reopen. Implementation: `aillies-codex/src/restart.rs` and `restart/macos.rs`, Node plan/execute methods, thin desktop IPC, `app.js`.

Existing-chat switching includes indexed same-provider rollout copies without deleting/merging history or applying canonical paginated offsets twice. Older provider-setting events without thread IDs are supported within their validated rollout. Conflicting copies or ambiguous unindexed histories still stop safely.

Alpha.9 removes the two-second history preparation cutoff. The provider row shows scanning, preparation counts and saving status while the existing background worker runs. The provider choice stays pending until commit; errors remain visible. Clients must stay closed, and transactional rollback/recovery, record bounds and path/schema checks remain.

## Current provider and pairing controls

Home separates Aillies Use and Aillies Share. Codex/Claude activation switches sit together; approved service-specific dropdowns refresh available devices and include This Mac. Unique incoming/outgoing sharing counts are informational. Current local account readiness and Share live below; Allow Fast appears only with Codex sharing. Pairing and per-device permissions remain separate. Connection details holds editable name/version/Diagnostics; latest four warnings follow it. Full latest100 incoming/upstream/error logs and export are inside Diagnostics. macOS activation reviews closure and reopens desktop apps only, never terminals/editors. Disconnect & quit reports independent cleanup and offers Quit anyway after refusal. The supplied brand artwork is retained; physical native lifecycle acceptance remains pending.


## Claude — experimental native account adapters

`crates/aillies-claude` owns the native Claude login reader, fixed Anthropic executor and transactional user settings. `aillies-core/src/claude.rs` validates native requests without rewriting them; the shared SSE scanner supports framed Claude terminal events. `aillies-node/src/claude_node.rs` and `claude_route.rs` provide independent destinations/sharing/grants, authenticated local Messages/token counting and peer endpoints. The existing retry controller, account-scope cache and diagnostics are shared with Codex. Native signed thinking, tools and cache markers pass byte-for-byte; credentials stay on the selected device. HTTP and SSE validation/access refusals stop retries.

Desktop Claude settings and its tray shortcut expose only provider, selected device, signed-in account readiness, sharing and per-device permissions. CLI uses `aillies claude` subcommands. User settings receipts preserve later edits; Claude transcripts, session IDs, project trust and working folders are never rewritten. Restart/resume manually. Older Codex approvals do not grant Claude; Claude-only pairing does not grant Codex. Codex permissions can also be managed in the provider screen.

Mock coverage includes streaming and finite Messages, token counts, exact native bytes/header passthrough, separate local tokens, explicit grants, account/token/destination pinning, bounded retries, partial suppression, diagnostics and settings recovery. Installed-client mock probes supplement these checks. Live Anthropic access/billing, physical devices, actual native desktop gateway/Keychain acceptance, VS Code environment setup, model discovery and image generation remain outside this candidate. See [CLAUDE.md](CLAUDE.md).


### Desktop Chat / existing account sharing — alpha.3

Desktop Chat has independent transactional provider settings (`aillies-claude/src/desktop_config.rs`), backed by Claude’s native `Claude-3p/configLibrary`. The UI distinguishes Desktop Chat from Claude Code, routes new requests through the selected device and keeps failures visible. Normal cloud chats/account labels are not rewritten; gateway chats are local. Mac switching requires Claude closed; managed/bootstrap configurations refuse. No automatic reopen.

`desktop_auth.rs` reuses the sharing Mac’s existing encrypted inference OAuth cache on explicit Share. Exact Keychain item, active account/org/scope/expiry checks, no cookie/token mint/refresh, no CLI credential fallback. Existing desktop cache is required; direct reuse on Windows/Linux is not implemented. Native cache reads are fixture-tested, not physically verified. Node sharing pins an explicit account source; desktop local routes use only desktop credentials, peers use the sharing owner’s selected source. Retry/account/token/grant boundaries are covered by two-node tests; desktop turns are capped at 240 seconds. Actual desktop runtime/Keychain/live entitlement acceptance remains open. See [CLAUDE.md](CLAUDE.md).


### Local account check feedback — alpha.4

`aillies-claude/src/auth.rs` provides closed, value-free local sharing reasons while preserving request FailureCodes. The node checks only the explicitly selected source. `app.js` preserves focused native dropdowns during polling, names the sharing source, labels unchecked accounts Not shared, and offers Desktop Chat from Code errors without enabling sharing automatically. Share errors appear once in a persistent card. Status polling while sharing is off never reads either Claude login. Covered by auth, two-node fixture and UI regression checks; actual user credential failures cannot be inferred from screenshots of the older generic message.


### Connect Claude account — alpha.5

The primary UI now connects one account independently of Desktop Chat/Code. Browser PKCE authorization, a masked one-time code field, fixed failure messages and explicit Cancel/Disconnect replace private-cache dependency. Native macOS Keychain stores only Aillies-owned credentials; normal Claude credentials/history are unchanged. `aillies-claude/src/oauth.rs` owns random state/verifier, one-use/expiry, token exchange/bootstrap identity validation, bounded responses, serialized pre-turn refresh and rotated-token persistence. Refused/uncertain refresh requires reconnect. `Reader.connected`, Node connect/disconnect methods and the existing executor pin each logical turn without refresh during retries or native-source fallback. Other platforms can request through a paired Mac but cannot connect locally yet. Legacy native sources remain explicit CLI/control compatibility paths; alpha.3/4 UI descriptions above are historical. New UI/state/crypto and two-node OAuth mock tests are separate from native/live acceptance.

### Default Claude model discovery — alpha.6

`claude_route.rs`, `aillies-core/src/claude.rs`, the Claude executor and pinned peer transport handle bodyless `GET /claude-desktop/v1/models` and `/v1/models`. Discovery follows the independent Claude destination/account/grants, serialized pre-turn refresh and pinned retry budget. Native model/capability/pagination JSON passes only after bounded shape validation; failures are fixed and no static fallback exists. `desktop_config.rs` enables discovery by default, removes the static override and journals automatic upgrades of exact owned profiles; edited profiles refuse. There is no new button or toggle. Synthetic local/two-node and interrupted-profile tests cover behavior; live OAuth catalog access and the native picker still need acceptance.

### Claude frontier compatibility — alpha.7

Automatic discovery is retained. `crates/aillies-core/src/claude_compat.rs` prepares reviewed native model options for both clients via `aillies-claude/src/executor.rs`: adaptive thinking, current effort, fast/thread/cache fields and mid-conversation directives with selective betas. Model/system/tools/signed history remain raw JSON, no-op body unchanged. Unsupported semantic combinations refuse before sending; no account/device/model fallback. New closed diagnostic model IDs and speed metadata improve latest100 logs without content. Two-node tests bridge discovery to all reviewed model IDs and verify native thinking/tool streams; isolated mocks do not establish live account entitlement. See [compatibility policy](CLAUDE.md#frontier-model-compatibility--alpha7).


### Unified activation and private account storage — alpha.8

`aillies-node/src/activation.rs` owns expiring single-use service plans, independent cleanup reports and an unconditional exit gate. `aillies-codex/src/restart{,/macos}.rs` validates all inventories before closure; shell IPC delegates to Node. Codex retains journaled history migration. Claude installs/restores both Code and Desktop; edited owned desktop profiles can be detached without discarding unrelated edits. Malformed/symlink/unsupported history recovery remains protected and visible. `oauth.rs` saves connected tokens in private accounts/claude.json atomically with owner-only permissions; no automatic Keychain read/import. Old connections require explicit reconnect. Native legacy readers are explicit CLI/control compatibility only. Device name updates use approved pinned status and do not alter identity/grants.

Alpha.9 restores manual window activation/disconnect on Windows/Linux with affected-client closure checks and explicit manual reopen copy. Hosted Claude retries invalidate on consent/source/account changes, including same-account off/on. Pairing completion shows both service statuses by provider ID and links to Aillies Use.


### Onboarding and window redesign — 2026-10-14

The supplied 18-screen onboarding design is adapted to the existing plain HTML/CSS/JS window. Home uses separate Use and Share cards, per-service activation/device readiness, a device-name shortcut to the editable details screen, and a conditional requester Fast switch. Informational sharing counts, separate grants, Connection details and the latest four warnings remain. Dark/light palettes use translucent neutral panels; layouts scroll at the existing 600×820 and 420×520 window sizes and respect reduced motion.

First setup offers Use or Share without enabling routing. Invitation setup discloses sharing before generating a code, preserves leading zeroes, hides locally expired codes and clears the display when all sharing stops. Approval shows current Codex consent and the separate Claude grant requirement. Waiting for approval remains cancellable; late completions cannot navigate the UI after cancellation. Device picker arrows/Escape work; inactive devices show specific readiness reasons. Remove/Revoke require a consequence review.

Activation has its own review/progress view, using the existing single-use Node plans. Deferred/manual switching and invitation-specific permission selection are implemented by the node as described below. The layout pass preserved the peer protocol, credential storage and retry logic; the completed flows below extend activation and invitation consent. Claude Connect retains masked code input and private account storage; incomplete cleanup retains Quit anyway.


### Completed designed flows — 2026-10-14

- Activation offers Quit/switch/reopen, When I next quit, and I’ll close them myself on Mac; Windows/Linux use passive manual closure. An in-memory node-owned worker continues with the window hidden, shows static app labels/counts, requires two closed samples and rechecks the closure barrier before applying. Cancellation/replacement/shutdown prevents a pending apply. Device, connected Claude account and installed-provider changes invalidate consent. A completed/failed delayed operation reveals Aillies once and the UI shows its result. Pending schedules are not restored after node restart.
- Immediate closure retains reviewed identity checks. Passive review never prepares termination targets. Later mode reopens only the reviewed service desktop app; manual mode leaves reopening to the user. Terminals/editors remain closed. Codex retains its existing ChatGPT-only reopen scope.
- Review has a bounded read-only chat estimate; the actual count is revalidated after closure. A content-free timeline reports closure, saved recovery journal, history, settings and reopening. Failures expose Try again, passive alternatives and the explicit Mac Automation settings link. No force-kill or transaction cancellation is added.
- Desktop invitations bind Codex/Claude booleans to their one-time secret, numeric bootstrap and pending approval. Generating the code explicitly enables selected-account sharing; only the invitation mask is saved on approval. Other already-approved clients retain their own grants. The UI shows the bound mask during approval and a live countdown. Legacy CLI/control invitations preserve their existing defaults.
- Readiness offers a service-specific activation-review shortcut, with explicit selection and no automatic apply. Claude Connect uses the designed numbered browser/code steps; the complete one-time code remains masked and ephemeral.


### Iroh full-code pairing — 1.1.1-alpha.1

Full pairing codes now include the owner's persistent Iroh node ID. Pair → full code uses built-in internet connectivity; no VPN or Iroh account is needed. Eight digits remain a nearby-network feature. Existing LAN/private-network codes and approved peers continue to work. The same pinned mTLS, two approvals, selected account and service permissions apply to both transports. Implementation: `internet.rs`, `peer_client.rs`, shared `peer_server::serve_stream`, Node invite/address handling; UI uses the existing full-code disclosure. Public discovery/relay sees network metadata, not decrypted application content.


Alpha.2 relay follow-up: explicitly use the current official n0 AP/NA/EU hosts rather than the old Iroh0.92 defaults. A local `internet_ready` flag drives a full-code-only warning when no home relay is available. New code retains numeric/full format. No relay configuration control is added. Public full-code pairing and mock inference passed after the repair; physical cross-network testing remains.


### Safe per-chat history switching — 1.1.1-alpha.3

Compatible chat groups switch while damaged or unsupported groups stay unchanged. All replicas of a chat stage together; failures preserve original files and index rows. Required SQLite columns are detected across numbered generations, with exact database names journaled for crash recovery. No native schema is forced or repaired. Valid legacy null fields and complete final records without newlines are retained. Repeat activation skips already-target provider metadata; unresolved histories are retried later.

A completion notice reports older histories left unchanged and directs users to new chats for Aillies. Diagnostics adds a History tab/export section with the latest100 fixed-code/stage/reference/record/timestamp entries; the four recent warnings include history issues. No chat content or paths appear. Core logic: `provider_switch.rs`; node status/diagnostic integration and activation reports; CLI and desktop remain thin.


### Claude omitted-effort default (unreleased source)

`aillies-core/src/claude_compat.rs` supplies medium effort for missing/null generation effort on reviewed supported models, shared by Code/Desktop and local/peer execution. Explicit request/history directives, signed payload bytes and output formats are preserved; no reasoning enablement or effort injection for token counting/discovery/unknown or unsupported models. Unit and two-node mock coverage exercise defaults, explicit overrides, both clients, both streaming modes and idempotence. Decision67 records scope.
