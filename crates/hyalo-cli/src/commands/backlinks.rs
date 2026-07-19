#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;

use crate::output::{CommandOutcome, Format};
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
///
/// When `case_insensitive` is true, links that differ from the resolved path
/// only in ASCII case are also returned (via `LinkGraph::backlinks_ci`), so a
/// linking file that wrote `[[foo]]` still counts as a backlink of `Foo.md`
/// even though the wikilink casing doesn't match the target's on-disk name.
///
/// When `case_insensitive` is true this ALSO makes the `file_arg` resolution
/// case-insensitive: `resolve_file_user_ci` falls back to a case-insensitive
/// directory scan when the literal-casing lookup misses, so
/// `backlinks --file foo.md` resolves against an on-disk `Foo.md` even on a
/// case-sensitive filesystem (Linux). Previously only the *link-target lookup*
/// (`LinkGraph::backlinks_ci`) honored the setting; Task 4 (iter-185) closed
/// the CLI-argument gap.
pub fn backlinks(
    index: &dyn VaultIndex,
    file_arg: &str,
    dir: &Path,
    format: Format,
    limit: Option<usize>,
    case_insensitive: bool,
) -> Result<CommandOutcome> {
    // Resolve the file argument to a relative path. When case-insensitive mode
    // is on, `resolve_file_user_ci` falls back to a case-insensitive directory
    // scan so `backlinks --file foo.md` resolves against an on-disk `Foo.md`
    // even on a case-sensitive filesystem (Task 4 / iter-184 CI fix).
    let (_full_path, rel) =
        match crate::commands::resolve_file_user_ci(dir, file_arg, case_insensitive) {
            Ok(r) => r,
            Err(e) => {
                return Ok(crate::commands::resolve_error_to_outcome(e, format));
            }
        };

    let graph = index.link_graph();

    let raw = if case_insensitive {
        graph.backlinks_ci(&rel)
    } else {
        graph.backlinks(&rel)
    };
    let entries: Vec<_> = raw.into_iter().filter(|e| !is_self_link(e, &rel)).collect();

    let total = entries.len() as u64;
    let take_n = limit.filter(|n| *n > 0).unwrap_or(usize::MAX);
    let items: Vec<BacklinkItem> = entries
        .iter()
        .take(take_n)
        .map(|e| BacklinkItem {
            source: e.source.to_string_lossy().replace('\\', "/"),
            line: e.line,
            target: e.link.target.clone(),
            label: e.link.label.clone(),
        })
        .collect();
    let result = serde_json::json!({ "file": rel, "backlinks": items });
    Ok(CommandOutcome::success_with_total(
        serde_json::to_string_pretty(&result).context("failed to serialize")?,
        total,
    ))
}
