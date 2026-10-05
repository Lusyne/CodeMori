"""Two isolated VS Code hosts sharing only a Git repository fixture."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
if os.environ.get("CODEMORI_ALLOW_IDE_TEST") != "1" and os.environ.get("CI") != "true":
    raise SystemExit("IDE test opt-in required")
for script in ["compile", "build:native"]:
    subprocess.run(["npm", "run", script], cwd=ROOT / "plugins/vscode", check=True)
parent = ROOT / "plugins/vscode/.vscode-test"
parent.mkdir(exist_ok=True)
qa = Path(tempfile.mkdtemp(prefix="project-sharing-", dir=parent))
origin = qa / "origin"
(origin / "docs").mkdir(parents=True)
(qa / "empty-hooks").mkdir()
(origin / "PaymentService.java").write_text("class PaymentService {\n  // 支付重试\n  void retry() {}\n}\n")
(origin / "src").mkdir()
(origin / "src/中文 +#%.rs").write_text("// header\nfn main() {}\n// footer\n")
(origin / "docs/guide.md").write_text("# Team guide\n")
subprocess.run(["git", "init", "-b", "main", "--template=" + str(qa / "empty-hooks"), str(origin)], check=True)
env = dict(os.environ, CODEMORI_PROJECT_SHARING_QA=str(qa))
print("Two-clone fixture: " + str(qa), flush=True)
for phase in ["write", "read"]:
    env["CODEMORI_PROJECT_SHARING_PHASE"] = phase
    subprocess.run(["node", "dist/test/run-host.js"], cwd=ROOT / "plugins/vscode", env=env, check=True)
    if phase == "write":
        args = ["git", "-C", str(origin), "-c", "core.hooksPath=" + str(qa / "empty-hooks"), "-c", "commit.gpgsign=false", "-c", "user.name=CodeMori QA", "-c", "user.email=qa@example.invalid"]
        subprocess.run(args + ["add", "PaymentService.java", "docs", "src", ".codemori/shared.json", ".codemori/project.json", ".codemori/.gitignore"], check=True)
        subprocess.run(args + ["commit", "-m", "Add shared project fixture"], check=True)
        subprocess.run(args + ["check-ignore", "--quiet", ".codemori/.shared.lock"], check=True)
        tracked = subprocess.check_output(args + ["ls-files"], text=True)
        assert ".shared.lock" not in tracked and "sqlite" not in tracked
        subprocess.run(["git", "-c", "protocol.file.allow=always", "clone", "--no-hardlinks", str(origin), str(qa / "clone")], check=True)
result = {phase: json.loads((qa / (phase + ".json")).read_text()) for phase in ["write", "read"]}
(qa / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(result, ensure_ascii=False, indent=2))
