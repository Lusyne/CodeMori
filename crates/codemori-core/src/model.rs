use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Snippet,
    Document,
}

impl RecordKind {
    pub(crate) fn key(&self) -> &'static str {
        match self {
            Self::Snippet => "snippet",
            Self::Document => "document",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub workspace_id: String,
    pub path: String,
    #[serde(default)]
    pub line: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecordInput {
    pub kind: RecordKind,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub starred: bool,
    #[serde(default)]
    pub source: Option<Source>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub is_demo: bool,
    pub input: RecordInput,
}

/// Derived navigation options; never persisted in records or backups.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentOpenTargets {
    pub original_url: String,
    pub feishu_applink: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub workspace_id: String,
    pub path: String,
    pub document_id: String,
    #[serde(default, skip_serializing_if = "BindingKind::is_file")]
    pub kind: BindingKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<Review>,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BindingKind {
    #[default]
    File,
    Module,
}
impl BindingKind {
    pub fn is_file(&self) -> bool {
        *self == Self::File
    }
    pub(crate) fn key(self) -> &'static str {
        if self.is_file() { "file" } else { "module" }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub fingerprint: String,
    pub confirmed_at: i64,
    pub confirmed_by: Option<crate::profile::Author>,
    pub git_commit: Option<String>,
    pub path: String,
}
impl Default for Binding {
    fn default() -> Self {
        Self {
            workspace_id: String::new(),
            path: String::new(),
            document_id: String::new(),
            kind: BindingKind::File,
            review: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backup {
    pub format_version: u32,
    pub exported_at: i64,
    pub workspaces: Vec<Workspace>,
    pub records: Vec<Record>,
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchFilter {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub kind: Option<RecordKind>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub starred: bool,
    #[serde(default)]
    pub demo: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}
fn default_limit() -> usize {
    50
}
impl Default for SearchFilter {
    fn default() -> Self {
        Self {
            query: String::new(),
            kind: None,
            tag: None,
            workspace_id: None,
            starred: false,
            demo: false,
            limit: 50,
            offset: 0,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TextSpan {
    pub text: String,
    pub highlight: bool,
}
#[derive(Debug, Serialize)]
pub struct SearchHit {
    #[serde(skip)]
    pub(crate) rank: u8,
    pub record: Record,
    pub workspace_names: Vec<String>,
    pub excerpt: Vec<TextSpan>,
}
#[derive(Debug, Serialize)]
pub struct SearchPage {
    pub items: Vec<SearchHit>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Serialize, Default)]
pub struct ImportReport {
    pub new_workspaces: usize,
    pub new_records: usize,
    pub new_bindings: usize,
    pub unchanged: usize,
    pub conflicts: Vec<ImportConflict>,
    pub invalid_count: usize,
    pub invalid_entries: Vec<ImportInvalid>,
}
#[derive(Debug, Serialize)]
pub struct ImportInvalid {
    pub kind: String,
    /// Zero-based position in the corresponding backup array.
    pub index: usize,
    pub id: String,
    pub reason: String,
}
#[derive(Debug, Serialize)]
pub struct ImportConflict {
    pub kind: String,
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewConfirmation {
    pub binding: Binding,
    pub fingerprint: String,
    pub document_revision: u64,
}
