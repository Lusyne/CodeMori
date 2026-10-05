use codemori_core::{Error, SCHEMA_VERSION, Store, model::*, runtime_info};
use rusqlite::Connection;

fn snippet(title: &str, code: &str) -> RecordInput {
    RecordInput {
        kind: RecordKind::Snippet,
        title: title.into(),
        content: code.into(),
        language: "rust".into(),
        description: String::new(),
        tags: vec![],
        starred: false,
        source: None,
        url: None,
    }
}
fn document(title: &str, url: &str) -> RecordInput {
    RecordInput {
        kind: RecordKind::Document,
        title: title.into(),
        content: String::new(),
        language: String::new(),
        description: String::new(),
        tags: vec![],
        starred: false,
        source: None,
        url: Some(url.into()),
    }
}
fn open(root: &std::path::Path) -> Store {
    Store::open(&runtime_info(Some(root.join("store"))).unwrap()).unwrap()
}

#[test]
fn snapshots_remain_unchanged_and_stale_edits_fail() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    let file = root.path().join("main.rs");
    std::fs::write(&file, "original").unwrap();
    let workspace = store
        .register_workspace(root.path().to_str().unwrap(), None)
        .unwrap();
    let mut input = snippet("快照", "original");
    input.source = Some(Source {
        workspace_id: workspace.id.clone(),
        path: "main.rs".into(),
        line: Some(1),
    });
    input.tags = vec![
        " 性能 ".into(),
        "性能".into(),
        "Redis".into(),
        "redis".into(),
        "".into(),
    ];
    let record = store.create(input).unwrap();
    std::fs::write(&file, "changed externally").unwrap();
    assert_eq!(store.get(&record.id).unwrap().input.content, "original");
    assert_eq!(record.input.tags, vec!["性能", "Redis"]);
    let mut change = record.input.clone();
    change.starred = true;
    let updated = store
        .update(&record.id, record.revision, change.clone())
        .unwrap();
    assert_eq!(updated.revision, 2);
    assert!(matches!(
        store.update(&record.id, 1, change),
        Err(Error::Conflict {
            current_revision: 2
        })
    ));
    assert!(matches!(
        store.delete(&record.id, 1),
        Err(Error::Conflict { .. })
    ));
    assert!(store.get(&record.id).unwrap().input.starred);
    store.delete(&record.id, 2).unwrap();
    assert!(matches!(store.get(&record.id), Err(Error::NotFound(_))));
}

#[test]
fn bindings_are_project_specific_reusable_and_repairable() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    let first = root.path().join("one");
    let second = root.path().join("two");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(first.join("main.rs"), "one").unwrap();
    std::fs::write(second.join("main.rs"), "two").unwrap();
    let one = store
        .register_workspace(first.to_str().unwrap(), Some("One".into()))
        .unwrap();
    let two = store
        .register_workspace(second.to_str().unwrap(), None)
        .unwrap();
    assert_eq!(
        store
            .register_workspace(first.to_str().unwrap(), None)
            .unwrap()
            .id,
        one.id
    );
    let doc = store
        .create(document("说明", "https://example.com/design"))
        .unwrap();
    let duplicate = store
        .create(document("不要覆盖原摘要", "https://EXAMPLE.com/design"))
        .unwrap();
    assert_eq!(duplicate.id, doc.id);
    assert_eq!(duplicate.input.title, "说明");
    let binding = Binding {
        workspace_id: one.id.clone(),
        path: "main.rs".into(),
        document_id: doc.id.clone(),
        kind: BindingKind::File,
        review: None,
    };
    store.link(binding.clone()).unwrap();
    store.link(binding.clone()).unwrap();
    assert_eq!(store.file_documents(&one.id, "main.rs").unwrap().len(), 1);
    assert!(store.file_documents(&two.id, "main.rs").unwrap().is_empty());
    let moved = root.path().join("moved");
    std::fs::rename(first, &moved).unwrap();
    let repaired = store
        .relocate_workspace(&one.id, moved.to_str().unwrap(), one.revision)
        .unwrap();
    assert_eq!(repaired.id, one.id);
    let target = store
        .source_target(&Source {
            workspace_id: one.id.clone(),
            path: "main.rs".into(),
            line: None,
        })
        .unwrap();
    assert_eq!(std::fs::read_to_string(target).unwrap(), "one");
    store.unlink(binding).unwrap();
    assert!(store.file_documents(&one.id, "main.rs").unwrap().is_empty());
    assert!(store.get(&doc.id).is_ok());
}

