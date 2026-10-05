use codemori_core::{
    Error, Store, code_link,
    model::*,
    profile,
    rpc::{RpcEnvelope, execute},
    runtime_info,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
fn doc(store: &mut Store) -> Record {
    store
        .create(
            serde_json::from_value(
                json!({"kind":"document","title":"Design","url":"https://example.com/design"}),
            )
            .unwrap(),
        )
        .unwrap()
}
fn binding(w: &Workspace, d: &Record, path: &str, kind: BindingKind) -> Binding {
    Binding {
        workspace_id: w.id.clone(),
        document_id: d.id.clone(),
        path: path.into(),
        kind,
        review: None,
    }
}
fn call(data: &Path, value: Value) -> Result<Value, Error> {
    execute(
        &runtime_info(Some(data.into()))?,
        serde_json::from_value::<RpcEnvelope>(json!({"protocol_version":1,"request":value}))
            .unwrap(),
    )
}
#[test]
fn module_inheritance_review_and_unlink_keep_the_real_binding_origin() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir_all(root.join("pay/sub")).unwrap();
    fs::create_dir_all(root.join("payments")).unwrap();
    fs::write(root.join("pay/sub/a.rs"), "fn main() {}\n").unwrap();
    let mut store = Store::open(&runtime_info(Some(tmp.path().join("data"))).unwrap()).unwrap();
    let w = store
        .register_workspace(root.to_str().unwrap(), None)
        .unwrap();
    let d = doc(&mut store);
    let module = store
        .link(binding(&w, &d, "pay", BindingKind::Module))
        .unwrap();
    let direct = store
        .link(binding(&w, &d, "pay/sub/a.rs", BindingKind::File))
        .unwrap();
    assert!(module.review.is_some());
    assert_eq!(
        store.file_documents(&w.id, "pay/sub/a.rs").unwrap().len(),
        1
    );
    assert!(
        store
            .file_documents(&w.id, "payments/a.rs")
            .unwrap()
            .is_empty()
    );
    let entries = store.context_entries(&w.id, "pay/sub/a.rs").unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .any(|e| e["binding"]["path"] == "pay" && e["inherited"] == true)
    );
    fs::write(root.join("pay/sub/a.rs"), "fn main() { changed(); }\n").unwrap();
    assert_eq!(store.review_state(&module)["status"], "needs_review");
    assert_eq!(store.review_state(&direct)["status"], "needs_review");
    store.unlink(module).unwrap();
    assert_eq!(
        store.context_entries(&w.id, "pay/sub/a.rs").unwrap().len(),
        1
    );
    assert!(
        store
            .file_documents(&w.id, "pay/sub/other.rs")
            .unwrap()
            .is_empty()
    );
}
#[test]
fn confirming_review_checks_both_code_and_observed_association() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.rs"), "a\n").unwrap();
    let mut store = Store::open(&runtime_info(Some(tmp.path().join("data"))).unwrap()).unwrap();
    let w = store
        .register_workspace(tmp.path().to_str().unwrap(), None)
        .unwrap();
    let d = doc(&mut store);
    let b = store
        .link(binding(&w, &d, "a.rs", BindingKind::File))
        .unwrap();
    fs::write(tmp.path().join("a.rs"), "b\n").unwrap();
    let observed = store.review_state(&b)["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    fs::write(tmp.path().join("a.rs"), "c\n").unwrap();
    assert!(matches!(
        store.confirm_review(b.clone(), &observed, d.revision),
        Err(Error::BindingConflict)
    ));
    let fingerprint = store.review_state(&b)["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        store
            .confirm_review(b.clone(), &fingerprint, d.revision + 1)
            .is_err()
    );
    let confirmed = store
        .confirm_review(b.clone(), &fingerprint, d.revision)
        .unwrap();
    assert_eq!(store.review_state(&confirmed)["status"], "current");
    assert!(store.confirm_review(b, &fingerprint, d.revision).is_err());
}
#[test]
fn backup2_roundtrips_module_review_and_keeps_local_metadata_conflicts() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.py"), "print(1)\n").unwrap();
    let mut store = Store::open(&runtime_info(Some(tmp.path().join("private"))).unwrap()).unwrap();
    let w = store
        .register_workspace(tmp.path().to_str().unwrap(), None)
        .unwrap();
    let d = doc(&mut store);
    let b = store
        .link(binding(&w, &d, ".", BindingKind::Module))
        .unwrap();
    let backup = store.export_backup().unwrap();
    assert_eq!(backup.format_version, 2);
    assert_eq!(backup.bindings[0], b);
    let mut other = Store::open(&runtime_info(Some(tmp.path().join("other"))).unwrap()).unwrap();
    other.import_backup(backup.clone(), false).unwrap();
    assert_eq!(other.export_backup().unwrap().bindings, backup.bindings);
    let mut conflict = backup.clone();
    conflict.bindings[0].review = None;
    let report = other.import_backup(conflict, false).unwrap();
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(other.export_backup().unwrap().bindings, backup.bindings);
    let mut duplicate = backup;
    let mut second = b;
    second.review = None;
    duplicate.bindings.push(second);
    assert!(other.import_backup(duplicate, false).is_err());
}
#[test]
fn migration_from_schema3_preserves_legacy_binding_and_takes_online_backup() {
    let tmp = tempfile::tempdir().unwrap();
    let info = runtime_info(Some(tmp.path().join("data"))).unwrap();
    let mut store = Store::open(&info).unwrap();
    let w = store
        .register_workspace(tmp.path().to_str().unwrap(), None)
        .unwrap();
    let d = doc(&mut store);
    store
        .link(binding(&w, &d, "missing.rs", BindingKind::File))
        .unwrap();
    drop(store);
    let db = rusqlite::Connection::open(&info.database_path).unwrap();
    db.execute_batch("ALTER TABLE bindings DROP COLUMN kind; ALTER TABLE bindings DROP COLUMN review; PRAGMA user_version=3;").unwrap();
    drop(db);
    let store = Store::open(&info).unwrap();
    let b = &store.document_bindings(&d.id).unwrap()[0];
    assert_eq!(b.kind, BindingKind::File);
    assert_eq!(b.review, None);
    assert_eq!(store.file_documents(&w.id, "missing.rs").unwrap().len(), 1);
    let backup = fs::read_dir(&info.data_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("migration-v3-")
        })
        .unwrap();
    let db = rusqlite::Connection::open(backup).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        3
    );
}
#[test]
fn path_repair_previews_and_moves_subtree_bindings_and_sources_atomically() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir_all(root.join("old/sub")).unwrap();
    fs::write(root.join("old/sub/a.java"), "class A {}\n").unwrap();
    let mut store = Store::open(&runtime_info(Some(tmp.path().join("data"))).unwrap()).unwrap();
    let w = store
        .register_workspace(root.to_str().unwrap(), None)
        .unwrap();
    let d = doc(&mut store);
    store
        .link(binding(&w, &d, "old", BindingKind::Module))
        .unwrap();
    store
        .link(binding(&w, &d, "old/sub/a.java", BindingKind::File))
        .unwrap();
    let code=store.create(serde_json::from_value(json!({"kind":"snippet","content":"code","source":{"workspace_id":w.id,"path":"old/sub/a.java","line":1}})).unwrap()).unwrap();
    fs::rename(root.join("old"), root.join("new")).unwrap();
    let plan = store.repair_paths(&w.id, "old", "new", None).unwrap();
    assert_eq!(plan["bindings"], 2);
    assert_eq!(plan["records"], 1);
    assert_eq!(
        store.get(&code.id).unwrap().input.source.unwrap().path,
        "old/sub/a.java"
    );
    assert!(
        store
            .repair_paths(&w.id, "old", "new", Some("stale"))
            .is_err()
    );
    store
        .repair_paths(&w.id, "old", "new", plan["token"].as_str())
        .unwrap();
    assert_eq!(
        store.get(&code.id).unwrap().input.source.unwrap().path,
        "new/sub/a.java"
    );
    assert_eq!(
        store
            .context_entries(&w.id, "new/sub/a.java")
            .unwrap()
            .len(),
        2
    );
    assert!(
        store
            .file_documents(&w.id, "old/sub/a.java")
            .unwrap()
            .is_empty()
    );
}
#[test]
fn anchors_follow_insertions_modified_moves_and_refuse_ambiguous_copies() {
    let tmp = tempfile::tempdir().unwrap();
    let source = "class A {\n  int answer() {\n    return 42;\n  }\n}\n";
    fs::write(tmp.path().join("A.java"), source).unwrap();
    let links =
        code_link::create(tmp.path().to_str().unwrap(), "A.java", 3, "idea", "vscode").unwrap();
    let uri = links["vscode_url"].as_str().unwrap();
    fs::write(
        tmp.path().join("A.java"),
        format!("// heading\n// next\n{source}"),
    )
    .unwrap();
    let target = code_link::resolve(tmp.path().to_str().unwrap(), uri).unwrap();
    assert_eq!(target["line"], 5);
    assert_eq!(target["resolution"], "symbol");
    fs::rename(tmp.path().join("A.java"), tmp.path().join("Renamed.java")).unwrap();
    fs::write(tmp.path().join("Renamed.java"), source.replace("42", "43")).unwrap();
    let target = code_link::resolve(tmp.path().to_str().unwrap(), uri).unwrap();
    assert_eq!(target["line"], 2);
    assert_eq!(target["resolution"], "relocated");
    fs::copy(
        tmp.path().join("Renamed.java"),
        tmp.path().join("Duplicate.java"),
    )
    .unwrap();
    assert!(code_link::resolve(tmp.path().to_str().unwrap(), uri).is_err());
}
#[test]
fn shared_module_review_survives_clone_without_machine_roots() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    for root in [&a, &b] {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/a.ts"), "function test() {}\n").unwrap();
    }
    let data = tmp.path().join("private");
    profile::set(&data, "Alice".into()).unwrap();
    let saved=call(&data,json!({"op":"project_save","root":a,"expected_version":"missing","record":{"kind":"document","title":"Module design","url":"https://example.com/module"},"binding_path":"src","binding_kind":"module"})).unwrap();
    let text = fs::read_to_string(a.join(".codemori/shared.json")).unwrap();
    assert!(!text.contains(tmp.path().to_str().unwrap()));
    assert!(!text.contains("function test"));
    fs::create_dir(b.join(".codemori")).unwrap();
    fs::write(b.join(".codemori/shared.json"), text).unwrap();
    let w = call(&data, json!({"op":"workspace_register","root":b})).unwrap();
    let data = call(
        &data,
        json!({"op":"library_file_documents","root":b,"workspace_id":w["id"],"path":"src/a.ts"}),
    )
    .unwrap();
    assert_eq!(data["entries"][0]["binding"]["kind"], "module");
    assert_eq!(data["entries"][0]["review_state"]["status"], "current");
    assert_eq!(
        data["entries"][0]["binding"]["review"]["confirmed_by"]["display_name"],
        "Alice"
    );
    assert_eq!(data["records"][0]["id"], saved["id"]);
}

