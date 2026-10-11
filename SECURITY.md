# Security

## Reporting

Email **security@anyax.is** with details and reproduction steps. Please don’t open public issues for vulnerabilities.

## Scope and model

Aillies lets an approved device run Codex requests on another device’s ChatGPT login. In scope:

- Pairing (one-time invite secret, verification code, approval on both sides) and per-device revocation
- Pinned mutual TLS 1.3 between devices; authorization by certificate fingerprint
- The loopback API (127.0.0.1 only, `Host` check, separate bearer tokens for Codex and control)
- Handling of `~/.codex/auth.json` on the sharing device (read at request time, sent only to `chatgpt.com`)
- Codex `config.toml` install/restore (transactional, receipt-based)
- Logging: no prompts, outputs, images, tokens or upstream bodies

Known limitations (alpha): secrets are 0600 files rather than in the OS credential store; builds are unsigned; there is no relay yet, so devices connect directly. See `docs/STATUS.md`.

Anything an approved device sends runs on the sharing account and uses its quota. Approve only devices you control, and revoke a device as soon as you stop trusting it.
