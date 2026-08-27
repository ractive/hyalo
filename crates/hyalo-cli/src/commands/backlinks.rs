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
    /// The link's own target text, exactly as `LinkGraph::build` left it —
    /// relative path components resolved (so `../target.md` reports
    /// `target.md`, not the raw `../` the author wrote) but casing and `.md`
    /// presence untouched.
    ///
    /// PR #251 review L8: `target` reports the query's own canonical path
    /// uniformly across every entry (see its own comment below) — necessary
    /// for a consistent spelling, but it erases exactly the signal someone
    /// chasing a case mismatch needs: whether THIS occurrence was written
    /// `[[NOTE]]` or `[[note]]`. Kept under a separate key rather than folded
    /// back into `target` so both questions ("what file does every entry
    /// really point at" and "how did each occurrence spell it") stay
    /// answerable without re-adding the inconsistency the NEW-18 fix removed.
    written_target: String,
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
                return Ok(crate::commands::resolve_error_to_outcome(e, format, dir));
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
    // NEW-18 (dogfood pre3): `target` used to report each occurrence's own
    // written text (`e.link.target`) as normalized by `LinkGraph::build` —
    // which resolves relative path components (`../target.md` → `target.md`)
    // but leaves the `.md` suffix exactly as the author happened to type it
    // (present, absent, or a bare stem), so two entries pointing at the same
    // file could report different spellings. Every entry here necessarily
    // points at `rel` (that is what was queried), so report it uniformly —
    // the consistently-normalized form this finding's second option allows,
    // cheaper and lower-risk than threading the raw pre-normalization text
    // through `Link`/`BacklinkEntry` (a snapshot-serialized struct used far
    // beyond this one command) to recover the true authored spelling.
    let items: Vec<BacklinkItem> = entries
        .iter()
        .take(take_n)
        .map(|e| BacklinkItem {
            source: e.source.to_string_lossy().replace('\\', "/"),
            line: e.line,
            target: rel.clone(),
            written_target: e.link.target.clone(),
            label: e.link.label.clone(),
        })
        .collect();
    let result = serde_json::json!({ "file": rel, "backlinks": items });
    Ok(CommandOutcome::success_with_total(
        serde_json::to_string_pretty(&result).context("failed to serialize")?,
        total,
    ))
}

// ---------------------------------------------------------------------------
// Dispatch handler (ARCH-1, iter-225)
// ---------------------------------------------------------------------------

/// The `hyalo backlinks` dispatch arm, extracted verbatim from `dispatch.rs`.
/// `index_flags` was consumed earlier in `run.rs` (snapshot loading).
#[allow(clippy::items_after_statements)] // extracted handler keeps its mid-fn imports (ARCH-1, iter-225)
#[allow(clippy::needless_pass_by_value)] // args moved verbatim from the clap variant
pub(crate) fn run(
    ctx: &mut crate::dispatch::CommandContext<'_>,
    selection: crate::cli::inputs::InputSelection,
    cli_limit: Option<usize>,
) -> Result<CommandOutcome> {
    let dir = ctx.dir;
    let site_prefix = ctx.site_prefix;
    let effective_format = ctx.effective_format;
    let snapshot_index = &mut *ctx.snapshot_index;
    use crate::commands::inputs::{ResolutionPolicy, ResolvedInputsOrOutcome, resolve_inputs};
    use crate::commands::{IndexResolution, resolve_index};
    use crate::dispatch::resolve_limit;
    use hyalo_core::index::ScanOptions;
    use hyalo_core::mode_enabled;

    // iter-238: `--iteration <ID>` support (single-file command).
    let selection = match crate::commands::iteration::selection_with_iteration_resolved(
        selection,
        dir,
        ctx.schema,
        effective_format,
    ) {
        Ok(s) => s,
        Err(outcome) => return Ok(outcome),
    };
    match resolve_inputs(
        &selection,
        dir,
        ctx.configured_dir_str,
        snapshot_index.as_ref(),
        &ResolutionPolicy::Single { allow_glob: false },
        effective_format,
        mode_enabled(ctx.case_insensitive_mode, dir),
    )? {
        ResolvedInputsOrOutcome::Outcome(o) => Ok(o),
        ResolvedInputsOrOutcome::Resolved(r) => {
            ctx.files_from_counters = r.counters;
            let (_full, file) = r
                .files
                .into_iter()
                .next()
                .context("Single resolution returned no files")?;
            match resolve_index(
                snapshot_index.as_ref(),
                dir,
                &[],
                &[],
                effective_format,
                site_prefix,
                true,
                &ScanOptions {
                    scan_body: true,
                    bm25_tokenize: false,
                    default_language: None,
                    frontmatter_link_props: ctx.frontmatter_link_props,
                },
            )? {
                IndexResolution::Resolved(resolved) => backlinks(
                    resolved.as_index(),
                    &file,
                    dir,
                    effective_format,
                    resolve_limit(cli_limit, ctx.config_default_limit, ctx.programmatic_output),
                    mode_enabled(ctx.case_insensitive_mode, dir),
                ),
                IndexResolution::Outcome(outcome) => Ok(outcome),
            }
        }
    }
}
