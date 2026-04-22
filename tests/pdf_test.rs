use mdgdoc::pandoc::{run_libreoffice, run_pandoc};
use std::sync::Mutex;
use tempfile::tempdir;

/// A process-wide lock that serialises every test in this file so that the one
/// test which mutates `PATH` cannot race against the tests that rely on the
/// real binaries being on `PATH`.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// libreoffice error-path tests
// ---------------------------------------------------------------------------

/// When `libreoffice` is not on PATH the error message must say so clearly.
#[test]
fn run_libreoffice_missing_binary_error_mentions_path() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    let original_path = std::env::var_os("PATH");

    let empty_dir = tempdir().expect("create temp dir");
    // Safety: single-threaded access guaranteed by ENV_LOCK.
    unsafe { std::env::set_var("PATH", empty_dir.path()) };

    let tmp = tempdir().expect("create temp dir");
    let fake_docx = tmp.path().join("fake.docx");
    let result = run_libreoffice(&fake_docx, tmp.path());

    // Restore PATH unconditionally before any assert.
    match original_path {
        Some(p) => unsafe { std::env::set_var("PATH", p) },
        None => unsafe { std::env::remove_var("PATH") },
    }

    let err = result.expect_err("expected an error when libreoffice is not on PATH");
    let msg = err.to_string();
    assert!(
        msg.contains("libreoffice not found on PATH"),
        "error should mention libreoffice not found, got: {msg}"
    );
    assert!(
        msg.contains("LibreOffice"),
        "error should mention LibreOffice install suggestion, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// pandoc not-found propagation in pdf flow
// ---------------------------------------------------------------------------

/// When `pandoc` is not on PATH, calling `run_pandoc` (as cmd_pdf would) must
/// propagate an error that mentions the pandoc install URL.
#[test]
fn run_pandoc_missing_propagates_in_pdf_flow() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    let original_path = std::env::var_os("PATH");

    let empty_dir = tempdir().expect("create temp dir");
    // Safety: single-threaded access guaranteed by ENV_LOCK.
    unsafe { std::env::set_var("PATH", empty_dir.path()) };

    let tmp = tempdir().expect("create temp dir");
    let input = tmp.path().join("sample.md");
    let temp_docx = tmp.path().join("sample-abc.docx");

    std::fs::write(&input, "# Hello\n").expect("write markdown");

    // Simulate what cmd_pdf does: first call run_pandoc, only call run_libreoffice on success.
    let pandoc_result = run_pandoc(&input, &temp_docx, None);

    // Restore PATH unconditionally before any assert.
    match original_path {
        Some(p) => unsafe { std::env::set_var("PATH", p) },
        None => unsafe { std::env::remove_var("PATH") },
    }

    let err = pandoc_result.expect_err("expected error when pandoc is not on PATH");
    let msg = err.to_string();
    assert!(
        msg.contains("https://pandoc.org/installing.html"),
        "error should propagate pandoc install URL, got: {msg}"
    );
}
