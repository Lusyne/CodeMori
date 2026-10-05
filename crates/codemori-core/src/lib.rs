//! Storage and runtime information shared by every CodeMori client.

use std::path::PathBuf;

pub mod ai;
pub mod anchors;
pub mod backup;
pub mod code_link;
pub mod context;
pub mod index;
pub mod library;
pub mod model;
pub mod preview;
pub mod profile;
pub mod project;
pub mod rpc;
pub mod search;
pub mod store;
pub use store::Store;

#[cfg(test)]
use rusqlite::Connection;
use serde::Serialize;
use thiserror::Error;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL_VERSION: u32 = 1;
pub const SCHEMA_VERSION: u32 = 4;
/// Shared JSON integers must round-trip exactly through JavaScript clients.
pub const MAX_JSON_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Cannot determine the current user's home directory")]
    HomeUnavailable,
    #[error("Set your CodeMori display name before saving shared data")]
    IdentityRequired,
    #[error("Association or saved code changed; refresh before confirming or repairing")]
    BindingConflict,
    #[error("Data directory must not be empty")]
    InvalidDataDirectory,
    #[error("Filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("Invalid input: {0}")]
    Validation(String),
    #[error("Record or workspace not found: {0}")]
    NotFound(String),
    #[error("This record changed in another client; current revision is {current_revision}")]
    Conflict { current_revision: u64 },
    #[error("Project shared file changed; refresh before saving again")]
    ProjectConflict,
    #[error("Project shared file is busy; retry shortly")]
    ProjectBusy,
    #[error("Invalid JSON data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unsupported database schema {found}; this build supports {SCHEMA_VERSION}")]
    UnsupportedSchema { found: u32 },
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict { .. } | Self::ProjectConflict | Self::BindingConflict => "CONFLICT",
            Self::ProjectBusy => "STORAGE_ERROR",
            Self::Json(_) => "INVALID_JSON",
            Self::IdentityRequired => "IDENTITY_REQUIRED",
            Self::HomeUnavailable => "HOME_UNAVAILABLE",
            Self::InvalidDataDirectory => "INVALID_DATA_DIR",
            Self::Io(_) => "IO_ERROR",
            Self::Storage(_) => "STORAGE_ERROR",
            Self::UnsupportedSchema { .. } => "SCHEMA_UNSUPPORTED",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RuntimeInfo {
    pub version: &'static str,
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    /// The schema supported by this binary; `info` does not inspect the database.
    pub schema_version: u32,
}

pub fn runtime_info(data_dir: Option<PathBuf>) -> Result<RuntimeInfo, Error> {
    let data_dir = match data_dir {
        Some(path) if path.as_os_str().is_empty() => return Err(Error::InvalidDataDirectory),
        Some(path) => path,
        None => dirs::home_dir()
            .ok_or(Error::HomeUnavailable)?
            .join(".codemori"),
    };
    let data_dir = if data_dir.is_absolute() {
        data_dir
    } else {
        std::env::current_dir()?.join(data_dir)
    };
    Ok(RuntimeInfo {
        version: VERSION,
        database_path: data_dir.join("codemori.sqlite3"),
        data_dir,
        schema_version: SCHEMA_VERSION,
    })
}

/// Opens or upgrades local storage, backing up a previous schema before migration.
pub fn initialize(info: &RuntimeInfo) -> Result<(), Error> {
    Store::open(info).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_does_not_create_storage() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("中文 data");
        let info = runtime_info(Some(path.clone())).unwrap();
        assert_eq!(info.database_path, path.join("codemori.sqlite3"));
        assert!(!path.exists());
    }

    #[test]
    fn empty_override_is_rejected() {
        assert!(matches!(
            runtime_info(Some(PathBuf::new())),
            Err(Error::InvalidDataDirectory)
        ));
    }

    #[test]
    fn repeated_initialization_preserves_data() {
        let root = tempfile::tempdir().unwrap();
        let info = runtime_info(Some(root.path().to_owned())).unwrap();
        initialize(&info).unwrap();
        let db = Connection::open(&info.database_path).unwrap();
        db.execute_batch(
            "CREATE TABLE sentinel (value TEXT); INSERT INTO sentinel VALUES ('保留');",
        )
        .unwrap();
        initialize(&info).unwrap();
        let value: String = db
            .query_row("SELECT value FROM sentinel", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "保留");
        assert_eq!(
            db.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
    }

    #[test]
    fn future_schema_is_rejected_without_changes() {
        let root = tempfile::tempdir().unwrap();
        let info = runtime_info(Some(root.path().to_owned())).unwrap();
        let db = Connection::open(&info.database_path).unwrap();
        db.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(matches!(
            initialize(&info),
            Err(Error::UnsupportedSchema { .. })
        ));
        assert_eq!(
            db.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            SCHEMA_VERSION + 1
        );
    }
}
