//! Portable project knowledge. JSON is authoritative; SQLite is only an in-memory view.
use crate::{
    Error, Store,
    model::*,
    profile::Author,
    rpc::Request,
    store::{canonical_root, canonical_url, normalize_input, relative_path},
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const PROJECT_WORKSPACE: &str = "project";
const MAX_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct ProjectInfo {
    pub root: String,
    pub file: String,
    pub exists: bool,
    pub version: String,
    pub error: Option<String>,
}
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    format_version: u32,
    #[serde(default)]
    records: Vec<ProjectRecord>,
    #[serde(default)]
    bindings: Vec<ProjectBinding>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectRecord {
    id: String,
    revision: u64,
    created_at: i64,
    updated_at: i64,
    input: ProjectInput,
    #[serde(default)]
    created_by: Option<Author>,
    #[serde(default)]
    updated_by: Option<Author>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectInput {
    kind: RecordKind,
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    source: Option<ProjectSource>,
    #[serde(default)]
    url: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectSource {
    path: String,
    #[serde(default)]
    line: Option<u32>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectBinding {
    #[serde(default)]
    kind: BindingKind,
    #[serde(default)]
    review: Option<Review>,
    path: String,
    document_id: String,
}

pub(crate) struct Snapshot {
    pub info: ProjectInfo,
    pub store: Store,
    root: PathBuf,
    authors: std::collections::BTreeMap<String, (Option<Author>, Option<Author>)>,
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
pub(crate) fn plain_path(path: &Path, directory: bool) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Ok(meta)
            if meta.file_type().is_symlink()
                || (directory && !meta.is_dir())
                || (!directory && !meta.is_file()) =>
        {
            Err(invalid(
                "Project shared paths must be regular files/directories, not symbolic links",
            ))
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn project_path(root: &Path) -> Result<PathBuf, Error> {
    let directory = root.join(".codemori");
    plain_path(&directory, true)?;
    if directory.join("codemori.sqlite3").exists() {
        return Err(invalid(
            "The personal CodeMori data directory cannot be used as a project shared directory",
        ));
    }
    let file = directory.join("shared.json");
    plain_path(&file, false)?;
    Ok(file)
}
fn read_bytes(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    plain_path(path, false)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if file.metadata()?.len() > MAX_BYTES {
        return Err(invalid("Project shared file exceeds 128 MiB"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("Project shared file exceeds 128 MiB"));
    }
    Ok(Some(bytes))
}
fn version(bytes: Option<&[u8]>) -> String {
    bytes
        .map(|b| format!("{:x}", Sha256::digest(b)))
        .unwrap_or_else(|| "missing".into())
}
fn local_path(root: &Path, relative: &str) -> Result<PathBuf, Error> {
    let relative = relative_path(relative)?;
    let mut path = root.to_path_buf();
    for part in Path::new(&relative).components() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(invalid(
                    "Shared local document paths must not contain symbolic links",
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}
fn resolve_url(root: &Path, value: &str) -> Result<String, Error> {
    if let Some(relative) = value.strip_prefix("./") {
        return url::Url::from_file_path(local_path(root, relative)?)
            .map(|u| u.to_string())
            .map_err(|_| invalid("Invalid project document path"));
    }
    let canonical = canonical_url(value)?;
    if canonical.starts_with("file:") {
        return Err(invalid(
            "Shared local documents must use ./project-relative paths",
        ));
    }
    portable_url(root, &canonical)?;
    Ok(canonical)
}
fn portable_url(root: &Path, value: &str) -> Result<String, Error> {
    let canonical = canonical_url(value)?;
    let url = url::Url::parse(&canonical).map_err(|_| invalid("Invalid document URL"))?;
    if url.scheme() == "file" {
        let path = url
            .to_file_path()
            .map_err(|_| invalid("Invalid local document path"))?;
        let relative = match path.strip_prefix(root) {
            Ok(relative) => relative,
            Err(_) => {
                // Resolve aliases preceding the project, then reject in-project symlinks.
                let ancestor = path
                    .ancestors()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .find(|p| p.canonicalize().is_ok_and(|p| p == root))
                    .ok_or_else(|| {
                        invalid("Only local documents inside this project can be shared")
                    })?;
                path.strip_prefix(ancestor)
                    .map_err(|_| invalid("Invalid project document path"))?
            }
        };
        let relative = relative
            .to_str()
            .ok_or_else(|| invalid("Document path must be UTF-8"))?
            .replace('\\', "/");
        local_path(root, &relative)?;
        return Ok(format!("./{}", relative_path(&relative)?));
    }
    if url.scheme() == "obsidian" {
        let params: std::collections::HashMap<_, _> = url.query_pairs().collect();
        if params.contains_key("path")
            || params.get("vault").is_none_or(|v| v.is_empty())
            || !params.get("file").is_some_and(|v| relative_path(v).is_ok())
        {
            return Err(invalid(
                "Shared Obsidian links require vault and relative file, not a machine-specific path",
            ));
        }
    }
    Ok(canonical)
}
impl ProjectRecord {
    fn into_record(self, root: &Path) -> Result<Record, Error> {
        let input = normalize_input(RecordInput {
            kind: self.input.kind,
            title: self.input.title,
            content: self.input.content,
            language: self.input.language,
            description: self.input.description,
            tags: self.input.tags,
            starred: false,
            source: self.input.source.map(|s| Source {
                workspace_id: PROJECT_WORKSPACE.into(),
                path: s.path,
                line: s.line,
            }),
            url: self.input.url.map(|u| resolve_url(root, &u)).transpose()?,
        })?;
        Ok(Record {
            id: self.id,
            revision: self.revision,
            created_at: self.created_at,
            updated_at: self.updated_at,
            is_demo: false,
            input,
        })
    }
    fn from_record(record: Record, root: &Path) -> Result<Self, Error> {
        if record.is_demo || record.input.starred {
            return Err(invalid(
                "Examples and personal favorites cannot be written into project shared data",
            ));
        }
        let input = record.input;
        Ok(Self {
            created_by: None,
            updated_by: None,
            id: record.id,
            revision: record.revision,
            created_at: record.created_at,
            updated_at: record.updated_at,
            input: ProjectInput {
                kind: input.kind,
                title: input.title,
                content: input.content,
                language: input.language,
                description: input.description,
                tags: input.tags,
                source: input.source.map(|s| ProjectSource {
                    path: s.path,
                    line: s.line,
                }),
                url: input.url.map(|u| portable_url(root, &u)).transpose()?,
            },
        })
    }
}
impl Snapshot {
    pub(crate) fn load(root: &str) -> Result<Self, Error> {
        let root = PathBuf::from(canonical_root(root)?);
        let path = project_path(&root)?;
        let bytes = read_bytes(&path)?;
        let mut store = Store::memory()?;
        let file: ProjectFile = match &bytes {
            Some(bytes) => serde_json::from_slice(bytes)?,
            None => ProjectFile {
                format_version: 1,
                ..Default::default()
            },
        };
        if !matches!(file.format_version, 1..=3) {
            return Err(invalid(
                "Unsupported project shared format; update CodeMori before editing",
            ));
        }
        let mut authors = std::collections::BTreeMap::new();
        for record in &file.records {
            for author in [&record.created_by, &record.updated_by]
                .into_iter()
                .flatten()
            {
                author.validate()?;
            }
            authors.insert(
                record.id.clone(),
                (record.created_by.clone(), record.updated_by.clone()),
            );
        }
        let records = file
            .records
            .into_iter()
            .map(|r| r.into_record(&root))
            .collect::<Result<_, _>>()?;
        let bindings = file
            .bindings
            .into_iter()
            .map(|b| Binding {
                kind: b.kind,
                review: b.review,
                workspace_id: PROJECT_WORKSPACE.into(),
                path: b.path,
                document_id: b.document_id,
            })
            .collect();
        store.import_backup(
            Backup {
                format_version: 1,
                exported_at: 0,
                workspaces: vec![Workspace {
                    id: PROJECT_WORKSPACE.into(),
                    revision: 1,
                    name: root
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    root: root.to_string_lossy().into_owned(),
                }],
                records,
                bindings,
            },
            false,
        )?;
        let info = ProjectInfo {
            root: root.to_string_lossy().into_owned(),
            file: path.to_string_lossy().into_owned(),
            exists: bytes.is_some(),
            version: version(bytes.as_deref()),
            error: None,
        };
        Ok(Self {
            root,
            info,
            store,
            authors,
        })
    }
    pub(crate) fn view(&self, mut value: Value) -> Result<Value, Error> {
        fn visit(value: &mut Value, snapshot: &Snapshot) -> Result<(), Error> {
            if let Some(object) = value.as_object_mut() {
                if object.contains_key("input")
                    && object.contains_key("revision")
                    && object.contains_key("id")
                {
                    if let Some(url) = object.get_mut("input").and_then(|v| v.get_mut("url")) {
                        if let Some(text) = url.as_str() {
                            *url = json!(portable_url(&snapshot.root, text)?);
                        }
                    }
                    if let Some((created, updated)) = object
                        .get("id")
                        .and_then(Value::as_str)
                        .and_then(|id| snapshot.authors.get(id))
                    {
                        object.insert("created_by".into(), json!(created));
                        object.insert("updated_by".into(), json!(updated));
                    }
                    object.insert("scope".into(), json!("project"));
                    object.insert("project_root".into(), json!(snapshot.info.root));
                    object.insert("project_version".into(), json!(snapshot.info.version));
                } else {
                    for child in object.values_mut() {
                        visit(child, snapshot)?;
                    }
                }
            } else if let Some(array) = value.as_array_mut() {
                for child in array {
                    visit(child, snapshot)?;
                }
            }
            Ok(())
        }
        visit(&mut value, self)?;
        Ok(value)
    }
    fn prepare_input(
        &self,
        personal: &Store,
        mut input: RecordInput,
    ) -> Result<RecordInput, Error> {
        if input.starred {
            return Err(invalid(
                "Favorites belong to personal data, not the shared project file",
            ));
        }
        if let Some(source) = &mut input.source {
            if source.workspace_id != PROJECT_WORKSPACE {
                let workspace = crate::store::workspace_by_id(&personal.db, &source.workspace_id)?;
                if workspace.root != self.info.root {
                    return Err(invalid("The captured source belongs to another project"));
                }
            }
            source.workspace_id = PROJECT_WORKSPACE.into();
        }
        if let Some(value) = &input.url {
            input.url = Some(if value.starts_with("./") {
                resolve_url(&self.root, value)?
            } else {
                let canonical = canonical_url(value)?;
                let portable = portable_url(&self.root, &canonical)?;
                if portable.starts_with("./") {
                    resolve_url(&self.root, &portable)?
                } else {
                    canonical
                }
            });
        }
        normalize_input(input)
    }
    fn write(&mut self) -> Result<(), Error> {
        let backup = self.store.export_backup()?;
        let mut file = ProjectFile {
            format_version: 3,
            records: backup
                .records
                .into_iter()
                .map(|r| ProjectRecord::from_record(r, &self.root))
                .collect::<Result<_, _>>()?,
            bindings: backup
                .bindings
                .into_iter()
                .map(|b| ProjectBinding {
                    kind: b.kind,
                    review: b.review,
                    path: b.path,
                    document_id: b.document_id,
                })
                .collect(),
        };
        for record in &mut file.records {
            if let Some((created, updated)) = self.authors.get(&record.id) {
                record.created_by = created.clone();
                record.updated_by = updated.clone();
            }
        }
        file.records.sort_by(|a, b| a.id.cmp(&b.id));
        file.bindings
            .sort_by(|a, b| (&a.path, &a.document_id).cmp(&(&b.path, &b.document_id)));
        let mut bytes = serde_json::to_vec_pretty(&file)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_BYTES {
            return Err(invalid("Project shared file exceeds 128 MiB"));
        }
        let path = project_path(&self.root)?;
        if version(read_bytes(&path)?.as_deref()) != self.info.version {
            return Err(Error::ProjectConflict);
        }
        atomic_write(&path, &bytes)?;
        self.info.version = version(Some(&bytes));
        self.info.exists = true;
        Ok(())
    }
}
pub fn info(root: &str) -> ProjectInfo {
    match Snapshot::load(root) {
        Ok(snapshot) => snapshot.info,
        Err(error) => failed_info(root, &error),
    }
}
pub(crate) fn failed_info(root: &str, error: &Error) -> ProjectInfo {
    ProjectInfo {
        root: root.into(),
        file: Path::new(root)
            .join(".codemori/shared.json")
            .to_string_lossy()
            .into_owned(),
        exists: Path::new(root).join(".codemori/shared.json").exists(),
        version: "unavailable".into(),
        error: Some(error.to_string()),
    }
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut temporary = tempfile::Builder::new()
        .prefix(".shared-")
        .suffix(".tmp")
        .tempfile_in(path.parent().unwrap())?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|e| Error::Io(e.error))?;
    Ok(())
}
pub(crate) fn lock(root: &str) -> Result<File, Error> {
    let root = PathBuf::from(canonical_root(root)?);
    let path = project_path(&root)?;
    let directory = path.parent().unwrap();
    fs::create_dir_all(directory)?;
    plain_path(directory, true)?;
    let lock_path = directory.join(".shared.lock");
    plain_path(&lock_path, false)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    let started = Instant::now();
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => break,
            Err(e) if e.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
                if started.elapsed() >= Duration::from_secs(5) {
                    return Err(Error::ProjectBusy);
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(e.into()),
        }
    }
    let ignore = directory.join(".gitignore");
    plain_path(&ignore, false)?;
    let mut text = match fs::read_to_string(&ignore) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let before = text.clone();
    for pattern in [".shared.lock", ".shared-*.tmp"] {
        if !text.lines().any(|line| line == pattern) {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(pattern);
            text.push('\n');
        }
    }
    if text != before {
        atomic_write(&ignore, text.as_bytes())?;
    }
    Ok(file)
}
fn check_version(snapshot: &Snapshot, expected: Option<&str>, required: bool) -> Result<(), Error> {
    if required && expected.is_none() {
        return Err(invalid("Project writes require the observed file version"));
    }
    if expected.is_some_and(|v| v != snapshot.info.version) {
        return Err(Error::ProjectConflict);
    }
    Ok(())
}
pub fn save(
    personal: &Store,
    root: &str,
    expected: &str,
    observed: Option<(String, u64)>,
    input: RecordInput,
    binding_path: Option<String>,
    binding_kind: BindingKind,
) -> Result<Value, Error> {
    let actor = crate::profile::require(
        personal
            .data_dir
            .as_deref()
            .ok_or_else(|| invalid("Personal identity unavailable"))?,
    )?;
    let _lock = lock(root)?;
    let mut snapshot = Snapshot::load(root)?;
    snapshot.store.review_author = Some(actor.clone());
    check_version(&snapshot, Some(expected), true)?;
    let input = snapshot.prepare_input(personal, input)?;
    let editing = observed.is_some();
    let record = match observed {
        None => snapshot.store.create(input)?,
        Some((id, revision)) => snapshot.store.update(&id, revision, input)?,
    };
    if let Some(path) = binding_path {
        snapshot.store.link(Binding {
            kind: binding_kind,
            review: None,
            workspace_id: PROJECT_WORKSPACE.into(),
            path,
            document_id: record.id.clone(),
        })?;
    }
    let is_new = !snapshot.authors.contains_key(&record.id);
    if is_new || editing {
        let metadata = snapshot
            .authors
            .entry(record.id.clone())
            .or_insert((Some(actor.clone()), None));
        metadata.1 = Some(actor);
    }
    snapshot.write()?;
    snapshot.view(serde_json::to_value(record)?)
}
pub fn execute(
    personal: &Store,
    root: &str,
    expected: Option<&str>,
    mut request: Request,
) -> Result<Value, Error> {
    let write = match &request {
        Request::RecordCreate { .. }
        | Request::RecordUpdate { .. }
        | Request::RecordDelete { .. }
        | Request::DocumentLink { .. }
        | Request::DocumentUnlink { .. }
        | Request::BindingMove { .. }
        | Request::BindingReview { .. }
        | Request::BindingsReview { .. } => true,
        Request::PathsRepair { token, .. } => token.is_some(),
        Request::RecordGet { .. }
        | Request::Search { .. }
        | Request::Tags { .. }
        | Request::FileDocuments { .. }
        | Request::DocumentBindings { .. }
        | Request::SourceTarget { .. }
        | Request::DocumentTarget { .. }
        | Request::DocumentOpenTargets { .. }
        | Request::MarkdownPreview { .. }
        | Request::BindingChanges { .. } => false,
        _ => {
            return Err(invalid(
                "This operation is not supported in the project shared library",
            ));
        }
    };
    let actor = if write {
        Some(crate::profile::require(
            personal
                .data_dir
                .as_deref()
                .ok_or_else(|| invalid("Personal identity unavailable"))?,
        )?)
    } else {
        None
    };
    let editing = matches!(&request, Request::RecordUpdate { .. });
    let creating = matches!(&request, Request::RecordCreate { .. });
    let _lock = if write { Some(lock(root)?) } else { None };
    let mut snapshot = Snapshot::load(root)?;
    snapshot.store.review_author = actor.clone();
    check_version(&snapshot, expected, write)?;
    match &mut request {
        Request::RecordCreate { record } | Request::RecordUpdate { record, .. } => {
            *record = snapshot.prepare_input(personal, record.clone())?
        }
        Request::DocumentLink { binding }
        | Request::DocumentUnlink { binding }
        | Request::BindingMove { binding, .. }
        | Request::BindingReview { binding, .. }
        | Request::BindingChanges { binding } => binding.workspace_id = PROJECT_WORKSPACE.into(),
        Request::BindingsReview { items } => {
            for item in items {
                item.binding.workspace_id = PROJECT_WORKSPACE.into();
            }
        }
        Request::PathsRepair { workspace_id, .. } => *workspace_id = PROJECT_WORKSPACE.into(),
        Request::FileDocuments { workspace_id, .. } => *workspace_id = PROJECT_WORKSPACE.into(),
        Request::SourceTarget { source } => source.workspace_id = PROJECT_WORKSPACE.into(),
        Request::Search { filter } => {
            filter.workspace_id = None; // Every record in this file belongs to this project.
        }
        _ => {}
    }
    let mut result = crate::rpc::dispatch(&mut snapshot.store, request)?;
    if write {
        if let Some(ids) = result.get("record_ids").and_then(Value::as_array) {
            for id in ids.iter().filter_map(Value::as_str) {
                if let Some(metadata) = snapshot.authors.get_mut(id) {
                    metadata.1 = actor.clone();
                }
            }
        }
        if editing || creating {
            let id = result["id"]
                .as_str()
                .ok_or_else(|| invalid("Missing saved record ID"))?;
            if editing || !snapshot.authors.contains_key(id) {
                let metadata = snapshot
                    .authors
                    .entry(id.into())
                    .or_insert((actor.clone(), None));
                metadata.1 = actor;
            }
        }
        snapshot.write()?;
        if let Some(object) = result.as_object_mut() {
            object.insert("project_version".into(), json!(snapshot.info.version));
        }
    }
    snapshot.view(result)
}
