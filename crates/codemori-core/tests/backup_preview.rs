use codemori_core::{Store, model::*, runtime_info};
use serde_json::json;

#[test]
fn preview_counts_structural_and_semantic_errors_at_original_positions_without_writes() {
    let root = tempfile::tempdir().unwrap();
    let mut source = Store::open(&runtime_info(Some(root.path().join("source"))).unwrap()).unwrap();
    let workspace = source
        .register_workspace(root.path().to_str().unwrap(), None)
        .unwrap();
    source
        .create(
            serde_json::from_value(json!({"kind":"snippet","title":"valid","content":"code"}))
                .unwrap(),
        )
        .unwrap();
    let backup = source.export_backup().unwrap();
    let mut raw = serde_json::to_value(&backup).unwrap();
    let good = raw["records"][0].clone();
    let mut empty = good.clone();
    empty["id"] = json!("empty");
    empty["input"]["content"] = json!("");
    let mut bad_revision = good.clone();
    bad_revision["id"] = json!("revision");
    bad_revision["revision"] = json!(0);
    raw["records"] =
        json!([{"id":"malformed","revision":"not a number"}, empty, bad_revision, good]);
    raw["bindings"] =
        json!([{"workspace_id":workspace.id,"document_id":"missing","path":"../outside"}]);
    let mut restored =
        Store::open(&runtime_info(Some(root.path().join("restored"))).unwrap()).unwrap();
    let report = restored.preview_backup(raw).unwrap();
    assert_eq!(report.invalid_count, 4);
    assert_eq!(report.new_records, 0);
    let mut positions: Vec<_> = report
        .invalid_entries
        .iter()
        .map(|entry| (entry.kind.as_str(), entry.index))
        .collect();
    positions.sort_unstable();
    assert_eq!(
        positions,
        vec![("binding", 0), ("record", 0), ("record", 1), ("record", 2)]
    );
    assert!(restored.export_backup().unwrap().records.is_empty());
    assert!(restored.workspaces().unwrap().is_empty());
    let mut invalid_import = backup.clone();
    invalid_import.records[0].input.content.clear();
    assert!(restored.import_backup(invalid_import, false).is_err());
    assert!(restored.export_backup().unwrap().records.is_empty());
    let valid = restored
        .preview_backup(serde_json::to_value(backup).unwrap())
        .unwrap();
    assert_eq!(valid.invalid_count, 0);
    assert!(valid.invalid_entries.is_empty());
    assert_eq!(valid.new_records, 1);
    assert_eq!(valid.new_workspaces, 1);
    assert!(restored.export_backup().unwrap().records.is_empty());
}

#[test]
fn invalid_preview_diagnostics_are_bounded_but_the_count_is_complete() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(&runtime_info(Some(root.path().join("store"))).unwrap()).unwrap();
    let bad_records: Vec<_> = (0..150)
        .map(|index| json!({"id":format!("bad-{index}")}))
        .collect();
    let report = store.preview_backup(json!({"format_version":1,"exported_at":0,"workspaces":[],"records":bad_records,"bindings":[]})).unwrap();
    assert_eq!(report.invalid_count, 150);
    assert_eq!(report.invalid_entries.len(), 100);
    let unsupported = Backup {
        format_version: 99,
        exported_at: -1,
        workspaces: vec![],
        records: vec![],
        bindings: vec![],
    };
    let report = store
        .preview_backup(serde_json::to_value(&unsupported).unwrap())
        .unwrap();
    assert_eq!(report.invalid_count, 1);
    assert_eq!(report.invalid_entries[0].kind, "backup");
    assert!(store.import_backup(unsupported, false).is_err());
}
