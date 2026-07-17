//! Keep a Changelog 1.1.0 conformance lint rules (`hyalo lint --profile changelog`).
//!
//! `CHANGELOG.md` is a frontmatter-less reserved file, so the schema layer can
//! only bind the `changelog` type by path and exempt it from the frontmatter
//! rules (see `templates/profile-changelog.toml`). Everything that actually
//! validates the 1.1.0 grammar lives here, split between:
//!
//! - the generic [`heading_grammar`](crate::commands::heading_grammar) engine —
//!   heading-*shape* rules (the `# Changelog` title, the `## [version]` /
//!   `## [Unreleased]` line shape, the six allowed `###` category names), and
//! - this module's changelog-specific logic — version *ordering* (semver
//!   strictly descending, `[Unreleased]` pinned on top), release-date
//!   monotonicity, empty-section detection, and the footer link-reference
//!   cross-check (every `[x.y.z]` heading needs a matching `[x.y.z]: url` link
//!   definition and vice versa).
//!
//! Unlike the OKF profile (warn-only), the changelog grammar is **stricter**:
//! most rules default to *error* (a malformed changelog is a real defect, not a
//! smell). Severity is per-rule and still overridable via `[lint.rules.<id>]`.
//! Every rule is a pure function over the already-read body — no filesystem, no
//! network.

use crate::commands::heading_grammar::{
    self, GrammarFinding, HeadingRule, ScannedHeading, TextMatcher,
};

/// The six Keep a Changelog 1.1.0 change categories (order as in the spec).
pub(crate) const CATEGORIES: &[&str] = &[
    "Added",
    "Changed",
    "Deprecated",
    "Removed",
    "Fixed",
    "Security",
];

/// Rule IDs exposed by the changelog profile. Kept in one place so the catalog
/// (`lint-rules list`) and the runtime stay in lock-step; the parity test in
/// this module asserts every emitted id is listed here.
#[cfg(test)]
pub(crate) const CHANGELOG_RULE_IDS: &[&str] = &[
    "CHANGELOG-TITLE",
    "CHANGELOG-VERSION-HEADING",
    "CHANGELOG-CATEGORY",
    "CHANGELOG-VERSION-ORDER",
    "CHANGELOG-DATE-ORDER",
    "CHANGELOG-UNRELEASED-POSITION",
    "CHANGELOG-EMPTY-SECTION",
    "CHANGELOG-LINK-REF",
];

/// A changelog advisory/error finding, in the shape the lint pipeline converts
/// into an `InternalViolation`.
pub(crate) struct ChangelogFinding {
    pub(crate) rule_id: &'static str,
    /// Severity to use when `[lint.rules.<id>]` does not override it. The
    /// changelog grammar is strict, so most rules default to `"error"`.
    pub(crate) default_severity: &'static str,
    /// 1-based line within the file, for display.
    pub(crate) line: usize,
    pub(crate) message: String,
}

/// A parsed `## [X.Y.Z] - DATE` or `## [Unreleased]` version heading.
struct VersionHeading {
    /// The bracketed label (`Unreleased`, or a version like `1.2.0`).
    label: String,
    /// Parsed semver, `None` for `Unreleased` or an unparseable label.
    semver: Option<(u64, u64, u64)>,
    /// The `YYYY-MM-DD` release date, if present and well-formed.
    date: Option<String>,
    /// 1-based file line of the heading.
    line: usize,
    /// Whether this is the `[Unreleased]` section.
    is_unreleased: bool,
}

