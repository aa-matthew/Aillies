#!/usr/bin/env bash
# Cross-build Aillies for Windows x64 on Ubuntu 24.04 when rustup cannot
# download the Windows std (e.g. restricted network). Uses Ubuntu's Rust
# packages + -Zbuild-std + mingw-w64 + NSIS. Prefer CI (release.yml) when possible.
# Produces: target-win/.../aillies.exe and the NSIS installer.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
V=1.91
sudo apt-get install -y rustc-$V cargo-$V rust-$V-src gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 nsis
SYS=/usr/lib/rust-$V
LIB=$SYS/lib/rustlib/src/rust/library
# Ubuntu's std sources drop the Windows-only dependency; restore it.
grep -q windows-targets "$LIB/std/Cargo.toml" || sudo python3 - "$LIB/std/Cargo.toml" <<'PY'
import sys; p=sys.argv[1]; s=open(p).read()
s=s.replace("\n[dev-dependencies]","\n[target.'cfg(windows)'.dependencies.windows-targets]\npath = \"../windows_targets\"\n\n[dev-dependencies]",1)
open(p,"w").write(s)
PY
# rsbegin.o / rsend.o normally ship with the Windows std.
OBJ=$SYS/lib/rustlib/x86_64-pc-windows-gnu/lib
sudo mkdir -p "$OBJ"
for f in rsbegin rsend; do
  [ -f "$OBJ/$f.o" ] || sudo env RUSTC_BOOTSTRAP=1 "$SYS/bin/rustc" --target x86_64-pc-windows-gnu --emit=obj --crate-type=lib -Cpanic=abort -o "$OBJ/$f.o" "$LIB/rtstartup/$f.rs"
done
rustup toolchain link apt$V "$SYS" 2>/dev/null || true

export RUSTUP_TOOLCHAIN=apt$V RUSTC_BOOTSTRAP=1 CARGO_UNSTABLE_BUILD_STD=std,panic_abort
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar
T=x86_64-pc-windows-gnu
(cd "$ROOT" && cargo build --release -p aillies-cli --target $T --target-dir target-win)
(cd "$ROOT/apps/desktop/src-tauri" && cargo build --release --target $T --target-dir target-win)

# tauri-cli asks rustup for installed targets; linked toolchains can't answer.
FAKE=$(mktemp -d); printf '#!/bin/sh\n[ "$1 $2" = "target list" ] && { echo "%s (installed)"; exit 0; }\nexec "%s" "$@"\n' $T "$(command -v rustup)" > "$FAKE/rustup"; chmod +x "$FAKE/rustup"
(cd "$ROOT/apps/desktop/src-tauri" && PATH="$FAKE:$PATH" CARGO_TARGET_DIR=target-win cargo tauri bundle --target $T --bundles nsis)
ls -la "$ROOT/target-win/$T/release/aillies.exe" "$ROOT"/apps/desktop/src-tauri/target-win/$T/release/bundle/nsis/*.exe
