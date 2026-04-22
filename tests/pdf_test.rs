use mdgdoc::pandoc::{run_libreoffice, run_pandoc};
use std::sync::Mutex;
use tempfile::tempdir;

/// A process-wide lock that serialises every test in this file so that the one
/// test which mutates `PATH` cannot race against the tests that rely on the
/// real binaries being on `PATH`.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return `true` if the `libreoffice` binary is found on the current PATH or
/// any well-known system location.  Mirrors the pattern in `pandoc_test.rs`.
fn libreoffice_available() -> bool {
    if let Some(path_var) = std::env::var_os("PATH") {
        if std::env::split_paths(&path_var).any(|dir| dir.join("libreoffice").exists()) {
            return true;
        }
    }
    [
        "/usr/bin/libreoffice",
        "/usr/local/bin/libreoffice",
        "/opt/libreoffice/program/soffice",
    ]
    .iter()
    .any(|p| std::path::Path::new(p).exists())
}

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

    // Safety: ENV_LOCK is still held — restore PATH unconditionally before any assert.
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

    // Safety: ENV_LOCK is still held — restore PATH unconditionally before any assert.
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

// ---------------------------------------------------------------------------
// libreoffice happy-path tests (skip when libreoffice is not installed)
// ---------------------------------------------------------------------------

/// When libreoffice is available, `run_libreoffice` must return a `PathBuf`
/// whose stem matches the input docx stem and whose extension is `pdf`.
#[test]
fn run_libreoffice_success_output_path_matches_docx_stem() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    if !libreoffice_available() {
        eprintln!("skipping: libreoffice not installed on PATH");
        return;
    }

    // First produce a real .docx so libreoffice has valid input.
    let tmp = tempdir().expect("create temp dir");
    let input_md = tmp.path().join("mydoc.md");
    let docx = tmp.path().join("mydoc.docx");

    std::fs::write(&input_md, "# Hello\n\nWorld.\n").expect("write markdown");
    run_pandoc(&input_md, &docx, None).expect("pandoc must be available for this test");

    let result = run_libreoffice(&docx, tmp.path());
    let pdf_path = result.expect("run_libreoffice should succeed with valid docx");

    // Verify the returned path uses the docx stem and has a .pdf extension.
    assert_eq!(
        pdf_path.file_stem().and_then(|s| s.to_str()),
        Some("mydoc"),
        "returned PDF path stem should match docx stem"
    );
    assert_eq!(
        pdf_path.extension().and_then(|s| s.to_str()),
        Some("pdf"),
        "returned PDF path should have .pdf extension"
    );
    assert!(
        pdf_path.exists(),
        "returned PDF path should exist on disk: {}",
        pdf_path.display()
    );
}

/// `run_libreoffice` places the produced PDF inside the requested `out_dir`.
#[test]
fn run_libreoffice_success_pdf_placed_in_out_dir() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    if !libreoffice_available() {
        eprintln!("skipping: libreoffice not installed on PATH");
        return;
    }

    let input_dir = tempdir().expect("create input temp dir");
    let out_dir = tempdir().expect("create output temp dir");

    let input_md = input_dir.path().join("report.md");
    let docx = input_dir.path().join("report.docx");

    std::fs::write(&input_md, "# Report\n\nContent.\n").expect("write markdown");
    run_pandoc(&input_md, &docx, None).expect("pandoc must be available for this test");

    let pdf_path = run_libreoffice(&docx, out_dir.path())
        .expect("run_libreoffice should succeed with valid docx");

    // The returned path must be inside out_dir, not input_dir.
    assert!(
        pdf_path.starts_with(out_dir.path()),
        "PDF should be placed inside out_dir, got: {}",
        pdf_path.display()
    );
}
