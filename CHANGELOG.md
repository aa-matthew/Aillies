# Changelog

## Unreleased — repository cleanup

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
