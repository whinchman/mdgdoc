use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use uuid::Uuid;

use mdgdoc::config;
use mdgdoc::pandoc::{run_libreoffice, run_pandoc};

mod drive;

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
        /// Template name defined in config
        #[arg(short, long, default_value = "default")]
        template: String,
        /// Output path (defaults to input with .docx extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Convert and upload as a native Google Doc
    Upload {
        /// Input markdown file
        input: PathBuf,
        /// Template name defined in config
        #[arg(short, long, default_value = "default")]
        template: String,
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
        /// Template name defined in config
        #[arg(short, long, default_value = "default")]
        template: String,
        /// Output path (defaults to input with .pdf extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
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
        Cmd::Upload { .. } => todo!("not yet implemented"),
        Cmd::Pdf {
            input,
            template,
            output,
        } => cmd_pdf(cli.config, input, template, output)?,
    }

    Ok(())
}

/// Convert a markdown file to a styled `.docx`.
fn cmd_convert(
    config_path: Option<PathBuf>,
    input: PathBuf,
    template: String,
    output: Option<PathBuf>,
) -> Result<()> {
    let reference_doc = if template == "none" {
        None
    } else {
        let cfg = config::load_config(config_path)?;
        config::template_path(&cfg, &template)?
    };

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
    config_path: Option<PathBuf>,
    input: PathBuf,
    template: String,
    output: Option<PathBuf>,
) -> Result<()> {
    let reference_doc = if template == "none" {
        None
    } else {
        let cfg = config::load_config(config_path)?;
        config::template_path(&cfg, &template)?
    };

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

/// Create `~/.config/mdgdoc/config.yaml` and `~/.config/mdgdoc/templates/`.
fn cmd_init() -> Result<()> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine config directory"))?
        .join("mdgdoc");

    let config_path = config_dir.join("config.yaml");
    let templates_dir = config_dir.join("templates");

    std::fs::create_dir_all(&config_dir)?;
    std::fs::create_dir_all(&templates_dir)?;

    if config_path.exists() {
        println!(
            "Config already exists at {}, skipping.",
            config_path.display()
        );
    } else {
        config::write_default_config(&config_path)?;
        println!("Created config at {}", config_path.display());
    }

    println!("Templates directory: {}", templates_dir.display());
    Ok(())
}
