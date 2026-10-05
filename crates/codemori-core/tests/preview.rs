use codemori_core::{Error, Store, model::*, runtime_info};

#[test]
fn local_markdown_is_live_readonly_escaped_and_not_indexed() {
    let root = tempfile::tempdir().unwrap();
    let info = runtime_info(Some(root.path().join("store"))).unwrap();
    let mut store = Store::open(&info).unwrap();
    let file = root.path().join("中文 notes.md");
    let body = "# Private正文\n\n**bold** and `code`\n\n<script>alert(1)</script>\n\n![alt](https://example.com/image.png) [click](javascript:alert(1))\n\n<img src=file:///private/data>";
    std::fs::write(&file, body).unwrap();
    let record = store
        .create(RecordInput {
            kind: RecordKind::Document,
            title: "Notes".into(),
            content: String::new(),
            language: String::new(),
            description: "summary only".into(),
            tags: vec![],
            starred: false,
            source: None,
            url: Some(file.to_str().unwrap().into()),
        })
        .unwrap();
    let mut before = serde_json::to_value(store.export_backup().unwrap()).unwrap();
    before.as_object_mut().unwrap().remove("exported_at");
    let preview = store.markdown_preview(&record.id, record.revision).unwrap();
    assert!(preview.html.contains("<h1>Private正文</h1>"));
    assert!(preview.html.contains("<strong>bold</strong>"));
    assert!(preview.html.contains("&lt;script&gt;"));
    for forbidden in ["<script", "<img", "<a ", "src=\"", "href=\""] {
        assert!(!preview.html.contains(forbidden), "{forbidden}");
    }
    assert_eq!(std::fs::read_to_string(&file).unwrap(), body);
    assert_eq!(
        store
            .search(SearchFilter {
                query: "Private正文".into(),
                ..Default::default()
            })
            .unwrap()
            .total,
        0
    );
    let mut after = serde_json::to_value(store.export_backup().unwrap()).unwrap();
    after.as_object_mut().unwrap().remove("exported_at");
    assert_eq!(after, before);
    std::fs::write(&file, "# changed").unwrap();
    assert!(
        store
            .markdown_preview(&record.id, 1)
            .unwrap()
            .html
            .contains("changed")
    );
    assert!(matches!(
        store.markdown_preview(&record.id, 99),
        Err(Error::Conflict { .. })
    ));
    std::fs::write(&file, vec![b'x'; 2 * 1024 * 1024 + 1]).unwrap();
    assert!(matches!(
        store.markdown_preview(&record.id, 1),
        Err(Error::Validation(_))
    ));
    std::fs::write(&file, [0xff]).unwrap();
    assert!(matches!(
        store.markdown_preview(&record.id, 1),
        Err(Error::Validation(_))
    ));
    std::fs::remove_file(&file).unwrap();
    assert!(matches!(
        store.markdown_preview(&record.id, 1),
        Err(Error::Io(_))
    ));
    let mut input = record.input;
    input.url = Some("https://example.com/notes.md".into());
    let remote = store.update(&record.id, 1, input).unwrap();
    assert!(matches!(
        store.markdown_preview(&remote.id, 2),
        Err(Error::Validation(_))
    ));
}
