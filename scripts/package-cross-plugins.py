"""Stage the shared adapters with an inspected cross-built core. No target runtime is launched."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import zipfile
import tomllib
import xml.etree.ElementTree as ET
from native_binary import VSCODE_TARGETS, binary_target

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {target: value for target, value in VSCODE_TARGETS.items() if target != "macos-arm64"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--jetbrains", type=Path, required=True, help="Verified host ZIP whose platform-neutral Java adapter is reused")
    parser.add_argument("--jetbrains-only", action="store_true", help="Keep an already-built target-native VSIX unchanged")
    args = parser.parse_args()
    if binary_target(args.binary) != args.target:
        raise RuntimeError("Cross-built executable header/architecture mismatch")
    source = ROOT / "plugins/vscode"
    manifest = json.loads((source / "package.json").read_text(encoding="utf-8"))
    version = manifest["version"]
    if version != tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]:
        raise RuntimeError("VS Code and Rust workspace versions differ")
    plugin_version = None
    with zipfile.ZipFile(args.jetbrains) as original:
        for name in original.namelist():
            if name.endswith(".jar"):
                with zipfile.ZipFile(io.BytesIO(original.read(name))) as jar:
                    if "META-INF/plugin.xml" in jar.namelist():
                        descriptor = ET.fromstring(jar.read("META-INF/plugin.xml"))
                        if descriptor.findtext("id") == "com.lusyne.codemori":
                            plugin_version = descriptor.findtext("version")
    if plugin_version != version:
        raise RuntimeError("JetBrains adapter version differs; rebuild the host ZIP before packaging")
    output = ROOT / "target/dist"
    output.mkdir(parents=True, exist_ok=True)
    stage = ROOT / "target/cross-plugin-stage" / args.target
    if stage.exists():
        shutil.rmtree(stage)  # Only this script's generated staging directory.
    stage.mkdir(parents=True)
    for name in ["package.json", "README.md", ".vscodeignore"]:
        shutil.copy2(source / name, stage / name)
    for name in ["media", "dist"]:
        shutil.copytree(source / name, stage / name, ignore=shutil.ignore_patterns("test", "*.map"))
    shutil.copy2(ROOT / "LICENSE", stage / "LICENSE")
    readme = stage / "README.md"
    readme.write_text(f"Cross-built preview for {args.target}. Full target-platform acceptance is pending; see compatibility notes for separate core execution evidence.\n\n" + readme.read_text(encoding="utf-8"), encoding="utf-8")
    executable = "codemori.exe" if args.target.startswith("windows-") else "codemori"
    native = stage / "bin" / args.target / executable
    native.parent.mkdir(parents=True)
    shutil.copy2(args.binary, native); native.chmod(0o755)
    for file in stage.rglob("*"):
        if file.is_file():
            os.utime(file, (315532800, 315532800))
    vsce = source / "node_modules/@vscode/vsce/vsce"
    vsix = output / f"codemori-{version}-{TARGETS[args.target]}.vsix"
    if not args.jetbrains_only:
        subprocess.run(["node", str(vsce), "package", *(["--pre-release"] if version.startswith("0.") or "-" in version else []), "--target", TARGETS[args.target], "--no-dependencies", "--allow-missing-repository", "--out", str(vsix)], cwd=stage, check=True)
    plugin = output / f"codemori-{version}-{args.target}.zip"
    with zipfile.ZipFile(args.jetbrains) as original, zipfile.ZipFile(plugin, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        native_names = [name for name in original.namelist() if "/bin/" in name and name.endswith(("/codemori", "/codemori.exe"))]
        if len(native_names) != 1:
            raise RuntimeError("Host plugin must contain exactly one native core")
        prefix = native_names[0].split("/bin/", 1)[0]
        for info in original.infolist():
            if not info.filename.startswith(prefix + "/bin/"):
                bundle.writestr(info, original.read(info))
        info = zipfile.ZipInfo(f"{prefix}/bin/{args.target}/{executable}", (1980, 1, 1, 0, 0, 0))
        info.external_attr = 0o100755 << 16; info.compress_type = zipfile.ZIP_DEFLATED
        bundle.writestr(info, args.binary.read_bytes())
        notice = zipfile.ZipInfo(f"{prefix}/CROSS_BUILD.txt", (1980, 1, 1, 0, 0, 0))
        notice.external_attr = 0o100644 << 16; notice.compress_type = zipfile.ZIP_DEFLATED
        bundle.writestr(notice, f"Cross-built preview for {args.target}. Full target-platform acceptance pending; see compatibility notes for separate core execution evidence.\n")
    binary_digest = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    for artifact in ([plugin] if args.jetbrains_only else [vsix, plugin]):
        digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
        artifact.with_name(artifact.name + ".sha256").write_text(f"{digest}  {artifact.name}\n")
        artifact.with_name(artifact.name + ".build.json").write_text(json.dumps({"target": args.target, "version": version,
            "binary_sha256": binary_digest, "archive_sha256": digest, "runtime_info_verified": False,
            "scope": "Package construction only; consult acceptance notes for subsequent runtime tests"}, indent=2) + "\n")
        print(f"Created cross-built preview: {artifact.name}; runtime unverified")


if __name__ == "__main__":
    main()
