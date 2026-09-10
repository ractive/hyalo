#![allow(clippy::missing_errors_doc)]
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::cli::args::ConflictPolicy;
use crate::output::{CommandOutcome, Format};
use hyalo_core::discovery::{canonicalize_vault_dir, discover_files, match_globs};
use hyalo_core::filter::{PropertyFilter, matches_frontmatter_filters};
use hyalo_core::link_rewrite::{
    self, Replacement, RewritePlan, SkippedAmbiguous, SkippedFrontmatterLink,
};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MvResult {
    /// Original path or value.
    from: String,
    /// Destination path or value.
    to: String,
    /// Whether this result describes a preview without writing changes.
    dry_run: bool,
    /// Files with link replacements caused by the move.
    updated_files: Vec<UpdatedFile>,
    /// Number of files with rewritten links.
    total_files_updated: usize,
    /// Number of rewritten links.
    total_links_updated: usize,
    /// Links that were skipped because the stem was ambiguous (NEW-3).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    skipped_ambiguous: Vec<SkippedAmbiguous>,
    /// Frontmatter wikilinks left untouched because their `[[…]]` spans a line
    /// break — a folded/literal block scalar or a wrapped string (iter-262,
    /// FM-2). Omitted when empty, so the ordinary result shape is unchanged.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    frontmatter_links_skipped: Vec<SkippedFrontmatterLink>,
    /// The source, when `--on-conflict skip` left it where it was (iter-273,
    /// MV-3). Same key batch mode uses, so one `.results.skipped` read answers
    /// "did anything get skipped?" in either mode.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    skipped: Vec<String>,
}

#[derive(Serialize, Clone)]
struct UpdatedFile {
    /// Vault-relative Markdown file path.
    file: String,
    /// Link replacements within this file.
    replacements: Vec<Replacement>,
}

#[derive(Serialize)]
struct MoveEntry {
    /// Original path or value.
    from: String,
    /// Destination path or value.
    to: String,
    /// Frontmatter wikilinks naming *this* move's source that could not be
    /// rewritten because their `[[…]]` spans a line break (iter-273, MV-2).
    /// Same key and shape single-file `mv` reports; omitted when empty, so the
    /// ordinary batch result is byte-identical to before.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    frontmatter_links_skipped: Vec<SkippedFrontmatterLink>,
}

#[derive(Serialize)]
struct BatchTotals {
    /// Number of proposed or performed file moves.
    moves: usize,
    /// Number of files whose content changed.
    files_changed: usize,
    /// Number of rewritten link occurrences.
    replacements: usize,
}

/// One destination two sources want, or one already taken by a file the batch
/// is not moving (iter-275, MV-5 / BUG-25).
///
/// A dry run exists to *show* what a move would do, so meeting a collision by
/// refusing to answer at all told the caller least at the moment they asked
/// most. Each collision is now a row, the non-colliding moves are still
/// planned and reported, and `--apply` keeps refusing (a partial batch is not
/// what `mv --glob` promises).
#[derive(Serialize)]
struct Collision {
    /// Source path whose proposed destination conflicts.
    source: String,
    /// Conflicting destination path.
    destination: String,
}

#[derive(Serialize)]
struct BatchMvResult {
    /// Proposed or performed file moves.
    moves: Vec<MoveEntry>,
    /// Files with link replacements caused by the move.
    updated_files: Vec<UpdatedFile>,
    /// Aggregated move and rewrite counts.
    totals: BatchTotals,
    /// `totals.files_changed` and `totals.replacements` under the names
    /// single-file `mv` uses (iter-275, MV-5). Batch JSON had neither, so a
    /// caller reading `total_links_updated` got `null` from one mode and a
    /// number from the other.
    total_files_updated: usize,
    /// Number of rewritten links.
    total_links_updated: usize,
    /// Destination collisions, listed rather than fatal in a dry run.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    collisions: Vec<Collision>,
    /// Whether apply mode was requested.
    applied: bool,
    /// `!applied`, restated under the name the rest of the mutation family
    /// uses (iter-256 COH-9). `applied` predates the convention and is kept
    /// for back-compat; the two are always exact inverses.
    dry_run: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// Paths whose conflicting state prevented a change.
    conflicts: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// Vault-relative files left unchanged.
    skipped: Vec<String>,
}

// ---------------------------------------------------------------------------
// Single-file command entry point
// ---------------------------------------------------------------------------

/// Run `hyalo mv --file <old> --to <new> [--dry-run] [--allow-ambiguous]`.
#[allow(clippy::too_many_arguments)]
pub fn mv(
    dir: &Path,
    file_arg: &str,
    to_arg: &str,
    dry_run: bool,
    format: Format,
    site_prefix: Option<&str>,
    journal: &mut crate::commands::journal::MutationJournal<'_>,
    allow_ambiguous: bool,
    on_conflict: ConflictPolicy,
) -> Result<CommandOutcome> {
    // 1. Validate source exists
    let (_src_full, old_rel) = match super::resolve_file_user(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(crate::commands::resolve_error_to_outcome(e, format, dir)),
    };

    // 2. Validate target path
    let new_rel = match validate_target_single(dir, to_arg, &old_rel, format, on_conflict) {
        Ok(TargetSingle::Path(rel)) => rel,
        // BUG-26 (iter-273, MV-3): `--on-conflict skip` was accepted in
        // single-file mode and then ignored, so the run failed with "target
        // file already exists" — the very outcome the flag exists to avoid. A
        // skipped single move is a success that moved nothing, reported in the
        // same envelope shape as batch mode's `skipped`.
        Ok(TargetSingle::Skipped(dest)) => {
            crate::warn::warn(format!(
                "{old_rel} not moved: {dest} already exists (--on-conflict skip)"
            ));
            let result = MvResult {
                from: old_rel.clone(),
                to: dest,
                dry_run,
                updated_files: Vec::new(),
                total_files_updated: 0,
                total_links_updated: 0,
                skipped_ambiguous: Vec::new(),
                frontmatter_links_skipped: Vec::new(),
                skipped: vec![old_rel],
            };
            return Ok(CommandOutcome::success(
                serde_json::to_value(&result).context("failed to serialize")?,
            ));
        }
        Err(outcome) => return Ok(*outcome),
    };

    // 3. Capture exact source bytes and identity before planning. The prepared
    // move re-verifies this receipt immediately before publication.
    let sources = capture_move_sources(dir, std::slice::from_ref(&old_rel))?;

    // 4. Plan all rewrites
    let mv_plan = link_rewrite::plan_mv(dir, &old_rel, &new_rel, site_prefix, allow_ambiguous)?;

    // 5. Build result
    let updated_files: Vec<UpdatedFile> = mv_plan
        .plans
        .iter()
        .map(|p| UpdatedFile {
            file: p.rel_path.clone(),
            replacements: p.replacements.clone(),
        })
        .collect();
    let total_links: usize = updated_files.iter().map(|f| f.replacements.len()).sum();

    // iter-262 (FM-2): a frontmatter wikilink whose brackets straddle a line
    // break has no single-line span to rewrite. Say so on stderr in both
    // formats — a silent skip is exactly the dangling-link failure this
    // iteration exists to remove — and list the offenders in JSON.
    if !mv_plan.skipped_frontmatter.is_empty() {
        eprintln!(
            "warning: {} frontmatter wikilink{} not rewritten (see --format json for the files)",
            mv_plan.skipped_frontmatter.len(),
            if mv_plan.skipped_frontmatter.len() == 1 {
                ""
            } else {
                "s"
            }
        );
    }

    // NEW-3: emit stderr notes for text format (JSON envelope has skipped_ambiguous array).
    if format == Format::Text {
        for skipped in &mv_plan.skipped_ambiguous {
            // UX-2 (iter-275, MV-8): the JSON entry has carried `property`
            // since iteration 271 and `self` since this one; text mode showed
            // neither, so a reader had to guess whether a reported line was a
            // frontmatter value or body prose.
            let mut where_ = String::new();
            if let Some(property) = &skipped.property {
                let _ = write!(where_, " (property: {property})");
            }
            if skipped.is_self {
                where_.push_str(" (in the moved file itself)");
            }
            eprintln!(
                "note: skipped ambiguous link [[{}]] at {}:{}{where_}\n      candidates: {}\n      \
                 (use --allow-ambiguous to rewrite based on stem match anyway)",
                skipped.target,
                skipped.source,
                skipped.line,
                skipped.candidates.join(", ")
            );
        }
    }

    let result = MvResult {
        from: old_rel.clone(),
        to: new_rel.clone(),
        dry_run,
        updated_files,
        total_files_updated: mv_plan.plans.len(),
        total_links_updated: total_links,
        skipped_ambiguous: mv_plan.skipped_ambiguous,
        frontmatter_links_skipped: mv_plan.skipped_frontmatter,
        skipped: Vec::new(),
    };

    // 6. If not dry-run, execute the move and rewrites, then update the index.
    if !dry_run {
        if let Err(err) = link_rewrite::validate_rewrite_plans(&mv_plan.plans) {
            if let Some(outcome) = super::frontmatter_write_error_outcome(&err, format, &old_rel) {
                return Ok(outcome);
            }
            return Err(err);
        }
        let execution = execute_batch_mv(
            dir,
            &[(old_rel.clone(), new_rel.clone())],
            sources,
            &mv_plan.plans,
            journal,
        )?;
        let outcome =
            CommandOutcome::success(serde_json::to_value(&result).context("failed to serialize")?);
        return Ok(match execution.error {
            Some(error) => mutation_failure(error, execution.report),
            None => outcome.with_apply_report(execution.report),
        });
    }

    Ok(CommandOutcome::success(
        serde_json::to_value(&result).context("failed to serialize")?,
    ))
}

// ---------------------------------------------------------------------------
// Batch command entry point
// ---------------------------------------------------------------------------

