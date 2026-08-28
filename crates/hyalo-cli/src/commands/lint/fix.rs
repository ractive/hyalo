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
        let conflict_with = winners.iter().find(|(_, ws, we)| start < *we && end > *ws);
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
    winners.sort_by_key(|&(_, start, _)| std::cmp::Reverse(start));
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

/// Find the byte offset where the document body starts (after the closing `---` line).
/// Returns 0 if no frontmatter is found.
///
/// The opening check mirrors the shared frontmatter policy (iter-158 C-1):
/// an optional single UTF-8 BOM, then a line that is exactly `---` — so lint
/// splits BOM-prefixed files the same way the read/write paths do instead of
/// treating the whole file as body.
pub(super) fn find_body_start(content: &str) -> usize {
    let rest = content.strip_prefix('\u{feff}').unwrap_or(content);
    if !(rest.starts_with("---\n") || rest.starts_with("---\r\n") || rest == "---") {
        return 0;
    }
    // Find the second `---` delimiter.
    let after_first = content.find('\n').map_or(content.len(), |i| i + 1);
    let rest = &content[after_first..];
    if let Some(pos) = rest.find("\n---") {
        // Skip past `\n---\n` or `\n---` at end.
        let abs = after_first + pos + 4; // skip \n---
        // Skip the terminator after the closing `---` — LF or CRLF, so the
        // body slice never starts with a stray carriage return on CRLF files.
        let bytes = content.as_bytes();
        if bytes.get(abs) == Some(&b'\r') && bytes.get(abs + 1) == Some(&b'\n') {
            abs + 2
        } else if bytes.get(abs) == Some(&b'\n') {
            abs + 1
        } else {
            abs
        }
    } else {
        // No closing delimiter — treat whole file as body.
        0
    }
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
