<p align="center"><img src="assets/brand/logo-1024.png" width="96" alt="Aillies logo"></p>
<h1 align="center">Aillies</h1>
<p align="center">Use a trusted device’s Codex account from this device. The login never leaves the device that owns it.</p>

---

Aillies runs on each of your computers. Pair them once, choose which device to send through, and keep using Codex as usual. New Codex chats on this device run on the chosen device’s own Codex (ChatGPT) login and usage; that device streams the answers back, including reasoning events and generated images.

> **Status: 0.9.0-alpha.3.** Codex routing works end to end with the real Codex CLI (0.162.1) against a mock OpenAI backend, plus 34 automated tests. A two-device run on real accounts is the next step. Claude support comes later. See [docs/STATUS.md](docs/STATUS.md).

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

```text
aillies/
├── AGENTS.md            instructions for coding agents and new contributors (CLAUDE.md points here)
├── CONTRIBUTING.md      branches, commits, PR checklist, releases
├── SECURITY.md · LICENSE · CHANGELOG.md
├── Cargo.toml           Rust workspace · Makefile for common tasks
├── crates/
│   ├── aillies-core     protocol, request validation, stream framing, failure codes (pure)
│   ├── aillies-codex    Codex provider: login reader, upstream executor, config install/restore
│   ├── aillies-node     device node: identity, pairing, pinned TLS, loopback + control API
│   └── aillies-cli      `aillies` binary
├── apps/desktop/
│   ├── ui/              window UI (plain HTML/CSS/JS)
│   └── src-tauri/       Tauri 2 tray app
├── assets/brand/        logo, mark, wordmark
├── scripts/             install-macos.sh, smoke.sh, mock-upstream.py, build-windows-from-linux.sh
├── docs/                status, features, architecture, decisions, development, product spec, wireframes
├── legacy/              Peer Route 0.8.3 (Swift) and the Node prototype, for reference
└── .github/             CI, release builds, PR and issue templates
```

| Doc | |
|---|---|
| [AGENTS.md](AGENTS.md) | Start here if you’re an agent or new to the code |
| [docs/FEATURES.md](docs/FEATURES.md) | What it does and where each feature lives |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | How it works: protocol, flows, failure codes, security model |
| [docs/STATUS.md](docs/STATUS.md) | Progress, next steps, verification log |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | Build, test, debug |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to work in this repo |

Common tasks: `make test`, `make lint`, `make smoke`, `make run`, `make desktop`.

## Security notes

- The official ChatGPT desktop app is **not** supported: it can’t use another device’s account. Aillies works through Codex’s Responses provider setting.
- Aillies talks to Codex’s ChatGPT backend the way Codex does. This is not a documented public API and may change.
- OAuth tokens are read on the sharing device at request time and sent only to `chatgpt.com`. Prompts, outputs and tokens are never logged.

© 2026 Anyaxis Labs. All rights reserved.
