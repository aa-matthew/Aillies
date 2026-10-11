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
