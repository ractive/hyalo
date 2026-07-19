#![allow(clippy::missing_errors_doc)]
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::commands::mutation;
use crate::output::{CommandOutcome, Format};
use hyalo_core::discovery::{canonicalize_vault_dir, discover_files, match_globs};
use hyalo_core::filter::{PropertyFilter, matches_frontmatter_filters};
use hyalo_core::index::SnapshotIndex;
use hyalo_core::link_rewrite::{self, Replacement, RewritePlan, SkippedAmbiguous};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MvResult {
    from: String,
    to: String,
    dry_run: bool,
    updated_files: Vec<UpdatedFile>,
    total_files_updated: usize,
    total_links_updated: usize,
    /// Links that were skipped because the stem was ambiguous (NEW-3).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    skipped_ambiguous: Vec<SkippedAmbiguous>,
}

#[derive(Serialize, Clone)]
struct UpdatedFile {
    file: String,
    replacements: Vec<Replacement>,
}

#[derive(Serialize)]
struct MoveEntry {
    from: String,
    to: String,
}

#[derive(Serialize)]
struct BatchTotals {
    moves: usize,
    files_changed: usize,
    replacements: usize,
}

#[derive(Serialize)]
struct BatchMvResult {
    moves: Vec<MoveEntry>,
    updated_files: Vec<UpdatedFile>,
    totals: BatchTotals,
    applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    conflicts: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
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
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
    allow_ambiguous: bool,
) -> Result<CommandOutcome> {
    // 1. Validate source exists
    let (_src_full, old_rel) = match super::resolve_file_user(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(crate::commands::resolve_error_to_outcome(e, format)),
    };

    // 2. Validate target path
    let new_rel = match validate_target_single(dir, to_arg, &old_rel, format) {
        Ok(rel) => rel,
        Err(outcome) => return Ok(outcome),
    };

    // 3. Capture source fingerprint BEFORE planning so any concurrent edit
    //    during plan_mv is detected before the actual fs::rename.
    let src_mtime = hyalo_core::frontmatter::read_mtime(&dir.join(&old_rel))?;

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

    // NEW-3: emit stderr notes for text format (JSON envelope has skipped_ambiguous array).
    if format == Format::Text {
        for skipped in &mv_plan.skipped_ambiguous {
            eprintln!(
                "note: skipped ambiguous link [[{}]] at {}:{}\n      candidates: {}\n      \
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
    };

    // 6. If not dry-run, execute the move and rewrites, then update the index.
    if !dry_run {
        execute_mv(dir, &old_rel, &new_rel, &mv_plan.plans, src_mtime)?;

        // Patch index: rename the entry, re-scan files with rewritten links,
        // and update the link graph so backlink queries stay accurate.
        let rewritten: Vec<&str> = mv_plan.plans.iter().map(|p| p.rel_path.as_str()).collect();
        let mut index_dirty = false;
        mutation::rename_index_entry(
            snapshot_index,
            dir,
            &old_rel,
            &new_rel,
            &rewritten,
            &mut index_dirty,
        )?;
        mutation::save_index_if_dirty(snapshot_index, index_path, index_dirty)?;
    }

    Ok(CommandOutcome::success(
        serde_json::to_string_pretty(&result).context("failed to serialize")?,
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
    on_conflict: &str,
    format: Format,
    site_prefix: Option<&str>,
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
    allow_ambiguous: bool,
) -> Result<CommandOutcome> {
    // 1. Validate --to is a directory-shaped path.
    let to_dir = match validate_batch_target(to_arg, format) {
        Ok(d) => d,
        Err(outcome) => return Ok(outcome),
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
        let out = crate::output::format_error(
            format,
            "no files matched the given filters",
            Some(&filter_desc),
            Some("check your --glob, --property, --tag, or --type filters"),
            None,
        );
        return Ok(CommandOutcome::UserError(out));
    }

    // 4. Build rename map (old_rel → new_rel), detect collisions.
    let (renames, conflicts, skipped) =
        match build_rename_map(dir, &sources, &to_dir, on_conflict, format) {
            Ok(t) => t,
            Err(outcome) => return Ok(outcome),
        };

    // 5. Plan rewrites (build link graph once).
    let plans = if renames.is_empty() {
        vec![]
    } else {
        link_rewrite::plan_batch_mv(dir, &renames, site_prefix, allow_ambiguous)?
    };

    // 6. Build result.
    let moves: Vec<MoveEntry> = renames
        .iter()
        .map(|(f, t)| MoveEntry {
            from: f.clone(),
            to: t.clone(),
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
        moves,
        updated_files,
        applied: apply,
        conflicts,
        skipped,
    };

    // 7. Apply if requested.
    if apply && !renames.is_empty() {
        execute_batch_mv(dir, &renames, &plans)?;
        // Update index if present.
        if snapshot_index.is_some() {
            let mut index_dirty = false;
            let rewritten_paths: Vec<&str> = plans.iter().map(|p| p.rel_path.as_str()).collect();
            for (old_rel, new_rel) in &renames {
                mutation::rename_index_entry(
                    snapshot_index,
                    dir,
                    old_rel,
                    new_rel,
                    &rewritten_paths,
                    &mut index_dirty,
                )?;
            }
            mutation::save_index_if_dirty(snapshot_index, index_path, index_dirty)?;
        }
    }

    Ok(CommandOutcome::success(
        serde_json::to_string_pretty(&result).context("failed to serialize")?,
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
                let out = crate::output::format_error(
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

/// Build (renames, conflicts, skipped) from source paths and destination dir.
///
/// Returns `Ok((renames, conflicts, skipped))` or `Err(outcome)` on user error.
#[allow(clippy::type_complexity)]
fn build_rename_map(
    dir: &Path,
    sources: &[String],
    to_dir: &str,
    on_conflict: &str,
    format: Format,
) -> std::result::Result<(Vec<(String, String)>, Vec<String>, Vec<String>), CommandOutcome> {
    // Build proposed rename map: source → dest (old_rel → new_rel).
    let mut proposed: Vec<(String, String)> = Vec::new();
    for src in sources {
        let basename = Path::new(src)
            .file_name()
            .map_or_else(|| src.clone(), |n| n.to_string_lossy().into_owned());
        let new_rel = format!("{to_dir}/{basename}");
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
            let out = crate::output::format_error(
                format,
                "failed to canonicalize vault directory",
                Some(&e.to_string()),
                None,
                None,
            );
            CommandOutcome::UserError(out)
        })?;
        for (_, new_rel) in &proposed {
            if let Err(msg) = ensure_dest_within_vault(&canonical_vault, dir, new_rel) {
                let out = crate::output::format_error(format, &msg, Some(new_rel), None, None);
                return Err(CommandOutcome::UserError(out));
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
        if on_conflict == "skip" {
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
            return Ok((final_renames, vec![], skipped));
        }
        // Default: error
        let mut conflict_msgs: Vec<String> = all_collision_dests
            .iter()
            .flat_map(|dst| {
                let srcs = dest_to_sources.get(dst).map_or(&[][..], Vec::as_slice);
                srcs.iter().map(move |s| format!("{s} -> {dst}"))
            })
            .collect();
        conflict_msgs.sort();
        let desc = conflict_msgs.join(", ");
        let out = crate::output::format_error(
            format,
            "destination collision: multiple sources map to the same destination",
            Some(&desc),
            Some("use --on-conflict=skip to skip colliding files"),
            None,
        );
        return Err(CommandOutcome::UserError(out));
    }

    Ok((proposed, vec![], vec![]))
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// Execute a batch move: apply all renames, then write all rewrite plans.
/// On failure of any rename, roll back already-applied renames.
/// If link rewrite fails after renames succeeded, also rolls back renames.
fn execute_batch_mv(dir: &Path, renames: &[(String, String)], plans: &[RewritePlan]) -> Result<()> {
    // H-3 defense-in-depth: re-verify every destination stays within the
    // vault right before mutating the filesystem. `build_rename_map` already
    // rejects escaping destinations, but this guard makes the invariant
    // explicit here too and survives future refactors (mirrors the same
    // check in `link_rewrite::execute_plans`).
    let canonical_vault = canonicalize_vault_dir(dir)
        .context("failed to canonicalize vault directory for write safety check")?;
    for (_, new_rel) in renames {
        ensure_dest_within_vault(&canonical_vault, dir, new_rel).map_err(anyhow::Error::msg)?;
    }

    // Capture source mtimes upfront to detect concurrent modifications.
    let mut src_mtimes: Vec<(String, String, (std::time::SystemTime, u64))> = Vec::new();
    for (old_rel, new_rel) in renames {
        let src = dir.join(old_rel);
        let mtime = hyalo_core::frontmatter::read_mtime(&src)
            .with_context(|| format!("failed to read mtime for {}", src.display()))?;
        src_mtimes.push((old_rel.clone(), new_rel.clone(), mtime));
    }

    let mut applied: Vec<(PathBuf, PathBuf)> = Vec::new();

    for (old_rel, new_rel, mtime) in &src_mtimes {
        let src = dir.join(old_rel);
        let dst = dir.join(new_rel);

        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        // Check for concurrent modification before renaming.
        if let Err(e) = hyalo_core::frontmatter::check_mtime(&src, *mtime) {
            // Roll back already-applied renames.
            for (was_dst, was_src) in applied.iter().rev() {
                if let Err(rb_err) = fs::rename(was_dst, was_src) {
                    eprintln!(
                        "warning: rollback failed for {} -> {}: {rb_err}",
                        was_dst.display(),
                        was_src.display()
                    );
                }
            }
            return Err(e);
        }

        if let Err(e) = fs::rename(&src, &dst) {
            // Roll back already-applied renames.
            for (was_dst, was_src) in applied.iter().rev() {
                if let Err(rb_err) = fs::rename(was_dst, was_src) {
                    eprintln!(
                        "warning: rollback failed for {} -> {}: {rb_err}",
                        was_dst.display(),
                        was_src.display()
                    );
                }
            }
            return Err(anyhow::anyhow!(
                "failed to move {} to {}: {e}",
                src.display(),
                dst.display()
            ));
        }

        applied.push((dst, src));
    }

    // All renames succeeded — apply link rewrites.
    //
    // L-11 (DEC-187-batch-mv-report): completed link-rewrite `atomic_write`s
    // are NOT rolled back (a reliable content rollback would need per-file
    // pre-images and is out of scope). Instead we use the partial-execution
    // path so a mid-batch write failure can honestly *report* exactly which
    // files were durably rewritten before the abort, then still roll back the
    // renames to keep the directory structure consistent.
    let report = match link_rewrite::execute_plans_partial(dir, plans) {
        Ok(r) => r,
        Err(e) => {
            // Fatal (vault-boundary) failure: roll back renames and abort.
            rollback_renames(&applied);
            return Err(e);
        }
    };

    if report.has_failures() {
        // Roll back renames so the directory layout is consistent, then report
        // which link rewrites were durably applied before the failure.
        rollback_renames(&applied);
        let applied_paths = report.applied_paths();
        let failed: Vec<String> = report
            .outcomes
            .iter()
            .filter(|o| !o.applied)
            .map(|o| {
                format!(
                    "{}: {}",
                    o.rel_path,
                    o.error.as_deref().unwrap_or("write failed")
                )
            })
            .collect();
        return Err(anyhow::anyhow!(
            "batch mv aborted: {} link rewrite(s) failed [{}]; \
             {} file(s) were durably rewritten before the abort and were NOT rolled back [{}]; \
             renames were rolled back",
            failed.len(),
            failed.join("; "),
            applied_paths.len(),
            applied_paths.join(", "),
        ));
    }

    Ok(())
}

/// Roll back a set of already-applied renames (best effort; logs failures).
fn rollback_renames(applied: &[(PathBuf, PathBuf)]) {
    for (was_dst, was_src) in applied.iter().rev() {
        if let Err(rb_err) = fs::rename(was_dst, was_src) {
            eprintln!(
                "warning: rollback failed for {} -> {}: {rb_err}",
                was_dst.display(),
                was_src.display()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Walk up from `path` to find the nearest ancestor that exists on disk.
///
/// Used to find a safe point to canonicalize when the destination itself
/// (and possibly several of its parent directories) doesn't exist yet.
fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path;
    loop {
        if current.exists() {
            return current.to_path_buf();
        }
        match current.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => current = parent,
            _ => return current.to_path_buf(),
        }
    }
}

/// Verify that a prospective destination `new_rel` (relative to `dir`) stays
/// within the vault, even when its parent directories don't exist yet or an
/// in-vault path component is a symlink that resolves outside the vault
/// (H-3).
///
/// `fs::create_dir_all` only ever creates plain directories — never symlinks
/// — so canonicalizing the nearest *existing* ancestor of the destination and
/// checking it resolves inside `canonical_vault` is sufficient to guarantee
/// every path component that would be created below it also stays in the
/// vault. Must be called before any `fs::create_dir_all`/`fs::rename` on the
/// destination.
fn ensure_dest_within_vault(
    canonical_vault: &Path,
    dir: &Path,
    new_rel: &str,
) -> std::result::Result<(), String> {
    let dst = dir.join(new_rel);
    let start = dst.parent().unwrap_or(dir);
    let ancestor = nearest_existing_ancestor(start);
    let canonical_ancestor = dunce::canonicalize(&ancestor).map_err(|e| {
        format!(
            "failed to verify destination {} stays within the vault: {e}",
            ancestor.display()
        )
    })?;
    if canonical_ancestor.starts_with(canonical_vault) {
        Ok(())
    } else {
        Err(format!(
            "target path resolves outside vault boundary: {new_rel}"
        ))
    }
}

/// Validate the `--to` argument for single-file mode.
/// Returns vault-relative normalized path or a CommandOutcome error.
fn validate_target_single(
    dir: &Path,
    to_arg: &str,
    src_rel: &str,
    format: Format,
) -> std::result::Result<String, CommandOutcome> {
    let normalized = to_arg.replace('\\', "/");
    let normalized = normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_owned();

    // Must end with .md
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    if !normalized.ends_with(".md") {
        // Check if target is an existing directory — auto-append basename.
        let target_path = dir.join(&normalized);
        if target_path.is_dir() {
            let basename = Path::new(src_rel)
                .file_name()
                .map_or_else(|| src_rel.to_string(), |n| n.to_string_lossy().into_owned());
            let with_basename = if normalized.ends_with('/') {
                format!("{normalized}{basename}")
            } else {
                format!("{normalized}/{basename}")
            };
            return validate_target_single(dir, &with_basename, src_rel, format);
        }
        let out = crate::output::format_error(
            format,
            "target path must end with .md",
            Some(&normalized),
            Some(&format!("did you mean {normalized}.md?")),
            None,
        );
        return Err(CommandOutcome::UserError(out));
    }

    // Reject path traversal
    let has_traversal = std::path::Path::new(&normalized).components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::RootDir
        )
    }) || std::path::Path::new(&normalized).is_absolute();
    if has_traversal {
        let out = crate::output::format_error(
            format,
            "target path must be relative and within the vault",
            Some(&normalized),
            None,
            None,
        );
        return Err(CommandOutcome::UserError(out));
    }

    if normalized == src_rel {
        let out = crate::output::format_error(
            format,
            "source and destination are the same path",
            Some(&normalized),
            Some("choose a different destination path"),
            None,
        );
        return Err(CommandOutcome::UserError(out));
    }

    let target_path = dir.join(&normalized);
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
            let out = crate::output::format_error(
                format,
                "target file already exists",
                Some(&normalized),
                None,
                None,
            );
            return Err(CommandOutcome::UserError(out));
        }
    }

    // H-3: reject destinations that would escape the vault through a
    // symlinked directory component, even when the target's parent
    // directories don't exist yet.
    let canonical_vault = match canonicalize_vault_dir(dir) {
        Ok(c) => c,
        Err(e) => {
            let out = crate::output::format_error(
                format,
                "failed to canonicalize vault directory",
                Some(&e.to_string()),
                None,
                None,
            );
            return Err(CommandOutcome::UserError(out));
        }
    };
    if let Err(msg) = ensure_dest_within_vault(&canonical_vault, dir, &normalized) {
        let out = crate::output::format_error(format, &msg, Some(&normalized), None, None);
        return Err(CommandOutcome::UserError(out));
    }

    Ok(normalized)
}

/// Validate the `--to` argument for batch mode: must be directory-shaped.
/// Returns a normalized directory path string (no trailing slash).
fn validate_batch_target(
    to_arg: &str,
    format: Format,
) -> std::result::Result<String, CommandOutcome> {
    let normalized = to_arg.replace('\\', "/");
    let normalized = normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .trim_end_matches('/')
        .to_owned();

    // Reject .md suffix in batch mode.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    if normalized.ends_with(".md") {
        let out = crate::output::format_error(
            format,
            "batch --to must be a directory path, not a .md file",
            Some(to_arg),
            Some("use a directory path (with or without trailing '/') for batch moves"),
            None,
        );
        return Err(CommandOutcome::UserError(out));
    }

    // Reject empty path (e.g. `--to ./` or `--to /`).
    if normalized.is_empty() {
        let out = crate::output::format_error(
            format,
            "destination directory cannot be empty; use a relative subdirectory path",
            Some(to_arg),
            None,
            None,
        );
        return Err(CommandOutcome::UserError(out));
    }

    // Reject path traversal.
    let has_traversal = std::path::Path::new(&normalized).components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::RootDir
        )
    }) || std::path::Path::new(&normalized).is_absolute();
    if has_traversal {
        let out = crate::output::format_error(
            format,
            "target path must be relative and within the vault",
            Some(&normalized),
            None,
            None,
        );
        return Err(CommandOutcome::UserError(out));
    }

    Ok(normalized)
}

// ---------------------------------------------------------------------------
// Old execute_mv (single-file)
// ---------------------------------------------------------------------------

fn execute_mv(
    dir: &Path,
    old_rel: &str,
    new_rel: &str,
    plans: &[RewritePlan],
    src_mtime: (std::time::SystemTime, u64),
) -> Result<()> {
    let src = dir.join(old_rel);
    let dst = dir.join(new_rel);

    // H-3 defense-in-depth: re-verify the destination stays within the vault
    // right before mutating the filesystem. `validate_target_single` already
    // rejects escaping destinations, but this guard makes the invariant
    // explicit here too and survives future refactors (mirrors the same
    // check in `link_rewrite::execute_plans`).
    let canonical_vault = canonicalize_vault_dir(dir)
        .context("failed to canonicalize vault directory for write safety check")?;
    ensure_dest_within_vault(&canonical_vault, dir, new_rel).map_err(anyhow::Error::msg)?;

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    hyalo_core::frontmatter::check_mtime(&src, src_mtime)?;

    fs::rename(&src, &dst)
        .with_context(|| format!("failed to move {} to {}", src.display(), dst.display()))?;

    link_rewrite::execute_plans(dir, plans)?;

    Ok(())
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
