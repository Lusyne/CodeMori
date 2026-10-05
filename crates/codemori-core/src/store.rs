use crate::{Error, RuntimeInfo, SCHEMA_VERSION, model::*};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub struct Store {
    pub(crate) db: Connection,
    pub(crate) data_dir: Option<PathBuf>,
    pub(crate) review_author: Option<crate::profile::Author>,
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn id() -> String {
    Uuid::new_v4().to_string()
}

const SCHEMA: &str = r#"
CREATE TABLE workspaces (id TEXT PRIMARY KEY, root TEXT NOT NULL UNIQUE, body TEXT NOT NULL);
CREATE TABLE records (
 id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('snippet','document')), body TEXT NOT NULL,
 url TEXT UNIQUE, title_key TEXT NOT NULL, tags_key TEXT NOT NULL, text_key TEXT NOT NULL,
 workspace_id TEXT REFERENCES workspaces(id), source_path TEXT,
 starred INTEGER NOT NULL, is_demo INTEGER NOT NULL, updated_at INTEGER NOT NULL, revision INTEGER NOT NULL
);
CREATE INDEX records_scope ON records(workspace_id, is_demo, updated_at DESC);
CREATE INDEX records_order ON records(is_demo, updated_at DESC);
CREATE TABLE bindings (
 workspace_id TEXT NOT NULL REFERENCES workspaces(id), path TEXT NOT NULL,
 document_id TEXT NOT NULL REFERENCES records(id) ON DELETE CASCADE,
 kind TEXT NOT NULL DEFAULT 'file', review TEXT,
 PRIMARY KEY(workspace_id,path,document_id)
);
CREATE INDEX bindings_document ON bindings(document_id,workspace_id);
"#;

impl Store {
    pub fn open(info: &RuntimeInfo) -> Result<Self, Error> {
        std::fs::create_dir_all(&info.data_dir)?;
        let mut db = Connection::open(&info.database_path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.pragma_update(None, "foreign_keys", true)?;
        let version: u32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::UnsupportedSchema { found: version });
        }
        // The online API includes live WAL content and does not copy an inconsistent file.
        if version > 0 && version < SCHEMA_VERSION {
            let backup = info.data_dir.join(format!(
                "migration-v{version}-{}-{}.sqlite3.bak",
                now(),
                id()
            ));
            db.backup("main", backup, None)?;
        }
        {
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let version: u32 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
            if version > SCHEMA_VERSION {
                return Err(Error::UnsupportedSchema { found: version });
            }
            if version < 2 {
                tx.execute_batch(SCHEMA)?;
            }
            if version < 3 {
                tx.execute_batch(crate::index::SCHEMA)?;
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
            if (2..4).contains(&version) {
                tx.execute_batch("ALTER TABLE bindings ADD COLUMN kind TEXT NOT NULL DEFAULT 'file'; ALTER TABLE bindings ADD COLUMN review TEXT;")?;
            }
            if version < SCHEMA_VERSION {
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
            tx.commit()?;
        }
        db.pragma_update(None, "journal_mode", "WAL")?;
        Ok(Self {
            db,
            data_dir: Some(info.data_dir.clone()),
            review_author: None,
        })
    }

    pub(crate) fn memory() -> Result<Self, Error> {
        let db = Connection::open_in_memory()?;
        db.pragma_update(None, "foreign_keys", true)?;
        db.execute_batch(SCHEMA)?;
        db.execute_batch(crate::index::SCHEMA)?;
        Ok(Self {
            db,
            data_dir: None,
            review_author: None,
        })
    }

    pub fn workspaces(&self) -> Result<Vec<Workspace>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT body FROM workspaces ORDER BY root")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows.iter().map(|s| Ok(serde_json::from_str(s)?)).collect()
    }

