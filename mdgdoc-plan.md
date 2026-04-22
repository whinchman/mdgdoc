# mdgdoc — Build Plan

A Rust CLI that turns markdown into company-styled Word docs, Google Docs,
and PDFs. Wraps `pandoc --reference-doc=...`, the Google Drive API, and
`libreoffice --headless --convert-to pdf` into one tool.

## What it does

```
  my-notes.md
      │
      ├─ convert ──► my-notes.docx  (styled with company template)
      │
      ├─ upload  ──► Google Doc in Drive folder (commentable)
      │
      └─ pdf     ──► my-notes.pdf   (LibreOffice-rendered)
```

The styling trick: pandoc's `--reference-doc=template.docx` flag maps
markdown elements to the named styles (Heading 1, Normal, Code, etc.) in
the reference doc. Export the company Google Doc as `.docx`, keep it as a
"style primer," and pandoc inherits the full look automatically.

## External dependencies

- `pandoc` on PATH — required for all commands
- `libreoffice` on PATH — required for `pdf` command only
- Google Cloud project with Drive API enabled + OAuth Desktop app
  credentials — required for `upload` only

## Cargo.toml

```toml
[package]
name    = "mdgdoc"
version = "0.1.0"
edition = "2021"

[dependencies]
clap         = { version = "4", features = ["derive"] }
serde        = { version = "1", features = ["derive"] }
serde_yaml   = "0.9"
serde_json   = "1"
anyhow       = "1"
tokio        = { version = "1", features = ["rt-multi-thread", "macros", "fs"] }
reqwest      = { version = "0.12", features = ["json", "multipart"] }
yup-oauth2   = "11"
shellexpand  = "3"
dirs         = "5"
```

### Crate choices worth noting

- **`yup-oauth2`** handles installed-app OAuth including token caching to
  disk. It's the idiomatic Rust equivalent of Go's `oauth2` package.
- **Skip `google-drive3`.** The auto-generated Drive API crate is enormous
  and compile-slow. Hand-rolling the multipart upload against the REST API
  is ~40 lines of `reqwest` and keeps builds fast.
- **`anyhow`** everywhere in the CLI layer — `thiserror` is overkill for a
  tool this size.

## Module layout

```
mdgdoc/
├── Cargo.toml
└── src/
    ├── main.rs       # clap CLI, command dispatch
    ├── config.rs     # Config struct, YAML load, path expansion
    ├── pandoc.rs     # subprocess wrapper
    └── drive.rs      # OAuth + multipart upload
```

## CLI shape

```rust
#[derive(Parser)]
#[command(name = "mdgdoc", about = "Markdown → styled docx/Google Doc/PDF")]
struct Cli {
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
        input: PathBuf,
        #[arg(short, long, default_value = "default")]
        template: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Convert and upload as a native Google Doc
    Upload {
        input: PathBuf,
        #[arg(short, long, default_value = "default")]
        template: String,
        #[arg(short, long)]
        folder: Option<String>,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(long)]
        keep_docx: bool,
    },
    /// Convert to PDF via docx + LibreOffice
    Pdf {
        input: PathBuf,
        #[arg(short, long, default_value = "default")]
        template: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}
```

## Config shape

`~/.config/mdgdoc/config.yaml`:

```yaml
credentials_path: ~/.config/mdgdoc/credentials.json
token_path:       ~/.config/mdgdoc/token.json
default_folder_id: "1abc...xyz"

templates:
  default:     ~/.config/mdgdoc/templates/default.docx
  memo:        ~/.config/mdgdoc/templates/memo.docx
  engineering: ~/.config/mdgdoc/templates/eng.docx
```

```rust
#[derive(Deserialize)]
struct Config {
    credentials_path: PathBuf,
    token_path: PathBuf,
    #[serde(default)]
    default_folder_id: Option<String>,
    #[serde(default)]
    templates: HashMap<String, PathBuf>,
}
```

Use `shellexpand::tilde()` on every `PathBuf` field after deserialization.
Default config location is `dirs::config_dir()` + `mdgdoc/config.yaml` —
cross-platform without hand-rolling XDG logic.

A template name of `none` should bypass the reference doc entirely and let
pandoc use its defaults.

## Pandoc wrapper

Sync, dead simple:

```rust
pub fn run_pandoc(input: &Path, output: &Path, reference_doc: Option<&Path>) -> Result<()> {
    let mut cmd = Command::new("pandoc");
    cmd.arg(input).arg("-o").arg(output);
    if let Some(r) = reference_doc {
        cmd.arg(format!("--reference-doc={}", r.display()));
    }
    let status = cmd.status().map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => anyhow!("pandoc not found on PATH — install: https://pandoc.org/installing.html"),
        _ => anyhow!("running pandoc: {e}"),
    })?;
    if !status.success() {
        bail!("pandoc exited with status {status}");
    }
    Ok(())
}
```

## Drive piece — two halves

### OAuth via `yup-oauth2`

```rust
let secret = yup_oauth2::read_application_secret(&cfg.credentials_path).await?;
let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::Interactive)
    .persist_tokens_to_disk(&cfg.token_path)
    .build()
    .await?;
let token = auth.token(&["https://www.googleapis.com/auth/drive"]).await?;
```

First run: library prints a URL, user pastes the code back, token cached.
Subsequent runs: token loaded and refreshed automatically.

