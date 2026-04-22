//! Configuration loading and path expansion for mdgdoc.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::templates::templates_dir;

/// Top-level configuration for mdgdoc.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Path to the Google OAuth2 credentials JSON file.
    pub credentials_path: PathBuf,
    /// Path where the cached OAuth2 token is stored.
    pub token_path: PathBuf,
    /// Default Google Drive folder ID for uploads.
    #[serde(default)]
    pub default_folder_id: Option<String>,
}

/// Expand `~` in a `PathBuf` via `shellexpand::tilde`.
fn expand_path(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    PathBuf::from(shellexpand::tilde(s.as_ref()).as_ref())
}

/// Load config from `path`, or from the default location
/// (`$XDG_CONFIG_HOME/mdgdoc/config.yaml`) when `path` is `None`.
///
/// After deserialization `~` is expanded in every `PathBuf` field.
pub fn load_config(path: Option<PathBuf>) -> Result<Config> {
    let config_path: PathBuf = match path {
        Some(p) => p,
        None => dirs::config_dir()
            .ok_or_else(|| anyhow!("cannot determine config directory"))?
            .join("mdgdoc")
            .join("config.yaml"),
    };

    let contents = std::fs::read_to_string(&config_path)
        .map_err(|e| anyhow!("reading config {}: {e}", config_path.display()))?;

    let mut cfg: Config = serde_yml::from_str(&contents)
        .map_err(|e| anyhow!("parsing config {}: {e}", config_path.display()))?;

    // Expand ~ in every PathBuf field.
    cfg.credentials_path = expand_path(cfg.credentials_path);
    cfg.token_path = expand_path(cfg.token_path);

    Ok(cfg)
}

/// Resolve a template name to an `Option<PathBuf>`.
///
/// - `"none"` → `Ok(None)` (bypass reference doc)
/// - any other name → look up `~/.config/mdgdoc/templates/<name>.docx`;
///   returns an error if the file does not exist
pub fn template_path(name: &str) -> Result<Option<PathBuf>> {
    if name == "none" {
        return Ok(None);
    }
    let path = templates_dir()?.join(format!("{name}.docx"));
    if path.exists() {
        Ok(Some(path))
    } else {
        Err(anyhow!(
            "template '{name}' not found (expected at {})",
            path.display()
        ))
    }
}

/// Write the default config YAML template to `dest`.
pub fn write_default_config(dest: &Path) -> Result<()> {
    let config_dir = dest
        .parent()
        .ok_or_else(|| anyhow!("config path has no parent directory"))?;

    let content = format!(
        "\
credentials_path: {config_dir}/credentials.json
token_path: {config_dir}/token.json
default_folder_id: \"\"
",
        config_dir = config_dir.display(),
    );

    std::fs::write(dest, content).map_err(|e| anyhow!("writing config {}: {e}", dest.display()))?;
    Ok(())
}
