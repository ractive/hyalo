#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::output::{CommandOutcome, Format, format_success};

/// Delete a snapshot index file.
///
/// If `path` is `None`, defaults to `<dir>/.hyalo-index`.
/// Returns a user error if the file does not exist.
pub fn drop_index(
    dir: &Path,
    path: Option<&Path>,
    format: Format,
    allow_outside_vault: bool,
) -> Result<CommandOutcome> {
    let index_path: PathBuf = match path {
        Some(p) => p.to_path_buf(),
        None => dir.join(".hyalo-index"),
    };

    // Vault boundary check: only applies when the caller specified a custom path.
    // Fail closed — if canonicalization fails we refuse the operation rather than
    // allowing a potentially out-of-vault deletion.
    if path.is_some() && !allow_outside_vault {
        let canonical_dir = hyalo_core::discovery::canonicalize_vault_dir(dir)?;
        match dunce::canonicalize(&index_path) {
            Ok(canonical_path) => {
                if !canonical_path.starts_with(&canonical_dir) {
                    let out = crate::output::format_error(
                        format,
                        &hyalo_core::fs_util::outside_vault_message(
                            "index path",
                            Some(&canonical_path),
                        ),
                        Some(&index_path.display().to_string()),
                        Some("use --allow-outside-vault to override"),
                        None,
                    );
                    return Ok(CommandOutcome::UserError(out));
                }
            }
            // L-7: "the file isn't there" is the overwhelmingly common reason
            // canonicalization fails, and reporting it as a boundary-check
            // failure sent users chasing an irrelevant `--allow-outside-vault`.
            // Resolve the *parent* instead: if that sits inside the vault the
            // path is in-bounds and the honest answer is "no such index file".
            // Anything else keeps the fail-closed refusal.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let parent = index_path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
                match dunce::canonicalize(&parent) {
                    Ok(canonical_parent) if canonical_parent.starts_with(&canonical_dir) => {
                        return Ok(CommandOutcome::UserError(index_not_found_error(
                            format,
                            &index_path,
                        )));
                    }
                    Ok(canonical_parent) => {
                        let out = crate::output::format_error(
                            format,
                            &hyalo_core::fs_util::outside_vault_message(
                                "index path",
                                Some(
                                    &canonical_parent
                                        .join(index_path.file_name().unwrap_or_default()),
                                ),
                            ),
                            Some(&index_path.display().to_string()),
                            Some("use --allow-outside-vault to override"),
                            None,
                        );
                        return Ok(CommandOutcome::UserError(out));
                    }
                    Err(parent_err) => {
                        let details = format!(
                            "failed to resolve index path for boundary check: {parent_err}"
                        );
                        let out = crate::output::format_error(
                            format,
                            "could not verify that index path is inside the vault",
                            Some(&index_path.display().to_string()),
                            Some(
                                "ensure the path is accessible and inside the vault, or use --allow-outside-vault",
                            ),
                            Some(&details),
                        );
                        return Ok(CommandOutcome::UserError(out));
                    }
                }
            }
            Err(e) => {
                let details = format!("failed to resolve index path for boundary check: {e}");
                let out = crate::output::format_error(
                    format,
                    "could not verify that index path is inside the vault",
                    Some(&index_path.display().to_string()),
                    Some(
                        "ensure the path is accessible and inside the vault, or use --allow-outside-vault",
                    ),
                    Some(&details),
                );
                return Ok(CommandOutcome::UserError(out));
            }
        }
    }

    match std::fs::remove_file(&index_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CommandOutcome::UserError(index_not_found_error(
                format,
                &index_path,
            )));
        }
        Err(e) => {
            return Err(e)
                .with_context(|| format!("failed to delete index file: {}", index_path.display()));
        }
    }

    let result = serde_json::json!({
        "deleted": index_path.display().to_string(),
    });

    Ok(CommandOutcome::success(format_success(format, &result)))
}

/// The "there is no index at this path" user error, shared by the boundary
/// pre-check and the delete itself so both report the same thing (L-7).
fn index_not_found_error(format: Format, index_path: &Path) -> String {
    crate::output::format_error(
        format,
        "index file not found",
        Some(&index_path.display().to_string()),
        Some("create one with `hyalo create-index`"),
        None,
    )
}
