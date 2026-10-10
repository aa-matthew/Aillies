# Development

## Prerequisites

- Rust 1.87+ (`rustup`), plus Python 3 for the smoke test.
- Desktop app: `cargo install tauri-cli --version '^2' --locked`.
  - macOS: Xcode Command Line Tools.
  - Ubuntu: `sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev build-essential`.
  - Windows: Visual Studio Build Tools (C++), WebView2 (preinstalled on Windows 11).

## Everyday commands

```sh
cargo test --workspace              # unit + two-node end-to-end tests (mock upstream)
cargo clippy --workspace --all-targets
scripts/smoke.sh                    # two daemons + mock backend (+ real `codex exec` if installed)
cargo run -p aillies-cli -- run     # headless node
cd apps/desktop/src-tauri && cargo tauri dev     # desktop app
cd apps/desktop/src-tauri && cargo tauri build   # .app/.dmg, .msi/.exe, .deb/.AppImage
```

The window UI is plain files in `apps/desktop/ui`. Open `index.html` in a browser to preview it with fake data; add `?state=action`, `?state=pending` or `?state=new`. `mock.js` does nothing inside the app.

## Running two nodes on one machine

Each node needs its own home folder and ports:

```sh
AILLIES_HOME=/tmp/a AILLIES_LOOPBACK_PORT=18722 AILLIES_PEER_PORT=18745 aillies run
AILLIES_HOME=/tmp/b AILLIES_LOOPBACK_PORT=18622 AILLIES_PEER_PORT=18644 AILLIES_ADVERTISE_LOOPBACK=1 aillies run
```

CLI commands find a node through `AILLIES_HOME` (they read its `secrets.json`).

## Environment variables

| Variable | Purpose |
|---|---|
| `AILLIES_HOME` | State folder. Default: `~/Library/Application Support/is.anyax.Aillies` (macOS), `~/.config/aillies` (Linux), `%APPDATA%\anyax\Aillies\config` (Windows). |
| `AILLIES_LOOPBACK_PORT` / `AILLIES_PEER_PORT` | Default 18522 / 18544. |
| `AILLIES_CODEX_AUTH` | Codex login file to share. Default `$CODEX_HOME/auth.json` or `~/.codex/auth.json`. |
| `CODEX_HOME` | Where Codex keeps `config.toml` and `auth.json`. |
| `AILLIES_CODEX_UPSTREAM` | Tests only. Accepted only for `http://127.0.0.1:` / `http://localhost:` targets. |
| `AILLIES_ADVERTISE_LOOPBACK` | Add `127.0.0.1` to invites (two nodes on one machine). |
| `RUST_LOG` | Log level for `aillies run`. Content is never logged. |

## Files in the Aillies home folder

`identity.cert.der`, `identity.key.der` (device identity), `state.json` (settings, paired and approved devices), `secrets.json` (local Codex key, control token), `codex-restore.json` (Codex config backup while integration is on). All 0600.

## Rules for changes

- Security behavior belongs in the crates, never in the shells.
- Keep the invariants: no fallback to another account or device, no replay, OAuth stays on its device, fixed failure codes, no content in logs.
- Add a test for every new failure path. The two-node test in `crates/aillies-node/tests/e2e.rs` is the place for cross-device behavior.
- After a Codex update, run `scripts/smoke.sh` with the new `codex` installed; if Codex starts sending a new field or tool type, update `aillies-core/src/request.rs` and its tests deliberately.
- Update `docs/STATUS.md` (and `CHANGELOG.md` for user-visible changes) in the same commit.

## Releases

Push a tag `v*` to run `.github/workflows/release.yml`: it builds the CLI for macOS (arm64, x64), Windows and Linux, and the desktop bundles, then attaches them to a draft GitHub release. Signing and notarization secrets are milestone M8.
