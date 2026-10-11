# Update A and B for Fast mode and image creation

The alpha.12 Windows x64 installer has now been cross-built and its package verified, alongside the Mac alpha.12 installer. Use the router-core-v4-pairing build on both devices for eight-digit nearby pairing. Physical Windows installation, WebView2 rendering and firewall/pairing acceptance still need verification. Keep existing app data and the Codex login.

## This Mac (A)

The updated app is installed and running at `~/Applications/Aillies.app`; a build copy is at `apps/desktop/src-tauri/target/release/bundle/macos/Aillies.app`. The Codex provider and image tool have been refreshed with a private safety copy of the previous settings. Pairing and account files stay in their existing locations.

1. The updated Aillies app is already running with the existing Windows peer selected in **Send through**.
2. After this coding session, quit and reopen Codex. Start a new chat in a trusted workspace.
3. Ask: “Use the Aillies generate_image tool to create a plain orange square.” Approve that tool in Codex if asked. A successful result includes a PNG saved under `~/.codex/generated_images/aillies/`.
4. Leave **Use Fast mode** off until B has been updated and its owner allows it.

The image tool is for new image creation. It respects Codex tool approvals. With approvals disabled and no saved approval for this tool, Codex may refuse the call before anything is sent. It does not replace Codex's native image editor.

## Windows sharing device (B)

There is no remote administration connection to B. The existing Aillies pairing connection cannot install software or change the owner's permission. Someone at B must perform these steps.

If a Windows installer is built from this source update, quit Aillies from its tray menu, run that installer, then open Aillies again. Keep its existing data folder and Codex login; pairing should remain intact.

To build the installer locally:

1. Copy and extract `dist/Aillies-update-source.zip` from this Mac onto B. The archive contains repository source, not account/state files.
2. Install the prerequisites from [DEVELOPMENT.md](DEVELOPMENT.md): Rust 1.87+, Visual Studio Build Tools with C++, and WebView2. Reopen PowerShell afterward.
3. In PowerShell, change to the extracted `aillies-update` folder and run:

```powershell
cargo test --workspace
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo install tauri-cli --version '^2' --locked
Set-Location apps/desktop/src-tauri
cargo tauri build --bundles nsis
```

4. Quit the old Aillies from its tray menu. Open the newly built installer in `apps/desktop/src-tauri/target/release/bundle/nsis/`, install, then start Aillies.
5. Keep **Share this device** on. Turn **Allow Fast mode** on only if you want approved devices to request Fast on this account.
6. On A, reopen the updated Aillies window (to refresh readiness), then optionally turn **Use Fast mode** on. B can revoke permission at any time. Subsequent Fast requests then fail before upstream; A must turn its preference off to resume standard requests.

Fast is a request for a service tier, not an entitlement grant or a promise of a particular speed or usage rate. If the selected model/account rejects the tier, Aillies reports the failure and does not downgrade or retry.

## Remaining live checks

The live text and PNG route from this Mac through Windows B already passed. These checks still need someone at B:

- Compare A and B's usage before and after a small request. Only B should be charged for the routed request.
- Stop sharing on B, then send once from A: a fixed failure, no fallback.
- Sign out of Codex on B, then send once from A: a sign-in failure, no local request.
- Revoke A on B, then send once from A: refused immediately. This ends that approval; pair again only if desired.
- Test both Fast switches and the actual tier entitlement after the Windows update.

These checks deliberately interrupt routing or approval. They were exercised with simulated nodes, but not performed remotely on the physical B device.

## GPT 6.1 Sol compatibility rebuild

Use packages whose filename contains `sol61-fix` on both devices. The displayed version remains 0.9.0-alpha.3, so the version alone cannot distinguish this rebuild. Quit Aillies from its tray/menu before replacing it. Keep its existing data folder to preserve pairing. Update B first, then A; restart Codex and enable Aillies integration if it is currently off. The rebuild fixes a missing text-model request format; image generation and acceptance of the public-preview identity headers still require a new live test. Do not resend an earlier failed turn automatically.
