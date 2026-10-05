"""Use the already-running local Docker daemon without host credential helpers."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    if not sys.argv[1:] or sys.argv[1] not in {"version", "image", "images", "inspect", "ps", "pull", "run", "build", "stop"}:
        raise SystemExit("Expected a local validation Docker command; login/auth operations are not supported")
    docker = shutil.which("docker")
    if docker is None:
        raise SystemExit("Docker CLI not available; this script does not install or launch Docker Desktop")
    if not Path("/var/run/docker.sock").exists():
        raise SystemExit("Local Docker socket unavailable; this script does not start a desktop application")
    config = ROOT / "target/docker-config-anonymous"
    config.mkdir(parents=True, exist_ok=True)
    # Docker auto-detects osxkeychain when ContainsAuth() is false. An explicitly
    # anonymous registry entry prevents that fallback; no host config is loaded.
    (config / "config.json").write_text(json.dumps({"auths": {"https://index.docker.io/v1/": {}}}) + "\n")
    clean_path = "/usr/bin:/bin:/usr/sbin:/sbin"
    for helper in ["osxkeychain", "desktop", "pass", "secretservice"]:
        if shutil.which("docker-credential-" + helper, path=clean_path):
            raise SystemExit("System PATH contains a credential helper; refusing this validation run")
    env = {**os.environ, "PATH": clean_path, "DOCKER_CONFIG": str(config)}
    for name in ["DOCKER_AUTH_CONFIG", "DOCKER_CONTEXT", "DOCKER_HOST"]:
        env.pop(name, None)
    result = subprocess.run([docker, "--config", str(config), "--host", "unix:///var/run/docker.sock", *sys.argv[1:]], env=env)
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
