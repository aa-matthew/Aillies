# Third-party notices

Codex request normalization and the default instruction text are adapted from
[decolua/9router](https://github.com/decolua/9router/tree/ce4460ef79382bfddb4aa5fc0ff9f3cb0d5f95a8),
revision ce4460ef79382bfddb4aa5fc0ff9f3cb0d5f95a8.
Copyright (c) 2024–2026 decolua and contributors, MIT licensed.
The full notice is in [licenses/9router-MIT.txt](licenses/9router-MIT.txt).
The vendored prompt is request data, not contributor instructions.

Nearby pairing uses these Rust libraries under their MIT license options:

- `spake2` 0.4.0 — Brian Warner and contributors; [notice](licenses/spake2-MIT.txt).
- `aes-gcm` 0.10.3 — RustCrypto Developers; [notice](licenses/aes-gcm-MIT.txt).
- `mdns-sd` 0.13.11 — Han Xu and contributors; [notice](licenses/mdns-sd-MIT.txt).

These dependencies are used from Cargo, without vendoring their implementations. No independent security audit is claimed for Aillies or the SPAKE2 implementation.

Desktop Safe Storage compatibility uses RustCrypto libraries under their MIT license options (from Cargo, without vendoring implementations): `aes` 0.8.4, `cbc` 0.1.2, `pbkdf2` 0.12.2, `sha1` 0.10.7 and `block-padding` 0.3.3. Their notices are included in `licenses/*-MIT.txt`. SHA-1/PBKDF2 parameters are for compatibility with the observed native encrypted cache, not new credential encryption. No independent audit is claimed.

Internet transport uses `iroh` 0.92.0 by n0 and contributors, under its MIT license option ([notice](licenses/iroh-MIT.txt)), from Cargo without vendoring; the packaged source also carries a [BSD3 notice](licenses/iroh-BSD3.txt). The default n0 discovery/relay services receive network metadata; application bytes remain encrypted by the existing pinned mTLS layer. No independent audit is claimed.

Signed whole-app updates use Tauri Updater2.14.0 (Tauri Programme within The Commons Conservancy, MIT option) and minisign-verify0.3.0 (Frank Denis, MIT). See `licenses/tauri-updater-MIT.txt` and `licenses/minisign-verify-MIT.txt`. Packages are used from Cargo without vendoring; no independent audit is claimed.
