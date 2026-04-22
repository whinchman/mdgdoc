//! Unit tests for the folder-ID and doc-name resolution helpers in `drive.rs`,
//! plus filesystem-level tests for the BUG-005 unconditional temp-file cleanup.

use mdgdoc::drive::{resolve_doc_name, resolve_folder_id};
use tempfile::tempdir;

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

// ── BUG-005: unconditional temp-file cleanup ─────────────────────────────────
//
// These tests replicate the cleanup pattern used in `cmd_upload` (main.rs):
//
//   let upload_result = async { ... }.await;      // may fail
//   if !keep_docx { remove_file(&temp_docx) ... } // runs unconditionally
//   let file = upload_result?;                     // propagates error after cleanup
//
// They verify that:
//   1. A temp file is deleted even when the "upload" step fails.
//   2. A temp file is preserved when keep_docx is true, regardless of outcome.

/// Helper: simulate the BUG-005-fixed cleanup pattern.
///
/// Creates a real file at `temp_path`, runs a closure that returns a `Result`,
/// and — unless `keep_file` is true — removes `temp_path` unconditionally before
/// propagating the closure's error.  Returns the closure result.
fn run_with_cleanup<F>(temp_path: &std::path::Path, keep_file: bool, op: F) -> anyhow::Result<()>
where
    F: FnOnce() -> anyhow::Result<()>,
{
    let result = op();
    if !keep_file {
        if let Err(e) = std::fs::remove_file(temp_path) {
            eprintln!(
                "Warning: could not delete temp file {}: {e}",
                temp_path.display()
            );
        }
    }
    result
}

/// On upload failure the temp file must be deleted (BUG-005 fix).
#[test]
fn temp_file_deleted_after_upload_failure() {
    let tmp = tempdir().expect("create temp dir");
    let temp_docx = tmp.path().join("sample-abc123.docx");

    // Create the temp file (pandoc would create this in the real flow).
    std::fs::write(&temp_docx, b"fake docx content").expect("write temp file");
    assert!(
        temp_docx.exists(),
        "temp file should exist before upload attempt"
    );

    // Simulate an upload that fails; keep_docx=false.
    let result = run_with_cleanup(&temp_docx, false, || {
        anyhow::bail!("simulated upload failure")
    });

    // The operation should have returned an error...
    assert!(result.is_err(), "expected error from failed upload");
    // ...and the temp file must be gone.
    assert!(
        !temp_docx.exists(),
        "temp file should be deleted even when upload fails (BUG-005 fix)"
    );
}

/// On upload success the temp file must be deleted when keep_docx is false.
#[test]
fn temp_file_deleted_after_upload_success() {
    let tmp = tempdir().expect("create temp dir");
    let temp_docx = tmp.path().join("sample-def456.docx");

    std::fs::write(&temp_docx, b"fake docx content").expect("write temp file");
    assert!(temp_docx.exists(), "temp file should exist before upload");

    // Simulate a successful upload; keep_docx=false.
    let result = run_with_cleanup(&temp_docx, false, || Ok(()));

    assert!(result.is_ok(), "upload succeeded, result should be Ok");
    assert!(
        !temp_docx.exists(),
        "temp file should be deleted after successful upload when keep_docx=false"
    );
}

/// When --keep-docx is set the temp file must be preserved, even on failure.
#[test]
fn temp_file_preserved_when_keep_docx_on_failure() {
    let tmp = tempdir().expect("create temp dir");
    let temp_docx = tmp.path().join("sample-ghi789.docx");

    std::fs::write(&temp_docx, b"fake docx content").expect("write temp file");

    // Simulate a failed upload with keep_docx=true.
    let result = run_with_cleanup(&temp_docx, true, || {
        anyhow::bail!("simulated upload failure with keep_docx")
    });

    assert!(result.is_err(), "expected error from failed upload");
    assert!(
        temp_docx.exists(),
        "temp file should be preserved when keep_docx=true, even on failure"
    );
}

/// When --keep-docx is set the temp file must be preserved after success.
#[test]
fn temp_file_preserved_when_keep_docx_on_success() {
    let tmp = tempdir().expect("create temp dir");
    let temp_docx = tmp.path().join("sample-jkl012.docx");

    std::fs::write(&temp_docx, b"fake docx content").expect("write temp file");

    // Simulate a successful upload with keep_docx=true.
    let result = run_with_cleanup(&temp_docx, true, || Ok(()));

    assert!(result.is_ok(), "upload succeeded, result should be Ok");
    assert!(
        temp_docx.exists(),
        "temp file should be preserved when keep_docx=true after success"
    );
}

// ── resolve_folder_id error message quality ───────────────────────────────────

/// The no-folder error message must mention both --folder and default_folder_id
/// so users know both ways to fix the problem.
#[test]
fn folder_id_error_message_mentions_both_folder_flag_and_config_key() {
    let msg = resolve_folder_id(None, None).unwrap_err().to_string();
    assert!(
        msg.contains("--folder"),
        "error should hint at --folder flag, got: {msg}"
    );
    assert!(
        msg.contains("default_folder_id"),
        "error should hint at default_folder_id config key, got: {msg}"
    );
}
