#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;

use crate::output::{CommandOutcome, Format};
use hyalo_core::discovery;
use hyalo_core::index::VaultIndex;
use hyalo_core::link_graph::is_self_link;

#[derive(Serialize)]
struct BacklinkItem {
    source: String,
    line: usize,
    target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

/// Run `hyalo backlinks --file <path>` using pre-scanned index data.
///
/// `dir` is still needed to resolve the `file_arg` to a vault-relative path via
/// `discovery::resolve_file`. Link lookup is done against `index.link_graph()`.
/// `limit` caps how many backlink entries are returned (`None` = no cap).
pub fn backlinks(
    index: &dyn VaultIndex,
    file_arg: &str,
    dir: &Path,
    format: Format,
    limit: Option<usize>,
) -> Result<CommandOutcome> {
    // Resolve the file argument to a relative path (same as in `backlinks`)
    let (_full_path, rel) = match discovery::resolve_file(dir, file_arg) {
        Ok(r) => r,
        Err(e) => {
            return Ok(crate::commands::resolve_error_to_outcome(e, format));
        }
    };

    let graph = index.link_graph();

    let entries: Vec<_> = graph
        .backlinks(&rel)
        .into_iter()
        .filter(|e| !is_self_link(e, &rel))
        .collect();

    let total = entries.len() as u64;
    let mut items: Vec<BacklinkItem> = entries
        .iter()
        .map(|e| BacklinkItem {
            source: e.source.to_string_lossy().replace('\\', "/"),
            line: e.line,
            target: e.link.target.clone(),
            label: e.link.label.clone(),
        })
        .collect();

    if let Some(n) = limit {
        items.truncate(n);
    }
    let result = serde_json::json!({ "file": rel, "backlinks": items });
    Ok(CommandOutcome::success_with_total(
        serde_json::to_string_pretty(&result).context("failed to serialize")?,
        total,
    ))
}
