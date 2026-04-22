//! Thin wrapper around the `pandoc` CLI binary.

use anyhow::{anyhow, bail, Result};
use std::io;
use std::path::Path;
use std::process::Command;

/// Invoke `pandoc` to convert `input` to `output`.
///
/// If `reference_doc` is `Some`, the `--reference-doc` flag is passed so that
/// the output docx is styled with the given template.
///
/// # Errors
///
/// Returns an error when:
/// - `pandoc` is not found on `PATH` (the message includes the install URL).
/// - `pandoc` exits with a non-zero status code.
pub fn run_pandoc(input: &Path, output: &Path, reference_doc: Option<&Path>) -> Result<()> {
    let mut cmd = Command::new("pandoc");
    cmd.arg(input).arg("-o").arg(output);
    if let Some(r) = reference_doc {
        cmd.arg(format!("--reference-doc={}", r.display()));
    }
    let status = cmd.status().map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => {
            anyhow!("pandoc not found on PATH — install: https://pandoc.org/installing.html")
        }
        _ => anyhow!("running pandoc: {e}"),
    })?;
    if !status.success() {
        bail!("pandoc exited with status {status}");
    }
    Ok(())
}
