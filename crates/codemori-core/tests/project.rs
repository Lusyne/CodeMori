use codemori_core::{
    Error,
    rpc::{RpcEnvelope, execute},
    runtime_info,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn call(data: &Path, request: Value) -> Result<Value, Error> {
    if request["op"] == "project_save" || request["op"] == "project" {
        codemori_core::profile::set(data, "QA collaborator".into())?;
    }
    execute(
        &runtime_info(Some(data.to_path_buf()))?,
        serde_json::from_value::<RpcEnvelope>(json!({"protocol_version":1,"request":request}))
            .unwrap(),
    )
}
fn ok(data: &Path, request: Value) -> Value {
    call(data, request).unwrap()
}
fn project(root: &Path, name: &str) -> PathBuf {
    let p = root.join(name);
    fs::create_dir_all(p.join("src")).unwrap();
    fs::create_dir_all(p.join("docs")).unwrap();
    fs::write(p.join("src/main.rs"), "// shared note\nfn main() {}\n").unwrap();
    fs::write(p.join("docs/guide.md"), "# Project guide\n").unwrap();
    p
}
fn info(data: &Path, root: &Path) -> Value {
    ok(data, json!({"op":"project_info","root":root}))
}
fn save(data: &Path, root: &Path, version: &Value, input: Value, binding: Option<&str>) -> Value {
    ok(
        data,
        json!({"op":"project_save","root":root,"expected_version":version,"record":input,"binding_path":binding}),
    )
}
fn scoped(
    data: &Path,
    root: &Path,
    version: Option<&Value>,
    request: Value,
) -> Result<Value, Error> {
    call(
        data,
        json!({"op":"project","root":root,"expected_version":version,"request":request}),
    )
}
fn manifest(root: &Path) -> PathBuf {
    root.join(".codemori/shared.json")
}
fn snippet(title: &str) -> Value {
    json!({"kind":"snippet","title":title,"content":"needle();","tags":["team"]})
}

#[test]
fn shared_graph_survives_clone_without_private_data_or_absolute_roots() {
    let temp = tempfile::tempdir().unwrap();
    let a = project(temp.path(), "clone A");
    let b = project(temp.path(), "clone B");
    let private_a = temp.path().join("private A");
    let private_b = temp.path().join("private B");
    let status = info(&private_a, &a);
    assert_eq!(status["version"], "missing");
    assert!(!a.join(".codemori").exists());
    assert!(!private_a.exists());
    let personal = ok(
        &private_a,
        json!({"op":"record_create","record":{"kind":"snippet","title":"PRIVATE_SENTINEL","content":"secret","starred":true}}),
    );
    let mut input = snippet("Team code");
    input["source"] = json!({"workspace_id":"project","path":"src/main.rs","line":1});
    let code = save(&private_a, &a, &status["version"], input, None);
    let doc = save(
        &private_a,
        &a,
        &code["project_version"],
        json!({"kind":"document","title":"Guide","url":a.join("docs/guide.md")}),
        Some("src/main.rs"),
    );
    let bytes = fs::read(manifest(&a)).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(!text.contains("PRIVATE_SENTINEL"));
    assert!(!text.contains(temp.path().to_str().unwrap()));
    assert!(!text.contains("workspace_id"));
    assert!(!text.contains("starred"));
    assert!(!text.contains("is_demo"));
    assert!(text.contains("./docs/guide.md"));
    assert_eq!(
        ok(&private_a, json!({"op":"record_get","id":personal["id"]})),
        personal
    );
    assert_eq!(ok(&private_a, json!({"op":"search"}))["total"], 1);
    fs::create_dir_all(b.join(".codemori")).unwrap();
    fs::write(manifest(&b), bytes).unwrap();
    let status_b = info(&private_b, &b);
    assert_eq!(status_b["version"], doc["project_version"]);
    let result = scoped(&private_b, &b, None, json!({"op":"search"})).unwrap();
    assert_eq!(result["total"], 2);
    assert_eq!(result["items"][0]["record"]["scope"], "project");
    let target = scoped(&private_b,&b,None,json!({"op":"source_target","source":{"workspace_id":"project","path":"src/main.rs","line":1}})).unwrap();
    assert_eq!(
        PathBuf::from(target.as_str().unwrap()),
        b.join("src/main.rs").canonicalize().unwrap()
    );
    let linked = scoped(
        &private_b,
        &b,
        None,
        json!({"op":"file_documents","workspace_id":"project","path":"src/main.rs"}),
    )
    .unwrap();
    assert_eq!(linked[0]["id"], doc["id"]);
    assert_eq!(linked[0]["input"]["url"], "./docs/guide.md");
    let preview = scoped(
        &private_b,
        &b,
        None,
        json!({"op":"markdown_preview","id":doc["id"],"revision":doc["revision"]}),
    )
    .unwrap();
    assert!(preview["html"].as_str().unwrap().contains("Project guide"));
    assert_eq!(ok(&private_b, json!({"op":"search"}))["total"], 0);
}

#[test]
fn stale_file_versions_and_failed_bindings_never_replace_the_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let root = project(temp.path(), "repo");
    let data = temp.path().join("data");
    let first = save(&data, &root, &json!("missing"), snippet("One"), None);
    let original = fs::read(manifest(&root)).unwrap();
    let failed = call(
        &data,
        json!({"op":"project_save","root":root,"expected_version":first["project_version"],"record":{"kind":"document","title":"Bad binding","url":"https://example.com/doc"},"binding_path":"../outside.rs"}),
    );
    assert!(failed.is_err());
    assert_eq!(fs::read(manifest(&root)).unwrap(), original);
    let mut branch: Value = serde_json::from_slice(&original).unwrap();
    branch["records"][0]["input"]["description"] =
        json!("Git checkout changed content without changing record revision");
    let changed = serde_json::to_vec_pretty(&branch).unwrap();
    fs::write(manifest(&root), &changed).unwrap();
    let stale = scoped(
        &data,
        &root,
        Some(&first["project_version"]),
        json!({"op":"record_delete","id":first["id"],"revision":first["revision"]}),
    );
    assert!(matches!(stale, Err(Error::ProjectConflict)));
    assert_eq!(fs::read(manifest(&root)).unwrap(), changed);
    assert!(matches!(
        scoped(
            &data,
            &root,
            Some(&first["project_version"]),
            json!({"op":"record_get","id":first["id"]})
        ),
        Err(Error::ProjectConflict)
    ));
}

