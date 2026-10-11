# codex-stable-1.0

Aillies 1.0.0 lets Codex use a selected paired computer’s Codex login over a pinned, encrypted connection. Login credentials stay on the computer that owns them.

## Included

- Eight-digit nearby pairing with matching verification codes and approval on both devices; full pairing codes for reachable private networks.
- Normal/Aillies provider selection, selected-device routing, independent sharing and owner-controlled Fast permission.
- Existing-chat provider switching with history preservation, rollback/recovery and background preparation progress. macOS offers reviewed quit/switch/reopen; Windows/Linux use manual close/reopen.
- Up to three requester-owned retries after the first attempt, always on the same device, account and model. Only a successful completed response is shown; earlier attempts may consume usage. No account/device fallback.
- Explicit MCP image creation that validates and saves a PNG, with a display reference and normal Codex approvals.
- Content-free local diagnostics, transactional configuration restore, compact tray/window and refreshed branding.
- Windows embedded-instruction line-ending and Rust 1.99 compatibility fixes.

## Downloads

Desktop: Apple Silicon macOS 13+ DMG, Windows x64 per-user installer, Linux x64 DEB/AppImage. CLI: macOS arm64/x64, Windows x64 and Linux x64 archives. `SHA256SUMS` covers every attached package and the source ZIP. The release tag identifies the exact source used to build these files.

Update both paired devices to this version. Finish active Codex work and switch the old Aillies integration to Normal before quitting/replacing the app. Start the new app and choose Aillies again when ready. Existing chat switching requires Codex clients to be closed; save editor changes before using the macOS restart action.

## Known limitations

This release is marked stable at the owner’s explicit request. The label does not mean the remaining acceptance checks have passed.

- macOS packages are ad-hoc signed, without Developer ID/notarization; Windows installers are unsigned. OS security prompts may appear. Signing/notarization, auto-update and autostart remain planned.
- Current physical-device checks remain incomplete: Windows install/WebView2/tray/firewall, two-device numeric pairing and network permissions, macOS restart/save-dialog/large-history flows, sleep/wake and native lifecycle behavior.
- Fresh live model/image approval/display, sharing-account usage isolation, Fast entitlement and real interruption/cancellation/account-change acceptance remain pending. Automated mock tests and build checks do not establish these outcomes.
- Both devices need direct LAN/private-network reachability. There is no internet rendezvous or relay.
- The explicit image MCP adapter does not establish native image_gen support or image editing. Claude support is planned.
- Credentials use private local files; OS credential stores are planned. Aillies does not refresh OAuth; the owning device’s Codex must renew its login.
- Response buffering delays generation display. Possibly delivered retries can consume additional usage. Codex’s private backend can change.
- Run either the CLI daemon or desktop node on a device. Official ChatGPT desktop account substitution is unsupported.

See [current acceptance status](https://github.com/aa-matthew/Aillies/blob/codex-stable-1.0/docs/STATUS.md) and [changelog](https://github.com/aa-matthew/Aillies/blob/codex-stable-1.0/CHANGELOG.md).
