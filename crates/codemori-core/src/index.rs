//! Rebuildable, comment-only source index. Parsing never persists source bodies.
use crate::{
    Error, Store,
    model::{Source, TextSpan},
    store::{now, relative_path, workspace_by_id},
};
use ignore::WalkBuilder;
use rusqlite::{OptionalExtension, TransactionBehavior, params, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs::File, io::Read, path::Path};

pub(crate) const SCHEMA: &str = "
CREATE TABLE indexed_files (
 workspace_id TEXT NOT NULL REFERENCES workspaces(id), path TEXT NOT NULL,
 fingerprint TEXT NOT NULL, language TEXT NOT NULL, indexed_at INTEGER NOT NULL,
 PRIMARY KEY(workspace_id,path)
);
CREATE TABLE comments (
 workspace_id TEXT NOT NULL, path TEXT NOT NULL, line INTEGER NOT NULL,
 end_line INTEGER NOT NULL, text TEXT NOT NULL, text_key TEXT NOT NULL,
 FOREIGN KEY(workspace_id,path) REFERENCES indexed_files(workspace_id,path) ON DELETE CASCADE
);
CREATE INDEX comments_source ON comments(workspace_id,path,line);
";
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Serialize, Default)]
pub struct IndexReport {
    pub indexed_files: usize,
    pub unchanged_files: usize,
    pub removed_files: usize,
    pub skipped_files: usize,
    pub failed_files: usize,
    /// Bounded diagnostics; counts above include all visited files.
    pub details: Vec<String>,
    pub status: IndexStatus,
}
#[derive(Debug, Serialize, Default)]
pub struct IndexStatus {
    pub files: usize,
    pub comments: usize,
    pub indexed_at: Option<i64>,
}
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CommentFilter {
    #[serde(default)]
    pub query: String,
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
}
#[derive(Debug, Serialize)]
pub struct CommentHit {
    pub source: Source,
    pub end_line: u32,
    pub text: String,
    pub language: String,
    pub workspace_name: String,
    pub indexed_at: i64,
    pub excerpt: Vec<TextSpan>,
}
#[derive(Debug, Serialize)]
pub struct CommentPage {
    pub items: Vec<CommentHit>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

impl IndexReport {
    fn detail(&mut self, message: String) {
        if self.details.len() < 100 {
            self.details.push(message);
        }
    }
}

impl Store {
    pub fn comment_target(&self, source: &Source) -> Result<std::path::PathBuf, Error> {
        let path = self.source_target(source)?;
        let expected: Option<String> = self
            .db
            .query_row(
                "SELECT fingerprint FROM indexed_files WHERE workspace_id=?1 AND path=?2",
                params![source.workspace_id, relative_path(&source.path)?],
                |r| r.get(0),
            )
            .optional()?;
        let workspace = workspace_by_id(&self.db, &source.workspace_id)?;
        let bytes = read_source(&path, &Path::new(&workspace.root).canonicalize()?)?;
        if expected.as_deref() != Some(&format!("comments-v1:{:x}", Sha256::digest(&bytes))) {
            return Err(Error::Validation(
                "Comment index is stale; reindex the saved file before navigating".into(),
            ));
        }
        Ok(path)
    }
    /// None scans the workspace; Some scans only that file, still respecting ignore rules.
    /// The immediate transaction serializes concurrent rebuilds and rolls back on cancellation.
    pub fn index_comments(
        &mut self,
        workspace_id: &str,
        path: Option<&str>,
    ) -> Result<IndexReport, Error> {
        let target = path.map(relative_path).transpose()?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let workspace = workspace_by_id(&tx, workspace_id)?;
        let root = Path::new(&workspace.root).canonicalize()?;
        if !root.is_dir() {
            return Err(Error::Validation(
                "Workspace root must be a directory".into(),
            ));
        }
        let previous = {
            let mut stmt = tx.prepare("SELECT path FROM indexed_files WHERE workspace_id=?1")?;
            stmt.query_map([workspace_id], |r| r.get::<_, String>(0))?
                .collect::<Result<HashSet<_>, _>>()?
        };
        let mut seen = HashSet::new();
        let mut report = IndexReport::default();
        // Git's and .ignore's rules apply even without a .git directory. Never follow symlinks.
        let walker = WalkBuilder::new(&root)
            .require_git(false)
            .follow_links(false)
            .filter_entry(|entry| {
                entry.depth() == 0
                    || !entry.file_type().is_some_and(|t| t.is_dir())
                    || !matches!(
                        entry.file_name().to_str(),
                        Some(
                            "node_modules" | "target" | "build" | "dist" | "vendor" | "__pycache__"
                        )
                    )
            })
            .build();
        for entry in walker {
            // Traversal failure aborts the batch: an unreadable subtree is not a deleted subtree.
            let entry =
                entry.map_err(|e| Error::Validation(format!("Index traversal failed: {e}")))?;
            if let Some(error) = entry.error() {
                return Err(Error::Validation(format!("Invalid ignore rules: {error}")));
            }
            if entry.file_type().is_some_and(|t| t.is_dir()) {
                continue;
            }
            let Some(relative) = entry
                .path()
                .strip_prefix(&root)
                .ok()
                .and_then(|p| p.to_str())
            else {
                report.skipped_files += 1;
                report.detail("Skipped non-UTF-8 path".into());
                continue;
            };
            let relative = relative.replace('\\', "/");
            if target.as_ref().is_some_and(|wanted| wanted != &relative) {
                continue;
            }
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                report.skipped_files += 1;
                report.detail(format!("{relative}: symlink or non-regular file"));
                continue;
            }
            let Some((name, grammar)) = language(entry.path()) else {
                report.skipped_files += 1;
                report.detail(format!("{relative}: unsupported language"));
                continue;
            };
            let result =
                (|| -> Result<(), Error> {
                    let bytes = read_source(entry.path(), &root)?;
                    // Parser revision is part of the fingerprint; bump when extraction rules change.
                    let fingerprint = format!("comments-v1:{:x}", Sha256::digest(&bytes));
                    let existing: Option<String> = tx.query_row(
                    "SELECT fingerprint FROM indexed_files WHERE workspace_id=?1 AND path=?2",
                    params![workspace_id, relative], |r| r.get(0)).optional()?;
                    if existing.as_deref() == Some(&fingerprint) {
                        report.unchanged_files += 1;
                        seen.insert(relative.clone());
                        return Ok(());
                    }
                    let source = std::str::from_utf8(&bytes)
                        .map_err(|_| Error::Validation("Source must be UTF-8".into()))?;
                    let comments = extract(source, grammar)?;
                    tx.execute(
                        "DELETE FROM indexed_files WHERE workspace_id=?1 AND path=?2",
                        params![workspace_id, relative],
                    )?;
                    tx.execute(
                        "INSERT INTO indexed_files VALUES (?1,?2,?3,?4,?5)",
                        params![workspace_id, relative, fingerprint, name, now()],
                    )?;
                    for comment in comments {
                        tx.execute(
                            "INSERT INTO comments VALUES (?1,?2,?3,?4,?5,?6)",
                            params![
                                workspace_id,
                                relative,
                                comment.line,
                                comment.end_line,
                                comment.text,
                                comment.text.to_lowercase()
                            ],
                        )?;
                    }
                    seen.insert(relative.clone());
                    report.indexed_files += 1;
                    Ok(())
                })();
            if let Err(error) = result {
                // A storage failure must roll back the batch, not leave a half-written file.
                if matches!(error, Error::Storage(_)) {
                    return Err(error);
                }
                report.failed_files += 1;
                report.detail(format!("{relative}: {error}"));
            }
        }
        // Also clears entries that became ignored, unsupported, invalid, or disappeared.
        for removed in previous
            .difference(&seen)
            .filter(|p| target.as_ref().is_none_or(|t| t == *p))
        {
            tx.execute(
                "DELETE FROM indexed_files WHERE workspace_id=?1 AND path=?2",
                params![workspace_id, removed],
            )?;
            report.removed_files += 1;
        }
        if let Some(target) = target.filter(|p| !seen.contains(p)) {
            report.detail(format!(
                "{target}: not indexed; missing, ignored, unsupported or failed"
            ));
        }
        tx.commit()?;
        report.status = self.comment_index_status(workspace_id)?;
        Ok(report)
    }

