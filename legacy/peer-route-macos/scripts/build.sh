#!/bin/bash
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"
swift build -c release
binary_dir="$(swift build -c release --show-bin-path)"
bundle="$project_dir/dist/Peer Route.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$binary_dir/PeerRoute" "$bundle/Contents/MacOS/PeerRoute"
if [ ! -x "$project_dir/vendor/cli-proxy-api" ]; then
  bash "$project_dir/scripts/fetch-core.sh"
fi
cp "$project_dir/vendor/cli-proxy-api" "$bundle/Contents/Resources/cli-proxy-api"
cp "$project_dir/experimental/tls_probe.py" "$bundle/Contents/Resources/tls_probe.py"
cp "$project_dir/experimental/requirements-lock.txt" "$bundle/Contents/Resources/tls-probe-requirements.txt"
cp "$project_dir/vendor/LICENSE" "$bundle/Contents/Resources/CLIProxyAPI-LICENSE.txt"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>PeerRoute</string>
<key>CFBundleIdentifier</key><string>io.peerroute.mac</string>
<key>CFBundleName</key><string>Peer Route</string>
<key>CFBundleDisplayName</key><string>Peer Route</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.8.3</string>
<key>CFBundleVersion</key><string>14</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSLocalNetworkUsageDescription</key><string>Connect to a trusted Mac over pinned TLS and investigate local ChatGPT connections.</string>
<key>NSBonjourServices</key><array><string>_peerroute._tcp</string></array>
<key>CFBundleDocumentTypes</key><array><dict>
<key>CFBundleTypeName</key><string>Peer Route pairing</string>
<key>CFBundleTypeRole</key><string>Viewer</string>
<key>LSHandlerRank</key><string>Owner</string>
<key>LSItemContentTypes</key><array><string>io.peerroute.relay-pairing</string></array>
</dict></array>
<key>UTExportedTypeDeclarations</key><array><dict>
<key>UTTypeIdentifier</key><string>io.peerroute.relay-pairing</string>
<key>UTTypeDescription</key><string>Peer Route private pairing</string>
<key>UTTypeConformsTo</key><array><string>public.data</string></array>
<key>UTTypeTagSpecification</key><dict><key>public.filename-extension</key><array><string>peerrelay</string></array></dict>
</dict></array>
</dict></plist>
PLIST
codesign --force --deep --sign "${PEER_ROUTE_SIGN_IDENTITY:--}" "$bundle"
codesign --verify --deep --strict "$bundle"
ditto -c -k --sequesterRsrc --keepParent "$bundle" "$project_dir/dist/Peer-Route-macOS.zip"
echo "Built: $bundle"