#[test]
fn search_supports_short_chinese_and_mixed_and_queries_with_ranking() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    let a = store.create(snippet("支付重试", "redis retry")).unwrap();
    let mut input = snippet("别的实现", "REDIS 的连接 超时");
    input.tags = vec!["性能".into()];
    input.starred = true;
    let b = store.create(input).unwrap();
    store.create(snippet("无关", "Redis only")).unwrap();
    let mut doc = document("设计说明", "https://notion.so/abc");
    doc.description = "支付重试与 Redis 超时".into();
    let d = store.create(doc).unwrap();
    let result = store
        .search(SearchFilter {
            query: "重试".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(result.total, 2);
    assert_eq!(result.items[0].record.id, a.id);
    assert!(
        result.items[0]
            .excerpt
            .iter()
            .any(|s| s.highlight && s.text == "重试")
    );
    let result = store
        .search(SearchFilter {
            query: "redis 超时".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(result.total, 2);
    assert!(result.items.iter().any(|h| h.record.id == b.id));
    assert!(result.items.iter().any(|h| h.record.id == d.id));
    assert_eq!(
        store
            .search(SearchFilter {
                tag: Some("性能".into()),
                starred: true,
                ..Default::default()
            })
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        store
            .search(SearchFilter {
                query: "从未存储的正文".into(),
                ..Default::default()
            })
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        store
            .search(SearchFilter {
                query: "%".into(),
                ..Default::default()
            })
            .unwrap()
            .total,
        0
    );
    let page = store
        .search(SearchFilter {
            limit: 1,
            offset: 1,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.total, 4);
}

#[test]
fn unicode_highlights_keep_original_text_and_do_not_split_codepoints() {
    let spans =
        codemori_core::search::excerpt("Emoji🙂İSTANBUL 中文重试", &["i".into(), "重试".into()]);
    let joined: String = spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(joined, "Emoji🙂İSTANBUL 中文重试");
    assert!(spans.iter().any(|s| s.highlight && s.text == "İ"));
    assert!(spans.iter().any(|s| s.highlight && s.text == "重试"));
}

#[test]
fn project_filter_includes_bound_documents_and_only_its_own_snapshots() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    let w = store
        .register_workspace(root.path().to_str().unwrap(), None)
        .unwrap();
    let mut snap = snippet("local", "code");
    snap.source = Some(Source {
        workspace_id: w.id.clone(),
        path: "one.rs".into(),
        line: None,
    });
    store.create(snap).unwrap();
    store.create(snippet("global", "code")).unwrap();
    let d = store
        .create(document("doc", "https://example.com"))
        .unwrap();
    store
        .link(Binding {
            workspace_id: w.id.clone(),
            path: "one.rs".into(),
            document_id: d.id,
            kind: BindingKind::File,
            review: None,
        })
        .unwrap();
    assert_eq!(
        store
            .search(SearchFilter {
                workspace_id: Some(w.id),
                ..Default::default()
            })
            .unwrap()
            .total,
        2
    );
}

#[test]
fn backup_roundtrip_preview_is_readonly_and_conflicts_preserve_local() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    let w = store
        .register_workspace(root.path().to_str().unwrap(), None)
        .unwrap();
    let a = store.create(snippet("保存", "原文")).unwrap();
    let d = store
        .create(document("文档", "https://example.com/a"))
        .unwrap();
    store
        .link(Binding {
            workspace_id: w.id,
            path: "a.rs".into(),
            document_id: d.id,
            kind: BindingKind::File,
            review: None,
        })
        .unwrap();
    let backup = store.export_backup().unwrap();
    let other = tempfile::tempdir().unwrap();
    let mut restored = open(other.path());
    let preview = restored.import_backup(backup.clone(), true).unwrap();
    assert_eq!(
        (
            preview.new_workspaces,
            preview.new_records,
            preview.new_bindings
        ),
        (1, 2, 1)
    );
    assert!(restored.export_backup().unwrap().records.is_empty());
    restored.import_backup(backup.clone(), false).unwrap();
    let after = restored.export_backup().unwrap();
    assert_eq!(backup.records, after.records);
    assert_eq!(backup.workspaces, after.workspaces);
    assert_eq!(backup.bindings, after.bindings);
    let mut changed = a.input.clone();
    changed.content = "local edit".into();
    restored.update(&a.id, 1, changed).unwrap();
    let report = restored.import_backup(backup, false).unwrap();
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(report.new_records, 0);
    assert_eq!(restored.get(&a.id).unwrap().input.content, "local edit");
}

#[test]
fn invalid_backup_never_writes_partial_data() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    store.create(snippet("one", "first")).unwrap();
    let mut backup = store.export_backup().unwrap();
    let mut invalid = backup.records[0].clone();
    invalid.id = "new-id".into();
    invalid.input.content.clear();
    backup.records.push(invalid);
    let other = tempfile::tempdir().unwrap();
    let mut restored = open(other.path());
    assert!(restored.import_backup(backup, false).is_err());
    assert!(restored.export_backup().unwrap().records.is_empty());
}

#[test]
fn legacy_example_flags_survive_backup_without_generating_new_data() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    let original = store.create(snippet("mine", "real code")).unwrap();
    let mut backup = store.export_backup().unwrap();
    let mut legacy = original.clone();
    legacy.id = "example:legacy".into();
    legacy.is_demo = true;
    backup.records.push(legacy.clone());
    store.import_backup(backup, false).unwrap();
    assert_eq!(store.search(SearchFilter::default()).unwrap().total, 1);
    assert_eq!(store.get(&original.id).unwrap(), original);
    assert_eq!(store.get(&legacy.id).unwrap(), legacy);
    assert_eq!(store.export_backup().unwrap().records.len(), 2);
}

