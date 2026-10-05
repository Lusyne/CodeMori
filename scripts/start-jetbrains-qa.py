"""Launch an installed macOS IDEA bundle with an isolated profile and packaged CodeMori."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import socket
import sys
import subprocess
import tempfile
import zipfile

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--ide", type=Path, default=Path("/Applications/IntelliJ IDEA.app"))
parser.add_argument("--archive", type=Path, required=True)
parser.add_argument("--project", type=Path, required=True)
parser.add_argument("--data-dir", type=Path, required=True)
parser.add_argument("--performance-log", type=Path, help="Opt-in JSONL search-to-paint timings in this isolated profile")
parser.add_argument("--offline", action="store_true", help="Deny IP networking only for the spawned QA process tree")
args = parser.parse_args()
if os.environ.get("CODEMORI_ALLOW_IDE_TEST") != "1":
    raise SystemExit("Coordinate interactive acceptance before setting CODEMORI_ALLOW_IDE_TEST=1.")
if platform.system() != "Darwin":
    raise SystemExit("This native-bundle acceptance launcher currently targets macOS only.")
project = args.project.resolve(strict=True)
store = args.data_dir.resolve()
if not project.is_dir() or store == Path.home() / ".codemori":
    raise SystemExit("Use an existing test project and an isolated data directory.")
archive = args.archive.resolve(strict=True)
app = args.ide.resolve(strict=True) / "Contents"
defaults = (app / "bin/idea.vmoptions").read_text()
(root / "target").mkdir(exist_ok=True)
sandbox = Path(tempfile.mkdtemp(prefix="idea-native-qa-", dir=root / "target"))
for name in ["config", "system", "plugins", "logs"]:
    (sandbox / name).mkdir()
with zipfile.ZipFile(archive) as bundle:
    for item in bundle.infolist():
        destination = (sandbox / "plugins" / item.filename).resolve()
        if not destination.is_relative_to(sandbox / "plugins"):
            raise SystemExit("Plugin archive contains a path outside its staging directory.")
        bundle.extract(item, sandbox / "plugins")
        if not item.is_dir() and (item.external_attr >> 16) & 0o100:
            destination.chmod(0o755)
prefix = []
network_check = None
if args.offline:
    policy = sandbox / "offline.sb"
    policy.write_text("(version 1) (allow default) (deny network-outbound (remote ip)) (deny network-inbound (local ip))\n")
    prefix = ["/usr/bin/sandbox-exec", "-f", str(policy)]
    # A real listening endpoint distinguishes sandbox denial from connection refusal.
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0)); listener.listen()
        port = listener.getsockname()[1]
        with socket.create_connection(("127.0.0.1", port), timeout=2):
            pass
        probe = subprocess.run(prefix + [sys.executable, "-c",
            "import socket,errno,sys\ntry: socket.create_connection(('127.0.0.1',int(sys.argv[1])),2)\n"
            "except OSError as e: print(e.errno); sys.exit(0 if e.errno==errno.EPERM else 1)\n"
            "else: sys.exit(2)", str(port)], capture_output=True, text=True, check=True)
        network_check = {"control_connected": True, "sandbox_errno": int(probe.stdout.strip()),
                         "scope": "Probe uses the identical sandbox policy as the native IDEA process tree; not an in-IDE socket probe."}
options = sandbox / "idea.vmoptions"
properties = {"idea.config.path": sandbox / "config", "idea.system.path": sandbox / "system",
              "idea.plugins.path": sandbox / "plugins", "idea.log.path": sandbox / "logs",
              "codemori.testDataDir": store, "ide.show.tips.on.startup": "false", "ide.no.platform.update": "true"}
if args.performance_log:
    report = args.performance_log.resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    if report.exists():
        raise SystemExit("Use a new performance log path so measurements from different runs cannot mix.")
    properties["codemori.performanceLog"] = report
options.write_text(defaults.rstrip() + "\n" + "\n".join(f"-D{key}={value}" for key, value in properties.items()) + "\n")
evidence = {"project": str(project), "data_dir": str(store), "sandbox": str(sandbox),
            "archive": str(archive), "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
            "network_check": network_check,
            "scope": "Native installed IDEA launcher with isolated configuration, plugins and CodeMori store; no global application files modified."}
(sandbox / "launch.json").write_text(json.dumps(evidence, indent=2) + "\n")
print(json.dumps(evidence, ensure_ascii=False, indent=2), flush=True)
environment = dict(os.environ, IDEA_VM_OPTIONS=str(options))
environment.pop("IDEA_PROPERTIES", None)
environment.pop("JAVA_TOOL_OPTIONS", None)
environment["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
raise SystemExit(subprocess.call(prefix + [str(app / "MacOS/idea"), str(project)], env=environment))
