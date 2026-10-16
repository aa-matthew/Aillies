# Development

## Prerequisites

- Rust 1.87+ (`rustup`), Node.js 20+ for UI behavior/syntax checks and brand generation, plus Python 3 for the smoke test.
- Desktop app: `cargo install tauri-cli --version '^2' --locked`.
  - macOS: Xcode Command Line Tools.
  - Ubuntu: `sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev build-essential`.
  - Windows: Visual Studio Build Tools (C++), WebView2 (preinstalled on Windows 11).

## Everyday commands

The `Makefile` wraps these: `make test` (= `test-core` + `test-desktop` + `test-ui`), `make lint` (= `lint-core` + `lint-desktop` + `ui-check`), `make fmt`, `make smoke`, `make run`, `make desktop`, `make desktop-build`, `make install-macos`, `make windows`. The desktop targets need the platform prerequisites above; on Linux without WebKitGTK use `make test-core lint-core`.

```sh
cargo test --workspace              # unit + two-node end-to-end tests (mock upstream)
cargo clippy --workspace --all-targets
scripts/smoke.sh                    # two daemons + mock backend (+ real `codex exec` if installed)
cargo run -p aillies-cli -- run     # headless node
cd apps/desktop/src-tauri && cargo tauri dev     # desktop app
cd apps/desktop/src-tauri && cargo tauri build   # .app/.dmg, .msi/.exe, .deb/.AppImage
```

The window UI is plain files in `apps/desktop/ui`. Open `index.html` in a browser to preview it with fake data; add `?state=action`, `?state=pending` or `?state=new`. `mock.js` does nothing inside the app.

## Optional nearby-pairing check

`cargo test -p aillies-node --test e2e numeric_pairing_via_local_discovery -- --ignored --nocapture` uses synthetic homes/logins and local multicast, without live inference. It is opt-in because CI may block multicast. The default suite tests the same PAKE/approval path with deterministic discovery candidates. On macOS allow Aillies Local Network access when prompted; the bundle declares `_aillies-pair._tcp`. On Windows allow the app's peer TCP port and mDNS UDP 5353 on the trusted private network. Do not expose the loopback API. Full pairing codes bypass discovery when multicast is unavailable; direct reachability is still needed. Two physical Macs and Windows firewall/permission behavior are not covered by the same-host test.

## CodeGraph

CodeGraph maps symbols and dependencies for navigation and impact analysis. Run these commands from the repository root (check `codegraph --help` for the installed version):

```sh
codegraph status .
codegraph sync .                         # refresh after edits; one indexing session at a time
codegraph query merge_restore            # locate a symbol
codegraph node merge_restore             # source plus callers/callees
codegraph explore "Codex request routing"
codegraph impact merge_restore           # callers affected by a change
codegraph affected crates/aillies-codex/src/config.rs
```

If the repository has no index, run `codegraph init .`. Use `codegraph index .` only when a full rebuild is needed. The generated `.codegraph/` directory is ignored by Git. Index repository source only, never Codex/Aillies runtime state or login files.

The graph may lag active edits or miss dynamic calls, including UI-to-Tauri IPC. Confirm findings in source and use `rg` to fill gaps. Related-test suggestions do not replace the required full verification commands. CodeGraph is optional: contributors without it can use ordinary source searches.

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
| `AILLIES_CODEX_UPSTREAM` | Tests only. Parsed; accepted only for `http://` + loopback host (`127.x`, `::1`, `localhost`) + explicit non-default port, without user info or fragment. Anything else is ignored. |
| `AILLIES_ADVERTISE_LOOPBACK` | Add `127.0.0.1` to invites (two nodes on one machine). |
| `RUST_LOG` | Log level for `aillies run`. Content is never logged. |

## Files in the Aillies home folder

`identity.cert.der`, `identity.key.der` (device identity), `state.json` (settings, paired and approved devices), `secrets.json` (local Codex key, control token), `codex-restore.json` (Codex config backup while integration is on). All 0600.

## Rules for changes