#[test]
fn migration_backs_up_v1_and_retains_user_data() {
    let root = tempfile::tempdir().unwrap();
    let info = runtime_info(Some(root.path().to_owned())).unwrap();
    let db = Connection::open(&info.database_path).unwrap();
    db.execute_batch("CREATE TABLE sentinel(value TEXT); INSERT INTO sentinel VALUES ('original'); PRAGMA user_version=1;").unwrap();
    drop(db);
    let _store = Store::open(&info).unwrap();
    let backup = std::fs::read_dir(root.path())
        .unwrap()
        .map(|p| p.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "bak"))
        .unwrap();
    let backup = Connection::open(backup).unwrap();
    assert_eq!(
        backup
            .pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        backup
            .query_row("SELECT value FROM sentinel", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "original"
    );
    let db = Connection::open(&info.database_path).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        SCHEMA_VERSION
    );
}

#[test]
fn failed_migration_rolls_back_schema_changes() {
    let root = tempfile::tempdir().unwrap();
    let info = runtime_info(Some(root.path().to_owned())).unwrap();
    let db = Connection::open(&info.database_path).unwrap();
    db.execute_batch("CREATE TABLE records(value TEXT); PRAGMA user_version=1;")
        .unwrap();
    assert!(Store::open(&info).is_err());
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    let count: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='workspaces'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn unsafe_links_and_source_traversal_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    for url in [
        "javascript:alert(1)",
        "data:text/html,x",
        "file://remote-server/secrets",
        "obsidian://delete?vault=x",
        "relative.md",
    ] {
        assert!(store.create(document("bad", url)).is_err(), "{url}");
    }
    let w = store
        .register_workspace(root.path().to_str().unwrap(), None)
        .unwrap();
    for path in [
        "../escape.rs",
        "/absolute.rs",
        "C:\\escape.rs",
        "a/./b.rs",
        "a//b.rs",
    ] {
        let mut input = snippet("bad", "code");
        input.source = Some(Source {
            workspace_id: w.id.clone(),
            path: path.into(),
            line: None,
        });
        assert!(store.create(input).is_err(), "{path}");
    }
}

#[test]
fn obsidian_links_only_allow_navigation_parameters() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    for url in [
        "obsidian://open?vault=demo&file=note&append=true&content=changed",
        "obsidian://open?vault=demo&prepend=true",
        "obsidian://open?clipboard=true",
        "obsidian://open?overwrite=true",
        "obsidian://open?content=changed",
        "obsidian://open?x-success=another%3A%2F%2Faction",
        "obsidian://open?%61ppend=true",
        "obsidian://open?paneType=unknown",
        "obsidian://user@open?vault=demo",
        "obsidian://open:123?vault=demo",
        "obsidian://open/new?vault=demo",
    ] {
        assert!(store.create(document("bad", url)).is_err(), "{url}");
    }
    for url in [
        "obsidian://open?vault=demo&file=%E6%94%AF%E4%BB%98%20note%23Heading&paneType=tab",
        "obsidian://open?path=%2Ftmp%2Fvault%2Fnote.md&paneType=split",
        "obsidian://open?vault=demo&paneType=window",
        "obsidian://open?vault=demo",
    ] {
        assert_eq!(
            store
                .create(document("note", url))
                .unwrap()
                .input
                .url
                .as_deref(),
            Some(url)
        );
    }
    let mut backup = store.export_backup().unwrap();
    backup.records[0].input.url = Some("obsidian://open?vault=demo&append=true".into());
    let report = store
        .preview_backup(serde_json::to_value(&backup).unwrap())
        .unwrap();
    assert_eq!(report.invalid_count, 1);
    assert!(store.import_backup(backup, false).is_err());
    assert_eq!(store.search(SearchFilter::default()).unwrap().total, 4);
}

