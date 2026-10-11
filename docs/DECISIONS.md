# Decisions

Short log of choices that shape the code. Newest last. Add one line of context and the consequence.

1. **Name: Aillies** (2026-10-10). Picked over “Ailliance”: shorter, matches the project name, reads as “allies”. Bundle id `is.anyax.aillies`. Logo: two overlapping rings, with the shared lens highlighted.
2. **Rust core + Tauri 2 shell.** One codebase for macOS, Windows and Ubuntu. Security behavior lives only in the Rust crates; the shells are thin.
3. **Identity = certificate fingerprint; pinned mutual TLS 1.3.** Replaces Peer Route’s shared pairing key: every device has its own identity, so revocation is per device and immediate.
4. **Invite code + verification code.** The invite carries B’s fingerprint (so A can’t be pointed at an impostor) and a one-time secret; the three-part code binds both fingerprints and the secret for a human comparison. Approval is required on both sides.
5. **No retries anywhere.** The executor never retries; Aillies writes `request_max_retries = 0` and `stream_max_retries = 0` into the Codex provider so Codex doesn’t replay either. Failures that may have reached OpenAI are labeled *delivery uncertain*.
6. **Tool allowlist follows real Codex.** codex-cli 0.162.1 sends `function`, `custom`, `tool_search` and `web_search` tools. All are allowed, plus `image_generation` and `local_shell`; anything else (for example remote `mcp`) is refused.
7. **New ports 18522 (loopback) and 18544 (peer)** so Aillies can run next to the legacy Peer Route (18322/18444) during migration.
8. **Alpha secrets in 0600 files.** Faster to ship; moving to OS credential stores is milestone M7.
9. **Plain HTML/JS window, no build step.** Matches the “small static UI” direction; contributors don’t need Node to build the app. Move to TypeScript once the UI grows.
10. **Upstream override only to loopback.** `AILLIES_CODEX_UPSTREAM` accepts `http://127.0.0.1:` or `http://localhost:` targets, so tests can run without a stray variable ever sending tokens elsewhere.
11. **AGENTS.md is the single agent guide** (2026-10-11). `CLAUDE.md` only imports it, so Claude, Codex and other agents follow the same rules. Feature locations live in `docs/FEATURES.md`.
12. **Restore removes only Aillies’ lines** (alpha.3). The Codex app edits `config.toml` itself (trusted projects, notices), so a byte-for-byte guard blocked real users. `merge_restore` reverses Aillies’ edits structurally and still refuses when it can’t parse.
13. **Windows builds use the GNU target** when cross-building from Linux (Ubuntu Rust + `-Zbuild-std` + mingw + NSIS); CI builds MSVC on Windows runners. See `scripts/build-windows-from-linux.sh`.

