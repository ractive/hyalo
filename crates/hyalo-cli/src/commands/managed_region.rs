//! Shared managed-region splice + generate/apply machinery for profile
//! generators (`okf index`, `madr toc`, and future profiles).
//!
//! A *managed region* is the span of a generated file that a `hyalo`
//! generator owns, delimited by a pair of HTML-comment markers
//! (`<!-- <prefix>:begin -->` / `<!-- <prefix>:end -->`). Prose outside the
//! markers is preserved verbatim across regenerations. This mirrors the
//! long-standing `okf.rs` implementation but parametrizes the marker prefix so
//! every profile can reuse the same drift-safe splice and the same
//! "dry-run exits non-zero on drift" plan/apply shape.
//!
//! The splice is anchored on structural position — the closing marker is
//! searched for strictly *after* the opening marker — so a stray mention of the
//! marker text in prose above the real region cannot corrupt the splice (the
//! same class of bug iter-165/166 hit in the OKF code).

use anyhow::{Context, Result};
use std::path::Path;

/// The two HTML-comment markers for a given prefix.
pub(crate) struct Markers {
    begin: String,
    end: String,
}

impl Markers {
    /// Build the `<!-- <prefix>:begin -->` / `<!-- <prefix>:end -->` pair.
    pub(crate) fn new(prefix: &str) -> Self {
        Self {
            begin: format!("<!-- {prefix}:begin -->"),
            end: format!("<!-- {prefix}:end -->"),
        }
    }

    /// Splice `generated` into `old_content`'s managed region, preserving prose
    /// outside the markers.
    ///
    /// When markers already exist the region between them is replaced in place.
    /// When no markers exist the behavior depends on `mode`:
    /// - [`AdoptMode::Adopt`] (the default) preserves the entire existing body
    ///   and appends the managed region after it — never dropping content;
    /// - [`AdoptMode::Replace`] discards the body and writes a fresh file:
    ///   `title` (already `#`-prefixed, e.g. `"# ADRs"`) followed by the region.
    ///
    /// The managed block is wrapped as `<begin>\n\n<generated>\n\n<end>` so the
    /// heading/table `generated` starts and ends with keeps blank lines around
    /// it (MD022). Always ends with a single trailing newline.
    pub(crate) fn splice(
        &self,
        old_content: &str,
        generated: &str,
        title: &str,
        mode: AdoptMode,
    ) -> String {
        // Trim surrounding newlines so the wrap yields exactly one blank line
        // after BEGIN and before END (MD022/MD012 clean, no `lint --fix` drift).
        let body = generated.trim_matches('\n');
        let managed = format!("{}\n\n{body}\n\n{}", self.begin, self.end);

        // Find END strictly after BEGIN so a stray marker mention in prose (or a
        // code block) above the region can't be mistaken for the real closer.
        let markers = old_content.find(&self.begin).and_then(|begin| {
            old_content[begin + self.begin.len()..]
                .find(&self.end)
                .map(|rel_end| (begin, begin + self.begin.len() + rel_end))
        });
        if let Some((begin, end)) = markers {
            let before = &old_content[..begin];
            let after = &old_content[end + self.end.len()..];
            let mut result = String::with_capacity(before.len() + managed.len() + after.len());
            result.push_str(before);
            result.push_str(&managed);
            result.push_str(after);
            return ensure_trailing_newline(&result);
        }

        // No markers. Non-destructive adopt: keep the existing body and append
        // the managed region. Only Replace discards a marker-less body.
        if mode == AdoptMode::Adopt && !old_content.trim().is_empty() {
            let mut result = String::with_capacity(old_content.len() + managed.len() + 2);
            result.push_str(old_content);
            if !result.ends_with("\n\n") {
                while result.ends_with('\n') {
                    result.pop();
                }
                result.push_str("\n\n");
            }
            result.push_str(&managed);
            return ensure_trailing_newline(&result);
        }

        // Fresh/replaced file with the given title heading.
        let mut result = String::new();
        result.push_str(title);
        result.push_str("\n\n");
        result.push_str(&managed);
        ensure_trailing_newline(&result)
    }

    /// Whether `old_content` already carries a valid marker pair.
    pub(crate) fn has_markers(&self, old_content: &str) -> bool {
        old_content.find(&self.begin).is_some_and(|begin| {
            old_content[begin + self.begin.len()..]
                .find(&self.end)
                .is_some()
        })
    }
}

/// How a marker-less existing file is handled when regenerating its managed
/// region. Mirrors the `okf index` policy: preserve by default, overwrite only
/// on an explicit `--replace`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdoptMode {
    /// Preserve the whole existing body and append the managed region (default).
    Adopt,
    /// Overwrite a marker-less file with a fresh managed file (opt-in).
    Replace,
}

fn ensure_trailing_newline(s: &str) -> String {
    if s.ends_with('\n') {
        s.to_owned()
    } else {
        format!("{s}\n")
    }
}

