# Changelog

## Unreleased

- Refresh README with current app previews, existing launch artwork/storyboard media and direct downloads for the latest and previous releases.

## 1.1.1 — UI and onboarding update

- Redesign Aillies Use and Share, editable device names, account controls and recent warnings.
- Add guided onboarding, scoped pairing permissions, reviewed switching and scheduled activation.
- Support full-code pairing across networks with Iroh and current production relays.
- Migrate compatible Codex chats safely while preserving damaged histories with precise diagnostics.
- Fix rapid sharing restarts and the Windows/Linux manual-switch test expectations.


- Default missing Claude generation effort to medium for reviewed effort-capable models, including client test requests. Preserve explicit effort, signed history and structured outputs.

## 1.1.1-alpha.3 — Safe history migration

- Switch compatible chats while preserving and logging damaged/unsupported histories.
- Detect history database capabilities without forcing schema versions; retain crash recovery and original messages.
- Add precise content-free History diagnostics, partial-switch notices and recent history warnings.
- Wait for the sharing listener to close before immediately turning sharing back on.


## 1.1.1-alpha.2 — Iroh relay repair

- Correct retired relay defaults so public full-code pairing can bootstrap through current Iroh relays.
- Show when internet connectivity is not ready; keep full-code format when generating a new full code.


## 1.1.1-alpha.1 — Internet pairing candidate

- Full pairing codes connect across networks through built-in Iroh discovery and encrypted relay transport. Both pairing approvals and certificate pinning are retained.
- Includes the completed onboarding redesign, scheduled/manual activation, scoped invitations and recovery flows.


## Unreleased — window and onboarding redesign

- Add node-owned delayed/manual provider switching, waiting-client status, cancellation and one-time completion/recovery notice. Keep normal providers active until affected clients close.
- Bind each desktop pairing invitation to selected Codex/Claude permissions, show those permissions at approval, and display a live expiry countdown.
- Add chat-count review, real activation timeline, guided recovery/Automation settings, direct activation review after pairing, and numbered Claude authorization steps.

- Adapt the supplied design with translucent dark/light cards, clearer Use/Share sections, per-service readiness and larger onboarding choices. Keep device names editable and diagnostics accessible.
- Separate invitation setup from code generation, explain sharing consent, preserve eight-digit leading zeroes and hide expired/inactive codes. Keep pairing cancellation available while waiting.
- Move activation review/progress to a focused screen, add keyboard navigation to device pickers and review removal/revocation before applying it. Preserve existing activation, account, cleanup and grant controls.


## 1.1.0 — codex/claude-stable-1.1 (prepared, unreleased)

- Add explicitly connected Claude OAuth, native Claude Code and experimental Desktop Chat routing, default model discovery and reviewed frontier-model request compatibility. Credentials stay on the executing device.
- Introduce Aillies Use/Share, adjacent Codex/Claude activation, independent device discovery, editable name, local account sharing and conditional Fast permission.
- Offer recoverable Disconnect & quit that preserves unrelated edits, reports cleanup and always offers Quit anyway. macOS reopens desktop apps only; Windows/Linux use manual close/reopen.
- Keep connected Claude credentials in private app-data files with atomic updates; existing Keychain connections require explicit reconnect.
- Fix hosted Claude retry consent invalidation, portable manual activation and stale Codex-only onboarding/pairing text.
- Prepare exact slash-tag release identity, safe archive names and license resources in desktop installers. Claude live/native acceptance remains pending; signing limitations remain disclosed.


## 1.1.0-alpha.9 — Activation and retry repairs (unreleased)

- Restore window provider switching and disconnect cleanup on Windows/Linux after users close affected clients manually. Keep desktop-only automatic restart on Mac.
- Stop hosted Claude retries after sharing consent, account or account source changes, even when the old credentials remain valid. New turns can use the newly approved scope.
- Update onboarding and pairing to show Codex and Claude separately from advertised services; remove the obsolete Codex-only setup link.

## 1.1.0-alpha.8 — Aillies Use and Share (unreleased)

- Separate using a shared account from sharing this Mac. Put Codex/Claude activation together, discover approved service-specific devices in dropdowns and retain This Mac.
- Review client closure before switching either integration; reopen desktop apps only. Never reopen terminals or editors.
- Preserve user edits during owned-settings cleanup; show independent cleanup results and always offer Quit anyway after a refusal.
- Show local account status, sharing, conditional Allow Fast and separate paired-device permissions. Keep the device name editable.
- Move full request logs/export into Diagnostics; retain four recent warnings below compact connection details.
- Save connected Claude OAuth in private app-data files with atomic updates, not Keychain. Existing Keychain connections require explicit reconnect; no automatic import or OS prompt.

## 1.1.0-alpha.7 — Claude frontier compatibility (unreleased)

