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
use crate::link_graph::{LinkGraph, normalize_target, relative_path_between, strip_site_prefix};
use crate::links::{LinkKind, extract_link_spans_with_original};
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
pub fn plan_mv(
    dir: &Path,
    old_rel: &str,
    new_rel: &str,
    site_prefix: Option<&str>,
) -> Result<Vec<RewritePlan>> {
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

        let replacements = plan_inbound_rewrites(
            &content,
            &source_rel_str,
            old_rel,
            old_stem,
            new_rel,
            new_stem,
            site_prefix,
            Some(&case_index),
        );

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
    // even when its directory didn't change (NEW-BUG-2). When the directory
    // does change, relative links to other files are also rebased here.
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
        return Ok(plans.into_values().collect());
    }
    let content = std::fs::read_to_string(&old_abs)
        .with_context(|| format!("reading {}", old_abs.display()))?;

    let outbound_replacements = plan_outbound_rewrites(&content, old_rel, new_rel, dir_changed);

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

    Ok(plans.into_values().collect())
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

/// Walk every body line in `content` and return [`Replacement`]s for links
/// that target the moved file.
///
/// When `case_index` is provided, link targets are canonicalized through it
/// before comparison against `old_rel`/`old_stem`.  This fixes BUG-6: a link
/// written as `Web/JavaScript/…` resolves to the on-disk lowercase path and is
/// therefore detected as pointing at the moved file.
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
) -> Vec<Replacement> {
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
            // No frontmatter block found; mark done.
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

        // ---- Extract and compare link spans ----
        let stripped_code = strip_inline_code(line);
        let cleaned = strip_inline_comments(stripped_code.as_ref());
        let spans = extract_link_spans_with_original(&cleaned, line);

        for span in spans {
            let matches = match span.kind {
                LinkKind::Wikilink => {
                    let t = &span.link.target;
                    // Only match wikilinks that already contain a path separator.
                    // Bare name wikilinks (e.g. [[note]]) are left alone — they
                    // don't encode a location and will work once shortest-path
                    // resolution is implemented.
                    if !(t.contains('/') || t.contains('\\')) {
                        false
                    } else if t == old_stem || t == old_rel {
                        true
                    } else if let Some(idx) = case_index {
                        // Canonicalize the written target through the case index.
                        // Normalize path separators to forward slashes first so
                        // wikilinks written with backslashes (e.g. `[[Web\\Foo]]`)
                        // can still match. Wikilinks may be written without the
                        // `.md` extension, so try both the literal target and
                        // its `.md`-appended form.
                        let t_norm = t.replace('\\', "/").to_ascii_lowercase();
                        let canonical = idx.lookup_unique(&t_norm).or_else(|| {
                            let with_md = format!("{t_norm}.md");
                            idx.lookup_unique(&with_md)
                        });
                        canonical == Some(old_rel) || canonical == Some(old_stem)
                    } else {
                        false
                    }
                }
                LinkKind::Markdown => {
                    // Absolute links (starting with `/`) are stripped of the
                    // site_prefix then compared; relative links are normalized
                    // against the source file's directory.
                    let norm = if span.link.target.starts_with('/') {
                        strip_site_prefix(&span.link.target, site_prefix)
                    } else {
                        normalize_target(Path::new(source_rel), &span.link.target)
                    };
                    if norm == old_rel || norm == old_stem {
                        true
                    } else if let Some(idx) = case_index {
                        // Try canonicalizing the normalized target through the
                        // index. Markdown links may also be written without the
                        // `.md` extension (e.g. `[x](Web/Foo)`), so try both the
                        // literal and `.md`-appended forms — mirrors the wikilink
                        // branch above.
                        let norm_lower = norm.to_ascii_lowercase();
                        let canonical = idx.lookup_unique(&norm_lower).or_else(|| {
                            let with_md = format!("{norm_lower}.md");
                            idx.lookup_unique(&with_md)
                        });
                        canonical == Some(old_rel) || canonical == Some(old_stem)
                    } else {
                        false
                    }
                }
            };

            if !matches {
                continue;
            }

            // Compute the new target text.
            let new_target = match span.kind {
                LinkKind::Wikilink => new_stem.to_string(),
                LinkKind::Markdown => {
                    // If the original link was absolute-path style, preserve that
                    // style by re-prepending the site_prefix (or just `/`).
                    if span.link.target.starts_with('/') {
                        // Preserve whether the original used .md or stem style.
                        let target = if std::path::Path::new(&span.link.target)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                        {
                            new_rel
                        } else {
                            new_stem
                        };
                        match site_prefix {
                            Some(prefix) => format!("/{prefix}/{target}"),
                            None => format!("/{target}"),
                        }
                    } else {
                        relative_path_between(source_rel, new_rel)
                    }
                }
            };

            // Build old_text and new_text using the ORIGINAL line (not cleaned).
            let old_text = line[span.full_start..span.full_end].to_string();
            let new_text = format!(
                "{}{}{}",
                &line[span.full_start..span.target_start],
                new_target,
                &line[span.target_end..span.full_end]
            );

            if old_text != new_text {
                replacements.push(Replacement {
                    line: line_num,
                    byte_offset: span.full_start,
                    old_text,
                    new_text,
                });
            }
        }
    }

    replacements
}

