use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn rpc(dir: &Path, request: Value) -> (i32, Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_codemori"))
        .args(["rpc", "--data-dir"])
        .arg(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            serde_json::to_string(&json!({"protocol_version":1,"request":request}))
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.stderr.is_empty(), "{:?}", result);
    (
        result.status.code().unwrap(),
        serde_json::from_slice(&result.stdout).unwrap(),
    )
}
fn ok(dir: &Path, request: Value) -> Value {
    let (code, body) = rpc(dir, request);
    assert_eq!(code, 0, "{body}");
    assert_eq!(body["ok"], true);
    body["data"].clone()
}

#[test]
fn full_cli_workflow_roundtrips_across_processes() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("中文 storage");
    let workspace = ok(&dir, json!({"op":"workspace_register","root":root.path()}));
    let input = json!({"kind":"snippet","title":"Redis 重试","content":"retry(redis)","tags":["重试"],
        "source":{"workspace_id":workspace["id"],"path":"src/main.rs","line":1}});
    let snippet = ok(&dir, json!({"op":"record_create","record":input}));
    assert_eq!(snippet["input"]["language"], "rust");
    let document = ok(
        &dir,
        json!({"op":"record_create","record":{"kind":"document","title":"设计说明","url":"https://example.com/design","description":"Redis 重试需要幂等键"}}),
    );
    let binding =
        json!({"workspace_id":workspace["id"],"path":"src/main.rs","document_id":document["id"]});
    assert_eq!(
        ok(
            &dir,
            json!({"op":"document_target","id":document["id"],"revision":1})
        ),
        "https://example.com/design"
    );
    let (code, error) = rpc(
        &dir,
        json!({"op":"document_target","id":document["id"],"revision":2}),
    );
    assert_eq!(code, 1);
    assert_eq!(error["error"]["code"], "CONFLICT");
    ok(&dir, json!({"op":"document_link","binding":binding}));
    let results = ok(
        &dir,
        json!({"op":"search","filter":{"query":"REDIS 重试","workspace_id":workspace["id"]}}),
    );
    assert_eq!(results["total"], 2);
    let links = ok(
        &dir,
        json!({"op":"file_documents","workspace_id":workspace["id"],"path":"src/main.rs"}),
    );
    assert_eq!(links[0]["id"], document["id"]);
    let mut changed = snippet["input"].clone();
    changed["starred"] = json!(true);
    let updated = ok(
        &dir,
        json!({"op":"record_update","id":snippet["id"],"revision":1,"record":changed}),
    );
    let (code, conflict) = rpc(
        &dir,
        json!({"op":"record_update","id":snippet["id"],"revision":1,"record":changed}),
    );
    assert_eq!(code, 1);
    assert_eq!(conflict["error"]["code"], "CONFLICT");
    assert_eq!(updated["revision"], 2);
    let backup = ok(&dir, json!({"op":"backup_export"}));
    let restore_dir = root.path().join("restored");
    let preview = ok(&restore_dir, json!({"op":"backup_preview","backup":backup}));
    assert_eq!(preview["new_records"], 2);
    assert_eq!(ok(&restore_dir, json!({"op":"search"}))["total"], 0);
    ok(&restore_dir, json!({"op":"backup_import","backup":backup}));
    assert_eq!(ok(&restore_dir, json!({"op":"search"}))["total"], 2);
    assert_eq!(ok(&restore_dir, json!({"op":"tags"})), json!(["重试"]));
    ok(&dir, json!({"op":"document_unlink","binding":binding}));
    assert!(
        ok(
            &dir,
            json!({"op":"file_documents","workspace_id":workspace["id"],"path":"src/main.rs"})
        )
        .as_array()
        .unwrap()
        .is_empty()
    );
    ok(
        &dir,
        json!({"op":"record_delete","id":snippet["id"],"revision":2}),
    );
    let (_, missing) = rpc(&dir, json!({"op":"record_get","id":snippet["id"]}));
    assert_eq!(missing["error"]["code"], "NOT_FOUND");
}

#[test]
fn malformed_and_future_requests_are_rejected_before_storage_creation() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("never-created");
    for input in [
        r#"{"protocol_version":2,"request":{"op":"search"}}"#,
        r#"{"protocol_version":1,"request":{"op":"delete_all"}}"#,
        "not json",
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_codemori"))
            .args(["rpc", "--data-dir"])
            .arg(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(!result.status.success());
        let data: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(data["ok"], false);
        assert!(!dir.exists());
    }
}

#[test]
fn simultaneous_process_edits_have_one_winner() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("store");
    let record = ok(
        &dir,
        json!({"op":"record_create","record":{"kind":"snippet","title":"shared","content":"old"}}),
    );
    let handles: Vec<_> = (0..4)
        .map(|n| {
            let dir = dir.clone();
            let record = record.clone();
            std::thread::spawn(move || {
                let mut input = record["input"].clone();
                input["content"] = json!(format!("edit {n}"));
                rpc(
                    &dir,
                    json!({"op":"record_update","id":record["id"],"revision":1,"record":input}),
                )
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|(code, _)| *code == 0).count(), 1);
    for (_, result) in results.iter().filter(|(code, _)| *code != 0) {
        assert_eq!(result["error"]["code"], "CONFLICT");
    }
    assert_eq!(
        ok(&dir, json!({"op":"record_get","id":record["id"]}))["revision"],
        2
    );
}

#[test]
fn feishu_navigation_targets_roundtrip_without_changing_saved_links() {
    let root = tempfile::tempdir().unwrap();
    let record = ok(
        root.path(),
        json!({"op":"record_create","record":{
        "kind":"document","title":"Feishu","url":"https://tenant.feishu.cn/docx/abc?from=ide#anchor"}}),
    );
    let request =
        json!({"op":"document_open_targets","id":record["id"],"revision":record["revision"]});
    let targets = ok(root.path(), request);
    assert_eq!(targets["original_url"], record["input"]["url"]);
    assert_eq!(
        targets["feishu_applink"],
        "feishu://applink.feishu.cn/client/web_url/open?mode=window&url=https%3A%2F%2Ftenant.feishu.cn%2Fdocx%2Fabc%3Ffrom%3Dide%23anchor"
    );
    assert_eq!(
        ok(root.path(), json!({"op":"record_get","id":record["id"]})),
        record
    );
    let (code, error) = rpc(
        root.path(),
        json!({"op":"document_open_targets","id":record["id"],"revision":2}),
    );
    assert_eq!(code, 1);
    assert_eq!(error["error"]["code"], "CONFLICT");
}

#[test]
fn removed_demo_operations_fail_without_creating_storage() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("not-created");
    for operation in ["demo_install", "demo_remove"] {
        let (code, body) = rpc(&data, json!({"op":operation}));
        assert_ne!(code, 0);
        assert_eq!(body["ok"], false);
        assert!(!data.exists());
    }
}
