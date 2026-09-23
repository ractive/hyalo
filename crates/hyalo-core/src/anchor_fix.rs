//! Conservative, explicitly opted-in fragment repairs with captured source bytes.
#![allow(clippy::missing_errors_doc)]

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use anyhow::Result;
use serde::Serialize;

use crate::anchor::{fragment_matches_headings, numbered_heading_repair};
use crate::case_index::CaseInsensitiveIndex;
use crate::index::VaultIndex;
use crate::link_rewrite::{Replacement, RewritePlan, apply_replacements, execute_plans_partial};
use crate::scanner::{LineClass, LineScanner, lines_with_rest};

/// A reviewed fragment-only proposal. Counts describe the pre-apply source.
#[derive(Debug, Clone, Serialize)]
pub struct AnchorFixPlan {
    /// Vault-relative source document.
    pub source: String,
    /// One-based file-absolute source line.
    pub line: usize,
    /// Resolved target document.
    pub target: String,
    /// Original fragment, without `#`.
    pub old_fragment: String,
    /// Actual generated heading slug, without `#`.
    pub new_fragment: String,
    /// The unique matching heading.
    pub heading: String,
    #[serde(skip)]
    byte_offset: usize,
    #[serde(skip)]
    source_bytes: Rc<String>,
}

/// An anchor left untouched, with an explicit reason.
#[derive(Debug, Clone, Serialize)]
pub struct AnchorFixDeferral {
    /// Vault-relative source document.
    pub source: String,
    /// One-based file-absolute source line.
    pub line: usize,
    /// Original fragment, without `#`.
    pub fragment: String,
    /// Why no write is safe.
    pub reason: String,
}

/// Independent fragment repair inventory.
#[derive(Debug, Default)]
pub struct AnchorFixReport {
    /// Broken anchors before any writes, including unrepairable forms.
    pub broken: usize,
    /// Eligible proposals.
    pub fixes: Vec<AnchorFixPlan>,
    /// Broken anchors for which no safe proposal exists.
    pub deferred: Vec<AnchorFixDeferral>,
}

/// Build fragment plans using indexed link and heading semantics and exact
/// parsed source spans. Only source documents carrying broken anchors are read.
pub fn plan_anchor_fixes(
    dir: &Path,
    index: &dyn VaultIndex,
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
) -> Result<AnchorFixReport> {
    plan_anchor_fixes_filtered(dir, index, site_prefix, case_index, |_, _| true)
}