/// Run `hyalo mv` in batch mode (--glob/--property/--tag/--type selectors).
#[allow(clippy::too_many_arguments)]
pub fn mv_batch(
    dir: &Path,
    file_positional: Option<&str>,
    file_flag: Option<&str>,
    globs: &[String],
    property_filters: &[PropertyFilter],
    tag_filters: &[String],
    to_arg: &str,
    apply: bool,
    on_conflict: ConflictPolicy,
    format: Format,
    site_prefix: Option<&str>,
    journal: &mut crate::commands::journal::MutationJournal<'_>,
    allow_ambiguous: bool,
) -> Result<CommandOutcome> {
    // 1. Validate --to is a directory-shaped path.
    let to_dir = match validate_batch_target(to_arg, format) {
        Ok(d) => d,
        Err(outcome) => return Ok(*outcome),
    };

    // 2. Resolve source files.
    let sources = match resolve_batch_sources(
        dir,
        file_positional,
        file_flag,
        globs,
        property_filters,
        tag_filters,
        format,
    )? {
        Ok(v) => v,
        Err(outcome) => return Ok(outcome),
    };

    // 3. Check empty selection.
    if sources.is_empty() {
        let filter_desc = describe_filters(globs, property_filters, tag_filters);
        let out = crate::output::user_diagnostic(
            format,
            "no files matched the given filters",
            Some(&filter_desc),
            Some("check your --glob, --property, --tag, or --type filters"),
            None,
        );
        return Ok(CommandOutcome::UserError(out));
    }

    // 4. Build rename map (old_rel → new_rel), detect collisions.
    let (renames, conflicts, skipped, collisions) =
        match build_rename_map(dir, &sources, &to_dir, on_conflict, format, apply) {
            Ok(t) => t,
            Err(outcome) => return Ok(*outcome),
        };
    if !collisions.is_empty() {
        crate::warn::warn(format!(
            "{} destination collision(s) listed under `collisions`; those moves are not planned",
            collisions.len()
        ));
    }

    // Capture every selected physical source before planning. Aliases and hard
    // links cannot authorize two publications in one batch.
    let source_names: Vec<String> = renames.iter().map(|(source, _)| source.clone()).collect();
    let captured_sources = capture_move_sources(dir, &source_names)?;

    // 5. Plan rewrites (build link graph once).
    let mut plan_result = if renames.is_empty() {
        link_rewrite::BatchMvPlanResult::default()
    } else {
        link_rewrite::plan_batch_mv(dir, &renames, site_prefix, allow_ambiguous)?
    };
    let plans = std::mem::take(&mut plan_result.plans);

    // MV-2 (iter-273): batch mode now runs the same split-frontmatter-link
    // sweep single-file `mv` has run since iteration 269, so a link the move
    // cannot rewrite is announced instead of left dangling in silence.
    let total_split_links: usize = plan_result.skipped_frontmatter.values().map(Vec::len).sum();
    if total_split_links > 0 {
        eprintln!(
            "warning: {total_split_links} frontmatter wikilink{} not rewritten \
             (see --format json for the files)",
            if total_split_links == 1 { "" } else { "s" }
        );
    }

    // 6. Build result.
    let moves: Vec<MoveEntry> = renames
        .iter()
        .map(|(f, t)| MoveEntry {
            from: f.clone(),
            to: t.clone(),
            frontmatter_links_skipped: plan_result
                .skipped_frontmatter
                .get(f)
                .cloned()
                .unwrap_or_default(),
        })
        .collect();
    let updated_files: Vec<UpdatedFile> = plans
        .iter()
        .map(|p| UpdatedFile {
            file: p.rel_path.clone(),
            replacements: p.replacements.clone(),
        })
        .collect();
    let total_replacements: usize = updated_files.iter().map(|f| f.replacements.len()).sum();

    let result = BatchMvResult {
        totals: BatchTotals {
            moves: moves.len(),
            files_changed: updated_files.len(),
            replacements: total_replacements,
        },
        total_files_updated: updated_files.len(),
        total_links_updated: total_replacements,
        collisions,
        moves,
        updated_files,
        applied: apply,
        dry_run: !apply,
        conflicts,
        skipped,
    };

    // 7. Apply if requested.
    if apply && !renames.is_empty() {
        if let Err(err) = link_rewrite::validate_rewrite_plans(&plans) {
            if let Some(outcome) = super::frontmatter_write_error_outcome(&err, format, to_arg) {
                return Ok(outcome);
            }
            return Err(err);
        }
        let execution = execute_batch_mv(dir, &renames, captured_sources, &plans, journal)?;
        let outcome =
            CommandOutcome::success(serde_json::to_value(&result).context("failed to serialize")?);
        return Ok(match execution.error {
            Some(error) => mutation_failure(error, execution.report),
            None => outcome.with_apply_report(execution.report),
        });
    }

    Ok(CommandOutcome::success(
        serde_json::to_value(&result).context("failed to serialize")?,
    ))
}

// ---------------------------------------------------------------------------
// Source resolution
// ---------------------------------------------------------------------------

/// Resolve the batch source set given selectors.
///
/// Returns `Ok(Ok(Vec<String>))` on success, `Ok(Err(CommandOutcome))` on user error.
fn resolve_batch_sources(
    dir: &Path,
    file_positional: Option<&str>,
    file_flag: Option<&str>,
    globs: &[String],
    property_filters: &[PropertyFilter],
    tag_filters: &[String],
    format: Format,
) -> Result<std::result::Result<Vec<String>, CommandOutcome>> {
    // Walk vault to get all .md files.
    let all_files = discover_files(dir).context("discovering vault files")?;

    // Apply globs if any.
    let glob_filtered: Vec<PathBuf> = if globs.is_empty() {
        all_files
    } else {
        let matched = match match_globs(dir, &all_files, globs) {
            Ok(m) => m,
            Err(e) => {
                let out = crate::output::user_diagnostic(
                    format,
                    &format!("invalid glob pattern: {e}"),
                    None,
                    None,
                    None,
                );
                return Ok(Err(CommandOutcome::UserError(out)));
            }
        };
        matched.into_iter().map(|(p, _)| p).collect()
    };

    // Apply property/tag filters by reading frontmatter.
    let has_filters = !property_filters.is_empty() || !tag_filters.is_empty();
    let mut sources: Vec<String> = Vec::new();

    for abs_path in glob_filtered {
        let rel = match abs_path.strip_prefix(dir) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        if has_filters {
            let Ok(props) = hyalo_core::frontmatter::read_frontmatter(&abs_path) else {
                continue;
            };
            if !matches_frontmatter_filters(&props, property_filters, tag_filters) {
                continue;
            }
        }

        sources.push(rel);
    }

    // Apply positional/--file explicit inclusion if provided.
    if let Some(f) = file_positional.or(file_flag) {
        let rel = hyalo_core::discovery::strip_dir_prefix(dir, f).unwrap_or_else(|| f.to_string());
        if !sources.contains(&rel) {
            sources.push(rel);
        }
    }

    sources.sort();
    Ok(Ok(sources))
}

// ---------------------------------------------------------------------------
// Rename map construction
// ---------------------------------------------------------------------------

/// Build (renames, conflicts, skipped, collisions) from source paths and
/// destination dir.
///
/// Returns `Ok((renames, conflicts, skipped, collisions))` or `Err(outcome)`
/// on user error.
#[allow(clippy::type_complexity)]
fn build_rename_map(
    dir: &Path,
    sources: &[String],
    to_dir: &str,
    on_conflict: ConflictPolicy,
    format: Format,
    apply: bool,
) -> std::result::Result<
    (
        Vec<(String, String)>,
        Vec<String>,
        Vec<String>,
        Vec<Collision>,
    ),
    Box<CommandOutcome>,
