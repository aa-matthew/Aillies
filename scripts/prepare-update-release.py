#!/usr/bin/env python3
"""Build the static stable updater feed only after all platform artifacts exist."""
import base64
import hashlib
import json
from pathlib import Path
import re
import sys
from urllib.parse import quote


def prepare(root: Path, version: str, tag: str, notes: str = ""):
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version) or tag != "v" + version:
        raise ValueError("Updater release requires an exact version tag")
    targets = {
        "darwin-aarch64": f"Aillies_{version}_aarch64.app.tar.gz",
        "windows-x86_64": f"Aillies_{version}_x64-setup.exe",
        "linux-x86_64": f"Aillies_{version}_amd64.AppImage",
    }
    expected = {
        *(f"aillies-{tag}-{t}.tar.gz" for t in ["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"]),
        f"Aillies-{tag}-source.zip", f"Aillies_{version}_aarch64.dmg", f"Aillies_{version}_amd64.deb",
        *targets.values(), *(n + ".sig" for n in targets.values()),
    }
    files = [p for p in root.rglob("*") if p.is_file()]
    names = [p.name for p in files]
    if len(set(names)) != len(names) or set(names) != expected:
        raise ValueError("Missing, duplicate or unexpected release artifact")
    by_name = {p.name: p for p in files}
    platforms = {}
    for target, name in targets.items():
        if not by_name[name].stat().st_size:
            raise ValueError("Empty update package")
        signature = by_name[name + ".sig"].read_text().strip()
        decoded = base64.b64decode(signature, validate=True).decode("utf-8")
        trusted = next((line for line in decoded.splitlines() if line.startswith("trusted comment: ")), "")
        if "version:" + version not in trusted.split("\t"):
            raise ValueError("Update signature lacks the exact signed version")
        platforms[target] = {"url": f"https://github.com/aa-matthew/Aillies/releases/download/{quote(tag, safe='')}/{quote(name)}", "signature": signature}
    manifest = {"version": version, "notes": notes[:16000], "platforms": platforms}
    (root / "latest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    files.append(root / "latest.json")
    (root / "SHA256SUMS").write_text("".join(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in sorted(files, key=lambda p:p.name)))
    return manifest


if __name__ == "__main__":
    config = json.loads(Path("apps/desktop/src-tauri/tauri.conf.json").read_text())
    tag = sys.argv[1]
    notes_file = Path("docs/releases") / (tag + ".md")
    prepare(Path(sys.argv[2]), config["version"], tag, notes_file.read_text() if notes_file.exists() else "Aillies update")
