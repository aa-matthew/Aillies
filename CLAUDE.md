# Aillies — read this first

Aillies lets a device use a paired device’s Codex login. Claude support is planned (milestone M9). Formerly “Peer Route”.

Before changing code, read in this order:

1. [`docs/STATUS.md`](docs/STATUS.md): what works, what’s next (M5 checklist), known risks. Update it with every change.
2. [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md): processes, crates, pairing and request flow.
3. [`docs/DECISIONS.md`](docs/DECISIONS.md): why things are the way they are. Append when you make a new choice.
4. [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md): build, test, smoke test, env vars, change rules.
5. [`docs/PRODUCT-SPEC.md`](docs/PRODUCT-SPEC.md): the full product spec (UX, relay, platforms). Not all of it is built.

Invariants. Never break them; each has a test:

- Never fall back to another account, another device or a direct route. Fail closed with a fixed failure code.
- Never replay a request. Anything that may have reached OpenAI is reported as *delivery uncertain*.
- OAuth material stays on the device that owns it and is read at request time.
- No prompts, outputs, images or tokens in logs or error messages.
- Codex config edits are transactional and never overwrite later user edits.
- Don’t claim official ChatGPT desktop support. It can’t use another device’s account.

Verify with `cargo test --workspace` and `scripts/smoke.sh`. `legacy/` is reference only; don’t modify it unless asked.
