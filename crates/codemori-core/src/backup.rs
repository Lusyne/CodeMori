use crate::{
    Error, Store,
    model::*,
    store::{insert_record, insert_workspace, normalize_input, now},
};
use rusqlite::TransactionBehavior;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

impl Store {
    /// Preview reports every invalid entry, while import remains all-or-nothing.
    pub fn preview_backup(&mut self, value: Value) -> Result<ImportReport, Error> {
        let raw: PreviewInput = serde_json::from_value(value)?;
        let mut errors = Vec::new();
        let (workspaces, workspace_positions) =
            parse_entries(raw.workspaces, "workspace", &mut errors);
        let (records, record_positions) = parse_entries(raw.records, "record", &mut errors);
        let (bindings, binding_positions) = parse_entries(raw.bindings, "binding", &mut errors);
        let backup = Backup {
            format_version: raw.format_version,
            exported_at: raw.exported_at,
            workspaces,
            records,
            bindings,
        };
        for mut error in backup_issues(&backup) {
            error.index = match error.kind.as_str() {
                "workspace" => workspace_positions[error.index],
                "record" => record_positions[error.index],
                "binding" => binding_positions[error.index],
                _ => error.index,
            };
            errors.push(error);
        }
        if errors.is_empty() {
            return self.import_backup(backup, true);
        }
        let invalid_count = errors.len();
        errors.truncate(100);
        Ok(ImportReport {
            invalid_count,
            invalid_entries: errors,
            ..Default::default()
        })
    }

    pub fn export_backup(&mut self) -> Result<Backup, Error> {
        // A single snapshot prevents bindings from racing record/workspace changes.
        let tx = self.db.transaction()?;
        let workspaces = load_json(&tx, "SELECT body FROM workspaces ORDER BY id")?;
        let records = load_json(&tx, "SELECT body FROM records ORDER BY id")?;
        let bindings = load_bindings(&tx)?;
        tx.commit()?;
        Ok(Backup {
            format_version: 2,
            exported_at: now(),
            workspaces,
            records,
            bindings,
        })
    }

    pub fn import_backup(&mut self, backup: Backup, preview: bool) -> Result<ImportReport, Error> {
        validate_backup(&backup)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing_workspaces: Vec<Workspace> = load_json(&tx, "SELECT body FROM workspaces")?;
        let existing_records: Vec<Record> = load_json(&tx, "SELECT body FROM records")?;
        let mut workspaces: HashMap<String, Workspace> = existing_workspaces
            .into_iter()
            .map(|w| (w.id.clone(), w))
            .collect();
        let mut records: HashMap<String, Record> = existing_records
            .into_iter()
            .map(|r| (r.id.clone(), r))
            .collect();
        let mut bindings: HashSet<Binding> = load_bindings(&tx)?.into_iter().collect();
        let mut report = ImportReport::default();
        let mut new_workspaces = Vec::new();
        let mut new_records = Vec::new();
        let mut new_bindings = Vec::new();
        for workspace in backup.workspaces {
            if let Some(existing) = workspaces.get(&workspace.id) {
                if existing == &workspace {
                    report.unchanged += 1;
                } else {
                    conflict(
                        &mut report,
                        "workspace",
                        &workspace.id,
                        "Same ID has different local data; kept local record",
                    );
                }
            } else if workspaces.values().any(|w| w.root == workspace.root) {
                conflict(
                    &mut report,
                    "workspace",
                    &workspace.id,
                    "Root is already mapped to another workspace ID",
                );
            } else {
                workspaces.insert(workspace.id.clone(), workspace.clone());
                new_workspaces.push(workspace);
            }
        }
        for record in backup.records {
            if let Some(existing) = records.get(&record.id) {
                if existing == &record {
                    report.unchanged += 1;
                } else {
                    conflict(
                        &mut report,
                        "record",
                        &record.id,
                        "Same ID has different local data; kept local record",
                    );
                }
            } else if record
                .input
                .url
                .as_ref()
                .is_some_and(|url| records.values().any(|r| r.input.url.as_ref() == Some(url)))
            {
                conflict(
                    &mut report,
                    "record",
                    &record.id,
                    "URL already belongs to another document",
                );
            } else if record
                .input
                .source
                .as_ref()
                .is_some_and(|s| !workspaces.contains_key(&s.workspace_id))
            {
                conflict(
                    &mut report,
                    "record",
                    &record.id,
                    "Source workspace could not be imported",
                );
            } else {
                records.insert(record.id.clone(), record.clone());
                new_records.push(record);
            }
        }
        for binding in backup.bindings {
            if bindings.contains(&binding) {
                report.unchanged += 1;
            } else if bindings.iter().any(|b| {
                b.workspace_id == binding.workspace_id
                    && b.path == binding.path
                    && b.document_id == binding.document_id
            }) {
                conflict(
                    &mut report,
                    "binding",
                    &binding.document_id,
                    "Association metadata differs; kept local association",
                );
            } else if !workspaces.contains_key(&binding.workspace_id)
                || !records
                    .get(&binding.document_id)
                    .is_some_and(|r| r.input.kind == RecordKind::Document && !r.is_demo)
            {
                conflict(
                    &mut report,
                    "binding",
                    &binding.document_id,
                    "Document or workspace could not be imported",
                );
            } else {
                bindings.insert(binding.clone());
                new_bindings.push(binding);
            }
        }
        report.new_workspaces = new_workspaces.len();
        report.new_records = new_records.len();
        report.new_bindings = new_bindings.len();
        if !preview {
            for w in new_workspaces {
                insert_workspace(&tx, &w)?;
            }
            for r in new_records {
                insert_record(&tx, &r)?;
            }
            for b in new_bindings {
                crate::context::insert_binding(&tx, &b)?;
            }
        }
        tx.commit()?;
        Ok(report)
    }
}

