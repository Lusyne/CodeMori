//! Views combining private SQLite records with the current project's portable file.
use crate::{
    Error, Store,
    model::*,
    project::{PROJECT_WORKSPACE, ProjectInfo, Snapshot},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, VecDeque},
};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LibraryScope {
    #[default]
    All,
    Personal,
    Project,
}

fn load(root: Option<&str>) -> (Option<Snapshot>, Option<ProjectInfo>) {
    match root {
        None => (None, None),
        Some(root) => match Snapshot::load(root) {
            Ok(snapshot) => {
                let info = snapshot.info.clone();
                (Some(snapshot), Some(info))
            }
            Err(error) => (None, Some(crate::project::failed_info(root, &error))),
        },
    }
}
fn personal_record(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.insert("scope".into(), json!("personal"));
    }
    value
}
struct Stream<'a> {
    store: &'a Store,
    filter: SearchFilter,
    items: VecDeque<SearchHit>,
    next_offset: usize,
    total: usize,
    started: bool,
}
impl<'a> Stream<'a> {
    fn new(store: &'a Store, mut filter: SearchFilter) -> Self {
        filter.limit = filter.offset.saturating_add(filter.limit).min(200);
        filter.offset = 0;
        Self {
            store,
            filter,
            items: VecDeque::new(),
            next_offset: 0,
            total: 0,
            started: false,
        }
    }
    fn fill(&mut self) -> Result<(), Error> {
        if self.items.is_empty() && (!self.started || self.next_offset < self.total) {
            self.filter.offset = self.next_offset;
            let page = self.store.search(self.filter.clone())?;
            self.total = page.total;
            self.started = true;
            self.next_offset += page.items.len();
            // A transaction on the personal connection keeps paging stable.
            if page.items.is_empty() {
                self.next_offset = self.total;
            }
            self.items = page.items.into();
        }
        Ok(())
    }
}
fn compare(a: &SearchHit, b: &SearchHit) -> Ordering {
    a.rank
        .cmp(&b.rank)
        .then_with(|| b.record.updated_at.cmp(&a.record.updated_at))
        .then_with(|| a.record.id.cmp(&b.record.id))
}
pub fn search(
    personal: &Store,
    root: Option<&str>,
    scope: LibraryScope,
    filter: SearchFilter,
) -> Result<Value, Error> {
    crate::search::validate_filter(&filter)?;
    if scope == LibraryScope::Project && root.is_none() {
        return Err(Error::Validation(
            "Open a local project to view shared data".into(),
        ));
    }
    let (project, info) = load(root);
    let tx = personal.db.unchecked_transaction()?;
    let mut private =
        (scope != LibraryScope::Project).then(|| Stream::new(personal, filter.clone()));
    let mut shared = project
        .as_ref()
        .filter(|_| scope != LibraryScope::Personal)
        .map(|s| {
            let mut project_filter = filter.clone();
            project_filter.workspace_id = None; // Project ownership is implicit for all shared records.
            Stream::new(&s.store, project_filter)
        });
    if let Some(s) = &mut private {
        s.fill()?;
    }
    if let Some(s) = &mut shared {
        s.fill()?;
    }
    let total = private.as_ref().map_or(0, |s| s.total) + shared.as_ref().map_or(0, |s| s.total);
    let mut items = Vec::new();
    let mut seen = 0;
    if filter.offset < total {
        loop {
            if let Some(s) = &mut private {
                s.fill()?;
            }
            if let Some(s) = &mut shared {
                s.fill()?;
            }
            let a = private.as_ref().and_then(|s| s.items.front());
            let b = shared.as_ref().and_then(|s| s.items.front());
            let is_private = match (a, b) {
                (Some(a), Some(b)) => compare(a, b) != Ordering::Greater,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            let hit = if is_private {
                private.as_mut().unwrap().items.pop_front().unwrap()
            } else {
                shared.as_mut().unwrap().items.pop_front().unwrap()
            };
            if seen >= filter.offset {
                let mut value = serde_json::to_value(hit)?;
                value["record"] = if is_private {
                    personal_record(value["record"].take())
                } else {
                    project.as_ref().unwrap().view(value["record"].take())?
                };
                items.push(value);
                if items.len() == filter.limit {
                    break;
                }
            }
            seen += 1;
        }
    }
    tx.commit()?;
    Ok(
        json!({"items":items,"total":total,"limit":filter.limit,"offset":filter.offset,"project":info}),
    )
}
pub fn tags(
    personal: &Store,
    root: Option<&str>,
    scope: LibraryScope,
    demo: bool,
) -> Result<Value, Error> {
    let (project, info) = load(root);
    let mut tags = BTreeMap::new();
    if scope != LibraryScope::Project {
        for tag in personal.tags(demo)? {
            tags.entry(tag.to_lowercase()).or_insert(tag);
        }
    }
    if scope != LibraryScope::Personal {
        if let Some(project) = project {
            for tag in project.store.tags(demo)? {
                tags.entry(tag.to_lowercase()).or_insert(tag);
            }
        }
    }
    Ok(json!({"tags":tags.into_values().collect::<Vec<_>>(),"project":info}))
}
pub fn file_documents(
    personal: &Store,
    root: &str,
    workspace_id: &str,
    path: &str,
) -> Result<Value, Error> {
    let mut entries = personal.context_entries(workspace_id, path)?;
    for entry in &mut entries {
        entry["record"] = personal_record(entry["record"].take());
    }
    let (project, info) = load(Some(root));
    if let Some(project) = project {
        for entry in project.store.context_entries(PROJECT_WORKSPACE, path)? {
            entries.push(project.view(entry)?);
        }
    }
    let mut seen = std::collections::HashSet::new();
    let records: Vec<_> = entries
        .iter()
        .filter_map(|e| {
            let r = &e["record"];
            if seen.insert((r["scope"].to_string(), r["id"].to_string())) {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect();
    Ok(json!({"records":records,"entries":entries,"project":info}))
}
