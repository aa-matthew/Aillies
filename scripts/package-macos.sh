#!/usr/bin/env bash
# Seal the full local app bundle, then verify the exact DMG payload. No installation.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/apps/desktop/src-tauri/target/release/bundle/macos/Aillies.app"
VERSION=$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$APP/Contents/Info.plist")
OUT="${1:-$ROOT/dist/macos/Aillies_${VERSION}_router-core-v4-pairing_aarch64.dmg}"
STAGE=$(mktemp -d "${TMPDIR:-/tmp}/aillies-package.XXXXXX")
MOUNT=$(mktemp -d "${TMPDIR:-/tmp}/aillies-verify.XXXXXX")
MOUNTED=0
cleanup() {
  if [ "$MOUNTED" = 1 ]; then hdiutil detach "$MOUNT" >/dev/null || true; fi
}
trap cleanup EXIT
[ ! -e "$OUT" ] || { echo 'Installer already exists. Keep the verified artifact or choose a new build name.' >&2; exit 1; }
mkdir -p "$ROOT/dist/macos"
ditto "$APP" "$STAGE/Aillies.app"
cp "$ROOT/THIRD_PARTY_NOTICES.md" "$STAGE/THIRD_PARTY_NOTICES.md"
ditto "$ROOT/licenses" "$STAGE/licenses"
codesign --force --deep --sign - "$STAGE/Aillies.app"
codesign --verify --deep --strict --verbose=2 "$STAGE/Aillies.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname Aillies -srcfolder "$STAGE" -fs HFS+ -format UDZO "$OUT"
hdiutil verify "$OUT"
hdiutil attach "$OUT" -readonly -nobrowse -mountpoint "$MOUNT" >/dev/null
MOUNTED=1
codesign --verify --deep --strict --verbose=2 "$MOUNT/Aillies.app"
cmp "$STAGE/Aillies.app/Contents/MacOS/Aillies" "$MOUNT/Aillies.app/Contents/MacOS/Aillies"
cmp "$ROOT/apps/desktop/src-tauri/icons/icon.icns" "$MOUNT/Aillies.app/Contents/Resources/icon.icns"
shasum -a 256 "$OUT" > "$OUT.sha256"
printf 'Verified installer: %s\n' "$OUT"
