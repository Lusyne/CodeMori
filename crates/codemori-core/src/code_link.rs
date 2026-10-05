//! Portable, navigation-only links. The receiving IDE supplies the local root.
use crate::{
    Error,
    project::{atomic_write, plain_path},
    store::{canonical_root, relative_path},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectIdentity {
    format_version: u32,
    project_id: String,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Location {
    pub project_id: String,
    pub path: String,
    pub line: u32,
    pub anchor: Option<crate::anchors::Anchor>,
}
fn invalid(message: &str) -> Error {
    Error::Validation(message.into())
}
fn identity(root: &Path) -> Result<Option<ProjectIdentity>, Error> {
    plain_path(&root.join(".codemori"), true)?;
    let path = root.join(".codemori/project.json");
    plain_path(&path, false)?;
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(16_385).read_to_end(&mut bytes)?;
    if bytes.len() > 16_384 {
        return Err(invalid("Project identity exceeds 16 KiB"));
    }
    let identity: ProjectIdentity = serde_json::from_slice(&bytes)?;
    if identity.format_version != 1 || Uuid::parse_str(&identity.project_id).is_err() {
        return Err(invalid(
            "Invalid project identity; restore the committed project.json",
        ));
    }
    Ok(Some(identity))
}
fn target(root: &Path, path: &str) -> Result<PathBuf, Error> {
    let path = relative_path(path)?;
    if path.chars().any(char::is_control) {
        return Err(invalid("Code link paths cannot contain control characters"));
    }
    let mut target = root.to_path_buf();
    for part in Path::new(&path).components() {
        target.push(part);
        if fs::symlink_metadata(&target)?.file_type().is_symlink() {
            return Err(invalid(
                "Code links cannot navigate through project symbolic links",
            ));
        }
    }
    if !target.is_file() {
        return Err(invalid("Code link target is not a regular file"));
    }
    Ok(target)
}
fn product(value: &str) -> bool {
    matches!(
        value,
        "idea"
            | "pycharm"
            | "webstorm"
            | "goland"
            | "clion"
            | "rider"
            | "rubymine"
            | "phpstorm"
            | "rustrover"
            | "datagrip"
    )
}
pub fn parse(value: &str) -> Result<Location, Error> {
    if value.len() > 8192 {
        return Err(invalid("Code link exceeds 8 KiB"));
    }
    let url = url::Url::parse(value).map_err(|_| invalid("Invalid CodeMori code link"))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            "Code links do not accept credentials, ports or fragments",
        ));
    }
    let supported = match url.scheme() {
        "vscode" | "vscode-insiders" => {
            url.host_str() == Some("lusyne.codemori") && url.path() == "/open"
        }
        "jetbrains" => url.host_str().is_some_and(product) && url.path() == "/codemori/open",
        _ => false,
    };
    if !supported {
        return Err(invalid(
            "Use a CodeMori VS Code or JetBrains code-position link",
        ));
    }
    let mut params = BTreeMap::new();
    for (key, value) in url.query_pairs() {
        if !matches!(key.as_ref(), "project" | "path_hex" | "line" | "anchor_hex")
            || params.insert(key.to_string(), value.to_string()).is_some()
        {
            return Err(invalid("Unknown or duplicate code-link parameter"));
        }
    }
    let project = params
        .get("project")
        .ok_or_else(|| invalid("Missing project identity"))?;
    let project_id = Uuid::parse_str(project)
        .map_err(|_| invalid("Invalid project identity"))?
        .to_string();
    let encoded = params
        .get("path_hex")
        .ok_or_else(|| invalid("Missing encoded file path"))?;
    if encoded.len() % 2 != 0 || !encoded.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("Invalid encoded file path"));
    }
    let bytes = (0..encoded.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&encoded[i..i + 2], 16)
                .map_err(|_| invalid("Invalid encoded file path"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let decoded = String::from_utf8(bytes).map_err(|_| invalid("Code link paths must be UTF-8"))?;
    if decoded.chars().any(char::is_control) {
        return Err(invalid("Code link paths cannot contain control characters"));
    }
    let path = relative_path(&decoded)?;
    let line: u32 = params
        .get("line")
        .ok_or_else(|| invalid("Missing source line"))?
        .parse()
        .map_err(|_| invalid("Invalid source line"))?;
    if line == 0 || line > i32::MAX as u32 {
        return Err(invalid("Source line must be between 1 and 2147483647"));
    }
    let anchor = params
        .get("anchor_hex")
        .map(|value| {
            if value.len() % 2 != 0 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(invalid("Invalid encoded anchor"));
            }
            let bytes = (0..value.len())
                .step_by(2)
                .map(|i| {
                    u8::from_str_radix(&value[i..i + 2], 16)
                        .map_err(|_| invalid("Invalid encoded anchor"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let anchor: crate::anchors::Anchor = serde_json::from_slice(&bytes)?;
            crate::anchors::validate(&anchor)?;
            Ok(anchor)
        })
        .transpose()?;
    Ok(Location {
        anchor,
        project_id,
        path,
        line,
    })
}
pub fn create(
    root: &str,
    path: &str,
    line: u32,
    jetbrains_product: &str,
    vscode_scheme: &str,
) -> Result<Value, Error> {
    if line == 0
        || line > i32::MAX as u32
        || !product(jetbrains_product)
        || !matches!(vscode_scheme, "vscode" | "vscode-insiders")
    {
        return Err(invalid("Unsupported IDE target or source line"));
    }
    let root = PathBuf::from(canonical_root(root)?);
    let path = relative_path(path)?;
    let source = target(&root, &path)?;
    let anchor = crate::anchors::capture(&source, line)?;
    let anchor_hex: String = serde_json::to_vec(&anchor)?
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let _lock = crate::project::lock(
        root.to_str()
            .ok_or_else(|| invalid("Project root must be UTF-8"))?,
    )?;
    let previous = identity(&root)?;
    let created = previous.is_none();
    let identity = previous.unwrap_or_else(|| ProjectIdentity {
        format_version: 1,
        project_id: Uuid::new_v4().to_string(),
    });
    if created {
        let mut bytes = serde_json::to_vec_pretty(&identity)?;
        bytes.push(b'\n');
        atomic_write(&root.join(".codemori/project.json"), &bytes)?;
    }
    // ASCII-only path transport survives IDE URI decoders (notably VS Code query decoding).
    let encoded: String = path
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let make = |base: String| -> Result<String, Error> {
        let mut url = url::Url::parse(&base).map_err(|_| invalid("Invalid IDE link target"))?;
        url.query_pairs_mut()
            .append_pair("project", &identity.project_id)
            .append_pair("path_hex", &encoded)
            .append_pair("line", &line.to_string())
            .append_pair("anchor_hex", &anchor_hex);
        if url.as_str().len() > 8192 {
            return Err(invalid("Code link exceeds 8 KiB"));
        }
        Ok(url.into())
    };
    Ok(
        json!({"vscode_url":make(format!("{vscode_scheme}://lusyne.codemori/open"))?,
        "jetbrains_url":make(format!("jetbrains://{jetbrains_product}/codemori/open"))?,
        "project_id":identity.project_id,"path":path,"line":line,"created_project_identity":created}),
    )
}
pub fn resolve(root: &str, value: &str) -> Result<Value, Error> {
    let location = parse(value)?;
    let root = PathBuf::from(canonical_root(root)?);
    let Some(identity) = identity(&root)? else {
        return Ok(Value::Null);
    };
    if Uuid::parse_str(&identity.project_id).ok() != Uuid::parse_str(&location.project_id).ok() {
        return Ok(Value::Null);
    }
    let (path, line, resolution) = if let Some(anchor) = &location.anchor {
        crate::anchors::resolve(&root, &location.path, location.line, anchor)?
    } else {
        (
            target(&root, &location.path)?,
            location.line,
            "legacy_line".into(),
        )
    };
    Ok(
        json!({"root":root,"path":path,"line":line,"project_id":location.project_id,"resolution":resolution}),
    )
}
