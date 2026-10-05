"""Inspect a cross-built archive and its receipt without pretending to run the target OS."""
import hashlib
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import zipfile
import xml.etree.ElementTree as ET
from native_binary import VSCODE_TARGETS, binary_target

artifact = Path(sys.argv[1])
receipt = json.loads(artifact.with_name(artifact.name + ".build.json").read_text(encoding="utf-8"))
assert receipt["runtime_info_verified"] is False
assert hashlib.sha256(artifact.read_bytes()).hexdigest() == receipt["archive_sha256"]
executable = "codemori.exe" if receipt["target"].startswith("windows-") else "codemori"
if artifact.name.endswith(".tar.gz"):
    with tarfile.open(artifact) as bundle:
        names = bundle.getnames(); matches = [name for name in names if name.endswith("/" + executable)]
        assert len(matches) == 1
        content = bundle.extractfile(matches[0]).read()
        assert bundle.getmember(matches[0]).mode & 0o100
else:
    with zipfile.ZipFile(artifact) as bundle:
        names = bundle.namelist(); matches = [name for name in names if name.endswith("/" + executable)]
        assert len(matches) == 1
        content = bundle.read(matches[0])
        if artifact.suffix == ".vsix":
            manifest = json.loads(bundle.read("extension/package.json")); assert manifest["version"] == receipt["version"]
            identity = ET.fromstring(bundle.read("extension.vsixmanifest")).find(".//{*}Identity")
            assert identity is not None and identity.get("TargetPlatform") == VSCODE_TARGETS[receipt["target"]]
            assert matches[0].startswith(f'extension/bin/{receipt["target"]}/')
            assert "extension/dist/extension.js" in names
assert any(name.endswith(("/LICENSE", "/LICENSE.txt")) for name in names)
assert hashlib.sha256(content).hexdigest() == receipt["binary_sha256"]
with tempfile.TemporaryDirectory(prefix="codemori-cross-header-") as directory:
    binary = Path(directory) / executable; binary.write_bytes(content)
    assert binary_target(binary) == receipt["target"]
print(f"Inspected {artifact.name}: checksum, native header/architecture and license; target runtime NOT executed")
