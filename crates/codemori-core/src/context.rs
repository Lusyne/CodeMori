//! File/module associations and explicit review of saved code. No source bodies are persisted.
use crate::{
    Error, Store,
    model::*,
    profile::Author,
    store::{now, record_by_id, relative_path, workspace_by_id},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn invalid(message: &str) -> Error {
    Error::Validation(message.into())
}
pub(crate) fn binding_path(path: &str, kind: BindingKind) -> Result<String, Error> {
    if kind == BindingKind::Module && path == "." {
        Ok(".".into())
    } else {
        relative_path(path)
    }
}
pub(crate) fn validate_binding(b: &Binding) -> Result<(), Error> {
    if binding_path(&b.path, b.kind)? != b.path {
        return Err(invalid("Association path is not normalized"));
    }
    if let Some(r) = &b.review {
        if r.fingerprint.len() != 64
            || !r.fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
            || r.confirmed_at < 0
            || r.confirmed_at > crate::MAX_JSON_INTEGER as i64
        {
            return Err(invalid("Invalid review fingerprint or timestamp"));
        }
        binding_path(&r.path, b.kind)?;
        if let Some(author) = &r.confirmed_by {
            author.validate()?;
        }
        if r.git_commit.as_ref().is_some_and(|c| {
            !matches!(c.len(), 40 | 64) || !c.bytes().all(|b| b.is_ascii_hexdigit())
        }) {
            return Err(invalid("Invalid review commit"));
        }
    }
    Ok(())
}
pub(crate) fn insert_binding(db: &Connection, b: &Binding) -> Result<(), Error> {
    validate_binding(b)?;
    db.execute(
        "INSERT INTO bindings(workspace_id,path,document_id,kind,review) VALUES (?1,?2,?3,?4,?5)",
        params![
            b.workspace_id,
            b.path,
            b.document_id,
            b.kind.key(),
            b.review.as_ref().map(serde_json::to_string).transpose()?
        ],
    )?;
    Ok(())
}
pub(crate) fn all_bindings(db: &Connection) -> Result<Vec<Binding>, Error> {
    let mut stmt=db.prepare("SELECT workspace_id,path,document_id,kind,review FROM bindings ORDER BY workspace_id,path,document_id")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(workspace_id, path, document_id, kind, review)| {
            Ok(Binding {
                workspace_id,
                path,
                document_id,
                kind: serde_json::from_value(json!(kind))?,
                review: review.map(|s| serde_json::from_str(&s)).transpose()?,
            })
        })
        .collect()
}
pub(crate) fn stored_binding(db: &Connection, b: &Binding) -> Result<Binding, Error> {
    let row = db
        .query_row(
            "SELECT kind,review FROM bindings WHERE workspace_id=?1 AND path=?2 AND document_id=?3",
            params![b.workspace_id, b.path, b.document_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .optional()?;
    let (kind, review) = row.ok_or_else(|| Error::NotFound("Association".into()))?;
    Ok(Binding {
        workspace_id: b.workspace_id.clone(),
        path: b.path.clone(),
        document_id: b.document_id.clone(),
        kind: serde_json::from_value(json!(kind))?,
        review: review.map(|s| serde_json::from_str(&s)).transpose()?,
    })
}
pub(crate) fn safe_target(root: &Path, path: &str, kind: BindingKind) -> Result<PathBuf, Error> {
    let relative = binding_path(path, kind)?;
    let mut result = root.to_path_buf();
    if relative != "." {
        for part in Path::new(&relative).components() {
            result.push(part);
            if fs::symlink_metadata(&result)?.file_type().is_symlink() {
                return Err(invalid("Association targets cannot follow symbolic links"));
            }
        }
    }
    if (kind.is_file() && !result.is_file()) || (!kind.is_file() && !result.is_dir()) {
        return Err(invalid(
            "Association target has the wrong file/directory type",
        ));
    }
    Ok(result)
}
pub(crate) fn read_text(path: &Path) -> Result<String, Error> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(invalid("Source file exceeds 2 MiB"));
    }
    String::from_utf8(bytes)
        .map(|s| s.replace("\r\n", "\n"))
        .map_err(|_| invalid("Source must be UTF-8 text"))
}
pub(crate) fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
pub(crate) fn fingerprint(root: &str, path: &str, kind: BindingKind) -> Result<String, Error> {
    let root = Path::new(root).canonicalize()?;
    let target = safe_target(&root, path, kind)?;
    if kind.is_file() {
        return Ok(digest(&read_text(&target)?));
    }
    let mut hasher = Sha256::new();
    let mut bytes = 0;
    let mut checked = 0;
    for file in crate::anchors::files(&root)? {
        if !file.starts_with(&target) || !is_code(&file) {
            continue;
        }
        let text = read_text(&file)?;
        checked += 1;
        bytes += text.len();
        if bytes > 64 * 1024 * 1024 {
            return Err(invalid("Module review exceeds 64 MiB"));
        }
        let relative = file
            .strip_prefix(&target)
            .map_err(|_| invalid("Invalid module path"))?
            .to_string_lossy()
            .replace('\\', "/");
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(digest(&text).as_bytes());
        hasher.update([0]);
    }
    if checked == 0 {
        return Err(invalid(
            "Module has no non-ignored supported source files to review",
        ));
    }
    Ok(format!("{:x}", hasher.finalize()))
}
fn is_code(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|v| v.to_str()),
        Some(
            "java"
                | "kt"
                | "kts"
                | "js"
                | "jsx"
                | "mjs"
                | "cjs"
                | "ts"
                | "tsx"
                | "py"
                | "pyi"
                | "rs"
                | "go"
                | "c"
                | "h"
                | "cpp"
                | "hpp"
                | "cc"
                | "cs"
                | "rb"
                | "php"
                | "swift"
                | "scala"
                | "vue"
                | "svelte"
                | "sql"
                | "sh"
        )
    )
}
// Read-only Git, no shell, external diff/textconv/fsmonitor, hooks or network operations.
fn git(root: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new("git")
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "core.fsmonitor=false",
            "-C",
            root,
        ])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let output = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        output
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let started = Instant::now();
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if started.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let bytes = reader.join().ok()?.ok()?;
    if !success || bytes.len() > 1024 * 1024 {
        return None;
    }
    String::from_utf8(bytes).ok()
}
pub(crate) fn capture_review(
    root: &str,
    path: &str,
    kind: BindingKind,
    actor: Option<Author>,
) -> Result<Review, Error> {
    let fingerprint = fingerprint(root, path, kind)?;
    let tracked =
        !kind.is_file() || git(root, &["ls-files", "--error-unmatch", "--", path]).is_some();
    let commit = git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=normal",
            "--",
            path,
        ],
    )
    .filter(|s| tracked && s.is_empty())
    .and_then(|_| git(root, &["rev-parse", "--verify", "HEAD^{commit}"]))
    .map(|s| s.trim().to_string());
    Ok(Review {
        fingerprint,
        confirmed_at: now(),
        confirmed_by: actor,
        git_commit: commit,
        path: path.into(),
    })
}
fn review_status(b: &Binding, current: &Result<String, String>) -> Value {
    match current {
        Ok(value) => {
            json!({"status":match &b.review{None=>"unconfirmed",Some(r) if &r.fingerprint==value=>"current",_=>"needs_review"},"fingerprint":value,"error":null})
        }
        Err(error) => json!({"status":"unavailable","fingerprint":null,"error":error}),
    }
}
impl Store {
    pub(crate) fn review_actor(&self) -> Result<Option<Author>, Error> {
        if self.review_author.is_some() {
            return Ok(self.review_author.clone());
        }
        self.data_dir
            .as_deref()
            .map(crate::profile::get)
            .transpose()
            .map(Option::flatten)
    }
    pub fn effective_bindings(
        &self,
        workspace_id: &str,
        path: &str,
    ) -> Result<Vec<Binding>, Error> {
        let path = relative_path(path)?;
        workspace_by_id(&self.db, workspace_id)?;
        Ok(all_bindings(&self.db)?
            .into_iter()
            .filter(|b| {
                b.workspace_id == workspace_id
                    && if b.kind.is_file() {
                        b.path == path
                    } else {
                        b.path == "." || path.starts_with(&(b.path.clone() + "/"))
                    }
            })
            .collect())
    }
    pub fn review_state(&self, b: &Binding) -> Value {
        let result = workspace_by_id(&self.db, &b.workspace_id)
            .and_then(|w| fingerprint(&w.root, &b.path, b.kind))
            .map_err(|e| e.to_string());
        review_status(b, &result)
    }
    pub fn context_entries(&self, workspace_id: &str, path: &str) -> Result<Vec<Value>, Error> {
        let root = workspace_by_id(&self.db, workspace_id)?.root;
        let mut cache = std::collections::HashMap::new();
        let mut entries = Vec::new();
        for b in self.effective_bindings(workspace_id, path)? {
            let current = cache
                .entry((b.path.clone(), b.kind))
                .or_insert_with(|| fingerprint(&root, &b.path, b.kind).map_err(|e| e.to_string()));
            entries.push(json!({"record":self.get(&b.document_id)?,"binding":b,"inherited":b.kind==BindingKind::Module,"review_state":review_status(&b,current)}));
        }
        Ok(entries)
    }
    pub fn confirm_review(
        &mut self,
        b: Binding,
        expected: &str,
        document_revision: u64,
    ) -> Result<Binding, Error> {
        let mut confirmed = self.confirm_reviews(vec![ReviewConfirmation {
            binding: b,
            fingerprint: expected.into(),
            document_revision,
        }])?;
        Ok(confirmed.remove(0))
    }
    pub fn confirm_reviews(
        &mut self,
        items: Vec<ReviewConfirmation>,
    ) -> Result<Vec<Binding>, Error> {
        if items.is_empty() || items.len() > 200 {
            return Err(invalid("Choose 1–200 observed associations to confirm"));
        }
        let mut seen = std::collections::HashSet::new();
        let mut baselines = std::collections::HashMap::new();
        let mut prepared = Vec::new();
        let actor = self.review_actor()?;
        for item in &items {
            let b = &item.binding;
            if !seen.insert((&b.workspace_id, &b.path, &b.document_id)) {
                return Err(invalid("Duplicate association in batch confirmation"));
            }
            if stored_binding(&self.db, b)? != *b {
                return Err(Error::BindingConflict);
            }
            let key = (b.workspace_id.clone(), b.path.clone(), b.kind);
            if !baselines.contains_key(&key) {
                let w = workspace_by_id(&self.db, &b.workspace_id)?;
                baselines.insert(
                    key.clone(),
                    capture_review(&w.root, &b.path, b.kind, actor.clone())?,
                );
            }
            let review = baselines.get(&key).unwrap().clone();
            if review.fingerprint != item.fingerprint {
                return Err(Error::BindingConflict);
            }
            prepared.push(Binding {
                review: Some(review),
                ..b.clone()
            });
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for item in &items {
            if stored_binding(&tx, &item.binding)? != item.binding
                || record_by_id(&tx, &item.binding.document_id)?.revision != item.document_revision
            {
                return Err(Error::BindingConflict);
            }
        }
        for updated in &prepared {
            tx.execute("UPDATE bindings SET review=?1 WHERE workspace_id=?2 AND path=?3 AND document_id=?4",params![serde_json::to_string(&updated.review)?,updated.workspace_id,updated.path,updated.document_id])?;
        }
        tx.commit()?;
        Ok(prepared)
    }
    pub fn binding_changes(&self, b: &Binding) -> Result<Value, Error> {
        let current = stored_binding(&self.db, b)?;
        validate_binding(&current)?;
        let w = workspace_by_id(&self.db, &b.workspace_id)?;
        let diff = current.review.as_ref().and_then(|r| {
            r.git_commit.as_ref().and_then(|commit| {
                git(
                    &w.root,
                    &[
                        "diff",
                        "--no-ext-diff",
                        "--no-textconv",
                        commit,
                        "--",
                        &r.path,
                        &b.path,
                    ],
                )
            })
        });
        Ok(
            json!({"binding":current,"review_state":self.review_state(&current),"diff":diff,"note":if diff.is_none(){"差异不可用：没有干净 Git 基线、Git 不可用、超时或差异超过 1 MiB；仅展示内容指纹状态，未保存源码正文。"}else{"差异基于确认时的 Git 提交与当前已保存代码。"}}),
        )
    }
    pub fn repair_paths(
        &mut self,
        workspace_id: &str,
        from: &str,
        to: &str,
        token: Option<&str>,
    ) -> Result<Value, Error> {
        let from = relative_path(from)?;
        let to = relative_path(to)?;
        let w = workspace_by_id(&self.db, workspace_id)?;
        let root = Path::new(&w.root).canonicalize()?;
        if from == to {
            return Err(invalid("Choose a different target path"));
        }
        let map = |p: &str| {
            if p == from {
                Some(to.clone())
            } else {
                p.strip_prefix(&(from.clone() + "/"))
                    .map(|suffix| format!("{to}/{suffix}"))
            }
        };
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let all = all_bindings(&tx)?;
        let mut bindings = Vec::new();
        for old in all.iter().filter(|b| b.workspace_id == workspace_id) {
            if let Some(path) = map(&old.path) {
                safe_target(&root, &path, old.kind)?;
                let new = Binding {
                    path,
                    ..old.clone()
                };
                if all.iter().any(|b| {
                    b.workspace_id == new.workspace_id
                        && b.path == new.path
                        && b.document_id == new.document_id
                }) {
                    return Err(invalid("Repair collides with an existing association"));
                }
                bindings.push((old.clone(), new));
            }
        }
        let mut stmt = tx.prepare("SELECT body FROM records WHERE workspace_id=?1 ORDER BY id")?;
        let rows = stmt
            .query_map([workspace_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        let mut records = Vec::new();
        for row in rows {
            let old: Record = serde_json::from_str(&row)?;
            if let Some(source) = &old.input.source {
                if let Some(path) = map(&source.path) {
                    safe_target(&root, &path, BindingKind::File)?;
                    let mut new = old.clone();
                    new.input.source.as_mut().unwrap().path = path;
                    records.push((old, new));
                }
            }
        }
        let snapshot = json!({"bindings":bindings,"records":records});
        let expected = digest(&serde_json::to_string(&snapshot)?);
        if let Some(token) = token {
            if token != expected {
                return Err(Error::BindingConflict);
            }
            for (old, new) in &bindings {
                tx.execute(
                    "DELETE FROM bindings WHERE workspace_id=?1 AND path=?2 AND document_id=?3",
                    params![old.workspace_id, old.path, old.document_id],
                )?;
                insert_binding(&tx, new)?;
            }
            for (old, new) in &records {
                if old.revision >= crate::MAX_JSON_INTEGER {
                    return Err(invalid("Revision exhausted"));
                }
                let mut new = new.clone();
                new.revision += 1;
                new.updated_at = now()
                    .max(old.updated_at.saturating_add(1))
                    .min(crate::MAX_JSON_INTEGER as i64);
                crate::store::write_record(&tx, &new, false)?;
            }
        }
        tx.commit()?;
        Ok(
            json!({"token":expected,"bindings":bindings.len(),"records":records.len(),"applied":token.is_some(),"from":from,"to":to,"record_ids":records.iter().map(|(r,_)|&r.id).collect::<Vec<_>>()}),
        )
    }
}