/// Run the changelog grammar against a `CHANGELOG.md` body.
///
/// * `content` — the whole file (frontmatter-less for a reserved changelog, but
///   the caller passes the full content; footer link-ref scanning uses it).
/// * `body` — the post-frontmatter body slice (equal to `content` for a
///   frontmatter-less file).
/// * `body_line_offset` — 1-based file line on which `body` starts.
/// * `is_enabled` — predicate deciding whether a given rule id runs.
pub(crate) fn run_changelog_rules(
    content: &str,
    body: &str,
    body_line_offset: usize,
    is_enabled: &dyn Fn(&str) -> bool,
) -> Vec<ChangelogFinding> {
    let mut out: Vec<ChangelogFinding> = Vec::new();
    let headings = heading_grammar::scan_headings(body, body_line_offset);

    // --- Heading-shape rules via the generic grammar engine -----------------
    let mut grammar_out: Vec<GrammarFinding> = Vec::new();
    let title_rule = HeadingRule {
        rule_id: "CHANGELOG-TITLE",
        default_severity: "error",
        level: 1,
        matcher: TextMatcher::Exact("Changelog"),
    };
    if headings.iter().any(|h| h.level == 1) {
        // `check_level_rule` only visits *existing* level-1 headings, so a file
        // with zero H1s (e.g. one that starts straight at `## [Unreleased]`)
        // would silently pass. Only delegate to it when at least one H1 exists;
        // the `else` branch below covers the "missing entirely" case.
        heading_grammar::check_level_rule(&headings, &title_rule, is_enabled, &mut grammar_out);
    } else if is_enabled("CHANGELOG-TITLE") {
        grammar_out.push(GrammarFinding {
            rule_id: "CHANGELOG-TITLE",
            default_severity: "error",
            line: body_line_offset,
            message: "changelog must start with a `# Changelog` H1 heading (none found)".to_owned(),
        });
    }

    let version_rule = HeadingRule {
        rule_id: "CHANGELOG-VERSION-HEADING",
        default_severity: "error",
        level: 2,
        matcher: TextMatcher::Predicate(
            "`[Unreleased]` or `[X.Y.Z] - YYYY-MM-DD` (optional `[YANKED]`)",
            is_valid_version_heading,
        ),
    };
    heading_grammar::check_level_rule(&headings, &version_rule, is_enabled, &mut grammar_out);

    let category_rule = HeadingRule {
        rule_id: "CHANGELOG-CATEGORY",
        default_severity: "error",
        level: 3,
        matcher: TextMatcher::OneOf(CATEGORIES),
    };
    heading_grammar::check_level_rule(&headings, &category_rule, is_enabled, &mut grammar_out);

    for f in grammar_out {
        out.push(ChangelogFinding {
            rule_id: f.rule_id,
            default_severity: f.default_severity,
            line: f.line,
            message: f.message,
        });
    }

    // --- Version ordering / dates / unreleased position ---------------------
    let versions = parse_version_headings(&headings);
    check_version_order(&versions, is_enabled, &mut out);
    check_date_order(&versions, is_enabled, &mut out);
    check_unreleased_position(&versions, is_enabled, &mut out);
    check_empty_sections(&headings, body, body_line_offset, is_enabled, &mut out);

    // --- Footer link-reference cross-check ----------------------------------
    if is_enabled("CHANGELOG-LINK-REF") {
        check_link_refs(content, &versions, &mut out);
    }

    out
}

/// A level-2 heading is a valid version heading when its text is `[Unreleased]`
/// or `[X.Y.Z] - YYYY-MM-DD` (with an optional trailing ` [YANKED]`).
fn is_valid_version_heading(text: &str) -> bool {
    let Some(label) = bracket_label(text) else {
        return false;
    };
    if label.eq_ignore_ascii_case("unreleased") {
        // Unreleased carries no date.
        return text.trim() == format!("[{label}]");
    }
    if parse_semver(label).is_none() {
        return false;
    }
    // Remainder after `[label]` must be ` - YYYY-MM-DD` optionally ` [YANKED]`.
    let after = text[label.len() + 2..].trim_start();
    let Some(rest) = after.strip_prefix('-') else {
        return false;
    };
    let rest = rest.trim();
    let (date_part, yanked) = match rest.strip_suffix("[YANKED]") {
        Some(before) => (before.trim_end(), true),
        None => (rest, false),
    };
    let _ = yanked;
    is_iso_date(date_part)
}

/// Extract the `label` from a `[label]…` heading; `None` if not bracketed.
fn bracket_label(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('[')?;
    let close = rest.find(']')?;
    Some(&rest[..close])
}

