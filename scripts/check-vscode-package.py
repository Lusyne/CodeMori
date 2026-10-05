"""Validate the current host's VSIX and run its extracted native core."""
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import zipfile
import xml.etree.ElementTree as ET
from native_binary import VSCODE_TARGETS

archive = Path(sys.argv[1])
os_name = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}[platform.system()]
arch = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "x86_64", "AMD64": "x86_64"}[platform.machine()]
filename = "codemori.exe" if os_name == "windows" else "codemori"
with zipfile.ZipFile(archive) as bundle, tempfile.TemporaryDirectory(prefix="codemori-vsix-") as directory:
    manifest = json.loads(bundle.read("extension/package.json"))
    identity = ET.fromstring(bundle.read("extension.vsixmanifest")).find(".//{*}Identity")
    assert identity is not None and identity.get("TargetPlatform") == VSCODE_TARGETS[f"{os_name}-{arch}"]
    properties = ET.fromstring(bundle.read("extension.vsixmanifest")).findall(".//{*}Property")
    prerelease = any(p.get("Id") == "Microsoft.VisualStudio.Code.PreRelease" and p.get("Value") == "true" for p in properties)
    assert prerelease == (manifest["version"].startswith("0.") or "-" in manifest["version"]), "Wrong Marketplace channel"
    assert bundle.read("extension/LICENSE.txt")
    container = manifest["contributes"]["viewsContainers"]["activitybar"][0]
    assert container["id"] == "codemori" and bundle.read("extension/" + container["icon"])
    assert manifest["contributes"]["views"]["codemori"][0]["id"] == "codemori.library"
    for name in ["dist/extension.js", "dist/core.js", "media/app.js", "media/index.html", "media/app.css"]:
        assert "extension/" + name in bundle.namelist(), f"Missing {name}"
    assert not any("/node_modules/" in name or "/.vscode-test/" in name or name.startswith("extension/dist/test/") for name in bundle.namelist())
    info = bundle.getinfo(f"extension/bin/{os_name}-{arch}/{filename}")
    if os_name != "windows":
        assert (info.external_attr >> 16) & 0o100, "Executable mode missing"
    root = Path(directory)
    executable = root / filename
    executable.write_bytes(bundle.read(info))
    if os_name != "windows":
        os.chmod(executable, 0o755)
    store = root / "中文 data"
    output = subprocess.run([str(executable), "info", "--data-dir", str(store)], check=True, capture_output=True, text=True, encoding="utf-8", timeout=10)
    envelope = json.loads(output.stdout)
    assert envelope["protocol_version"] == 1 and envelope["ok"] is True
    assert envelope["data"]["version"] == manifest["version"]
    assert not store.exists()
    print(f"Verified {archive.name}: native mode/version, local webview assets, no runtime npm dependencies, read-only info")
