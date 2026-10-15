# Current release selection

## Latest — Aillies 1.1.1

Published **2026-10-15**: [v1.1.1](https://github.com/aa-matthew/Aillies/releases/tag/v1.1.1), app **1.1.1**, marker `router-core-v15-onboarding`. [Release notes](releases/v1.1.1.md) cover the UI/onboarding update, scoped pairing, Iroh full codes, safe per-chat migration and Claude effort defaults.

| Platform | Direct download |
|---|---|
| Apple Silicon · macOS13+ | [Aillies_1.1.1_aarch64.dmg](https://github.com/aa-matthew/Aillies/releases/download/v1.1.1/Aillies_1.1.1_aarch64.dmg) |
| Windows x64 | [Aillies_1.1.1_x64-setup.exe](https://github.com/aa-matthew/Aillies/releases/download/v1.1.1/Aillies_1.1.1_x64-setup.exe) |
| Linux x64 | [DEB](https://github.com/aa-matthew/Aillies/releases/download/v1.1.1/Aillies_1.1.1_amd64.deb) · [AppImage](https://github.com/aa-matthew/Aillies/releases/download/v1.1.1/Aillies_1.1.1_amd64.AppImage) |
| Integrity / source | [SHA256SUMS](https://github.com/aa-matthew/Aillies/releases/download/v1.1.1/SHA256SUMS) · [Tagged source ZIP](https://github.com/aa-matthew/Aillies/releases/download/v1.1.1/Aillies-v1.1.1-source.zip) |

The release page also contains four CLI archives: macOS arm64/x64, Windows x64 and Linux x64. PR, main and release workflows passed on all three platforms. Mac is ad-hoc signed/unnotarized; Windows is unsigned. Current native/live acceptance limits remain in [STATUS.md](STATUS.md); Claude is experimental. Existing published assets are immutable.

## Previous — Codex/Claude 1.1.0

Published **2026-10-12**: [codex/claude-stable-1.1](https://github.com/aa-matthew/Aillies/releases/tag/codex/claude-stable-1.1), app **1.1.0**, marker `router-core-v11-review-fixes`. [Curated notes](releases/codex/claude-stable-1.1.md) record that release's scope and limitations. It predates the new onboarding, Iroh relay and safe-history repairs; use 1.1.1 on both devices for current internet pairing.

| Platform | Direct download |
|---|---|
| Apple Silicon · macOS13+ | [Aillies_1.1.0_aarch64.dmg](https://github.com/aa-matthew/Aillies/releases/download/codex/claude-stable-1.1/Aillies_1.1.0_aarch64.dmg) |
| Windows x64 | [Aillies_1.1.0_x64-setup.exe](https://github.com/aa-matthew/Aillies/releases/download/codex/claude-stable-1.1/Aillies_1.1.0_x64-setup.exe) |
| Linux x64 | [DEB](https://github.com/aa-matthew/Aillies/releases/download/codex/claude-stable-1.1/Aillies_1.1.0_amd64.deb) · [AppImage](https://github.com/aa-matthew/Aillies/releases/download/codex/claude-stable-1.1/Aillies_1.1.0_amd64.AppImage) |
| Integrity | [SHA256SUMS](https://github.com/aa-matthew/Aillies/releases/download/codex/claude-stable-1.1/SHA256SUMS) |

Archives use `codex-claude-stable-1.1` to keep filenames flat. Prior alpha packages below remain historical and are never relabeled as a stable release.

## Codex stable 1.0

Target release: [codex-stable-1.0](https://github.com/aa-matthew/Aillies/releases/tag/codex-stable-1.0), app version **1.0.0**, build marker `router-core-v4-pairing`. The owner explicitly authorized stable labeling with unsigned/ad-hoc installers and incomplete acceptance disclosed; see [release notes](releases/codex-stable-1.0.md) and decision 51. Publication occurs only after the tag’s platform checks/builds succeed. Download packages and `SHA256SUMS` from that release; do not relabel the older local alpha.12 binaries as 1.0.0.

Selected release assets are four versioned CLI archives (macOS arm64/x64, Windows x64, Linux x64), an Apple Silicon DMG, Windows x64 NSIS installer, Linux x64 DEB/AppImage, a tagged source ZIP and SHA256SUMS. Only these files are uploaded. Existing alpha artifacts below remain historical delivered files until the new release finishes.

## Previous local Claude candidate — alpha.9

`claude-integration`, version **1.1.0-alpha.9**, marker `router-core-v11-review-fixes`. Local packages, separate from stable and not published or installed automatically:

| Platform | Path under dist/ | SHA256 |
|---|---|---|
| Apple Silicon/macOS13+ | `macos/Aillies_1.1.0-alpha.9_review-fixes_aarch64.dmg` | `c69aab7fc283278870b59a445e37e5b285599f191f651bcb61127b1ddfae4bf3` |
| Windows x64 | `windows/Aillies_1.1.0-alpha.9_review-fixes_x64-setup.exe` | `402a67a9e4651ebdad97ba51c2d7ab80d2a0f6ef07740f3a2d3e09c2fdff5961` |

Mac is ad-hoc signed/unnotarized; mounted checksum/signature/executable/icon checks passed. Windows is unsigned and cross-built; NSIS integrity/extraction, PE64/version, exactly three expected bundle-marker bytes and notices/nine license files verified. Matching copies and checksum sidecars are in the owner's Downloads. Native installation, physical pairing/save-dialog/restart, browser OAuth and live entitlement remain unverified. See STATUS for final source checks.

## Previous local Claude branch — alpha.8

`claude-integration`, version **1.1.0-alpha.8**, marker `router-core-v10-activation`. Local Apple Silicon/macOS13+ package: `dist/macos/Aillies_1.1.0-alpha.8_use-share_aarch64.dmg`; SHA256 `917f8183df3bd099a7b1567834ca84bb08a4949db2f776f1c64e9d86c27b6ebd`. Ad-hoc signed/not notarized, mounted signature/checksum/executable/icon checks passed. This is separate from Codex stable and is not published or installed automatically. See [current acceptance](STATUS.md) for source/mock checks and remaining native/live work.

## Previous local alpha.12 selection

Selected delivered version: **0.9.0-alpha.12 / router-core-v4-pairing**. This is an explicit download allowlist, not a recursive dist upload. Old installers/source snapshots/provenance remain stored. Private archives, browser previews and cleanup-only validation builds are excluded.

| Platform | Path under dist/ | SHA256 |
|---|---|---|
| Apple Silicon Mac | `macos/Aillies_0.9.0-alpha.12_router-core-v4-pairing_aarch64.dmg` | `532cdb1476974755e2db9a1e1de2f9e08256bf00dba2dd5ad8f86be49293c605` |
| Windows x64 | `windows/Aillies_0.9.0-alpha.12_router-core-v4-pairing_x64-setup.exe` | `f34720854b45f78efb8221c02756ef54c7d0396fe502b56e8f0c938bcd1ca42c` |
| Immutable release source | `Aillies-router-core-v4-pairing-alpha12-source.zip` | `91aefa9e892afdd1cbe041525b6d00fe081e14a19a16578629140c4d0623f818` |

Mac: Apple Silicon, macOS13+, ad-hoc signed/unnotarized. Windows: x64 NSIS, unsigned, physical install pending. Both devices need alpha.12 for eight-digit LAN pairing. Current hardware gaps are in [STATUS.md](STATUS.md).

Associated notes/provenance: `Install-router-core-v4-pairing-alpha12.md`, `Install-router-core-v4-pairing-alpha12-windows.md`, `router-core-v4-pairing-alpha12-build.txt`, `router-core-v4-pairing-alpha12-windows-build.txt` and artifact `.sha256` files under dist.

The source ZIP is the immutable source snapshot that produced the delivered alpha.12 builds, before repository cleanup. Current source has the reviewed cleanup and documentation changes; cleanup validation does not silently replace those installers or their source ZIP. A future release needs its own source snapshot, artifacts and verification record.

Machine-readable local selection: `dist/current-release.json`. No release was uploaded or published by cleanup.