> {
    // Build proposed rename map: source → dest (old_rel → new_rel).
    let mut proposed: Vec<(String, String)> = Vec::new();
    for src in sources {
        let basename = Path::new(src)
            .file_name()
            .map_or_else(|| src.clone(), |n| n.to_string_lossy().into_owned());
        // MV-4 (iter-275): an empty `to_dir` is the vault root, not a leading
        // slash — `--to .` from inside the vault must land at `a.md`, never
        // `/a.md`.
        let new_rel = if to_dir.is_empty() {
            basename
        } else {
            format!("{to_dir}/{basename}")
        };
        proposed.push((src.clone(), new_rel));
    }

    // Drop no-op renames (source already at destination).
    proposed.retain(|(old_rel, new_rel)| old_rel != new_rel);

    // H-3: reject destinations that would escape the vault through a
    // symlinked directory component, even when the destination's parent
    // directories don't exist yet. Checked before any fs mutation so the
    // whole batch fails atomically rather than escaping partway through.
    if !proposed.is_empty() {
        let canonical_vault = canonicalize_vault_dir(dir).map_err(|e| {
            let out = crate::output::user_diagnostic(
                format,
                "failed to canonicalize vault directory",
                Some(&e.to_string()),
                None,
                None,
            );
            CommandOutcome::UserError(out)
        })?;
        for (_, new_rel) in &proposed {
            if let Err((msg, hint)) = ensure_dest_within_vault(&canonical_vault, dir, new_rel) {
                let hint_opt = (!hint.is_empty()).then_some(hint.as_str());
                let out =
                    crate::output::user_diagnostic(format, &msg, Some(new_rel), hint_opt, None);
                return Err(Box::new(CommandOutcome::UserError(out)));
            }
        }
    }

    // Detect basename collisions (two sources mapping to the same dest).
    let mut dest_to_sources: HashMap<String, Vec<String>> = HashMap::new();
    for (src, dst) in &proposed {
        dest_to_sources
            .entry(dst.clone())
            .or_default()
            .push(src.clone());
    }

    let mut collision_dests: HashSet<String> = HashSet::new();
    for (dst, srcs) in &dest_to_sources {
        if srcs.len() > 1 {
            collision_dests.insert(dst.clone());
        }
    }

    // Check pre-existing files in target.
    // A pre-existing collision occurs when the destination path already exists
    // on disk AND it is not the same file as the source (i.e. we'd clobber a
    // file that isn't part of this batch).
    let mut pre_existing: HashSet<String> = HashSet::new();
    for (src, dst) in &proposed {
        let dst_path = dir.join(dst);
        let src_path = dir.join(src);
        // L-4: a dangling symlink is an existing directory entry even though
        // `exists()` (which follows the link) says otherwise — treat it as a
        // collision so batch mode reports it instead of clobbering it.
        if dst_path.symlink_metadata().is_ok() && !dst_path.exists() {
            pre_existing.insert(dst.clone());
            continue;
        }
        if dst_path.exists() {
            // Only skip if source and destination are literally the same file
            // (same path on disk — after normalization this shouldn't happen
            // for valid moves, but be safe).
            let same_file = dst_path
                .canonicalize()
                .ok()
                .zip(src_path.canonicalize().ok())
                .is_some_and(|(d, s)| d == s);
            if !same_file {
                pre_existing.insert(dst.clone());
            }
        }
    }

    // All collisions = basename collisions + pre-existing.
    let all_collision_dests: HashSet<String> =
        collision_dests.union(&pre_existing).cloned().collect();

    if !all_collision_dests.is_empty() {
        if on_conflict.is_skip() {
            // For each collision dest, keep the lexicographically first source.
            let mut skipped = Vec::new();
            let mut final_renames = Vec::new();

            for (dst, srcs) in &dest_to_sources {
                if all_collision_dests.contains(dst) {
                    // Sort sources, keep first.
                    let mut sorted_srcs = srcs.clone();
                    sorted_srcs.sort();
                    let keep = &sorted_srcs[0];
                    for s in &sorted_srcs[1..] {
                        skipped.push(s.clone());
                    }
                    // Also skip if dest pre-exists.
                    if pre_existing.contains(dst) {
                        for s in &sorted_srcs {
                            skipped.push(s.clone());
                        }
                    } else {
                        final_renames.push((keep.clone(), dst.clone()));
                    }
                } else {
                    final_renames.push((srcs[0].clone(), dst.clone()));
                }
            }
            skipped.sort();
            skipped.dedup();
            // Remove skipped from final_renames.
            let skipped_set: HashSet<&str> = skipped.iter().map(String::as_str).collect();
            let final_renames: Vec<(String, String)> = final_renames
                .into_iter()
                .filter(|(s, _)| !skipped_set.contains(s.as_str()))
                .collect();
            return Ok((final_renames, vec![], skipped, vec![]));
        }
        // BUG-25 (dogfood v0.22.0), iter-275 MV-5: a **dry run** lists the
        // collisions and still plans everything else, so `--dry-run` answers
        // the question it was asked. `--apply` keeps failing: half a batch is
        // not what `mv --glob … --apply` promises, and the caller has
        // `--on-conflict skip` for that.
        if !apply {
            let mut collisions: Vec<Collision> = all_collision_dests
                .iter()
                .flat_map(|dst| {
                    let srcs = dest_to_sources.get(dst).map_or(&[][..], Vec::as_slice);
                    srcs.iter().map(move |s| Collision {
                        source: s.clone(),
                        destination: dst.clone(),
                    })
                })
                .collect();
            collisions.sort_by(|a, b| {
                a.destination
                    .cmp(&b.destination)
                    .then(a.source.cmp(&b.source))
            });
            let planned: Vec<(String, String)> = proposed
                .into_iter()
                .filter(|(_, dst)| !all_collision_dests.contains(dst))
                .collect();
            return Ok((planned, vec![], vec![], collisions));
        }
        // Default (`--apply`): error.
        //
        // BUG-24 (iter-273, MV-3): "multiple sources map to the same
        // destination" was printed for *both* kinds of collision, so a single
        // file colliding with a note already sitting in the target directory
        // was reported as an internal clash between two sources the caller
        // could not find. The two have different fixes — rename one source vs
        // deal with the file already there — so they get different sentences,
        // and a batch hitting both says so.
        let mut conflict_msgs: Vec<String> = all_collision_dests
            .iter()
            .flat_map(|dst| {
                let srcs = dest_to_sources.get(dst).map_or(&[][..], Vec::as_slice);
                srcs.iter().map(move |s| format!("{s} -> {dst}"))
            })
            .collect();
        conflict_msgs.sort();
        let desc = conflict_msgs.join(", ");
        let has_source_clash = !collision_dests.is_empty();
        let has_pre_existing = !pre_existing.is_empty();
        let message = match (has_source_clash, has_pre_existing) {
            (true, false) => "destination collision: multiple sources map to the same destination",
            (false, true) => "destination collision: a file already exists at the destination",
            _ => {
                "destination collision: some sources share a destination and some destinations \
                  are already taken"
            }
        };
        let out = crate::output::user_diagnostic(
            format,
            message,
            Some(&desc),
            Some("use --on-conflict=skip to skip colliding files"),
            None,
        );
        return Err(Box::new(CommandOutcome::UserError(out)));
    }

    Ok((proposed, vec![], vec![], vec![]))
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// Execute prepared moves and rewrites. Every exit carries the observed
/// effects; compensation is guarded and best effort.
struct MoveExecution {
    report: crate::commands::apply::ApplyReport,
    error: Option<String>,
}

struct AppliedMove {
    old_rel: String,
    new_rel: String,
    receipt: hyalo_core::rooted::OwnedPublication,
}

fn execute_batch_mv(
    dir: &Path,
    renames: &[(String, String)],
    sources: Vec<hyalo_core::rooted::CapturedInput>,
    plans: &[RewritePlan],
    journal: &mut crate::commands::journal::MutationJournal<'_>,
) -> Result<MoveExecution> {
    use hyalo_core::rooted::{Durability, WriteSession};
    execute_batch_mv_with_session(
        dir,
        renames,
        sources,
        plans,
        journal,
        WriteSession::new(if renames.len() > 8 {
            Durability::PerDirectory
        } else {
            Durability::PerFile
        }),
    )
}

fn execute_batch_mv_with_session(
    dir: &Path,
    renames: &[(String, String)],
    sources: Vec<hyalo_core::rooted::CapturedInput>,
    plans: &[RewritePlan],
    journal: &mut crate::commands::journal::MutationJournal<'_>,
    mut session: hyalo_core::rooted::WriteSession,
) -> Result<MoveExecution> {
    use crate::commands::apply::{
        ApplyReport, EffectFailure, EffectState, IndexDisposition, PathEffect,
    };
    use hyalo_core::rooted::{RelativeName, VaultRoot};

    // H-3 defense-in-depth: re-verify every destination stays within the
    // vault right before mutating the filesystem. `build_rename_map` already
    // rejects escaping destinations, but this guard makes the invariant
    // explicit here too and survives future refactors (mirrors the same
    // check in `link_rewrite::execute_plans`).
    let canonical_vault = canonicalize_vault_dir(dir)
        .context("failed to canonicalize vault directory for write safety check")?;
    for (_, new_rel) in renames {
        ensure_dest_within_vault(&canonical_vault, dir, new_rel)
            .map_err(|(msg, _)| anyhow::Error::msg(msg))?;
    }

    // Link rewrites and moves form one user operation. Reject every
    // deterministic transformation before creating directories or notes.
    link_rewrite::validate_rewrite_plans(plans)?;
    let root = VaultRoot::new(dir)?;
    let mut paths = Vec::new();

    // Materialize the complete parent manifest before probing or moving.
    let mut needed = BTreeSet::new();
    for (_, destination) in renames {
        let mut relative = PathBuf::new();
        if let Some(parent) = Path::new(destination).parent() {
            for component in parent.components() {
                relative.push(component.as_os_str());
                let full = dir.join(&relative);
                match fs::symlink_metadata(&full) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        bail!("move destination parent is a symlink: {}", full.display())
                    }
                    Ok(meta) if !meta.is_dir() => {
                        bail!(
                            "move destination parent is not a directory: {}",
                            full.display()
                        )
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        needed.insert(relative.clone());
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    // Validate every lexical directory name before the first filesystem
    // effect. From this point onward all failures flow through the reporting
    // epilogue below.
    let manifest: Vec<(PathBuf, RelativeName)> = needed
        .into_iter()
        .map(|relative| Ok((relative.clone(), RelativeName::new(&relative)?)))
        .collect::<Result<_>>()?;
    let mut created = Vec::new();
    let mut preparation_error = None;
    for (position, (relative, name)) in manifest.iter().enumerate() {
        let effect = match root.create_directory(name, &mut session) {
            Ok(effect) => effect,
            Err(error) => {
                paths.push(PathEffect {
                    file: format!("{}/", relative.to_string_lossy().replace('\\', "/")),
                    state: EffectState::FailedBeforeCommit,
                    category: Some(EffectFailure::Io),
                    error: Some(error.to_string()),
                });
                for (remaining, _) in manifest.iter().skip(position + 1) {
                    paths.push(PathEffect {
                        file: format!("{}/", remaining.to_string_lossy().replace('\\', "/")),
                        state: EffectState::NotAttempted,
                        category: None,
                        error: None,
                    });
                }
                preparation_error = Some(error.to_string());
                break;
            }
        };
        let error = effect.finalization_error().map(str::to_owned);
        paths.push(PathEffect {
            file: format!("{}/", relative.to_string_lossy().replace('\\', "/")),
            state: if error.is_some() {
                EffectState::CommittedWithFinalizationError
            } else {
                EffectState::Committed
            },
            category: error.as_ref().map(|_| EffectFailure::Finalization),
            error: error.clone(),
        });
        match same_file::Handle::from_path(dir.join(relative)) {
            Ok(identity) => created.push(OwnedDirectory {
                identity,
                relative: relative.clone(),
            }),
            Err(identity_error) => {
                paths.push(PathEffect {
                    file: format!("{}/", relative.to_string_lossy().replace('\\', "/")),
                    state: EffectState::Kept,
                    category: Some(EffectFailure::Io),
                    error: Some(format!(
                        "created directory ownership could not be recorded; keeping it: {identity_error}"
                    )),
                });
                preparation_error = Some(format!(
                    "created directory ownership could not be recorded: {identity_error}"
                ));
            }
        }
        if let Some(finalization_error) = error {
            preparation_error = Some(format!(
                "directory creation published only partially: {finalization_error}"
            ));
        }
        if preparation_error.is_some() {
            for (remaining, _) in manifest.iter().skip(position + 1) {
                paths.push(PathEffect {
                    file: format!("{}/", remaining.to_string_lossy().replace('\\', "/")),
                    state: EffectState::NotAttempted,
                    category: None,
                    error: None,
                });
            }
            break;
        }
    }
    if let Some(mut primary) = preparation_error {
        for (_, destination) in renames {
            paths.push(PathEffect {
                file: destination.clone(),
                state: EffectState::NotAttempted,
                category: None,
                error: None,
            });
        }
        cleanup_created_directories(dir, &created, &mut paths);
        if let Err(finish_error) = session.finish() {
            primary = format!("{primary}; finalization failed: {finish_error}");
        }
        return Ok(MoveExecution {
            report: ApplyReport {
                paths,
                index: IndexDisposition::NotUsed,
                index_error: None,
            },
            error: Some(primary),
        });
    }
    if let Err(error) = probe_destination_equivalence(dir, renames) {
        cleanup_created_directories(dir, &created, &mut paths);
        let mut primary = error.to_string();
        if let Err(finish_error) = session.finish() {
            primary = format!("{primary}; finalization failed: {finish_error}");
        }
        return Ok(MoveExecution {
            report: ApplyReport {
                paths,
                index: IndexDisposition::NotUsed,
                index_error: None,
            },
            error: Some(primary),
        });
    }

    let mut applied = Vec::new();
    let mut error = None;
    for (((old_rel, new_rel), source), position) in renames.iter().zip(sources).zip(0usize..) {
        match move_one(
            dir,
            &root,
            source,
            old_rel,
            new_rel,
            &mut session,
            &mut paths,
        ) {
            Ok(outcome) => {
                let finalization_error = outcome.finalization_error.clone();
                paths.push(PathEffect {
                    file: old_rel.clone(),
                    state: if finalization_error.is_some() {
                        EffectState::CommittedWithFinalizationError
                    } else {
                        EffectState::Committed
                    },
                    error: finalization_error.clone(),
                    category: finalization_error
                        .as_ref()
                        .map(|_| EffectFailure::Finalization),
                });
                paths.push(PathEffect {
                    file: new_rel.clone(),
                    state: if finalization_error.is_some() {
                        EffectState::CommittedWithFinalizationError
                    } else {
                        EffectState::Committed
                    },
                    error: finalization_error.clone(),
                    category: finalization_error
                        .as_ref()
                        .map(|_| EffectFailure::Finalization),
                });
                applied.push(AppliedMove {
                    old_rel: old_rel.clone(),
                    new_rel: new_rel.clone(),
                    receipt: outcome.receipt,
                });
                if let Some(finalization_error) = finalization_error {
                    for (_, destination) in renames.iter().skip(position + 1) {
                        paths.push(PathEffect {
                            file: destination.clone(),
                            state: EffectState::NotAttempted,
                            error: None,
                            category: None,
                        });
                    }
                    error = Some(format!(
                        "move committed but finalization failed: {finalization_error}"
                    ));
                    break;
                }
            }
            Err(move_error) => {
                paths.push(PathEffect {
                    file: new_rel.clone(),
                    state: EffectState::FailedBeforeCommit,
                    error: Some(move_error.to_string()),
                    category: Some(EffectFailure::Io),
                });
                for (_, destination) in renames.iter().skip(position + 1) {
                    paths.push(PathEffect {
                        file: destination.clone(),
                        state: EffectState::NotAttempted,
                        error: None,
                        category: None,
                    });
                }
                error = Some(move_error.to_string());
                break;
            }
        }
    }

    let mut rewrite_report = None;
    let mut rewrite_receipts = std::collections::HashMap::new();
    if error.is_none() {
        let moved_destinations: Vec<String> = renames.iter().map(|(_, new)| new.clone()).collect();
        let report = match link_rewrite::execute_plans_partial_with_receipts(
            dir,
            plans,
            &moved_destinations,
        ) {
            Ok(execution) => {
                rewrite_receipts = execution.receipts;
                execution.report
            }
            Err(rewrite_error) => {
                error = Some(format!("link rewrite preparation failed: {rewrite_error}"));
                link_rewrite::PartialExecuteReport::default()
            }
        };
        for outcome in &report.outcomes {
            paths.push(PathEffect {
                file: outcome.rel_path.clone(),
                state: if outcome.applied {
                    if outcome.error.is_some() {
                        EffectState::CommittedWithFinalizationError
                    } else {
                        EffectState::Committed
                    }
                } else {
                    EffectState::FailedBeforeCommit
                },
                error: outcome.error.clone(),
                category: outcome.error.as_ref().map(|_| {
                    if outcome.applied {
                        EffectFailure::Finalization
                    } else {
                        EffectFailure::Io
                    }
                }),
            });
        }
        if report.has_failures() {
            error = Some("one or more link rewrites failed".to_owned());
        }
        rewrite_report = Some(report);
    }

    if error.is_some() {
        let rewritten: HashSet<&str> = rewrite_report
            .as_ref()
            .into_iter()
            .flat_map(|report| report.outcomes.iter())
            .filter(|outcome| outcome.applied)
            .map(|outcome| outcome.rel_path.as_str())
            .collect();
        // Restore moved entries only when both their name and, for a rewritten
        // self-file, their exact prepared bytes still match this operation.
        for applied_move in applied.iter_mut().rev() {
            let old_rel = &applied_move.old_rel;
            let new_rel = &applied_move.new_rel;
            let self_plan = plans.iter().find(|plan| {
                plan.rel_path == *new_rel && rewritten.contains(plan.rel_path.as_str())
            });
            if let Some(receipt) = rewrite_receipts.remove(new_rel) {
                applied_move.receipt = receipt;
            }
            let restored_content = if let Some(plan) = self_plan {
                match restore_plan_bytes(&applied_move.receipt, plan, true, &mut session) {
                    Ok(receipt) => {
                        applied_move.receipt = receipt;
                        true
                    }
                    Err(restore_error) => {
                        paths.push(PathEffect {
                            file: new_rel.clone(),
                            state: EffectState::RestoreFailed,
                            error: Some(format!("self-rewrite restore refused: {restore_error}")),
                            category: Some(EffectFailure::SourceConflict),
                        });
                        continue;
                    }
                }
            } else {
                false
            };
            let restore = (|| -> Result<()> {
                let captured = applied_move.receipt.capture_verified()?;
                let (effect, _) = hyalo_core::rooted::move_no_replace_with_receipt(
                    captured,
                    root.destination(RelativeName::new(old_rel)?)?,
                    &mut session,
                )?;
                if let Some(error) = effect.finalization_error() {
                    bail!("restore published only partially: {error}");
                }
                Ok(())
            })();
            if restore.is_err()
                && restored_content
                && let Some(plan) = self_plan
            {
                let reapplied =
                    restore_plan_bytes(&applied_move.receipt, plan, false, &mut session);
                paths.push(PathEffect {
                    file: new_rel.clone(),
                    state: if reapplied.is_ok() {
                        EffectState::Kept
                    } else {
                        EffectState::RestoreFailed
                    },
                    error: reapplied.err().map(|e| {
                        format!(
                            "inverse move failed and forward content could not be restored: {e}"
                        )
                    }),
                    category: Some(EffectFailure::Io),
                });
            }
            paths.push(PathEffect {
                file: old_rel.clone(),
                state: if restore.is_ok() {
                    EffectState::Restored
                } else {
                    EffectState::RestoreFailed
                },
                error: restore.as_ref().err().map(ToString::to_string),
                category: restore.err().map(|_| EffectFailure::Io),
            });
        }
        for outcome in rewrite_report
            .as_ref()
            .into_iter()
            .flat_map(|report| report.outcomes.iter())
            .filter(|outcome| outcome.applied)
        {
            if !renames.iter().any(|(_, new)| new == &outcome.rel_path) {
                paths.push(PathEffect {
                    file: outcome.rel_path.clone(),
                    state: EffectState::Kept,
                    error: None,
                    category: None,
                });
            }
        }
        cleanup_created_directories(dir, &created, &mut paths);
    }

    if let Err(finish_error) = session.finish() {
        error = Some(match error {
            Some(existing) => format!("{existing}; finalization failed: {finish_error}"),
            None => format!("finalization failed: {finish_error}"),
        });
    }
    let mut safe: Vec<String> = renames
        .iter()
        .flat_map(|(old, new)| [old.clone(), new.clone()])
        .collect();
    safe.extend(
        paths
            .iter()
            .filter(|path| !path.file.ends_with('/'))
            .filter(|path| {
                !matches!(
                    path.state,
                    EffectState::RestoreFailed | EffectState::NotAttempted
                )
            })
            .map(|path| path.file.clone()),
    );
    safe.sort();
    safe.dedup();
    let unsafe_paths: Vec<String> = paths
        .iter()
        .filter(|path| matches!(path.state, EffectState::RestoreFailed))
        .map(|path| path.file.clone())
        .collect();
    let (index, index_error) = journal.finalize_observed(dir, &safe, &unsafe_paths);
    Ok(MoveExecution {
        report: ApplyReport {
            paths,
            index,
            index_error,
        },
        error,
    })
}

fn mutation_failure(error: String, report: crate::commands::apply::ApplyReport) -> CommandOutcome {
    let mut diagnostic = crate::output::UserDiagnostic::new(error);
    diagnostic.category = Some("mutation_failure");
    diagnostic.hint =
        Some("inspect the reported committed, restored, and kept effects before retrying".into());
    diagnostic.effects = Some(report);
    CommandOutcome::UserError(diagnostic)
}

fn capture_move_sources(
    dir: &Path,
    sources: &[String],
) -> Result<Vec<hyalo_core::rooted::CapturedInput>> {
    use hyalo_core::rooted::{RelativeName, VaultRoot};
    let root = VaultRoot::new(dir)?;
    let mut identities = HashSet::new();
    let mut captured = Vec::with_capacity(sources.len());
    let mut bytes = 0u64;
    for source in sources {
        if fs::symlink_metadata(dir.join(source))?
            .file_type()
            .is_symlink()
        {
            bail!(hyalo_core::UserFacingError {
                message: format!("refusing to move symlink source: {source}"),
                hint: Some(
                    "move the regular file itself; symlink entry moves are unsupported".into()
                ),
                cause: None,
            });
        }
        let input = root.capture(&RelativeName::new(source)?)?;
        if !identities.insert(input.physical_identity()) {
            bail!(hyalo_core::UserFacingError {
                message: format!("duplicate physical move source: {source}"),
                hint: Some(
                    "select each file once; aliases and hard links cannot be moved together".into()
                ),
                cause: None,
            });
        }
        bytes = bytes
            .checked_add(input.size())
            .context("move staging size overflow")?;
        if bytes > 8 * 1024 * 1024 * 1024 {
            bail!("move preparation exceeds 8 GiB staging budget");
        }
        captured.push(input);
    }
    Ok(captured)
}

struct OwnedDirectory {
    relative: PathBuf,
    identity: same_file::Handle,
}

fn cleanup_created_directories(
    dir: &Path,
    created: &[OwnedDirectory],
    paths: &mut Vec<crate::commands::apply::PathEffect>,
) {
    use crate::commands::apply::{EffectFailure, EffectState, PathEffect};
    for owned in created.iter().rev() {
        let full = dir.join(&owned.relative);
        let cleanup = (|| -> Result<()> {
            let current = same_file::Handle::from_path(&full)?;
            if current != owned.identity {
                bail!("directory identity changed; keeping it");
            }
            fs::remove_dir(&full)?;
            Ok(())
        })();
        paths.push(PathEffect {
            file: format!("{}/", owned.relative.to_string_lossy().replace('\\', "/")),
            state: if cleanup.is_ok() {
                EffectState::Restored
            } else {
                EffectState::Kept
            },
            error: cleanup.as_ref().err().map(ToString::to_string),
            category: cleanup.err().map(|_| EffectFailure::Io),
        });
    }
}

struct ProbeReceipt {
    path: PathBuf,
    identity: same_file::Handle,
}

fn cleanup_probes(receipts: &[ProbeReceipt]) -> Result<()> {
    let mut failures = Vec::new();
    for receipt in receipts.iter().rev() {
        let result = (|| -> Result<()> {
            let current = same_file::Handle::from_path(&receipt.path)?;
            if current != receipt.identity {
                bail!("probe identity changed; refusing cleanup");
            }
            fs::remove_file(&receipt.path)?;
            Ok(())
        })();
        if let Err(error) = result {
            failures.push(format!("{}: {error}", receipt.path.display()));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        bail!("move probe cleanup failed: {}", failures.join("; "))
    }
}

/// Test requested destination spellings in their actual parent namespace.
/// A common random prefix preserves each requested leaf's equivalence while
/// preventing collisions with user entries.
fn probe_destination_equivalence(dir: &Path, renames: &[(String, String)]) -> Result<()> {
    let mut groups: HashMap<PathBuf, Vec<std::ffi::OsString>> = HashMap::new();
    for (_, destination) in renames {
        let destination = Path::new(destination);
        let parent = destination.parent().unwrap_or_else(|| Path::new(""));
        let actual_parent = dunce::canonicalize(dir.join(parent))?;
        let leaf = destination
            .file_name()
            .context("move destination has no file name")?
            .to_os_string();
        groups.entry(actual_parent).or_default().push(leaf);
    }
    for (parent, leaves) in groups {
        let seed_file = tempfile::Builder::new()
            .prefix(".hyalo-move-probe-")
            .tempfile_in(&parent)?;
        let seed = seed_file
            .path()
            .file_name()
            .context("move probe has no file name")?
            .to_os_string();
        seed_file.close()?;
        let mut receipts = Vec::new();
        let mut collision = None;
        for leaf in leaves {
            let mut probe_leaf = seed.clone();
            probe_leaf.push("--");
            probe_leaf.push(&leaf);
            let path = parent.join(probe_leaf);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => match same_file::Handle::from_file(file) {
                    Ok(identity) => receipts.push(ProbeReceipt { path, identity }),
                    Err(error) => {
                        collision = Some(format!(
                            "could not record ownership of destination-name probe {}; \
                             leaving that probe untouched for safe inspection: {error}",
                            path.display()
                        ));
                        break;
                    }
                },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    collision = Some(format!(
                        "destination names are equivalent on this filesystem near {}: {}",
                        parent.display(),
                        leaf.to_string_lossy()
                    ));
                    break;
                }
                Err(error) => {
                    collision = Some(format!(
                        "could not verify destination name equivalence in {}: {error}",
                        parent.display()
                    ));
                    break;
                }
            }
        }
        let cleanup = cleanup_probes(&receipts);
        if let Some(collision) = collision {
            cleanup?;
            bail!(collision);
        }
        cleanup?;
    }
    Ok(())
}

fn exact_entry_exists(parent: &Path, leaf: &std::ffi::OsStr) -> Result<bool> {
    for entry in fs::read_dir(parent)? {
        if entry?.file_name() == leaf {
            return Ok(true);
        }
    }
    Ok(false)
}

fn case_only_move(dir: &Path, old_rel: &str, new_rel: &str) -> Result<bool> {
    let old = Path::new(old_rel);
    let new = Path::new(new_rel);
    if old.parent() != new.parent() || old.file_name() == new.file_name() {
        return Ok(false);
    }
    let old_full = dir.join(old);
    let new_full = dir.join(new);
    if fs::symlink_metadata(&new_full).is_err() || !same_file::is_same_file(&old_full, &new_full)? {
        return Ok(false);
    }
    let parent = old_full.parent().context("move source has no parent")?;
    Ok(
        exact_entry_exists(parent, old.file_name().context("move source has no name")?)?
            && !exact_entry_exists(
                parent,
                new.file_name().context("move destination has no name")?,
            )?,
    )
}

fn cleanup_partial_link(receipt: &hyalo_core::rooted::OwnedPublication) -> Result<()> {
    let mut session =
        hyalo_core::rooted::WriteSession::new(hyalo_core::rooted::Durability::PerFile);
    let effect = receipt.capture_verified()?.remove(&mut session)?;
    if let Some(error) = effect.finalization_error() {
        bail!("partial move cleanup finalization failed: {error}");
    }
    session.finish()?;
    Ok(())
}

struct MoveLeg {
    receipt: hyalo_core::rooted::OwnedPublication,
    finalization_error: Option<String>,
}

fn no_replace_leg(
    root: &hyalo_core::rooted::VaultRoot,
    source: hyalo_core::rooted::CapturedInput,
    _source_name: &str,
    destination_name: &str,
    session: &mut hyalo_core::rooted::WriteSession,
    paths: &mut Vec<crate::commands::apply::PathEffect>,
) -> Result<MoveLeg> {
    use crate::commands::apply::{EffectFailure, EffectState, PathEffect};
    use hyalo_core::rooted::{RelativeName, move_no_replace_with_receipt};
    let (effect, receipt) = move_no_replace_with_receipt(
        source,
        root.destination(RelativeName::new(destination_name)?)?,
        session,
    )?;
    if effect.operation() != hyalo_core::rooted::Operation::Moved
        && let Some(error) = effect.finalization_error()
    {
        let cleanup = cleanup_partial_link(&receipt);
        paths.push(PathEffect {
            file: destination_name.to_owned(),
            state: if cleanup.is_ok() {
                EffectState::Restored
            } else {
                EffectState::Kept
            },
            error: cleanup
                .as_ref()
                .err()
                .map(|cleanup| format!("{error}; destination cleanup failed: {cleanup}")),
            category: cleanup.err().map(|_| EffectFailure::Io),
        });
        if paths
            .last()
            .is_some_and(|path| path.state == EffectState::Kept)
        {
            bail!("{error}; destination cleanup after partial move failed");
        }
        bail!("move was not published: {error}");
    }
    Ok(MoveLeg {
        receipt,
        finalization_error: effect.finalization_error().map(str::to_owned),
    })
}

struct MoveOneOutcome {
    receipt: hyalo_core::rooted::OwnedPublication,
    finalization_error: Option<String>,
}

fn move_one(
    dir: &Path,
    root: &hyalo_core::rooted::VaultRoot,
    source: hyalo_core::rooted::CapturedInput,
    old_rel: &str,
    new_rel: &str,
    session: &mut hyalo_core::rooted::WriteSession,
    paths: &mut Vec<crate::commands::apply::PathEffect>,
) -> Result<MoveOneOutcome> {
    move_one_with_temporary_capture(
        dir,
        root,
        source,
        old_rel,
        new_rel,
        session,
        paths,
        hyalo_core::rooted::OwnedPublication::capture_verified,
    )
}

#[allow(clippy::too_many_arguments)]
fn move_one_with_temporary_capture(
    dir: &Path,
    root: &hyalo_core::rooted::VaultRoot,
    source: hyalo_core::rooted::CapturedInput,
    old_rel: &str,
    new_rel: &str,
    session: &mut hyalo_core::rooted::WriteSession,
    paths: &mut Vec<crate::commands::apply::PathEffect>,
    capture_temporary: impl FnOnce(
        &hyalo_core::rooted::OwnedPublication,
    ) -> Result<hyalo_core::rooted::CapturedInput>,
) -> Result<MoveOneOutcome> {
    use crate::commands::apply::{EffectFailure, EffectState, PathEffect};
    if !case_only_move(dir, old_rel, new_rel)? {
        return no_replace_leg(root, source, old_rel, new_rel, session, paths).map(|leg| {
            MoveOneOutcome {
                receipt: leg.receipt,
                finalization_error: leg.finalization_error,
            }
        });
    }

    let parent = Path::new(old_rel).parent().unwrap_or_else(|| Path::new(""));
    let temp_file = tempfile::Builder::new()
        .prefix(".hyalo-case-move-")
        .tempfile_in(dir.join(parent))?;
    let temp_leaf = temp_file
        .path()
        .file_name()
        .context("case move temporary entry has no name")?
        .to_string_lossy()
        .into_owned();
    temp_file.close()?;
    let temp_rel = if parent.as_os_str().is_empty() {
        temp_leaf
    } else {
        parent.join(temp_leaf).to_string_lossy().replace('\\', "/")
    };
    let temporary_leg = no_replace_leg(root, source, old_rel, &temp_rel, session, paths)?;
    if let Some(error) = temporary_leg.finalization_error {
        let restore = temporary_leg
            .receipt
            .capture_verified()
            .and_then(|temporary| {
                no_replace_leg(root, temporary, &temp_rel, old_rel, session, paths).map(|_| ())
            });
        paths.push(PathEffect {
            file: old_rel.to_owned(),
            state: if restore.is_ok() {
                EffectState::Restored
            } else {
                EffectState::RestoreFailed
            },
            error: restore.as_ref().err().map(ToString::to_string),
            category: Some(EffectFailure::Finalization),
        });
        bail!("case-only move first leg finalization failed: {error}");
    }
    let second = match capture_temporary(&temporary_leg.receipt) {
        Ok(second) => second,
        Err(error) => {
            let restore = temporary_leg
                .receipt
                .capture_verified()
                .and_then(|temporary| {
                    no_replace_leg(root, temporary, &temp_rel, old_rel, session, paths).and_then(
                        |leg| {
                            if let Some(error) = leg.finalization_error {
                                bail!("temporary move restoration finalization failed: {error}");
                            }
                            Ok(())
                        },
                    )
                });
            paths.push(PathEffect {
                file: temp_rel.clone(),
                state: if restore.is_ok() {
                    EffectState::Restored
                } else {
                    EffectState::Kept
                },
                error: restore.as_ref().err().map(ToString::to_string),
                category: restore.as_ref().err().map(|_| EffectFailure::Io),
            });
            paths.push(PathEffect {
                file: old_rel.to_owned(),
                state: if restore.is_ok() {
                    EffectState::Restored
                } else {
                    EffectState::RestoreFailed
                },
                error: restore.as_ref().err().map(ToString::to_string),
                category: restore.err().map(|_| EffectFailure::Io),
            });
            return Err(error.context(
                "case-only move could not verify its temporary publication; compensation was attempted",
            ));
        }
    };
    let final_receipt = match no_replace_leg(root, second, &temp_rel, new_rel, session, paths) {
        Ok(leg) => leg,
        Err(error) => {
            let restore = (|| -> Result<()> {
                let temporary = temporary_leg.receipt.capture_verified()?;
                no_replace_leg(root, temporary, &temp_rel, old_rel, session, paths).map(|_| ())
            })();
            paths.push(PathEffect {
                file: old_rel.to_owned(),
                state: if restore.is_ok() {
                    EffectState::Restored
                } else {
                    EffectState::RestoreFailed
                },
                error: restore.as_ref().err().map(ToString::to_string),
                category: restore.err().map(|_| EffectFailure::Io),
            });
            return Err(error);
        }
    };
    Ok(MoveOneOutcome {
        receipt: final_receipt.receipt,
        finalization_error: final_receipt.finalization_error,
    })
}

fn restore_plan_bytes(
    receipt: &hyalo_core::rooted::OwnedPublication,
    plan: &RewritePlan,
    reverse: bool,
    session: &mut hyalo_core::rooted::WriteSession,
) -> Result<hyalo_core::rooted::OwnedPublication> {
    let replacement = if reverse {
        plan.original_content
            .as_deref()
            .context("self-rewrite plan has no original content")?
            .as_bytes()
    } else {
        plan.rewritten_content.as_bytes()
    };
    let captured = receipt.capture_verified()?;
    let (effect, replacement_receipt) = captured
        .prepare(replacement, session)?
        .commit_with_receipt(session)?;
    if let Some(error) = effect.finalization_error() {
        bail!("self-rewrite restoration finalization failed: {error}");
    }
    Ok(replacement_receipt)
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Verify that a prospective destination `new_rel` (relative to `dir`) stays
/// within the vault, even when its parent directories don't exist yet or an
/// in-vault path component is a symlink that resolves outside the vault
/// (H-3).
///
/// Delegates to [`hyalo_core::escaping_write_target`], which anchors
/// on the nearest existing ancestor: `fs::create_dir_all` only ever creates
/// plain directories — never symlinks — so an in-vault anchor guarantees every
/// component created below it also stays in the vault. Must be called before
/// any `fs::create_dir_all`/`fs::rename` on the destination.
///
/// `canonical_vault` is accepted (rather than re-derived) so callers that
/// already canonicalized the vault keep a single source of truth for it.
fn ensure_dest_within_vault(
    canonical_vault: &Path,
    dir: &Path,
    new_rel: &str,
) -> std::result::Result<(), (String, String)> {
    let dst = dir.join(new_rel);
    match hyalo_core::escaping_write_target(canonical_vault, &dst) {
        Ok(None) => Ok(()),
        Ok(Some(target)) => Err((
            hyalo_core::outside_vault_message_with_dir(
                "target path",
                Some(&target),
                canonical_vault,
            ),
            hyalo_core::outside_vault_hint(canonical_vault),
        )),
        Err(e) => Err((
            format!("failed to verify destination {new_rel} stays within the vault: {e}"),
            String::new(),
        )),
    }
}

/// Strip a CWD-relative vault-dir prefix from a `mv` destination, exactly as
/// `resolve_file` already does for the *source* (iter-273, MV-1 / BUG-14).
///
/// With `dir = "kb"` in `.hyalo.toml`, `hyalo mv kb/a.md kb/sub/a.md` run from
/// the project root resolved the source to `a.md` and the destination to the
/// literal `kb/sub/a.md`, which is vault-relative — so the file landed in
/// `kb/kb/sub/a.md`. Every destination form had the bug (positional DEST, `--to
/// file`, `--to dir/`, batch), because all four bypassed the normalisation the
/// source goes through. Applied once, in the dispatch arm, so the four forms
/// cannot drift apart again.
///
/// A trailing slash is preserved: single mode reads it as "this is a
/// directory", and losing it would turn a directory destination into a
/// `.md`-less filename.
fn strip_vault_prefix_from_destination(dir: &Path, to_arg: &str) -> String {
    // MV-4 (iter-275, BUG-10): an absolute destination *inside* the vault is
    // accepted exactly as an absolute source is — `mv sub/a.md
    // /abs/vault/a3.md` used to be refused with "target path must be relative
    // and within the vault" about a path that was both, while the source form
    // was accepted with a stylistic nag. Same helper, same nag.
    if let Some(rel) = hyalo_core::discovery::strip_absolute_vault_prefix(dir, to_arg) {
        crate::warn::warn_llm_misuse(dir);
        return rel;
    }
    let normalized = to_arg.replace('\\', "/");
    let mut trimmed = normalized.as_str();
    while let Some(rest) = trimmed.strip_prefix("./") {
        trimmed = rest;
    }
    // MV-4: `--to .`, `--to ./` and `--to <vault-dir>/` all name the vault
    // root. The empty string is this function's root sentinel; DEC-304's
    // prefix strip used to leave `"/"` behind for it, which every downstream
    // check then read as an absolute path.
    if trimmed.is_empty() || trimmed == "." {
        return String::new();
    }
    let bare = trimmed.trim_end_matches('/');
    let had_trailing_slash = bare.len() != trimmed.len();
    if destination_names_vault_root(dir, bare) {
        return String::new();
    }
    match hyalo_core::discovery::strip_dir_prefix(dir, bare) {
        Some(stripped) if had_trailing_slash => format!("{stripped}/"),
        Some(stripped) => stripped,
        // No prefix to strip: keep what the caller typed (minus any `./`).
        None if had_trailing_slash => format!("{bare}/"),
        None => bare.to_owned(),
    }
}

/// Whether a `--to` path names the configured vault directory itself, and so
/// means "the vault root" (iter-275, MV-4).
///
/// With `dir = "kb"` in `.hyalo.toml`, `--to kb/` used to be read as the
/// *subdirectory* `kb/kb`, and the error suggested creating `kb/` — the vault
/// the caller was already inside. `strip_dir_prefix` cannot answer this: it
/// returns `None` both for "no prefix here" and for "the prefix is the whole
/// path".
fn destination_names_vault_root(dir: &Path, bare: &str) -> bool {
    if bare.is_empty() {
        return false;
    }
    let dir_str = dir.to_string_lossy().replace('\\', "/");
    if bare == dir_str.trim_end_matches('/') {
        return true;
    }
    dir.file_name()
        .is_some_and(|name| bare == name.to_string_lossy())
}

/// Outcome of validating a single-file `mv` destination.
enum TargetSingle {
    /// The vault-relative destination path to move to.
    Path(String),
    /// The destination is taken and `--on-conflict skip` was asked for, so the
    /// move is a no-op rather than a failure. Carries the destination that was
    /// already occupied.
    Skipped(String),
}

/// Validate the `--to` argument for single-file mode.
/// Returns the vault-relative normalized path, a skip verdict, or a
/// `CommandOutcome` error.
fn validate_target_single(
    dir: &Path,
    to_arg: &str,
    src_rel: &str,
    format: Format,
    on_conflict: ConflictPolicy,
) -> std::result::Result<TargetSingle, Box<CommandOutcome>> {
    let normalized = to_arg.replace('\\', "/");
    // MV-4 (iter-275): strip *every* leading `./`, and read a bare `.` as the
    // vault root. `--to ./` used to normalise to the empty string and then be
    // rebuilt as `/a.md`, which the traversal guard below rejected as
    // "must be relative and within the vault".
    let mut trimmed = normalized.as_str();
    while let Some(rest) = trimmed.strip_prefix("./") {
        trimmed = rest;
    }
    let normalized = if trimmed == "." {
        String::new()
    } else {
        trimmed.to_owned()
    };

    // UX (BUG-21, iter-276): a destination carrying `..` is refused for the
    // same reason a source is — every path is vault-relative (DEC-304) — so it
    // must say the same thing. `--to ../deep/` used to be answered with
    // "destination directory does not exist", which invites the reader to go
    // and create a directory outside the vault.
    if normalized.split('/').any(|seg| seg == "..") {
        let out = crate::output::user_diagnostic(
            format,
            "path contains '..' and is rejected",
            Some(&normalized),
            Some(
                "destinations are vault-relative, like sources — drop the '..' and name the path \
from the vault root, e.g. \"sub/note.md\"",
            ),
            None,
        );
        return Err(Box::new(CommandOutcome::UserError(out)));
    }

    // Must end with .md
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    if !normalized.ends_with(".md") {
        // Check if target is an existing directory — auto-append basename.
        // An empty destination is the vault root, which is always a directory.
        let target_path = dir.join(&normalized);
        if normalized.is_empty() || target_path.is_dir() {
            let basename = Path::new(src_rel)
                .file_name()
                .map_or_else(|| src_rel.to_string(), |n| n.to_string_lossy().into_owned());
            let with_basename = if normalized.is_empty() {
                basename
            } else if normalized.ends_with('/') {
                format!("{normalized}{basename}")
            } else {
                format!("{normalized}/{basename}")
            };
            return validate_target_single(dir, &with_basename, src_rel, format, on_conflict);
        }
        // MV-1 (iter-273, BUG-24): a trailing slash is directory syntax, so
        // `--to sub/` on a directory that does not exist used to be answered
        // with `did you mean sub/.md?` — a path nothing can ever name. Say
        // what is actually wrong: the directory is missing.
        if normalized.ends_with('/') {
            let bare = normalized.trim_end_matches('/');
            let basename = Path::new(src_rel)
                .file_name()
                .map_or_else(|| src_rel.to_string(), |n| n.to_string_lossy().into_owned());
            let out = crate::output::user_diagnostic(
                format,
                "destination directory does not exist",
                Some(&normalized),
                Some(&format!(
                    "create {bare}/ first, or name the destination file directly \
                     (--to {bare}/{basename})"
                )),
                None,
            );
            return Err(Box::new(CommandOutcome::UserError(out)));
        }
        let out = crate::output::user_diagnostic(
            format,
            "target path must end with .md",
            Some(&normalized),
            Some(&format!("did you mean {normalized}.md?")),
            None,
        );
        return Err(Box::new(CommandOutcome::UserError(out)));
    }

    // Reject path traversal
    let has_traversal = std::path::Path::new(&normalized).components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::RootDir
        )
    }) || std::path::Path::new(&normalized).is_absolute();
    if has_traversal {
        let out = crate::output::user_diagnostic(
            format,
            "target path must be relative and within the vault",
            Some(&normalized),
            None,
            None,
        );
        return Err(Box::new(CommandOutcome::UserError(out)));
    }

    if normalized == src_rel {
        let out = crate::output::user_diagnostic(
            format,
            "source and destination are the same path",
            Some(&normalized),
            Some("choose a different destination path"),
            None,
        );
        return Err(Box::new(CommandOutcome::UserError(out)));
    }

    let target_path = dir.join(&normalized);
    // L-4: `exists()` follows symlinks, so a *dangling* symlink at DEST reads as
    // absent and the rename silently replaced it — destroying the link without a
    // word. `symlink_metadata` answers "is there an entry here", which is the
    // question the collision guard is actually asking.
    if target_path.symlink_metadata().is_ok() && !target_path.exists() {
        let out = crate::output::user_diagnostic(
            format,
            "target path is a broken symlink",
            Some(&normalized),
            Some("remove the dangling symlink first, then re-run the move"),
            None,
        );
        return Err(Box::new(CommandOutcome::UserError(out)));
    }
    if target_path.exists() {
        // L-14: on a case-insensitive filesystem, a pure case rename like
        // `a.md` → `A.md` reports the destination as "existing" because it
        // resolves to the same inode as the source. Reuse batch mode's
        // canonicalize-based same-file check (see `dest_to_sources` handling)
        // so such renames are allowed rather than rejected as a collision.
        let src_path = dir.join(src_rel);
        let same_file = target_path
            .canonicalize()
            .ok()
            .zip(src_path.canonicalize().ok())
            .is_some_and(|(d, s)| d == s);
        if !same_file {
            if on_conflict.is_skip() {
                return Ok(TargetSingle::Skipped(normalized));
            }
            let out = crate::output::user_diagnostic(
                format,
                "target file already exists",
                Some(&normalized),
                Some("pass --on-conflict skip to leave the source in place instead"),
                None,
            );
            return Err(Box::new(CommandOutcome::UserError(out)));
        }
    }

    // H-3: reject destinations that would escape the vault through a
    // symlinked directory component, even when the target's parent
    // directories don't exist yet.
    let canonical_vault = match canonicalize_vault_dir(dir) {
        Ok(c) => c,
        Err(e) => {
            let out = crate::output::user_diagnostic(
                format,
                "failed to canonicalize vault directory",
                Some(&e.to_string()),
                None,
                None,
            );
            return Err(Box::new(CommandOutcome::UserError(out)));
        }
    };
    if let Err((msg, hint)) = ensure_dest_within_vault(&canonical_vault, dir, &normalized) {
        let hint_opt = (!hint.is_empty()).then_some(hint.as_str());
        let out = crate::output::user_diagnostic(format, &msg, Some(&normalized), hint_opt, None);
        return Err(Box::new(CommandOutcome::UserError(out)));
    }

    Ok(TargetSingle::Path(normalized))
}

/// Validate the `--to` argument for batch mode: must be directory-shaped.
/// Returns a normalized directory path string (no trailing slash).
fn validate_batch_target(
    to_arg: &str,
    format: Format,
) -> std::result::Result<String, Box<CommandOutcome>> {
    let replaced = to_arg.replace('\\', "/");
    let mut trimmed = replaced.as_str();
    while let Some(rest) = trimmed.strip_prefix("./") {
        trimmed = rest;
    }
    let normalized = if trimmed == "." {
        String::new()
    } else {
        trimmed.trim_end_matches('/').to_owned()
    };

    // Reject .md suffix in batch mode.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    if normalized.ends_with(".md") {
        let out = crate::output::user_diagnostic(
            format,
            "batch --to must be a directory path, not a .md file",
            Some(to_arg),
            Some("use a directory path (with or without trailing '/') for batch moves"),
            None,
        );
        return Err(Box::new(CommandOutcome::UserError(out)));
    }

    // MV-4 (iter-275): an empty normalised path is the **vault root** —
    // `--to .`, `--to ./` and `--to <vault-dir>/` all mean "move these to the
    // top of the vault", which is a legitimate batch destination. A literally
    // empty `--to ""` is caught in `run` before the vault prefix is stripped,
    // where the caller's own text is still available to quote.

    // Reject path traversal.
    let has_traversal = std::path::Path::new(&normalized).components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::RootDir
        )
    }) || std::path::Path::new(&normalized).is_absolute();
    if has_traversal {
        let out = crate::output::user_diagnostic(
            format,
            "target path must be relative and within the vault",
            Some(&normalized),
            None,
            None,
        );
        return Err(Box::new(CommandOutcome::UserError(out)));
    }

    Ok(normalized)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn describe_filters(
    globs: &[String],
    property_filters: &[PropertyFilter],
    tag_filters: &[String],
) -> String {
    let mut parts = Vec::new();
    for g in globs {
        parts.push(format!("--glob {g}"));
    }
    for f in property_filters {
        parts.push(format!("--property {f:?}"));
    }
    for t in tag_filters {
        parts.push(format!("--tag {t}"));
    }
    parts.join(", ")
}

