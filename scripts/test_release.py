import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import tomllib

from release import ROOT, VSCODE_TARGETS, archive_names, asset_names, binary_name, finalize, version_for_tag


class ReleaseTests(unittest.TestCase):
    def test_version_metadata_uses_utf8_under_a_legacy_windows_locale(self):
        version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for file in ["Cargo.toml", "Cargo.lock", "plugins/vscode/package.json", "plugins/vscode/package-lock.json", "plugins/jetbrains/build.gradle.kts"]:
                dest = root / file; dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes((ROOT / file).read_bytes())
            package = root / "plugins/vscode/package.json"
            metadata = json.loads(package.read_text(encoding="utf-8"))
            metadata["description"] = "配置检查"
            package.write_text(json.dumps(metadata, ensure_ascii=False), encoding="utf-8")
            original_read = Path.read_text
            def legacy_read(path, encoding=None, **kwargs):
                return original_read(path, encoding=encoding or "cp1252", **kwargs)
            with patch.object(Path, "read_text", legacy_read):
                with self.assertRaises(UnicodeDecodeError):
                    package.read_text()
                self.assertEqual(version_for_tag("v" + version, root), version)

    def test_tag_requires_consistent_package_versions(self):
        version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        self.assertEqual(version_for_tag("v" + version), version)
        for tag in ["main", "v99.0.0", "v0.1.0;echo", "v00.1.0"]:
            with self.assertRaises(ValueError):
                version_for_tag(tag)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for file in ["Cargo.toml", "Cargo.lock", "plugins/vscode/package.json", "plugins/vscode/package-lock.json", "plugins/jetbrains/build.gradle.kts"]:
                dest = root / file; dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes((ROOT / file).read_bytes())
            gradle = root / "plugins/jetbrains/build.gradle.kts"
            gradle.write_text(gradle.read_text().replace(f'version = "{version}"', 'version = "99.0.0"'))
            with self.assertRaises(ValueError):
                version_for_tag("v" + version, root)

    def test_incomplete_or_corrupt_matrix_cannot_publish(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with self.assertRaises(ValueError):
                finalize("0.1.0", root)
            for target in VSCODE_TARGETS:
                for name in asset_names("0.1.0", target):
                    body = name.encode(); (root / name).write_bytes(body)
                    (root / (name + ".sha256")).write_text(f"{hashlib.sha256(body).hexdigest()}  {name}\n")
            finalize("0.1.0", root)
            self.assertEqual(len((root / "SHA256SUMS").read_text().splitlines()), 21)
            standalone = root / binary_name("0.1.0", "windows-x86_64")
            original_binary = standalone.read_bytes()
            standalone.unlink()
            with self.assertRaises(ValueError):
                finalize("0.1.0", root)
            standalone.write_bytes(original_binary)
            self.assertEqual((root / "LICENSE").read_bytes(), (ROOT / "LICENSE").read_bytes())
            victim = root / archive_names("0.1.0", "linux-arm64")[0]
            original = victim.read_bytes(); victim.write_bytes(b"corrupted")
            with self.assertRaises(ValueError):
                finalize("0.1.0", root)
            victim.write_bytes(original)
            (root / "unexpected.zip").write_bytes(b"extra")
            with self.assertRaises(ValueError):
                finalize("0.1.0", root)


if __name__ == "__main__":
    unittest.main()
