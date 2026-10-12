"""Release identity checks use synthetic manifests, never runtime data."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("release_version", Path(__file__).with_name("check-release-version.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ReleaseVersionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        desktop = self.root / "apps/desktop/src-tauri"
        desktop.mkdir(parents=True)
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion = "1.1.0"\n')
        (desktop / "Cargo.toml").write_text('[package]\nversion = "1.1.0"\n')
        (desktop / "tauri.conf.json").write_text(json.dumps({"version": "1.1.0"}))
        for directory in (self.root, desktop):
            (directory / "Cargo.lock").write_text('[[package]]\nname = "aillies-node"\nversion = "1.1.0"\n')

    def test_owner_tag_has_flat_asset_name(self):
        slug = module.validate(self.root, "codex/claude-stable-1.1")
        self.assertEqual(slug, "codex-claude-stable-1.1")
        self.assertEqual(Path(f"Aillies-{slug}-source.zip").name, f"Aillies-{slug}-source.zip")

    def test_standard_tag_and_unknown_owner_tag(self):
        self.assertEqual(module.validate(self.root, "v1.1.0"), "v1.1.0")
        for tag in ("codex/claude-stable-1.2", "codex-stable-1.0", "v1.0.0", "../../1.1.0"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                module.validate(self.root, tag)

    def test_mismatched_manifest_or_lock_refuses_release(self):
        for relative in ("apps/desktop/src-tauri/Cargo.toml", "Cargo.lock", "apps/desktop/src-tauri/Cargo.lock"):
            path = self.root / relative
            original = path.read_text()
            path.write_text(original.replace("1.1.0", "1.0.0"))
            with self.subTest(path=relative), self.assertRaises(ValueError):
                module.validate(self.root, "codex/claude-stable-1.1")
            path.write_text(original)


if __name__ == "__main__":
    unittest.main()