- Security behavior belongs in the crates, never in the shells.
- Keep the invariants: no fallback to another account or device, bounded requester-owned retries (decision 45), OAuth stays on its device, fixed failure codes, no content in logs.
- Add a test for every new failure path. The two-node test in `crates/aillies-node/tests/e2e.rs` is the place for cross-device behavior.
- After a Codex update, run `scripts/smoke.sh` with the new `codex` installed; if Codex starts sending a new field or tool type, update `aillies-core/src/request.rs` and its tests deliberately.
- Update `docs/STATUS.md` (and `CHANGELOG.md` for user-visible changes) in the same commit.

## Building for Windows without a Windows machine

`scripts/build-windows-from-linux.sh` (Ubuntu 24.04) builds `aillies.exe` and the NSIS installer using Ubuntu’s Rust packages, `-Zbuild-std`, mingw-w64 and NSIS. Use it when rustup can’t download the Windows standard library; otherwise use the release workflow. Check the CLI under Wine with `wine aillies.exe --version`. The desktop window needs WebView2, which Wine lacks; the service and tray still start (see `desktop.log`).

## Releases

Push a tag `v*` (or the owner-authorized `codex-stable-1.0` or `codex/claude-stable-1.1`) to run `.github/workflows/release.yml`. Tag/application/lockfile versions must match. All CLI and desktop checks/builds must succeed before one publisher attaches explicit CLI archives, DMG, NSIS, DEB/AppImage, tagged source and `SHA256SUMS`, then publishes. Curated notes come from `docs/releases/<tag>.md`, otherwise GitHub generates notes. Pre-release version tags are marked prerelease; the owner-named 1.0/1.1 tags use stable labeling with outstanding acceptance disclosed (decisions 51/61). Slash tags keep their exact Git/release identity and notes path, while archive filenames use a flat hyphenated name. `make test` and CI cover tag/version/lockfile checks. Desktop bundles include notices and license files. Release preparation does not push a publishing tag. Manual dispatch requires the existing tag. Reruns can finish a draft but refuse to change a published release. Branch pushes still run CI only. Signing and notarization remain M8; unsigned/ad-hoc status and incomplete hardware acceptance must stay disclosed.

## Request diagnostics and Windows firewall

See [DIAGNOSTICS-WINDOWS.md](DIAGNOSTICS-WINDOWS.md) for export steps, retained fields, and a Private-profile TCP 18544 rule. `GET /control/diagnostics` reads the report; `POST /control/diagnostics/export` saves it to Downloads. Both require the existing control bearer token. The rolling journal is `request-diagnostics.json` in the Aillies home folder.

## Router core v4 checks and packages

`make test` also runs the lightweight settings behavior test (fake bridge, no user settings). Optional installed-Codex isolated provider-history check: `cargo test -p aillies-codex installed_codex_lists -- --ignored --nocapture`. It uses Python and the installed Codex app-server, with synthetic history and loopback-only failed inference; no live account is used. Regenerating 9router fixtures now writes both formatter and session relationship data.

For a Mac candidate, build `cargo tauri build --bundles app` in apps/desktop/src-tauri, then run `scripts/package-macos.sh` from the repository. The script copies the built app, seals the full bundle, creates an Apple Silicon DMG, mounts it read-only and verifies the strict signature and exact copied executable. Ad-hoc signing is not Developer ID/notarization. No installed application is replaced.

The mock smoke test waits for both nodes' authenticated control endpoints before pairing. A fixed startup sleep can race with first-run initialization while release builds are running; startup readiness never sends an inference request.

## macOS restart verification

Default unit/UI tests exercise changed/expired plans, shutdown refusal, the migration barrier, recovery, provider direction, duplicate UI input, pending provider selection and ChatGPT-only reopen using fake hosts. They never quit real apps. `cargo test -p aillies-codex local_process_preview -- --ignored --nocapture` is a read-only Mac process inventory check; it does not read chats/auth/environment or close/reopen anything. Live acceptance requires finishing active work, saving editor changes, using the new action, and checking ChatGPT reopens while editors and terminal clients stay closed. macOS may request Automation access to quit the named apps; denial leaves the migration blocked. Windows/Linux automatic restart is intentionally unavailable.

### Background provider-switch progress

