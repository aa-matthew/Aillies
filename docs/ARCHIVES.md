# Archive and recovery index

The 2026-10-11 cleanup retired Peer Route from the active tree. The tracked legacy files were unchanged from base commit `e668e69c241aa0ae7451dc19acee2a18694323df`; they remain in Git history. No tag or commit was created by cleanup.

## Private local backup

Local directory: `dist/archive/cleanup-2026-10-11/` (ignored by Git; owner-only directory).

- `pre-cleanup-source.zip`: 70 files, including all 42 tracked legacy sources, both Peer Route investigation notes, old wireframes, eight removed icon variants and pre-edit cleanup files. SHA256: `9cf8778c671557d0be6149fedef7ed213a4bfa74e776957852690a1bd1233d87`.
- `manifest.json`: base commit and per-file SHA256 values; verified against every ZIP member before removal.
- `legacy-working-copy/`: complete original legacy directory, including ignored/generated files, moved intact. No open files were found before the move. This preserves local runtime/build material without putting it into a source release.

Restore into a separate directory before inspecting or running old code. In another clone, retrieve tracked files from the base commit with Git; local ignored material exists only in the private backup. Do not execute archival scripts as current setup instructions.

The protocol-evidence JSON is retained with the source backup and original working copy. A focused scan found no credential-shaped token values or credential fields; this is not a public-distribution or comprehensive secret-content clearance. The backup stays private and is excluded from release selection.

## Versioned documentation history

[history/](history/README.md) retains full pre-cleanup status, handoff, feature, architecture and product-proposal snapshots, with relative links rebased. The early [interactive wireframes](history/design/wireframes.html) remain design evidence. Historical policy statements, paths and capability claims are superseded by current documents and later decisions.

Current installers and immutable release source packages remain in dist, with provenance. [RELEASES.md](RELEASES.md) is an explicit allowlist; never recursively upload dist or automatically delete older artifacts/private archives.
