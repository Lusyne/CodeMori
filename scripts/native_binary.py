"""Read native executable headers without running a foreign-platform binary."""
from pathlib import Path
import struct

VSCODE_TARGETS = {"macos-arm64": "darwin-arm64", "macos-x86_64": "darwin-x64",
                  "windows-x86_64": "win32-x64", "linux-arm64": "linux-arm64", "linux-x86_64": "linux-x64"}


def binary_target(path: Path) -> str:
    data = path.read_bytes()
    try:
        if data.startswith(b"MZ"):
            offset = struct.unpack_from("<I", data, 0x3C)[0]
            if data[offset:offset + 4] != b"PE\0\0":
                raise ValueError("Invalid PE signature")
            machine = struct.unpack_from("<H", data, offset + 4)[0]
            flags = struct.unpack_from("<H", data, offset + 22)[0]
            magic = struct.unpack_from("<H", data, offset + 24)[0]
            if magic != 0x20B or not flags & 2 or flags & 0x2000:
                raise ValueError("Expected a 64-bit PE executable, not a DLL")
            return "windows-" + {0x8664: "x86_64", 0xAA64: "arm64"}[machine]
        if data.startswith(b"\xcf\xfa\xed\xfe"):
            cpu, _, kind = struct.unpack_from("<III", data, 4)
            if kind != 2:
                raise ValueError("Expected a Mach-O executable")
            return "macos-" + {0x01000007: "x86_64", 0x0100000C: "arm64"}[cpu]
        if data.startswith(b"\x7fELF\x02\x01"):
            kind, machine = struct.unpack_from("<HH", data, 16)
            if kind not in (2, 3):
                raise ValueError("Expected an ELF executable/PIE")
            return "linux-" + {62: "x86_64", 183: "arm64"}[machine]
    except (struct.error, KeyError) as error:
        raise ValueError("Unsupported or truncated executable header") from error
    raise ValueError("Unsupported executable format")
