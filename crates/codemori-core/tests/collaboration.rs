use codemori_core::{
    Error, code_link, profile,
    rpc::{RpcEnvelope, execute},
    runtime_info,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
fn rpc(data: &Path, request: Value) -> Result<Value, Error> {
    execute(
        &runtime_info(Some(data.into()))?,
        serde_json::from_value::<RpcEnvelope>(json!({"protocol_version":1,"request":request}))
            .unwrap(),
    )
}
#[test]
fn identity_is_explicit_stable_and_shared_authors_survive_other_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("private");
    let other = tmp.path().join("other");
    let root = tmp.path().join("repo");
    fs::create_dir(&root).unwrap();
    assert_eq!(
        rpc(&data, json!({"op":"identity_get"})).unwrap()["author"],
        Value::Null
    );
    assert!(!data.exists());
    let save = json!({"op":"project_save","root":root,"expected_version":"missing","record":{"kind":"snippet","content":"a"}});
    assert!(matches!(
        rpc(&data, save.clone()),
        Err(Error::IdentityRequired)
    ));
    assert!(!root.join(".codemori/shared.json").exists());
    let a = profile::set(&data, "Alice".into()).unwrap();
    let b = profile::set(&other, "Bob".into()).unwrap();
    assert_eq!(
        profile::set(&data, "Alice renamed".into()).unwrap().id,
        a.id
    );
    let first = rpc(&data, save).unwrap();
    assert_eq!(first["created_by"]["id"], a.id);
    assert_eq!(first["updated_by"]["display_name"], "Alice renamed");
    let edited=rpc(&other,json!({"op":"project_save","root":root,"expected_version":first["project_version"],"id":first["id"],"revision":first["revision"],"record":{"kind":"snippet","content":"b"}})).unwrap();
    assert_eq!(edited["created_by"], first["created_by"]);
    assert_eq!(edited["updated_by"]["id"], b.id);
    rpc(&other,json!({"op":"project_save","root":root,"expected_version":edited["project_version"],"record":{"kind":"snippet","content":"unrelated"}})).unwrap();
    let found = rpc(
        &data,
        json!({"op":"project","root":root,"request":{"op":"record_get","id":first["id"]}}),
    )
    .unwrap();
    assert_eq!(found["updated_by"], edited["updated_by"]);
    let file: Value =
        serde_json::from_slice(&fs::read(root.join(".codemori/shared.json")).unwrap()).unwrap();
    assert_eq!(file["format_version"], 3);
    assert!(profile::set(&data, "\n".into()).is_err());
    assert!(profile::set(&data, "a\nb".into()).is_err());
}
#[test]
fn upgrading_legacy_records_keeps_unknown_creator_and_attributes_only_the_editor() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    let data = tmp.path().join("private");
    fs::create_dir_all(root.join(".codemori")).unwrap();
    let file = root.join(".codemori/shared.json");
    fs::write(&file,serde_json::to_vec(&json!({"format_version":1,"records":[{"id":"old","revision":1,"created_at":0,"updated_at":0,"input":{"kind":"snippet","content":"old"}}],"bindings":[]})).unwrap()).unwrap();
    let original = fs::read(&file).unwrap();
    let status = rpc(&data, json!({"op":"project_info","root":root})).unwrap();
    assert_eq!(fs::read(&file).unwrap(), original);
    profile::set(&data, "Editor".into()).unwrap();
    let result=rpc(&data,json!({"op":"project_save","root":root,"expected_version":status["version"],"id":"old","revision":1,"record":{"kind":"snippet","content":"new"}})).unwrap();
    assert_eq!(result["created_by"], Value::Null);
    assert_eq!(result["updated_by"]["display_name"], "Editor");
}
#[test]
fn code_links_resolve_across_clones_and_have_no_machine_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("clone a");
    let b = tmp.path().join("clone b");
    for root in [&a, &b] {
        fs::create_dir_all(root.join("中文 space")).unwrap();
        fs::write(root.join("中文 space/a+#%.rs"), "fn main() {}\n").unwrap();
    }
    let links = code_link::create(
        a.to_str().unwrap(),
        "中文 space/a+#%.rs",
        2,
        "idea",
        "vscode",
    )
    .unwrap();
    assert_eq!(links["created_project_identity"], true);
    fs::create_dir_all(b.join(".codemori")).unwrap();
    fs::copy(
        a.join(".codemori/project.json"),
        b.join(".codemori/project.json"),
    )
    .unwrap();
    for key in ["vscode_url", "jetbrains_url"] {
        let link = links[key].as_str().unwrap();
        assert!(!link.contains(tmp.path().to_str().unwrap()));
        let parsed = code_link::parse(link).unwrap();
        assert_eq!(parsed.path, "中文 space/a+#%.rs");
        let resolved = code_link::resolve(b.to_str().unwrap(), link).unwrap();
        assert_eq!(resolved["line"], 2);
        assert_eq!(
            Path::new(resolved["path"].as_str().unwrap()),
            b.canonicalize().unwrap().join("中文 space/a+#%.rs")
        );
    }
    assert_eq!(
        code_link::create(
            a.to_str().unwrap(),
            "中文 space/a+#%.rs",
            1,
            "goland",
            "vscode-insiders"
        )
        .unwrap()["project_id"],
        links["project_id"]
    );
    assert_eq!(
        code_link::resolve(
            tmp.path().to_str().unwrap(),
            links["vscode_url"].as_str().unwrap()
        )
        .unwrap(),
        Value::Null
    );
}
#[test]
fn code_links_reject_unsafe_uri_fields_paths_and_project_metadata() {
    let id = "550e8400-e29b-41d4-a716-446655440000";
    let valid = format!(
        "vscode://lusyne.codemori/open?project={id}&path_hex=7372632f6d61696e2e7273&line=1"
    );
    for bad in [
        valid.replace("&line=1", "&line=0"),
        valid.replace("&line=1", "&line=1&line=2"),
        valid.replace("7372632f6d61696e2e7273", "2e2e2f6f757473696465"),
        valid.replace("7372632f6d61696e2e7273", "2f6574632f706173737764"),
        valid.clone() + "&command=run",
        valid.clone() + "#fragment",
        valid.replace("lusyne.codemori", "attacker"),
        valid.replace("vscode:", "https:"),
    ] {
        assert!(code_link::parse(&bad).is_err(), "{bad}");
    }
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("main.rs"), "x").unwrap();
    fs::create_dir(tmp.path().join(".codemori")).unwrap();
    fs::write(tmp.path().join(".codemori/project.json"), "conflicted").unwrap();
    assert!(
        code_link::create(tmp.path().to_str().unwrap(), "main.rs", 1, "idea", "vscode").is_err()
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join(".codemori/project.json")).unwrap(),
        "conflicted"
    );
}
#[cfg(unix)]
#[test]
fn code_links_refuse_symbolic_links_even_when_the_file_is_inside_the_project() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("main.rs"), "x").unwrap();
    std::os::unix::fs::symlink(tmp.path().join("main.rs"), tmp.path().join("alias.rs")).unwrap();
    assert!(
        code_link::create(
            tmp.path().to_str().unwrap(),
            "alias.rs",
            1,
            "idea",
            "vscode"
        )
        .is_err()
    );
    assert!(!tmp.path().join(".codemori/project.json").exists());
}
