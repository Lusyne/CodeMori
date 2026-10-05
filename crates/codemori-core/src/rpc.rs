use crate::{Error, PROTOCOL_VERSION, RuntimeInfo, Store, model::*};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcEnvelope {
    pub protocol_version: u32,
    pub request: Request,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    BindingsReview {
        items: Vec<ReviewConfirmation>,
    },
    CodeSearch {
        root: String,
        query: String,
        under: Option<String>,
        limit: Option<usize>,
    },
    CodeRead {
        root: String,
        path: String,
        start_line: Option<usize>,
        end_line: Option<usize>,
    },
    AiContext {
        root: String,
        query: Option<String>,
        path: Option<String>,
    },
    BindingReview {
        binding: Binding,
        fingerprint: String,
        document_revision: u64,
    },
    BindingChanges {
        binding: Binding,
    },
    PathsRepair {
        workspace_id: String,
        from: String,
        to: String,
        token: Option<String>,
    },
    IdentityGet,
    IdentitySet {
        display_name: String,
    },
    CodeLinkCreate {
        root: String,
        path: String,
        line: u32,
        #[serde(default = "default_product")]
        jetbrains_product: String,
        #[serde(default = "default_scheme")]
        vscode_scheme: String,
    },
    CodeLinkParse {
        url: String,
    },
    CodeLinkResolve {
        root: String,
        url: String,
    },
    ProjectInfo {
        root: String,
    },
    ProjectSave {
        root: String,
        expected_version: String,
        id: Option<String>,
        revision: Option<u64>,
        record: RecordInput,
        binding_path: Option<String>,
        #[serde(default)]
        binding_kind: BindingKind,
    },
    Project {
        root: String,
        expected_version: Option<String>,
        request: Box<Request>,
    },
    LibrarySearch {
        root: Option<String>,
        #[serde(default)]
        scope: crate::library::LibraryScope,
        #[serde(default)]
        filter: SearchFilter,
    },
    LibraryTags {
        root: Option<String>,
        #[serde(default)]
        scope: crate::library::LibraryScope,
        #[serde(default)]
        demo: bool,
    },
    LibraryFileDocuments {
        root: String,
        workspace_id: String,
        path: String,
    },
    WorkspaceRegister {
        root: String,
        #[serde(default)]
        name: Option<String>,
    },
    WorkspaceList,
    WorkspaceRelocate {
        id: String,
        root: String,
        revision: u64,
    },
    RecordCreate {
        record: RecordInput,
    },
    RecordGet {
        id: String,
    },
    RecordUpdate {
        id: String,
        revision: u64,
        record: RecordInput,
    },
    RecordDelete {
        id: String,
        revision: u64,
    },
    Search {
        #[serde(default)]
        filter: SearchFilter,
    },
    Tags {
        #[serde(default)]
        demo: bool,
    },
    DocumentLink {
        binding: Binding,
    },
    DocumentUnlink {
        binding: Binding,
    },
    FileDocuments {
        workspace_id: String,
        path: String,
    },
    DocumentBindings {
        document_id: String,
    },
    BindingMove {
        binding: Binding,
        path: String,
    },
    SourceTarget {
        source: Source,
    },
    DocumentTarget {
        id: String,
        revision: u64,
    },
    DocumentOpenTargets {
        id: String,
        revision: u64,
    },
    MarkdownPreview {
        id: String,
        revision: u64,
    },
    CommentsIndex {
        workspace_id: String,
        path: Option<String>,
    },
    CommentsStatus {
        workspace_id: String,
    },
    CommentsTarget {
        source: Source,
    },
    CommentsSearch {
        #[serde(default)]
        filter: crate::index::CommentFilter,
    },
    BackupExport,
    BackupPreview {
        backup: Value,
    },
    BackupImport {
        backup: Backup,
    },
}

fn default_product() -> String {
    "idea".into()
}
fn default_scheme() -> String {
    "vscode".into()
}

pub fn execute(info: &RuntimeInfo, envelope: RpcEnvelope) -> Result<Value, Error> {
    if envelope.protocol_version != PROTOCOL_VERSION {
        return Err(Error::Validation(
            "Unsupported input protocol version".into(),
        ));
    }
    match &envelope.request {
        Request::CodeSearch {
            root,
            query,
            under,
            limit,
        } => {
            return crate::ai::search(
                root,
                query,
                under.as_deref().unwrap_or("."),
                limit.unwrap_or(20),
            );
        }
        Request::CodeRead {
            root,
            path,
            start_line,
            end_line,
        } => return crate::ai::read(root, path, start_line.unwrap_or(1), *end_line),
        Request::IdentityGet => {
            return Ok(json!({"author":crate::profile::get(&info.data_dir)?}));
        }
        Request::IdentitySet { display_name } => {
            return Ok(
                json!({"author":crate::profile::set(&info.data_dir, display_name.clone())?}),
            );
        }
        Request::ProjectInfo { root } => {
            return Ok(serde_json::to_value(crate::project::info(root))?);
        }
        Request::CodeLinkCreate {
            root,
            path,
            line,
            jetbrains_product,
            vscode_scheme,
        } => return crate::code_link::create(root, path, *line, jetbrains_product, vscode_scheme),
        Request::CodeLinkParse { url } => {
            return Ok(serde_json::to_value(crate::code_link::parse(url)?)?);
        }
        Request::CodeLinkResolve { root, url } => {
            return Ok(json!({"target":crate::code_link::resolve(root, url)?}));
        }
        _ => {}
    }
    let mut store = Store::open(info)?;
    dispatch(&mut store, envelope.request)
}