/// Parse a `MAJOR.MINOR.PATCH` semver (numeric core only — pre-release / build
/// metadata are out of scope for the ordering check and rejected here).
fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let mut it = s.split('.');
    let major = it.next()?.parse::<u64>().ok()?;
    let minor = it.next()?.parse::<u64>().ok()?;
    let patch = it.next()?.parse::<u64>().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// `YYYY-MM-DD` shape check (digits + dashes; not a full calendar validation).
fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..].iter().all(u8::is_ascii_digit)
}

/// Turn the level-2 headings into structured version records (in document
/// order). Headings that aren't bracketed version headings are skipped (the
/// shape rule already flags those).
fn parse_version_headings(headings: &[ScannedHeading]) -> Vec<VersionHeading> {
    let mut out = Vec::new();
    for h in headings.iter().filter(|h| h.level == 2) {
        let Some(label) = bracket_label(&h.text) else {
            continue;
        };
        let is_unreleased = label.eq_ignore_ascii_case("unreleased");
        let semver = parse_semver(label);
        let date = extract_date(&h.text);
        out.push(VersionHeading {
            label: label.to_owned(),
            semver,
            date,
            line: h.line,
            is_unreleased,
        });
    }
    out
}

/// Pull the `YYYY-MM-DD` out of a `[x] - YYYY-MM-DD [YANKED]` heading.
fn extract_date(text: &str) -> Option<String> {
    let dash = text.find(" - ")?;
    let after = text[dash + 3..].trim();
    let candidate = after.split_whitespace().next()?;
    if is_iso_date(candidate) {
        Some(candidate.to_owned())
    } else {
        None
    }
}

/// Versions must be strictly descending by semver (newest first), ignoring the
/// `[Unreleased]` pseudo-version.
fn check_version_order(
    versions: &[VersionHeading],
    is_enabled: &dyn Fn(&str) -> bool,
    out: &mut Vec<ChangelogFinding>,
) {
    if !is_enabled("CHANGELOG-VERSION-ORDER") {
        return;
    }
    let released: Vec<&VersionHeading> = versions
        .iter()
        .filter(|v| !v.is_unreleased && v.semver.is_some())
        .collect();
    for pair in released.windows(2) {
        let (prev, cur) = (pair[0], pair[1]);
        let (pv, cv) = (prev.semver.unwrap(), cur.semver.unwrap());
        if cv >= pv {
            out.push(ChangelogFinding {
                rule_id: "CHANGELOG-VERSION-ORDER",
                default_severity: "error",
                line: cur.line,
                message: format!(
                    "version `[{}]` is not strictly below the preceding `[{}]` (versions must be newest-first, strictly descending)",
                    cur.label, prev.label
                ),
            });
        }
    }
}

/// Release dates must be monotonically non-increasing (newest first).
fn check_date_order(
    versions: &[VersionHeading],
    is_enabled: &dyn Fn(&str) -> bool,
    out: &mut Vec<ChangelogFinding>,
) {
    if !is_enabled("CHANGELOG-DATE-ORDER") {
        return;
    }
    let dated: Vec<(&str, &str, usize)> = versions
        .iter()
        .filter(|v| !v.is_unreleased)
        .filter_map(|v| v.date.as_deref().map(|d| (v.label.as_str(), d, v.line)))
        .collect();
    for pair in dated.windows(2) {
        let (_, prev_date, _) = pair[0];
        let (cur_label, cur_date, cur_line) = pair[1];
        if cur_date > prev_date {
            out.push(ChangelogFinding {
                rule_id: "CHANGELOG-DATE-ORDER",
                default_severity: "error",
                line: cur_line,
                message: format!(
                    "release date {cur_date} of `[{cur_label}]` is newer than the preceding entry's date {prev_date} (dates must be newest-first)"
                ),
            });
        }
    }
}

/// `[Unreleased]`, when present, must be the first version section.
fn check_unreleased_position(
    versions: &[VersionHeading],
    is_enabled: &dyn Fn(&str) -> bool,
    out: &mut Vec<ChangelogFinding>,
) {
    if !is_enabled("CHANGELOG-UNRELEASED-POSITION") {
        return;
    }
    for (i, v) in versions.iter().enumerate() {
        if v.is_unreleased && i != 0 {
            out.push(ChangelogFinding {
                rule_id: "CHANGELOG-UNRELEASED-POSITION",
                default_severity: "error",
                line: v.line,
                message:
                    "`## [Unreleased]` must be the first version section (pinned above all releases)"
                        .to_owned(),
            });
        }
    }
}

