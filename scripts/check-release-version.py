"""Validate the release identity and return a flat artifact-name component."""
import json
from pathlib import Path
import re
import sys
import tomllib

OWNER_TAGS = {"codex-stable-1.0": "1.0.0", "codex/claude-stable-1.1": "1.1.0"}


def validate(root: Path, tag: str) -> str:
    version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    desktop = root / "apps/desktop/src-tauri"
    desktop_version = tomllib.loads((desktop / "Cargo.toml").read_text())["package"]["version"]
    config_version = json.loads((desktop / "tauri.conf.json").read_text())["version"]
    if not version == desktop_version == config_version:
        raise ValueError("Application versions must match")
    if tag != "v" + version and OWNER_TAGS.get(tag) != version:
        raise ValueError("Tag/version mismatch")
    slug = tag.replace("/", "-")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", slug):
        raise ValueError("Invalid artifact name")
    for directory in (root, desktop):
        packages = tomllib.loads((directory / "Cargo.lock").read_text())["package"]
        owned = [p for p in packages if p["name"].startswith("aillies-")]
        if not owned or any(p["version"] != version for p in owned):
            raise ValueError("Lockfile version mismatch")
    return slug


if __name__ == "__main__":
    print(validate(Path(__file__).resolve().parent.parent, sys.argv[1]))