Alpha.9 migration has no production wall-clock deadline. Keep Codex closed until the UI finishes; preparation runs on the existing blocking worker. A `provider_switch` status field contains only a closed phase enum and numeric completed/total counts, remains readable while migration owns its lock, and clears on return. It is not a diagnostic export. Tests simulate an elapsed preparation clock rather than sleeping or migrating real histories, and verify progress precedes commit. Optional private deadlines test cleanup/failure only.

### Brand assets

`node scripts/brand-icons.mjs` regenerates the SVG wrappers, window image, app PNG/ICNS/ICO and status tray PNGs from the exact PNGs in assets/brand/source. It uses the installed Tauri CLI, keeps intermediate/mobile outputs in a temporary directory, and does not change installed apps.

## Source and release hygiene

Current download selection is explicit in [RELEASES.md](RELEASES.md); do not recursively upload dist. Private legacy backups and cleanup verification bundles stay outside release selection. Historical contributor notes are evidence, not setup instructions; see [ARCHIVES.md](ARCHIVES.md). Keep both Cargo lockfiles, synthetic fixtures/probes, the default instruction text, mock.js and master artwork. The brand generator emits only configured platform inputs and the three runtime 44px tray icons.

Git ignores local browser artifacts in `.playwright-cli/`, `output/playwright/`, `playwright-report/` and `test-results/`, plus dependency/Python caches and `.env` values. Environment examples/templates remain eligible for Git. These rules do not delete files or untrack files already committed.

The embedded `codex_default_instructions.txt` uses an explicit LF checkout rule in `.gitattributes`; Windows `core.autocrlf` must not change upstream request data. CI follows current stable Rust, which can be newer than a locally installed stable toolchain; reproduce a reported lint with the compiler version shown in the job log.


## Claude branch development

1.1.0-alpha.3 adds `aillies-claude` to the workspace. [CLAUDE.md](CLAUDE.md) documents synthetic setup, test-only `AILLIES_CLAUDE_UPSTREAM`, `CLAUDE_CONFIG_DIR`, CLI controls and the opt-in installed-Claude mock probe. The requester installs a distinct local Claude token and client retry/compression/backend controls in user settings. Claude OAuth stays in its owner’s native Keychain/credential file or configured OAuth settings; `claude-restore.json` is private and never exported. Explicit test homes disable Keychain access. Native custom Keychain service names use NFC-normalized path hashes, matching Claude Code. Never test against a user’s live history/settings or put credentials in arguments/logs. Aillies does not refresh or overwrite native login data. The old alpha.1 API-key file is unused.

The Codex smoke daemons set distinct `AILLIES_TEST_CLAUDE_HOME` roots. This explicit test-only override disables native Keychain lookup and uses only synthetic file credentials. Smoke asserts both Claude accounts are unavailable before routing Codex. Never point this variable at a real account folder.

Desktop reader tests must inject `DesktopReader::with_test_password` with synthetic fixtures; never use real caches/Keychain during automated checks. `NodeConfig.claude_home` disables native OS credential reads. Desktop gateway config tests use isolated `desktop-configLibrary` roots. Independent OpenSSL fixtures cover read-only cache reuse, expiry, account change, token rotation and absence without CLI fallback. Desktop client source evidence is 2.31226.1 / Electron 44.4.3; native runtime and live inference are unverified. CLI controls: `aillies claude on/off --desktop` and `aillies claude share on --source desktop` (or `code`). Manual quit/reopen only.


### Connected Claude OAuth checks — alpha.5

Never authorize/read actual user credentials while developing. Explicit `NodeConfig.claude_home` fixture roots route Aillies-owned OAuth storage to `claude-oauth-fixture.json` under the synthetic Aillies home and disable native Keychain. A configured mock inference origin is reused for OAuth only with that explicit test root, and independently must parse as an HTTP loopback origin with port and no userinfo/path/query/fragment. Production Connect is Mac-only and stores tokens in private accounts/claude.json (0700 directory,0600 file, atomic updates). Older Keychain connections require explicit reconnect; never import/read them automatically. `claude_oauth_*` / `claude_connected_*` e2e tests mock exchange, bootstrap, refresh and native Messages; native browser/Keychain/live acceptance remains manual. Control start returns an ephemeral authorization URL; finish receives the complete code in a private authenticated JSON body, never CLI arguments or logging. Tests use synthetic token/error values only.

