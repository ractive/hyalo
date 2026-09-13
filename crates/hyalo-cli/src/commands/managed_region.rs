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

    /// Only standalone comments recognized by the shared Markdown syntax own bytes.
    pub(crate) fn classify(&self, content: &str) -> Result<Option<(usize, usize)>> {
        let (begins, ends) = marker_offsets(content, &self.begin, &self.end)?;
        match (begins.as_slice(), ends.as_slice()) {
            ([], []) => Ok(None),
            ([begin], [end]) if begin < end => Ok(Some((*begin, *end))),
            _ => anyhow::bail!(
                "malformed managed markers: expected one standalone {} followed by one {}; found {} opening and {} closing markers",
                self.begin,
                self.end,
                begins.len(),
                ends.len()
            ),
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
    /// it (MD022). An existing suffix is preserved byte for byte; new files
    /// end with a trailing newline.
    pub(crate) fn splice_validated(
        &self,
        old_content: &str,
        markers: Option<(usize, usize)>,
        generated: &str,
        title: &str,
        mode: AdoptMode,
    ) -> String {
        // Trim surrounding newlines so the wrap yields exactly one blank line
        // after BEGIN and before END (MD022/MD012 clean, no `lint --fix` drift).
        let body = generated.trim_matches('\n');
        let managed = format!("{}\n\n{body}\n\n{}", self.begin, self.end);

        if let Some((begin, end)) = markers {
            let before = &old_content[..begin];
            let after = &old_content[end + self.end.len()..];
            let mut result = String::with_capacity(before.len() + managed.len() + after.len());
            result.push_str(before);
            result.push_str(&managed);
            result.push_str(after);
            return result;
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

    #[cfg(test)]
    fn splice(&self, old: &str, generated: &str, title: &str, mode: AdoptMode) -> String {
        self.splice_validated(old, self.classify(old).unwrap(), generated, title, mode)
    }
}

/// Byte offsets of real standalone marker comments, excluding literal examples.
pub(crate) fn marker_offsets(
    content: &str,
    begin: &str,
    end: &str,
) -> Result<(Vec<usize>, Vec<usize>)> {
    let frame = hyalo_core::frontmatter::DocumentFrame::parse(content)?;
    let body_offset = frame.body_offset();
    // A document signature is outside Markdown syntax. Exclude it before
    // classifying literal code too, while keeping absolute splice offsets.
    let body_offset = if body_offset == 0 && content.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        body_offset
    };
    let content = &content[body_offset..];
    let syntax = hyalo_core::body_syntax::BodySyntax::new(content);
    let mut begins = Vec::new();
    let mut ends = Vec::new();
    for comment in syntax.html_comments() {
        let span = comment.content_span();
        let start = span.start - 4;
        let finish = span.end + 3;
        let line_start = syntax.line_start(comment.line()).unwrap_or(start);
        let line_end = content[finish..]
            .find('\n')
            .map_or(content.len(), |n| finish + n);
        if !content[line_start..start].trim().is_empty()
            || !content[finish..line_end].trim().is_empty()
        {
            continue;
        }
        match &content[start..finish] {
            marker if marker == begin => begins.push(body_offset + start),
            marker if marker == end => ends.push(body_offset + start),
            _ => {}
        }
    }
    Ok((begins, ends))
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
    /// Whether an entry existed when the transform was planned. Empty files
    /// remain replacements and never lose the exclusive-create guard.
    pub(crate) existed: bool,
}

impl GeneratePlan {
    pub(crate) fn changed(&self) -> bool {
        self.new_content != self.old_content
    }
    pub(crate) fn is_new(&self) -> bool {
        !self.existed
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

/// Publish a generated file through an exact captured replacement or an
/// exclusive new entry, with an invocation-owned durability session.
pub(crate) fn apply_plan(
    dir: &Path,
    plan: &GeneratePlan,
) -> Result<crate::commands::apply::ApplyReport> {
    apply_content(
        dir,
        &plan.rel_path,
        &plan.old_content,
        plan.existed,
        plan.new_content.as_bytes(),
    )
}

pub(crate) fn apply_content(
    root_path: &Path,
    rel_path: &str,
    old_content: &str,
    existed: bool,
    new_content: &[u8],
) -> Result<crate::commands::apply::ApplyReport> {
    use crate::commands::apply::{
        ApplyReport, EffectFailure, EffectState, IndexDisposition, PathEffect,
    };
    use hyalo_core::rooted::{Durability, RelativeName, VaultRoot, WriteSession};
    let root = VaultRoot::new(root_path)?;
    let name = RelativeName::new(rel_path)?;
    let mut session = WriteSession::new(Durability::PerFile);
    let effect = if existed {
        let captured = root.capture(&name)?;
        if captured.bytes()? != old_content.as_bytes() {
            anyhow::bail!(hyalo_core::rooted::SourceConflict(
                "generated file bytes changed"
            ));
        }
        captured
            .prepare(new_content, &session)?
            .commit(&mut session)?
    } else {
        root.destination(name)?.create(new_content, &mut session)?
    };
    let effect_error = effect.finalization_error().map(str::to_owned);
    let finish_error = session.finish().err().map(|error| error.to_string());
    let error = effect_error.or(finish_error);
    Ok(ApplyReport {
        paths: vec![PathEffect {
            file: rel_path.to_owned(),
            state: if error.is_some() {
                EffectState::CommittedWithFinalizationError
            } else {
                EffectState::Committed
            },
            category: error.as_ref().map(|_| EffectFailure::Finalization),
            error,
        }],
        index: IndexDisposition::NotUsed,
        index_error: None,
    })
}

pub(crate) fn reconcile_generated_notes(
    dir: &Path,
    report: &mut crate::commands::apply::ApplyReport,
    journal: &mut crate::commands::journal::MutationJournal<'_>,
) {
    use crate::commands::apply::{EffectState, IndexDisposition};
    let observed: Vec<_> = report
        .paths
        .iter()
        .filter(|path| {
            matches!(
                path.state,
                EffectState::Unchanged
                    | EffectState::Committed
                    | EffectState::CommittedWithFinalizationError
            )
        })
        .map(|path| path.file.clone())
        .collect();
    let unsafe_paths: Vec<_> = report
        .paths
        .iter()
        .filter(|path| path.state == EffectState::FailedBeforeCommit)
        .map(|path| path.file.clone())
        .collect();
    let (index, error) = journal.finalize_observed(dir, &observed, &unsafe_paths);
    report.index = index;
    report.index_error = error;
    if !journal.has_index() {
        report.index = IndexDisposition::NotUsed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_bom_markers_preserve_signature_and_exact_suffix() {
        for namespace in ["madr:toc", "okf:index"] {
            for eol in ["\n", "\r\n"] {
                let m = Markers::new(namespace);
                let old = format!("\u{feff}{}{eol}OLD{eol}{}{eol}Footer", m.begin, m.end);
                assert_eq!(
                    m.classify(&old).unwrap(),
                    Some((3, old.find(&m.end).unwrap()))
                );
                let out = m.splice(&old, "New table", "# Index", AdoptMode::Adopt);
                assert!(out.starts_with(&format!("\u{feff}{}", m.begin)));
                assert!(out.ends_with(&format!("{}{eol}Footer", m.end)));
                assert!(!out.contains("OLD"));
                assert_eq!(
                    m.splice(&out, "New table", "# Index", AdoptMode::Adopt),
                    out
                );
            }
        }
    }

    #[test]
    fn bom_is_only_a_signature_at_document_start() {
        let m = Markers::new("madr:toc");
        for prefix in [
            "\n\u{feff}",
            " \u{feff}",
            "\u{feff}prose ",
            "---\ntitle: X\n---\n\u{feff}",
        ] {
            let old = format!("{prefix}{}\nOLD\n{}\n", m.begin, m.end);
            assert!(m.classify(&old).is_err(), "{old:?}");
        }
        for literal in [
            format!("\u{feff}    {}\n    {}\n", m.begin, m.end),
            format!("\u{feff}```md\n{}\n{}\n```\n", m.begin, m.end),
            format!("\u{feff}`{}` and `{}`\n", m.begin, m.end),
        ] {
            assert_eq!(m.classify(&literal).unwrap(), None, "{literal:?}");
        }
    }

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