// ---------------------------------------------------------------------------
// Dispatch handler (ARCH-1, iter-225)
// ---------------------------------------------------------------------------

/// The `hyalo mv` dispatch arm, extracted verbatim from `dispatch.rs`.
/// `files_from` and `index_flags` were consumed earlier in `run.rs`
/// (snapshot loading) and never reach here.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::needless_pass_by_value)] // args moved verbatim from the clap variant
#[allow(clippy::items_after_statements)] // extracted handler keeps its mid-fn imports (ARCH-1, iter-225)
pub(crate) fn run(
    ctx: &mut crate::dispatch::CommandContext<'_>,
    file_positional: Option<String>,
    file: Option<String>,
    to_positional: Option<String>,
    to: Option<String>,
    glob: Vec<String>,
    properties: Vec<String>,
    tag: Vec<String>,
    r#type: Vec<String>,
    dry_run: bool,
    apply: bool,
    on_conflict: ConflictPolicy,
    allow_ambiguous: bool,
) -> Result<CommandOutcome> {
    let dir = ctx.dir;
    let site_prefix = ctx.site_prefix;
    let effective_format = ctx.effective_format;
    let mut journal =
        crate::commands::journal::MutationJournal::new(&mut *ctx.snapshot_index, ctx.index_path);
    use crate::cli::args::resolve_single_file;
    use crate::dispatch::property_filter_error_outcome;

    // Resolve the destination from either the positional DEST alias or
    // the --to flag (iter-181 task 5). clap enforces they are mutually
    // exclusive and that DEST requires the positional source; a missing
    // destination (neither form given) is reported here.
    let Some(to) = to.or(to_positional) else {
        return Ok(CommandOutcome::UserError(crate::output::user_diagnostic(
            effective_format,
            "no destination provided: pass DEST positionally (e.g. `hyalo mv old.md new.md`) or --to <path>",
            None,
            None,
            None,
        )));
    };
    if to.trim().is_empty() {
        return Ok(CommandOutcome::UserError(crate::output::user_diagnostic(
            effective_format,
            "destination cannot be empty",
            Some(&to),
            Some("pass a path, or `.` for the vault root"),
            None,
        )));
    }
    // MV-1 (iter-273): the destination goes through the same CWD-relative →
    // vault-relative normalisation the source does, so `hyalo mv kb/a.md
    // kb/sub/a.md` from the project root stops creating `kb/kb/sub/a.md`.
    // MV-4 (iter-275) extends it to absolute in-vault destinations and to the
    // vault root itself, which the strip used to leave as a bare `/`.
    let to = strip_vault_prefix_from_destination(dir, &to);
    // Parse property filters for batch mode
    let prop_filters: Vec<hyalo_core::filter::PropertyFilter> = match properties
        .iter()
        .map(|s| hyalo_core::filter::parse_property_filter(s))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(f) => f,
        Err(e) => {
            return Ok(property_filter_error_outcome(&e, effective_format));
        }
    };
    // Build type filters as additional property filters (type=<value>)
    let type_filters: Vec<hyalo_core::filter::PropertyFilter> = {
        let mut tf = Vec::new();
        for t in &r#type {
            match hyalo_core::filter::parse_property_filter(&format!("type={t}")) {
                Ok(f) => tf.push(f),
                Err(e) => {
                    return Ok(CommandOutcome::UserError(crate::output::user_diagnostic(
                        effective_format,
                        &e.to_string(),
                        None,
                        None,
                        None,
                    )));
                }
            }
        }
        tf
    };
    let all_prop_filters: Vec<hyalo_core::filter::PropertyFilter> =
        prop_filters.into_iter().chain(type_filters).collect();

    let has_selectors = !glob.is_empty() || !all_prop_filters.is_empty() || !tag.is_empty();
    let has_file = file_positional.is_some() || file.is_some();

    if !has_selectors && !has_file {
        return Ok(CommandOutcome::UserError(crate::output::user_diagnostic(
            effective_format,
            "no source selection provided: pass a FILE (single-file mode) or at least one of --glob/--property/--tag/--type (batch mode)",
            None,
            None,
            None,
        )));
    }

    let is_batch = has_selectors;

    if is_batch {
        // Validate tag filters.
        for t in &tag {
            if let Err(msg) = crate::commands::tags::validate_tag(t) {
                return Ok(CommandOutcome::UserError(crate::output::user_diagnostic(
                    effective_format,
                    &msg,
                    None,
                    None,
                    None,
                )));
            }
        }

        // --dry-run and --apply are mutually exclusive (also enforced by clap).
        let effective_apply = apply && !dry_run;

        self::mv_batch(
            dir,
            file_positional.as_deref(),
            file.as_deref(),
            &glob,
            &all_prop_filters,
            &tag,
            &to,
            effective_apply,
            on_conflict,
            effective_format,
            site_prefix,
            &mut journal,
            allow_ambiguous,
        )
    } else {
        // `mv` has an asymmetric default: single-file mode writes
        // immediately, batch mode defaults to dry-run and needs
        // `--apply` to commit. Accepting `--apply` here as a silent
        // no-op hid that asymmetry from anyone who learned the batch
        // form first — they had no way to tell whether the flag was
        // doing the work or the default was (iter-192, DEC-192-mv-apply).
        if apply {
            return Ok(CommandOutcome::UserError(crate::output::user_diagnostic(
                effective_format,
                "single-file mv applies by default; use --dry-run to preview",
                None,
                Some(
                    "--apply is only meaningful in batch mode (--glob/--property/--tag/--type), which defaults to dry-run",
                ),
                None,
            )));
        }
        let file = match resolve_single_file(file_positional, file) {
            Ok(f) => f,
            Err(e) => {
                return Ok(CommandOutcome::UserError(crate::output::user_diagnostic(
                    effective_format,
                    &e.to_string(),
                    None,
                    None,
                    None,
                )));
            }
        };
        let effective_dry_run = dry_run;
        self::mv(
            dir,
            &file,
            &to,
            effective_dry_run,
            ctx.user_format,
            site_prefix,
            &mut journal,
            allow_ambiguous,
            on_conflict,
        )
    }
}