#[test]
fn git_diff_uses_a_clean_committed_baseline_and_ignores_untracked_baselines() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    let hooks = tmp.path().join("empty-hooks");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&hooks).unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args([
                "-c",
                "user.name=CodeMori QA",
                "-c",
                "user.email=qa@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
            ])
            .arg(format!("core.hooksPath={}", hooks.display()))
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--template=", "-b", "main"]);
    fs::write(root.join("a.py"), "print(1)\n").unwrap();
    fs::write(root.join(".gitignore"), "private.py\n").unwrap();
    git(&["add", "a.py", ".gitignore"]);
    git(&["commit", "-m", "fixture"]);
    let mut store = Store::open(&runtime_info(Some(tmp.path().join("data"))).unwrap()).unwrap();
    let w = store
        .register_workspace(root.to_str().unwrap(), None)
        .unwrap();
    let d = doc(&mut store);
    let b = store
        .link(binding(&w, &d, "a.py", BindingKind::File))
        .unwrap();
    assert!(b.review.as_ref().unwrap().git_commit.is_some());
    fs::write(root.join("a.py"), "print(2)\n").unwrap();
    let diff = store.binding_changes(&b).unwrap();
    assert!(diff["diff"].as_str().unwrap().contains("+print(2)"));
    assert_eq!(diff["review_state"]["status"], "needs_review");
    fs::write(root.join("private.py"), "secret()\n").unwrap();
    let hidden = store
        .link(binding(&w, &d, "private.py", BindingKind::File))
        .unwrap();
    assert!(hidden.review.unwrap().git_commit.is_none());
    let mut corrupted = serde_json::to_value(b.review.as_ref().unwrap()).unwrap();
    let forbidden = tmp.path().join("must-not-write");
    corrupted["git_commit"] = json!(format!("--output={}", forbidden.display()));
    let db = rusqlite::Connection::open(tmp.path().join("data/codemori.sqlite3")).unwrap();
    db.execute(
        "UPDATE bindings SET review=?1 WHERE path='a.py'",
        [serde_json::to_string(&corrupted).unwrap()],
    )
    .unwrap();
    assert!(store.binding_changes(&b).is_err());
    assert!(!forbidden.exists());
}
#[test]
fn symbol_anchors_cover_python_and_typescript() {
    let tmp = tempfile::tempdir().unwrap();
    for (file, source, line) in [
        ("example.py", "def answer():\n    return 42\n", 2),
        (
            "example.ts",
            "class A {\n  answer() {\n    return 42;\n  }\n}\n",
            3,
        ),
    ] {
        fs::write(tmp.path().join(file), source).unwrap();
        let links =
            code_link::create(tmp.path().to_str().unwrap(), file, line, "idea", "vscode").unwrap();
        fs::write(tmp.path().join(file), format!("\n\n{source}")).unwrap();
        let result = code_link::resolve(
            tmp.path().to_str().unwrap(),
            links["vscode_url"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(result["line"], line + 2);
        assert_eq!(result["resolution"], "symbol");
    }
}
#[test]
fn missing_review_targets_and_repair_collisions_never_report_success() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.rs"), "fn a() {}\n").unwrap();
    fs::write(tmp.path().join("b.rs"), "fn b() {}\n").unwrap();
    fs::create_dir(tmp.path().join("ignored")).unwrap();
    fs::write(tmp.path().join("ignored/private.rs"), "fn private() {}\n").unwrap();
    fs::write(tmp.path().join(".ignore"), "ignored/\n").unwrap();
    let mut store = Store::open(&runtime_info(Some(tmp.path().join("data"))).unwrap()).unwrap();
    let w = store
        .register_workspace(tmp.path().to_str().unwrap(), None)
        .unwrap();
    let d = doc(&mut store);
    let ignored = store
        .link(binding(&w, &d, "ignored", BindingKind::Module))
        .unwrap();
    assert_eq!(store.review_state(&ignored)["status"], "unavailable");
    store.unlink(ignored).unwrap();
    let a = store
        .link(binding(&w, &d, "a.rs", BindingKind::File))
        .unwrap();
    store
        .link(binding(&w, &d, "b.rs", BindingKind::File))
        .unwrap();
    assert!(store.repair_paths(&w.id, "a.rs", "b.rs", None).is_err());
    assert_eq!(store.document_bindings(&d.id).unwrap().len(), 2);
    fs::remove_file(tmp.path().join("a.rs")).unwrap();
    assert_eq!(store.review_state(&a)["status"], "unavailable");
    assert!(store.confirm_review(a, "ignored", d.revision).is_err());
}
