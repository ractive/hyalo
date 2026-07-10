//! Core rewriting engine for `hyalo mv`.
//!
//! Given a source-file move (`old_rel` → `new_rel`), this module:
//! 1. Finds all files that link *to* the moved file (inbound links) and
//!    rewrites those links to point at the new location.
//! 2. Rewrites relative markdown links *inside* the moved file whose targets
//!    change because the file's directory context has changed (outbound links).
//!
//! The public entry point is [`plan_mv`], which returns a list of
//! [`RewritePlan`] values without touching the filesystem.  Call
//! [`execute_plans`] to apply them.

#![allow(clippy::missing_errors_doc)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::case_index::CaseInsensitiveIndex;
use crate::discovery::{canonicalize_vault_dir, ensure_within_vault};
use crate::link_graph::{LinkGraph, normalize_target, relative_path_between};
use crate::link_resolve::LinkResolver;
use crate::link_write::{LinkWriter, SpanReplacement};
use crate::links::{LinkKind, PreserveForm, Resolution, extract_link_spans_with_original};
use crate::scanner::{
    FenceTracker, MAX_FILE_SIZE, is_comment_fence, strip_inline_code, strip_inline_comments,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A planned replacement within a single line of a file.
#[derive(Debug, Clone, Serialize)]
pub struct Replacement {
    /// 1-based line number.
    pub line: usize,
    /// Byte offset of `old_text` within the line.
    #[serde(skip)]
    pub byte_offset: usize,
    /// The original full link syntax, e.g. `[[old-path]]`.
    pub old_text: String,
    /// The replacement full link syntax, e.g. `[[new-path]]`.
    pub new_text: String,
}

/// A rewrite plan for a single file.
#[derive(Debug)]
pub struct RewritePlan {
    /// Absolute path to the file on disk.
    pub path: PathBuf,
    /// Vault-relative path (forward slashes).
    pub rel_path: String,
    /// All replacements to apply to this file.
    pub replacements: Vec<Replacement>,
    /// Full file content with all replacements already applied.
    pub rewritten_content: String,
    /// Fingerprint (mtime, file size) of the file when the plan was built, used
    /// to detect concurrent modifications before writing. `None` for the moved
    /// file's outbound plan (which is written to a new path after `fs::rename`
    /// and needs no check).
    pub mtime: Option<(std::time::SystemTime, u64)>,
}

/// A link that was skipped because it resolved to multiple candidates
/// (ambiguous bare stem), surfaced in the `mv` output for diagnostics (NEW-3).
#[derive(Debug, Clone, Serialize)]
pub struct SkippedAmbiguous {
    /// Vault-relative path of the file that contains the ambiguous link.
    pub source: String,
    /// 1-based line number of the link within that file.
    pub line: usize,
    /// The written target text (e.g. `"target"` from `[[target]]`).
    pub target: String,
    /// All candidate vault paths that the target matches.
    pub candidates: Vec<String>,
}

/// Result of [`plan_mv`]: the list of rewrite plans plus any ambiguous
/// inbound links that were skipped (NEW-3).
pub struct MvPlanResult {
    /// Rewrite plans for all files that will be modified.
    pub plans: Vec<RewritePlan>,
    /// Inbound links that were skipped because the stem was ambiguous.
    pub skipped_ambiguous: Vec<SkippedAmbiguous>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Plan all file rewrites needed when moving `old_rel` → `new_rel`.
///
/// Does NOT include the actual file move — only link updates in other files
/// (inbound links) and outbound link updates in the moved file itself.
///
/// Both `old_rel` and `new_rel` must use forward slashes and be relative to
/// `dir`.
///
/// `allow_ambiguous` — when `false` (default), bare wikilinks that map to an
/// ambiguous stem (multiple files with the same basename) are skipped with a
/// warning instead of being silently rewritten (BUG-2 fix).  Pass `true` to
/// opt-in to rewriting ambiguous bare wikilinks at the caller's risk.
pub fn plan_mv(
    dir: &Path,
    old_rel: &str,
    new_rel: &str,
    site_prefix: Option<&str>,
    allow_ambiguous: bool,
) -> Result<MvPlanResult> {
    // Step 1: build link graph to discover inbound links.
    // The build also yields a case-insensitive index of all vault paths, which
    // is used later to canonicalize link targets that differ only in casing.
    let build = LinkGraph::build(dir, site_prefix, None).context("building link graph")?;
    for (path, msg) in &build.warnings {
        eprintln!("warning: skipping {}: {msg}", path.display());
    }
    let graph = build.graph;
    let case_index = build.case_index;

    let old_stem = old_rel.strip_suffix(".md").unwrap_or(old_rel);
    let new_stem = new_rel.strip_suffix(".md").unwrap_or(new_rel);

    // Whether the file moves to a different directory.
    let old_dir = parent_dir(old_rel);
    let new_dir = parent_dir(new_rel);
    let dir_changed = old_dir != new_dir;

    // Step 2: gather inbound backlinks, grouped by source file.
    //
    // Skip self-links here — links that point from the moved file to itself
    // are handled as outbound rewrites. Leaving them in the inbound set would
    // produce a plan whose `path` still refers to `old_rel`, which no longer
    // exists on disk after `fs::rename` (NEW-BUG-2).
    //
    // Use the case-insensitive backlinks query so that links written with
    // different casing (e.g. `[[Web/JavaScript/…]]` targeting
    // `web/javascript/….md`) are included in the source set.  The
    // `plan_inbound_rewrites` function then re-checks the match using the
    // case index, so only genuinely matching links produce replacements.
    let backlinks = graph.backlinks_case_insensitive(old_rel);
    let old_rel_norm = old_rel.replace('\\', "/");
    let mut by_source: HashMap<PathBuf, Vec<_>> = HashMap::new();
    for entry in backlinks {
        let source_norm = entry.source.to_string_lossy().replace('\\', "/");
        if source_norm == old_rel_norm {
            continue;
        }
        by_source
            .entry(entry.source.clone())
            .or_default()
            .push(entry);
    }

    // Step 3: for each source file, build a RewritePlan for inbound links.
    let mut plans: HashMap<PathBuf, RewritePlan> = HashMap::new();
    let mut all_skipped_ambiguous: Vec<SkippedAmbiguous> = Vec::new();

    for source_rel in by_source.keys() {
        let abs_path = dir.join(source_rel);
        let source_rel_str = source_rel.to_string_lossy().replace('\\', "/");
        let meta = std::fs::metadata(&abs_path)
            .with_context(|| format!("failed to stat {}", abs_path.display()))?;
        let file_size = meta.len();
        let file_mtime = Some(
            meta.modified()
                .with_context(|| format!("failed to read mtime for {}", abs_path.display()))
                .map(|t| (t, file_size))?,
        );
        if file_size > MAX_FILE_SIZE {
            eprintln!(
                "warning: skipping {} ({} MiB exceeds {} MiB limit)",
                abs_path.display(),
                file_size / (1024 * 1024),
                MAX_FILE_SIZE / (1024 * 1024)
            );
            continue;
        }
        let content = std::fs::read_to_string(&abs_path)
            .with_context(|| format!("reading {}", abs_path.display()))?;

        let (replacements, skipped) = plan_inbound_rewrites(
            &content,
            &source_rel_str,
            old_rel,
            old_stem,
            new_rel,
            new_stem,
            site_prefix,
            Some(&case_index),
            allow_ambiguous,
        );

        all_skipped_ambiguous.extend(skipped);

        if !replacements.is_empty() {
            let rewritten_content = apply_replacements(&content, &replacements);
            plans.insert(
                source_rel.clone(),
                RewritePlan {
                    path: abs_path,
                    rel_path: source_rel_str,
                    replacements,
                    rewritten_content,
                    mtime: file_mtime,
                },
            );
        }
    }

    // Step 4: plan outbound link updates for the moved file itself.
    //
    // Always run — the moved file may contain self-links that need rewriting
    // even when its directory didn't change. When the directory does change,
    // relative links to other files are also rebased here.
    //
    // NEW-1: wikilink self-links are now also rewritten here (previously only
    // markdown self-links were handled). The `plan_outbound_rewrites` function
    // accepts the case_index so it can match wikilink self-links via the
    // same resolver used for inbound rewrites.
    let old_abs = dir.join(old_rel);
    let old_meta = std::fs::metadata(&old_abs)
        .with_context(|| format!("failed to stat {}", old_abs.display()))?;
    let old_file_size = old_meta.len();
    // Capture mtime before reading content so concurrent edits can be detected
    // after fs::rename (which preserves mtime).
    let old_file_mtime = old_meta
        .modified()
        .with_context(|| format!("failed to read mtime for {}", old_abs.display()))
        .map(|t| (t, old_file_size))?;
    if old_file_size > MAX_FILE_SIZE {
        eprintln!(
            "warning: skipping outbound rewrite for {} ({} MiB exceeds {} MiB limit)",
            old_abs.display(),
            old_file_size / (1024 * 1024),
            MAX_FILE_SIZE / (1024 * 1024)
        );
        return Ok(MvPlanResult {
            plans: plans.into_values().collect(),
            skipped_ambiguous: all_skipped_ambiguous,
        });
    }
    let content = std::fs::read_to_string(&old_abs)
        .with_context(|| format!("reading {}", old_abs.display()))?;

    let outbound_replacements = plan_outbound_rewrites(
        &content,
        old_rel,
        old_stem,
        new_rel,
        new_stem,
        dir_changed,
        site_prefix,
        &case_index,
    );

    if !outbound_replacements.is_empty() {
        let moved_key = PathBuf::from(old_rel.replace('\\', "/"));
        // The path targets the NEW location — after fs::rename, that's where
        // the file lives and where the rewritten content must be written by
        // execute_plans.
        let new_abs = dir.join(new_rel);

        if let Some(existing) = plans.get_mut(&moved_key) {
            existing.path = new_abs;
            existing.rel_path = new_rel.to_string();
            existing.replacements.extend(outbound_replacements);
            existing.rewritten_content = apply_replacements(&content, &existing.replacements);
            // Preserve the mtime captured before reading so execute_plans can
            // still detect concurrent edits (fs::rename preserves mtime).
            existing.mtime = Some(old_file_mtime);
        } else {
            let rewritten_content = apply_replacements(&content, &outbound_replacements);
            plans.insert(
                moved_key,
                RewritePlan {
                    path: new_abs,
                    rel_path: new_rel.to_string(),
                    replacements: outbound_replacements,
                    rewritten_content,
                    // Preserve mtime — fs::rename keeps mtime, so execute_plans
                    // can detect concurrent edits before writing.
                    mtime: Some(old_file_mtime),
                },
            );
        }
    }

    Ok(MvPlanResult {
        plans: plans.into_values().collect(),
        skipped_ambiguous: all_skipped_ambiguous,
    })
}

/// Split a markdown-link target into its path portion and any trailing
/// `#fragment`. Returns `(path, fragment_including_hash)`.
///
/// For `"peer.md#intro"` this returns `("peer.md", "#intro")`; for a target
/// without a fragment the second element is `""`.
fn split_target_fragment(target: &str) -> (&str, &str) {
    match target.find('#') {
        Some(idx) => (&target[..idx], &target[idx..]),
        None => (target, ""),
    }
}

/// Decide whether a markdown-link target should be considered for rewriting
/// when a file moves.
///
/// Returns `false` (skip) for link targets that clearly do **not** point at a
/// vault markdown file:
/// - Site-absolute paths (start with `/`) — these are left untouched so that
///   downstream site renderers can resolve them from their own root.
/// - URL schemes (`http://`, `mailto:`, …) and fragment-only refs (`#anchor`).
///   Windows drive-letter paths like `C:\notes\x.md` are **not** treated as
///   URL schemes.
/// - Bare tokens with no `.md` suffix *and* no path separator — these look
///   like Obsidian wikilink labels or plain anchor text rather than file
///   paths.
///
/// Any trailing `#fragment` on the target is ignored when classifying — the
/// file portion is what matters. So `peer.md#intro` is still treated as an
/// `.md` link, and `#anchor` alone is still a pure fragment.
///
/// Inbound rewriting already narrows by string-equality against the moved
/// file's rel/stem, so this extra guard mostly protects outbound rewriting
/// from blindly rebasing non-filesystem references.
fn should_rewrite_outbound_target(target: &str) -> bool {
    if target.is_empty() || target.starts_with('#') {
        return false;
    }
    let (path_part, _) = split_target_fragment(target);
    if path_part.is_empty() {
        return false;
    }
    if path_part.starts_with('/') {
        return false;
    }
    // URL schemes: `http://`, `https://`, `mailto:`, `tel:`, …
    //
    // Exclude Windows drive-letter paths (single ASCII letter followed by `:`
    // and a path separator) — they are filesystem paths, not URLs.
    if let Some(colon) = path_part.find(':') {
        let scheme = &path_part[..colon];
        let is_drive_letter = colon == 1
            && scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && path_part
                .as_bytes()
                .get(colon + 1)
                .is_some_and(|b| *b == b'/' || *b == b'\\');
        if !is_drive_letter
            && !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        {
            return false;
        }
    }
    // Bare token with no `.md` suffix and no path separator: treat as label.
    let has_path_sep = path_part.contains('/') || path_part.contains('\\');
    let is_md = std::path::Path::new(path_part)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
    if !has_path_sep && !is_md {
        return false;
    }
    true
}

/// Execute rewrite plans: write each plan's `rewritten_content` back to disk.
///
/// `vault_dir` is the root vault directory.  Every plan path is asserted to be
/// within the vault before writing, which guards against future refactors
/// accidentally constructing paths that escape the vault.
pub fn execute_plans(vault_dir: &Path, plans: &[RewritePlan]) -> Result<()> {
    // Canonicalize once up front; all plan paths are resolved against this.
    let canonical_vault = canonicalize_vault_dir(vault_dir)
        .context("failed to canonicalize vault directory for write safety check")?;

    for plan in plans {
        // Safety assertion: verify the target is inside the vault before
        // writing.  Plans are generated by `plan_mv` which constrains paths to
        // the vault, but this check makes the invariant explicit and
        // survives future refactors.
        let within = ensure_within_vault(&canonical_vault, &plan.path)
            .with_context(|| format!("could not verify {} is within vault", plan.path.display()))?;
        anyhow::ensure!(
            within,
            "refusing to write outside vault: {}",
            plan.path.display()
        );

        // Detect concurrent modification: if the file was changed between
        // plan and execute, abort to avoid silently clobbering the new content.
        if let Some(expected_mtime) = plan.mtime {
            crate::frontmatter::check_mtime(&plan.path, expected_mtime)?;
        }

        crate::fs_util::atomic_write(&plan.path, plan.rewritten_content.as_bytes())
            .with_context(|| format!("writing {}", plan.path.display()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Inbound rewrite planning
// ---------------------------------------------------------------------------

/// Walk every line in `content` — including YAML frontmatter — and return
/// [`Replacement`]s for links that target the moved file, plus any
/// [`SkippedAmbiguous`] entries for links that were skipped due to ambiguity
/// (NEW-3).
///
/// Body links are matched via [`LinkResolver`] and rewritten with
/// [`LinkWriter`] so the new target is emitted in the user's original written
/// form (BUG-1 fix: `[[sub/target]]` stays path-form after rename, not
/// collapsed to `[[renamed]]`). Wikilinks found inside YAML frontmatter link
/// properties (`related`, `depends-on`, `supersedes`, `superseded-by`) are
/// also rewritten (H-4 fix: previously only the batch mv path did this,
/// leaving single-file mv with dangling frontmatter wikilinks).
///
/// When `allow_ambiguous` is `false`, bare body wikilinks whose stem matches
/// multiple vault files are skipped with a stderr warning instead of being
/// silently rewritten to an incorrect target (BUG-2 fix).
#[allow(clippy::too_many_arguments)]
fn plan_inbound_rewrites(
    content: &str,
    source_rel: &str,
    old_rel: &str,
    old_stem: &str,
    new_rel: &str,
    new_stem: &str,
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
    allow_ambiguous: bool,
) -> (Vec<Replacement>, Vec<SkippedAmbiguous>) {
    let resolver_idx = case_index.unwrap_or(&EMPTY_CASE_INDEX);
    let resolver = LinkResolver::new(resolver_idx, site_prefix);

    let mut replacements = Vec::new();
    let mut skipped_ambiguous: Vec<SkippedAmbiguous> = Vec::new();
    let mut fence = FenceTracker::new();
    let mut in_comment_fence = false;
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    let mut line_num = 0usize;

    for line in content.split('\n') {
        line_num += 1;

        // ---- Frontmatter handling (trim() matches scanner behaviour) ----
        if !frontmatter_done {
            if line_num == 1 && line.trim() == "---" {
                in_frontmatter = true;
                continue;
            }
            if in_frontmatter {
                if line.trim() == "---" {
                    in_frontmatter = false;
                    frontmatter_done = true;
                    continue;
                }
                // H-4: also rewrite wikilinks inside frontmatter link
                // properties so single-file mv doesn't leave dangling
                // frontmatter links (previously only the batch path did this).
                let fm_repls = plan_frontmatter_wikilink_rewrites(
                    line, line_num, old_rel, old_stem, new_rel, new_stem, case_index,
                );
                replacements.extend(fm_repls);
                continue;
            }
            // No frontmatter block found; mark done.
            frontmatter_done = true;
        }

        // ---- Comment fence (Obsidian %% blocks) ----
        if in_comment_fence {
            if is_comment_fence(line) {
                in_comment_fence = false;
            }
            continue;
        }

        // ---- Fenced code block ----
        if fence.process_line(line) {
            continue;
        }

        // ---- Comment fence opening (only outside code blocks) ----
        if is_comment_fence(line) {
            in_comment_fence = true;
            continue;
        }

        // ---- Extract and compare link spans ----
        let stripped_code = strip_inline_code(line);
        let cleaned = strip_inline_comments(stripped_code.as_ref());
        let spans = extract_link_spans_with_original(&cleaned, line);

        for span in spans {
            // BUG-2 fix / NEW-3: for bare wikilinks, check for ambiguity before
            // accepting the match. If the stem resolves to multiple files
            // and `allow_ambiguous` is false, record a SkippedAmbiguous entry
            // (for the mv JSON envelope) and emit a stderr note. When
            // `allow_ambiguous` is true, rewrite directly if the moved file
            // is among the ambiguous candidates (LinkResolver::matches_target
            // would reject the link because lookup_stem returns None for
            // ambiguous stems).
            let mut bare_ambiguous_match = false;
            if span.kind == LinkKind::Wikilink {
                let t = &span.link.target;
                // Only bare wikilinks (no path separator) need the ambiguity check.
                let normalized = if let Some(wo) = t.strip_prefix("./") {
                    std::borrow::Cow::Owned(normalize_target(Path::new(source_rel), wo))
                } else {
                    std::borrow::Cow::Borrowed(t.as_str())
                };
                let is_bare = !(normalized.contains('/') || normalized.contains('\\'));
                if is_bare && case_index.is_some() {
                    let t_norm = normalized.to_ascii_lowercase();
                    let stem = if std::path::Path::new(&t_norm)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                    {
                        &t_norm[..t_norm.len() - 3]
                    } else {
                        &t_norm
                    };
                    let res = resolver.resolve_stem(stem);
                    match res {
                        Resolution::Hit { ref vault_path } => {
                            let matches_old = vault_path == old_rel
                                || vault_path == old_stem
                                || vault_path.strip_suffix(".md") == Some(old_stem);
                            if !matches_old {
                                continue;
                            }
                        }
                        Resolution::Ambiguous(ref candidates) => {
                            // Check if the moved file is among the ambiguous
                            // candidates — only then is this link relevant.
                            let matches_old = candidates.iter().any(|c| {
                                c == old_rel
                                    || c == old_stem
                                    || c.strip_suffix(".md") == Some(old_stem)
                            });
                            if !matches_old {
                                continue;
                            }
                            if !allow_ambiguous {
                                // NEW-3: record the skipped link for the CLI to
                                // surface (JSON envelope or stderr note).
                                skipped_ambiguous.push(SkippedAmbiguous {
                                    source: source_rel.to_string(),
                                    line: line_num,
                                    target: t.clone(),
                                    candidates: candidates.clone(),
                                });
                                continue;
                            }
                            bare_ambiguous_match = true;
                        }
                        Resolution::Broken => continue,
                    }
                }
            }

            if !bare_ambiguous_match
                && !resolver.matches_target(&span, source_rel, old_rel, old_stem)
            {
                continue;
            }

            // Use LinkWriter to emit the new target in the user's written form.
            if let Some(SpanReplacement {
                byte_offset,
                old_text,
                new_text,
            }) = LinkWriter::rewrite(
                &span,
                line,
                new_rel,
                source_rel,
                PreserveForm::Preserve,
                site_prefix,
            ) {
                replacements.push(Replacement {
                    line: line_num,
                    byte_offset,
                    old_text,
                    new_text,
                });
            }
        }
    }

    (replacements, skipped_ambiguous)
}

/// A static empty index used as a fallback when no case_index is available.
static EMPTY_CASE_INDEX: std::sync::LazyLock<CaseInsensitiveIndex> =
    std::sync::LazyLock::new(CaseInsensitiveIndex::new);

// ---------------------------------------------------------------------------
// Outbound rewrite planning
// ---------------------------------------------------------------------------

/// Walk every body line in `content` (which lives at `old_rel`) and return
/// [`Replacement`]s for links whose targets change when the file moves to
/// `new_rel`.
///
/// NEW-1: This function now also handles wikilink self-links. When the moving
/// file contains a wikilink that resolves to itself (`old_rel`), the link is
/// rewritten to point at the new location (`new_rel`) using `LinkWriter` with
/// form preservation.
///
/// Markdown self-links and outbound relative markdown links that need rebasing
/// (when the directory changes) are handled via the existing logic.
///
/// `dir_changed` indicates whether the move crosses directory boundaries.
/// When `false`, only self-links (pointing at `old_rel` itself) need updating;
/// all other relative links remain valid.
#[allow(clippy::too_many_arguments)]
fn plan_outbound_rewrites(
    content: &str,
    old_rel: &str,
    old_stem: &str,
    new_rel: &str,
    _new_stem: &str,
    dir_changed: bool,
    site_prefix: Option<&str>,
    case_index: &CaseInsensitiveIndex,
) -> Vec<Replacement> {
    let link_resolver = LinkResolver::new(case_index, site_prefix);
    let mut replacements = Vec::new();
    let mut fence = FenceTracker::new();
    let mut in_comment_fence = false;
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    let mut line_num = 0usize;

    for line in content.split('\n') {
        line_num += 1;

        // ---- Frontmatter handling (trim() matches scanner behaviour) ----
        if !frontmatter_done {
            if line_num == 1 && line.trim() == "---" {
                in_frontmatter = true;
                continue;
            }
            if in_frontmatter {
                if line.trim() == "---" {
                    in_frontmatter = false;
                    frontmatter_done = true;
                }
                continue;
            }
            frontmatter_done = true;
        }

        // ---- Comment fence (Obsidian %% blocks) ----
        // When already inside a comment, only look for the closing `%%`.
        // Code fences are literal text inside comments, so don't process them.
        if in_comment_fence {
            if is_comment_fence(line) {
                in_comment_fence = false;
            }
            continue;
        }

        // ---- Fenced code block ----
        if fence.process_line(line) {
            continue;
        }

        // ---- Comment fence opening (only outside code blocks) ----
        // A `%%` inside a fenced code block is literal text, not a delimiter.
        if is_comment_fence(line) {
            in_comment_fence = true;
            continue;
        }

        // ---- Extract all link spans ----
        let stripped_code = strip_inline_code(line);
        let cleaned = strip_inline_comments(stripped_code.as_ref());
        let spans = extract_link_spans_with_original(&cleaned, line);

        for span in spans {
            // NEW-1: Handle wikilink self-links.
            // Wikilinks are vault-relative so they don't need rebasing when
            // the file moves — EXCEPT when the link points back to the same
            // file (self-link). In that case we rewrite it via LinkWriter so
            // the form is preserved and the target points to new_rel.
            if span.kind == LinkKind::Wikilink {
                if link_resolver.matches_target(&span, old_rel, old_rel, old_stem)
                    && let Some(SpanReplacement {
                        byte_offset,
                        old_text,
                        new_text,
                    }) = LinkWriter::rewrite(
                        &span,
                        line,
                        new_rel,
                        new_rel, // source is now the destination file
                        PreserveForm::Preserve,
                        site_prefix,
                    )
                {
                    replacements.push(Replacement {
                        line: line_num,
                        byte_offset,
                        old_text,
                        new_text,
                    });
                }
                continue;
            }

            // Markdown link handling below.
            if span.kind != LinkKind::Markdown {
                continue;
            }

            // Skip targets that aren't vault markdown paths (site-absolute,
            // URL schemes, bare labels). See `should_rewrite_outbound_target`.
            if !should_rewrite_outbound_target(&span.link.target) {
                continue;
            }

            // Strip any trailing `#fragment` for resolution / comparison, then
            // re-attach it to the rewritten path so anchored file links keep
            // their anchor.
            let (target_path, target_fragment) = split_target_fragment(&span.link.target);

            // Resolve target relative to the OLD location's directory.
            let resolved = normalize_target(Path::new(old_rel), target_path);

            // Self-links: the file moves to `new_rel`, so the link should
            // continue to refer to the file at its new location.
            let target_after_move = if resolved == old_rel {
                new_rel.to_string()
            } else {
                // When the directory hasn't changed, relative links to other
                // files in the same directory are still valid — skip them.
                if !dir_changed {
                    continue;
                }
                resolved
            };

            // Compute new relative path from the NEW location, then re-attach
            // any fragment that was stripped above.
            let new_target = format!(
                "{}{}",
                relative_path_between(new_rel, &target_after_move),
                target_fragment
            );

            // Original target as written in the file.
            let original_target = &line[span.target_start..span.target_end];

            if new_target == original_target {
                continue;
            }

            let old_text = line[span.full_start..span.full_end].to_string();
            let new_text = format!(
                "{}{}{}",
                &line[span.full_start..span.target_start],
                new_target,
                &line[span.target_end..span.full_end]
            );

            replacements.push(Replacement {
                line: line_num,
                byte_offset: span.full_start,
                old_text,
                new_text,
            });
        }
    }

    replacements
}

// ---------------------------------------------------------------------------
// Applying replacements to file content
// ---------------------------------------------------------------------------

/// Apply all replacements to `content`, returning the rewritten string.
///
/// Replacements are matched against lines by their `old_text`.  Multiple
/// replacements on the same line are applied right-to-left (by first
/// occurrence of `old_text`) to avoid offset shifts.
pub(crate) fn apply_replacements(content: &str, replacements: &[Replacement]) -> String {
    // Group replacements by 1-based line number.
    let mut by_line: HashMap<usize, Vec<&Replacement>> = HashMap::new();
    for r in replacements {
        by_line.entry(r.line).or_default().push(r);
    }

    // Reconstruct content line by line, preserving exact line endings.
    // We split on '\n' and re-join, tracking whether the original ended with '\n'.
    let ends_with_newline = content.ends_with('\n');

    let lines: Vec<&str> = content.split('\n').collect();
    let mut out = String::with_capacity(content.len());

    for (idx, &raw_line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let is_last = idx + 1 == lines.len();

        let mut line = raw_line.to_string();

        if let Some(repls) = by_line.get(&line_num) {
            // Sort right-to-left by byte offset so that applying one
            // substitution doesn't shift offsets for subsequent ones.
            let mut sorted: Vec<&&Replacement> = repls.iter().collect();
            sorted.sort_by_key(|r| std::cmp::Reverse(r.byte_offset));

            for r in sorted {
                let pos = r.byte_offset;
                let end = pos + r.old_text.len();
                if end <= line.len() && line[pos..end] == *r.old_text {
                    line = format!("{}{}{}", &line[..pos], r.new_text, &line[end..]);
                }
            }
        }

        out.push_str(&line);

        // Re-add '\n' between lines but not after the final segment.
        // split('\n') on "a\nb\n" gives ["a", "b", ""] — the trailing
        // empty segment is handled by the safety net below.
        if !is_last {
            out.push('\n');
        }
    }

    // If original content ended with '\n', ensure it still does.
    if ends_with_newline && !out.ends_with('\n') {
        out.push('\n');
    }

    out
}

// ---------------------------------------------------------------------------
// Batch mv planning
// ---------------------------------------------------------------------------

/// Plan all file rewrites for a batch move: multiple `(old_rel, new_rel)` pairs.
///
/// Builds the link graph **once**, then:
/// 1. Collects all inbound backlinks for every source file, merging by the
///    linking file so each external file gets a single [`RewritePlan`] with
///    combined replacements.  Also rewrites wikilinks found in frontmatter
///    `related`/`depends-on`/`supersedes`/`superseded-by` properties.
/// 2. Rewrites outbound relative markdown links inside each moved file,
///    using the full rename map so a link from moved file A to moved file B
///    resolves to B's **new** location.
///
/// The returned plans must be applied after all physical renames have been
/// completed by the caller.
pub fn plan_batch_mv(
    dir: &Path,
    renames: &[(String, String)],
    site_prefix: Option<&str>,
    allow_ambiguous: bool,
) -> Result<Vec<RewritePlan>> {
    if renames.is_empty() {
        return Ok(vec![]);
    }

    // Build the link graph once.
    let build = LinkGraph::build(dir, site_prefix, None).context("building link graph")?;
    for (path, msg) in &build.warnings {
        eprintln!("warning: skipping {}: {msg}", path.display());
    }
    let graph = build.graph;
    let case_index = build.case_index;

    // Build a full rename map: old_stem/old_rel → new_rel for fast lookups.
    // Stored as (old_rel, old_stem, new_rel, new_stem, old_dir, new_dir).
    let rename_info: Vec<(String, String, String, String, String, String)> = renames
        .iter()
        .map(|(old, new)| {
            let old_stem = old.strip_suffix(".md").unwrap_or(old.as_str()).to_string();
            let new_stem = new.strip_suffix(".md").unwrap_or(new.as_str()).to_string();
            let old_dir = parent_dir(old).to_string();
            let new_dir = parent_dir(new).to_string();
            (
                old.clone(),
                old_stem,
                new.clone(),
                new_stem,
                old_dir,
                new_dir,
            )
        })
        .collect();

    // Map from old_rel to new_rel for outbound rewriting.
    let rename_map: HashMap<String, String> = renames
        .iter()
        .map(|(o, n)| (o.clone(), n.clone()))
        .collect();

    // plans keyed by the FILE THAT WILL BE MODIFIED (its new path if it is
    // itself being moved, its current path otherwise).
    let mut plans: HashMap<PathBuf, RewritePlan> = HashMap::new();

    // --- Step 1: inbound rewrites ---
    // Build a map from source_file → Vec<(old_rel, old_stem, new_rel, new_stem)>
    // to process each source file exactly once.
    let mut source_to_renames: HashMap<String, Vec<(String, String, String, String)>> =
        HashMap::new();

    for (old_rel, old_stem, new_rel, new_stem, _, _) in &rename_info {
        let backlinks = graph.backlinks_case_insensitive(old_rel);
        let old_rel_norm = old_rel.replace('\\', "/");
        for entry in backlinks {
            let source_norm = entry.source.to_string_lossy().replace('\\', "/");
            if source_norm == old_rel_norm {
                continue; // skip self
            }
            source_to_renames
                .entry(source_norm.clone())
                .or_default()
                .push((
                    old_rel.clone(),
                    old_stem.clone(),
                    new_rel.clone(),
                    new_stem.clone(),
                ));
        }
    }

    // Now process each source file once, computing all its inbound replacements.
    for (source_rel, move_pairs) in &source_to_renames {
        let abs_path = dir.join(source_rel);
        let meta = std::fs::metadata(&abs_path)
            .with_context(|| format!("failed to stat {}", abs_path.display()))?;
        let file_size = meta.len();
        if file_size > MAX_FILE_SIZE {
            eprintln!(
                "warning: skipping {} ({} MiB exceeds {} MiB limit)",
                abs_path.display(),
                file_size / (1024 * 1024),
                MAX_FILE_SIZE / (1024 * 1024)
            );
            continue;
        }
        let file_mtime = meta
            .modified()
            .with_context(|| format!("failed to read mtime for {}", abs_path.display()))
            .map(|t| (t, file_size))?;
        let content = std::fs::read_to_string(&abs_path)
            .with_context(|| format!("reading {}", abs_path.display()))?;

        let mut all_replacements = Vec::new();
        for (old_rel, old_stem, new_rel, new_stem) in move_pairs {
            let (repls, skipped) = plan_inbound_rewrites(
                &content,
                source_rel,
                old_rel,
                old_stem,
                new_rel,
                new_stem,
                site_prefix,
                Some(&case_index),
                allow_ambiguous,
            );
            // Batch mode doesn't surface skipped-ambiguous links in a JSON
            // envelope (unlike single-file `plan_mv`) — preserve the prior
            // stderr-only warning behavior here.
            for s in skipped {
                eprintln!(
                    "warning: skipping ambiguous bare wikilink [[{}]] in {}:{} — matches {} \
                     files: {}. Use --allow-ambiguous to rewrite anyway.",
                    s.target,
                    s.source,
                    s.line,
                    s.candidates.len(),
                    s.candidates.join(", ")
                );
            }
            all_replacements.extend(repls);
        }

        // Deduplicate replacements: same line + old_text (multiple passes
        // may generate the same replacement if glob and property match the
        // same file).
        all_replacements.sort_by_key(|r| (r.line, r.byte_offset, r.old_text.clone()));
        all_replacements.dedup_by(|a, b| {
            a.line == b.line && a.byte_offset == b.byte_offset && a.old_text == b.old_text
        });

        if all_replacements.is_empty() {
            continue;
        }

        let rewritten_content = apply_replacements(&content, &all_replacements);

        // Determine the plan key: if this source file is itself being moved,
        // its rewritten content goes to the new path.
        let plan_key = if let Some(new_source) = rename_map.get(source_rel.as_str()) {
            dir.join(new_source)
        } else {
            abs_path.clone()
        };
        let plan_rel = if let Some(new_source) = rename_map.get(source_rel.as_str()) {
            new_source.clone()
        } else {
            source_rel.clone()
        };

        plans.insert(
            plan_key.clone(),
            RewritePlan {
                path: plan_key,
                rel_path: plan_rel,
                replacements: all_replacements,
                rewritten_content,
                mtime: Some(file_mtime),
            },
        );
    }

    // --- Step 2: outbound rewrites for each moved file ---
    for (old_rel, _, new_rel, _, old_dir, new_dir) in &rename_info {
        let dir_changed = old_dir != new_dir;
        let old_abs = dir.join(old_rel);
        let meta = std::fs::metadata(&old_abs)
            .with_context(|| format!("failed to stat {}", old_abs.display()))?;
        let file_size = meta.len();
        if file_size > MAX_FILE_SIZE {
            eprintln!(
                "warning: skipping outbound rewrite for {} ({} MiB exceeds {} MiB limit)",
                old_abs.display(),
                file_size / (1024 * 1024),
                MAX_FILE_SIZE / (1024 * 1024)
            );
            continue;
        }
        let old_file_mtime = meta
            .modified()
            .with_context(|| format!("failed to read mtime for {}", old_abs.display()))
            .map(|t| (t, file_size))?;
        let content = std::fs::read_to_string(&old_abs)
            .with_context(|| format!("reading {}", old_abs.display()))?;

        // Outbound: rewrite relative markdown links using the FULL rename map.
        let outbound_repls =
            plan_outbound_rewrites_batch(&content, old_rel, new_rel, &rename_map, dir_changed);

        if outbound_repls.is_empty() {
            // Even if no outbound rewrites, if the inbound pass already
            // created a plan for the new path (because another file links to
            // this one), no action needed here.
            continue;
        }

        let new_abs = dir.join(new_rel);
        let plan_key = new_abs.clone();

        match plans.entry(plan_key.clone()) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                // Merge outbound into the existing inbound plan.
                let existing = e.get_mut();
                // The existing plan was computed from the original content.
                // Re-read content for merged application.
                let existing_content = std::fs::read_to_string(&old_abs)
                    .with_context(|| format!("reading {}", old_abs.display()))?;
                existing.replacements.extend(outbound_repls);
                existing.rewritten_content =
                    apply_replacements(&existing_content, &existing.replacements);
                existing.path = new_abs;
                existing.rel_path.clone_from(new_rel);
                existing.mtime = Some(old_file_mtime);
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                let rewritten_content = apply_replacements(&content, &outbound_repls);
                e.insert(RewritePlan {
                    path: new_abs,
                    rel_path: new_rel.clone(),
                    replacements: outbound_repls,
                    rewritten_content,
                    mtime: Some(old_file_mtime),
                });
            }
        }
    }

    Ok(plans.into_values().collect())
}

/// A single `[[...]]` occurrence found on a YAML frontmatter line.
///
/// `target` is the raw text between the brackets, alias included (e.g.
/// `path/to/file|My Alias`). Offsets are byte offsets into the original line.
pub(crate) struct FrontmatterWikilinkOccurrence<'a> {
    pub target: &'a str,
    pub full_start: usize,
    pub full_end: usize,
}

/// Find every `[[...]]` occurrence in a single frontmatter YAML line.
///
/// Matches patterns like `  - "[[path/to/file]]"` or `  - [[path/to/file]]`
/// (YAML list items containing Obsidian wikilinks), as well as inline
/// flow-sequence forms like `related: ["[[a]]", "[[b]]"]`. This is a raw
/// bracket scan (not YAML-aware) shared by `mv`'s frontmatter rewriter and
/// `links fix`'s frontmatter replacement builder so both stay symmetric.
pub(crate) fn find_frontmatter_wikilinks(line: &str) -> Vec<FrontmatterWikilinkOccurrence<'_>> {
    let mut occurrences = Vec::new();
    let mut search = line;
    let mut base_offset = 0usize;
    while let Some(open) = search.find("[[") {
        let after_open = open + 2;
        if after_open >= search.len() {
            break;
        }
        if let Some(close) = search[after_open..].find("]]") {
            let target_start = after_open;
            let target_end = after_open + close;
            let full_start = open;
            let full_end = target_end + 2;

            occurrences.push(FrontmatterWikilinkOccurrence {
                target: &search[target_start..target_end],
                full_start: base_offset + full_start,
                full_end: base_offset + full_end,
            });

            let advance = full_end;
            base_offset += advance;
            search = &search[advance..];
        } else {
            break;
        }
    }
    occurrences
}

/// Find and plan wikilink replacements inside a single frontmatter YAML line.
///
/// Matches patterns like `  - "[[path/to/file]]"` or `  - [[path/to/file]]`
/// (YAML list items containing Obsidian wikilinks).  Only replaces when the
/// target normalizes to `old_rel` or `old_stem`.
///
/// Uses the same form-preserving approach as body link rewrites: path-form
/// wikilinks in frontmatter stay path-form after the rename (BUG-1 fix).
fn plan_frontmatter_wikilink_rewrites(
    line: &str,
    line_num: usize,
    old_rel: &str,
    old_stem: &str,
    new_rel: &str,
    _new_stem: &str,
    case_index: Option<&CaseInsensitiveIndex>,
) -> Vec<Replacement> {
    let new_stem = new_rel.strip_suffix(".md").unwrap_or(new_rel);
    let new_basename_stem = new_stem.rsplit('/').next().unwrap_or(new_stem);

    let mut replacements = Vec::new();

    for occ in find_frontmatter_wikilinks(line) {
        let target = occ.target;
        // Strip alias suffix (e.g. `path|alias` → `path`)
        let target_path = target.split('|').next().unwrap_or(target).trim();

        let matches = if target_path == old_stem || target_path == old_rel {
            true
        } else if let Some(idx) = case_index {
            let t_norm = target_path.replace('\\', "/").to_ascii_lowercase();
            let canonical = idx.lookup_unique(&t_norm).or_else(|| {
                let with_md = format!("{t_norm}.md");
                idx.lookup_unique(&with_md)
            });
            canonical == Some(old_rel) || canonical == Some(old_stem)
        } else {
            false
        };

        if matches {
            let old_text = format!("[[{target}]]");
            // Detect written form of the target path (before alias).
            // Preserve alias if present (e.g. `path|My Alias` → `new_target|My Alias`).
            let alias_suffix = target.find('|').map_or("", |i| &target[i..]);
            // Preserve the user's written form: path-form → path-form, bare → bare.
            let new_wikilink_target = if target_path.contains('/') || target_path.contains('\\') {
                // Path-form wikilink: preserve path-form with new stem (BUG-1 fix).
                new_stem.to_string()
            } else {
                // Bare wikilink: preserve bare form (just the basename).
                new_basename_stem.to_string()
            };
            let new_text = format!("[[{new_wikilink_target}{alias_suffix}]]");
            if old_text != new_text {
                replacements.push(Replacement {
                    line: line_num,
                    byte_offset: occ.full_start,
                    old_text,
                    new_text,
                });
            }
        }
    }

    replacements
}

/// Outbound rewrite for batch mode: like `plan_outbound_rewrites` but uses the
/// full rename map so that a link from moved file A to moved file B is rewritten
/// to B's new path (not left dangling at B's old path).
fn plan_outbound_rewrites_batch(
    content: &str,
    old_rel: &str,
    new_rel: &str,
    rename_map: &HashMap<String, String>,
    dir_changed: bool,
) -> Vec<Replacement> {
    let mut replacements = Vec::new();
    let mut fence = FenceTracker::new();
    let mut in_comment_fence = false;
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    let mut line_num = 0usize;

    for line in content.split('\n') {
        line_num += 1;

        // Frontmatter
        if !frontmatter_done {
            if line_num == 1 && line.trim() == "---" {
                in_frontmatter = true;
                continue;
            }
            if in_frontmatter {
                if line.trim() == "---" {
                    in_frontmatter = false;
                    frontmatter_done = true;
                }
                continue;
            }
            frontmatter_done = true;
        }

        if in_comment_fence {
            if is_comment_fence(line) {
                in_comment_fence = false;
            }
            continue;
        }
        if fence.process_line(line) {
            continue;
        }
        if is_comment_fence(line) {
            in_comment_fence = true;
            continue;
        }

        let stripped_code = strip_inline_code(line);
        let cleaned = strip_inline_comments(stripped_code.as_ref());
        let spans = extract_link_spans_with_original(&cleaned, line);

        for span in spans {
            if span.kind != LinkKind::Markdown {
                continue;
            }
            if !should_rewrite_outbound_target(&span.link.target) {
                continue;
            }

            let (target_path, target_fragment) = split_target_fragment(&span.link.target);
            let resolved = normalize_target(Path::new(old_rel), target_path);

            // Check if the target is itself being moved.
            let target_after_move = if let Some(new_target_rel) = rename_map.get(&resolved) {
                new_target_rel.clone()
            } else if resolved == old_rel {
                new_rel.to_string()
            } else if !dir_changed {
                continue;
            } else {
                resolved
            };

            let new_target = format!(
                "{}{}",
                relative_path_between(new_rel, &target_after_move),
                target_fragment
            );

            let original_target = &line[span.target_start..span.target_end];
            if new_target == original_target {
                continue;
            }

            let old_text = line[span.full_start..span.full_end].to_string();
            let new_text = format!(
                "{}{}{}",
                &line[span.full_start..span.target_start],
                new_target,
                &line[span.target_end..span.full_end]
            );

            replacements.push(Replacement {
                line: line_num,
                byte_offset: span.full_start,
                old_text,
                new_text,
            });
        }
    }

    replacements
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return the directory part of a vault-relative path (e.g. `"sub/dir"` for
/// `"sub/dir/file.md"`).  Returns an empty string for root-level files.
fn parent_dir(rel: &str) -> &str {
    match rel.rfind('/') {
        Some(pos) => &rel[..pos],
        None => "",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_vault(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, content).unwrap();
        }
        dir
    }

    #[test]
    fn plan_mv_bare_wikilink_stays_short_form_when_stem_unique() {
        // When the moved file's stem is unique vault-wide, bare wikilinks
        // already use short-form and do not need rewriting — they continue
        // to resolve correctly at the new location.
        let vault = create_vault(&[
            ("a.md", "---\ntitle: A\n---\nSee [[b]] for details\n"),
            ("b.md", "---\ntitle: B\n---\nContent\n"),
        ]);
        let plans = plan_mv(vault.path(), "b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        // [[b]] is already correct short-form — no replacement needed.
        let a_plan = plans.iter().find(|p| p.rel_path == "a.md");
        assert!(
            a_plan.is_none(),
            "short-form [[b]] needs no rewrite when stem is unique: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_path_wikilink_rewritten_to_short_form_when_stem_unique() {
        // A path-form wikilink [[sub/b]] should be rewritten to the new path
        // [[archive/b]] — written form (path-relative) is preserved.
        let vault = create_vault(&[
            ("a.md", "---\ntitle: A\n---\nSee [[sub/b]] for details\n"),
            ("sub/b.md", "---\ntitle: B\n---\nContent\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(
            plans.len(),
            1,
            "path wikilink [[sub/b]] should be rewritten"
        );
        assert_eq!(plans[0].replacements[0].old_text, "[[sub/b]]");
        assert_eq!(plans[0].replacements[0].new_text, "[[archive/b]]");
    }

    #[test]
    fn plan_mv_bare_wikilink_ambiguous_not_rewritten() {
        // When multiple files share the same stem, the wikilink is ambiguous
        // and must not be rewritten (it could point at either file).
        let vault = create_vault(&[
            ("a.md", "See [[b]] here\n"),
            ("b.md", "Content root\n"),
            ("sub/b.md", "Content sub\n"),
        ]);
        let plans = plan_mv(vault.path(), "b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        // stem "b" is ambiguous (two files: b.md and sub/b.md) — no rewrite.
        let a_plan = plans.iter().find(|p| p.rel_path == "a.md");
        assert!(
            a_plan.is_none(),
            "ambiguous bare wikilink [[b]] should not be rewritten: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_bare_wikilink_with_alias_stays_short_form_when_unique() {
        // [[b|my note]] with unique stem: already short-form, no rewrite needed.
        let vault = create_vault(&[("a.md", "See [[b|my note]] here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None, false)
            .unwrap()
            .plans;
        // Already short-form and stem is unique — no replacement.
        let a_plan = plans.iter().find(|p| p.rel_path == "a.md");
        assert!(
            a_plan.is_none(),
            "short-form alias wikilink needs no rewrite when stem is unique: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_path_wikilink_with_alias_rewritten_to_short_form() {
        // [[sub/b|my note]] should become [[archive/b|my note]] — path-form preserved.
        let vault = create_vault(&[
            ("a.md", "See [[sub/b|my note]] here\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[sub/b|my note]]");
        assert_eq!(plans[0].replacements[0].new_text, "[[archive/b|my note]]");
    }

    #[test]
    fn plan_mv_bare_wikilink_with_fragment_stays_short_form_when_unique() {
        // [[b#section]] with unique stem: already short-form, no rewrite needed.
        let vault = create_vault(&[("a.md", "See [[b#section]] here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None, false)
            .unwrap()
            .plans;
        let a_plan = plans.iter().find(|p| p.rel_path == "a.md");
        assert!(
            a_plan.is_none(),
            "short-form fragment wikilink needs no rewrite when stem is unique: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_path_wikilink_with_fragment_rewritten_to_short_form() {
        // [[sub/b#section]] should become [[archive/b#section]] — path-form preserved.
        let vault = create_vault(&[
            ("a.md", "See [[sub/b#section]] here\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[sub/b#section]]");
        assert_eq!(plans[0].replacements[0].new_text, "[[archive/b#section]]");
    }

    #[test]
    fn plan_mv_bare_wikilink_case_mismatch_rewritten_to_short_form() {
        // [[B]] matches b.md case-insensitively; stem is unique → short-form.
        let vault = create_vault(&[("a.md", "See [[B]] here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(
            plans.len(),
            1,
            "case-mismatched bare wikilink should be rewritten"
        );
        assert_eq!(plans[0].replacements[0].old_text, "[[B]]");
        // stem "b" is unique → short-form "b"
        assert_eq!(plans[0].replacements[0].new_text, "[[b]]");
    }

    #[test]
    fn plan_mv_bare_wikilink_unrelated_not_rewritten() {
        // [[c]] and [[bb]] must not be rewritten when b.md is moved.
        let vault = create_vault(&[
            ("a.md", "See [[c]] and [[bb]] here\n"),
            ("b.md", "B\n"),
            ("c.md", "C\n"),
            ("bb.md", "BB\n"),
        ]);
        let plans = plan_mv(vault.path(), "b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        // a.md has no links to b.md — no plan for it.
        let a_plan = plans.iter().find(|p| p.rel_path == "a.md");
        assert!(
            a_plan.is_none(),
            "unrelated wikilinks must not be rewritten: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_inbound_wikilink_with_path() {
        // Wikilinks that already contain a path ARE rewritten — path-form preserved.
        let vault = create_vault(&[
            (
                "a.md",
                "---\ntitle: A\n---\nSee [[backlog/item]] for details\n",
            ),
            ("backlog/item.md", "---\ntitle: Item\n---\nContent\n"),
        ]);
        let plans = plan_mv(
            vault.path(),
            "backlog/item.md",
            "backlog/done/item.md",
            None,
            false,
        )
        .unwrap()
        .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].rel_path, "a.md");
        assert_eq!(plans[0].replacements.len(), 1);
        // Path-form preserved: [[backlog/item]] → [[backlog/done/item]]
        assert_eq!(plans[0].replacements[0].old_text, "[[backlog/item]]");
        assert_eq!(plans[0].replacements[0].new_text, "[[backlog/done/item]]");
    }

    #[test]
    fn plan_mv_inbound_wikilink_with_path_and_alias() {
        let vault = create_vault(&[
            ("a.md", "See [[sub/b|my note]] here\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[sub/b|my note]]");
        // Path-form preserved, alias preserved
        assert_eq!(plans[0].replacements[0].new_text, "[[archive/b|my note]]");
    }

    #[test]
    fn plan_mv_inbound_wikilink_with_path_and_fragment() {
        let vault = create_vault(&[
            ("a.md", "See [[sub/b#section]] here\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[sub/b#section]]");
        // Path-form preserved, fragment preserved
        assert_eq!(plans[0].replacements[0].new_text, "[[archive/b#section]]");
    }

    #[test]
    fn plan_mv_inbound_markdown_link() {
        let vault = create_vault(&[("a.md", "See [note](b.md) here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[note](b.md)");
        assert_eq!(plans[0].replacements[0].new_text, "[note](sub/b.md)");
    }

    #[test]
    fn plan_mv_outbound_relative_link() {
        // b.md in root links to a.md. Moving b.md to sub/b.md means the
        // relative path changes.
        let vault = create_vault(&[("a.md", "Content A\n"), ("b.md", "See [note](a.md) here\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None, false)
            .unwrap()
            .plans;
        let moved_plan = plans.iter().find(|p| p.rel_path == "sub/b.md").unwrap();
        assert_eq!(moved_plan.replacements[0].old_text, "[note](a.md)");
        assert_eq!(moved_plan.replacements[0].new_text, "[note](../a.md)");
    }

    #[test]
    fn plan_mv_outbound_wikilink_unchanged() {
        // Wikilinks are vault-relative, so moving the file doesn't change them.
        let vault = create_vault(&[("a.md", "Content A\n"), ("b.md", "See [[a]] here\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None, false)
            .unwrap()
            .plans;
        // b.md should NOT appear in plans (no outbound changes needed for wikilinks).
        let moved_plan = plans.iter().find(|p| p.rel_path == "b.md");
        assert!(moved_plan.is_none());
    }

    #[test]
    fn plan_mv_links_in_code_block_untouched() {
        let vault = create_vault(&[
            (
                "a.md",
                "---\ntitle: A\n---\n```\n[[sub/b]]\n```\nReal [[sub/b]]\n",
            ),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        // Only the real link outside code block should be rewritten.
        assert_eq!(plans[0].replacements.len(), 1);
        assert_eq!(plans[0].replacements[0].line, 7);
    }

    #[test]
    fn plan_mv_links_in_inline_code_untouched() {
        let vault = create_vault(&[
            ("a.md", "Use `[[sub/b]]` and real [[sub/b]]\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[sub/b]]");
    }

    #[test]
    fn plan_mv_no_links_empty_result() {
        let vault = create_vault(&[("a.md", "No links here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None, false)
            .unwrap()
            .plans;
        assert!(plans.is_empty());
    }

    #[test]
    fn plan_mv_multiple_links_one_line() {
        let vault = create_vault(&[
            ("a.md", "See [[sub/b]] and [[sub/b|alias]]\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements.len(), 2);
    }

    #[test]
    fn execute_plans_writes_files() {
        let vault = create_vault(&[("a.md", "See [[sub/b]] here\n"), ("sub/b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        execute_plans(vault.path(), &plans).unwrap();
        let content = fs::read_to_string(vault.path().join("a.md")).unwrap();
        // Path-form preserved: [[sub/b]] → [[archive/b]]
        assert!(content.contains("[[archive/b]]"), "content: {content}");
        assert!(!content.contains("[[sub/b]]"));
    }

    // ---- Additional edge-case tests ----

    #[test]
    fn apply_replacements_preserves_trailing_newline() {
        let content = "line one\nline two\n";
        let repls = vec![Replacement {
            line: 1,
            byte_offset: 5,
            old_text: "one".to_string(),
            new_text: "ONE".to_string(),
        }];
        let result = apply_replacements(content, &repls);
        assert_eq!(result, "line ONE\nline two\n");
    }

    #[test]
    fn apply_replacements_no_trailing_newline() {
        let content = "line one\nline two";
        let repls = vec![Replacement {
            line: 2,
            byte_offset: 5,
            old_text: "two".to_string(),
            new_text: "TWO".to_string(),
        }];
        let result = apply_replacements(content, &repls);
        assert_eq!(result, "line one\nline TWO");
    }

    #[test]
    fn plan_mv_frontmatter_links_rewritten() {
        // H-4: single-file mv must rewrite wikilinks inside frontmatter link
        // properties (e.g. `related`), not just in the document body.
        let vault = create_vault(&[
            ("a.md", "---\nrelated: \"[[sub/b]]\"\n---\nBody [[sub/b]]\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements.len(), 2);
        let lines: Vec<usize> = plans[0].replacements.iter().map(|r| r.line).collect();
        assert!(
            lines.contains(&2),
            "frontmatter line not rewritten: {plans:?}"
        );
        assert!(lines.contains(&4), "body line not rewritten: {plans:?}");
    }

    #[test]
    fn plan_mv_same_directory_no_outbound_changes() {
        // Moving within the same directory: outbound relative links don't change.
        let vault = create_vault(&[("a.md", "Content A\n"), ("b.md", "See [note](a.md) here\n")]);
        // Both old and new are in the root → no outbound rewrite needed.
        let plans = plan_mv(vault.path(), "b.md", "c.md", None, false)
            .unwrap()
            .plans;
        let moved_plan = plans.iter().find(|p| p.rel_path == "b.md");
        assert!(moved_plan.is_none());
    }

    #[test]
    fn plan_mv_inbound_markdown_link_from_subdir() {
        // A file in a subdirectory links to the moved file using a relative path.
        let vault = create_vault(&[
            ("sub/a.md", "See [note](../b.md) here\n"),
            ("b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].rel_path, "sub/a.md");
        assert_eq!(plans[0].replacements[0].old_text, "[note](../b.md)");
        // From sub/a.md to archive/b.md: ../archive/b.md
        assert_eq!(plans[0].replacements[0].new_text, "[note](../archive/b.md)");
    }

    #[test]
    fn plan_mv_bare_markdown_link_from_subdir_not_false_positive() {
        // sub/a.md has [note](b.md) which resolves to sub/b.md, NOT root b.md.
        // Moving root b.md must NOT rewrite this link.
        let vault = create_vault(&[
            ("sub/a.md", "See [note](b.md) here\n"),
            ("sub/b.md", "Content sub\n"),
            ("b.md", "Content root\n"),
        ]);
        let plans = plan_mv(vault.path(), "b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        // sub/a.md links to sub/b.md, not root b.md — should NOT be rewritten.
        let sub_plan = plans.iter().find(|p| p.rel_path == "sub/a.md");
        assert!(sub_plan.is_none(), "false positive: {plans:?}");
    }

    #[test]
    fn plan_mv_bare_wikilink_with_md_extension_rewritten() {
        // [[b.md]] — MdSuffixed form, bare stem. After moving b.md → sub/b.md,
        // the MdSuffixed form is preserved: new target is [[b.md]] (same stem).
        // Since the replacement equals the original, no rewrite is emitted.
        let vault = create_vault(&[("a.md", "See [[b.md]] here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None, false)
            .unwrap()
            .plans;
        // [[b.md]] resolves correctly at the new location — no rewrite needed.
        let a_plan = plans.iter().find(|p| p.rel_path == "a.md");
        assert!(
            a_plan.is_none(),
            "[[b.md]] MdSuffixed should not need a rewrite when basename unchanged: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_wikilink_with_path_and_md_extension() {
        // [[sub/b.md]] has a path separator → PathRelative form.
        // Path-form is preserved: [[sub/b.md]] → [[archive/b]] (stem without .md).
        let vault = create_vault(&[
            ("a.md", "See [[sub/b.md]] here\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[sub/b.md]]");
        // PathRelative form preserved, emits stem without .md suffix
        assert_eq!(plans[0].replacements[0].new_text, "[[archive/b]]");
    }

    // ---- Absolute-path inbound link tests ----

    #[test]
    fn plan_mv_inbound_absolute_link_with_site_prefix() {
        // a.md uses an absolute link /docs/configure/settings.md to reference
        // configure/settings.md (with site_prefix = "docs").
        // After moving configure/settings.md → config/settings.md, the link
        // must be rewritten to /docs/config/settings.md (preserving absolute style).
        let vault = create_vault(&[
            (
                "a.md",
                "See [settings](/docs/configure/settings.md) for details\n",
            ),
            ("configure/settings.md", "# Settings\n"),
        ]);
        let plans = plan_mv(
            vault.path(),
            "configure/settings.md",
            "config/settings.md",
            Some("docs"),
            false,
        )
        .unwrap()
        .plans;
        assert_eq!(plans.len(), 1, "should produce one rewrite plan");
        assert_eq!(plans[0].rel_path, "a.md");
        assert_eq!(plans[0].replacements.len(), 1);
        assert_eq!(
            plans[0].replacements[0].old_text,
            "[settings](/docs/configure/settings.md)"
        );
        assert_eq!(
            plans[0].replacements[0].new_text,
            "[settings](/docs/config/settings.md)"
        );
    }

    #[test]
    fn plan_mv_inbound_absolute_link_without_site_prefix() {
        // With no site_prefix, /page.md is an absolute link to page.md.
        // Moving page.md → archive/page.md must rewrite to /archive/page.md.
        let vault = create_vault(&[
            ("index.md", "See [page](/page.md) here\n"),
            ("page.md", "# Page\n"),
        ]);
        let plans = plan_mv(vault.path(), "page.md", "archive/page.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].rel_path, "index.md");
        assert_eq!(plans[0].replacements[0].old_text, "[page](/page.md)");
        assert_eq!(
            plans[0].replacements[0].new_text,
            "[page](/archive/page.md)"
        );
    }

    #[test]
    fn plan_mv_inbound_absolute_link_no_false_positive_wrong_prefix() {
        // /other/page.md does NOT match configure/settings.md even with
        // site_prefix = "docs" — must not produce a spurious rewrite.
        let vault = create_vault(&[
            ("a.md", "See [other](/other/page.md) here\n"),
            ("configure/settings.md", "# Settings\n"),
            ("other/page.md", "# Other\n"),
        ]);
        let plans = plan_mv(
            vault.path(),
            "configure/settings.md",
            "config/settings.md",
            Some("docs"),
            false,
        )
        .unwrap()
        .plans;
        let a_plan = plans.iter().find(|p| p.rel_path == "a.md");
        assert!(
            a_plan.is_none(),
            "absolute link to a different file must not be rewritten: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_inbound_absolute_stem_match() {
        // Absolute link without .md extension: /docs/configure/settings
        // should still match configure/settings.md (stem comparison).
        let vault = create_vault(&[
            (
                "a.md",
                "See [settings](/docs/configure/settings) for details\n",
            ),
            ("configure/settings.md", "# Settings\n"),
        ]);
        let plans = plan_mv(
            vault.path(),
            "configure/settings.md",
            "config/settings.md",
            Some("docs"),
            false,
        )
        .unwrap()
        .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].replacements[0].old_text,
            "[settings](/docs/configure/settings)"
        );
        assert_eq!(
            plans[0].replacements[0].new_text,
            "[settings](/docs/config/settings)"
        );
    }

    // ---- Self-link tests (NEW-BUG-2) ----

    #[test]
    fn plan_mv_self_link_same_directory() {
        // `self.md` contains a markdown link to itself. Moving it to `other.md`
        // in the same directory must (1) succeed, (2) produce a plan whose path
        // points to the NEW location, and (3) rewrite the self-link.
        let vault = create_vault(&[("self.md", "A link to [me](self.md).\n")]);
        let plans = plan_mv(vault.path(), "self.md", "other.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        let plan = &plans[0];
        assert_eq!(plan.rel_path, "other.md");
        assert_eq!(plan.path, vault.path().join("other.md"));
        assert_eq!(plan.replacements.len(), 1);
        assert_eq!(plan.replacements[0].old_text, "[me](self.md)");
        assert_eq!(plan.replacements[0].new_text, "[me](other.md)");
    }

    #[test]
    fn execute_plans_self_link_same_directory_e2e() {
        // Full mv flow: rename + rewrite without canonicalization error.
        let vault = create_vault(&[("self.md", "A link to [me](self.md).\n")]);
        let plans = plan_mv(vault.path(), "self.md", "other.md", None, false)
            .unwrap()
            .plans;
        fs::rename(vault.path().join("self.md"), vault.path().join("other.md")).unwrap();
        execute_plans(vault.path(), &plans).unwrap();
        let content = fs::read_to_string(vault.path().join("other.md")).unwrap();
        assert!(content.contains("[me](other.md)"), "got: {content:?}");
        assert!(!content.contains("self.md"));
    }

    // ---- Outbound skip-rule tests (BUG-6) ----

    #[test]
    fn plan_mv_outbound_skips_site_absolute_links() {
        // Site-absolute links (/…) must NOT be rewritten when the moved file's
        // directory changes.
        let vault = create_vault(&[
            (
                "games/anatomy/index.md",
                "See [MDN](/en-US/docs/Web/API/Web_Workers).\n",
            ),
            ("games/anatomy-renamed/.gitkeep", ""),
        ]);
        let plans = plan_mv(
            vault.path(),
            "games/anatomy/index.md",
            "games/anatomy-renamed/index.md",
            None,
            false,
        )
        .unwrap()
        .plans;
        let moved_plan = plans
            .iter()
            .find(|p| p.rel_path == "games/anatomy-renamed/index.md");
        assert!(
            moved_plan.is_none(),
            "site-absolute link must not trigger a rewrite: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_outbound_skips_url_schemes() {
        let vault = create_vault(&[(
            "sub/note.md",
            "See [link](https://example.com/x) and [mail](mailto:a@b.c).\n",
        )]);
        let plans = plan_mv(vault.path(), "sub/note.md", "archive/note.md", None, false)
            .unwrap()
            .plans;
        let moved_plan = plans.iter().find(|p| p.rel_path == "archive/note.md");
        assert!(
            moved_plan.is_none(),
            "URL-scheme links must not be rewritten: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_outbound_skips_fragment_only() {
        let vault = create_vault(&[("sub/note.md", "Jump to [top](#top).\n")]);
        let plans = plan_mv(vault.path(), "sub/note.md", "archive/note.md", None, false)
            .unwrap()
            .plans;
        let moved_plan = plans.iter().find(|p| p.rel_path == "archive/note.md");
        assert!(
            moved_plan.is_none(),
            "fragment-only links must not be rewritten: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_outbound_skips_bare_token_without_md_extension() {
        // `[obsidian](Note One)` — no `.md`, no path separator. Must be left
        // alone (treated as a label / Obsidian-style reference).
        let vault = create_vault(&[("sub/note.md", "See [obsidian](Note One) here.\n")]);
        let plans = plan_mv(vault.path(), "sub/note.md", "archive/note.md", None, false)
            .unwrap()
            .plans;
        let moved_plan = plans.iter().find(|p| p.rel_path == "archive/note.md");
        assert!(
            moved_plan.is_none(),
            "bare non-md link must not be rewritten: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_outbound_rewrites_genuine_relative_link() {
        // Regression: genuine relative links that would resolve differently
        // after the move are rebased. Here `sub/a.md` moves to `a.md` at the
        // root; the link `peer.md` (previously sub/peer.md) must be rewritten
        // to `sub/peer.md` from the new location.
        let vault = create_vault(&[
            ("sub/a.md", "See [peer](peer.md).\n"),
            ("sub/peer.md", "peer\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/a.md", "a.md", None, false)
            .unwrap()
            .plans;
        let moved_plan = plans
            .iter()
            .find(|p| p.rel_path == "a.md")
            .expect("expected rewrite plan");
        assert_eq!(moved_plan.replacements.len(), 1);
        assert_eq!(moved_plan.replacements[0].old_text, "[peer](peer.md)");
        assert_eq!(moved_plan.replacements[0].new_text, "[peer](sub/peer.md)");
    }

    #[test]
    fn should_rewrite_outbound_target_rules() {
        assert!(!should_rewrite_outbound_target(""));
        assert!(!should_rewrite_outbound_target("/en-US/docs/x"));
        assert!(!should_rewrite_outbound_target("/page.md"));
        assert!(!should_rewrite_outbound_target("#anchor"));
        assert!(!should_rewrite_outbound_target("https://a.b/c"));
        assert!(!should_rewrite_outbound_target("mailto:a@b.c"));
        assert!(!should_rewrite_outbound_target("tel:+1"));
        assert!(!should_rewrite_outbound_target("Note One"));
        assert!(!should_rewrite_outbound_target("plain-label"));
        // Rewritable:
        assert!(should_rewrite_outbound_target("../notes/x.md"));
        assert!(should_rewrite_outbound_target("sub/x.md"));
        assert!(should_rewrite_outbound_target("x.md"));
        assert!(should_rewrite_outbound_target("sub/label"));
        // Fragments: classify by the path portion, not the whole target.
        assert!(
            should_rewrite_outbound_target("x.md#intro"),
            "anchored .md link should be rewritable"
        );
        assert!(
            should_rewrite_outbound_target("sub/x.md#section-1"),
            "anchored nested .md link should be rewritable"
        );
        assert!(
            !should_rewrite_outbound_target("#anchor-with-dashes"),
            "fragment-only target must still be skipped"
        );
        // Windows drive-letter paths are filesystem paths, not URL schemes.
        assert!(
            should_rewrite_outbound_target("C:/notes/x.md"),
            "Windows drive-letter forward-slash path should be rewritable"
        );
        assert!(
            should_rewrite_outbound_target("C:\\notes\\x.md"),
            "Windows drive-letter backslash path should be rewritable"
        );
    }

    #[test]
    fn plan_mv_outbound_rewrites_anchored_self_link() {
        // NEW-BUG-2 follow-up: self-links with fragments must still be rewritten
        // to point at the file's new location while preserving the anchor.
        let vault = create_vault(&[("self.md", "Jump to [intro](self.md#intro) in this file.\n")]);
        let plans = plan_mv(vault.path(), "self.md", "other.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        let plan = &plans[0];
        assert_eq!(plan.rel_path, "other.md");
        assert_eq!(plan.replacements.len(), 1);
        assert_eq!(plan.replacements[0].old_text, "[intro](self.md#intro)");
        assert_eq!(plan.replacements[0].new_text, "[intro](other.md#intro)");
    }

    #[test]
    fn plan_mv_outbound_rewrites_anchored_relative_link_on_dir_change() {
        // Anchored relative links must be rebased when the directory changes.
        // `sub/a.md` moves to `a.md`; the link `peer.md#heading` should become
        // `sub/peer.md#heading` from the new location.
        let vault = create_vault(&[
            ("sub/a.md", "See [peer](peer.md#heading).\n"),
            ("sub/peer.md", "peer\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/a.md", "a.md", None, false)
            .unwrap()
            .plans;
        let moved_plan = plans
            .iter()
            .find(|p| p.rel_path == "a.md")
            .expect("expected rewrite plan");
        assert_eq!(moved_plan.replacements.len(), 1);
        assert_eq!(
            moved_plan.replacements[0].old_text,
            "[peer](peer.md#heading)"
        );
        assert_eq!(
            moved_plan.replacements[0].new_text,
            "[peer](sub/peer.md#heading)"
        );
    }

    #[test]
    fn execute_plans_rejects_path_outside_vault() {
        // Construct a RewritePlan whose absolute path points outside the vault.
        // execute_plans must refuse to write it.
        let vault = create_vault(&[("a.md", "content\n")]);
        let outside = tempfile::tempdir().unwrap();
        let outside_path = outside.path().join("escaped.md");
        fs::write(&outside_path, "original\n").unwrap();

        let bad_plan = RewritePlan {
            path: outside_path.clone(),
            rel_path: "escaped.md".to_string(),
            replacements: vec![],
            rewritten_content: "malicious\n".to_string(),
            mtime: None,
        };

        let result = execute_plans(vault.path(), &[bad_plan]);
        assert!(result.is_err(), "must refuse to write outside vault");

        // Original file must be untouched.
        let content = fs::read_to_string(&outside_path).unwrap();
        assert_eq!(content, "original\n");
    }

    // ---------------------------------------------------------------------------
    // Case-insensitive inbound rewrite tests (BUG-6)
    // ---------------------------------------------------------------------------

    #[test]
    fn plan_mv_case_insensitive_wikilink_inbound() {
        // MDN-shape: file on disk `web/javascript/reference/iteration_protocols/index.md`.
        // Another file `promise/any/index.md` links to `Web/JavaScript/Reference/Iteration_protocols/index`
        // (PascalCase as commonly used in MDN-mirror vaults).
        // Moving the lowercase file should detect the PascalCase link as inbound.
        let vault = create_vault(&[
            (
                "web/javascript/reference/iteration_protocols/index.md",
                "# Iteration Protocols\n",
            ),
            (
                "promise/any/index.md",
                "See [[Web/JavaScript/Reference/Iteration_protocols/index]]\n",
            ),
        ]);
        let plans = plan_mv(
            vault.path(),
            "web/javascript/reference/iteration_protocols/index.md",
            "web/javascript/reference/iteration_protocols_v2/index.md",
            None,
            false,
        )
        .unwrap()
        .plans;

        // promise/any/index.md should have been detected as an inbound link source.
        let promise_plan = plans.iter().find(|p| p.rel_path == "promise/any/index.md");
        assert!(
            promise_plan.is_some(),
            "case-insensitive inbound link in promise/any/index.md should produce a rewrite plan; got plans: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_case_insensitive_markdown_link_inbound() {
        // Same BUG-6 scenario but with a markdown-style link.
        let vault = create_vault(&[
            ("web/foo.md", "Content\n"),
            ("other.md", "See [link](Web/Foo.md)\n"),
        ]);
        let plans = plan_mv(vault.path(), "web/foo.md", "archive/foo.md", None, false)
            .unwrap()
            .plans;

        let other_plan = plans.iter().find(|p| p.rel_path == "other.md");
        assert!(
            other_plan.is_some(),
            "case-insensitive inbound markdown link should produce a rewrite plan; got: {plans:?}"
        );
    }

    // ---------------------------------------------------------------------------
    // Fence-ordering bug: %% inside a fenced code block must not toggle comment mode
    // ---------------------------------------------------------------------------

    #[test]
    fn plan_mv_percent_percent_inside_code_fence_does_not_toggle_comment_mode() {
        // A `%%` line that appears inside a fenced code block is literal text.
        // It must NOT be treated as an Obsidian comment-fence delimiter, which
        // would put the parser into comment mode and cause it to skip the real
        // [[sub/target]] wikilink that follows the code block.
        let vault = create_vault(&[
            (
                "a.md",
                "---\ntitle: test\n---\n# Test\n\n```markdown\n%%\nThis is inside a code fence\n```\n\nSee [[sub/target]] for more.\n",
            ),
            ("sub/target.md", "Content\n"),
        ]);
        let plans = plan_mv(
            vault.path(),
            "sub/target.md",
            "archive/target.md",
            None,
            false,
        )
        .unwrap()
        .plans;
        // The [[sub/target]] link after the code block must be detected as inbound.
        let a_plan = plans.iter().find(|p| p.rel_path == "a.md");
        assert!(
            a_plan.is_some(),
            "[[sub/target]] after a code fence containing %% should be found; got plans: {plans:?}"
        );
        let a_plan = a_plan.unwrap();
        assert_eq!(
            a_plan.replacements.len(),
            1,
            "exactly one replacement expected; got: {:?}",
            a_plan.replacements
        );
        assert_eq!(a_plan.replacements[0].old_text, "[[sub/target]]");
        // Path-form preserved: [[sub/target]] → [[archive/target]]
        assert_eq!(a_plan.replacements[0].new_text, "[[archive/target]]");
    }

    #[test]
    fn plan_mv_outbound_percent_percent_inside_code_fence_does_not_toggle_comment_mode() {
        // Same ordering bug in plan_outbound_rewrites: a `%%` inside a code
        // fence must not suppress the markdown link that follows the fence.
        let vault = create_vault(&[
            ("sub/a.md", "```markdown\n%%\n```\n\nSee [peer](peer.md).\n"),
            ("sub/peer.md", "peer\n"),
        ]);
        // Moving sub/a.md to root triggers outbound rewrite of [peer](peer.md).
        let plans = plan_mv(vault.path(), "sub/a.md", "a.md", None, false)
            .unwrap()
            .plans;
        let moved_plan = plans
            .iter()
            .find(|p| p.rel_path == "a.md")
            .expect("outbound link after %%-in-code-fence should be detected");
        assert_eq!(moved_plan.replacements.len(), 1);
        assert_eq!(moved_plan.replacements[0].old_text, "[peer](peer.md)");
        assert_eq!(moved_plan.replacements[0].new_text, "[peer](sub/peer.md)");
    }

    // ---------------------------------------------------------------------------
    // Code fence inside a %% comment must not leave comment mode stuck open
    // ---------------------------------------------------------------------------

    #[test]
    fn plan_mv_code_fence_inside_comment_does_not_break_parsing() {
        // A fenced code block that appears inside a `%%` comment block must be
        // treated as literal text.  In particular, the ```` ``` ```` must not
        // flip the code-fence state, which would prevent the closing `%%` from
        // being recognised, leaving the parser stuck in comment mode.
        let vault = create_vault(&[
            (
                "a.md",
                "---\ntitle: test\n---\n# Intro\n\n%%\n```\ncode in comment\n```\n%%\n\nSee [[sub/target]] for more.\n",
            ),
            ("sub/target.md", "Content\n"),
        ]);
        let plans = plan_mv(
            vault.path(),
            "sub/target.md",
            "archive/target.md",
            None,
            false,
        )
        .unwrap()
        .plans;
        let a_plan = plans.iter().find(|p| p.rel_path == "a.md");
        assert!(
            a_plan.is_some(),
            "[[sub/target]] after a comment containing a code fence should be found; got: {plans:?}"
        );
        let a_plan = a_plan.unwrap();
        assert_eq!(a_plan.replacements.len(), 1);
        assert_eq!(a_plan.replacements[0].old_text, "[[sub/target]]");
        // Path-form preserved: [[sub/target]] → [[archive/target]]
        assert_eq!(a_plan.replacements[0].new_text, "[[archive/target]]");
    }

    #[test]
    fn plan_mv_outbound_code_fence_inside_comment_does_not_break_parsing() {
        // Same as above but for the outbound rewrite path.
        let vault = create_vault(&[
            (
                "sub/a.md",
                "%%\n```\ncode\n```\n%%\n\nSee [peer](peer.md).\n",
            ),
            ("sub/peer.md", "peer\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/a.md", "a.md", None, false)
            .unwrap()
            .plans;
        let moved_plan = plans
            .iter()
            .find(|p| p.rel_path == "a.md")
            .expect("outbound link after comment-with-code-fence should be detected");
        assert_eq!(moved_plan.replacements.len(), 1);
        assert_eq!(moved_plan.replacements[0].old_text, "[peer](peer.md)");
        assert_eq!(moved_plan.replacements[0].new_text, "[peer](sub/peer.md)");
    }

    // --- BUG-1: `[[./relative]]` wikilink rewriting ---

    #[test]
    fn plan_mv_inbound_wikilink_dot_slash_plain() {
        // `[[./b]]` in a.md (root): after mv b.md → sub/b.md, DotRelative
        // preserves the `./` prefix (iter-151 NEW-2: linker at root → emit `./sub/b`).
        let vault = create_vault(&[("a.md", "See [[./b]] here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].rel_path, "a.md");
        assert_eq!(plans[0].replacements.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[./b]]");
        // DotRelative preserved: linker at root → ./sub/b
        assert_eq!(plans[0].replacements[0].new_text, "[[./sub/b]]");
    }

    #[test]
    fn plan_mv_inbound_wikilink_dot_slash_with_alias() {
        let vault = create_vault(&[
            ("a.md", "See [[./b|my note]] here\n"),
            ("b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[./b|my note]]");
        // DotRelative preserved, alias preserved
        assert_eq!(plans[0].replacements[0].new_text, "[[./sub/b|my note]]");
    }

    #[test]
    fn plan_mv_inbound_wikilink_dot_slash_with_section() {
        let vault = create_vault(&[("a.md", "See [[./b#sec]] here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[./b#sec]]");
        // DotRelative preserved, fragment preserved
        assert_eq!(plans[0].replacements[0].new_text, "[[./sub/b#sec]]");
    }

    #[test]
    fn plan_mv_inbound_wikilink_dot_slash_with_section_and_alias() {
        let vault = create_vault(&[
            ("a.md", "See [[./b#sec|note]] here\n"),
            ("b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None, false)
            .unwrap()
            .plans;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[./b#sec|note]]");
        // DotRelative preserved, fragment+alias preserved
        assert_eq!(plans[0].replacements[0].new_text, "[[./sub/b#sec|note]]");
    }

    #[test]
    fn plan_mv_inbound_wikilink_unrelated_dot_slash_not_rewritten() {
        // `[[./other]]` should NOT be rewritten when moving `b.md`.
        let vault = create_vault(&[
            ("a.md", "See [[./other]] here\n"),
            ("b.md", "Content\n"),
            ("other.md", "Other\n"),
        ]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None, false)
            .unwrap()
            .plans;
        // a.md should not appear in plans (it has no link to b.md).
        let a_plan = plans.iter().find(|p| p.rel_path == "a.md");
        assert!(
            a_plan.is_none(),
            "[[./other]] must not be rewritten for mv b.md"
        );
    }
}
