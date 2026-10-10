#!/usr/bin/env bash
# Build and install Aillies on this Mac from source.
#   scripts/install-macos.sh            CLI + Aillies.app in /Applications
#   scripts/install-macos.sh --cli-only just the `aillies` command
set -euo pipefail
cd "$(dirname "$0")/.."

if ! xcode-select -p >/dev/null 2>&1; then
  xcode-select --install || true
  echo "Install the Command Line Tools in the dialog, then run this script again."
  exit 1
fi
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
if pgrep -fl 'rustup|rustup-init' >/dev/null 2>&1; then
  echo "Another rustup is running (it blocks the install). Quit it first:"; pgrep -fl 'rustup|rustup-init'
  echo "Then: pkill -f rustup"; exit 1
fi
if command -v rustup >/dev/null 2>&1; then
  rustup toolchain install stable --profile minimal
  rustup default stable
elif ! command -v cargo >/dev/null 2>&1; then
  echo "Installing Rust (rustup)…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  source "$HOME/.cargo/env"
fi

echo "Building the aillies command…"
cargo install --path crates/aillies-cli --locked --force
echo "✓ $(command -v aillies || echo "$HOME/.cargo/bin/aillies")"

if [ "${1:-}" = "--cli-only" ]; then
  echo "Next: aillies run   (then aillies invite / aillies pair in another terminal)"
  exit 0
fi

echo "Building Aillies.app (first build takes a few minutes)…"
cargo install tauri-cli --version '^2' --locked
(cd apps/desktop/src-tauri && cargo tauri build --bundles app)
APP="apps/desktop/src-tauri/target/release/bundle/macos/Aillies.app"
osascript -e 'quit app "Aillies"' >/dev/null 2>&1 || true
rm -rf "/Applications/Aillies.app"
ditto "$APP" "/Applications/Aillies.app"
open "/Applications/Aillies.app"
echo "✓ Aillies is in your menu bar. Use the app or 'aillies run', not both at once."