    pub fn comment_index_status(&self, workspace_id: &str) -> Result<IndexStatus, Error> {
        workspace_by_id(&self.db, workspace_id)?;
        let (files, indexed_at) = self.db.query_row(
            "SELECT count(*), max(indexed_at) FROM indexed_files WHERE workspace_id=?1",
            [workspace_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let comments = self.db.query_row(
            "SELECT count(*) FROM comments WHERE workspace_id=?1",
            [workspace_id],
            |r| r.get(0),
        )?;
        Ok(IndexStatus {
            files,
            comments,
            indexed_at,
        })
    }

    pub fn search_comments(&self, filter: CommentFilter) -> Result<CommentPage, Error> {
        let limit = filter.limit.unwrap_or(50);
        if !(1..=200).contains(&limit) || filter.offset > crate::MAX_JSON_INTEGER as usize {
            return Err(Error::Validation(
                "Comment search limit must be 1..200 and offset valid".into(),
            ));
        }
        let terms: Vec<String> = filter
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let mut clauses = vec!["1=1".to_string()];
        let mut values: Vec<Value> = vec![];
        if let Some(workspace) = filter.workspace_id {
            workspace_by_id(&self.db, &workspace)?;
            clauses.push("c.workspace_id=?".into());
            values.push(workspace.into());
        }
        for term in &terms {
            clauses.push("instr(c.text_key,?)>0".into());
            values.push(term.clone().into());
        }
        let clause = clauses.join(" AND ");
        let total = self.db.query_row(
            &format!("SELECT count(*) FROM comments c WHERE {clause}"),
            params_from_iter(values.iter()),
            |r| r.get(0),
        )?;
        values.push((limit as i64).into());
        values.push((filter.offset as i64).into());
        let mut stmt = self.db.prepare(&format!("SELECT c.workspace_id,c.path,c.line,c.end_line,c.text,f.language,json_extract(w.body,'$.name'),f.indexed_at
            FROM comments c JOIN indexed_files f ON f.workspace_id=c.workspace_id AND f.path=c.path JOIN workspaces w ON w.id=c.workspace_id
            WHERE {clause} ORDER BY f.indexed_at DESC,c.workspace_id,c.path,c.line LIMIT ? OFFSET ?"))?;
        let items = stmt
            .query_map(params_from_iter(values.iter()), |r| {
                let text: String = r.get(4)?;
                Ok(CommentHit {
                    source: Source {
                        workspace_id: r.get(0)?,
                        path: r.get(1)?,
                        line: Some(r.get(2)?),
                    },
                    end_line: r.get(3)?,
                    excerpt: crate::search::excerpt(&text, &terms),
                    text,
                    language: r.get(5)?,
                    workspace_name: r.get(6)?,
                    indexed_at: r.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CommentPage {
            items,
            total,
            limit,
            offset: filter.offset,
        })
    }
}

fn read_source(path: &Path, root: &Path) -> Result<Vec<u8>, Error> {
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(root) {
        return Err(Error::Validation(
            "Source resolves outside workspace".into(),
        ));
    }
    let metadata = canonical.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(Error::Validation(
            "Source must be a regular file of at most 2 MiB".into(),
        ));
    }
    let mut bytes = Vec::new();
    File::open(canonical)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::Validation("Source exceeds 2 MiB".into()));
    }
    Ok(bytes)
}
pub(crate) fn language(path: &Path) -> Option<(&'static str, tree_sitter::Language)> {
    Some(match path.extension()?.to_str()? {
        "java" => ("java", tree_sitter_java::LANGUAGE.into()),
        "js" | "jsx" | "mjs" | "cjs" => ("javascript", tree_sitter_javascript::LANGUAGE.into()),
        "ts" | "mts" | "cts" => (
            "typescript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        ),
        "tsx" => ("tsx", tree_sitter_typescript::LANGUAGE_TSX.into()),
        "py" | "pyi" => ("python", tree_sitter_python::LANGUAGE.into()),
        _ => return None,
    })
}
struct ParsedComment {
    line: u32,
    end_line: u32,
    text: String,
}
fn extract(source: &str, language: tree_sitter::Language) -> Result<Vec<ParsedComment>, Error> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .map_err(|e| Error::Validation(e.to_string()))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| Error::Validation("Parser cancelled".into()))?;
    // Fail closed: malformed source can make a parser misclassify source text as comments.
    if tree.root_node().has_error() {
        return Err(Error::Validation(
            "Syntax error; save valid source and retry indexing".into(),
        ));
    }
    let mut cursor = tree.walk();
    let mut comments = Vec::new();
    loop {
        let node = cursor.node();
        if matches!(node.kind(), "comment" | "line_comment" | "block_comment") {
            comments.push(ParsedComment {
                line: node.start_position().row as u32 + 1,
                end_line: node.end_position().row as u32 + 1,
                text: source[node.byte_range()].to_string(),
            });
        } else if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Ok(comments);
            }
        }
    }
}
