//! Unit tests for the folder-ID and doc-name resolution helpers in `drive.rs`.

use mdgdoc::drive::{resolve_doc_name, resolve_folder_id};

// ── resolve_folder_id ────────────────────────────────────────────────────────

/// `Some("")` from config is treated as None → error because no folder is set.
#[test]
fn folder_id_empty_config_no_cli_is_error() {
    let result = resolve_folder_id(None, Some(""));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("default_folder_id"),
        "expected hint about default_folder_id, got: {msg}"
    );
}

/// No folder at all (both None) → error.
#[test]
fn folder_id_both_none_is_error() {
    let result = resolve_folder_id(None, None);
    assert!(result.is_err());
}

/// CLI `--folder` takes precedence over a config value.
#[test]
fn folder_id_cli_overrides_config() {
    let result = resolve_folder_id(Some("cli-folder-123"), Some("config-folder-456"));
    assert_eq!(result.unwrap(), "cli-folder-123");
}

/// Config value is used when no `--folder` arg is provided.
#[test]
fn folder_id_config_used_when_no_cli() {
    let result = resolve_folder_id(None, Some("config-folder-789"));
    assert_eq!(result.unwrap(), "config-folder-789");
}

/// An empty CLI `--folder` falls back to the config value.
#[test]
fn folder_id_empty_cli_falls_back_to_config() {
    let result = resolve_folder_id(Some(""), Some("config-folder-abc"));
    assert_eq!(result.unwrap(), "config-folder-abc");
}

// ── resolve_doc_name ─────────────────────────────────────────────────────────

/// `--name` arg is used when present and non-empty.
#[test]
fn doc_name_uses_cli_arg_when_present() {
    let name = resolve_doc_name(Some("My Report"), "fallback-stem");
    assert_eq!(name, "My Report");
}

/// Falls back to the file stem when no `--name` is given.
#[test]
fn doc_name_falls_back_to_stem() {
    let name = resolve_doc_name(None, "my-document");
    assert_eq!(name, "my-document");
}

/// An empty `--name` arg falls back to the file stem.
#[test]
fn doc_name_empty_cli_falls_back_to_stem() {
    let name = resolve_doc_name(Some(""), "my-document");
    assert_eq!(name, "my-document");
}