fn conflict(report: &mut ImportReport, kind: &str, id: &str, reason: &str) {
    report.conflicts.push(ImportConflict {
        kind: kind.into(),
        id: id.into(),
        reason: reason.into(),
    });
}
fn load_json<T: serde::de::DeserializeOwned>(
    db: &rusqlite::Connection,
    sql: &str,
) -> Result<Vec<T>, Error> {
    let mut stmt = db.prepare(sql)?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.iter().map(|s| Ok(serde_json::from_str(s)?)).collect()
}
fn load_bindings(db: &rusqlite::Connection) -> Result<Vec<Binding>, Error> {
    crate::context::all_bindings(db)
}
fn validate_backup(backup: &Backup) -> Result<(), Error> {
    if let Some(error) = backup_issues(backup).first() {
        return Err(Error::Validation(format!(
            "{}[{}] {}: {}",
            error.kind, error.index, error.id, error.reason
        )));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewInput {
    format_version: u32,
    exported_at: i64,
    workspaces: Vec<Value>,
    records: Vec<Value>,
    bindings: Vec<Value>,
}

fn parse_entries<T: DeserializeOwned>(
    values: Vec<Value>,
    kind: &str,
    errors: &mut Vec<ImportInvalid>,
) -> (Vec<T>, Vec<usize>) {
    let mut entries = Vec::new();
    let mut positions = Vec::new();
    for (index, value) in values.into_iter().enumerate() {
        let id = value
            .get("id")
            .or_else(|| value.get("document_id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        match serde_json::from_value(value) {
            Ok(entry) => {
                entries.push(entry);
                positions.push(index);
            }
            Err(error) => invalid(errors, kind, index, &id, error.to_string()),
        }
    }
    (entries, positions)
}

fn invalid(
    errors: &mut Vec<ImportInvalid>,
    kind: &str,
    index: usize,
    id: &str,
    reason: impl Into<String>,
) {
    errors.push(ImportInvalid {
        kind: kind.into(),
        index,
        id: id.into(),
        reason: reason.into(),
    });
}

fn backup_issues(backup: &Backup) -> Vec<ImportInvalid> {
    let mut errors = Vec::new();
    if !matches!(backup.format_version, 1 | 2)
        || backup.exported_at < 0
        || backup.exported_at > crate::MAX_JSON_INTEGER as i64
    {
        invalid(
            &mut errors,
            "backup",
            0,
            "",
            "Unsupported format or invalid export timestamp",
        );
    }
    let mut workspaces = HashMap::new();
    let mut roots = HashSet::new();
    for (index, w) in backup.workspaces.iter().enumerate() {
        let duplicate_id = workspaces.insert(&w.id, w).is_some();
        let duplicate_root = !roots.insert(&w.root);
        if w.id.is_empty()
            || w.name.trim().is_empty()
            || w.revision == 0
            || w.revision > crate::MAX_JSON_INTEGER
            || !portable_absolute(&w.root)
            || duplicate_id
            || duplicate_root
        {
            invalid(
                &mut errors,
                "workspace",
                index,
                &w.id,
                "Invalid or duplicate workspace",
            );
        }
    }
    let mut records = HashMap::new();
    let mut urls = HashSet::new();
    for (index, r) in backup.records.iter().enumerate() {
        let duplicate_id = records.insert(&r.id, r).is_some();
        let duplicate_url = r.input.url.as_ref().is_some_and(|url| !urls.insert(url));
        if r.id.is_empty()
            || r.revision == 0
            || r.revision > crate::MAX_JSON_INTEGER
            || r.created_at < 0
            || r.updated_at < r.created_at
            || r.updated_at > crate::MAX_JSON_INTEGER as i64
            || duplicate_id
        {
            invalid(
                &mut errors,
                "record",
                index,
                &r.id,
                "Invalid or duplicate record metadata",
            );
            continue;
        }
        match normalize_input(r.input.clone()) {
            Err(error) => {
                invalid(&mut errors, "record", index, &r.id, error.to_string());
                continue;
            }
            Ok(input) if input != r.input => {
                invalid(
                    &mut errors,
                    "record",
                    index,
                    &r.id,
                    "Record is not normalized",
                );
                continue;
            }
            _ => {}
        };
        if r.input
            .source
            .as_ref()
            .is_some_and(|s| !workspaces.contains_key(&s.workspace_id))
        {
            invalid(
                &mut errors,
                "record",
                index,
                &r.id,
                "Source refers to a missing workspace",
            );
        } else if duplicate_url {
            invalid(
                &mut errors,
                "record",
                index,
                &r.id,
                "Duplicate document URL in backup",
            );
        }
    }
    let mut bindings = HashSet::new();
    for (index, b) in backup.bindings.iter().enumerate() {
        let duplicate = !bindings.insert((&b.workspace_id, &b.path, &b.document_id));
        if !crate::context::validate_binding(b).is_ok()
            || !workspaces.contains_key(&b.workspace_id)
            || !records
                .get(&b.document_id)
                .is_some_and(|r| r.input.kind == RecordKind::Document && !r.is_demo)
            || duplicate
        {
            invalid(
                &mut errors,
                "binding",
                index,
                &b.document_id,
                "Invalid path, missing reference, demo document or duplicate binding",
            );
        }
    }
    errors
}

fn portable_absolute(path: &str) -> bool {
    std::path::Path::new(path).is_absolute()
        || (path.len() >= 3
            && path.as_bytes()[0].is_ascii_alphabetic()
            && path.as_bytes()[1] == b':'
            && matches!(path.as_bytes()[2], b'/' | b'\\'))
        || path.starts_with("\\\\")
}
