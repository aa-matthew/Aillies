# Contributing to Aillies

Guidance for working in this repository. Agents should also follow [AGENTS.md](AGENTS.md).

## Workflow

- `main` is always releasable: tests and clippy pass, and docs match behavior.
- Work on short-lived branches: `feat/<topic>`, `fix/<topic>`, `docs/<topic>`, `chore/<topic>`.
- Open a pull request into `main`; CI must pass (tests and clippy on macOS, Windows and Ubuntu, plus desktop builds).
- Squash-merge with a message that explains *why*.

## Commits

```text
<area>: <what changed, imperative, ≤ 72 chars>

<why it was needed and anything a reviewer should know>
```

Areas: `core`, `codex`, `node`, `cli`, `desktop`, `ui`, `docs`, `scripts`, `ci`, `release`.

## Pull request checklist

- [ ] Tests added or updated (`make test`)
- [ ] `make lint` passes
- [ ] `make smoke` run if networking, the executor or the Codex config changed
- [ ] Invariants in AGENTS.md still hold
- [ ] `docs/STATUS.md` updated (verification log or milestones)
- [ ] `docs/FEATURES.md` updated if behavior changed
- [ ] `docs/DECISIONS.md` entry for any new design choice
- [ ] `CHANGELOG.md` entry if users can notice the change

## Versioning and releases

- Semantic versioning with pre-release tags: `0.9.0-alpha.N` → `0.9.0-beta.N` → `0.9.0`.
- Keep the version in sync in `Cargo.toml` (workspace), `apps/desktop/src-tauri/Cargo.toml` and `apps/desktop/src-tauri/tauri.conf.json`.
- Release: update `CHANGELOG.md`, bump versions and both Cargo lockfiles, then tag `vX.Y.Z[-pre]` and push the tag. The owner-authorized 1.0.0 release uses `codex-stable-1.0` (decision 51). `.github/workflows/release.yml` validates tag/version agreement, runs checks and builds the CLI for macOS (arm64/x64), Windows and Linux, plus desktop bundles. A final job attaches explicit artifacts, source and SHA256 checksums to a draft, then publishes only after all checks/builds succeed. Prerelease version tags stay marked prerelease.
- Put curated release notes in `docs/releases/<tag>.md`; otherwise GitHub generates notes. Manual reruns require an existing tag. Published assets are never overwritten; a failed draft upload can be retried. Ordinary branch pushes run CI, without releasing.
- Installers are release assets, never committed (`/dist/` is ignored).

## Docs map

| Doc | Owner question |
|---|---|
| `README.md` | What is it and how do I start? |
| `AGENTS.md` | How should an agent or new contributor work here? |
| `docs/STATUS.md` | Where are we, what’s next, what was verified? |
| `docs/FEATURES.md` | What can it do and where is that implemented? |
| `docs/ARCHITECTURE.md` | How does it work inside? |
| `docs/DECISIONS.md` | Why is it built this way? |
| `docs/DEVELOPMENT.md` | How do I build, test and debug? |
| `docs/PRODUCT-SPEC.md` | What is the full target product? |
| `SECURITY.md` | How do I report a problem and what is in scope? |

## Reporting bugs

Include OS and version, Aillies version (`aillies --version` or Connection details), what you did, and what you saw. Attach `desktop.log` from the Aillies folder (it holds lifecycle events only). Never attach `secrets.json`, `identity.key.der` or Codex’s `auth.json`.
