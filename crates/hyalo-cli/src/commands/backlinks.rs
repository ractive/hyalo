#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use crate::output::{CommandOutcome, Format};
use hyalo_core::discovery;
use hyalo_core::index::VaultIndex;
use hyalo_core::link_graph::{LinkGraph, is_self_link};

#[derive(Serialize)]
struct BacklinkResult {
    file: String,
    backlinks: Vec<BacklinkItem>,
    total: usize,
}

#[derive(Serialize)]
struct BacklinkItem {
    source: String,
    line: usize,
    target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

/// Run `hyalo backlinks --file <path>`.
pub fn backlinks(
    dir: &Path,
    site_prefix: Option<&str>,
    file_arg: &str,
    format: Format,
) -> Result<CommandOutcome> {
    // Resolve the file argument to a relative path
    let (_full_path, rel) = match discovery::resolve_file(dir, file_arg) {
        Ok(r) => r,
        Err(e) => {
            return Ok(crate::commands::resolve_error_to_outcome(e, format));
        }
    };

    // Build the in-memory link graph
    let build = LinkGraph::build(dir, site_prefix)?;
    for (path, msg) in &build.warnings {
        crate::warn::warn(format!("skipping {}: {msg}", path.display()));
    }

    // Look up backlinks, then exclude self-links at the display boundary.
    // Self-links are kept in the core graph so that `mv` can rewrite them.
    let entries: Vec<_> = build
        .graph
        .backlinks(&rel)
        .into_iter()
        .filter(|e| !is_self_link(e, &rel))
        .collect();

    let items: Vec<BacklinkItem> = entries
        .iter()
        .map(|e| BacklinkItem {
            source: e.source.to_string_lossy().replace('\\', "/"),
            line: e.line,
            target: e.link.target.clone(),
            label: e.link.label.clone(),
        })
        .collect();

    let total = items.len();
    let result = BacklinkResult {
        file: rel,
        backlinks: items,
        total,
    };

    let output = match format {
        Format::Json => serde_json::to_string_pretty(&result)?,
        Format::Text => format_text(&result),
    };

    Ok(CommandOutcome::Success(output))
}

/// Run `hyalo backlinks --file <path>` using pre-scanned index data.
///
/// `dir` is still needed to resolve the `file_arg` to a vault-relative path via
/// `discovery::resolve_file`. Link lookup is done against `index.link_graph()`.
pub fn backlinks_from_index(
    index: &dyn VaultIndex,
    file_arg: &str,
    dir: &Path,
    format: Format,
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

    let items: Vec<BacklinkItem> = entries
        .iter()
        .map(|e| BacklinkItem {
            source: e.source.to_string_lossy().replace('\\', "/"),
            line: e.line,
            target: e.link.target.clone(),
            label: e.link.label.clone(),
        })
        .collect();

    let total = items.len();
    let result = BacklinkResult {
        file: rel,
        backlinks: items,
        total,
    };

    let output = match format {
        Format::Json => serde_json::to_string_pretty(&result)?,
        Format::Text => format_text(&result),
    };

    Ok(CommandOutcome::Success(output))
}

fn format_text(result: &BacklinkResult) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    if result.backlinks.is_empty() {
        write!(out, "No backlinks found for {}", result.file).unwrap();
        return out;
    }

    writeln!(
        out,
        "{} backlink{} to {}:",
        result.total,
        if result.total == 1 { "" } else { "s" },
        result.file
    )
    .unwrap();

    for item in &result.backlinks {
        write!(out, "  {}:{}", item.source, item.line).unwrap();
        if let Some(label) = &item.label {
            write!(out, " (\"{}\")", label).unwrap();
        }
        writeln!(out).unwrap();
    }

    out.trim_end().to_owned()
}
