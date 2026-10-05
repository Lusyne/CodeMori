//! On-demand local Markdown rendering. Neither body nor rendered HTML is persisted.
use crate::{Error, Store, model::RecordKind};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use serde::Serialize;
use std::{fs::File, io::Read, path::PathBuf};

const MAX_MARKDOWN_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct MarkdownPreview {
    pub title: String,
    pub path: PathBuf,
    pub html: String,
}

impl Store {
    pub fn markdown_preview(&self, id: &str, revision: u64) -> Result<MarkdownPreview, Error> {
        let record = self.get(id)?;
        if record.revision != revision {
            return Err(Error::Conflict {
                current_revision: record.revision,
            });
        }
        if record.input.kind != RecordKind::Document {
            return Err(Error::Validation("Select a local Markdown document".into()));
        }
        let url = url::Url::parse(record.input.url.as_deref().unwrap_or_default())
            .map_err(|_| Error::Validation("Invalid document URL".into()))?;
        let path = url
            .to_file_path()
            .map_err(|_| Error::Validation("Preview supports local Markdown files only".into()))?;
        if !path
            .extension()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.eq_ignore_ascii_case("md") || v.eq_ignore_ascii_case("markdown"))
        {
            return Err(Error::Validation(
                "Preview supports .md and .markdown files only".into(),
            ));
        }
        let metadata = path.metadata()?;
        if !metadata.is_file() || metadata.len() > MAX_MARKDOWN_BYTES {
            return Err(Error::Validation(
                "Preview requires a regular file of at most 2 MiB".into(),
            ));
        }
        let mut bytes = Vec::new();
        File::open(&path)?
            .take(MAX_MARKDOWN_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_MARKDOWN_BYTES {
            return Err(Error::Validation("Markdown file exceeds 2 MiB".into()));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error::Validation("Markdown file must be UTF-8".into()))?;
        Ok(MarkdownPreview {
            title: record.input.title,
            path,
            html: render(text),
        })
    }
}

fn render(text: &str) -> String {
    let events = Parser::new_ext(text, Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH)
        .filter_map(|event| match event {
            // Show literal HTML as text. No attributes, scripts or resource loads survive.
            Event::Html(value) | Event::InlineHtml(value) => Some(Event::Text(value)),
            // Preserve labels/alt text without navigation or image loading in either IDE.
            Event::Start(Tag::Link { .. } | Tag::Image { .. })
            | Event::End(TagEnd::Link | TagEnd::Image) => None,
            other => Some(other),
        });
    let mut result = String::new();
    html::push_html(&mut result, events);
    result
}
