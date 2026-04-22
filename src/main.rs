use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use mdgdoc::config;
use mdgdoc::pandoc::run_pandoc;

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
        Cmd::Pdf { .. } => todo!("not yet implemented"),
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
