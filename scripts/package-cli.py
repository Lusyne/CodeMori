"""Package an executable for the running host with deterministic archive metadata."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import platform
import subprocess
import tarfile
import zipfile
import tomllib
from native_binary import binary_target

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release" / ("codemori.exe" if platform.system() == "Windows" else "codemori"))
    parser.add_argument("--output", type=Path, default=ROOT / "target/dist")
    parser.add_argument("--cross-target", choices=["windows-x86_64", "macos-x86_64"], help="Inspect headers only; do not claim runtime verification")
    args = parser.parse_args()
    os_name = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}[platform.system()]
    arch = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "x86_64", "AMD64": "x86_64"}[platform.machine()]
    expected = args.cross_target or f"{os_name}-{arch}"
    declared_version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    if binary_target(args.binary) != expected:
        raise RuntimeError(f"Executable architecture does not match {expected}")
    if args.cross_target:
        os_name, arch = expected.split("-", 1)
        version = declared_version
    else:
        runtime = json.loads(subprocess.check_output([str(args.binary.resolve()), "info"], text=True, encoding="utf-8"))
        if runtime.get("protocol_version") != 1 or not runtime.get("ok"):
            raise RuntimeError("Cannot identify native core")
        version = runtime["data"]["version"]
        if version != declared_version:
            raise RuntimeError("Native binary version differs from the workspace; rebuild before packaging")
    name = f"codemori-cli-{version}-{os_name}-{arch}"
    executable = "codemori.exe" if os_name == "windows" else "codemori"
    files = [(executable, args.binary, 0o755), ("LICENSE", ROOT / "LICENSE", 0o644),
             ("README.md", ROOT / "packaging/cli-README.md", 0o644)]
    skill = ROOT / "skills/codemori-knowledge"
    for resource in sorted(skill.rglob("*")):
        if resource.is_file() and "__pycache__" not in resource.parts and resource.suffix != ".pyc":
            files.append(("skills/codemori-knowledge/" + resource.relative_to(skill).as_posix(), resource, 0o644))
    args.output.mkdir(parents=True, exist_ok=True)
    output = args.output / (name + (".zip" if os_name == "windows" else ".tar.gz"))
    def contents(filename, source):
        body = source.read_bytes()
        if args.cross_target and filename == "README.md":
            body = f"Cross-built preview for {expected}. Full target-platform acceptance is pending; see compatibility notes for separate core execution evidence.\n\n".encode() + body
        return body
    if os_name == "windows":
        with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
            for filename, source, mode in files:
                member = zipfile.ZipInfo(f"{name}/{filename}", (1980, 1, 1, 0, 0, 0))
                member.external_attr = (0o100000 | mode) << 16
                member.compress_type = zipfile.ZIP_DEFLATED
                bundle.writestr(member, contents(filename, source))
    else:
        with output.open("wb") as stream, gzip.GzipFile(fileobj=stream, filename="", mode="wb", mtime=0) as compressed, tarfile.open(fileobj=compressed, mode="w") as bundle:
            for filename, source, mode in files:
                content = contents(filename, source)
                member = tarfile.TarInfo(f"{name}/{filename}")
                member.size = len(content); member.mode = mode; member.mtime = 0
                bundle.addfile(member, io.BytesIO(content))
    digest = hashlib.sha256(output.read_bytes()).hexdigest()
    output.with_name(output.name + ".sha256").write_text(f"{digest}  {output.name}\n")
    output.with_name(output.name + ".build.json").write_text(json.dumps({"target": expected, "version": version,
        "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(), "archive_sha256": digest,
        "runtime_info_verified": args.cross_target is None,
        "scope": "Package construction only; consult acceptance notes for subsequent runtime tests"}, indent=2) + "\n")
    print(f"Created {output.name}; sha256 {digest}")


if __name__ == "__main__":
    main()
