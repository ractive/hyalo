//! The vault-level extended lint pass and its result accounting.
//!
//! Split out of the single 4,005-line `commands/lint.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `commands::lint::...` paths and behaviour are
//! unchanged.

use super::{
    BodyViolation, ConflictEntry, ExtFileLintFixResult, ExtFileLintResult, ExtLintFixOutput,
    ExtLintOptions, ExtLintOutput, FixedGroup, RuleGroup, lint_one_file_extended,
    schema_has_completed_status,
};
use crate::output::{CommandOutcome, Format};
use anyhow::{Context, Result};
use hyalo_core::schema::SchemaConfig;
use hyalo_mdlint::schema::{FileFixResult, FixAction, FixMode, LintCounts};
use std::collections::HashSet;

/// How many per-conflict explanation lines one file lists before the output
/// switches to `… and N more (use --detailed)` (iteration 263, UX-16).
///
/// A file with a pathological overlap can produce hundreds of conflicts;
/// listing them all would bury the fixed/remaining sections that describe what
/// actually changed. `--detailed` prints the full list.
const MAX_CONFLICT_LINES: usize = 20;

/// Run the extended lint (frontmatter + body) and return the new output shape.
#[allow(clippy::too_many_arguments)]
pub fn lint_files_extended(
    files: &[(std::path::PathBuf, String)],
    schema: &SchemaConfig,
    md_lint_engine: &hyalo_mdlint::HyaloLintEngine,
    md_lint_config: &hyalo_mdlint::LintConfig,
    opts: &mut ExtLintOptions<'_>,
) -> Result<(CommandOutcome, LintCounts)> {
    #[cfg(not(miri))]
    use rayon::prelude::*;

    // Build rule filter list
    let rule_filter: Vec<String> = match (opts.rule_filter, opts.rule_prefix) {
        (Some(rule), _) => vec![rule.to_owned()],
        // M-10: prefix matching is case-insensitive so `--rule-prefix hyalo`
        // selects the same family as `--rule-prefix HYALO`.
        (None, Some(prefix)) => {
            let mut ids: Vec<String> = md_lint_engine
                .rules_matching_prefix_ci(prefix)
                .iter()
                .map(|e| e.id.clone())
                .collect();
            // UX-5 (iter-274): the schema pass is selectable by prefix too. It
            // is not in the markdown catalog, so it has to be added here.
            if super::SCHEMA_PSEUDO_RULE
                .to_ascii_lowercase()
                .starts_with(&prefix.to_ascii_lowercase())
            {
                ids.push(super::SCHEMA_PSEUDO_RULE.to_owned());
            }
            ids
        }
        (None, None) => vec![],
    };

    // Determine if schema has `status: completed` in any type.
    let schema_has_completed = schema_has_completed_status(schema);

    let strict = opts.strict;
    let okf_profile = opts.okf_profile;
    let madr_profile = opts.madr_profile;
    let skills_profile = opts.skills_profile;
    let changelog_profile = opts.changelog_profile;
    let vault_dir = opts.vault_dir;
    let case_insensitive = opts.case_insensitive;
    // Shared HYALO006 context (built once in dispatch); borrowed by every
    // worker. `None` disables the rule for this run.
    let link_ctx = opts.link_lint_ctx.as_ref();

    // Process files in parallel. Each worker lints one file.
    let lint_file = |(full_path, rel_path): &(std::path::PathBuf, String)| {
        lint_one_file_extended(
            full_path,
            rel_path,
            schema,
            md_lint_engine,
            md_lint_config,
            &rule_filter,
            schema_has_completed,
            opts.fix,
            opts.fix_rules,
            opts.max_per_rule,
            strict,
            okf_profile,
            madr_profile,
            skills_profile,
            changelog_profile,
            vault_dir,
            case_insensitive,
            link_ctx,
        )
    };
    #[cfg(not(miri))]
    let per_file: Vec<Result<PerFileLintResult>> = files.par_iter().map(lint_file).collect();
    #[cfg(miri)]
    let per_file: Vec<Result<PerFileLintResult>> = files.iter().map(lint_file).collect();

    // Merge results serially.
    let mut all_results: Vec<PerFileLintResult> = Vec::with_capacity(files.len());
    let mut modified_files: Vec<String> = Vec::new();

    for result in per_file {
        let mut r = result?;
        if r.body_modified {
            modified_files.push(r.rel_path.clone());
            r.body_modified = false;
        }
        all_results.push(r);
    }

    // Patch index for body-modified files (ARCH-3, iter-226): through the
    // MutationJournal so entry AND link graph stay current, flushed once.
    if !modified_files.is_empty() {
        let mut journal =
            crate::commands::journal::MutationJournal::new(opts.snapshot_index, opts.index_path);
        journal.rescan_modified(opts.vault_dir, &modified_files)?;
        journal.flush()?;
    }

    let is_fix_mode = matches!(opts.fix, FixMode::Apply | FixMode::DryRun);

    // Sort by total violations descending (worst offenders first).
    // UX-4 (dogfood v0.20.0): error-carrying files sort ahead of
    // warning-only files (then by total violations) — the display cap
    // drops the least severe files first, so "4 errors" hidden behind a
    // 50-file listing of pure warnings can no longer happen.
    all_results.sort_by_key(|r| {
        let (errors, _) = count_file_errors_warnings(r, is_fix_mode);
        (
            std::cmp::Reverse(errors),
            std::cmp::Reverse(r.total_violations),
        )
    });

    // Cap files.
    let total_files_with_violations = all_results
        .iter()
        .filter(|r| r.total_violations > 0)
        .count();
    let files_checked_total = all_results.len();
    // `files_truncated` describes the *listed* files, not the examined ones.
    // `all_results` is sorted worst-first, so the display cap only ever drops a
    // violating file once there are more of those than `max_files`. Deriving it
    // from `files_checked > max_files` (as this did before iter-210 BUG-6)
    // false-positived on every clean vault larger than the limit.
    let files_truncated = total_files_with_violations > opts.max_files;

    // Authoritative error/warning totals, computed over EVERY result *before*
    // the display list is capped to `max_files`. The per-file display loops
    // below only build the (possibly truncated) `files[]` array — they must not
    // be the source of the summary counters, or a `--limit`/`max_files` cap
    // would under-report errors and let a corrupt vault exit 0 (ff-rdp B5).
    let (authoritative_errors, authoritative_warnings) =
        count_errors_warnings(&all_results, is_fix_mode);

    // Whole-run violation total and distinct firing rules, computed over EVERY
    // result *before* the display cap, exactly like the error/warning totals
    // above. Deriving them from the capped display loops (pre-iter-210) made
    // JSON `total` describe a different run than `errors` + `warnings` — on a
    // large vault the two disagreed by an order of magnitude (BUG-6).
    let authoritative_total: usize = all_results.iter().map(|r| r.total_violations).sum();
    let authoritative_rules_fired = {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for r in &all_results {
            for rule_id in r.violations_by_rule.keys() {
                seen.insert(rule_id.as_str());
            }
        }
        seen.len()
    };

    // Fix-mode totals, computed over EVERY result *before* the display cap,
    // exactly like `authoritative_total`/`authoritative_rules_fired` above.
    // The per-file display loop below only builds the (possibly truncated)
    // `files[]` array for fix-mode too — deriving `total_fixed` /
    // `total_remaining` / `total_conflicts` from it (as this did before
    // iter-218) made `--limit` silently understate `conflicts`, hiding
    // apply-time surprises from anyone who dry-ran at the default limit
    // (NEW-6).
    let (authoritative_total_fixed, authoritative_total_remaining, authoritative_total_conflicts) =
        if is_fix_mode {
            all_results
                .iter()
                .fold((0, 0, 0), |(fixed, remaining, conflicts), r| {
                    let (f, rem, c) = fix_mode_file_totals(r);
                    (fixed + f, remaining + rem, conflicts + c)
                })
        } else {
            (0, 0, 0)
        };

    // Cap the display list (0 == unlimited is normalized to `usize::MAX` by the
    // caller, so truncation here is a no-op for `--limit 0`).
    all_results.truncate(opts.max_files);

    let val = if is_fix_mode {
        // -------------------------------------------------------------------
        // Fix-mode output: fixed_groups / remaining_groups / conflicts shape.
        // -------------------------------------------------------------------
        let mut output_fix_files: Vec<ExtFileLintFixResult> = Vec::new();

        for r in &all_results {
            // Build a set of (rule_id, order-within-rule) → FixOutcome from
            // body_fix_outcomes.  We use a Vec<(rule, outcome)> indexed by the
            // position among fixable diagnostics (same order as fixable_indices).
            // Easier to just build maps indexed by rule.
            let mut applied_by_rule: indexmap::IndexMap<String, Vec<BodyViolation>> =
                indexmap::IndexMap::new();
            // Keyed by (rule, line) rather than by rule alone (UX-16): two
            // violations of the same rule can lose to two different blocking
            // rules on two different lines, and collapsing them to one entry
            // was exactly why `conflicts N` could not be explained.
            let mut conflict_by_violation: indexmap::IndexMap<(String, usize), String> =
                indexmap::IndexMap::new();

            for (rule_id, line, outcome) in &r.body_fix_outcomes {
                match outcome {
                    FixOutcome::Applied => {
                        applied_by_rule.entry(rule_id.clone()).or_default();
                    }
                    FixOutcome::Conflict { blocking_rule } => {
                        conflict_by_violation
                            .entry((rule_id.clone(), *line))
                            .or_insert_with(|| blocking_rule.clone());
                    }
                    FixOutcome::NoFix => {}
                }
            }

            // SCHEMA fixed count: derived from re-validating post-fix
            // properties (resolved = before - after). Falls back to 0 when
            // the post-fix re-validation didn't run.
            let schema_before = r
                .violations_by_rule
                .get("SCHEMA")
                .map_or(0, std::vec::Vec::len);
            let schema_after = r
                .post_fix_schema_remaining
                .as_ref()
                .map_or(schema_before, std::vec::Vec::len);
            let schema_fix_count = schema_before.saturating_sub(schema_after);

            // fixed_groups: rules with at least one applied fix + SCHEMA if fixed.
            let mut fixed_groups: Vec<FixedGroup> = Vec::new();
            if schema_fix_count > 0 {
                fixed_groups.push(FixedGroup {
                    rule: "SCHEMA".to_owned(),
                    count: schema_fix_count,
                    violations: Vec::new(),
                });
            }
            for (rule_id, _) in &applied_by_rule {
                // Surface only diagnostics whose fix was actually Applied —
                // not the entire `violations_by_rule[rule_id]` set, which
                // also contains conflicts and not-selected entries for the
                // same rule.
                let viols: Vec<BodyViolation> = r
                    .violations_by_rule
                    .get(rule_id)
                    .map(|vs| {
                        vs.iter()
                            .filter(|v| v.fixed)
                            .take(opts.max_per_rule)
                            .map(|v| BodyViolation {
                                line: v.line,
                                column: v.column,
                                severity: v.severity.clone(),
                                message: v.message.clone(),
                                fix: v.fix.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let count = r
                    .violations_by_rule
                    .get(rule_id)
                    .map_or(0, |vs| vs.iter().filter(|v| v.fixed).count());
                if count > 0 {
                    fixed_groups.push(FixedGroup {
                        rule: rule_id.clone(),
                        count,
                        violations: viols,
                    });
                }
            }

            // remaining_groups: violations not fixed.
            let mut remaining_groups: Vec<RuleGroup> = Vec::new();
            for (rule_id, violations) in &r.violations_by_rule {
                // SCHEMA: post-fix remaining set comes from re-running
                // `validate_properties` against the mutated frontmatter,
                // not from a fix_actions count heuristic.
                if rule_id == "SCHEMA" {
                    let remaining_owned: Vec<InternalViolation> =
                        if let Some(post) = r.post_fix_schema_remaining.as_ref() {
                            post.iter()
                                .map(|v| InternalViolation {
                                    line: v.line,
                                    column: v.column,
                                    message: v.message.clone(),
                                    severity: v.severity.clone(),
                                    fix: v.fix.clone(),
                                    fixed: v.fixed,
                                    autofixable: v.autofixable,
                                })
                                .collect()
                        } else {
                            // No fix-mode SCHEMA pass ran (e.g., --fix-rule
                            // filtered SCHEMA out). All originals remain.
                            violations
                                .iter()
                                .map(|v| InternalViolation {
                                    line: v.line,
                                    column: v.column,
                                    message: v.message.clone(),
                                    severity: v.severity.clone(),
                                    fix: v.fix.clone(),
                                    fixed: v.fixed,
                                    autofixable: v.autofixable,
                                })
                                .collect()
                        };
                    let remaining = remaining_owned.len();
                    if remaining == 0 {
                        continue;
                    }
                    let shown = remaining.min(opts.max_per_rule);
                    let truncated = remaining > shown;
                    let body_violations = remaining_owned
                        .iter()
                        .take(shown)
                        .map(|v| BodyViolation {
                            line: v.line,
                            column: v.column,
                            severity: v.severity.clone(),
                            message: v.message.clone(),
                            fix: v.fix.clone(),
                        })
                        .collect();
                    remaining_groups.push(RuleGroup {
                        rule: rule_id.clone(),
                        count: remaining,
                        shown,
                        truncated,
                        severity: group_severity(&remaining_owned),
                        // Autofixable only if at least one remaining SCHEMA
                        // violation could still be fixed (mapl BUG-3).
                        autofixable: remaining_owned
                            .iter()
                            .any(|v| v.autofixable.unwrap_or(true)),
                        violations: body_violations,
                    });
                    continue;
                }

                // Body rules: filter out violations that were actually fixed.
                let remaining_violations: Vec<&InternalViolation> =
                    violations.iter().filter(|v| !v.fixed).collect();
                let remaining_count = remaining_violations.len();
                if remaining_count == 0 {
                    continue;
                }

                let autofixable = md_lint_engine
                    .available_rules()
                    .iter()
                    .find(|e| &e.id == rule_id)
                    .is_some_and(|e| e.autofixable);
                let severity = remaining_violations
                    .first()
                    .map_or_else(|| "warn".to_owned(), |v| v.severity.clone());
                let shown = remaining_count.min(opts.max_per_rule);
                let truncated = remaining_count > shown;
                let body_violations = remaining_violations
                    .iter()
                    .take(shown)
                    .map(|v| BodyViolation {
                        line: v.line,
                        column: v.column,
                        severity: v.severity.clone(),
                        message: v.message.clone(),
                        fix: v.fix.clone(),
                    })
                    .collect();
                remaining_groups.push(RuleGroup {
                    rule: rule_id.clone(),
                    count: remaining_count,
                    shown,
                    truncated,
                    severity,
                    autofixable,
                    violations: body_violations,
                });
            }
            remaining_groups.sort_by_key(|g| std::cmp::Reverse(g.count));

            // conflicts: one entry per (rule, line) whose fix was skipped.
            let mut conflicts: Vec<ConflictEntry> = Vec::new();
            for ((rule_id, line), blocking_rule) in &conflict_by_violation {
                conflicts.push(ConflictEntry {
                    rule: rule_id.clone(),
                    line: *line,
                    reason: format!("range overlap with {blocking_rule}"),
                });
            }
            conflicts.sort_by(|a, b| a.rule.cmp(&b.rule).then(a.line.cmp(&b.line)));
            // The listing is capped the same way the remaining-violation
            // listing is; `--detailed` lifts it. `conflicts_total` keeps the
            // "… and N more" line honest.
            let conflicts_total = conflicts.len();
            if !opts.detailed && conflicts_total > MAX_CONFLICT_LINES {
                conflicts.truncate(MAX_CONFLICT_LINES);
            }

            if !fixed_groups.is_empty() || !remaining_groups.is_empty() || !conflicts.is_empty() {
                output_fix_files.push(ExtFileLintFixResult {
                    file: r.rel_path.clone(),
                    doc_type: r.doc_type.clone(),
                    fixed_groups,
                    remaining_groups,
                    conflicts,
                    conflicts_total,
                });
            }
        }

        // UX-13: per-rule fixed counts, aggregated over every file — not just
        // the ones that survived the display cap, so the map agrees with
        // `total_fixed` rather than with what happens to be printed.
        let mut rules_fixed: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for f in &output_fix_files {
            for g in &f.fixed_groups {
                *rules_fixed.entry(g.rule.clone()).or_insert(0) += g.count;
            }
        }

        let fix_output = ExtLintFixOutput {
            files: output_fix_files,
            total_fixed: authoritative_total_fixed,
            total_remaining: authoritative_total_remaining,
            total_conflicts: authoritative_total_conflicts,
            rules_fired: authoritative_rules_fired,
            rules_fixed,
            files_with_violations: total_files_with_violations,
            files_checked: files_checked_total,
            files_truncated,
            remaining_errors: authoritative_errors,
            remaining_warnings: authoritative_warnings,
            dry_run: matches!(opts.fix, FixMode::DryRun),
        };
        serde_json::to_value(&fix_output).context("failed to serialize fix lint output")?
    } else {
        // -------------------------------------------------------------------
        // Read-only output: unchanged rule_groups shape.
        // -------------------------------------------------------------------
        let mut output_files: Vec<ExtFileLintResult> = Vec::new();
        let mut all_fix_actions: Vec<FileFixResult> = Vec::new();

        for r in &all_results {
            if !r.fix_actions.is_empty() {
                all_fix_actions.push(FileFixResult {
                    file: r.rel_path.clone(),
                    actions: r.fix_actions.clone(),
                });
            }
            if r.total_violations == 0 {
                continue;
            }
            let mut rule_groups: Vec<RuleGroup> = Vec::new();
            for (rule_id, violations) in &r.violations_by_rule {
                let count = violations.len();

                let autofixable = if rule_id == "SCHEMA" {
                    // The SCHEMA group folds several checks; it is autofixable
                    // only if at least one member is (per-violation
                    // `autofixable`). A group made up entirely of
                    // non-synthesizable missing-required violations reports
                    // `false` (mapl BUG-3).
                    violations.iter().any(|v| v.autofixable.unwrap_or(true))
                } else {
                    md_lint_engine
                        .available_rules()
                        .iter()
                        .find(|e| &e.id == rule_id)
                        .is_some_and(|e| e.autofixable)
                };
                let severity = group_severity(violations);

                let shown = if opts.detailed {
                    violations.len()
                } else {
                    violations.len().min(opts.max_per_rule)
                };
                let truncated = count > shown;
                let body_violations: Vec<BodyViolation> = violations[..shown]
                    .iter()
                    .map(|v| BodyViolation {
                        line: v.line,
                        column: v.column,
                        severity: v.severity.clone(),
                        message: v.message.clone(),
                        fix: v.fix.clone(),
                    })
                    .collect();

                rule_groups.push(RuleGroup {
                    rule: rule_id.clone(),
                    count,
                    shown,
                    truncated,
                    severity,
                    autofixable,
                    violations: body_violations,
                });
            }
            rule_groups.sort_by_key(|g| std::cmp::Reverse(g.count));
            output_files.push(ExtFileLintResult {
                file: r.rel_path.clone(),
                doc_type: r.doc_type.clone(),
                rule_groups,
            });
        }

        let output = ExtLintOutput {
            files: output_files,
            violations: authoritative_total,
            rules_fired: authoritative_rules_fired,
            files_with_violations: total_files_with_violations,
            files_checked: files_checked_total,
            files_truncated,
            errors: authoritative_errors,
            warnings: authoritative_warnings,
            files_ignored: opts.files_ignored,
            dry_run: false,
            fixes: all_fix_actions,
        };
        serde_json::to_value(&output).context("failed to serialize extended lint output")?
    };

    // Counts drive the exit code and `hyalo summary`. Use the authoritative
    // pre-truncation totals so a `--limit`/`max_files` cap can never mask an
    // error (which would let a corrupt vault exit 0). `total_errors` /
    // `total_warnings` accumulated by the display loops only ever cover the
    // capped `files[]` slice and would under-report.
    let counts = LintCounts {
        errors: authoritative_errors,
        warnings: authoritative_warnings,
        files_with_issues: total_files_with_violations,
    };

    let outcome = CommandOutcome::success_with_total(
        crate::output::format_success(Format::Json, &val),
        total_files_with_violations as u64,
    );

    Ok((outcome, counts))
}

/// Per-file violation entry (internal).
pub(super) struct InternalViolation {
    pub(super) line: usize,
    pub(super) column: usize,
    pub(super) message: String,
    pub(super) severity: String,
    pub(super) fix: Option<serde_json::Value>,
    /// True when this body diagnostic's fix was successfully applied during
    /// the current fix-mode run. Always `false` for read-only and frontmatter
    /// (SCHEMA) violations — frontmatter fixes are tracked separately via
    /// `fix_actions`.
    pub(super) fixed: bool,
    /// Whether `--fix` could resolve THIS specific violation. Meaningful for
    /// SCHEMA violations, whose group otherwise reports a single coarse
    /// `autofixable`: a "missing required property" with no declared default
    /// cannot be synthesized (mapl BUG-3), so it is `false` here while a
    /// missing-but-defaulted property is `true`. `None` = use the group/rule
    /// default (body rules, where autofixability is a property of the rule).
    pub(super) autofixable: Option<bool>,
}

/// Severity label for a group of violations under one rule id.
///
/// Most rule groups hold violations that all share one severity (hyalo
/// assigns severity per rule, not per violation), but the synthetic
/// `"SCHEMA"` group folds together several distinct checks (missing
/// required field, undeclared property, missing type, ...) that can mix
/// error and warn — using `violations.first()` there mislabels the whole
/// group whenever a warn happens to be first. Returns the max severity
/// across the group instead, which is correct for both the uniform and
/// mixed cases.
pub(super) fn group_severity(violations: &[InternalViolation]) -> String {
    if violations.iter().any(|v| v.severity == "error") {
        "error".to_owned()
    } else {
        "warn".to_owned()
    }
}

/// Outcome for a single diagnostic's fix attempt (internal, used in fix-mode).
#[derive(Debug)]
pub(super) enum FixOutcome {
    /// Fix was applied (or would be in DryRun).
    Applied,
    /// Fix conflicted with another fix's range.
    Conflict { blocking_rule: String },
    /// Diagnostic had no fix, or fix was not selected.
    NoFix,
}

/// Per-file lint result (internal, before grouping).
pub(super) struct PerFileLintResult {
    pub(super) rel_path: String,
    /// Frontmatter `type:` discriminator, if declared. Propagated into
    /// `ExtFileLintResult.doc_type` for the hint layer.
    pub(super) doc_type: Option<String>,
    pub(super) violations_by_rule: indexmap::IndexMap<String, Vec<InternalViolation>>,
    pub(super) total_violations: usize,
    pub(super) body_modified: bool,
    /// Frontmatter fix actions applied or previewed.
    pub(super) fix_actions: Vec<FixAction>,
    /// One `(rule_id, line, outcome)` per fixable diagnostic the fix loop
    /// considered, in pass order. The line is the diagnostic's own 1-based
    /// line, carried so a conflict can be explained as `<rule> line <n>`
    /// (iteration 263). Only populated in fix-mode.
    pub(super) body_fix_outcomes: Vec<(String, usize, FixOutcome)>,
    /// SCHEMA (frontmatter) violations remaining *after* applying fixes,
    /// computed by re-running `validate_properties` against the mutated
    /// frontmatter. `Some(vec![])` means all SCHEMA violations were resolved;
    /// `None` means fix-mode was off or no SCHEMA pass ran. Body rules use
    /// `InternalViolation.fixed` instead.
    pub(super) post_fix_schema_remaining: Option<Vec<InternalViolation>>,
}

/// Compute one file's fix-mode totals — (fixed, remaining, conflicts) —
/// without building the display groups (`fixed_groups`/`remaining_groups`/
/// `conflicts`). Mirrors the per-file accounting the display loop in
/// [`run_ext_lint`] does inline, but is called separately over the *full*
/// `all_results` before the `--limit` display cap is applied, so
/// `total_fixed`/`total_remaining`/`total_conflicts` describe the whole run
/// even when the listing is truncated (iter-218 NEW-6).
pub(super) fn fix_mode_file_totals(r: &PerFileLintResult) -> (usize, usize, usize) {
    let mut applied_rules: HashSet<&str> = HashSet::new();
    // Keyed by (rule, line), matching the per-violation `conflicts` array the
    // display loop builds (iteration 263). Counting distinct *rules* here
    // made the summary's `conflicts N` smaller than the number of `conflict`
    // lines printed above it, which reads as an accounting bug.
    let mut conflict_violations: HashSet<(&str, usize)> = HashSet::new();
    for (rule_id, line, outcome) in &r.body_fix_outcomes {
        match outcome {
            FixOutcome::Applied => {
                applied_rules.insert(rule_id.as_str());
            }
            FixOutcome::Conflict { .. } => {
                conflict_violations.insert((rule_id.as_str(), *line));
            }
            FixOutcome::NoFix => {}
        }
    }

    let schema_before = r
        .violations_by_rule
        .get("SCHEMA")
        .map_or(0, std::vec::Vec::len);
    let schema_after = r
        .post_fix_schema_remaining
        .as_ref()
        .map_or(schema_before, std::vec::Vec::len);
    let mut fixed = schema_before.saturating_sub(schema_after);
    let mut remaining = 0usize;

    for (rule_id, violations) in &r.violations_by_rule {
        if rule_id == "SCHEMA" {
            remaining += schema_after;
            continue;
        }
        if applied_rules.contains(rule_id.as_str()) {
            fixed += violations.iter().filter(|v| v.fixed).count();
        }
        remaining += violations.iter().filter(|v| !v.fixed).count();
    }

    (fixed, remaining, conflict_violations.len())
}

/// Count the error- and warning-severity violations across every result,
/// independent of the `max_files` display cap.
///
/// In read-only mode this is simply every violation, reported as
/// [`ExtLintOutput::errors`]/`::warnings`. In fix mode it counts the
/// violations that *remain* after fixing — SCHEMA remainders come from
/// `post_fix_schema_remaining`, body remainders from `!v.fixed` — reported as
/// [`ExtLintFixOutput::remaining_errors`]/`::remaining_warnings` (renamed
/// from `errors`/`warnings` in iter-218 NEW-6b, since this mode's count means
/// something different from the read-only mode's despite the pre-rename JSON
/// using the same key for both). Either way, the exit code they drive
/// reflects the true whole-vault state, not just the first `max_files`
/// shown.
pub(super) fn count_errors_warnings(
    results: &[PerFileLintResult],
    is_fix_mode: bool,
) -> (usize, usize) {
    results
        .iter()
        .map(|r| count_file_errors_warnings(r, is_fix_mode))
        .fold((0, 0), |(e, w), (fe, fw)| (e + fe, w + fw))
}

/// One file's contribution to [`count_errors_warnings`] — (errors, warnings).
///
/// Also used by the display ordering (UX-4, dogfood v0.20.0): files that
/// carry *errors* sort ahead of warning-only files so a display cap can
/// never push the run's errors out of the listed slice.
pub(super) fn count_file_errors_warnings(
    r: &PerFileLintResult,
    is_fix_mode: bool,
) -> (usize, usize) {
    let mut errors = 0usize;
    let mut warnings = 0usize;
    let mut tally = |severity: &str| {
        if severity == "error" {
            errors += 1;
        } else {
            warnings += 1;
        }
    };
    for (rule_id, violations) in &r.violations_by_rule {
        if is_fix_mode {
            if rule_id == "SCHEMA" {
                // Post-fix SCHEMA remainder, or all originals if no fix pass ran.
                let remaining = r.post_fix_schema_remaining.as_ref().unwrap_or(violations);
                for v in remaining {
                    tally(&v.severity);
                }
            } else {
                for v in violations.iter().filter(|v| !v.fixed) {
                    tally(&v.severity);
                }
            }
        } else {
            for v in violations {
                tally(&v.severity);
            }
        }
    }
    (errors, warnings)
}

/// Record a `--fix` write-path failure (TOCTOU or I/O) as a `FILE`-rule
/// error violation, so it shows up in the report instead of only aborting
/// via `?` (Finding 4, review round on PR #254 — the write-path sibling of
/// M-1's read-path fix).
pub(super) fn push_fix_write_error_violation(
    violations_by_rule: &mut indexmap::IndexMap<String, Vec<InternalViolation>>,
    message: &str,
) {
    violations_by_rule
        .entry("FILE".to_owned())
        .or_default()
        .push(InternalViolation {
            line: 1,
            column: 1,
            message: message.to_owned(),
            severity: "error".to_owned(),
            fix: None,
            fixed: false,
            autofixable: None,
        });
}
