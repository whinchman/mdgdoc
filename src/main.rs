use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use uuid::Uuid;

use mdgdoc::config;
use mdgdoc::drive::{get_token, resolve_doc_name, resolve_folder_id, upload_docx};
use mdgdoc::pandoc::{run_libreoffice, run_pandoc};
use mdgdoc::templates;

/// Markdown → styled docx / Google Doc / PDF
#[derive(Parser)]
#[command(name = "mdgdoc", about = "Markdown → styled docx/Google Doc/PDF")]
struct Cli {
    /// Path to an alternative config file
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scaffold config file and templates directory
    Init,

    /// Convert markdown to styled .docx locally
    Convert {
        /// Input markdown file
        input: PathBuf,
        /// Template name, or "none" to skip styling (omit to pick interactively)
        #[arg(short, long)]
        template: Option<String>,
        /// Output path (defaults to input with .docx extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Convert and upload as a native Google Doc
    Upload {
        /// Input markdown file
        input: PathBuf,
        /// Template name, or "none" to skip styling (omit to pick interactively)
        #[arg(short, long)]
        template: Option<String>,
        /// Target Google Drive folder ID (overrides config)
        #[arg(short, long)]
        folder: Option<String>,
        /// Name for the Google Doc (defaults to input filename stem)
        #[arg(short, long)]
        name: Option<String>,
        /// Keep the intermediate .docx file after upload
        #[arg(long)]
        keep_docx: bool,
    },

    /// Convert to PDF via docx + LibreOffice
    Pdf {
        /// Input markdown file
        input: PathBuf,
        /// Template name, or "none" to skip styling (omit to pick interactively)
        #[arg(short, long)]
        template: Option<String>,
        /// Output path (defaults to input with .pdf extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Manage reference-doc templates
    Template {
        #[command(subcommand)]
        action: TemplateCmd,
    },
}

#[derive(Subcommand)]
enum TemplateCmd {
    /// Copy a .docx file into the templates store
    Scrape {
        /// Path to the source .docx file
        path: PathBuf,
        /// Name to register the template under (defaults to filename stem)
        #[arg(long)]
        name: Option<String>,
        /// Overwrite an existing template without prompting
        #[arg(long)]
        force: bool,
    },
    /// List installed templates
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Init => cmd_init()?,
        Cmd::Convert {
            input,
            template,
            output,
        } => cmd_convert(cli.config, input, template, output)?,
        Cmd::Upload {
            input,
            template,
            folder,
            name,
            keep_docx,
        } => cmd_upload(cli.config, input, template, folder, name, keep_docx).await?,
        Cmd::Pdf {
            input,
            template,
            output,
        } => cmd_pdf(cli.config, input, template, output)?,
        Cmd::Template { action } => match action {
            TemplateCmd::Scrape { path, name, force } => {
                templates::cmd_scrape(&path, name.as_deref(), force)?
            }
            TemplateCmd::List => templates::cmd_list()?,
        },
    }

    Ok(())
}

/// Resolve the `--template` argument for convert/pdf/upload.
///
/// - `None` (omitted) → launch interactive picker
/// - `Some("none")` → bypass reference doc (`Ok(None)`)
/// - `Some(name)` → look up `~/.config/mdgdoc/templates/<name>.docx`
fn resolve_template(template: Option<&str>) -> Result<Option<PathBuf>> {
    match template {
        None => templates::pick_interactive().map(Some),
        Some("none") => Ok(None),
        Some(name) => config::template_path(name),
    }
}

/// Convert a markdown file to a styled `.docx`.
fn cmd_convert(
    _config_path: Option<PathBuf>,
    input: PathBuf,
    template: Option<String>,
    output: Option<PathBuf>,
) -> Result<()> {
    let reference_doc = resolve_template(template.as_deref())?;

    let out = match output {
        Some(p) => p,
        None => input.with_extension("docx"),
    };

    run_pandoc(&input, &out, reference_doc.as_deref())?;
    println!("Converted: {}", out.display());
    Ok(())
}

/// Convert a markdown file to PDF via a temporary docx intermediate.
fn cmd_pdf(
    _config_path: Option<PathBuf>,
    input: PathBuf,
    template: Option<String>,
    output: Option<PathBuf>,
) -> Result<()> {
    let reference_doc = resolve_template(template.as_deref())?;

    // Build a unique temp docx path: <tempdir>/<stem>-<uuid>.docx
    let stem = input
        .file_stem()
        .unwrap_or_else(|| std::ffi::OsStr::new("input"))
        .to_string_lossy();
    let temp_docx = std::env::temp_dir().join(format!("{stem}-{}.docx", Uuid::new_v4()));

    // Step 1: markdown → docx
    let pandoc_result = run_pandoc(&input, &temp_docx, reference_doc.as_deref());

    // Step 2: docx → pdf (only if pandoc succeeded)
    let lo_result = pandoc_result.and_then(|()| run_libreoffice(&temp_docx, &std::env::temp_dir()));

    // Clean up temp docx regardless of outcome
    let _ = std::fs::remove_file(&temp_docx);

    let tmp_pdf = lo_result?;

    // Derive final output path
    let final_pdf = match output {
        Some(p) => p,
        None => input.with_extension("pdf"),
    };

    // Move the PDF from temp dir to the final destination.
    // Cross-device rename fails with os error 18 (EXDEV); fall back to copy + delete.
    if let Err(e) = std::fs::rename(&tmp_pdf, &final_pdf) {
        if e.raw_os_error() == Some(18) {
            std::fs::copy(&tmp_pdf, &final_pdf)?;
            let _ = std::fs::remove_file(&tmp_pdf);
        } else {
            let _ = std::fs::remove_file(&tmp_pdf);
            return Err(e.into());
        }
    }

    println!("PDF: {}", final_pdf.display());
    Ok(())
}

/// Convert a markdown file to docx then upload it as a native Google Doc.
async fn cmd_upload(
    config_path: Option<PathBuf>,
    input: PathBuf,
    template: Option<String>,
    folder: Option<String>,
    name: Option<String>,
    keep_docx: bool,
) -> Result<()> {
    let cfg = config::load_config(config_path)?;

    // Resolve folder ID — BUG-001: treat Some("") as None.
    let config_folder = cfg.default_folder_id.as_deref().filter(|s| !s.is_empty());
    let folder_id = resolve_folder_id(folder.as_deref(), config_folder)?;

    // Resolve doc name.
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("input path has no valid file stem"))?;
    let doc_name = resolve_doc_name(name.as_deref(), stem);

