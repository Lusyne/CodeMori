"""Measure CLI search latency against 10,000 saved snapshots (~50 MiB content)."""
import json
import argparse
from contextlib import nullcontext
from pathlib import Path
import statistics
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", nargs="?", default="target/release/codemori")
parser.add_argument("--prepare", type=Path, help="Create a new fixture directory for IDE rendering measurements, without measuring the backend.")
args = parser.parse_args()
binary = Path(args.binary).resolve()
if args.prepare:
    args.prepare = args.prepare.resolve()
    args.prepare.mkdir(parents=True, exist_ok=False)

def call(root, request):
    payload = json.dumps({"protocol_version": 1, "request": request}, ensure_ascii=False).encode()
    start = time.perf_counter()
    result = subprocess.run([str(binary), "rpc", "--data-dir", str(root)], input=payload,
                            capture_output=True, check=True, timeout=120)
    body = json.loads(result.stdout)
    assert body["ok"], body
    return body["data"], (time.perf_counter() - start) * 1000

fixture = nullcontext(str(args.prepare)) if args.prepare else tempfile.TemporaryDirectory(prefix="codemori-benchmark-")
with fixture as temp:
    root = Path(temp) / "store"
    workspaces = [{"id": f"workspace-{number}", "name": f"Project {number}",
                   "root": str(Path(temp) / f"project-{number}"), "revision": 1}
                  for number in range(10)]
    for workspace in workspaces:
        Path(workspace["root"]).mkdir()
    records = []
    for number in range(10_000):
        code = ("// ordinary implementation line\n" * 180)[:5100]
        if number % 100 == 0:
            code += "\n// Redis 支付重试 超时"
        records.append({"id": f"benchmark-{number:05d}", "revision": 1,
                        "created_at": number, "updated_at": number, "is_demo": False,
                        "input": {"kind": "snippet", "title": f"Fixture {number}",
                                  "content": code, "language": "rust", "description": "",
                                  "tags": [], "starred": False, "source": {"workspace_id": f"workspace-{number % 10}", "path": f"src/fixture-{number}.rs", "line": 1}, "url": None}})
    text_bytes = sum(len(r["input"]["content"].encode()) for r in records)
    searchable_bytes = sum(len("\n".join([r["input"]["title"], r["input"]["content"], r["input"]["description"], *r["input"]["tags"]]).encode()) for r in records)
    assert searchable_bytes <= 50 * 1024 * 1024
    report, setup = call(root, {"op": "backup_import", "backup": {
        "format_version": 1, "exported_at": 0, "workspaces": workspaces, "records": records, "bindings": []}})
    assert report["new_records"] == 10_000
    if args.prepare:
        print(json.dumps({"records": len(records), "workspaces": len(workspaces), "content_bytes": text_bytes,
                          "searchable_text_bytes": searchable_bytes, "store": str(root),
                          "project": workspaces[0]["root"], "setup_ms": round(setup, 2)}))
        raise SystemExit(0)
    measurements = []
    for query, expected in [("redis 重试", 100), ("ordinary", 10000), ("not-present-anywhere", 0)]:
        request = {"op": "search", "filter": {"query": query, "limit": 50}}
        result, first = call(root, request)
        assert result["total"] == expected
        samples = []
        for _ in range(20):
            result, elapsed = call(root, request)
            assert result["total"] == expected
            samples.append(elapsed)
        samples.sort()
        measurements.append({"query": query, "matches": expected, "first_query_ms": round(first, 2),
                             "median_ms": round(statistics.median(samples), 2),
                             "p95_ms": round(samples[18], 2), "max_ms": round(samples[-1], 2)})
    print(json.dumps({"records": 10000, "workspaces": 10, "content_bytes": text_bytes, "searchable_text_bytes": searchable_bytes,
                      "setup_ms": round(setup, 2), "measurements": measurements,
                      "scope": "fresh CLI process + SQLite query + JSON decode; excludes IDE rendering; OS cache not flushed"}, indent=2))
