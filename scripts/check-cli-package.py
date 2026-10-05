"""Verify a CLI archive/checksum and exercise its binary without a toolchain on PATH."""
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from urllib.parse import urlparse, parse_qs

archive = Path(sys.argv[1])
expected = archive.with_name(archive.name + ".sha256").read_text().split()[0]
assert hashlib.sha256(archive.read_bytes()).hexdigest() == expected, "Archive checksum differs"
prefix = archive.name.removesuffix(".tar.gz").removesuffix(".zip")
filename = "codemori.exe" if platform.system() == "Windows" else "codemori"
if archive.suffix == ".zip":
    with zipfile.ZipFile(archive) as bundle:
        binary = bundle.read(f"{prefix}/{filename}")
        for name in ["LICENSE", "README.md", "skills/codemori-knowledge/SKILL.md", "skills/codemori-knowledge/scripts/codemori_tool.py", "skills/codemori-knowledge/scripts/install.py"]:
            assert bundle.read(f"{prefix}/{name}"), f"Missing {name}"
else:
    with tarfile.open(archive) as bundle:
        member = bundle.getmember(f"{prefix}/{filename}")
        assert member.mode & 0o100, "Missing native execute permission"
        binary = bundle.extractfile(member).read()
        for name in ["LICENSE", "README.md", "skills/codemori-knowledge/SKILL.md", "skills/codemori-knowledge/scripts/codemori_tool.py", "skills/codemori-knowledge/scripts/install.py"]:
            assert bundle.extractfile(f"{prefix}/{name}").read(), f"Missing {name}"
with tempfile.TemporaryDirectory(prefix="codemori-cli-archive-") as directory:
    root = Path(directory); executable = root / filename
    executable.write_bytes(binary); executable.chmod(0o755)
    store = root / "中文 data"
    env = {**os.environ, "PATH": ""}
    def run(command, request=None):
        data = None if request is None else json.dumps({"protocol_version": 1, "request": request}, ensure_ascii=False).encode()
        result = subprocess.run([str(executable), command, "--data-dir", str(store)], input=data,
                                capture_output=True, check=True, timeout=15, env=env)
        output = json.loads(result.stdout)
        assert output["protocol_version"] == 1 and output["ok"], output
        return output["data"]
    info = run("info"); assert not store.exists()
    saved = run("rpc", {"op": "record_create", "record": {"kind": "snippet", "title": "支付重试", "content": "retry();"}})
    found = run("rpc", {"op": "search", "filter": {"query": "重试"}})
    assert found["total"] == 1 and found["items"][0]["record"]["id"] == saved["id"]
    document = run("rpc", {"op": "record_create", "record": {"kind": "document", "title": "Feishu", "url": "https://tenant.feishu.cn/wiki/demo?x=a%26b#section"}})
    targets = run("rpc", {"op": "document_open_targets", "id": document["id"], "revision": document["revision"]})
    assert targets["original_url"] == document["input"]["url"]
    link = urlparse(targets["feishu_applink"])
    assert (link.scheme, link.netloc, link.path) == ("feishu", "applink.feishu.cn", "/client/web_url/open")
    assert parse_qs(link.query) == {"mode": ["window"], "url": [document["input"]["url"]]}
    run("rpc", {"op": "identity_set", "display_name": "Package QA"})
    project = root / "team-project"; project.mkdir()
    status = run("rpc", {"op": "project_info", "root": str(project)})
    shared = run("rpc", {"op": "project_save", "root": str(project), "expected_version": status["version"],
        "record": {"kind": "snippet", "title": "Team retry", "content": "shared_retry();"}})
    assert shared["scope"] == "project"
    assert shared["created_by"]["display_name"] == "Package QA"
    source = project / "中文 +#%.rs"; source.write_text("// head\nfn main() {}\n")
    links = run("rpc", {"op":"code_link_create","root":str(project),"path":source.name,"line":2})
    target = run("rpc", {"op":"code_link_resolve","root":str(project),"url":links["vscode_url"]})["target"]
    assert Path(target["path"]).samefile(source) and target["line"] == 2
    manifest = (project / ".codemori/shared.json").read_text()
    assert "workspace_id" not in manifest and "starred" not in manifest and str(root) not in manifest
    combined = run("rpc", {"op": "library_search", "root": str(project)})
    assert combined["total"] == 3 and run("rpc", {"op": "search"})["total"] == 2
    workspace = run("rpc", {"op":"workspace_register","root":str(project)})
    association = run("rpc", {"op":"document_link","binding":{"workspace_id":workspace["id"],"document_id":document["id"],"path":".","kind":"module"}})
    source.write_text("// changed\nfn main() { changed(); }\n")
    context = run("rpc", {"op":"library_file_documents","root":str(project),"workspace_id":workspace["id"],"path":source.name})
    entry = context["entries"][0]
    assert entry["inherited"] and entry["binding"]["path"] == "." and entry["review_state"]["status"] == "needs_review"
    run("rpc", {"op":"binding_review","binding":entry["binding"],"fingerprint":entry["review_state"]["fingerprint"],"document_revision":document["revision"]})
    assert run("rpc", {"op":"backup_export"})["format_version"] == 2 and info["schema_version"] == 4
    source.write_text("// final formatting\nfn main() { changed(); }\n")
    entry = run("rpc", {"op":"library_file_documents","root":str(project),"workspace_id":workspace["id"],"path":source.name})["entries"][0]
    batch = run("rpc", {"op":"bindings_review","items":[{"binding":entry["binding"],"fingerprint":entry["review_state"]["fingerprint"],"document_revision":document["revision"]}]})
    assert batch["confirmed"] == 1
    assert "changed" in run("rpc", {"op":"code_read","root":str(project),"path":source.name,"start_line":2,"end_line":2})["content"]
    assert run("rpc", {"op":"code_search","root":str(project),"query":"changed"})["hits"]
    assert prefix.startswith(f'codemori-cli-{info["version"]}-')
    print(f"Verified {archive.name}: checksum, license, readonly info and real save/search/Feishu/authors/anchors/module review/batch/AI tools with empty PATH")
