use codemori_core::{Error, Store, index::CommentFilter, runtime_info};
use std::{fs, path::Path};

fn file(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn search(store: &Store, term: &str) -> codemori_core::index::CommentPage {
    store
        .search_comments(CommentFilter {
            query: term.into(),
            ..Default::default()
        })
        .unwrap()
}

#[test]
fn language_parsers_extract_comments_and_never_string_contents() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    file(
        &root,
        "Main.java",
        "class Main {\n String s = \"// STRING_SECRET\";\n // Redis 支付重试\n /* java block */\n}",
    );
    file(
        &root,
        "one.js",
        "const s = '// STRING_SECRET'; const t = `/* STRING_SECRET */`; const re = /\\/\\/STRING_SECRET/;\n// javascript note\n/* js block */",
    );
    file(
        &root,
        "two.ts",
        "const s: string = '// STRING_SECRET';\n// typescript note",
    );
    file(
        &root,
        "view.tsx",
        "const s = <div>{/* tsx note */}STRING_SECRET</div>;",
    );
    file(
        &root,
        "main.py",
        "\"\"\"STRING_SECRET docstring\"\"\"\ns = '# STRING_SECRET'\n# python note\n",
    );
    let info = runtime_info(Some(temp.path().join("store"))).unwrap();
    let mut store = Store::open(&info).unwrap();
    let workspace = store
        .register_workspace(root.to_str().unwrap(), None)
        .unwrap();
    let report = store.index_comments(&workspace.id, None).unwrap();
    assert_eq!(report.indexed_files, 5);
    assert_eq!(report.failed_files, 0, "{:?}", report.details);
    assert_eq!(report.status.comments, 7);
    assert_eq!(search(&store, "STRING_SECRET").total, 0);
    let found = search(&store, "rEDis 重试");
    assert_eq!(found.total, 1);
    assert_eq!(found.items[0].source.path, "Main.java");
    assert_eq!(found.items[0].source.line, Some(3));
    assert!(
        found.items[0]
            .excerpt
            .iter()
            .any(|span| span.highlight && span.text == "重试")
    );
    let db = rusqlite::Connection::open(&info.database_path).unwrap();
    let texts: String = db
        .query_row("SELECT group_concat(text) FROM comments", [], |r| r.get(0))
        .unwrap();
    assert!(!texts.contains("STRING_SECRET"));
    assert!(
        !serde_json::to_string(&store.export_backup().unwrap())
            .unwrap()
            .contains("javascript note")
    );
    let repeat = store.index_comments(&workspace.id, None).unwrap();
    assert_eq!(repeat.unchanged_files, 5);
    assert_eq!(repeat.indexed_files, 0);
    assert!(
        store
            .comment_target(&found.items[0].source)
            .unwrap()
            .ends_with("Main.java")
    );
    file(&root, "Main.java", "class Main {\n // Redis 更新\n}");
    assert!(matches!(
        store.comment_target(&found.items[0].source),
        Err(Error::Validation(_))
    ));
    let single = store
        .index_comments(&workspace.id, Some("Main.java"))
        .unwrap();
    assert_eq!(single.indexed_files, 1);
    assert_eq!(single.status.files, 5);
    assert_eq!(search(&store, "重试").total, 0);
    assert_eq!(search(&store, "更新").total, 1);
    fs::remove_file(root.join("one.js")).unwrap();
    assert_eq!(
        store
            .index_comments(&workspace.id, None)
            .unwrap()
            .removed_files,
        1
    );
    assert_eq!(search(&store, "javascript").total, 0);
}