/// Flag a version or category section whose body is empty (no content before
/// the next heading). An empty `[Unreleased]` is allowed (a fresh changelog).
fn check_empty_sections(
    headings: &[ScannedHeading],
    body: &str,
    body_line_offset: usize,
    is_enabled: &dyn Fn(&str) -> bool,
    out: &mut Vec<ChangelogFinding>,
) {
    if !is_enabled("CHANGELOG-EMPTY-SECTION") {
        return;
    }
    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    for (idx, h) in headings.iter().enumerate() {
        // Only version (l2) and category (l3) sections are content-bearing.
        if h.level != 2 && h.level != 3 {
            continue;
        }
        // A version heading with an `[Unreleased]` label may be empty.
        if h.level == 2
            && bracket_label(&h.text).is_some_and(|l| l.eq_ignore_ascii_case("unreleased"))
        {
            continue;
        }
        // A section spans from its heading to the next heading of level <= its
        // own (a sibling or higher). Nested deeper headings (its subsections)
        // are part of the section — a version H2 with `### Added` children is
        // NOT empty even though no prose sits directly under the H2 line.
        let next_same_or_higher = headings[idx + 1..]
            .iter()
            .find(|n| n.level <= h.level)
            .map_or(lines.len(), |n| n.line - body_line_offset);
        // A deeper (child) heading immediately following this one means the
        // section has structured content — treat it as non-empty.
        let has_child_heading = headings.get(idx + 1).is_some_and(|n| {
            n.level > h.level && (n.line - body_line_offset) < next_same_or_higher
        });
        if has_child_heading {
            continue;
        }
        let start = h.line + 1 - body_line_offset; // 0-based index of first content line
        let end = next_same_or_higher;
        // Footer link-reference definitions (`[x]: url`) are not section content
        // — a released section whose only trailing lines are the footer refs is
        // still empty.
        let has_content = lines
            .get(start..end)
            .into_iter()
            .flatten()
            .map(|l| l.strip_suffix('\n').unwrap_or(l))
            .map(|l| l.strip_suffix('\r').unwrap_or(l))
            .any(|l| !l.trim().is_empty() && parse_link_def_label(l).is_none());
        if !has_content {
            out.push(ChangelogFinding {
                rule_id: "CHANGELOG-EMPTY-SECTION",
                default_severity: "warn",
                line: h.line,
                message: format!("section `{}` is empty (add entries or remove it)", h.text),
            });
        }
    }
}

/// Cross-check the footer link-reference definitions against the version
/// headings: every `[x.y.z]` version heading needs a matching `[x.y.z]: url`
/// definition, and every definition should correspond to a heading. The
/// `[Unreleased]` label participates too (it commonly links to a `compare` URL).
fn check_link_refs(content: &str, versions: &[VersionHeading], out: &mut Vec<ChangelogFinding>) {
    // Collect defined link labels: lines of the form `[label]: url`.
    let mut defined: Vec<(String, usize)> = Vec::new();
    for (i, raw) in content.split_inclusive('\n').enumerate() {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(label) = parse_link_def_label(line) {
            defined.push((label, i + 1));
        }
    }

    let defined_labels: Vec<&str> = defined.iter().map(|(l, _)| l.as_str()).collect();

    // Every version heading label needs a definition.
    for v in versions {
        let has_def = defined_labels
            .iter()
            .any(|d| d.eq_ignore_ascii_case(&v.label));
        if !has_def {
            out.push(ChangelogFinding {
                rule_id: "CHANGELOG-LINK-REF",
                default_severity: "warn",
                line: v.line,
                message: format!(
                    "version `[{}]` has no matching link reference definition (`[{}]: <url>`)",
                    v.label, v.label
                ),
            });
        }
    }

    // Every definition should match a version heading.
    for (label, line) in &defined {
        let has_heading = versions.iter().any(|v| v.label.eq_ignore_ascii_case(label));
        if !has_heading {
            out.push(ChangelogFinding {
                rule_id: "CHANGELOG-LINK-REF",
                default_severity: "warn",
                line: *line,
                message: format!(
                    "link reference `[{label}]` has no matching version heading (`## [{label}]`)"
                ),
            });
        }
    }
}