/// A planned regeneration of one generated file (index/TOC).
pub(crate) struct GeneratePlan {
    /// Vault-relative path of the file (forward slashes).
    pub(crate) rel_path: String,
    /// The full new file content.
    pub(crate) new_content: String,
    /// The current on-disk content (empty when the file is absent).
    pub(crate) old_content: String,
}

impl GeneratePlan {
    pub(crate) fn changed(&self) -> bool {
        self.new_content != self.old_content
    }
    pub(crate) fn is_new(&self) -> bool {
        self.old_content.is_empty()
    }

    /// The change kind for reporting: `create` (new file), `adopt` (existing
    /// marker-less body preserved), or `update` (managed region refreshed).
    /// `markers` is whether the *old* content already had a marker pair.
    pub(crate) fn action_str(&self, markers: bool) -> &'static str {
        if self.is_new() {
            "create"
        } else if markers {
            "update"
        } else {
            "adopt"
        }
    }
}

/// Read the current content of `dir/rel_path` (empty string when absent).
pub(crate) fn read_old_content(dir: &Path, rel_path: &str) -> Result<String> {
    let full = dir.join(rel_path);
    if full.is_file() {
        std::fs::read_to_string(&full).with_context(|| format!("failed to read {rel_path}"))
    } else {
        Ok(String::new())
    }
}

/// Write a changed plan to disk atomically.
pub(crate) fn apply_plan(dir: &Path, plan: &GeneratePlan) -> Result<()> {
    let full = dir.join(&plan.rel_path);
    hyalo_core::fs_util::atomic_write(&full, plan.new_content.as_bytes())
        .with_context(|| format!("failed to write {}", plan.rel_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splice_fresh_file_uses_title() {
        let m = Markers::new("madr:toc");
        let out = m.splice("", "* [1](1.md)", "# ADRs", AdoptMode::Adopt);
        assert!(out.starts_with("# ADRs\n"));
        assert!(out.contains("<!-- madr:toc:begin -->"));
        assert!(out.contains("* [1](1.md)"));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn splice_preserves_prose_outside_markers() {
        let m = Markers::new("madr:toc");
        let old =
            "# ADRs\n\nIntro.\n\n<!-- madr:toc:begin -->\nOLD\n<!-- madr:toc:end -->\n\nFooter.\n";
        let out = m.splice(old, "* [1](1.md)", "# ADRs", AdoptMode::Adopt);
        assert!(out.contains("Intro."));
        assert!(out.contains("Footer."));
        assert!(out.contains("* [1](1.md)"));
        assert!(!out.contains("OLD"));
    }

    #[test]
    fn splice_ignores_end_marker_in_prose_before_begin() {
        let m = Markers::new("madr:toc");
        let old = "See `<!-- madr:toc:end -->` in docs.\n\n<!-- madr:toc:begin -->\nOLD\n<!-- madr:toc:end -->\n\nFooter.\n";
        let out = m.splice(old, "* [1](1.md)", "# ADRs", AdoptMode::Adopt);
        assert!(out.contains("See `"), "prose before begin survives: {out}");
        assert!(out.contains("Footer."));
        assert!(!out.contains("OLD"));
    }

    #[test]
    fn splice_is_idempotent() {
        let m = Markers::new("madr:toc");
        let first = m.splice("", "* [1](1.md)", "# ADRs", AdoptMode::Adopt);
        let second = m.splice(&first, "* [1](1.md)", "# ADRs", AdoptMode::Adopt);
        assert_eq!(first, second);
    }

    #[test]
    fn splice_adopts_marker_less_body() {
        let m = Markers::new("madr:toc");
        let old = "# ADRs\n\nHand-written intro that must survive.\n";
        let out = m.splice(old, "| x |", "# ADRs", AdoptMode::Adopt);
        assert!(
            out.contains("Hand-written intro that must survive."),
            "adopt preserves body: {out}"
        );
        assert!(out.contains("<!-- madr:toc:begin -->"));
        assert!(out.contains("| x |"));
        // Second pass finds markers → in-place update, no duplicate region.
        let again = m.splice(&out, "| x |", "# ADRs", AdoptMode::Adopt);
        assert_eq!(again.matches("<!-- madr:toc:begin -->").count(), 1);
    }

    #[test]
    fn splice_replace_discards_marker_less_body() {
        let m = Markers::new("madr:toc");
        let old = "# ADRs\n\nThrow me away.\n";
        let out = m.splice(old, "| x |", "# ADRs", AdoptMode::Replace);
        assert!(!out.contains("Throw me away."), "replace drops body: {out}");
        assert!(out.contains("<!-- madr:toc:begin -->"));
    }

    #[test]
    fn splice_managed_block_has_md022_blank_lines() {
        let m = Markers::new("madr:toc");
        let out = m.splice("", "| # | Title |", "# ADRs", AdoptMode::Adopt);
        assert!(
            out.contains("<!-- madr:toc:begin -->\n\n"),
            "blank line after begin: {out}"
        );
        assert!(
            out.contains("\n\n<!-- madr:toc:end -->"),
            "blank line before end: {out}"
        );
    }
}