/// Plan only links accepted by the caller's source and ignored-target scope.
pub fn plan_anchor_fixes_filtered(
    dir: &Path,
    index: &dyn VaultIndex,
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
    include: impl Fn(&str, &str) -> bool,
) -> Result<AnchorFixReport> {
    let canonical = crate::discovery::canonicalize_vault_dir(dir)?;
    let mut report = AnchorFixReport::default();
    for entry in index.entries() {
        let first_plan = report.fixes.len();
        let mut source_bytes = None;
        let mut spans = None;
        let links = entry
            .links
            .iter()
            .filter_map(|(line, link)| {
                if link.external {
                    return None;
                }
                let fragment = link.fragment.as_deref()?;
                let target = crate::discovery::resolve_link_from_source(
                    &canonical,
                    &entry.rel_path,
                    link.kind,
                    &link.target,
                    site_prefix,
                    case_index,
                )?;
                Some((*line, link.target.as_str(), fragment, target))
            })
            .chain(entry.self_anchors.iter().map(|anchor| {
                (
                    anchor.line,
                    "",
                    anchor.fragment.as_str(),
                    entry.rel_path.clone(),
                )
            }));
        for (line, authored_target, fragment, target) in links {
            if !include(&entry.rel_path, authored_target) {
                continue;
            }
            let Some(target_entry) = index.get(&target) else {
                continue;
            };
            if fragment_matches_headings(fragment, &target_entry.sections) {
                if (crate::anchor::is_templated_heading(fragment)
                    || target_entry.sections.iter().any(|section| {
                        section
                            .heading
                            .as_deref()
                            .is_some_and(crate::anchor::is_templated_heading)
                    }))
                    && !crate::anchor::fragment_matches_literal_headings(
                        fragment,
                        &target_entry.sections,
                    )
                {
                    report.deferred.push(AnchorFixDeferral {
                        source: entry.rel_path.clone(),
                        line,
                        fragment: fragment.to_owned(),
                        reason:
                            "templated heading or fragment is unknowable; no anchor repair proposed"
                                .to_owned(),
                    });
                }
                continue;
            }
            report.broken += 1;
            let candidate = numbered_heading_repair(fragment, &target_entry.sections);
            let result = (|| -> Result<AnchorFixPlan, String> {
                let (heading, new_fragment) = candidate.map_err(str::to_owned)?;
                if entry
                    .sections
                    .iter()
                    .any(|section| section.heading.is_some() && section.line == line)
                {
                    return Err(
                        "link occurs in a heading; repair could rename its generated anchor"
                            .to_owned(),
                    );
                }
                if source_bytes.is_none() {
                    let content = std::fs::read_to_string(canonical.join(&entry.rel_path))
                        .map_err(|error| format!("source unavailable: {error}"))?;
                    spans = Some(source_spans(&content));
                    source_bytes = Some(Rc::new(content));
                }
                let byte_offset = spans.as_mut().and_then(|spans| spans.get_mut(&(line, authored_target.to_owned(), fragment.to_owned())))
                    .and_then(std::collections::VecDeque::pop_front)
                    .ok_or_else(|| "unsupported destination form or stale source span (reference definitions, frontmatter and multiline links are deferred)".to_owned())?;
                Ok(AnchorFixPlan {
                    source: entry.rel_path.clone(),
                    line,
                    target,
                    old_fragment: fragment.to_owned(),
                    new_fragment,
                    heading,
                    byte_offset,
                    source_bytes: Rc::clone(source_bytes.as_ref().ok_or("source unavailable")?),
                })
            })();
            match result {
                Ok(plan) => report.fixes.push(plan),
                Err(reason) => report.deferred.push(AnchorFixDeferral {
                    source: entry.rel_path.clone(),
                    line,
                    fragment: fragment.to_owned(),
                    reason,
                }),
            }
        }
        if let Some(content) = source_bytes.as_ref()
            && report.fixes.len() > first_plan
        {
            let replacements: Vec<_> = report.fixes[first_plan..]
                .iter()
                .map(|plan| Replacement {
                    line: plan.line,
                    byte_offset: plan.byte_offset,
                    old_text: plan.old_fragment.clone(),
                    new_text: plan.new_fragment.clone(),
                })
                .collect();
            let projected = apply_replacements(content, &replacements);
            if !same_headings(content, &projected)? {
                let deferred: Vec<_> = report
                    .fixes
                    .drain(first_plan..)
                    .map(|plan| deferral(&plan, "repair would change source headings; no write"))
                    .collect();
                report.deferred.extend(deferred);
            }
        }
    }
    Ok(report)
}

fn same_headings(before: &str, after: &str) -> Result<bool> {
    // Setext headings are not yet part of the shared heading index. Protect
    // these author-written heading lines too, without broadening matching.
    fn setext_lines(content: &str) -> Vec<(usize, &str)> {
        let lines: Vec<_> = content.lines().collect();
        lines
            .windows(2)
            .enumerate()
            .filter_map(|(line, pair)| {
                let underline = pair[1].trim();
                (!pair[0].trim().is_empty()
                    && !underline.is_empty()
                    && (underline.bytes().all(|c| c == b'=')
                        || underline.bytes().all(|c| c == b'-')))
                .then_some((line, pair[0]))
            })
            .collect()
    }
    if setext_lines(before) != setext_lines(after) {
        return Ok(false);
    }
    let before_sections = crate::index::scan_slice_sections(before.as_bytes())?;
    let after_sections = crate::index::scan_slice_sections(after.as_bytes())?;
    Ok(before_sections
        .iter()
        .filter_map(|section| {
            section
                .heading
                .as_ref()
                .map(|heading| (section.line, heading))
        })
        .eq(after_sections.iter().filter_map(|section| {
            section
                .heading
                .as_ref()
                .map(|heading| (section.line, heading))
        })))
}

type SourceSpans = HashMap<(usize, String, String), std::collections::VecDeque<usize>>;

fn source_spans(content: &str) -> SourceSpans {
    let mut scanner = LineScanner::new();
    let mut spans = SourceSpans::new();
    for (line, rest) in lines_with_rest(content) {
        let cleaned = match scanner.classify(line, rest) {
            // YAML quoting/escape decoding can differ from indexed wikilinks.
            // Frontmatter destinations remain explicitly deferred.
            LineClass::Body(body) => body.cleaned(line, rest),
            _ => continue,
        };
        for span in crate::links::extract_fragment_spans(&cleaned, line) {
            spans
                .entry((scanner.line_num(), span.target, span.fragment))
                .or_default()
                .push_back(span.start);
        }
    }
    spans
}

