"""Validate tagged versions and stage checked release assets. Never publishes by itself."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib

from native_binary import VSCODE_TARGETS, binary_target

ROOT = Path(__file__).resolve().parents[1]


def version_for_tag(tag, root=ROOT):
    match = re.fullmatch(r"v((?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?)", tag)
    if not match:
        raise ValueError("Release tag must be v<major>.<minor>.<patch> with an optional prerelease suffix")
    version = match[1]
    cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    package = json.loads((root / "plugins/vscode/package.json").read_text(encoding="utf-8"))["version"]
    lock = json.loads((root / "plugins/vscode/package-lock.json").read_text(encoding="utf-8"))
    gradle = re.search(r'^version = "([^"]+)"', (root / "plugins/jetbrains/build.gradle.kts").read_text(encoding="utf-8"), re.M)
    if not gradle or any(v != version for v in [cargo, package, lock["version"], lock["packages"][""]["version"], gradle[1]]):
        raise ValueError("Tag, Cargo, VS Code package/lock and JetBrains versions must match")
    locked = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    if any(p["version"] != version for p in locked if p["name"] in ("codemori-core", "codemori-cli")):
        raise ValueError("Cargo.lock workspace versions differ")
    return version


def archive_names(version, target):
    cli_ext = "zip" if target.startswith("windows-") else "tar.gz"
    return [f"codemori-cli-{version}-{target}.{cli_ext}",
            f"codemori-{version}-{VSCODE_TARGETS[target]}.vsix",
            f"codemori-{version}-{target}.zip"]


def binary_name(version, target):
    suffix = ".exe" if target.startswith("windows-") else ""
    return f"codemori-cli-{version}-{target}{suffix}"


def asset_names(version, target):
    return archive_names(version, target) + [binary_name(version, target)]


def stage(version, target, output):
    binary = ROOT / "target/release" / ("codemori.exe" if target.startswith("windows-") else "codemori")
    if binary_target(binary) != target:
        raise ValueError("Runner architecture differs from the declared release target")
    names = archive_names(version, target)
    sources = [ROOT / "target/dist" / names[0], ROOT / "plugins/vscode" / names[1],
               ROOT / "plugins/jetbrains/build/distributions" / names[2]]
    checks = ["check-cli-package.py", "check-vscode-package.py", "check-plugin-package.py"]
    for source, check in zip(sources, checks):
        subprocess.run([sys.executable, str(ROOT / "scripts" / check), str(source)], check=True)
    output.mkdir(parents=True, exist_ok=True)
    for source, name in zip(sources + [binary], names + [binary_name(version, target)]):
        destination = output / name
        shutil.copy2(source, destination)
        digest = hashlib.sha256(destination.read_bytes()).hexdigest()
        destination.with_name(destination.name + ".sha256").write_text(f"{digest}  {destination.name}\n")
    print(f"Staged 3 verified archives and standalone CLI for {target}")


def finalize(version, directory):
    expected = sorted(name for target in VSCODE_TARGETS for name in asset_names(version, target))
    allowed = set(expected + [name + ".sha256" for name in expected])
    actual = {p.name for p in directory.iterdir()} - {"SHA256SUMS", "LICENSE"}
    if actual != allowed:
        raise ValueError(f"Release files differ: missing={sorted(allowed-actual)}, unexpected={sorted(actual-allowed)}")
    lines = []
    for name in expected:
        digest = hashlib.sha256((directory / name).read_bytes()).hexdigest()
        line = f"{digest}  {name}\n"
        if (directory / (name + ".sha256")).read_text() != line:
            raise ValueError(f"Checksum mismatch: {name}")
        lines.append(line)
    for name, source in [("LICENSE", ROOT / "LICENSE")]:
        destination = directory / name
        shutil.copy2(source, destination)
        lines.append(f"{hashlib.sha256(destination.read_bytes()).hexdigest()}  {name}\n")
    (directory / "SHA256SUMS").write_text("".join(lines))
    print("Verified complete release: 15 archives + 5 standalone CLIs + licenses + SHA256SUMS")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["version", "stage", "finalize"])
    parser.add_argument("--tag", required=True)
    parser.add_argument("--target", choices=VSCODE_TARGETS)
    parser.add_argument("--directory", type=Path, default=ROOT / "target/release-assets")
    args = parser.parse_args()
    version = version_for_tag(args.tag)
    if args.action == "stage":
        if not args.target:
            parser.error("stage requires --target")
        stage(version, args.target, args.directory)
    elif args.action == "finalize":
        finalize(version, args.directory)
    else:
        print(version)


if __name__ == "__main__":
    main()
