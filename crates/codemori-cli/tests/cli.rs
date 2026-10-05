use serde_json::Value;
use std::process::{Command, Output};

fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codemori"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn info_is_json_and_does_not_touch_disk() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("中文 data");
    let result = run(&["info".as_ref(), "--data-dir".as_ref(), path.as_os_str()]);
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let body: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(body["protocol_version"], 1);
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["data_dir"], path.to_str().unwrap());
    assert!(!path.exists());
}

#[test]
fn init_is_repeatable_and_creates_sqlite() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("store");
    for _ in 0..2 {
        let result = run(&["--data-dir".as_ref(), path.as_os_str(), "init".as_ref()]);
        assert!(result.status.success(), "{:?}", result);
        let body: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(body["ok"], true);
    }
    let bytes = std::fs::read(path.join("codemori.sqlite3")).unwrap();
    assert!(bytes.starts_with(b"SQLite format 3"));
}

#[test]
fn invalid_command_has_json_error_and_exit_two() {
    let result = run(&["unknown".as_ref()]);
    assert_eq!(result.status.code(), Some(2));
    let body: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "INVALID_ARGUMENT");
}

#[test]
fn storage_failure_cannot_report_success() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("occupied");
    std::fs::write(&path, "keep me").unwrap();
    let result = run(&["init".as_ref(), "--data-dir".as_ref(), path.as_os_str()]);
    assert_eq!(result.status.code(), Some(1));
    let body: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "IO_ERROR");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "keep me");
}
