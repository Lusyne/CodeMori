//! Local, user-confirmed collaboration identity; never inferred from the OS account.
use crate::{
    Error,
    project::{atomic_write, plain_path},
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct Author {
    pub id: String,
    pub display_name: String,
}
impl Author {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if Uuid::parse_str(&self.id).is_err()
            || self.display_name.trim() != self.display_name
            || self.display_name.is_empty()
            || self.display_name.chars().count() > 80
            || self.display_name.chars().any(char::is_control)
        {
            return Err(Error::Validation("Identity requires a UUID and a display name of 1–80 characters without control characters".into()));
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    format_version: u32,
    author: Author,
}
pub fn get(directory: &Path) -> Result<Option<Author>, Error> {
    let path = directory.join("profile.json");
    plain_path(&path, false)?;
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(16_385).read_to_end(&mut bytes)?;
    if bytes.len() > 16_384 {
        return Err(Error::Validation("Profile exceeds 16 KiB".into()));
    }
    let profile: Profile = serde_json::from_slice(&bytes)?;
    if profile.format_version != 1 {
        return Err(Error::Validation("Unsupported profile format".into()));
    }
    profile.author.validate()?;
    Ok(Some(profile.author))
}
pub fn require(directory: &Path) -> Result<Author, Error> {
    get(directory)?.ok_or(Error::IdentityRequired)
}
pub fn set(directory: &Path, display_name: String) -> Result<Author, Error> {
    let mut author = Author {
        id: Uuid::new_v4().to_string(),
        display_name: display_name.trim().into(),
    };
    author.validate()?;
    fs::create_dir_all(directory)?;
    let path = directory.join(".profile.lock");
    plain_path(&path, false)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    lock.try_lock_exclusive().map_err(|e| {
        if e.raw_os_error() == fs2::lock_contended_error().raw_os_error() {
            Error::ProjectBusy
        } else {
            Error::Io(e)
        }
    })?;
    if let Some(existing) = get(directory)? {
        author.id = existing.id;
    }
    let mut bytes = serde_json::to_vec_pretty(&Profile {
        format_version: 1,
        author: author.clone(),
    })?;
    bytes.push(b'\n');
    atomic_write(&directory.join("profile.json"), &bytes)?;
    Ok(author)
}