#[test]
fn malformed_or_future_project_data_does_not_hide_personal_results_or_get_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let root = project(temp.path(), "repo");
    let data = temp.path().join("data");
    ok(
        &data,
        json!({"op":"record_create","record":snippet("Private")}),
    );
    fs::create_dir_all(root.join(".codemori")).unwrap();
    for bytes in [
        b"<<<<<<< HEAD\nconflict".as_slice(),
        b"{\"format_version\":4,\"records\":[],\"bindings\":[]}",
    ] {
        fs::write(manifest(&root), bytes).unwrap();
        let status = info(&data, &root);
        assert!(status["error"].is_string());
        let results = ok(
            &data,
            json!({"op":"library_search","root":root,"scope":"all"}),
        );
        assert_eq!(results["total"], 1);
        assert!(results["project"]["error"].is_string());
        assert!(call(&data,json!({"op":"project_save","root":root,"expected_version":status["version"],"record":snippet("No overwrite")})).is_err());
        assert_eq!(fs::read(manifest(&root)).unwrap(), bytes);
    }
}

#[test]
fn separate_clients_serialize_project_writes_and_report_one_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let root = project(temp.path(), "repo");
    let data = temp.path().join("data");
    let first = save(&data, &root, &json!("missing"), snippet("Initial"), None);
    let handles:Vec<_>=(0..2).map(|n| {
        let root=root.clone();let data=temp.path().join(format!("client-{n}"));let version=first["project_version"].clone();
        std::thread::spawn(move||call(&data,json!({"op":"project_save","root":root,"expected_version":version,"record":snippet(&format!("Writer{n}"))})))
    }).collect();
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|r| matches!(r, Err(Error::ProjectConflict)))
            .count(),
        1
    );
    assert_eq!(
        scoped(&data, &root, None, json!({"op":"search"})).unwrap()["total"],
        2
    );
}

#[test]
fn shared_scope_rejects_private_paths_favorites_and_foreign_capture_contexts() {
    let temp = tempfile::tempdir().unwrap();
    let root = project(temp.path(), "repo");
    let other = project(temp.path(), "other");
    let data = temp.path().join("data");
    let foreign = ok(&data, json!({"op":"workspace_register","root":other}));
    for input in [
        json!({"kind":"snippet","content":"x","starred":true}),
        json!({"kind":"snippet","content":"x","source":{"workspace_id":foreign["id"],"path":"src/main.rs"}}),
        json!({"kind":"snippet","content":"x","source":{"workspace_id":"project","path":"../secret.rs"}}),
        json!({"kind":"document","title":"Outside","url":other.join("docs/guide.md")}),
        json!({"kind":"document","title":"Outside","url":"./../secret.md"}),
        json!({"kind":"document","title":"Machine vault","url":"obsidian://open?path=/Users/someone/note.md"}),
    ] {
        assert!(
            call(
                &data,
                json!({"op":"project_save","root":root,"expected_version":"missing","record":input})
            )
            .is_err()
        );
        assert!(!manifest(&root).exists());
    }
    let doc = save(
        &data,
        &root,
        &json!("missing"),
        json!({"kind":"document","title":"Feishu","url":"https://tenant.feishu.cn/wiki/abc"}),
        None,
    );
    let targets = scoped(
        &data,
        &root,
        Some(&doc["project_version"]),
        json!({"op":"document_open_targets","id":doc["id"],"revision":doc["revision"]}),
    )
    .unwrap();
    assert!(
        targets["feishu_applink"]
            .as_str()
            .unwrap()
            .starts_with("feishu://")
    );
    assert!(scoped(&data, &root, None, json!({"op":"backup_export"})).is_err());
}

