use codemori_core::{Error, Store, ai, model::*, runtime_info};
use serde_json::json;
use std::fs;
fn setup(root: &std::path::Path) -> (Store, Workspace, Record, Record) {
    let mut s = Store::open(&runtime_info(Some(root.join("data"))).unwrap()).unwrap();
    let w = s.register_workspace(root.to_str().unwrap(), None).unwrap();
    let a = s
        .create(
            serde_json::from_value(
                json!({"kind":"document","title":"A","url":"https://example.com/a"}),
            )
            .unwrap(),
        )
        .unwrap();
    let b = s
        .create(
            serde_json::from_value(
                json!({"kind":"document","title":"B","url":"https://example.com/b"}),
            )
            .unwrap(),
        )
        .unwrap();
    (s, w, a, b)
}
#[test]
fn batch_confirms_all_or_none_and_rejects_duplicate_or_stale_observations() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.rs"), "a\n").unwrap();
    fs::write(tmp.path().join("b.rs"), "b\n").unwrap();
    let (mut s, w, a, b) = setup(tmp.path());
    let ba = s
        .link(Binding {
            workspace_id: w.id.clone(),
            path: "a.rs".into(),
            document_id: a.id.clone(),
            ..Default::default()
        })
        .unwrap();
    let bb = s
        .link(Binding {
            workspace_id: w.id.clone(),
            path: "b.rs".into(),
            document_id: b.id.clone(),
            ..Default::default()
        })
        .unwrap();
    fs::write(tmp.path().join("a.rs"), "a changed\n").unwrap();
    fs::write(tmp.path().join("b.rs"), "b changed\n").unwrap();
    let make = |s: &Store, binding: Binding, revision| ReviewConfirmation {
        fingerprint: s.review_state(&binding)["fingerprint"]
            .as_str()
            .unwrap()
            .into(),
        binding,
        document_revision: revision,
    };
    let first = make(&s, ba.clone(), a.revision);
    let second = make(&s, bb.clone(), b.revision);
    assert!(
        s.confirm_reviews(vec![first.clone(), first.clone()])
            .is_err()
    );
    let mut stale = second.clone();
    stale.document_revision += 1;
    assert!(matches!(
        s.confirm_reviews(vec![first.clone(), stale]),
        Err(Error::BindingConflict)
    ));
    assert_eq!(s.document_bindings(&a.id).unwrap()[0], ba);
    fs::write(tmp.path().join("b.rs"), "changed again\n").unwrap();
    assert!(s.confirm_reviews(vec![first.clone(), second]).is_err());
    assert_eq!(s.document_bindings(&a.id).unwrap()[0], ba);
    let second = make(&s, bb, b.revision);
    let result = s.confirm_reviews(vec![first, second]).unwrap();
    assert_eq!(result.len(), 2);
    for binding in result {
        assert_eq!(s.review_state(&binding)["status"], "current");
    }
}
#[test]
fn ai_tools_search_ignored_files_and_ranges_without_overclaiming_scope() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("src/a.py"),
        "# 支付 retry\ndef retry():\n    return 42\n",
    )
    .unwrap();
    fs::write(tmp.path().join(".ignore"), "private.py\n").unwrap();
    fs::write(tmp.path().join("private.py"), "retry secret\n").unwrap();
    let result = ai::search(tmp.path().to_str().unwrap(), "RETRY", ".", 1).unwrap();
    assert_eq!(result["hits"].as_array().unwrap().len(), 1);
    assert_eq!(result["hits"][0]["path"], "src/a.py");
    assert_eq!(result["truncated"], true);
    let read = ai::read(tmp.path().to_str().unwrap(), "src/a.py", 2, Some(3)).unwrap();
    assert!(read["content"].as_str().unwrap().contains("return 42"));
    assert_eq!(read["start_line"], 2);
    assert!(ai::read(tmp.path().to_str().unwrap(), "../outside", 1, None).is_err());
    assert!(ai::read(tmp.path().to_str().unwrap(), "src/a.py", 1, Some(201)).is_err());
    let (mut s, w, a, _) = setup(tmp.path());
    s.link(Binding {
        workspace_id: w.id,
        path: "src/a.py".into(),
        document_id: a.id,
        ..Default::default()
    })
    .unwrap();
    let before = s.export_backup().unwrap();
    let context = ai::context(
        &mut s,
        tmp.path().to_str().unwrap(),
        "retry",
        Some("src/a.py"),
    )
    .unwrap();
    assert_eq!(context["external_document_bodies_loaded"], false);
    assert_eq!(context["review_confirmation_performed"], false);
    assert_eq!(
        context["associations"]["records"].as_array().unwrap().len(),
        1
    );
    assert_eq!(s.export_backup().unwrap().bindings, before.bindings);
    s.create(serde_json::from_value(json!({"kind":"snippet","title":"Huge retry","content":"retry ".repeat(90000),"source":{"workspace_id":before.workspaces[0].id,"path":"src/a.py","line":1}})).unwrap()).unwrap();
    assert!(ai::context(&mut s, tmp.path().to_str().unwrap(), "retry", None).is_err());
}
#[cfg(unix)]
#[test]
fn ai_source_tools_refuse_symlink_escapes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir(&root).unwrap();
    fs::write(tmp.path().join("secret.py"), "secret\n").unwrap();
    std::os::unix::fs::symlink(tmp.path().join("secret.py"), root.join("link.py")).unwrap();
    assert!(ai::read(root.to_str().unwrap(), "link.py", 1, None).is_err());
    assert!(
        ai::search(root.to_str().unwrap(), "secret", ".", 20).unwrap()["hits"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
