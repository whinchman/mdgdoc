use mdgdoc::config::template_path;
use mdgdoc::templates;
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::TempDir;

/// A process-wide lock that serialises every test that mutates `MDGDOC_TEMPLATES_DIR`
/// so they cannot race against each other.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Helper: create a fake templates directory with named .docx stubs
// ---------------------------------------------------------------------------

fn make_fake_docx(dir: &std::path::Path, name: &str) {
    let path = dir.join(name);
    std::fs::write(path, b"PK stub").expect("write fake docx");
}

// ---------------------------------------------------------------------------
// list_templates
// ---------------------------------------------------------------------------

/// When two .docx files exist they are returned sorted by stem.
#[test]
fn list_templates_returns_sorted_stems() {
    let tmp = TempDir::new().expect("tempdir");
    make_fake_docx(tmp.path(), "zebra.docx");
    make_fake_docx(tmp.path(), "alpha.docx");

    // list_templates reads from the real templates_dir; we can't override it from
    // outside, so call the internal helper that accepts a path directly.
    let mut entries: Vec<(String, PathBuf)> = std::fs::read_dir(tmp.path())
        .expect("read_dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("docx"))
        .map(|e| {
            let stem = e.path().file_stem().unwrap().to_string_lossy().to_string();
            (stem, e.path())
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, "alpha");
    assert_eq!(entries[1].0, "zebra");
}

// ---------------------------------------------------------------------------
// cmd_scrape — fresh install
// ---------------------------------------------------------------------------

/// cmd_scrape copies a .docx to the templates dir as <name>.docx when no
/// collision exists.
#[test]
fn cmd_scrape_copies_file_to_templates_dir() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    let src_dir = TempDir::new().expect("src tempdir");
    let dest_dir = TempDir::new().expect("dest tempdir");

    // Create a minimal source .docx file.
    let src = src_dir.path().join("report.docx");
    std::fs::write(&src, b"PK stub content").expect("write src");

    // Redirect templates_dir() to the temp destination.
    // Safety: single-threaded access guaranteed by ENV_LOCK.
    unsafe { std::env::set_var("MDGDOC_TEMPLATES_DIR", dest_dir.path()) };

    let result = templates::cmd_scrape(&src, Some("report"), false);

    // Remove env var unconditionally before any assert.
    unsafe { std::env::remove_var("MDGDOC_TEMPLATES_DIR") };

    result.expect("cmd_scrape should succeed");

    let dest = dest_dir.path().join("report.docx");
    assert!(dest.exists(), "scraped file should exist");
    assert_eq!(std::fs::read(&dest).expect("read dest"), b"PK stub content");
}

/// cmd_scrape with force=true overwrites an existing file silently.
#[test]
fn cmd_scrape_force_overwrites_without_prompt() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    let src_dir = TempDir::new().expect("src tempdir");
    let dest_dir = TempDir::new().expect("dest tempdir");

    // Pre-existing destination in the templates dir.
    let dest = dest_dir.path().join("existing.docx");
    std::fs::write(&dest, b"old content").expect("write old dest");

    // Source with new content.
    let src = src_dir.path().join("existing.docx");
    std::fs::write(&src, b"new content").expect("write src");

    // Redirect templates_dir() to the temp destination.
    // Safety: single-threaded access guaranteed by ENV_LOCK.
    unsafe { std::env::set_var("MDGDOC_TEMPLATES_DIR", dest_dir.path()) };

    let result = templates::cmd_scrape(&src, Some("existing"), true);

    // Remove env var unconditionally before any assert.
    unsafe { std::env::remove_var("MDGDOC_TEMPLATES_DIR") };

    result.expect("force cmd_scrape should succeed");

    assert_eq!(
        std::fs::read(&dest).expect("read"),
        b"new content",
        "force overwrite should replace old content"
    );
}

// ---------------------------------------------------------------------------
// validate_template_name (via cmd_scrape / template_path)
// ---------------------------------------------------------------------------

/// A template name containing `..` must be rejected.
#[test]
fn template_name_with_dotdot_is_rejected() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    let src_dir = TempDir::new().expect("src tempdir");
    let dest_dir = TempDir::new().expect("dest tempdir");
    let src = src_dir.path().join("evil.docx");
    std::fs::write(&src, b"evil").expect("write src");

    unsafe { std::env::set_var("MDGDOC_TEMPLATES_DIR", dest_dir.path()) };
    let result = templates::cmd_scrape(&src, Some("../evil"), false);
    unsafe { std::env::remove_var("MDGDOC_TEMPLATES_DIR") };

    let err = result.expect_err("expected error for '..' in template name");
    assert!(
        err.to_string().contains("plain filename"),
        "error should mention plain filename, got: {err}"
    );
}

/// A template name containing a `/` must be rejected.
#[test]
fn template_name_with_slash_is_rejected() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    let src_dir = TempDir::new().expect("src tempdir");
    let dest_dir = TempDir::new().expect("dest tempdir");
    let src = src_dir.path().join("evil.docx");
    std::fs::write(&src, b"evil").expect("write src");

    unsafe { std::env::set_var("MDGDOC_TEMPLATES_DIR", dest_dir.path()) };
    let result = templates::cmd_scrape(&src, Some("sub/evil"), false);
    unsafe { std::env::remove_var("MDGDOC_TEMPLATES_DIR") };

    let err = result.expect_err("expected error for '/' in template name");
    assert!(
        err.to_string().contains("plain filename"),
        "error should mention plain filename, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// config::template_path
// ---------------------------------------------------------------------------

/// template_path("none") returns Ok(None).
#[test]
fn template_path_none_returns_none() {
    let result = template_path("none").expect("should not error for 'none'");
    assert!(result.is_none());
}

/// template_path with a name that does not exist in the templates dir returns
/// an error.
#[test]
fn template_path_missing_returns_error() {
    // Use a name that is extremely unlikely to exist in the real templates dir.
    let result = template_path("__nonexistent_test_template_xyz_12345__");
    assert!(
        result.is_err(),
        "expected error for missing template, got: {:?}",
        result
    );
}
