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
14. **Upstream override is parsed, not prefix-matched** (2026-10-11, review finding 1). `upstream_override` parses the URL and accepts only `http`, a loopback host (`127.0.0.0/8`, `::1`, `localhost`), an explicit non-default port, no user info and no fragment. Before this, `http://127.0.0.1:80@example.com/…` passed while targeting `example.com`.
15. **Stream completion means a real SSE terminal event** (2026-10-11, review finding 2). `TerminalScanner` parses SSE framing incrementally (any chunking, `\n` or `\r\n`, comments ignored, dispatch on a blank line). It counts `response.completed|failed|incomplete` only from the `event:` field or the payload’s top-level `type`. Lines are capped at 4 KiB, so huge payloads aren’t buffered; a truncated payload is read only if it starts with `{"type":"…"`. An event without its terminating blank line isn’t dispatched, so a cut stream is reported as *delivery uncertain*.
16. **Desktop crate stays outside the Cargo workspace, but `make test`/`make lint` cover it** (2026-10-11, review finding 3). It pulls WebKitGTK/WebView system libraries and has its own lockfile, so `cargo test --workspace` stays portable. `make test` = `test-core` + `test-desktop`; `make lint` also runs desktop fmt and clippy plus a UI JS syntax check; CI runs the same per OS.
17. **Model choice.** Install keeps the user’s existing root `model`; it writes `gpt-5.5` only when none is set, unless `--model` is given.
18. **Codex client identity.** Upstream requests send `originator: codex_cli_rs`, `User-Agent: codex_cli_rs/0.159.0` and `version: 0.159.0`, carried over from Peer Route. The real Codex is newer (0.162.1); revisit if the backend starts checking versions. *Unverified against the live backend.*
19. **No system proxy for upstream.** The executor ignores `HTTPS_PROXY` (`no_proxy()`, as in Peer Route) so tokens never go through an intermediary. Sharing devices that require a proxy can’t reach OpenAI. This is a known limitation.
20. **Invites list every non-loopback IPv4 address plus `<hostname>.local`** (and `127.0.0.1` only with `AILLIES_ADVERTISE_LOOPBACK`). Whoever holds the code learns these LAN addresses. Accepted because the code is private and single-use.
21. **Convenience defaults.** Creating an invite turns sharing on. The first successful pairing selects the new device if the destination was *This device*.
22. **Pause blocks every destination**, including *This device* (fail closed; Codex shows `routing_paused`).
23. **Gray for “This device”.** Following the spec, green means routing through a paired device, gray means paused or using this device, orange means action required.
24. **Desktop lifecycle.** Node, then tray, then the window on demand. Closing the window hides it; Quit stops the node. The second instance hands off to the first via `tauri-plugin-single-instance`.
25. **Windows installer** is per-user (no admin) and fetches WebView2 with the download bootstrapper (embedding it was blocked in the build environment).