pub(crate) fn dispatch(store: &mut Store, request: Request) -> Result<Value, Error> {
    let value = match request {
        Request::BindingsReview { items } => {
            let bindings = store.confirm_reviews(items)?;
            json!({"confirmed":bindings.len(),"bindings":bindings})
        }
        Request::AiContext { root, query, path } => crate::ai::context(
            store,
            &root,
            query.as_deref().unwrap_or(""),
            path.as_deref(),
        )?,
        Request::CodeSearch { .. } | Request::CodeRead { .. } => {
            return Err(Error::Validation("Use top-level AI source requests".into()));
        }
        Request::BindingReview {
            binding,
            fingerprint,
            document_revision,
        } => {
            serde_json::to_value(store.confirm_review(binding, &fingerprint, document_revision)?)?
        }
        Request::BindingChanges { binding } => store.binding_changes(&binding)?,
        Request::PathsRepair {
            workspace_id,
            from,
            to,
            token,
        } => store.repair_paths(&workspace_id, &from, &to, token.as_deref())?,
        Request::IdentityGet
        | Request::IdentitySet { .. }
        | Request::CodeLinkCreate { .. }
        | Request::CodeLinkParse { .. }
        | Request::CodeLinkResolve { .. } => {
            return Err(Error::Validation(
                "This operation requires a top-level RPC".into(),
            ));
        }
        Request::ProjectInfo { root } => serde_json::to_value(crate::project::info(&root))?,
        Request::ProjectSave {
            root,
            expected_version,
            id,
            revision,
            record,
            binding_path,
            binding_kind,
        } => {
            let observed = match (id, revision) {
                (None, None) => None,
                (Some(id), Some(revision)) => Some((id, revision)),
                _ => {
                    return Err(Error::Validation(
                        "Existing project records require both ID and revision".into(),
                    ));
                }
            };
            crate::project::save(
                store,
                &root,
                &expected_version,
                observed,
                record,
                binding_path,
                binding_kind,
            )?
        }
        Request::Project {
            root,
            expected_version,
            request,
        } => crate::project::execute(store, &root, expected_version.as_deref(), *request)?,
        Request::LibrarySearch {
            root,
            scope,
            filter,
        } => crate::library::search(store, root.as_deref(), scope, filter)?,
        Request::LibraryTags { root, scope, demo } => {
            crate::library::tags(store, root.as_deref(), scope, demo)?
        }
        Request::LibraryFileDocuments {
            root,
            workspace_id,
            path,
        } => crate::library::file_documents(store, &root, &workspace_id, &path)?,
        Request::WorkspaceRegister { root, name } => {
            serde_json::to_value(store.register_workspace(&root, name)?)?
        }
        Request::WorkspaceList => serde_json::to_value(store.workspaces()?)?,
        Request::WorkspaceRelocate { id, root, revision } => {
            serde_json::to_value(store.relocate_workspace(&id, &root, revision)?)?
        }
        Request::RecordCreate { record } => serde_json::to_value(store.create(record)?)?,
        Request::RecordGet { id } => serde_json::to_value(store.get(&id)?)?,
        Request::RecordUpdate {
            id,
            revision,
            record,
        } => serde_json::to_value(store.update(&id, revision, record)?)?,
        Request::RecordDelete { id, revision } => {
            store.delete(&id, revision)?;
            json!({"deleted":true})
        }
        Request::Search { filter } => serde_json::to_value(store.search(filter)?)?,
        Request::Tags { demo } => serde_json::to_value(store.tags(demo)?)?,
        Request::DocumentLink { binding } => serde_json::to_value(store.link(binding)?)?,
        Request::DocumentUnlink { binding } => {
            store.unlink(binding)?;
            json!({"unlinked":true})
        }
        Request::FileDocuments { workspace_id, path } => {
            serde_json::to_value(store.file_documents(&workspace_id, &path)?)?
        }
        Request::DocumentBindings { document_id } => {
            serde_json::to_value(store.document_bindings(&document_id)?)?
        }
        Request::BindingMove { binding, path } => {
            serde_json::to_value(store.move_binding(binding, &path)?)?
        }
        Request::SourceTarget { source } => serde_json::to_value(store.source_target(&source)?)?,
        Request::DocumentTarget { id, revision } => {
            serde_json::to_value(store.document_target(&id, revision)?)?
        }
        Request::DocumentOpenTargets { id, revision } => {
            serde_json::to_value(store.document_open_targets(&id, revision)?)?
        }
        Request::MarkdownPreview { id, revision } => {
            serde_json::to_value(store.markdown_preview(&id, revision)?)?
        }
        Request::CommentsIndex { workspace_id, path } => {
            serde_json::to_value(store.index_comments(&workspace_id, path.as_deref())?)?
        }
        Request::CommentsStatus { workspace_id } => {
            serde_json::to_value(store.comment_index_status(&workspace_id)?)?
        }
        Request::CommentsTarget { source } => serde_json::to_value(store.comment_target(&source)?)?,
        Request::CommentsSearch { filter } => serde_json::to_value(store.search_comments(filter)?)?,
        Request::BackupExport => serde_json::to_value(store.export_backup()?)?,
        Request::BackupPreview { backup } => serde_json::to_value(store.preview_backup(backup)?)?,
        Request::BackupImport { backup } => {
            serde_json::to_value(store.import_backup(backup, false)?)?
        }
    };
    Ok(value)
}
