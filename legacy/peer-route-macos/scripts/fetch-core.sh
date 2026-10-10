#!/bin/bash
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
version="8.0.23"
case "$(uname -m)" in
  arm64) core_arch="aarch64" ;;
  x86_64) core_arch="amd64" ;;
  *) echo "Unsupported architecture" >&2; exit 1 ;;
esac
asset="CLIProxyAPI_${version}_darwin_${core_arch}.tar.gz"
mkdir -p "$project_dir/vendor"
curl -fL "https://github.com/router-for-me/CLIProxyAPI/releases/download/v${version}/${asset}" -o "$project_dir/vendor/core.tar.gz"
curl -fsSL "https://github.com/router-for-me/CLIProxyAPI/releases/download/v${version}/checksums.txt" -o "$project_dir/vendor/checksums.txt"
python3 - "$project_dir/vendor" "$asset" <<'PY'
import hashlib, pathlib, sys
directory = pathlib.Path(sys.argv[1])
expected = next(line.split()[0] for line in (directory/'checksums.txt').read_text().splitlines() if line.split()[-1].lstrip('*') == sys.argv[2])
actual = hashlib.sha256((directory/'core.tar.gz').read_bytes()).hexdigest()
if actual != expected:
    raise SystemExit('Proxy engine checksum mismatch')
print('Proxy engine release checksum verified')
PY
tar -xzf "$project_dir/vendor/core.tar.gz" -C "$project_dir/vendor"
