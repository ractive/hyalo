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

use crate::discovery::{canonicalize_vault_dir, ensure_within_vault};
use crate::link_graph::{LinkGraph, normalize_target, relative_path_between, strip_site_prefix};
use crate::links::{LinkKind, extract_link_spans_with_original};
use crate::scanner::{FenceTracker, is_comment_fence, strip_inline_code, strip_inline_comments};

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
    let build = LinkGraph::build(dir, site_prefix).context("building link graph")?;
    for (path, msg) in &build.warnings {
        eprintln!("warning: skipping {}: {msg}", path.display());
    }
    let graph = build.graph;

    let old_stem = old_rel.strip_suffix(".md").unwrap_or(old_rel);
    let new_stem = new_rel.strip_suffix(".md").unwrap_or(new_rel);

    // Whether the file moves to a different directory.
    let old_dir = parent_dir(old_rel);
    let new_dir = parent_dir(new_rel);
    let dir_changed = old_dir != new_dir;

    // Step 2: gather inbound backlinks, grouped by source file.
    let backlinks = graph.backlinks(old_rel);
    let mut by_source: HashMap<PathBuf, Vec<_>> = HashMap::new();
    for entry in backlinks {
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
                },
            );
        }
    }

    // Step 4: plan outbound link updates for the moved file itself,
    // but only when its directory context changes.
    if dir_changed {
        let old_abs = dir.join(old_rel);
        let content = std::fs::read_to_string(&old_abs)
            .with_context(|| format!("reading {}", old_abs.display()))?;

        let outbound_replacements = plan_outbound_rewrites(&content, old_rel, new_rel);

        if !outbound_replacements.is_empty() {
            let moved_key = PathBuf::from(old_rel.replace('\\', "/"));
            // The path targets the NEW location — after fs::rename, that's
            // where the file lives and where the rewritten content must be
            // written by execute_plans.
            let new_abs = dir.join(new_rel);

            if let Some(existing) = plans.get_mut(&moved_key) {
                // The moved file already has an inbound plan; merge.
                // Update path to point to the new location and re-apply
                // all replacements (inbound + outbound) together.
                existing.path = new_abs;
                existing.rel_path = new_rel.to_string();
                existing.replacements.extend(outbound_replacements);
                existing.rewritten_content = apply_replacements(&content, &existing.replacements);
            } else {
                let rewritten_content = apply_replacements(&content, &outbound_replacements);
                plans.insert(
                    moved_key,
                    RewritePlan {
                        path: new_abs,
                        rel_path: new_rel.to_string(),
                        replacements: outbound_replacements,
                        rewritten_content,
                    },
                );
            }
        }
    }

    Ok(plans.into_values().collect())
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

        std::fs::write(&plan.path, &plan.rewritten_content)
            .with_context(|| format!("writing {}", plan.path.display()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Inbound rewrite planning
// ---------------------------------------------------------------------------

/// Walk every body line in `content` and return [`Replacement`]s for links
/// that target the moved file.
fn plan_inbound_rewrites(
    content: &str,
    source_rel: &str,
    old_rel: &str,
    old_stem: &str,
    new_rel: &str,
    new_stem: &str,
    site_prefix: Option<&str>,
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
        if is_comment_fence(line) {
            in_comment_fence = !in_comment_fence;
            continue;
        }
        if in_comment_fence {
            continue;
        }

        // ---- Fenced code block ----
        if fence.process_line(line) {
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
                    (t.contains('/') || t.contains('\\')) && (t == old_stem || t == old_rel)
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
                    norm == old_rel || norm == old_stem
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
fn plan_outbound_rewrites(content: &str, old_rel: &str, new_rel: &str) -> Vec<Replacement> {
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

        // ---- Comment fence ----
        if is_comment_fence(line) {
            in_comment_fence = !in_comment_fence;
            continue;
        }
        if in_comment_fence {
            continue;
        }

        // ---- Fenced code block ----
        if fence.process_line(line) {
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

            // Resolve target relative to the OLD location's directory.
            let resolved = normalize_target(Path::new(old_rel), &span.link.target);

            // Compute new relative path from the NEW location.
            let new_target = relative_path_between(new_rel, &resolved);

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
            sorted.sort_by(|a, b| b.byte_offset.cmp(&a.byte_offset));

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
        };

        let result = execute_plans(vault.path(), &[bad_plan]);
        assert!(result.is_err(), "must refuse to write outside vault");

        // Original file must be untouched.
        let content = fs::read_to_string(&outside_path).unwrap();
        assert_eq!(content, "original\n");
    }
}