- Prepare newer Claude thinking, effort and sampling options for both Desktop Chat and Claude Code, keeping discovery automatic.
- Preserve exact signed history/tools/model bytes and native thinking/tool streaming. Add feature headers only when needed.
- Validate fast/thread/cache/system directives; refuse unsupported combinations clearly without switching model/account or changing tool choice.
- Name reviewed newer models in private diagnostics; no request/response content is logged.

## 1.1.0-alpha.6 — Claude model discovery (unreleased)

- Serve authenticated model discovery at `/claude-desktop/v1/models` and `/v1/models` for Claude Code, through the selected device/account with existing Claude grants and bounded retries.
- Forward native catalog metadata and validated pagination/lifecycle queries; refuse malformed catalogs and upstream errors without a static/account/device fallback.
- Enable desktop discovery by default and automatically upgrade exact Aillies-owned profiles with a recoverable receipt. No new configuration button; native logins/history stay unchanged. Restart Claude to load discovery.

## 1.1.0-alpha.5 — Connect Claude account (unreleased)

- Add explicit browser OAuth with PKCE and a one-time authorization code for both Desktop Chat and Claude Code. Cancel/expiry/replacement refuse stale exchanges.
- Keep Aillies’ connection in its own macOS Keychain item; preserve normal Claude logins/history. No private desktop cache is needed for the new UI.
- Refresh before new turns, persist rotated tokens and stop uncertain refreshes until reconnect. Pinned retries never change account/token.
- Add Connect/Disconnect controls and independent Share/grants. Mac connection support; other devices can send through a paired Mac. Native browser/Keychain and live Anthropic acceptance remain unverified.

## 1.1.0-alpha.4 — Account checks and stable selections (unreleased)

- Preserve focused native dropdowns during status polling so an open client/provider/device picker is not replaced.
- Report fixed local sharing reasons for malformed settings, alternate backends, another gateway, API-key setup and conflicting/unsupported OAuth tokens. Do not expose their values.
- Label the selected sharing source and unchecked accounts clearly; add an explicit Desktop Chat shortcut from Code errors. Keep one persistent Share error without a duplicate toast.
- Defer both Claude account lookups until explicit Share or execution while sharing is off. No automatic login, repair, fallback or credential edits.

## 1.1.0-alpha.3 — Existing desktop account sharing (unreleased)

- Add a separate Desktop Chat provider switch using Claude’s native gateway configuration; preserve normal cloud history and restore the previous configuration.
- Reuse the sharing Mac’s existing desktop inference OAuth cache, without another account sign-in. Missing/expired/ambiguous cache refuses; no Claude Code fallback, token minting or refresh.
- Replace the inactive Sign in badge with a working Share action, persistent errors, explicit client choice and selected-device status.
- Keep native login and cloud chats unchanged. Desktop gateway conversations remain local; desktop turns have a 240-second budget.
- Experimental Mac credential reuse: real Keychain permission, desktop startup, live Anthropic access and physical-device acceptance remain pending.

## 1.1.0-alpha.2 - Claude account authentication (unreleased)

- Replace the rejected API-key-only setup with the selected device’s signed-in Claude account, following OmniRoute’s native OAuth mechanism.
- Read native macOS Keychain / Claude credential files or an explicitly configured OAuth token; send Bearer authentication with required Claude OAuth headers. Keep login credentials on their owner’s device.
- Show account readiness instead of an API-key form. Preserve native sessions and transactional settings; `settings.json` routes to Aillies using a separate local bearer.
- Stop retries on account/token changes; accept same-account native token refresh between turns. Reset alpha.1 sharing consent on upgrade without removing pairings or permissions.
- Live account entitlement and physical Keychain/device acceptance remain pending.

## 1.1.0-alpha.1 - Claude integration (unreleased)

- Add native Claude Code Messages and token counting through the selected paired device, using its own Anthropic API key. Keep tool/thinking/cache history intact.
- Add separate Claude provider/device selection, sharing and explicit per-device permissions, with a masked key field and tray shortcut. Existing approvals stay Codex-only.
- Preserve Claude sessions and working folders; restore user settings transactionally and restart/resume manually.
- Reuse bounded retries and content-free latest-100 diagnostics for both providers; disable Claude client retries and stop on access/validation refusals.
- Experimental API-key integration; Claude subscription sharing, automatic editor setup, Claude image generation and live/physical acceptance are not included.

## 1.0.0 — codex-stable-1.0 (2026-10-12)

- Publish the Codex release as stable at the owner’s explicit request, with unsigned/ad-hoc signing and remaining physical-device acceptance disclosed in the release notes.
- Build version-matched CLI archives and desktop installers in GitHub Actions; publish only after all release checks/builds succeed, with source and SHA256 checksums.

