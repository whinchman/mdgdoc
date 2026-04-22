use mdgdoc::config::{load_config, template_path, write_default_config};
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

/// `load_config` succeeds on a valid minimal YAML and returns expected field values.
#[test]
fn load_config_valid_minimal_yaml_succeeds() {
    let yaml = "\
credentials_path: /tmp/creds.json
token_path: /tmp/token.json
";
    let mut tmp = NamedTempFile::new().expect("create temp file");
    tmp.write_all(yaml.as_bytes()).expect("write yaml");
    tmp.flush().expect("flush");

    let cfg = load_config(Some(tmp.path().to_path_buf())).expect("load config");
    assert_eq!(cfg.credentials_path, std::path::PathBuf::from("/tmp/creds.json"));
    assert_eq!(cfg.token_path, std::path::PathBuf::from("/tmp/token.json"));
    assert!(cfg.default_folder_id.is_none(), "default_folder_id should be None when absent");
    assert!(cfg.templates.is_empty(), "templates map should be empty when absent");
}

/// `load_config` returns an error when the YAML is structurally invalid.
#[test]
fn load_config_malformed_yaml_returns_error() {
    let bad_yaml = "this: is: not: valid: yaml: [[[";
    let mut tmp = NamedTempFile::new().expect("create temp file");
    tmp.write_all(bad_yaml.as_bytes()).expect("write yaml");
    tmp.flush().expect("flush");

    let result = load_config(Some(tmp.path().to_path_buf()));
    assert!(result.is_err(), "expected error for malformed YAML");
}

/// `load_config` returns an error when a required field is missing.
#[test]
fn load_config_missing_required_field_returns_error() {
    // credentials_path is required; omitting it should cause a deserialization error.
    let yaml = "token_path: /tmp/token.json\n";
    let mut tmp = NamedTempFile::new().expect("create temp file");
    tmp.write_all(yaml.as_bytes()).expect("write yaml");
    tmp.flush().expect("flush");

    let result = load_config(Some(tmp.path().to_path_buf()));
    assert!(result.is_err(), "expected error when credentials_path is absent");
}

/// `template_path` returns the correct `PathBuf` for a known template name.
#[test]
fn template_path_known_name_returns_path() {
    let yaml = "\
credentials_path: /tmp/creds.json
token_path: /tmp/token.json
templates:
  report: /tmp/report.docx
";
    let mut tmp = NamedTempFile::new().expect("create temp file");
    tmp.write_all(yaml.as_bytes()).expect("write yaml");
    tmp.flush().expect("flush");

    let cfg = load_config(Some(tmp.path().to_path_buf())).expect("load config");
    let path = template_path(&cfg, "report")
        .expect("template_path should not error for a known name")
        .expect("expected Some(path) for a known template");
    assert_eq!(path, std::path::PathBuf::from("/tmp/report.docx"));
}

/// `write_default_config` produces a file that `load_config` can successfully parse.
#[test]
fn write_default_config_produces_loadable_config() {
    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let config_path = tmp_dir.path().join("config.yaml");
    // write_default_config also needs the templates/ subdir to be reachable;
    // the function itself only writes a file — it does not create directories.
    write_default_config(&config_path).expect("write default config");

    let cfg = load_config(Some(config_path)).expect("load default config");
    // The scaffolded config writes default_folder_id: "" which deserializes as Some("").
    // Verify that the field is present (Some) and contains an empty string — documenting
    // the known behaviour flagged by the code reviewer (see task Notes).
    assert_eq!(
        cfg.default_folder_id,
        Some(String::new()),
        "scaffolded config: default_folder_id should be Some(\"\") (empty string) — \
         see code-review warning about treating this as None in upload logic"
    );
    // credentials_path and token_path should be absolute (no tilde).
    assert!(
        cfg.credentials_path.is_absolute(),
        "credentials_path should be absolute after tilde expansion, got: {}",
        cfg.credentials_path.display()
    );
    assert!(
        cfg.token_path.is_absolute(),
        "token_path should be absolute after tilde expansion, got: {}",
        cfg.token_path.display()
    );
    // The default template entry should exist.
    assert!(
        cfg.templates.contains_key("default"),
        "scaffolded config should contain a 'default' template entry"
    );
}
