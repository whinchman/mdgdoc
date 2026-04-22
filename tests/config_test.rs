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
}

/// `template_path` returns `None` for the special name `"none"`.
#[test]
fn template_path_none_returns_none() {
    let result = template_path("none").expect("template_path should not error for 'none'");
    assert!(result.is_none());
}

/// `template_path` returns an error for an unknown template name.
#[test]
fn template_path_unknown_returns_error() {
    let result = template_path("__nonexistent_template_xyz_12345__");
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
    assert_eq!(
        cfg.credentials_path,
        std::path::PathBuf::from("/tmp/creds.json")
    );
    assert_eq!(cfg.token_path, std::path::PathBuf::from("/tmp/token.json"));
    assert!(
        cfg.default_folder_id.is_none(),
        "default_folder_id should be None when absent"
    );
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
    assert!(
        result.is_err(),
        "expected error when credentials_path is absent"
    );
}

/// `write_default_config` produces a file that `load_config` can successfully parse.
#[test]
fn write_default_config_produces_loadable_config() {
    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let config_path = tmp_dir.path().join("config.yaml");
    write_default_config(&config_path).expect("write default config");

    let cfg = load_config(Some(config_path)).expect("load default config");
    // The scaffolded config writes default_folder_id: "" which deserializes as Some("").
    assert_eq!(
        cfg.default_folder_id,
        Some(String::new()),
        "scaffolded config: default_folder_id should be Some(\"\") (empty string)"
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
}

/// `write_default_config` no longer emits a `templates:` stanza.
#[test]
fn write_default_config_has_no_templates_stanza() {
    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let config_path = tmp_dir.path().join("config.yaml");
    write_default_config(&config_path).expect("write default config");

    let contents = std::fs::read_to_string(&config_path).expect("read config");
    assert!(
        !contents.contains("templates:"),
        "default config should not contain a templates: stanza, got:\n{contents}"
    );
}