### Changes since alpha.12

- Fix Windows checkout changing embedded default-instruction bytes; keep LF line endings so upstream requests match the pinned formatter fixtures on every platform.
- Fix Rust 1.99 lint failures with a fixed typed body-read error and direct platform-name lookup.
- Ignore local browser verification output, dependency/cache folders, environment values and Windows desktop metadata; retain environment examples/templates and project sources.
- Remove unused internal wrappers, obsolete desktop-only enable/restore IPC and eight unused icon outputs; keep the transaction path and configured artwork.
- Archive Peer Route locally with verified recovery metadata; retain historical design and verification evidence.
- Refresh current feature/status/handoff/roadmap guides, retry/buffering and provider-preparation wording, Node prerequisites and explicit alpha.12 artifact selection. Existing delivered installers remain unchanged.

## 0.9.0-alpha.12 — eight-digit nearby pairing

- Windows x64 installer available with the same alpha.12 core and new branding; physical Windows acceptance remains pending.

- Pair nearby devices with eight random digits, displayed as `1234 5678` and preserving leading zeroes.
- Exchange one-time pairing details through a password-authenticated encrypted handshake, then compare and approve on both devices. OpenAI login tokens stay home.
- Codes expire after ten minutes and allow at most five password attempts. Full codes remain available for private networks and older devices.
- Declare macOS Local Network access for nearby discovery. Both devices need alpha.12 for numeric pairing.

## 0.9.0-alpha.11 — simpler controls and new logo

- Replace desktop routing pause with Normal/Aillies provider selection and the same saved-chat switch review in the tray and window.
- Group Codex under Use Aillies with; mark Claude Code as Upcoming.
- Show Allow Fast mode only while sharing is on.
- Open pairing directly at code entry, with Show a pairing code as a secondary action.
- Offer switching to Normal before quitting Aillies.
- Use the new silver A artwork for the app, window and status icons.

## 0.9.0-alpha.10 — retries and recent warnings

- Retry upstream errors, interrupted responses and transient transport/busy failures up to three times after the first attempt, keeping the same device, account and model. Earlier attempts may consume usage.
- Hold generation events until success, with keepalive comments while waiting; discard partial failed answers. Forced image calls also retry invalid PNG results.
- Stop when cancelled, paused, redirected, denied access or when the account changes. Codex and the sharing node do not add another retry budget.
- Show the latest four request/retry warnings on Home and retain each attempt in diagnostics.

## 0.9.0-alpha.9 — background history preparation

- Large chat histories can finish preparing in the background instead of failing after two seconds.
- Show history preparation progress beside your selected provider; retain errors until the next attempt.
- Keep the existing closed-Codex checks, backups, rollback and ChatGPT-only reopen.

## 0.9.0-alpha.8 — provider switch correction

- Keep your chosen provider selected during review and switching; Cancel or failure restores the current provider.
- Quit, switch & reopen now opens only ChatGPT. Editors and terminal Codex sessions stay closed.
- Switch indexed chat copies and older provider-setting records safely, preserving messages and permissions.

## 0.9.0-alpha.7 — Quit, switch & reopen

- macOS provider switching now offers an app/terminal review and a Quit, switch & reopen action when Codex is running. History is preserved using the existing transaction.
- Terminal sessions reopen in their saved-chat picker; interrupted requests are never submitted automatically.
- Unknown clients, failed shutdown and unfinished migration recovery stop the flow safely. Windows/Linux retain manual quit/reopen.

## 0.9.0-alpha.6 — image display guidance

- Image creation now returns a ready-to-display Markdown image reference with an absolute local path and instructs Codex to show it in the final reply. Saved images can be shown again without regeneration.
- Image data, account routing, selected model and approval behavior are unchanged. This addresses missing display guidance; it does not establish the cause of a separate upstream image HTTP 400.

## 0.9.0-alpha.5 — history switch correction

- Switch valid unindexed active/archived chats alongside indexed chats, including older rollout-only homes. No fabricated database records or history deletion.
- Preserve original history records and append the selected provider setting so Codex index rebuilding and resume agree. Permissions and other chat settings remain intact.
- Recover interrupted file-only switches safely; conflicting IDs, malformed/unsupported history and external paths still stop before switching.

## 0.9.0-alpha.4 — router-core-v4 candidate

- Keep existing chats when choosing Normal ↔ Aillies after quitting Codex, with rollback and recovery for indexed active/archived history and paginated offsets. Large/unsupported histories stop safely before switching.
- Export the latest 100 incoming requests and 100 upstream attempts, including successes, statuses, timings and safe validation categories. Existing error history is retained.
- Align Codex session selection and bounded identity caching with pinned 9router. Fix rejection of optional client fields, null references and repeated tool definitions.
- Image creation can use the current model explicitly and verifies complete PNG files. Updated mock coverage includes image routing and approval-review request shapes; fresh live acceptance remains pending.
- Simplify provider settings and expose the build marker beside the app version.