    pub fn register_workspace(
        &mut self,
        root: &str,
        name: Option<String>,
    ) -> Result<Workspace, Error> {
        let root = canonical_root(root)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(body) = tx
            .query_row("SELECT body FROM workspaces WHERE root=?1", [&root], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
        {
            return Ok(serde_json::from_str(&body)?);
        }
        let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
            Path::new(&root)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.clone())
        });
        let workspace = Workspace {
            id: id(),
            name,
            root,
            revision: 1,
        };
        insert_workspace(&tx, &workspace)?;
        tx.commit()?;
        Ok(workspace)
    }

    pub fn relocate_workspace(
        &mut self,
        id: &str,
        root: &str,
        revision: u64,
    ) -> Result<Workspace, Error> {
        let root = canonical_root(root)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut workspace = workspace_by_id(&tx, id)?;
        check_revision(workspace.revision, revision)?;
        if tx
            .query_row(
                "SELECT id FROM workspaces WHERE root=?1 AND id<>?2",
                params![root, id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .is_some()
        {
            return Err(Error::Validation(
                "Another workspace already uses that directory".into(),
            ));
        }
        if workspace.root != root {
            tx.execute("DELETE FROM indexed_files WHERE workspace_id=?1", [id])?;
        }
        workspace.root = root;
        workspace.revision = next_revision(workspace.revision)?;
        tx.execute(
            "UPDATE workspaces SET root=?1,body=?2 WHERE id=?3",
            params![
                workspace.root,
                serde_json::to_string(&workspace)?,
                workspace.id
            ],
        )?;
        tx.commit()?;
        Ok(workspace)
    }

    pub fn get(&self, id: &str) -> Result<Record, Error> {
        record_by_id(&self.db, id)
    }

    pub fn document_target(&self, id: &str, revision: u64) -> Result<String, Error> {
        let record = self.get(id)?;
        if record.revision != revision {
            return Err(Error::Conflict {
                current_revision: record.revision,
            });
        }
        if record.input.kind != RecordKind::Document {
            return Err(Error::Validation("Select a document to open".into()));
        }
        // Revalidate old records too: an Obsidian `open` URL may carry write parameters.
        canonical_url(record.input.url.as_deref().unwrap_or_default())
    }

    pub fn document_open_targets(
        &self,
        id: &str,
        revision: u64,
    ) -> Result<DocumentOpenTargets, Error> {
        let original_url = self.document_target(id, revision)?;
        let feishu_applink = feishu_applink(&original_url);
        Ok(DocumentOpenTargets {
            original_url,
            feishu_applink,
        })
    }

    pub fn create(&mut self, input: RecordInput) -> Result<Record, Error> {
        let input = normalize_input(input)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_source(&tx, &input)?;
        if let Some(url) = &input.url {
            if let Some(body) = tx
                .query_row("SELECT body FROM records WHERE url=?1", [url], |r| {
                    r.get::<_, String>(0)
                })
                .optional()?
            {
                return Ok(serde_json::from_str(&body)?);
            }
        }
        let stamp = now();
        let record = Record {
            id: id(),
            revision: 1,
            created_at: stamp,
            updated_at: stamp,
            is_demo: false,
            input,
        };
        insert_record(&tx, &record)?;
        tx.commit()?;
        Ok(record)
    }

    pub fn update(&mut self, id: &str, revision: u64, input: RecordInput) -> Result<Record, Error> {
        let input = normalize_input(input)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record = record_by_id(&tx, id)?;
        check_revision(record.revision, revision)?;
        if record.input.kind != input.kind {
            return Err(Error::Validation("A record cannot change kind".into()));
        }
        validate_source(&tx, &input)?;
        if let Some(url) = &input.url {
            if tx
                .query_row(
                    "SELECT id FROM records WHERE url=?1 AND id<>?2",
                    params![url, id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .is_some()
            {
                return Err(Error::Validation(
                    "This document URL already exists; link the existing document".into(),
                ));
            }
        }
        record.input = input;
        record.revision = next_revision(record.revision)?;
        record.updated_at = now()
            .max(record.updated_at.saturating_add(1))
            .min(crate::MAX_JSON_INTEGER as i64);
        write_record(&tx, &record, false)?;
        tx.commit()?;
        Ok(record)
    }

    pub fn delete(&mut self, id: &str, revision: u64) -> Result<(), Error> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = record_by_id(&tx, id)?;
        check_revision(record.revision, revision)?;
        tx.execute("DELETE FROM records WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn link(&mut self, mut binding: Binding) -> Result<Binding, Error> {
        binding.path = crate::context::binding_path(&binding.path, binding.kind)?;
        let workspace = workspace_by_id(&self.db, &binding.workspace_id)?;
        if let Ok(existing) = crate::context::stored_binding(&self.db, &binding) {
            if existing.kind != binding.kind {
                return Err(Error::Validation(
                    "A different association already exists at this path".into(),
                ));
            }
            return Ok(existing);
        }
        binding.review = crate::context::capture_review(
            &workspace.root,
            &binding.path,
            binding.kind,
            self.review_actor()?,
        )
        .ok();
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = record_by_id(&tx, &binding.document_id)?;
        if record.input.kind != RecordKind::Document || record.is_demo {
            return Err(Error::Validation(
                "Only real documents can be associated".into(),
            ));
        }
        crate::context::insert_binding(&tx, &binding)?;
        tx.commit()?;
        Ok(binding)
    }
    pub fn unlink(&mut self, binding: Binding) -> Result<(), Error> {
        let path = crate::context::binding_path(&binding.path, binding.kind)?;
        self.db.execute(
            "DELETE FROM bindings WHERE workspace_id=?1 AND path=?2 AND document_id=?3 AND kind=?4",
            params![
                binding.workspace_id,
                path,
                binding.document_id,
                binding.kind.key()
            ],
        )?;
        Ok(())
    }
    pub fn file_documents(&self, workspace_id: &str, path: &str) -> Result<Vec<Record>, Error> {
        let mut seen = HashSet::new();
        let mut records = Vec::new();
        for binding in self.effective_bindings(workspace_id, path)? {
            if seen.insert(binding.document_id.clone()) {
                records.push(self.get(&binding.document_id)?);
            }
        }
        records.sort_by(|a, b| (&a.input.title, &a.id).cmp(&(&b.input.title, &b.id)));
        Ok(records)
    }
    pub fn document_bindings(&self, document_id: &str) -> Result<Vec<Binding>, Error> {
        if record_by_id(&self.db, document_id)?.input.kind != RecordKind::Document {
            return Err(Error::Validation("Only documents have associations".into()));
        }
        Ok(crate::context::all_bindings(&self.db)?
            .into_iter()
            .filter(|b| b.document_id == document_id)
            .collect())
    }
    pub fn move_binding(&mut self, binding: Binding, new_path: &str) -> Result<Binding, Error> {
        let old = crate::context::stored_binding(&self.db, &binding)?;
        let new = Binding {
            path: crate::context::binding_path(new_path, old.kind)?,
            ..old.clone()
        };
        if old.path == new.path {
            return Ok(old);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if crate::context::stored_binding(&tx, &new).is_ok() {
            return Err(Error::Validation(
                "The target association already exists".into(),
            ));
        }
        tx.execute(
            "DELETE FROM bindings WHERE workspace_id=?1 AND path=?2 AND document_id=?3",
            params![old.workspace_id, old.path, old.document_id],
        )?;
        crate::context::insert_binding(&tx, &new)?;
        tx.commit()?;
        Ok(new)
    }
    pub fn source_target(&self, source: &Source) -> Result<PathBuf, Error> {
        let workspace = workspace_by_id(&self.db, &source.workspace_id)?;
        let root = PathBuf::from(workspace.root).canonicalize()?;
        let path = root.join(relative_path(&source.path)?).canonicalize()?;
        if !path.starts_with(&root) {
            return Err(Error::Validation(
                "Source resolves outside the workspace".into(),
            ));
        }
        Ok(path)
    }
    pub fn tags(&self, demo: bool) -> Result<Vec<String>, Error> {
        let mut stmt = self
            .db
            .prepare("SELECT body FROM records WHERE is_demo=?1 ORDER BY updated_at DESC")?;
        let rows = stmt
            .query_map([demo], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut seen = HashSet::new();
        let mut tags = Vec::new();
        for row in rows {
            let record: Record = serde_json::from_str(&row)?;
            for tag in record.input.tags {
                if seen.insert(tag.to_lowercase()) {
                    tags.push(tag);
                }
            }
        }
        tags.sort_by_key(|s| s.to_lowercase());
        Ok(tags)
    }
}

pub(crate) fn workspace_by_id(db: &Connection, id: &str) -> Result<Workspace, Error> {
    let body: Option<String> = db
        .query_row("SELECT body FROM workspaces WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(serde_json::from_str(
        &body.ok_or_else(|| Error::NotFound(id.into()))?,
    )?)
}
pub(crate) fn record_by_id(db: &Connection, id: &str) -> Result<Record, Error> {
    let body: Option<String> = db
        .query_row("SELECT body FROM records WHERE id=?1", [id], |r| r.get(0))
        .optional()?;
    Ok(serde_json::from_str(
        &body.ok_or_else(|| Error::NotFound(id.into()))?,
    )?)
}
pub(crate) fn insert_workspace(db: &Connection, item: &Workspace) -> Result<(), Error> {
    db.execute(
        "INSERT INTO workspaces VALUES (?1,?2,?3)",
        params![item.id, item.root, serde_json::to_string(item)?],
    )?;
    Ok(())
}
pub(crate) fn insert_record(db: &Connection, record: &Record) -> Result<(), Error> {
    write_record(db, record, true)
}
pub(crate) fn write_record(db: &Connection, record: &Record, insert: bool) -> Result<(), Error> {
    let r = &record.input;
    let tags = serde_json::to_string(&r.tags.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>())?;
    let text = format!(
        "{}\n{}\n{}\n{}",
        r.title,
        r.tags.join("\n"),
        r.description,
        r.content
    )
    .to_lowercase();
    let body = serde_json::to_string(record)?;
    let workspace = r.source.as_ref().map(|s| &s.workspace_id);
    let path = r.source.as_ref().map(|s| &s.path);
    let sql = if insert {
        "INSERT INTO records (id,kind,body,url,title_key,tags_key,text_key,workspace_id,source_path,starred,is_demo,updated_at,revision) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
    } else {
        "UPDATE records SET kind=?2,body=?3,url=?4,title_key=?5,tags_key=?6,text_key=?7,workspace_id=?8,source_path=?9,starred=?10,is_demo=?11,updated_at=?12,revision=?13 WHERE id=?1"
    };
    db.execute(
        sql,
        params![
            record.id,
            r.kind.key(),
            body,
            r.url,
            r.title.to_lowercase(),
            tags,
            text,
            workspace,
            path,
            r.starred,
            record.is_demo,
            record.updated_at,
            record.revision
        ],
    )?;
    Ok(())
}
fn check_revision(current: u64, expected: u64) -> Result<(), Error> {
    if current != expected {
        return Err(Error::Conflict {
            current_revision: current,
        });
    }
    Ok(())
}
fn validate_source(db: &Connection, input: &RecordInput) -> Result<(), Error> {
    if let Some(source) = &input.source {
        workspace_by_id(db, &source.workspace_id)?;
    }
    Ok(())
}
pub(crate) fn canonical_root(root: &str) -> Result<String, Error> {
    let path = Path::new(root).canonicalize()?;
    if !path.is_dir() {
        return Err(Error::Validation(
            "Workspace root must be a directory".into(),
        ));
    }
    path.into_os_string()
        .into_string()
        .map_err(|_| Error::Validation("Workspace path must be UTF-8".into()))
}
pub(crate) fn relative_path(path: &str) -> Result<String, Error> {
    let path = path.replace('\\', "/");
    if path.is_empty()
        || path.contains(':')
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || Path::new(&path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error::Validation(
            "Source path must be relative without parent/current components".into(),
        ));
    }
    Ok(path)
}
pub(crate) fn normalize_input(mut input: RecordInput) -> Result<RecordInput, Error> {
    input.title = input.title.trim().to_string();
    let mut seen = HashSet::new();
    input.tags = input
        .tags
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && seen.insert(s.to_lowercase()))
        .collect();
    if let Some(source) = &mut input.source {
        source.path = relative_path(&source.path)?;
        if source.workspace_id.is_empty() || source.line == Some(0) {
            return Err(Error::Validation("Invalid source workspace or line".into()));
        }
    }
    match input.kind {
        RecordKind::Snippet => {
            if input.content.trim().is_empty() {
                return Err(Error::Validation("Snippet code must not be empty".into()));
            }
            if input.url.is_some() {
                return Err(Error::Validation(
                    "A snippet cannot have a document URL".into(),
                ));
            }
            if input.title.is_empty() {
                input.title = input
                    .content
                    .trim()
                    .lines()
                    .next()
                    .unwrap_or("Snippet")
                    .chars()
                    .take(80)
                    .collect();
            }
            if input.language.trim().is_empty() {
                input.language = match input
                    .source
                    .as_ref()
                    .and_then(|s| Path::new(&s.path).extension())
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                {
                    "rs" => "rust",
                    "java" => "java",
                    "kt" | "kts" => "kotlin",
                    "go" => "go",
                    "py" => "python",
                    "js" | "jsx" => "javascript",
                    "ts" | "tsx" => "typescript",
                    "json" => "json",
                    "md" => "markdown",
                    "sh" => "shell",
                    "sql" => "sql",
                    "css" => "css",
                    "html" => "html",
                    _ => "text",
                }
                .into();
            }
        }
        RecordKind::Document => {
            if input.title.is_empty() {
                return Err(Error::Validation("Document title is required".into()));
            }
            if !input.content.is_empty() || input.source.is_some() || input.starred {
                return Err(Error::Validation(
                    "Documents use summaries and bindings, not code/source/star fields".into(),
                ));
            }
            input.url = Some(canonical_url(input.url.as_deref().unwrap_or(""))?);
            input.language.clear();
        }
    }
    Ok(input)
}
pub fn canonical_url(value: &str) -> Result<String, Error> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::Validation("Document link is required".into()));
    }
    if Path::new(value).is_absolute() {
        return url::Url::from_file_path(value)
            .map(|u| u.to_string())
            .map_err(|_| Error::Validation("Invalid local file path".into()));
    }
    let url = url::Url::parse(value).map_err(|_| {
        Error::Validation("Use an absolute local file path or a valid document URL".into())
    })?;
    match url.scheme() {
        "http" | "https"
            if url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none() => {}
        "obsidian"
            if url.host_str() == Some("open")
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && matches!(url.path(), "" | "/") =>
        {
            for (key, value) in url.query_pairs() {
                match key.as_ref() {
                    "vault" | "file" | "path" => {}
                    "paneType" if matches!(value.as_ref(), "tab" | "split" | "window") => {}
                    _ => {
                        return Err(Error::Validation(
                            "Obsidian links allow navigation only: vault, file, path and paneType (tab/split/window). Remove content, write or callback parameters.".into(),
                        ));
                    }
                }
            }
        }
        "file" if url.to_file_path().is_ok() && url.host_str().is_none_or(|h| h == "localhost") => {
        }
        _ => {
            return Err(Error::Validation(
                "Only HTTP(S), obsidian://open and local file links are supported".into(),
            ));
        }
    }
    Ok(url.to_string())
}

