"""从已校验的 Release 产物组装 JetBrains 通用插件，不执行上传。"""
import argparse
import hashlib
import io
from pathlib import Path, PurePosixPath
import xml.etree.ElementTree as ET
import zipfile

from native_binary import VSCODE_TARGETS, binary_target
from release import binary_name, version_for_tag


def checked_bytes(path):
    body = path.read_bytes()
    expected = f"{hashlib.sha256(body).hexdigest()}  {path.name}\n"
    if path.with_name(path.name + ".sha256").read_text() != expected:
        raise ValueError(f"Checksum mismatch: {path.name}")
    return body


def build_universal(version, directory):
    source = directory / f"codemori-{version}-macos-arm64.zip"
    original = checked_bytes(source)
    binaries = {}
    for target in VSCODE_TARGETS:
        path = directory / binary_name(version, target)
        binaries[target] = checked_bytes(path)
        if binary_target(path) != target:
            raise ValueError(f"Wrong binary architecture: {target}")
    with zipfile.ZipFile(io.BytesIO(original)) as bundle:
        names = bundle.namelist()
        if len(names) != len(set(names)):
            raise ValueError("Duplicate plugin archive entries")
        for name in names:
            path = PurePosixPath(name)
            if path.is_absolute() or ".." in path.parts or "\\" in name:
                raise ValueError("Unsafe plugin archive path")
        roots = {PurePosixPath(name).parts[0] for name in names}
        if len(roots) != 1:
            raise ValueError("Expected one plugin root directory")
        prefix = roots.pop()
        descriptors = []
        for name in names:
            if name.endswith(".jar"):
                with zipfile.ZipFile(io.BytesIO(bundle.read(name))) as jar:
                    if "META-INF/plugin.xml" in jar.namelist():
                        descriptors.append(ET.fromstring(jar.read("META-INF/plugin.xml")))
        if len(descriptors) != 1 or descriptors[0].findtext("id") != "com.lusyne.codemori" or descriptors[0].findtext("version") != version:
            raise ValueError("JetBrains plugin identity/version mismatch")
        native = f"{prefix}/bin/macos-arm64/codemori"
        if bundle.read(native) != binaries["macos-arm64"]:
            raise ValueError("Canonical plugin core differs from the release core")
        if not bundle.read(f"{prefix}/LICENSE"):
            raise ValueError("Missing project license")
        output = directory / f"codemori-{version}-jetbrains-universal.zip"
        with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as result:
            for name in names:
                if name.endswith("/") or name.startswith(prefix + "/bin/"):
                    continue
                info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
                info.external_attr = 0o100644 << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                result.writestr(info, bundle.read(name))
            for target, body in binaries.items():
                executable = "codemori.exe" if target.startswith("windows-") else "codemori"
                info = zipfile.ZipInfo(f"{prefix}/bin/{target}/{executable}", (1980, 1, 1, 0, 0, 0))
                info.external_attr = 0o100755 << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                result.writestr(info, body)
    line = f"{hashlib.sha256(output.read_bytes()).hexdigest()}  {output.name}\n"
    output.with_name(output.name + ".sha256").write_text(line)
    print(f"Created {output.name}: one Java adapter, five native cores")
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    version = version_for_tag(args.tag)
    # Release finalization must have succeeded before the additional market package.
    sums = args.directory / "SHA256SUMS"
    previous = sums.read_text()
    output = build_universal(version, args.directory)
    line = output.with_name(output.name + ".sha256").read_text()
    sums.write_text("".join(item + "\n" for item in previous.splitlines() if not item.endswith("  " + output.name)) + line)


if __name__ == "__main__":
    main()
