# AGENTS.md

Instructions for coding agents (Claude, Codex and others) and new contributors. Read this whole file before changing anything.

## What this repo is

**Aillies** lets one computer use a paired computer’s Codex (ChatGPT) login. The login never leaves the device that owns it. Codex on device A talks to Aillies on `127.0.0.1`. Aillies forwards requests over pinned mutual TLS to device B, with the bounded requester-owned retry policy below. B runs each attempt once with B’s own login. A buffers generation events until an attempt completes. Claude support is planned (milestone M9).

Formerly called **Peer Route**. Historical code is retired from the active tree; [docs/ARCHIVES.md](docs/ARCHIVES.md) records the private backup and Git recovery point.

## Read before coding

| Order | File | Why |
|---|---|---|
| 1 | [`docs/STATUS.md`](docs/STATUS.md) | What works, what’s next, known risks, the verification log |
| 2 | [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Processes, crates, protocol, request and pairing flows, state, threat model |
| 3 | [`docs/FEATURES.md`](docs/FEATURES.md) | Every user-facing feature, where it lives in code, and its status |
| 4 | [`docs/DECISIONS.md`](docs/DECISIONS.md) | Why things are the way they are. Append; never rewrite history |
| 5 | [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) | Build, test, smoke test, environment variables, cross-builds |
| 6 | [`CONTRIBUTING.md`](CONTRIBUTING.md) | Branches, commits, PR checklist, releases |
| 7 | [`docs/HANDOFF-CODEX.md`](docs/HANDOFF-CODEX.md) | Latest session handoff: open bugs, failed approaches, mock-vs-real coverage, next task |

## Repository map

```text
crates/aillies-core     Pure logic: request validation, SSE detection, failure codes, protocol, verification code. No I/O.
crates/aillies-codex    Codex provider: auth.json reader, upstream executor, config.toml install/restore.
crates/aillies-node     Device node: identity, pairing, pinned TLS, peer server/client, loopback + control API, state.
crates/aillies-cli      `aillies` binary: headless daemon (`aillies run`) and control commands.
apps/desktop/ui         Window UI: plain HTML/CSS/JS, no build step (mock.js previews it in a browser).
apps/desktop/src-tauri  Tauri 2 shell: tray/menu bar, window, IPC commands. Thin wrapper over aillies-node.
assets/brand            Logo, mark, wordmark (SVG + PNG).
scripts/                install-macos.sh, smoke.sh, mock-upstream.py, build-windows-from-linux.sh
docs/                   Current status/architecture/features, decisions, roadmap, release/archive indices.
docs/history/           Dated evidence and early wireframes. Historical only, not current instructions.
```

## Invariants. Never break these

Each should have a test that fails if it breaks; current coverage and gaps are mapped in [`docs/HANDOFF-CODEX.md`](docs/HANDOFF-CODEX.md#current-invariant-coverage). If a change needs to bend one, stop and ask.

1. **No fallback.** Never send a request through another account, another device or a direct route. Fail closed with a fixed `FailureCode`.
2. **Bounded, explicitly authorized retries** (user override, 2026-10-11; decision 45). The requesting node may retry upstream errors, interrupted responses and transient transport/busy failures up to three times after the first attempt, on the same device, account and model. Buffer generation events until a successful terminal event; never mix attempts. Stop on cancellation, route/account changes or permission/validation refusals. The executor and sharing node still execute each attempt once; Codex `request_max_retries = 0` and `stream_max_retries = 0` prevent nested budgets. Possibly delivered attempts can consume usage; keep their warnings and never claim nothing was sent for the whole turn after such an attempt.
3. **Credentials stay home.** OAuth tokens are read on the sharing device at request time, sent only to `chatgpt.com`, and never cross the peer link.
4. **No content in logs or errors.** No prompts, outputs, images, tokens or upstream error bodies, anywhere. Errors are fixed codes plus fixed messages (`aillies-core/src/failure.rs`).
5. **Identity is the certificate fingerprint.** Peers are authorized per request against the approved list. Pairing needs the one-time secret and approval on both devices.
6. **Codex config edits are transactional.** Install keeps a receipt. Restore removes only Aillies’ lines, keeps later edits, and refuses (changing nothing) when it can’t parse safely.
7. **Security logic lives in crates, not shells.** The CLI and desktop app only call `Node` methods.
8. **Loopback stays loopback.** The local API binds `127.0.0.1`, checks the `Host` header and requires a bearer token. The test-only upstream override is a parsed URL: `http`, loopback host, explicit port, no user info.
9. **Don’t claim official ChatGPT desktop support.** That app can’t use another device’s account.

## Commands

```sh
make test        # workspace tests (unit + two-node end-to-end) + desktop shell tests
make lint        # fmt check + clippy -D warnings for workspace and desktop + UI JS syntax check
make test-core lint-core   # workspace only (Linux without WebKitGTK)
make smoke       # two daemons + mock backend (+ real `codex exec` if installed)
make run         # headless node: cargo run -p aillies-cli -- run
make desktop     # cargo tauri dev
```

Without `make`, see the same commands in [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md).

## CodeGraph

Use CodeGraph alongside `rg` to understand call paths and change impact in this repo. The CLI is `codegraph`; use its MCP equivalents when available. See [DEVELOPMENT.md](docs/DEVELOPMENT.md#codegraph) for commands.

- Check `codegraph status .` before relying on the graph. When changes are pending, run `codegraph sync .` before analysis; coordinate with other active work so only one session refreshes the shared index.
- Locate relevant symbols with `codegraph query`, then inspect callers/callees with `codegraph node` or explore a feature with `codegraph explore`.
- Before changing shared request, auth, pairing, transport or config logic, use `codegraph impact` to inspect affected callers. Use `codegraph affected` to find related tests.
- Verify graph results against current source. An empty result does not prove there are no callers or tests; use `rg` for text, dynamic JS/IPC connections and gaps in graph coverage.
- Graph-selected tests supplement the required `make test lint` and smoke checks; they do not replace them.
- Keep `.codegraph/` local and out of Git. Never index runtime account/state directories or credentials. If CodeGraph is unavailable, continue with `rg` and direct source inspection.

## How to make a change

1. Find the feature in `docs/FEATURES.md`; it names the files involved.
2. Write or extend a test first:
   - pure logic → the module’s `#[cfg(test)]` block;
   - anything that crosses devices → `crates/aillies-node/tests/e2e.rs`.
3. Keep functions small and errors typed. Return `Failure` for request paths and `anyhow::Error` with a user-readable message for control paths.
4. Run `make test lint`. Run `make smoke` if you touched networking, the executor or the Codex config.
5. Update docs in the same commit: `STATUS.md` (verification log or milestones), `FEATURES.md` (if behavior changed), `DECISIONS.md` (if you made a choice), `CHANGELOG.md` (if users can notice).

## Style

- Rust 2021, MSRV 1.87, `cargo fmt` with the repo `rustfmt.toml`, no `unwrap()` on input-derived data outside tests.
- User-facing text: short, plain, says what happened and what to do; “Nothing was sent.” when that is true.
- UI: three status colors only (green ready, gray paused/this device, orange action required); accent color only for buttons and selection.
- Escape every peer-provided string before putting it into HTML (`esc()` in `apps/desktop/ui/app.js`).

## Gotchas

- Two nodes on one machine need distinct `AILLIES_HOME`, `AILLIES_LOOPBACK_PORT` and `AILLIES_PEER_PORT`.
- The CLI daemon and the desktop app both bind 127.0.0.1:18522; run one.
- Codex changes its own `config.toml`; restore must tolerate that (see `merge_restore`).
- After a Codex update, rerun `make smoke` with the new `codex`. New request fields or tool types must be allowed deliberately in `aillies-core/src/request.rs`.
- `codex exec` waits on stdin; run it with `< /dev/null` in scripts.
- Never decide stream completion by searching bytes; use `wire::TerminalScanner` (SSE-aware).
- Never validate URLs with string prefixes; parse them (`reqwest::Url`).
- Don’t `pkill -f` a pattern that also matches your own shell’s command line.
