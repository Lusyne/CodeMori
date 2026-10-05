"""Check the host-specific JetBrains ZIP and exercise its bundled CLI."""
import json
import os
import platform
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import zipfile


def main():
    archive = Path(sys.argv[1])
    os_name = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}[platform.system()]
    arch = {"arm64": "arm64", "aarch64": "arm64", "AMD64": "x86_64", "x86_64": "x86_64"}[platform.machine()]
    executable = "codemori.exe" if os_name == "windows" else "codemori"
    suffix = f"/bin/{os_name}-{arch}/{executable}"
    with tempfile.TemporaryDirectory(prefix="codemori-package-") as directory:
        root = Path(directory)
        with zipfile.ZipFile(archive) as bundle:
            assert any(name.endswith("/LICENSE") for name in bundle.namelist())
            binaries = [info for info in bundle.infolist() if info.filename.endswith(suffix)]
            assert len(binaries) == 1, f"Expected one bundled CLI matching {suffix}"
            info = binaries[0]
            mode = info.external_attr >> 16
            if os_name != "windows":
                assert mode & stat.S_IXUSR, "Bundled CLI lost executable permissions"
            binary = root / executable
            binary.write_bytes(bundle.read(info))
            if os_name != "windows":
                os.chmod(binary, 0o755)
        data_dir = root / "中文 data"
        result = subprocess.run([str(binary), "info", "--data-dir", str(data_dir)],
                                check=True, capture_output=True, text=True, encoding="utf-8", timeout=10)
        body = json.loads(result.stdout)
        assert body["protocol_version"] == 1 and body["ok"] is True
        assert Path(body["data"]["data_dir"]) == data_dir
        assert not data_dir.exists(), "info must be read-only"
        assert not result.stderr, result.stderr
        print(f"Verified {archive.name}: executable host CLI, protocol 1, UTF-8 paths, no data writes")


if __name__ == "__main__":
    main()
