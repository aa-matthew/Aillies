<p align="center"><img src="assets/brand/logo-1024.png" width="96" alt="Aillies logo"></p>
<h1 align="center">Aillies</h1>
<p align="center">Use a trusted device’s Codex account from this device. The login never leaves the device that owns it.</p>

---

Aillies runs on each of your computers. Pair them once, choose which device to send through, and keep using Codex as usual. New Codex chats on this device run on the chosen device’s own Codex (ChatGPT) login and usage; that device streams the answers back, including reasoning events and generated images.

> **Status: 0.9.0-alpha.1.** Codex routing works end to end with the real Codex CLI (0.162.1) against a mock OpenAI backend, plus 32 automated tests. A two-device run on real accounts is the next step. Claude support comes later. See [docs/STATUS.md](docs/STATUS.md).

## How it works

```text
Codex on A ──HTTP──▶ Aillies on A (127.0.0.1) ══ pinned mutual TLS ══▶ Aillies on B ──▶ OpenAI (B’s Codex login)
                         ▲                                                    │
                         └──────────────── streamed response ◀────────────────┘
```

- **Pairing:** B shows a one-time code; A enters it; both screens show the same three-part verification code (for example `MINT · 45 · SAGE`); both people approve. Each device then knows the other by certificate fingerprint.
- **No fallback, no replay:** if B can’t be reached, its login has expired, or the account changed, the request fails with a clear message and nothing is sent elsewhere. Aillies never retries a request that may already have run, and it configures Codex not to retry either.
- **Per-device control:** each approved device can be revoked on its own, effective immediately.

## Quick start (two computers, terminal)

Signed installers aren’t available yet. On a Mac, one script installs Rust if needed, then builds the `aillies` command and **Aillies.app**:

```sh
scripts/install-macos.sh            # or: scripts/install-macos.sh --cli-only
```

Elsewhere: `cargo install --path crates/aillies-cli --locked` (Rust 1.87+).

On **B**, the computer whose Codex login you want to use (signed in to Codex already):

```sh
aillies run              # keep this terminal open
aillies invite           # in a second terminal; shows a code, then asks you to approve
```

On **A**, the computer you work on:

```sh
aillies run              # keep this terminal open
aillies pair 'aillies1:…'  # paste B's code; compare the verification code; approve on B
aillies codex on         # points Codex at Aillies (backs up your config)
```

Quit and reopen Codex, then start a new chat. Useful commands: `aillies status`, `aillies check`, `aillies use local|<device>`, `aillies pause`, `aillies revoke <device>`, `aillies codex off`.

Both computers must reach each other directly: the same network, or a private network such as Tailscale. Pairing across the internet without one comes later (rendezvous relay).

## Desktop app

`apps/desktop` is a tray/menu-bar app with one compact window, built with Tauri 2. It embeds the same node, so run either the app or `aillies run`, not both.

```sh
cargo install tauri-cli --version '^2' --locked
cd apps/desktop/src-tauri && cargo tauri dev     # or: cargo tauri build
```

## Repository

| Path | What |
|---|---|
| `crates/aillies-core` | Protocol, request validation, stream framing and failure codes. Pure, platform-neutral. |
| `crates/aillies-codex` | Codex provider: login file reader, upstream executor, transactional `config.toml` install/restore. |
| `crates/aillies-node` | Device node: identity, pairing, pinned TLS transport, loopback and control APIs. |
| `crates/aillies-cli` | `aillies` binary (headless daemon and control). |
| `apps/desktop` | Tauri tray app and the plain HTML/JS window. |
| `assets/brand` | Logo, mark and wordmark. |
| `scripts/` | `install-macos.sh`, `smoke.sh` (two nodes + mock backend + real Codex), `mock-upstream.py`. |
| `docs/` | [Status](docs/STATUS.md), [architecture](docs/ARCHITECTURE.md), [development](docs/DEVELOPMENT.md), [decisions](docs/DECISIONS.md), [product spec](docs/PRODUCT-SPEC.md). |
| `legacy/` | Peer Route 0.8.3 (Swift macOS preview) and the older Node/SSH prototype, kept for reference. |

## Security notes

- The official ChatGPT desktop app is **not** supported: it can’t use another device’s account. Aillies works through Codex’s Responses provider setting.
- Aillies talks to Codex’s ChatGPT backend the way Codex does. This is not a documented public API and may change.
- OAuth tokens are read on the sharing device at request time and sent only to `chatgpt.com`. Prompts, outputs and tokens are never logged.

© 2026 Anyaxis Labs. All rights reserved.