#[test]
fn ignores_failures_and_removed_single_files_clean_the_index() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    file(&root, "good.py", "# oldnote\n");
    file(&root, "bad.py", "def broken(\n# must not index\n");
    file(&root, "ignored.py", "# ignorednote\n");
    file(&root, ".gitignore", "ignored.py\nnested/skip.py\n");
    file(&root, "nested/skip.py", "# ignorednote\n");
    file(&root, "node_modules/pkg/index.js", "// ignorednote\n");
    file(&root, ".hidden.py", "# ignorednote\n");
    file(&root, "unknown.rb", "# ignorednote\n");
    let info = runtime_info(Some(temp.path().join("store"))).unwrap();
    let mut store = Store::open(&info).unwrap();
    let workspace = store
        .register_workspace(root.to_str().unwrap(), None)
        .unwrap();
    let report = store.index_comments(&workspace.id, None).unwrap();
    assert_eq!(report.indexed_files, 1);
    assert_eq!(report.failed_files, 1);
    assert_eq!(report.skipped_files, 1);
    assert_eq!(search(&store, "ignorednote").total, 0);
    let ignored = store
        .index_comments(&workspace.id, Some("ignored.py"))
        .unwrap();
    assert_eq!(ignored.indexed_files, 0);
    assert!(!ignored.details.is_empty());
    fs::remove_file(root.join("good.py")).unwrap();
    assert_eq!(
        store
            .index_comments(&workspace.id, Some("good.py"))
            .unwrap()
            .removed_files,
        1
    );
    assert_eq!(search(&store, "oldnote").total, 0);
    file(&root, "good.py", "# oldnote\n");
    store.index_comments(&workspace.id, None).unwrap();
    file(&root, ".ignore", "good.py\n");
    assert_eq!(
        store
            .index_comments(&workspace.id, None)
            .unwrap()
            .removed_files,
        1
    );
    assert_eq!(search(&store, "oldnote").total, 0);
    assert!(matches!(
        store.index_comments(&workspace.id, Some("../escape.py")),
        Err(Error::Validation(_))
    ));
}

#[test]
fn project_isolation_relocation_and_v2_migration_preserve_user_records() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("first");
    let second = temp.path().join("second");
    file(&root, "same.py", "# firstnote\n");
    file(&second, "same.py", "# secondnote\n");
    let info = runtime_info(Some(temp.path().join("store"))).unwrap();
    let mut store = Store::open(&info).unwrap();
    let first = store
        .register_workspace(root.to_str().unwrap(), None)
        .unwrap();
    let other = store
        .register_workspace(second.to_str().unwrap(), None)
        .unwrap();
    store.index_comments(&first.id, None).unwrap();
    store.index_comments(&other.id, None).unwrap();
    assert_eq!(
        store
            .search_comments(CommentFilter {
                workspace_id: Some(first.id.clone()),
                ..Default::default()
            })
            .unwrap()
            .items[0]
            .text,
        "# firstnote"
    );
    let moved = temp.path().join("moved");
    fs::rename(&root, &moved).unwrap();
    store
        .relocate_workspace(&first.id, moved.to_str().unwrap(), first.revision)
        .unwrap();
    assert_eq!(search(&store, "firstnote").total, 0);
    assert_eq!(search(&store, "secondnote").total, 1);
    drop(store);
    let db = rusqlite::Connection::open(&info.database_path).unwrap();
    db.execute_batch("DROP TABLE comments; DROP TABLE indexed_files; ALTER TABLE bindings DROP COLUMN kind; ALTER TABLE bindings DROP COLUMN review; PRAGMA user_version=2;")
        .unwrap();
    drop(db);
    let store = Store::open(&info).unwrap();
    assert_eq!(store.workspaces().unwrap().len(), 2);
    assert_eq!(store.comment_index_status(&other.id).unwrap().files, 0);
    assert!(fs::read_dir(&info.data_dir).unwrap().any(|p| {
        p.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("migration-v2-")
    }));
}

#[cfg(unix)]
#[test]
fn symlinked_files_and_directories_are_not_indexed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    file(temp.path(), "outside/secret.py", "# private comment\n");
    std::os::unix::fs::symlink(
        temp.path().join("outside/secret.py"),
        root.join("linked.py"),
    )
    .unwrap();
    std::os::unix::fs::symlink(temp.path().join("outside"), root.join("linked-dir")).unwrap();
    let mut store = Store::open(&runtime_info(Some(temp.path().join("store"))).unwrap()).unwrap();
    let workspace = store
        .register_workspace(root.to_str().unwrap(), None)
        .unwrap();
    let report = store.index_comments(&workspace.id, None).unwrap();
    assert_eq!(report.indexed_files, 0);
    assert_eq!(report.skipped_files, 2);
    assert_eq!(search(&store, "private").total, 0);
}