#[test]
fn document_targets_revalidate_legacy_links_and_observed_revisions() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    let mut saved = store
        .create(document("note", "obsidian://open?vault=demo&file=note"))
        .unwrap();
    assert_eq!(
        store.document_target(&saved.id, saved.revision).unwrap(),
        saved.input.url.as_deref().unwrap()
    );
    assert!(matches!(
        store.document_target(&saved.id, saved.revision + 1),
        Err(Error::Conflict { .. })
    ));
    let snippet = store.create(snippet("code", "read();")).unwrap();
    assert!(matches!(
        store.document_target(&snippet.id, snippet.revision),
        Err(Error::Validation(_))
    ));
    // Simulate a row accepted by an older core without going through current validation.
    saved.input.url =
        Some("obsidian://open?vault=demo&file=note&append=true&content=changed".into());
    Connection::open(root.path().join("store/codemori.sqlite3"))
        .unwrap()
        .execute(
            "UPDATE records SET body=?1,url=?2 WHERE id=?3",
            rusqlite::params![
                serde_json::to_string(&saved).unwrap(),
                saved.input.url,
                saved.id
            ],
        )
        .unwrap();
    assert!(matches!(
        store.document_target(&saved.id, saved.revision),
        Err(Error::Validation(_))
    ));
    assert!(matches!(
        store.document_open_targets(&saved.id, saved.revision),
        Err(Error::Validation(_))
    ));
    // Keep the legacy record readable/editable so the user can repair the link.
    assert_eq!(store.get(&saved.id).unwrap(), saved);
    let repaired = store
        .update(
            &saved.id,
            saved.revision,
            document("note", "obsidian://open?vault=demo&file=note"),
        )
        .unwrap();
    assert!(
        store
            .document_target(&repaired.id, repaired.revision)
            .is_ok()
    );
}

