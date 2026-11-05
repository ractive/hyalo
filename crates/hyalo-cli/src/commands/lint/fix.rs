//! Body auto-fix application and the offsets it needs.
//!
//! Split out of the single 4,005-line `commands/lint.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `commands::lint::...` paths and behaviour are
//! unchanged.

use super::FixOutcome;
use hyalo_core::schema::{PropertyConstraint, SchemaConfig};

/// Apply body fixes greedily.
///
/// Returns `(fixed_content, outcomes)` where `outcomes[i]` corresponds to
/// `fixes[i]` — either `Applied`, `Conflict`, or (for diagnostics without a
/// fix, a fix that lost a conflict, or a fix that turned out to be a
/// byte-for-byte no-op) `NoFix`.
///
/// Conflict resolution and buffer mutation use two different orderings on
/// purpose:
/// - **Winner selection** happens in priority order (`Error` before `Warn`,
///   then descending start offset), so a higher-severity fix (e.g. HYALO001)
///   is never displaced by an overlapping lower-severity one (e.g. MD009)
///   just because the latter happens to sort first by offset.
/// - **Buffer mutation** of the resulting non-overlapping winners always
///   proceeds in descending start-offset order, which is required for
///   correctness: each edit's range must still be valid against the
///   partially mutated buffer, and that only holds if edits at higher
///   offsets (later in the string) are applied first.
pub(super) fn apply_body_fixes(
    body: &str,
    fixes: &[&hyalo_mdlint::Diagnostic],
) -> (String, Vec<FixOutcome>) {
    let severity_rank = |sev: hyalo_mdlint::DiagSeverity| match sev {
        hyalo_mdlint::DiagSeverity::Error => 0,
        hyalo_mdlint::DiagSeverity::Warn => 1,
    };

    // (original_index, start, end), ordered by selection priority.
    let mut candidates: Vec<(usize, usize, usize)> = fixes
        .iter()
        .enumerate()
        .filter_map(|(i, d)| d.fix.as_ref().map(|f| (i, f.start, f.end)))
        .collect();
    candidates.sort_by(|&(ia, sa, _), &(ib, sb, _)| {
        severity_rank(fixes[ia].severity)
            .cmp(&severity_rank(fixes[ib].severity))
            .then(sb.cmp(&sa))
    });

    let mut winners: Vec<(usize, usize, usize)> = Vec::new(); // (orig_idx, start, end)
    let mut outcome_map: std::collections::HashMap<usize, FixOutcome> =
        std::collections::HashMap::new();

    for &(orig_idx, start, end) in &candidates {
        let conflict_with = winners
            .iter()
            .find(|(_, ws, we)| edits_conflict((start, end), (*ws, *we)));
        if let Some(&(blocking_idx, _, _)) = conflict_with {
            let blocking_rule = fixes[blocking_idx].rule_id.clone();
            outcome_map.insert(orig_idx, FixOutcome::Conflict { blocking_rule });
            continue;
        }
        // `start > end`, a non-char-boundary offset, or `end` past the body all
        // make the range unusable — `str` indexing would panic on any of them.
        // `get` is the single check that covers all three (iter-191).
        if body.get(start..end).is_none() {
            outcome_map.insert(
                orig_idx,
                FixOutcome::Conflict {
                    blocking_rule: "out-of-bounds".to_owned(),
                },
            );
            continue;
        }
        winners.push((orig_idx, start, end));
    }

    // Mutate the buffer in descending start order (see doc comment above).
    // Ties cannot involve a replacement any more (`edits_conflict` rejects an
    // insertion at a replacement's start); sorting the end descending as well
    // keeps the order total regardless.
    winners.sort_by_key(|&(_, start, end)| std::cmp::Reverse((start, end)));
    let mut result = body.to_owned();
    for (orig_idx, start, end) in winners {
        let replacement = fixes[orig_idx]
            .fix
            .as_ref()
            .map_or("", |f| f.replacement.as_str());
        // Re-validate against the *partially mutated* buffer: the winner
        // selection above checked the range against the pristine body, and
        // although descending-offset mutation keeps earlier ranges valid, a
        // `get` here is what makes that reasoning enforced rather than
        // assumed. `None` means the range is unusable — report it as a
        // conflict instead of panicking on a slice (iter-191).
        let Some(current) = result.get(start..end) else {
            outcome_map.insert(
                orig_idx,
                FixOutcome::Conflict {
                    blocking_rule: "out-of-bounds".to_owned(),
                },
            );
            continue;
        };
        if current == replacement {
            // Byte-for-byte no-op: nothing changed, don't count it as fixed.
            outcome_map.insert(orig_idx, FixOutcome::NoFix);
            continue;
        }
        result.replace_range(start..end, replacement);
        outcome_map.insert(orig_idx, FixOutcome::Applied);
    }

    let outcomes: Vec<FixOutcome> = (0..fixes.len())
        .map(|i| outcome_map.remove(&i).unwrap_or(FixOutcome::NoFix))
        .collect();

    (result, outcomes)
}