/// Parse a link-reference definition line `[label]: url`, returning `label`.
/// Requires a non-empty URL after the colon so a bare `[x]:` is not counted.
fn parse_link_def_label(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix('[')?;
    let close = rest.find("]:")?;
    let label = &rest[..close];
    if label.is_empty() {
        return None;
    }
    // Reject a label containing `[` (would be a nested/link-in-text, not a def).
    if label.contains('[') {
        return None;
    }
    let url = rest[close + 2..].trim();
    if url.is_empty() {
        return None;
    }
    Some(label.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn always_enabled(_: &str) -> bool {
        true
    }

    fn run(content: &str) -> Vec<ChangelogFinding> {
        run_changelog_rules(content, content, 1, &always_enabled)
    }

    fn ids(out: &[ChangelogFinding]) -> Vec<&str> {
        out.iter().map(|f| f.rule_id).collect()
    }

    const REFERENCE: &str = "\
# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- New thing.

## [1.1.0] - 2023-03-05

### Added

- Something.

### Fixed

- A bug.

## [1.0.0] - 2017-06-20

### Added

- First release.

[Unreleased]: https://example.com/compare/v1.1.0...HEAD
[1.1.0]: https://example.com/compare/v1.0.0...v1.1.0
[1.0.0]: https://example.com/releases/tag/v1.0.0
";

    #[test]
    fn reference_example_lints_clean() {
        let out = run(REFERENCE);
        assert!(
            out.is_empty(),
            "reference changelog must be clean: {:?}",
            out.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_emitted_rule_id_is_registered() {
        // A messy changelog that trips many rules; all ids must be registered.
        let messy = "\
# Wrong Title

## [Unreleased]

## [1.0.0] - 2020-01-01

### Bogus

- x.

## [2.0.0] - 2019-01-01

### Added

- y.
";
        let out = run_changelog_rules(messy, messy, 1, &always_enabled);
        for f in &out {
            assert!(
                CHANGELOG_RULE_IDS.contains(&f.rule_id),
                "unregistered rule id: {}",
                f.rule_id
            );
        }
    }

    #[test]
    fn wrong_title_errors() {
        let c = "# Change Log\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- x.\n\n[1.0.0]: u\n";
        let out = run(c);
        assert!(ids(&out).contains(&"CHANGELOG-TITLE"));
        assert_eq!(
            out.iter()
                .find(|f| f.rule_id == "CHANGELOG-TITLE")
                .unwrap()
                .default_severity,
            "error"
        );
    }

    #[test]
    fn missing_title_entirely_errors() {
        // No H1 at all (starts straight at the version heading) — must still
        // trip CHANGELOG-TITLE, not silently pass (check_level_rule alone only
        // visits *existing* H1s, so this needs the explicit "no H1" branch).
        let c = "## [1.0.0] - 2020-01-01\n\n### Added\n\n- x.\n\n[1.0.0]: u\n";
        let out = run(c);
        assert!(
            ids(&out).contains(&"CHANGELOG-TITLE"),
            "missing title must error: {:?}",
            out.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
        assert_eq!(
            out.iter()
                .find(|f| f.rule_id == "CHANGELOG-TITLE")
                .unwrap()
                .default_severity,
            "error"
        );
    }

    #[test]
    fn missing_title_respects_disabled_rule() {
        let c = "## [1.0.0] - 2020-01-01\n\n### Added\n\n- x.\n\n[1.0.0]: u\n";
        let out = run_changelog_rules(c, c, 1, &|id| id != "CHANGELOG-TITLE");
        assert!(!ids(&out).contains(&"CHANGELOG-TITLE"));
    }

    #[test]
    fn unknown_category_errors() {
        let c = "# Changelog\n\n## [1.0.0] - 2020-01-01\n\n### Improved\n\n- x.\n\n[1.0.0]: u\n";
        let out = run(c);
        assert!(ids(&out).contains(&"CHANGELOG-CATEGORY"));
    }

    #[test]
    fn out_of_order_versions_error() {
        let c = "# Changelog\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- x.\n\n## [2.0.0] - 2019-01-01\n\n### Added\n\n- y.\n\n[1.0.0]: a\n[2.0.0]: b\n";
        let out = run(c);
        assert!(ids(&out).contains(&"CHANGELOG-VERSION-ORDER"));
    }

    #[test]
    fn out_of_order_dates_error() {
        // versions descending but dates ascending
        let c = "# Changelog\n\n## [2.0.0] - 2019-01-01\n\n### Added\n\n- x.\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- y.\n\n[2.0.0]: a\n[1.0.0]: b\n";
        let out = run(c);
        assert!(ids(&out).contains(&"CHANGELOG-DATE-ORDER"));
    }

    #[test]
    fn malformed_version_heading_errors() {
        let c = "# Changelog\n\n## Version 1.0\n\n### Added\n\n- x.\n";
        let out = run(c);
        assert!(ids(&out).contains(&"CHANGELOG-VERSION-HEADING"));
    }

    #[test]
    fn unreleased_not_first_errors() {
        let c = "# Changelog\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- x.\n\n## [Unreleased]\n\n### Added\n\n- y.\n\n[1.0.0]: a\n[Unreleased]: b\n";
        let out = run(c);
        assert!(ids(&out).contains(&"CHANGELOG-UNRELEASED-POSITION"));
    }

    #[test]
    fn empty_released_section_warns() {
        let c = "# Changelog\n\n## [1.0.0] - 2020-01-01\n\n[1.0.0]: a\n";
        let out = run(c);
        let empty = out
            .iter()
            .find(|f| f.rule_id == "CHANGELOG-EMPTY-SECTION")
            .unwrap();
        assert_eq!(empty.default_severity, "warn");
    }

    #[test]
    fn empty_unreleased_is_allowed() {
        let c = "# Changelog\n\n## [Unreleased]\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- x.\n\n[Unreleased]: h\n[1.0.0]: a\n";
        let out = run(c);
        assert!(
            !out.iter().any(|f| f.rule_id == "CHANGELOG-EMPTY-SECTION"),
            "empty Unreleased must not warn: {:?}",
            out.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn missing_link_ref_warns() {
        let c = "# Changelog\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- x.\n";
        let out = run(c);
        let f = out
            .iter()
            .find(|f| f.rule_id == "CHANGELOG-LINK-REF")
            .unwrap();
        assert!(f.message.contains("no matching link reference"));
    }

    #[test]
    fn orphan_link_ref_warns() {
        let c = "# Changelog\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- x.\n\n[1.0.0]: a\n[9.9.9]: b\n";
        let out = run(c);
        assert!(
            out.iter()
                .any(|f| f.rule_id == "CHANGELOG-LINK-REF" && f.message.contains("9.9.9")),
            "orphan definition must warn"
        );
    }

    #[test]
    fn yanked_marker_accepted() {
        let c =
            "# Changelog\n\n## [1.0.0] - 2020-01-01 [YANKED]\n\n### Fixed\n\n- x.\n\n[1.0.0]: a\n";
        let out = run(c);
        assert!(
            !out.iter().any(|f| f.rule_id == "CHANGELOG-VERSION-HEADING"),
            "YANKED marker must be accepted: {:?}",
            out.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn semver_parses_and_compares() {
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("1.2"), None);
        assert_eq!(parse_semver("1.2.3.4"), None);
        assert_eq!(parse_semver("v1.2.3"), None);
        assert!((1, 0, 0) < (2, 0, 0));
        assert!((1, 2, 0) < (1, 10, 0));
    }

    #[test]
    fn link_def_parsing() {
        assert_eq!(
            parse_link_def_label("[1.0.0]: http://x").as_deref(),
            Some("1.0.0")
        );
        assert_eq!(
            parse_link_def_label("  [Unreleased]: u  ").as_deref(),
            Some("Unreleased")
        );
        assert_eq!(parse_link_def_label("[1.0.0]:"), None, "empty url rejected");
        assert_eq!(parse_link_def_label("not a def"), None);
    }
}