/// Durable application results, separate from file-target repairs.
#[derive(Debug, Default)]
pub struct AnchorApplyReport {
    /// Plans whose bytes were published.
    pub applied: Vec<AnchorFixPlan>,
    /// Stale, conflicting, or failed proposals.
    pub deferred: Vec<AnchorFixDeferral>,
    /// Published source paths for snapshot refresh.
    pub modified_files: Vec<String>,
    /// Write failures (including finalization errors).
    pub failed: bool,
}

/// Revalidate source bytes and target heading eligibility immediately before
/// writing. A source with any file-target repair is conservatively deferred,
/// preventing a fragment plan from applying against a changed destination.
pub fn apply_anchor_fixes(
    dir: &Path,
    fixes: &[AnchorFixPlan],
    target_fixes: &[crate::link_fix::FixPlan],
) -> Result<AnchorApplyReport> {
    let canonical = crate::discovery::canonicalize_vault_dir(dir)?;
    let mut result = AnchorApplyReport::default();
    let mut by_source: std::collections::BTreeMap<&str, Vec<&AnchorFixPlan>> =
        std::collections::BTreeMap::new();
    for plan in fixes {
        by_source.entry(&plan.source).or_default().push(plan);
    }
    for (source, plans) in by_source {
        let Some(first) = plans.first() else { continue };
        let validation = (|| -> Result<Vec<Replacement>, String> {
            if target_fixes.iter().any(|fix| {
                fix.source == source || plans.iter().any(|plan| plan.target == fix.source)
            }) {
                return Err(
                    "conflicting file-target repair in source; rerun after target repairs"
                        .to_owned(),
                );
            }
            let source_path = canonical.join(source);
            if !crate::discovery::ensure_within_vault(&canonical, &source_path).unwrap_or(false) {
                return Err("source is missing or outside vault".to_owned());
            }
            let current = std::fs::read_to_string(&source_path)
                .map_err(|e| format!("source unavailable: {e}"))?;
            if current != *first.source_bytes {
                return Err("stale source bytes; rebuild anchor proposals".to_owned());
            }
            let mut headings = HashMap::new();
            let mut replacements = Vec::new();
            for plan in &plans {
                if !headings.contains_key(&plan.target) {
                    let path = canonical.join(&plan.target);
                    if !crate::discovery::ensure_within_vault(&canonical, &path).unwrap_or(false) {
                        return Err("target is missing or outside vault".to_owned());
                    }
                    let sections = crate::index::scan_file_sections(&path)
                        .map_err(|e| format!("target unavailable: {e}"))?;
                    headings.insert(&plan.target, sections);
                }
                let sections = headings
                    .get(&plan.target)
                    .ok_or("target headings unavailable")?;
                let candidate =
                    numbered_heading_repair(&plan.old_fragment, sections).map_err(str::to_owned)?;
                if candidate.0 != plan.heading
                    || candidate.1 != plan.new_fragment
                    || fragment_matches_headings(&plan.old_fragment, sections)
                    || !fragment_matches_headings(&plan.new_fragment, sections)
                {
                    return Err("target headings changed; rebuild anchor proposals".to_owned());
                }
                replacements.push(Replacement {
                    line: plan.line,
                    byte_offset: plan.byte_offset,
                    old_text: plan.old_fragment.clone(),
                    new_text: plan.new_fragment.clone(),
                });
            }
            Ok(replacements)
        })();
        let replacements = match validation {
            Ok(replacements) => replacements,
            Err(reason) => {
                result
                    .deferred
                    .extend(plans.iter().map(|p| deferral(p, &reason)));
                continue;
            }
        };
        let rewritten_content = apply_replacements(&first.source_bytes, &replacements);
        if !same_headings(&first.source_bytes, &rewritten_content)? {
            result.deferred.extend(
                plans
                    .iter()
                    .map(|p| deferral(p, "repair would change source headings; no write")),
            );
            continue;
        }
        let rewrite = RewritePlan {
            path: canonical.join(source),
            rel_path: source.to_owned(),
            replacements,
            rewritten_content,
            mtime: None,
            original_content: Some(first.source_bytes.to_string()),
        };
        let written = execute_plans_partial(&canonical, &[rewrite])?;
        result.failed |= written.has_failures();
        for outcome in written.outcomes {
            if outcome.applied {
                result.modified_files.push(source.to_owned());
                result.applied.extend(plans.iter().map(|p| (*p).clone()));
            }
            if let Some(error) = outcome.error {
                result
                    .deferred
                    .extend(plans.iter().map(|p| deferral(p, &error)));
            }
        }
    }
    Ok(result)
}

