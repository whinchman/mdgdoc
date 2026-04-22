use mdgdoc::pandoc::run_pandoc;
use std::sync::Mutex;
use tempfile::tempdir;

/// A process-wide lock that serialises every test in this file so that the one
/// test which mutates `PATH` cannot race against the tests that rely on the
/// real `pandoc` binary being on `PATH`.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// When pandoc is not on PATH the error message must mention the install URL.
#[test]
fn run_pandoc_missing_binary_error_mentions_install_url() {
    let _guard = ENV_LOCK.lock().unwrap();

    // Save the original PATH so we can restore it unconditionally.
    let original_path = std::env::var_os("PATH");

    let empty_dir = tempdir().expect("create temp dir");
    // Safety: single-threaded access is guaranteed by ENV_LOCK.
    unsafe { std::env::set_var("PATH", empty_dir.path()) };

    let result = run_pandoc(
        &std::path::PathBuf::from("input.md"),
        &std::path::PathBuf::from("output.docx"),
        None,
    );

    // Restore PATH unconditionally before any assert.
    match original_path {
        Some(p) => unsafe { std::env::set_var("PATH", p) },
        None => unsafe { std::env::remove_var("PATH") },
    }

    let err = result.expect_err("expected an error when pandoc is not on PATH");
    let msg = err.to_string();
    assert!(
        msg.contains("https://pandoc.org/installing.html"),
        "error should mention the pandoc install URL, got: {msg}"
    );
}

/// When pandoc exits with a non-zero status the error message should mention
/// "pandoc exited".
#[test]
fn run_pandoc_nonzero_exit_returns_error() {
    let _guard = ENV_LOCK.lock().unwrap();

    if pandoc_path().is_none() {
        eprintln!("skipping: pandoc not installed on PATH");
        return;
    }

    let tmp = tempdir().expect("create temp dir");
    // A non-existent input file causes pandoc to exit non-zero.
    let bad_input = tmp.path().join("does_not_exist.md");
    let output = tmp.path().join("out.docx");

    let result = run_pandoc(&bad_input, &output, None);

    let err = result.expect_err("expected an error for a non-existent input file");
    let msg = err.to_string();
    assert!(
        msg.contains("pandoc exited"),
        "error should say 'pandoc exited', got: {msg}"
    );
}

/// When pandoc is available and the input is valid, run_pandoc succeeds and
/// writes the output file.
#[test]
fn run_pandoc_success_produces_output_file() {
    let _guard = ENV_LOCK.lock().unwrap();

    if pandoc_path().is_none() {
        eprintln!("skipping: pandoc not installed on PATH");
        return;
    }

    let tmp = tempdir().expect("create temp dir");
    let input = tmp.path().join("sample.md");
    let output = tmp.path().join("sample.docx");

    std::fs::write(&input, "# Hello\n\nWorld.\n").expect("write markdown");

    run_pandoc(&input, &output, None).expect("run_pandoc should succeed");

    assert!(
        output.exists(),
        "output .docx should exist after run_pandoc"
    );
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Return the path to the `pandoc` binary, preferring well-known locations so
/// the helper works even when a test has temporarily clobbered `PATH`.
fn pandoc_path() -> Option<std::path::PathBuf> {
    // First try the current PATH.
    if let Some(path_var) = std::env::var_os("PATH") {
        if let Some(p) = std::env::split_paths(&path_var).find_map(|dir| {
            let candidate = dir.join("pandoc");
            candidate.exists().then_some(candidate)
        }) {
            return Some(p);
        }
    }
    // Fallback: well-known system locations.
    ["/usr/bin/pandoc", "/usr/local/bin/pandoc", "/bin/pandoc"]
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(std::path::PathBuf::from)
}