#[cfg(test)]
mod mutation_tests {
    use super::*;
    use hyalo_core::rooted::{Durability, FaultPoint, RelativeName, VaultRoot, WriteSession};

    #[test]
    fn partial_no_replace_source_removal_failure_reports_restored_destination() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), b"source").unwrap();
        let root = VaultRoot::new(dir.path()).unwrap();
        let source = root.capture(&RelativeName::new("a.md").unwrap()).unwrap();
        let mut session = WriteSession::with_fault(Durability::PerFile, FaultPoint::Remove);
        let mut paths = Vec::new();
        let Err(error) = no_replace_leg(&root, source, "a.md", "b.md", &mut session, &mut paths)
        else {
            panic!("injected source-removal failure must fail the move")
        };
        assert!(error.to_string().contains("not published"), "{error:#}");
        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0].state,
            crate::commands::apply::EffectState::Restored
        );
        assert_eq!(fs::read(dir.path().join("a.md")).unwrap(), b"source");
        assert!(!dir.path().join("b.md").exists());
    }

    #[test]
    fn competing_source_entry_survives_no_replace_compensation() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("old.md"), b"competitor").unwrap();
        fs::write(dir.path().join("new.md"), b"owned move").unwrap();
        let root = VaultRoot::new(dir.path()).unwrap();
        let source = root.capture(&RelativeName::new("new.md").unwrap()).unwrap();
        let mut session = WriteSession::new(Durability::PerFile);
        let mut paths = Vec::new();
        assert!(
            no_replace_leg(&root, source, "new.md", "old.md", &mut session, &mut paths,).is_err()
        );
        assert_eq!(fs::read(dir.path().join("old.md")).unwrap(), b"competitor");
        assert_eq!(fs::read(dir.path().join("new.md")).unwrap(), b"owned move");
    }

    #[test]
    fn directory_finalization_failure_reports_cleanup_and_unattempted_move() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), b"source").unwrap();
        let root = VaultRoot::new(dir.path()).unwrap();
        let source = root.capture(&RelativeName::new("a.md").unwrap()).unwrap();
        let renames = vec![("a.md".to_owned(), "one/two/a.md".to_owned())];
        let mut index = None;
        let mut journal = crate::commands::journal::MutationJournal::new(&mut index, None);
        let execution = execute_batch_mv_with_session(
            dir.path(),
            &renames,
            vec![source],
            &[],
            &mut journal,
            WriteSession::with_fault(Durability::PerFile, FaultPoint::Finalize),
        )
        .unwrap();

        assert!(execution.error.is_some());
        assert!(execution.report.paths.iter().any(|effect| {
            effect.file == "one/"
                && effect.state
                    == crate::commands::apply::EffectState::CommittedWithFinalizationError
        }));
        assert!(execution.report.paths.iter().any(|effect| {
            effect.file == "one/" && effect.state == crate::commands::apply::EffectState::Restored
        }));
        assert!(execution.report.paths.iter().any(|effect| {
            effect.file == "one/two/a.md"
                && effect.state == crate::commands::apply::EffectState::NotAttempted
        }));
        assert_eq!(fs::read(dir.path().join("a.md")).unwrap(), b"source");
        assert!(!dir.path().join("one").exists());
    }

    #[test]
    fn completed_move_finalization_failure_keeps_effect_and_compensates_owned_entry() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), b"source").unwrap();
        let root = VaultRoot::new(dir.path()).unwrap();
        let source = root.capture(&RelativeName::new("a.md").unwrap()).unwrap();
        let renames = vec![("a.md".to_owned(), "b.md".to_owned())];
        let mut index = None;
        let mut journal = crate::commands::journal::MutationJournal::new(&mut index, None);
        let execution = execute_batch_mv_with_session(
            dir.path(),
            &renames,
            vec![source],
            &[],
            &mut journal,
            WriteSession::with_fault(Durability::PerFile, FaultPoint::Finalize),
        )
        .unwrap();

        assert!(execution.error.is_some());
        assert!(execution.report.paths.iter().any(|effect| {
            effect.file == "b.md"
                && effect.state
                    == crate::commands::apply::EffectState::CommittedWithFinalizationError
        }));
        assert_eq!(fs::read(dir.path().join("a.md")).unwrap(), b"source");
        assert!(!dir.path().join("b.md").exists());
    }

    #[test]
    fn case_only_temporary_capture_failure_reports_and_restores_both_entries() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Source.md"), b"source").unwrap();
        if !case_only_move(dir.path(), "Source.md", "source.md").unwrap() {
            eprintln!("HYALO_CASE_MATRIX=case_only_temporary_capture_unsupported");
            return;
        }
        eprintln!("HYALO_CASE_MATRIX=case_only_temporary_capture_exercised");
        let root = VaultRoot::new(dir.path()).unwrap();
        let source = root
            .capture(&RelativeName::new("Source.md").unwrap())
            .unwrap();
        let mut session = WriteSession::new(Durability::PerFile);
        let mut paths = Vec::new();
        let result = move_one_with_temporary_capture(
            dir.path(),
            &root,
            source,
            "Source.md",
            "source.md",
            &mut session,
            &mut paths,
            |_| anyhow::bail!("injected temporary capture failure"),
        );
        let Err(error) = result else {
            panic!("injected temporary capture failure must fail the move")
        };

        assert!(error.to_string().contains("temporary publication"));
        assert!(exact_entry_exists(dir.path(), std::ffi::OsStr::new("Source.md")).unwrap());
        assert!(!exact_entry_exists(dir.path(), std::ffi::OsStr::new("source.md")).unwrap());
        assert_eq!(fs::read(dir.path().join("Source.md")).unwrap(), b"source");
        assert!(!fs::read_dir(dir.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".hyalo-case-move-")
        }));
        assert!(paths.iter().any(|effect| {
            effect.file.starts_with(".hyalo-case-move-")
                && effect.state == crate::commands::apply::EffectState::Restored
        }));
        assert!(paths.iter().any(|effect| {
            effect.file == "Source.md"
                && effect.state == crate::commands::apply::EffectState::Restored
        }));
    }
}