fn feishu_applink(original: &str) -> Option<String> {
    let url = url::Url::parse(original).ok()?;
    if url.scheme() != "https"
        || url.port().is_some()
        || url.host_str()?.strip_suffix(".feishu.cn")?.is_empty()
    {
        return None;
    }
    let mut segments = url.path_segments()?;
    let kind = segments.next()?;
    if kind == "drive" {
        if segments.next()? != "file" {
            return None;
        }
    } else if !matches!(
        kind,
        "doc" | "docs" | "docx" | "wiki" | "sheets" | "base" | "mindnotes" | "slides" | "file"
    ) {
        return None;
    }
    if segments.next()?.is_empty() {
        return None;
    }
    // The docs/open AppLink excludes PC. web_url/open supports PC windows.
    // Direct feishu launch has no automatic web fallback; adapters keep the original action.
    let mut target = url::Url::parse("feishu://applink.feishu.cn/client/web_url/open").ok()?;
    target
        .query_pairs_mut()
        .append_pair("mode", "window")
        .append_pair("url", original);
    Some(target.into())
}

fn next_revision(current: u64) -> Result<u64, Error> {
    if current >= crate::MAX_JSON_INTEGER {
        return Err(Error::Validation(
            "Revision exceeds the supported JSON integer range".into(),
        ));
    }
    Ok(current + 1)
}
