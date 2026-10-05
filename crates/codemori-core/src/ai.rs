//! Bounded evidence for coding agents. Retrieval never mutates knowledge/review/source content.
use crate::{
    Error, Store,
    context::{digest, read_text, safe_target},
    model::{BindingKind, SearchFilter},
    store::canonical_root,
};
use serde_json::{Value, json};
use std::path::Path;
fn invalid(message: &str) -> Error {
    Error::Validation(message.into())
}
pub fn search(root: &str, query: &str, under: &str, limit: usize) -> Result<Value, Error> {
    if !(1..=50).contains(&limit) || query.trim().is_empty() || query.len() > 1024 {
        return Err(invalid(
            "Use a nonempty query up to 1024 bytes and limit 1–50",
        ));
    }
    let root = canonical_root(root)?;
    let base = Path::new(&root);
    let folder = safe_target(base, under, BindingKind::Module)?;
    let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut hits = Vec::new();
    let mut scanned = 0;
    let mut bytes = 0;
    let mut skipped = 0;
    let mut truncated = false;
    let mut diagnostics = Vec::new();
    for (visited, entry) in crate::anchors::walk(&folder).enumerate() {
        if visited >= 10000 {
            truncated = true;
            break;
        }
        let entry = match entry {
            Ok(value) => value,
            Err(error) => {
                skipped += 1;
                if diagnostics.len() < 10 {
                    diagnostics.push(error.to_string());
                }
                continue;
            }
        };
        if let Some(error) = entry.error() {
            skipped += 1;
            if diagnostics.len() < 10 {
                diagnostics.push(error.to_string());
            }
        }
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if scanned >= 2000 {
            truncated = true;
            break;
        }
        scanned += 1;
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if size > 2 * 1024 * 1024 {
            skipped += 1;
            continue;
        }
        bytes += size;
        if bytes > 64 * 1024 * 1024 {
            truncated = true;
            break;
        }
        let relative = entry
            .path()
            .strip_prefix(base)
            .map_err(|_| invalid("Source search escaped root"))?
            .to_string_lossy()
            .replace('\\', "/");
        let text = match safe_target(base, &relative, BindingKind::File).and_then(|p| read_text(&p))
        {
            Ok(text) => text,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        for (index, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            if !terms.iter().all(|term| lower.contains(term)) {
                continue;
            }
            if hits.len() == limit {
                truncated = true;
                break;
            }
            hits.push(json!({"path":relative,"line":index+1,"excerpt":line.chars().take(300).collect::<String>(),"excerpt_truncated":line.chars().count()>300}));
        }
        if truncated {
            break;
        }
    }
    Ok(
        json!({"hits":hits,"scanned_files":scanned,"skipped_files":skipped,"truncated":truncated,"diagnostics":diagnostics,"scope":"saved_local_files","under":under}),
    )
}
pub fn read(root: &str, path: &str, start: usize, end: Option<usize>) -> Result<Value, Error> {
    let end = end.unwrap_or_else(|| start.saturating_add(199));
    if start == 0 || end < start || end - start >= 200 {
        return Err(invalid("Read 1–200 lines using 1-based line numbers"));
    }
    let root = canonical_root(root)?;
    let file = safe_target(Path::new(&root), path, BindingKind::File)?;
    let text = read_text(&file)?;
    let total = text.lines().count().max(1);
    if start > total {
        return Err(invalid("Start line is beyond the end of the file"));
    }
    let mut content = String::new();
    let mut last = start;
    let mut truncated = false;
    for (i, line) in text
        .split_inclusive('\n')
        .enumerate()
        .skip(start - 1)
        .take(end - start + 1)
    {
        let available = 65536 - content.len();
        let mut count = line.len().min(available);
        while !line.is_char_boundary(count) {
            count -= 1;
        }
        content.push_str(&line[..count]);
        last = i + 1;
        if count < line.len() {
            truncated = true;
            break;
        }
    }
    Ok(
        json!({"path":path,"start_line":start,"end_line":last,"total_lines":total,"content":content,"fingerprint":digest(&text),"truncated":truncated,"scope":"saved_local_file"}),
    )
}
pub fn context(
    store: &mut Store,
    root: &str,
    query: &str,
    path: Option<&str>,
) -> Result<Value, Error> {
    let workspace = store.register_workspace(root, None)?;
    let knowledge = crate::library::search(
        store,
        Some(&workspace.root),
        crate::library::LibraryScope::All,
        SearchFilter {
            query: query.into(),
            workspace_id: Some(workspace.id.clone()),
            limit: 20,
            ..Default::default()
        },
    )?;
    let associations = path
        .map(|path| crate::library::file_documents(store, &workspace.root, &workspace.id, path))
        .transpose()?;
    let code = if query.trim().is_empty() {
        None
    } else {
        Some(search(&workspace.root, query, ".", 20)?)
    };
    let data = json!({"root":workspace.root,"scope":"current_project","knowledge":knowledge,"associations":associations,"code":code,"external_document_bodies_loaded":false,"review_confirmation_performed":false});
    if serde_json::to_vec(&data)?.len() > 512 * 1024 {
        return Err(invalid(
            "AI context exceeds 512 KiB; narrow the query or use code search/read",
        ));
    }
    Ok(data)
}
