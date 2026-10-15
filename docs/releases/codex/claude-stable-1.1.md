# codex/claude-stable-1.1

Aillies 1.1.0 brings Codex and Claude routing into one Use/Share window. Each account's credentials stay on the computer executing its requests.

## Included

- Adjacent Codex and Claude activation, separate service/device discovery including This Mac, editable device name and current local account sharing. Fast permission appears only while Codex sharing is enabled. Paired-device permissions remain separate.
- Explicit **Connect account** for Claude, owned OAuth renewal, native Claude Code and experimental Desktop Chat gateways, default model discovery and reviewed frontier-model request compatibility. Signed history is preserved. There is no account/device/model fallback.
- macOS reviewed quit/switch/reopen closes affected clients normally and reopens desktop apps only. Terminal/editor sessions stay closed. Windows/Linux check that clients have been closed and use manual reopening.
- **Disconnect & quit** removes owned settings while preserving other edits. Unsafe files retain recovery material; cleanup results and **Quit anyway** remain available.
- Private Claude token files with atomic updates, separate latest-100 incoming/upstream/failure diagnostics and four recent warnings. No credentials or conversation content enter diagnostics or peer traffic.
- Hosted Claude retry bindings stop on consent/source/account changes, including sharing off/on. Onboarding and pairing describe both services. Codex pairing, history switching, bounded retries and explicit image MCP support remain included.

## Downloads and upgrade

The publishing workflow builds fresh version-matched desktop packages for Apple Silicon macOS13+, Windows x64 and Linux x64, plus four CLI archives, source ZIP and SHA256SUMS. Exact Git/release tag: `codex/claude-stable-1.1`; archive filename component: `codex-claude-stable-1.1`. Older alpha installers are not renamed as 1.1.0.

Update both paired devices for Claude support. Finish active work, disconnect the previous Aillies integration and quit before replacing the app. Existing Codex pairings remain; Claude access needs its own sharing consent and approved-device permission. Claude connection setup currently runs on macOS; Windows/Linux can route through a paired Mac. Existing Claude Keychain selections require explicit reconnect; Aillies does not read or migrate them automatically. Normal Claude cloud chats remain with the normal account; gateway chats are local.

## Known limitations

The owner requested the stable 1.1 release name; Claude remains experimental. Stable labeling does not establish the remaining acceptance checks.

- Mac installers are ad-hoc signed and unnotarized; Windows installers are unsigned. Auto-update/autostart remain planned.
- Live Anthropic OAuth/model entitlement, native desktop gateway/tool/history behavior, physical pairing/firewall and native shutdown/save-dialog/large-history flows still require acceptance. Automated mock tests and package checks do not establish those outcomes.
- Connected Claude access/refresh tokens use user-only private app-data files, which offer less protection than Keychain. Tokens stay out of client settings, diagnostics and peer requests. Uncertain refresh requires reconnect.
- Direct LAN/private-network reachability is required; there is no internet relay. Run one Aillies node per device.
- Initial request plus at most three retries stays on the same account/device/model. Buffered generation delays display; earlier attempts may consume usage.
- Native OpenAI image_gen/editing and Claude image generation are not established by the explicit Codex image MCP path. Official ChatGPT desktop account substitution is unsupported.

See [acceptance status](../../STATUS.md), [Claude setup](../../CLAUDE.md) and [changelog](../../../CHANGELOG.md).


## Direct downloads

- [Mac · Apple Silicon](https://github.com/aa-matthew/Aillies/releases/download/codex/claude-stable-1.1/Aillies_1.1.0_aarch64.dmg)
- [Windows · x64](https://github.com/aa-matthew/Aillies/releases/download/codex/claude-stable-1.1/Aillies_1.1.0_x64-setup.exe)
- Linux · [DEB](https://github.com/aa-matthew/Aillies/releases/download/codex/claude-stable-1.1/Aillies_1.1.0_amd64.deb) · [AppImage](https://github.com/aa-matthew/Aillies/releases/download/codex/claude-stable-1.1/Aillies_1.1.0_amd64.AppImage)
- [SHA256 checksums](https://github.com/aa-matthew/Aillies/releases/download/codex/claude-stable-1.1/SHA256SUMS)

[Latest release](https://github.com/aa-matthew/Aillies/releases/latest) · [Download guide](https://github.com/aa-matthew/Aillies#download)