Claude model discovery tests use synthetic account roots and the mock Anthropic models endpoint: `cargo test -p aillies-node --test e2e model`. They cover both client paths, local/peer ownership, grants, query validation, bounded retries, malformed catalogs and automatic transactional profile upgrade. Do not probe real credentials/settings to run these checks.

Claude frontier adapter unit cases live in `claude_compat_tests.rs`; two-node discovery/inference and retry/refusal cases use synthetic connected account roots in `e2e.rs` (`claude_frontier`). Model rules are reviewed against the sources in CLAUDE.md, never inferred from arbitrary name substrings. Keep raw signed-history preservation tests when extending them.


## Alpha.8 activation and cleanup checks

`make test lint smoke` covers synthetic config cleanup/restore, private OAuth permissions and refresh intent, combined reviewed closure ordering, UI pending state/Cancel/double-apply protection, independent exit reports and Quit anyway. The macOS engine inventories first; only a user-reviewed apply can quit clients. Never run apply against this active coding chat while developing. Browser mock states include `?state=cleanup` for incomplete cleanup. Live app/save-dialog/authorization behavior is still manual acceptance.

Connected account storage is accounts/claude.json under Aillies home, with user-only Unix permissions and atomic writes. No automatic Keychain migration is performed. A missing private connection with retained old selection disables sharing and requires Reconnect. Real credentials must not be fixtures or appear in logs/arguments/diagnostics. Name refresh is bounded metadata over an approved pinned status request; grants and fingerprints do not change.


### Alpha.9 regression checks

`claude_peer_retries_pin_key_and_stop_on_destination_changes` includes source switching with unchanged old credentials and same-account consent off/on. The pre-fix source regression executed four times instead of once. Non-macOS node test `manually_closed_clients_can_activate_restore_and_disconnect` covers Codex/Claude settings in both directions, one-use plans and empty quit; run in an isolated environment with clients closed. Parser tests include Windows image names and native versioned Claude paths. UI contracts include manual-close/reopen instructions and Claude-only paired service status. Do not exercise native shutdown or actual accounts during checks.


### Scheduled activation and scoped invitations

Focused checks: `cargo test -p aillies-node scheduled_switch`, `cargo test -p aillies-codex waiting_summary`, `cargo test -p aillies-codex review_count`, and `cargo test -p aillies-node --test e2e scoped_invitation`. All use synthetic account/settings roots; controller tests inject process probes/apply callbacks, never terminate real apps. Do not use native activation apply while this coding chat is running. Run full `make test lint smoke` after changing pairing/config progress. Scoped desktop invites bind grants in the node; the public peer protocol still carries only the existing secret/proof. Scheduled state is not persisted or exported in diagnostics. Mock previews support `?theme=light`/`dark`; scheduled completion there is simulated and proves only UI behavior.


Iroh regression: `cargo test -p aillies-node --test e2e iroh_full_code`. Default tests use explicit loopback addresses, disabled relay/discovery and synthetic credential homes. Optional `iroh_public_discovery_full_code_probe -- --ignored` uses public n0 discovery/relay with temporary synthetic accounts and loopback mock inference only; it does not prove physical cross-network or relay-only connectivity.


History migration checks: `cargo test -p aillies-codex provider_switch`, `cargo test -p aillies-node --test e2e history_diagnostics_are_authenticated`, and the optional `installed_codex_lists_and_resumes_migrated_history -- --ignored` use synthetic homes only. Required full `make test lint smoke` remains. Check byte-for-byte preservation of skipped files, unchanged skipped index rows, mixed batches/replicas, legacy null fields, final delimiter offsets, newest compatible/unsupported database candidates, journal recovery and bounded/redacted diagnostics. Never exercise provider switching against the development chat's actual home or running clients.

## Signed updater

Desktop builds require Rust1.90+ for Tauri Updater; the headless workspace retains Rust1.87. Use Tauri CLI2.12.1 for signed-version artifacts. See [UPDATES.md](UPDATES.md) for signing secrets, bootstrap installation, supported formats and verification. Plain local app builds do not require signing secrets; release builds enable updater artifacts via a config override.
