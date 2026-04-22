//! Google Drive upload helpers using `yup-oauth2` and `reqwest`.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::path::Path;

use crate::config::Config;

/// Metadata returned by the Drive API after a successful upload.
#[derive(Debug, Deserialize)]
pub struct DriveFile {
    /// Google Drive file ID.
    pub id: String,
    /// Display name of the created Google Doc.
    pub name: String,
    /// URL to open the file in the browser.
    #[serde(rename = "webViewLink")]
    pub web_view_link: String,
}

/// Obtain an OAuth2 access token for the Drive scope.
///
/// Reads OAuth2 client credentials from `cfg.credentials_path`.  On the first
/// run the user will be prompted to complete the browser-based consent flow.
/// Subsequent runs reuse the cached token stored at `cfg.token_path`.
///
/// # Errors
///
/// Returns a descriptive error if `credentials.json` is missing (with a hint
/// to visit the Google Cloud Console) or if the OAuth2 flow fails.
pub async fn get_token(cfg: &Config) -> Result<yup_oauth2::AccessToken> {
    let secret = yup_oauth2::read_application_secret(&cfg.credentials_path)
        .await
        .map_err(|e| {
            anyhow!(
                "reading credentials.json: {e}\n\
                 Create OAuth Desktop credentials at \
                 https://console.cloud.google.com/"
            )
        })?;

    let auth = yup_oauth2::InstalledFlowAuthenticator::builder(
        secret,
        yup_oauth2::InstalledFlowReturnMethod::Interactive,
    )
    .persist_tokens_to_disk(&cfg.token_path)
    .build()
    .await?;

    Ok(auth
        .token(&["https://www.googleapis.com/auth/drive"])
        .await?)
}

/// Upload `docx_path` to Google Drive, converting it to a native Google Doc.
///
/// The file is placed in `folder_id`.  On success the Drive API response is
/// returned, which contains the file's `id`, `name`, and `webViewLink`.
///
/// # Errors
///
/// Returns an error if the OAuth token string is missing, if reading the docx
/// file fails, or if the HTTP request fails or returns an error status.
pub async fn upload_docx(
    token: &yup_oauth2::AccessToken,
    docx_path: &Path,
    name: &str,
    folder_id: &str,
) -> Result<DriveFile> {
    let metadata = serde_json::json!({
        "name": name,
        "parents": [folder_id],
        "mimeType": "application/vnd.google-apps.document",
    });

    let docx_bytes = tokio::fs::read(docx_path).await?;

    let form = reqwest::multipart::Form::new()
        .part(
            "metadata",
            reqwest::multipart::Part::text(metadata.to_string()).mime_str("application/json")?,
        )
        .part(
            "file",
            reqwest::multipart::Part::bytes(docx_bytes).mime_str(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            )?,
        );

    let token_str = token.token().ok_or_else(|| {
        anyhow!("OAuth token missing — delete the cached token file and re-authenticate")
    })?;

    let client = reqwest::Client::new();
    let resp: DriveFile = client
        .post("https://www.googleapis.com/upload/drive/v3/files")
        .query(&[
            ("uploadType", "multipart"),
            ("fields", "id,name,webViewLink"),
        ])
        .bearer_auth(token_str)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(resp)
}

/// Resolve the effective folder ID from optional CLI arg and optional config value.
///
/// Priority: `cli_folder` > `config_folder` (empty string treated as `None`).
/// Returns an error if neither source provides a non-empty value.
pub fn resolve_folder_id<'a>(
    cli_folder: Option<&'a str>,
    config_folder: Option<&'a str>,
) -> Result<&'a str> {
    cli_folder
        .filter(|s| !s.is_empty())
        .or_else(|| config_folder.filter(|s| !s.is_empty()))
        .ok_or_else(|| anyhow!("set default_folder_id in config or pass --folder"))
}

/// Resolve the effective document name from optional CLI arg and the input file stem.
///
/// Uses `cli_name` when present; falls back to `stem`.
pub fn resolve_doc_name<'a>(cli_name: Option<&'a str>, stem: &'a str) -> &'a str {
    cli_name.filter(|s| !s.is_empty()).unwrap_or(stem)
}
