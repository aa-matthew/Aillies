# Current release selection

## Codex stable 1.0

Target release: [codex-stable-1.0](https://github.com/aa-matthew/Aillies/releases/tag/codex-stable-1.0), app version **1.0.0**, build marker `router-core-v4-pairing`. The owner explicitly authorized stable labeling with unsigned/ad-hoc installers and incomplete acceptance disclosed; see [release notes](releases/codex-stable-1.0.md) and decision 51. Publication occurs only after the tag’s platform checks/builds succeed. Download packages and `SHA256SUMS` from that release; do not relabel the older local alpha.12 binaries as 1.0.0.

Selected release assets are four versioned CLI archives (macOS arm64/x64, Windows x64, Linux x64), an Apple Silicon DMG, Windows x64 NSIS installer, Linux x64 DEB/AppImage, a tagged source ZIP and SHA256SUMS. Only these files are uploaded. Existing alpha artifacts below remain historical delivered files until the new release finishes.

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
