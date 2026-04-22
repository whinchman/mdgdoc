use mdgdoc::config::{load_config, template_path};
use std::io::Write;
use tempfile::NamedTempFile;

/// `load_config` must return an error when the config file does not exist.
#[test]
fn load_config_missing_file_returns_error() {
    let result = load_config(Some(std::path::PathBuf::from(
        "/nonexistent/path/config.yaml",
    )));
    assert!(result.is_err(), "expected error for missing config file");
}

/// After loading a config that contains `~/something` paths, no field should
/// start with `~`.
#[test]
fn load_config_expands_tilde() {
    let yaml = "\
credentials_path: ~/some/credentials.json
token_path: ~/some/token.json
templates:
  default: ~/templates/default.docx
";
    let mut tmp = NamedTempFile::new().expect("create temp file");
    tmp.write_all(yaml.as_bytes()).expect("write yaml");
    tmp.flush().expect("flush");

    let cfg = load_config(Some(tmp.path().to_path_buf())).expect("load config");

    assert!(
        !cfg.credentials_path.to_string_lossy().starts_with('~'),
        "credentials_path should have ~ expanded, got: {}",
        cfg.credentials_path.display()
    );
    assert!(
        !cfg.token_path.to_string_lossy().starts_with('~'),
        "token_path should have ~ expanded, got: {}",
        cfg.token_path.display()
    );
    for (name, path) in &cfg.templates {
        assert!(
            !path.to_string_lossy().starts_with('~'),
            "template '{name}' path should have ~ expanded, got: {}",
            path.display()
        );
    }
}

/// `template_path` returns `None` for the special name `"none"`.
#[test]
fn template_path_none_returns_none() {
    let yaml = "\
credentials_path: /tmp/creds.json
token_path: /tmp/token.json
";
    let mut tmp = NamedTempFile::new().expect("create temp file");
    tmp.write_all(yaml.as_bytes()).expect("write yaml");
    tmp.flush().expect("flush");

    let cfg = load_config(Some(tmp.path().to_path_buf())).expect("load config");
    let result = template_path(&cfg, "none").expect("template_path should not error for 'none'");
    assert!(result.is_none());
}

/// `template_path` returns an error for an unknown template name.
#[test]
fn template_path_unknown_returns_error() {
    let yaml = "\
credentials_path: /tmp/creds.json
token_path: /tmp/token.json
templates:
  default: /tmp/default.docx
";
    let mut tmp = NamedTempFile::new().expect("create temp file");
    tmp.write_all(yaml.as_bytes()).expect("write yaml");
    tmp.flush().expect("flush");

    let cfg = load_config(Some(tmp.path().to_path_buf())).expect("load config");
    let result = template_path(&cfg, "does-not-exist");
    assert!(result.is_err(), "expected error for unknown template name");
}