// ---------------------------------------------------------------------------
// Outbound rewrite planning
// ---------------------------------------------------------------------------

/// Walk every body line in `content` (which lives at `old_rel`) and return
/// [`Replacement`]s for relative markdown links whose targets change when the
/// file moves to `new_rel`.
///
/// Wikilinks are vault-relative and never change.
///
/// `dir_changed` indicates whether the move crosses directory boundaries.
/// When `false`, only self-links (pointing at `old_rel` itself) need updating;
/// all other relative links remain valid.
fn plan_outbound_rewrites(
    content: &str,
    old_rel: &str,
    new_rel: &str,
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

        // ---- Extract markdown link spans only ----
        let stripped_code = strip_inline_code(line);
        let cleaned = strip_inline_comments(stripped_code.as_ref());
        let spans = extract_link_spans_with_original(&cleaned, line);

        for span in spans {
            // Only rewrite markdown links — wikilinks are vault-relative.
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
    fn plan_mv_bare_wikilink_not_rewritten() {
        // Bare wikilinks (no path separator) are left alone — they are name-based
        // references that don't encode a location.
        let vault = create_vault(&[
            ("a.md", "---\ntitle: A\n---\nSee [[b]] for details\n"),
            ("b.md", "---\ntitle: B\n---\nContent\n"),
        ]);
        let plans = plan_mv(vault.path(), "b.md", "archive/b.md", None).unwrap();
        assert!(
            plans.is_empty(),
            "bare wikilink [[b]] should not be rewritten"
        );
    }

    #[test]
    fn plan_mv_bare_wikilink_with_alias_not_rewritten() {
        let vault = create_vault(&[("a.md", "See [[b|my note]] here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None).unwrap();
        assert!(
            plans.is_empty(),
            "bare wikilink [[b|my note]] should not be rewritten"
        );
    }

    #[test]
    fn plan_mv_bare_wikilink_with_fragment_not_rewritten() {
        let vault = create_vault(&[("a.md", "See [[b#section]] here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None).unwrap();
        assert!(
            plans.is_empty(),
            "bare wikilink [[b#section]] should not be rewritten"
        );
    }

    #[test]
    fn plan_mv_inbound_wikilink_with_path() {
        // Wikilinks that already contain a path ARE rewritten.
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
        )
        .unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].rel_path, "a.md");
        assert_eq!(plans[0].replacements.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[backlog/item]]");
        assert_eq!(plans[0].replacements[0].new_text, "[[backlog/done/item]]");
    }

    #[test]
    fn plan_mv_inbound_wikilink_with_path_and_alias() {
        let vault = create_vault(&[
            ("a.md", "See [[sub/b|my note]] here\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[sub/b|my note]]");
        assert_eq!(plans[0].replacements[0].new_text, "[[archive/b|my note]]");
    }

    #[test]
    fn plan_mv_inbound_wikilink_with_path_and_fragment() {
        let vault = create_vault(&[
            ("a.md", "See [[sub/b#section]] here\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[sub/b#section]]");
        assert_eq!(plans[0].replacements[0].new_text, "[[archive/b#section]]");
    }

    #[test]
    fn plan_mv_inbound_markdown_link() {
        let vault = create_vault(&[("a.md", "See [note](b.md) here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[note](b.md)");
        assert_eq!(plans[0].replacements[0].new_text, "[note](sub/b.md)");
    }

    #[test]
    fn plan_mv_outbound_relative_link() {
        // b.md in root links to a.md. Moving b.md to sub/b.md means the
        // relative path changes.
        let vault = create_vault(&[("a.md", "Content A\n"), ("b.md", "See [note](a.md) here\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None).unwrap();
        let moved_plan = plans.iter().find(|p| p.rel_path == "sub/b.md").unwrap();
        assert_eq!(moved_plan.replacements[0].old_text, "[note](a.md)");
        assert_eq!(moved_plan.replacements[0].new_text, "[note](../a.md)");
    }

    #[test]
    fn plan_mv_outbound_wikilink_unchanged() {
        // Wikilinks are vault-relative, so moving the file doesn't change them.
        let vault = create_vault(&[("a.md", "Content A\n"), ("b.md", "See [[a]] here\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None).unwrap();
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
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None).unwrap();
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
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[sub/b]]");
    }

    #[test]
    fn plan_mv_no_links_empty_result() {
        let vault = create_vault(&[("a.md", "No links here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None).unwrap();
        assert!(plans.is_empty());
    }

    #[test]
    fn plan_mv_multiple_links_one_line() {
        let vault = create_vault(&[
            ("a.md", "See [[sub/b]] and [[sub/b|alias]]\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements.len(), 2);
    }

    #[test]
    fn execute_plans_writes_files() {
        let vault = create_vault(&[("a.md", "See [[sub/b]] here\n"), ("sub/b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None).unwrap();
        execute_plans(vault.path(), &plans).unwrap();
        let content = fs::read_to_string(vault.path().join("a.md")).unwrap();
        assert!(content.contains("[[archive/b]]"));
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
    fn plan_mv_frontmatter_links_untouched() {
        // Links inside frontmatter must not be rewritten.
        let vault = create_vault(&[
            ("a.md", "---\nrelated: \"[[sub/b]]\"\n---\nBody [[sub/b]]\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements.len(), 1);
        assert_eq!(plans[0].replacements[0].line, 4); // Body line
    }

    #[test]
    fn plan_mv_same_directory_no_outbound_changes() {
        // Moving within the same directory: outbound relative links don't change.
        let vault = create_vault(&[("a.md", "Content A\n"), ("b.md", "See [note](a.md) here\n")]);
        // Both old and new are in the root → no outbound rewrite needed.
        let plans = plan_mv(vault.path(), "b.md", "c.md", None).unwrap();
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
        let plans = plan_mv(vault.path(), "b.md", "archive/b.md", None).unwrap();
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
        let plans = plan_mv(vault.path(), "b.md", "archive/b.md", None).unwrap();
        // sub/a.md links to sub/b.md, not root b.md — should NOT be rewritten.
        let sub_plan = plans.iter().find(|p| p.rel_path == "sub/a.md");
        assert!(sub_plan.is_none(), "false positive: {plans:?}");
    }

    #[test]
    fn plan_mv_bare_wikilink_with_md_extension_not_rewritten() {
        // [[b.md]] is a bare wikilink (no path separator) — leave it alone.
        let vault = create_vault(&[("a.md", "See [[b.md]] here\n"), ("b.md", "Content\n")]);
        let plans = plan_mv(vault.path(), "b.md", "sub/b.md", None).unwrap();
        assert!(
            plans.is_empty(),
            "bare wikilink [[b.md]] should not be rewritten"
        );
    }

    #[test]
    fn plan_mv_wikilink_with_path_and_md_extension() {
        // [[sub/b.md]] has a path separator — should be rewritten.
        let vault = create_vault(&[
            ("a.md", "See [[sub/b.md]] here\n"),
            ("sub/b.md", "Content\n"),
        ]);
        let plans = plan_mv(vault.path(), "sub/b.md", "archive/b.md", None).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements[0].old_text, "[[sub/b.md]]");
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
        )
        .unwrap();
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
        let plans = plan_mv(vault.path(), "page.md", "archive/page.md", None).unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
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
        let plans = plan_mv(vault.path(), "self.md", "other.md", None).unwrap();
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
        let plans = plan_mv(vault.path(), "self.md", "other.md", None).unwrap();
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
        )
        .unwrap();
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
        let plans = plan_mv(vault.path(), "sub/note.md", "archive/note.md", None).unwrap();
        let moved_plan = plans.iter().find(|p| p.rel_path == "archive/note.md");
        assert!(
            moved_plan.is_none(),
            "URL-scheme links must not be rewritten: {plans:?}"
        );
    }

    #[test]
    fn plan_mv_outbound_skips_fragment_only() {
        let vault = create_vault(&[("sub/note.md", "Jump to [top](#top).\n")]);
        let plans = plan_mv(vault.path(), "sub/note.md", "archive/note.md", None).unwrap();
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
        let plans = plan_mv(vault.path(), "sub/note.md", "archive/note.md", None).unwrap();
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
        let plans = plan_mv(vault.path(), "sub/a.md", "a.md", None).unwrap();
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
        let plans = plan_mv(vault.path(), "self.md", "other.md", None).unwrap();
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
        let plans = plan_mv(vault.path(), "sub/a.md", "a.md", None).unwrap();
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
        )
        .unwrap();

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
        let plans = plan_mv(vault.path(), "web/foo.md", "archive/foo.md", None).unwrap();

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
        let plans = plan_mv(vault.path(), "sub/target.md", "archive/target.md", None).unwrap();
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
        let plans = plan_mv(vault.path(), "sub/a.md", "a.md", None).unwrap();
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
        let plans = plan_mv(vault.path(), "sub/target.md", "archive/target.md", None).unwrap();
        let a_plan = plans.iter().find(|p| p.rel_path == "a.md");
        assert!(
            a_plan.is_some(),
            "[[sub/target]] after a comment containing a code fence should be found; got: {plans:?}"
        );
        let a_plan = a_plan.unwrap();
        assert_eq!(a_plan.replacements.len(), 1);
        assert_eq!(a_plan.replacements[0].old_text, "[[sub/target]]");
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
        let plans = plan_mv(vault.path(), "sub/a.md", "a.md", None).unwrap();
        let moved_plan = plans
            .iter()
            .find(|p| p.rel_path == "a.md")
            .expect("outbound link after comment-with-code-fence should be detected");
        assert_eq!(moved_plan.replacements.len(), 1);
        assert_eq!(moved_plan.replacements[0].old_text, "[peer](peer.md)");
        assert_eq!(moved_plan.replacements[0].new_text, "[peer](sub/peer.md)");
    }
}
