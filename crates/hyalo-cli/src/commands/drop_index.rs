#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::output::{CommandOutcome, Format, format_success};

/// Delete a snapshot index file.
///
/// If `path` is `None`, defaults to `<dir>/.hyalo-index`.
/// Returns a user error if the file does not exist.
pub fn drop_index(dir: &Path, path: Option<&Path>, format: Format) -> Result<CommandOutcome> {
    let index_path: PathBuf = match path {
        Some(p) => p.to_path_buf(),
        None => dir.join(".hyalo-index"),
    };

    match std::fs::remove_file(&index_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let out = crate::output::format_error(
                format,
                "index file not found",
                Some(&index_path.display().to_string()),
                Some("create one with `hyalo create-index`"),
                None,
            );
            return Ok(CommandOutcome::UserError(out));
        }
        Err(e) => {
            return Err(e)
                .with_context(|| format!("failed to delete index file: {}", index_path.display()));
        }
    }

    let result = serde_json::json!({
        "deleted": index_path.display().to_string(),
    });

    Ok(CommandOutcome::Success(format_success(format, &result)))
}