#[test]
fn combined_search_keeps_scope_identity_ranking_paging_and_private_favorites() {
    let temp = tempfile::tempdir().unwrap();
    let root = project(temp.path(), "repo");
    let data = temp.path().join("data");
    let private = ok(
        &data,
        json!({"op":"record_create","record":{"kind":"snippet","title":"Private","content":"needle body","tags":["team"],"starred":true}}),
    );
    let mut records = Vec::new();
    for n in 0..230 {
        records.push(json!({"id":format!("shared-{n:03}"),"revision":1,"created_at":0,"updated_at":n,"input":{"kind":"snippet","title":format!("Item{n}"),"content":"needle","tags":["team"]}}));
    }
    records.push(json!({"id":private["id"],"revision":1,"created_at":0,"updated_at":0,"input":{"kind":"snippet","title":"needle","content":"shared exact","tags":["team"]}}));
    fs::create_dir_all(root.join(".codemori")).unwrap();
    fs::write(
        manifest(&root),
        serde_json::to_vec(&json!({"format_version":1,"records":records,"bindings":[]})).unwrap(),
    )
    .unwrap();
    let page = ok(
        &data,
        json!({"op":"library_search","root":root,"filter":{"query":"needle"}}),
    );
    assert_eq!(page["total"], 232);
    assert_eq!(page["items"][0]["record"]["scope"], "project");
    assert_eq!(page["items"][0]["record"]["id"], private["id"]);
    let mut identities = std::collections::HashSet::new();
    for offset in (0..250).step_by(50) {
        let page = ok(
            &data,
            json!({"op":"library_search","root":root,"filter":{"query":"needle","offset":offset}}),
        );
        for hit in page["items"].as_array().unwrap() {
            assert!(identities.insert(format!(
                "{}:{}",
                hit["record"]["scope"], hit["record"]["id"]
            )));
        }
    }
    assert_eq!(identities.len(), 232);
    assert_eq!(
        ok(
            &data,
            json!({"op":"library_search","root":root,"scope":"personal"})
        )["total"],
        1
    );
    assert_eq!(
        ok(
            &data,
            json!({"op":"library_search","root":root,"scope":"project"})
        )["total"],
        231
    );
    assert_eq!(
        ok(
            &data,
            json!({"op":"library_search","root":root,"filter":{"starred":true}})
        )["total"],
        1
    );
    let workspace = ok(&data, json!({"op":"workspace_register","root":root}));
    assert_eq!(
        ok(
            &data,
            json!({"op":"library_search","root":root,"filter":{"workspace_id":workspace["id"]}})
        )["total"],
        231,
        "Project scope must include shared snippets without source locations"
    );
    let version = info(&data, &root)["version"].clone();
    scoped(
        &data,
        &root,
        Some(&version),
        json!({"op":"record_delete","id":private["id"],"revision":1}),
    )
    .unwrap();
    assert_eq!(
        ok(&data, json!({"op":"record_get","id":private["id"]})),
        private
    );
}

#[cfg(unix)]
#[test]
fn shared_paths_do_not_follow_repository_symlinks() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = project(temp.path(), "repo");
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let data = temp.path().join("data");
    symlink(&outside, root.join(".codemori")).unwrap();
    assert!(info(&data, &root)["error"].is_string());
    assert!(call(&data,json!({"op":"project_save","root":root,"expected_version":"missing","record":snippet("No write")})).is_err());
    assert!(!outside.join("shared.json").exists());
    fs::remove_file(root.join(".codemori")).unwrap();
    fs::write(outside.join("secret.md"), "private").unwrap();
    symlink(outside.join("secret.md"), root.join("docs/leak.md")).unwrap();
    assert!(call(&data,json!({"op":"project_save","root":root,"expected_version":"missing","record":{"kind":"document","title":"No read","url":"./docs/leak.md"}})).is_err());
    assert!(!manifest(&root).exists());
}
