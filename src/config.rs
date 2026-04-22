//! Configuration loading and path expansion for mdgdoc.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    /// Named reference-doc templates keyed by template name.
    #[serde(default)]
    pub templates: HashMap<String, PathBuf>,
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
    cfg.templates = cfg
        .templates
        .into_iter()
        .map(|(k, v)| (k, expand_path(v)))
        .collect();

    Ok(cfg)
}

/// Resolve a template name to a `PathBuf`.
///
/// Returns `None` when `name` is `"none"` (bypass reference doc).
/// Returns an error if the name is not found in the template map.
pub fn template_path(cfg: &Config, name: &str) -> Result<Option<PathBuf>> {
    if name == "none" {
        return Ok(None);
    }
    cfg.templates
        .get(name)
        .cloned()
        .map(Some)
        .ok_or_else(|| anyhow!("template '{name}' not found in config"))
}

/// Write the default config YAML template to `dest`.
pub fn write_default_config(dest: &Path) -> Result<()> {
    let config_dir = dest
        .parent()
        .ok_or_else(|| anyhow!("config path has no parent directory"))?;
    let templates_dir = config_dir.join("templates");

    let content = format!(
        "\
credentials_path: {config_dir}/credentials.json
token_path: {config_dir}/token.json
default_folder_id: \"\"

templates:
  default: {templates_dir}/default.docx
",
        config_dir = config_dir.display(),
        templates_dir = templates_dir.display(),
    );

    std::fs::write(dest, content).map_err(|e| anyhow!("writing config {}: {e}", dest.display()))?;
    Ok(())
}
