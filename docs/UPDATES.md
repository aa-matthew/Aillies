# App updates

Aillies bundles its HTML/CSS/JS and Rust backend into each version. The signed updater downloads whole compiled apps; it never executes remote source code.

## Using updates

The desktop app checks20seconds after startup and every6hours. Connection details also offers Check for updates. Verified downloads show Update & restart near Connection details. Choose it to pause new work and wait for existing requests to finish; Later cancels the wait and resumes work. The wait times out after15minutes without force-closing clients. Finish/cancel any provider-switch plan first. Accounts, pairings, routing receipts and history remain in app-data. Failed checks/downloads keep the current app usable; failed installation offers retry/manual download. No automatic downgrade/rollback promise is made.

Mac Apple Silicon, Windows x64 NSIS and Linux x64 AppImage are supported. Linux DEB and development builds link to the manual installer. Intel Mac desktop packages are not currently shipped. Version1.1.3 has no updater: install the first updater-enabled release manually once. Alpha builds read the stable feed and never push alpha releases to stable users.

## Signing and publication

1. Keep the updater private key in GitHub Actions Secrets as `TAURI_SIGNING_PRIVATE_KEY`; optional `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is empty for an unencrypted key. The public key is embedded in `apps/desktop/src-tauri/tauri.conf.json`. Back up the private key outside Git. Never place it in app-data, peer traffic, diagnostics, PR text or release assets. Losing the key requires a deliberate trust-key migration/manual reinstall.
2. Use Tauri CLI2.12.1 (or a deliberately tested successor) to generate signatures containing the exact app version. The app requires signed-version binding and rejects mismatched metadata.
3. Release workflow enables `createUpdaterArtifacts` only for signing builds. Ordinary local debug builds need no private key. macOS packaging additionally creates/signs a tar.gz from the exact sealed DMG app.
4. `scripts/prepare-update-release.py` requires all platform artifacts, exact version signatures and an explicit file allowlist. It generates latest.json and SHA256SUMS. Native CI/builds must pass before publication. Stable publication changes the latest feed; prereleases remain prereleases. Published assets are immutable.

Updater signatures are separate from Apple Developer ID/notarization and Windows code signing. Mac remains ad-hoc signed/unnotarized and Windows unsigned. Native OS restrictions can still require using a manual installer.

## Verification

`make test lint smoke` includes policy, response-lifetime drain, two-node active request and synthetic UI tests. The real Tauri verifier is tested with a throwaway signed payload, tampered bytes and an altered advertised version; no installer is executed. `python3 scripts/update-release-test.py` validates complete feed/checksum generation and missing/duplicate/wrong-version rejection. No real account, installed app or actual provider settings are used. Test a physical installed-app upgrade on each OS before claiming production acceptance.
