//! Bounded saved-text anchors; neither source text nor machine paths are embedded in links.
use crate::{
    Error,
    context::{digest, read_text, safe_target},
    model::BindingKind,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub file_hash: String,
    pub line_hash: Option<String>,
    pub symbol: Option<String>,
}
fn invalid(message: &str) -> Error {
    Error::Validation(message.into())
}
pub(crate) fn walk(root: &Path) -> ignore::Walk {
    ignore::WalkBuilder::new(root)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|e| {
            e.depth() == 0
                || !e.file_type().is_some_and(|t| t.is_dir())
                || !matches!(
                    e.file_name().to_str(),
                    Some("node_modules" | "target" | "build" | "dist" | "vendor" | "__pycache__")
                )
        })
        .build()
}
pub(crate) fn files(root: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut files = Vec::new();
    for entry in walk(root) {
        let entry = entry.map_err(|e| invalid(&e.to_string()))?;
        if let Some(error) = entry.error() {
            return Err(invalid(&error.to_string()));
        }
        if entry.file_type().is_some_and(|t| t.is_file()) {
            files.push(entry.into_path());
            if files.len() > 2000 {
                return Err(invalid(
                    "Workspace lookup exceeds 2000 non-ignored files; narrow the project or repair the path explicitly",
                ));
            }
        }
    }
    files.sort();
    Ok(files)
}
struct Symbol {
    key: String,
    start: u32,
    end: u32,
}
fn symbols(path: &Path, text: &str) -> Result<Vec<Symbol>, Error> {
    let Some((language, grammar)) = crate::index::language(path) else {
        return Ok(Vec::new());
    };
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&grammar)
        .map_err(|_| invalid("Cannot load symbol parser"))?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| invalid("Cannot parse saved source"))?;
    let mut output = Vec::new();
    fn visit(
        node: tree_sitter::Node<'_>,
        text: &str,
        language: &str,
        parents: &[String],
        out: &mut Vec<Symbol>,
        depth: usize,
    ) -> Result<(), Error> {
        if depth > 256 {
            return Err(invalid("Source syntax nesting exceeds the anchor limit"));
        }
        let mut parents = parents.to_vec();
        if matches!(
            node.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "class_definition"
                | "function_definition"
                | "method_declaration"
                | "constructor_declaration"
                | "function_declaration"
                | "method_definition"
                | "function_signature"
                | "arrow_function"
        ) {
            let name = node.child_by_field_name("name").or_else(|| {
                node.parent()
                    .filter(|p| p.kind() == "variable_declarator")
                    .and_then(|p| p.child_by_field_name("name"))
            });
            if let Some(name) = name
                .and_then(|n| n.utf8_text(text.as_bytes()).ok())
                .filter(|v| v.len() <= 256)
            {
                let types = node
                    .child_by_field_name("parameters")
                    .map(|parameters| {
                        let mut cursor = parameters.walk();
                        parameters
                            .named_children(&mut cursor)
                            .map(|n| {
                                n.child_by_field_name("type")
                                    .and_then(|t| t.utf8_text(text.as_bytes()).ok())
                                    .unwrap_or("_")
                                    .chars()
                                    .filter(|c| !c.is_whitespace())
                                    .collect::<String>()
                            })
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default();
                parents.push(format!("{}:{}:{}", node.kind(), name, digest(&types)));
                out.push(Symbol {
                    key: format!("{language}/{}", parents.join("/")),
                    start: node.start_position().row as u32 + 1,
                    end: node.end_position().row as u32 + 1,
                });
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            visit(child, text, language, &parents, out, depth + 1)?;
        }
        Ok(())
    }
    visit(tree.root_node(), text, language, &[], &mut output, 0)?;
    Ok(output)
}
pub fn capture(path: &Path, line: u32) -> Result<Anchor, Error> {
    let text = read_text(path)?;
    let symbol = symbols(path, &text)?
        .into_iter()
        .filter(|s| s.start <= line && s.end >= line)
        .min_by_key(|s| s.end - s.start)
        .map(|s| s.key);
    let line_hash = text
        .lines()
        .nth((line - 1) as usize)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(digest);
    Ok(Anchor {
        file_hash: digest(&text),
        line_hash,
        symbol,
    })
}
pub(crate) fn validate(anchor: &Anchor) -> Result<(), Error> {
    for hash in std::iter::once(&anchor.file_hash).chain(anchor.line_hash.iter()) {
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid("Invalid code anchor hash"));
        }
    }
    if anchor
        .symbol
        .as_ref()
        .is_some_and(|s| s.len() > 4096 || s.chars().any(char::is_control))
    {
        return Err(invalid("Invalid symbol anchor"));
    }
    Ok(())
}
fn position(
    path: &Path,
    text: &str,
    line: u32,
    anchor: &Anchor,
) -> Result<Option<(u32, &'static str)>, Error> {
    if digest(text) == anchor.file_hash {
        return Ok(Some((line, "exact")));
    }
    let symbols = symbols(path, text)?;
    if let Some(key) = &anchor.symbol {
        let matches: Vec<_> = symbols.iter().filter(|s| &s.key == key).collect();
        if matches.len() > 1 {
            return Err(invalid(
                "Symbol anchor is ambiguous; repair the link explicitly",
            ));
        }
        if let Some(symbol) = matches.first() {
            let lines: Vec<_> = text
                .lines()
                .enumerate()
                .filter(|(i, s)| {
                    let n = *i as u32 + 1;
                    n >= symbol.start
                        && n <= symbol.end
                        && anchor
                            .line_hash
                            .as_ref()
                            .is_some_and(|h| digest(s.trim()) == *h)
                })
                .map(|(i, _)| i as u32 + 1)
                .collect();
            return Ok(Some((
                if lines.len() == 1 {
                    lines[0]
                } else {
                    symbol.start
                },
                "symbol",
            )));
        }
    }
    if let Some(hash) = &anchor.line_hash {
        let lines: Vec<_> = text
            .lines()
            .enumerate()
            .filter(|(_, s)| digest(s.trim()) == *hash)
            .map(|(i, _)| i as u32 + 1)
            .collect();
        if lines.len() == 1 {
            return Ok(Some((lines[0], "content")));
        }
    }
    Ok(None)
}
pub fn resolve(
    root: &Path,
    path: &str,
    line: u32,
    anchor: &Anchor,
) -> Result<(PathBuf, u32, String), Error> {
    validate(anchor)?;
    let original = root.join(path);
    if original.symlink_metadata().is_ok() {
        let target = safe_target(root, path, BindingKind::File)?;
        let text = read_text(&target)?;
        if let Some((line, method)) = position(&target, &text, line, anchor)? {
            return Ok((target, line, method.into()));
        }
    }
    let mut exact = Vec::new();
    let mut candidates = Vec::new();
    let mut bytes = 0;
    for file in files(root)? {
        if file.extension() != original.extension() {
            continue;
        }
        let text = read_text(&file)?;
        bytes += text.len();
        if bytes > 64 * 1024 * 1024 {
            return Err(invalid(
                "Anchor lookup exceeds 64 MiB; repair the link explicitly",
            ));
        }
        if digest(&text) == anchor.file_hash {
            exact.push((file, line));
            continue;
        }
        // Moving modified code requires a qualified symbol, not a common line in an unrelated file.
        if anchor.symbol.is_some() {
            if let Some((line, "symbol")) = position(&file, &text, line, anchor)? {
                candidates.push((file, line));
            }
        }
    }
    let matches = if exact.is_empty() { candidates } else { exact };
    if matches.len() != 1 {
        return Err(invalid(if matches.is_empty() {
            "Code anchor not found; the file/symbol may have changed. Repair the link explicitly"
        } else {
            "More than one code anchor matches; choose and repair the path explicitly"
        }));
    }
    let (file, line) = matches.into_iter().next().unwrap();
    Ok((file, line, "relocated".into()))
}