### Multipart upload to Drive

Drive's "upload + convert to Google Doc" trick: send multipart where part 1
is JSON metadata with `mimeType: "application/vnd.google-apps.document"`
(signals "convert on import") and part 2 is the `.docx` bytes.

```rust
let metadata = serde_json::json!({
    "name": name,
    "parents": [folder_id],
    "mimeType": "application/vnd.google-apps.document",
});

let docx_bytes = tokio::fs::read(&docx_path).await?;
let form = reqwest::multipart::Form::new()
    .part("metadata",
        reqwest::multipart::Part::text(metadata.to_string())
            .mime_str("application/json")?)
    .part("file",
        reqwest::multipart::Part::bytes(docx_bytes)
            .mime_str("application/vnd.openxmlformats-officedocument.wordprocessingml.document")?);

#[derive(Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    #[serde(rename = "webViewLink")]
    web_view_link: String,
}

let client = reqwest::Client::new();
let resp: DriveFile = client
    .post("https://www.googleapis.com/upload/drive/v3/files")
    .query(&[("uploadType", "multipart"), ("fields", "id,name,webViewLink")])
    .bearer_auth(token.token().unwrap())
    .multipart(form)
    .send().await?
    .error_for_status()?
    .json().await?;

println!("Uploaded: {}\nLink:     {}", resp.name, resp.web_view_link);
```

For huge files with embedded images, swap to `reqwest::Body::wrap_stream`.
Not worth it for typical docs.

## PDF command

Same flow as the Go version: md → temp docx via pandoc → LibreOffice:

```rust
let status = Command::new("libreoffice")
    .args(["--headless", "--convert-to", "pdf", "--outdir"])
    .arg(out_dir)
    .arg(&temp_docx)
    .status()?;
```

LibreOffice names output by stripping `.docx` and adding `.pdf` in
`--outdir`. Rename to the user's requested output path if different.

## Phased build order

Each phase should be a working checkpoint before moving on.

### Phase 1 — Scaffolding

- `cargo init mdgdoc`, populate `Cargo.toml`
- `config.rs` with `Config` struct, `load_config()`, tilde expansion
- `init` command that writes config template to `~/.config/mdgdoc/` and
  creates `templates/` subdirectory
- `main.rs` with clap scaffolding, all four subcommands stubbed to
  `todo!()` except `init`

**Verify:** `cargo run -- init` produces the config file and directory
structure. `cargo run -- convert test.md` panics with `todo!()`.

### Phase 2 — Convert command

- `pandoc.rs` with `run_pandoc()`
- Wire up `convert` command: resolve template from config, default output
  path to input with `.docx` extension, call pandoc
- Handle "pandoc not found" with a useful error message

**Verify:** with a real template .docx in place, `mdgdoc convert
sample.md` produces a styled .docx that opens correctly in
LibreOffice/Word/Google Docs.

### Phase 3 — PDF command

- Reuse `run_pandoc()` for the intermediate docx
- Shell out to LibreOffice, handle the output-filename rename dance
- Use `tempfile` crate or manual `std::env::temp_dir()` for intermediate —
  either works

**Verify:** `mdgdoc pdf sample.md` produces a PDF that matches the
styling of the Google Doc version.

### Phase 4 — Upload command

- `drive.rs` with `yup-oauth2` auth flow and multipart upload
- Make `main.rs` async (add `#[tokio::main]`) for this command only, or
  async for all commands — simpler to just make the whole thing async
- Error message for "no folder ID" pointing at `--folder` or config
- Optional `--keep-docx` flag to preserve the intermediate file

**Verify:** `mdgdoc upload sample.md` walks through first-run OAuth,
caches token, creates a Google Doc in the target folder, prints the
shareable link.

## Gotchas to know up front

1. **First build is slow.** `reqwest` + `tokio` + `yup-oauth2` together
   pull a lot. Incremental builds after that are fast. `cargo build
   --release` once happy.
2. **Token scope:** `yup-oauth2` wants the full URL
   (`https://www.googleapis.com/auth/drive`) not short names. Swap to
   `.../auth/drive.file` for narrower "only files this app created" scope.
3. **Interactive OAuth in TUI context:** `InstalledFlowReturnMethod::Interactive`
   prompts on stdin. If this ever gets embedded in a Textual-style TUI,
   switch to `HTTPRedirect` which spins up a local listener on a random
   port.
4. **LibreOffice concurrency:** running two headless LibreOffice instances
   at once without separate `-env:UserInstallation` dirs causes one to
   fail silently. Not a concern for single-user CLI use, but worth
   knowing if this ever gets parallelized.
5. **Drive API "insufficient permissions" on folder upload:** means the
   OAuth consent granted `drive.file` scope but config requests `drive`,
   or vice versa. Delete the cached token and re-auth.

## Stretch features (not in v0.1)

- `watch` mode that re-converts on file change (use `notify` crate)
- Template inheritance — e.g., `memo` extends `default` by overriding
  specific styles (would require docx style merging, non-trivial)
- Auto-open the resulting Google Doc in the browser after upload
  (`opener` crate, one line)
- Detect embedded image references in markdown and upload those alongside
- Update-in-place: if a doc with the same name already exists in the
  target folder, update it rather than creating a duplicate (uses Drive's
  `files.update` endpoint with `uploadType=multipart`)

## License

GPL — same as your other tooling.