## Earlier unreleased development

- Router core v3: use the reviewed 9router client identity in new installers; align instruction defaults, supported model aliases, reasoning effort normalization, function tool/schema formatting and conversation session/cache keys. Logs retain the existing content-free contract.
- Preserve Lite developer IDs and namespace tool validation. Known unsupported API options are removed before sending; server references, unsupported tools and unknown fields still fail closed. Requests are never retried or moved to another account.

- Compatibility: align ordinary/Lite request selection and final Codex upstream tier fields with the reviewed 9router implementation. Hosted web search uses ordinary Responses; GPT-5.6 no longer uses Lite. Preserve reasoning history and request encrypted reasoning output when enabled.
- Validation: apply the tool allowlist to existing Lite prefixes, and lift them into ordinary requests without dropping history. Diagnostic exports identify this build as `request-compat-v2`.

- Diagnostics: keep the latest 100 request errors and warnings across restarts. Export them to Downloads from Connection details or `aillies diagnostics`; logs contain only fixed codes and allowlisted metadata.
- Errors: an upstream HTTP 503 now says Service Unavailable. Requests are never retried automatically.

- Fast mode: account owners can allow Fast requests; requesters can choose it independently for each destination. Permission is enforced on the sharing device, with no downgrade or retry.
- Images: Codex integration now registers an explicit `generate_image` tool using the selected Aillies device. Completed PNGs are saved on the requesting device. Existing setups need integration off/on, then a Codex restart and new chat.
- Fix: restoring Codex settings recognizes valid quoting, tabs and comments, and refuses invalid TOML safely.
- Fix: forgetting the selected peer no longer silently switches requests to the local account.
- Privacy: desktop logs now accept fixed lifecycle events; raw errors and panic payloads are excluded.
- Tests: added coverage for permissions, credential isolation, TLS pins, pairing expiry/cancellation, connection resets, logging, concurrency, request limits and image tool routing. Smoke and UI checks now correctly fail when their assertions fail.

- Security: the test-only upstream override (`AILLIES_CODEX_UPSTREAM`) is now parsed and accepts only plain-HTTP loopback URLs with an explicit port and no user info. `http://127.0.0.1:80@example.com/…` was previously accepted.
- Fix: a reply that ends early is now reported as “may not have arrived” even when the generated text mentions `response.completed`. Completion is detected from real SSE events.
- Dev: `make test` and `make lint` now cover the desktop app (tests, fmt, clippy, UI syntax); CI does the same on each OS.

## 0.9.0-alpha.3 — 2026-10-10

- Turning off "Use Aillies with Codex" now works after Codex has changed its own settings (trusted folders, dismissed warnings, a different model). Aillies removes only its own lines and keeps everything else. It still refuses, without changing anything, if the file can't be read safely.
- The "routing paused" error in Codex now says how to turn routing back on.

## 0.9.0-alpha.2 — 2026-10-10

- Desktop: Aillies starts its background service and tray icon before opening the window, so routing works even if the window can't open (for example when Microsoft Edge WebView2 is missing). It then shows a tray tooltip and opens the WebView2 download page.
- Desktop: opening Aillies again (Start menu, desktop icon) brings up the running app's window instead of starting a second copy.
- Desktop: writes a startup log, `desktop.log`, in the Aillies folder (`%APPDATA%\anyax\Aillies\config` on Windows). It holds lifecycle events and errors only, never content.
- Windows installer installs for the current user without an admin prompt.

## 0.9.0-alpha.1 — 2026-10-10

First Aillies build (formerly Peer Route).

- New cross-platform Rust core, Codex provider and device node; the Swift app moved to `legacy/`.
- `aillies` CLI: `run`, `invite`, `pair`, `approve`, `use`, `pause`, `check`, `revoke`, `codex on|off`, and more.
- Pairing with a one-time code and a three-part verification code; approval on both devices.
- Per-device identities over pinned mutual TLS 1.3; per-device revocation, effective immediately.
- Codex integration writes a backed-up provider with retries disabled; restore refuses to overwrite later edits.
- Accepts the tools current Codex sends (`tool_search`, `web_search`), so real Codex sessions work.
- Tauri tray app with the compact window: destination switcher, routing and sharing switches, pairing, approval banner, devices, Codex setting.
- New name and logo.

### GPT 6.1 Sol request compatibility

- Include `gpt-6.1-sol` in the Responses Lite model list. Text requests now use the matching input prefix and upstream header without changing the selected model or account. Both devices need this build. Image generation compatibility is not established by this fix.