#[test]
fn concurrent_clients_preserve_all_committed_records() {
    let root = tempfile::tempdir().unwrap();
    let info = runtime_info(Some(root.path().join("store"))).unwrap();
    let _store = Store::open(&info).unwrap();
    let handles: Vec<_> = (0..8)
        .map(|n| {
            let path = info.data_dir.clone();
            std::thread::spawn(move || {
                let mut store = Store::open(&runtime_info(Some(path)).unwrap()).unwrap();
                for m in 0..5 {
                    store
                        .create(snippet(&format!("{n}-{m}"), "concurrent"))
                        .unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(_store.search(SearchFilter::default()).unwrap().total, 40);
}

#[test]
fn cross_platform_workspace_backup_can_be_relinked_later() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    let backup = Backup {
        format_version: 1,
        exported_at: 0,
        workspaces: vec![Workspace {
            id: "windows".into(),
            name: "Windows project".into(),
            root: "C:\\Users\\dev\\project".into(),
            revision: 1,
        }],
        records: vec![],
        bindings: vec![],
    };
    assert_eq!(
        store.import_backup(backup, false).unwrap().new_workspaces,
        1
    );
    assert_eq!(store.workspaces().unwrap()[0].id, "windows");
}

#[test]
fn binding_move_is_atomic_and_preserves_the_document() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    let workspace = store
        .register_workspace(root.path().to_str().unwrap(), None)
        .unwrap();
    let doc = store
        .create(document("Design", "https://example.com/move"))
        .unwrap();
    let binding = Binding {
        workspace_id: workspace.id.clone(),
        path: "old.rs".into(),
        document_id: doc.id.clone(),
        kind: BindingKind::File,
        review: None,
    };
    store.link(binding.clone()).unwrap();
    assert!(store.move_binding(binding.clone(), "../escape").is_err());
    assert_eq!(store.document_bindings(&doc.id).unwrap()[0].path, "old.rs");
    let moved = store.move_binding(binding.clone(), "src/new.rs").unwrap();
    assert_eq!(store.document_bindings(&doc.id).unwrap(), vec![moved]);
    assert!(matches!(
        store.move_binding(binding, "another.rs"),
        Err(Error::NotFound(_))
    ));
    assert!(store.get(&doc.id).is_ok());
}

#[test]
fn backup_cannot_introduce_integers_that_javascript_would_round() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    store.create(snippet("safe", "code")).unwrap();
    let backup = store.export_backup().unwrap();
    let restored_root = tempfile::tempdir().unwrap();
    let mut restored = open(restored_root.path());
    let mut invalid = backup.clone();
    invalid.records[0].revision = codemori_core::MAX_JSON_INTEGER + 1;
    assert!(restored.import_backup(invalid, false).is_err());
    let mut invalid = backup.clone();
    invalid.records[0].updated_at = i64::MAX;
    assert!(restored.import_backup(invalid, false).is_err());
    assert!(restored.export_backup().unwrap().records.is_empty());
    let mut edge = backup;
    edge.records[0].updated_at = codemori_core::MAX_JSON_INTEGER as i64;
    restored.import_backup(edge.clone(), false).unwrap();
    let record = &edge.records[0];
    let updated = restored
        .update(&record.id, record.revision, record.input.clone())
        .unwrap();
    assert_eq!(updated.updated_at, codemori_core::MAX_JSON_INTEGER as i64);
    assert_eq!(updated.revision, record.revision + 1);
}

#[test]
fn feishu_targets_preserve_original_urls_and_only_route_supported_documents() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    for path in [
        "doc/abc",
        "docs/abc",
        "docx/abc",
        "wiki/abc",
        "sheets/abc",
        "base/abc",
        "mindnotes/abc",
        "slides/abc",
        "file/abc",
        "drive/file/abc",
    ] {
        let original = format!("https://tenant.feishu.cn/{path}?q=中文%20spaces&next=a%26b#标题");
        let record = store.create(document("Design", &original)).unwrap();
        let targets = store
            .document_open_targets(&record.id, record.revision)
            .unwrap();
        assert_eq!(Some(&targets.original_url), record.input.url.as_ref());
        let link = url::Url::parse(&targets.feishu_applink.unwrap()).unwrap();
        assert_eq!(link.scheme(), "feishu");
        assert_eq!(link.host_str(), Some("applink.feishu.cn"));
        assert_eq!(link.path(), "/client/web_url/open");
        let params = link
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(params.len(), 2);
        assert_eq!(params["mode"], "window");
        assert_eq!(params["url"], targets.original_url);
        assert_eq!(store.get(&record.id).unwrap(), record);
    }
    for original in [
        "https://notion.so/page",
        "https://evilfeishu.cn/docx/abc",
        "https://tenant.feishu.cn.evil.example/docx/abc",
        "https://feishu.cn/docx/abc",
        "https://tenant.feishu.cn:8443/docx/abc",
        "http://tenant.feishu.cn/docx/abc",
        "https://tenant.feishu.cn/auth/login",
        "https://tenant.feishu.cn/docx/",
        "https://tenant.feishu.cn/drive/home",
        "https://applink.feishu.cn/client/op/open",
        "obsidian://open?vault=test&file=note",
    ] {
        let record = store.create(document("Other", original)).unwrap();
        let targets = store
            .document_open_targets(&record.id, record.revision)
            .unwrap();
        assert!(targets.feishu_applink.is_none(), "{original}");
        assert_eq!(Some(targets.original_url), record.input.url);
    }
}

#[test]
fn feishu_targets_reject_stale_records_and_unsupported_inputs() {
    let root = tempfile::tempdir().unwrap();
    let mut store = open(root.path());
    let record = store
        .create(document("Design", "https://tenant.feishu.cn/docx/abc"))
        .unwrap();
    let mut edit = record.input.clone();
    edit.url = Some("https://notion.so/new".into());
    let updated = store.update(&record.id, record.revision, edit).unwrap();
    assert!(matches!(
        store.document_open_targets(&record.id, record.revision),
        Err(Error::Conflict { .. })
    ));
    assert!(
        store
            .document_open_targets(&record.id, updated.revision)
            .unwrap()
            .feishu_applink
            .is_none()
    );
    let snippet = store.create(snippet("Code", "abc")).unwrap();
    assert!(
        store
            .document_open_targets(&snippet.id, snippet.revision)
            .is_err()
    );
    assert!(
        store
            .create(document(
                "Unsafe",
                "https://user:pass@tenant.feishu.cn/docx/abc"
            ))
            .is_err()
    );
    assert!(
        store
            .create(document(
                "Direct scheme",
                "feishu://applink.feishu.cn/client/op/open"
            ))
            .is_err()
    );
}