    // Resolve reference doc template.
    let reference_doc = resolve_template(template.as_deref())?;

    // Build temp docx path: <tmp>/<stem>-<uuid>.docx
    let temp_docx = std::env::temp_dir().join(format!("{stem}-{}.docx", Uuid::new_v4()));

    run_pandoc(&input, &temp_docx, reference_doc.as_deref())?;

    let upload_result = async {
        let token = get_token(&cfg).await?;
        upload_docx(&token, &temp_docx, doc_name, folder_id).await
    }
    .await;

    if !keep_docx {
        if let Err(e) = std::fs::remove_file(&temp_docx) {
            eprintln!(
                "Warning: could not delete temp file {}: {e}",
                temp_docx.display()
            );
        }
    }

    let file = upload_result?;

    println!("Uploaded: {}", file.name);
    println!("Link:     {}", file.web_view_link);
    Ok(())
}

/// Create `~/.config/mdgdoc/config.yaml` and `~/.config/mdgdoc/templates/`.
fn cmd_init() -> Result<()> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine config directory"))?
        .join("mdgdoc");

    let config_path = config_dir.join("config.yaml");
    let tpl_dir = templates::templates_dir()?;

    std::fs::create_dir_all(&config_dir)?;
    std::fs::create_dir_all(&tpl_dir)?;

    if config_path.exists() {
        println!(
            "Config already exists at {}, skipping.",
            config_path.display()
        );
    } else {
        config::write_default_config(&config_path)?;
        println!("Created config at {}", config_path.display());
    }

    println!("Templates directory: {}", tpl_dir.display());
    Ok(())
}
