use crate::{Error, Store, model::*};
use rusqlite::{params_from_iter, types::Value};

pub(crate) fn validate_filter(filter: &SearchFilter) -> Result<(), Error> {
    if filter.limit == 0 || filter.limit > 200 || filter.offset > i64::MAX as usize {
        return Err(Error::Validation(
            "Search limit must be 1..200 and offset must be valid".into(),
        ));
    }
    Ok(())
}

impl Store {
    pub fn search(&self, filter: SearchFilter) -> Result<SearchPage, Error> {
        validate_filter(&filter)?;
        let terms: Vec<String> = filter
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let mut clauses = vec!["r.is_demo=?".to_string()];
        let mut values: Vec<Value> = vec![i64::from(filter.demo).into()];
        for term in &terms {
            clauses.push("instr(r.text_key,?)>0".into());
            values.push(term.clone().into());
        }
        if let Some(kind) = &filter.kind {
            clauses.push("r.kind=?".into());
            values.push(kind.key().to_string().into());
        }
        if let Some(tag) = &filter.tag {
            clauses.push("EXISTS (SELECT 1 FROM json_each(r.tags_key) WHERE value=?)".into());
            values.push(tag.trim().to_lowercase().into());
        }
        if filter.starred {
            clauses.push("r.starred=1".into());
        }
        if let Some(workspace) = &filter.workspace_id {
            super::store::workspace_by_id(&self.db, workspace)?;
            clauses.push("(r.workspace_id=? OR EXISTS(SELECT 1 FROM bindings b WHERE b.document_id=r.id AND b.workspace_id=?))".into());
            values.push(workspace.clone().into());
            values.push(workspace.clone().into());
        }
        let conditions = clauses.join(" AND ");
        let count: usize = self.db.query_row(
            &format!("SELECT COUNT(*) FROM records r WHERE {conditions}"),
            params_from_iter(values.iter()),
            |r| r.get(0),
        )?;
        let mut ordering = String::from("2");
        if !terms.is_empty() {
            let exact = filter.query.trim().to_lowercase();
            ordering="CASE WHEN r.title_key=? OR EXISTS(SELECT 1 FROM json_each(r.tags_key) WHERE value=?) THEN 0".into();
            values.push(exact.clone().into());
            values.push(exact.into());
            for term in &terms {
                ordering.push_str(" WHEN instr(r.title_key,?)>0 OR EXISTS(SELECT 1 FROM json_each(r.tags_key) WHERE instr(value,?)>0) THEN 1");
                values.push(term.clone().into());
                values.push(term.clone().into());
            }
            ordering.push_str(" ELSE 2 END");
        }
        // A numeric literal in ORDER BY is a column ordinal; use a constant expression for empty queries.
        if terms.is_empty() {
            ordering = "(2+0)".into();
        }
        let sql = format!(
            "WITH matched AS (SELECT r.* FROM records r WHERE {conditions}) SELECT r.body,{ordering} AS sort_rank FROM matched r ORDER BY sort_rank,r.updated_at DESC,r.id ASC LIMIT ? OFFSET ?"
        );
        values.push((filter.limit as i64).into());
        values.push((filter.offset as i64).into());
        let mut stmt = self.db.prepare(&sql)?;
        let bodies = stmt
            .query_map(params_from_iter(values.iter()), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, u8>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut items = Vec::new();
        for (body, rank) in bodies {
            let record: Record = serde_json::from_str(&body)?;
            let text = format!(
                "{}\n{}\n{}",
                record.input.title, record.input.description, record.input.content
            );
            let mut projects = self.db.prepare("SELECT DISTINCT json_extract(w.body,'$.name') FROM workspaces w WHERE w.id=?1 OR EXISTS(SELECT 1 FROM bindings b WHERE b.document_id=?2 AND b.workspace_id=w.id) ORDER BY 1")?;
            let workspace_names = projects
                .query_map(
                    rusqlite::params![
                        record.input.source.as_ref().map(|s| &s.workspace_id),
                        record.id
                    ],
                    |r| r.get::<_, String>(0),
                )?
                .collect::<Result<Vec<_>, _>>()?;
            items.push(SearchHit {
                rank,
                excerpt: excerpt(&text, &terms),
                workspace_names,
                record,
            });
        }
        Ok(SearchPage {
            items,
            total: count,
            limit: filter.limit,
            offset: filter.offset,
        })
    }
}

fn matches(text: &str, terms: &[String]) -> Vec<(usize, usize)> {
    let mut lowered = String::new();
    let mut positions = Vec::new();
    for (start, ch) in text.char_indices() {
        let fold = ch.to_lowercase().to_string();
        positions.extend(std::iter::repeat_n(
            (start, start + ch.len_utf8()),
            fold.len(),
        ));
        lowered.push_str(&fold);
    }
    let mut spans = Vec::new();
    for term in terms.iter().filter(|s| !s.is_empty()) {
        for (start, _) in lowered.match_indices(term) {
            spans.push((positions[start].0, positions[start + term.len() - 1].1));
        }
    }
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for span in spans {
        if let Some(last) = merged.last_mut().filter(|last| last.1 >= span.0) {
            last.1 = last.1.max(span.1);
        } else {
            merged.push(span);
        }
    }
    merged
}

pub fn excerpt(text: &str, terms: &[String]) -> Vec<TextSpan> {
    let ranges = matches(text, terms);
    let first = ranges.first().map_or(0, |s| s.0);
    let chars: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let at = chars.partition_point(|i| *i < first);
    let start = chars[at.saturating_sub(80)];
    let end = chars[(at.saturating_sub(80) + 350).min(chars.len() - 1)];
    let slice = &text[start..end];
    let ranges = matches(slice, terms);
    let mut output = Vec::new();
    if start > 0 {
        output.push(TextSpan {
            text: "…".into(),
            highlight: false,
        });
    }
    let mut pos = 0;
    for (a, b) in ranges {
        if a > pos {
            output.push(TextSpan {
                text: slice[pos..a].into(),
                highlight: false,
            });
        }
        output.push(TextSpan {
            text: slice[a..b].into(),
            highlight: true,
        });
        pos = b;
    }
    if pos < slice.len() {
        output.push(TextSpan {
            text: slice[pos..].into(),
            highlight: false,
        });
    }
    if end < text.len() {
        output.push(TextSpan {
            text: "…".into(),
            highlight: false,
        });
    }
    output
}
