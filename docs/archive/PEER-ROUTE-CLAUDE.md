# Peer Route — Claude entry point

Read [`docs/CLAUDE-HANDOFF.md`](docs/CLAUDE-HANDOFF.md) before changing code. It is the current source of truth for product scope, verified behavior, architecture, design direction, and next steps.

Also read:

- [`docs/NEXT-VERSION-MIGRATION.md`](docs/NEXT-VERSION-MIGRATION.md) for the proposed 0.9 cross-platform architecture.
- [`macos/README.md`](macos/README.md) for the existing native macOS preview and its operational limits.
- [`macos/CHATGPT-ADAPTER.md`](macos/CHATGPT-ADAPTER.md) only when working on the experimental official ChatGPT desktop investigation.

Do not infer that the official ChatGPT desktop app can use a peer's account. The working account-sharing path is the loopback Responses-compatible endpoint used by Codex. Keep credentials on the sharing peer and preserve fail-closed behavior.