/// Whether two edits of half-open body ranges cannot both be applied.
///
/// Ranges that overlap conflict, and so does a zero-width insertion at the
/// start of a non-empty replacement: the plain overlap test calls them
/// disjoint, yet whichever is applied second lands inside text the first one
/// moved. Applied insert-first, the replacement then covered the inserted
/// bytes and left the tail of the original range behind as a stray line
/// (L-1, iteration 321). An insertion at a replacement's *end* is fine — it
/// sits after the replaced bytes in either order.
fn edits_conflict(a: (usize, usize), b: (usize, usize)) -> bool {
    let overlap = a.0 < b.1 && a.1 > b.0;
    let insert_at_start = |insert: (usize, usize), other: (usize, usize)| {
        insert.0 == insert.1 && other.1 > other.0 && insert.0 == other.0
    };
    overlap || insert_at_start(a, b) || insert_at_start(b, a)
}

/// Find the byte offset where the document body starts (after the closing `---` line).
/// Returns 0 if no frontmatter is found.
///
/// The opening check mirrors the shared frontmatter policy (iter-158 C-1):
/// an optional single UTF-8 BOM, then a line that is exactly `---` — so lint
/// splits BOM-prefixed files the same way the read/write paths do instead of
/// treating the whole file as body.
pub(super) fn find_body_start(content: &str) -> usize {
    hyalo_core::frontmatter::DocumentFrame::parse(content).map_or(0, |frame| frame.body_offset())
}

/// Compute the 1-based file line number on which the body begins, given the
/// byte offset returned by [`find_body_start`]. Used so OKF findings report
/// file-absolute line numbers rather than body-relative ones.
pub(super) fn find_body_line_offset(content: &str, body_start: usize) -> usize {
    // Line 1 when there is no frontmatter; otherwise 1 + number of newlines
    // consumed by the frontmatter block.
    1 + content[..body_start]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
}

/// Check whether any schema type declares `status` as an enum with `completed`.
pub(super) fn schema_has_completed_status(schema: &SchemaConfig) -> bool {
    // Check default schema.
    if has_completed_in_type(&schema.default_schema().properties) {
        return true;
    }
    // Check all typed schemas.
    for ts in schema.types.values() {
        if has_completed_in_type(&ts.properties) {
            return true;
        }
    }
    false
}

pub(super) fn has_completed_in_type(
    props: &std::collections::HashMap<String, PropertyConstraint>,
) -> bool {
    if let Some(PropertyConstraint::Enum { values }) = props.get("status") {
        return values.iter().any(|v| v == "completed");
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyalo_mdlint::{DiagFix, DiagSeverity, Diagnostic};

    fn diag(rule: &str, start: usize, end: usize, replacement: &str) -> Diagnostic {
        Diagnostic {
            rule_id: rule.to_owned(),
            rule_name: rule.to_owned(),
            message: String::new(),
            line: 1,
            column: 1,
            severity: DiagSeverity::Warn,
            fix: Some(DiagFix {
                description: String::new(),
                start,
                end,
                replacement: replacement.to_owned(),
            }),
        }
    }

    #[test]
    fn an_insertion_at_a_replacements_start_conflicts_with_it() {
        assert!(edits_conflict((7, 7), (7, 20)));
        assert!(edits_conflict((7, 20), (7, 7)));
        assert!(
            edits_conflict((5, 9), (8, 12)),
            "plain overlap still conflicts"
        );
    }

    #[test]
    fn disjoint_and_end_adjacent_edits_do_not_conflict() {
        assert!(
            !edits_conflict((7, 20), (20, 20)),
            "insert at the end is safe"
        );
        assert!(
            !edits_conflict((7, 7), (7, 7)),
            "two insertions can coexist"
        );
        assert!(!edits_conflict((0, 3), (3, 6)));
    }

    /// L-1: MD022 inserts a newline at 7 while MD023 replaces `[7, 20)`. The
    /// old overlap test let both win and left a stray `d` behind.
    #[test]
    fn insert_and_replace_at_one_offset_report_a_conflict_not_corruption() {
        let body = "# Wide\n ## Indented\ntext\n";
        let insert = diag("MD022", 7, 7, "\n");
        let replace = diag("MD023", 7, 20, "## Indented\n");
        let (fixed, outcomes) = apply_body_fixes(body, &[&insert, &replace]);
        let applied = outcomes
            .iter()
            .filter(|o| matches!(o, FixOutcome::Applied))
            .count();
        let conflicts = outcomes
            .iter()
            .filter(|o| matches!(o, FixOutcome::Conflict { .. }))
            .count();
        assert_eq!((applied, conflicts), (1, 1), "{outcomes:?}");
        assert!(
            fixed == "# Wide\n\n ## Indented\ntext\n" || fixed == "# Wide\n## Indented\ntext\n",
            "exactly one of the two edits applied: {fixed:?}"
        );
    }
}
