//! Template management for mdgdoc: scrape, list, and interactive selection.

use anyhow::{anyhow, bail, Result};
use std::path::{Path, PathBuf};

/// Return the templates directory: `~/.config/mdgdoc/templates/`.
pub fn templates_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| anyhow!("cannot determine config directory"))?
        .join("mdgdoc")
        .join("templates");
    Ok(dir)
}

/// Scan `templates_dir()` and return a sorted list of `(stem, path)` pairs.
///
/// The `stem` is the file name without extension (e.g. `"company"` for
/// `company.docx`).  Only `.docx` files are returned.
pub fn list_templates() -> Result<Vec<(String, PathBuf)>> {
    let dir = templates_dir()?;
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut entries: Vec<(String, PathBuf)> = std::fs::read_dir(&dir)
        .map_err(|e| anyhow!("reading templates dir {}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("docx"))
        .map(|e| {
            let stem = e
                .path()
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            (stem, e.path())
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries)
}

/// Copy `src` into the templates directory as `<name>.docx`.
///
/// - If `name` is `None`, the file stem of `src` is used.
/// - If the destination already exists and `force` is `false`, the user is
///   prompted interactively before overwriting.
/// - Prints a confirmation message on success.
pub fn cmd_scrape(src: &Path, name: Option<&str>, force: bool) -> Result<()> {
    let stem: String = match name {
        Some(n) => n.to_string(),
        None => src
            .file_stem()
            .ok_or_else(|| anyhow!("source path has no file stem"))?
            .to_string_lossy()
            .to_string(),
    };

    let dir = templates_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| anyhow!("creating templates dir {}: {e}", dir.display()))?;

    let dest = dir.join(format!("{stem}.docx"));

    if dest.exists() && !force {
        let answer =
            inquire::Confirm::new(&format!("Template '{stem}' already exists. Overwrite?"))
                .with_default(false)
                .prompt()
                .map_err(|e| anyhow!("prompt error: {e}"))?;

        if !answer {
            bail!("Aborted.");
        }
    }

    std::fs::copy(src, &dest)
        .map_err(|e| anyhow!("copying {} to {}: {e}", src.display(), dest.display()))?;

    println!("Template '{stem}' installed at {}", dest.display());
    Ok(())
}

/// Print all installed templates to stdout.
///
/// Prints a help message when no templates are found.
pub fn cmd_list() -> Result<()> {
    let templates = list_templates()?;
    if templates.is_empty() {
        println!("No templates installed. Run: mdgdoc template scrape <path>");
    } else {
        for (stem, _) in &templates {
            println!("{stem}");
        }
    }
    Ok(())
}

/// Present an interactive `inquire::Select` picker and return the chosen
/// template path.
///
/// Returns `Err` if no templates are installed, the user cancels, or the
/// environment is not a TTY.
pub fn pick_interactive() -> Result<PathBuf> {
    let templates = list_templates()?;
    if templates.is_empty() {
        bail!("No templates installed. Run: mdgdoc template scrape <path/to/file.docx>");
    }

    let names: Vec<String> = templates.iter().map(|(s, _)| s.clone()).collect();

    let selected = inquire::Select::new("Pick a template:", names)
        .prompt()
        .map_err(|e| match e {
            inquire::InquireError::OperationCanceled
            | inquire::InquireError::OperationInterrupted => anyhow!("Aborted."),
            inquire::InquireError::NotTTY => anyhow!(
                "No template specified and stdin is not a terminal. \
                 Pass --template <name> or --template none."
            ),
            other => anyhow!("picker error: {other}"),
        })?;

    let path = templates
        .into_iter()
        .find(|(s, _)| s == &selected)
        .map(|(_, p)| p)
        .ok_or_else(|| anyhow!("selected template not found"))?;

    Ok(path)
}