fn deferral(plan: &AnchorFixPlan, reason: &str) -> AnchorFixDeferral {
    AnchorFixDeferral {
        source: plan.source.clone(),
        line: plan.line,
        fragment: plan.old_fragment.clone(),
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{ScanOptions, ScannedIndex};

    fn fixture(source: &str, target: &str) -> (tempfile::TempDir, ScannedIndex) {
        let temp = tempfile::tempdir().unwrap();
        let files: Vec<_> = [("source.md", source), ("target.md", target)]
            .into_iter()
            .map(|(name, body)| {
                let path = temp.path().join(name);
                std::fs::write(&path, body).unwrap();
                (path, name.to_owned())
            })
            .collect();
        let index = ScannedIndex::build(
            &files,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap()
        .index;
        (temp, index)
    }

    #[test]
    fn numbered_repair_preserves_all_other_bytes_and_is_idempotent() {
        let source = "---\r\ntitle: Source\r\n---\r\n## 2. Local section\r\n[é #label](target.md?q=1#success-metrics \"title #success-metrics\") [[target#success-metrics|alias]]\r\n[s](#local-section) [[#local-section|Local]]\r\n`[code](target.md#success-metrics)`\r\n";
        let (temp, index) = fixture(source, "## 6. Success metrics\n");
        let report = plan_anchor_fixes(temp.path(), &index, None, None).unwrap();
        assert_eq!(report.broken, 4);
        assert_eq!(report.fixes.len(), 4, "{report:?}");
        assert_eq!(
            std::fs::read_to_string(temp.path().join("source.md")).unwrap(),
            source
        );
        let applied = apply_anchor_fixes(temp.path(), &report.fixes, &[]).unwrap();
        assert_eq!(applied.applied.len(), 4, "{applied:?}");
        let expected = source
            .replace("?q=1#success-metrics", "?q=1#6-success-metrics")
            .replace("[[target#success-metrics", "[[target#6-success-metrics")
            .replace("(#local-section)", "(#2-local-section)")
            .replace("[[#local-section", "[[#2-local-section");
        assert_eq!(
            std::fs::read_to_string(temp.path().join("source.md")).unwrap(),
            expected
        );
        let again = apply_anchor_fixes(temp.path(), &report.fixes, &[]).unwrap();
        assert!(again.applied.is_empty());
        assert!(
            again
                .deferred
                .iter()
                .all(|d| d.reason.contains("stale source"))
        );
        let (_, fresh) = fixture(&expected, "## 6. Success metrics\n");
        let preview = plan_anchor_fixes(temp.path(), &fresh, None, None).unwrap();
        assert_eq!(preview.broken, 0);
    }

    #[test]
    fn ambiguous_similar_reference_and_frontmatter_forms_are_visible_deferrals() {
        let source = "---\nrelated: '[[target#success-metrics]]'\n---\n[x][r]\n[r]: target.md#success-metrics \"title\"\n";
        let (temp, index) = fixture(source, "## 6. Success metrics\n");
        let report = plan_anchor_fixes(temp.path(), &index, None, None).unwrap();
        assert!(report.fixes.is_empty(), "{report:?}");
        assert!(!report.deferred.is_empty());
        assert!(
            report
                .deferred
                .iter()
                .all(|d| d.reason.contains("unsupported destination"))
        );
        for headings in [
            "## 6. Success metrics\n## 7. Success metrics\n",
            "## 6. Success metrics\n## 6. Success metrics\n",
            "## 6. Successful metrics\n",
        ] {
            let (temp, index) = fixture("[x](target.md#success-metrics)\n", headings);
            let report = plan_anchor_fixes(temp.path(), &index, None, None).unwrap();
            assert_eq!(report.broken, 1);
            assert!(report.fixes.is_empty());
            assert_eq!(report.deferred.len(), 1);
        }
    }

    #[test]
    fn stale_source_heading_and_missing_target_never_write() {
        for mutation in ["source", "heading", "missing", "duplicate"] {
            let source = "[x](target.md#success-metrics)\n";
            let (temp, index) = fixture(source, "## 6. Success metrics\n");
            let report = plan_anchor_fixes(temp.path(), &index, None, None).unwrap();
            assert_eq!(report.fixes.len(), 1);
            match mutation {
                "source" => {
                    std::fs::write(temp.path().join("source.md"), format!("prefix\n{source}"))
                        .unwrap();
                }
                "heading" => {
                    std::fs::write(temp.path().join("target.md"), "## 7. Success metrics\n")
                        .unwrap();
                }
                "duplicate" => std::fs::write(
                    temp.path().join("target.md"),
                    "## 6. Success metrics\n## 6. Success metrics\n",
                )
                .unwrap(),
                _ => std::fs::remove_file(temp.path().join("target.md")).unwrap(),
            }
            let before = std::fs::read(temp.path().join("source.md")).unwrap();
            let applied = apply_anchor_fixes(temp.path(), &report.fixes, &[]).unwrap();
            assert!(applied.applied.is_empty(), "{mutation}");
            assert_eq!(applied.deferred.len(), 1);
            assert_eq!(
                std::fs::read(temp.path().join("source.md")).unwrap(),
                before
            );
        }
    }

    #[test]
    fn conflicting_file_target_plan_defers_anchor_plan() {
        let source = "[x](target.md#success-metrics)\n";
        let (temp, index) = fixture(source, "## 6. Success metrics\n");
        let report = plan_anchor_fixes(temp.path(), &index, None, None).unwrap();
        let target_fix = crate::link_fix::FixPlan {
            source: "source.md".into(),
            line: 1,
            old_target: "target.md".into(),
            new_target: "other.md".into(),
            strategy: crate::link_fix::FixStrategy::CaseInsensitive,
            confidence: 1.0,
            emitted_target: None,
        };
        let applied = apply_anchor_fixes(temp.path(), &report.fixes, &[target_fix]).unwrap();
        assert!(applied.applied.is_empty());
        assert!(applied.deferred[0].reason.contains("conflicting"));
        assert_eq!(
            std::fs::read_to_string(temp.path().join("source.md")).unwrap(),
            source
        );
    }
    #[test]
    fn heading_link_repairs_are_deferred_without_renaming_anchors() {
        for source in [
            "## 6. [Section](target.md#success-metrics)\n[s](#sectiontargetmdsuccess-metrics)\n",
            "6. [Section](target.md#success-metrics)\n--------------------------------------\n[s](#sectiontargetmdsuccess-metrics)\n",
        ] {
            let (temp, index) = fixture(source, "## 6. Success metrics\n");
            let report = plan_anchor_fixes(temp.path(), &index, None, None).unwrap();
            assert!(
                report.deferred.iter().any(|d| d.reason.contains("heading")),
                "{report:?}"
            );
            assert!(
                report.fixes.iter().all(|p| p.target == "source.md"),
                "{source:?}: {report:?}"
            );
            let applied = apply_anchor_fixes(temp.path(), &report.fixes, &[]).unwrap();
            assert!(applied.applied.iter().all(|p| p.target == "source.md"));
            assert!(
                std::fs::read_to_string(temp.path().join("source.md"))
                    .unwrap()
                    .contains("target.md#success-metrics")
            );
        }
    }

    #[test]
    fn repeated_occurrences_use_distinct_exact_spans() {
        let source = "[x](target.md#success-metrics) ".repeat(1000);
        let (temp, index) = fixture(&source, "## 6. Success metrics\n");
        let report = plan_anchor_fixes(temp.path(), &index, None, None).unwrap();
        assert_eq!(report.fixes.len(), 1000);
        let applied = apply_anchor_fixes(temp.path(), &report.fixes, &[]).unwrap();
        assert_eq!(applied.applied.len(), 1000);
        assert_eq!(
            std::fs::read_to_string(temp.path().join("source.md")).unwrap(),
            source.replace("#success-metrics", "#6-success-metrics")
        );
    }
    #[test]
    fn unknown_template_anchors_are_visible_but_never_counted_broken() {
        let (temp, index) = fixture(
            "[x](target.md#success-metrics)\n",
            "## {{ numbered_heading }}\n",
        );
        let report = plan_anchor_fixes(temp.path(), &index, None, None).unwrap();
        assert_eq!(report.broken, 0);
        assert!(report.fixes.is_empty());
        assert_eq!(report.deferred.len(), 1);
        assert!(report.deferred[0].reason.contains("unknowable"));
    }
}
