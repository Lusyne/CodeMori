import hashlib
import importlib.util
import io
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile

from native_binary import VSCODE_TARGETS
from release import binary_name

spec = importlib.util.spec_from_file_location("marketplace", Path(__file__).with_name("package-marketplace.py"))
marketplace = importlib.util.module_from_spec(spec)
spec.loader.exec_module(marketplace)


def native_header(target):
    if target.startswith("macos-"):
        cpu = 0x0100000C if target.endswith("arm64") else 0x01000007
        return b"\xcf\xfa\xed\xfe" + struct.pack("<III", cpu, 0, 2)
    if target.startswith("linux-"):
        data = bytearray(64); data[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<HH", data, 16, 2, 183 if target.endswith("arm64") else 62)
        return bytes(data)
    data = bytearray(128); data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 64); data[64:68] = b"PE\0\0"
    struct.pack_into("<H", data, 68, 0x8664); struct.pack_into("<H", data, 86, 2); struct.pack_into("<H", data, 88, 0x20B)
    return bytes(data)


def save(path, data):
    path.write_bytes(data)
    path.with_name(path.name + ".sha256").write_text(f"{hashlib.sha256(data).hexdigest()}  {path.name}\n")


def seed(root, version="0.1.0", plugin_version="0.1.0"):
    for target in VSCODE_TARGETS:
        save(root / binary_name(version, target), native_header(target))
    jar = io.BytesIO()
    with zipfile.ZipFile(jar, "w") as bundle:
        bundle.writestr("META-INF/plugin.xml", f"<idea-plugin><id>com.lusyne.codemori</id><version>{plugin_version}</version></idea-plugin>")
        bundle.writestr("example.class", b"unchanged adapter")
    plugin = io.BytesIO()
    with zipfile.ZipFile(plugin, "w") as bundle:
        bundle.writestr("CodeMori/lib/plugin.jar", jar.getvalue())
        bundle.writestr("CodeMori/LICENSE", "MIT")
        bundle.writestr("CodeMori/bin/macos-arm64/codemori", native_header("macos-arm64"))
    save(root / f"codemori-{version}-macos-arm64.zip", plugin.getvalue())


class MarketplaceTests(unittest.TestCase):
    def test_universal_keeps_adapter_and_all_five_executable_targets(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); seed(root)
            output = marketplace.build_universal("0.1.0", root)
            with zipfile.ZipFile(output) as bundle, zipfile.ZipFile(root / "codemori-0.1.0-macos-arm64.zip") as original:
                self.assertEqual(bundle.read("CodeMori/lib/plugin.jar"), original.read("CodeMori/lib/plugin.jar"))
                cores = [n for n in bundle.namelist() if "/bin/" in n]
                self.assertEqual(len(cores), 5)
                for target in VSCODE_TARGETS:
                    name = "codemori.exe" if target.startswith("windows-") else "codemori"
                    item = bundle.getinfo(f"CodeMori/bin/{target}/{name}")
                    self.assertTrue(item.external_attr >> 16 & 0o100)
                    self.assertEqual(bundle.read(item), native_header(target))
            self.assertEqual(marketplace.checked_bytes(output), output.read_bytes())

    def test_missing_changed_or_wrong_architecture_core_is_rejected(self):
        for scenario in ["missing", "checksum", "architecture"]:
            with self.subTest(scenario=scenario), tempfile.TemporaryDirectory() as temp:
                root = Path(temp); seed(root)
                binary = root / binary_name("0.1.0", "linux-arm64")
                if scenario == "missing": binary.unlink()
                elif scenario == "checksum": binary.write_bytes(b"changed")
                else: save(binary, native_header("linux-x86_64"))
                with self.assertRaises((ValueError, FileNotFoundError)):
                    marketplace.build_universal("0.1.0", root)
                self.assertFalse((root / "codemori-0.1.0-jetbrains-universal.zip").exists())

    def test_adapter_version_and_canonical_core_must_match(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); seed(root, plugin_version="0.0.1")
            with self.assertRaisesRegex(ValueError, "identity/version"):
                marketplace.build_universal("0.1.0", root)
            seed(root)
            path = root / binary_name("0.1.0", "macos-arm64")
            save(path, native_header("macos-arm64") + b"different build")
            with self.assertRaisesRegex(ValueError, "differs"):
                marketplace.build_universal("0.1.0", root)


if __name__ == "__main__":
    unittest.main()
