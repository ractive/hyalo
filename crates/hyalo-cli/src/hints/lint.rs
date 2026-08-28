//! Hints for `lint`, including per-rule drill-downs.
//!
//! Split out of the single 5,059-line `hints.rs` in iteration 247 (deep-review
//! hotspot). This is a file split only: the items keep the visibility they had
//! inside the one module, so `hints::...` paths and behaviour are unchanged.

use super::{
    Hint, HintBuilder, HintContext, MAX_HINTS, PARSE_ERROR_PREFIX, build_command_no_glob,
    build_command_with_glob_and_files,
};

/// Ratio threshold for rule dominance (UX-2).
pub(super) const RULE_DOMINANCE_RATIO: f64 = 0.5;
/// Absolute minimum violations for rule dominance (UX-2).
pub(super) const RULE_DOMINANCE_MIN: usize = 50;

/// Return a per-rule hint entry for HYALO001 or HYALO002, or `None` for other rules.
pub(super) fn per_rule_hint(
    ctx: &HintContext,
    rule_id: &str,
    worst_file: Option<&str>,
) -> Option<Hint> {
    match rule_id {
        "HYALO001" => Some(Hint::new(
            "Auto-fix HYALO001 violations",
            build_lint_with_filter_flags(ctx, &["lint", "--rule", "HYALO001", "--fix"]),
        )),
        "HYALO002" => worst_file.map(|file| {
            // Use `--file <path>` rather than a positional, since `find`'s
            // positional argument is the search pattern.
            Hint::new(
                format!("See open tasks in {file}"),
                build_command_no_glob(ctx, &["find", "--task", "todo", "--file", file]),
            )
        }),
        _ => None,
    }
}

/// Build a lint command that preserves `--rule`, `--rule-prefix`, `--fix-rule`, glob, and
/// file targets from the current context, then appends `args`.
pub(super) fn build_lint_with_filter_flags(ctx: &HintContext, args: &[&str]) -> String {
    let mut b = HintBuilder::empty();
    for arg in args {
        b.push_quoted(arg);
    }
    // Preserve rule/prefix/fix-rule filters from the original invocation.
    if let Some(rule) = &ctx.lint_rule
        && !args.contains(&"--rule")
    {
        b.push_raw("--rule");
        b.push_quoted(rule);
    }
    if let Some(prefix) = &ctx.lint_rule_prefix {
        b.push_raw("--rule-prefix");
        b.push_quoted(prefix);
    }
    for fr in &ctx.lint_fix_rules {
        b.push_raw("--fix-rule");
        b.push_quoted(fr);
    }
    for glob in &ctx.glob {
        b.push_raw("--glob");
        b.push_quoted(glob);
    }
    for ft in &ctx.file_targets {
        b.push_quoted(ft);
    }
    b.finish(ctx)
}

/// Accumulate rule violation counts from a named array field in a file JSON object.
pub(super) fn accumulate_rule_groups(
    file: &serde_json::Value,
    key: &str,
    totals: &mut std::collections::HashMap<String, usize>,
) {
    if let Some(groups) = file.get(key).and_then(|rg| rg.as_array()) {
        for group in groups {
            if let (Some(rule), Some(count)) = (
                group.get("rule").and_then(serde_json::Value::as_str),
                group.get("count").and_then(serde_json::Value::as_u64),
            ) {
                *totals.entry(rule.to_owned()).or_default() +=
                    usize::try_from(count).unwrap_or(usize::MAX);
            }
        }
    }
}

/// Collect per-rule violation counts across all files, scanning both `rule_groups` (read-only)
/// and `fixed_groups` + `remaining_groups` (fix-mode).
pub(super) fn collect_rule_totals(
    data: &serde_json::Value,
) -> std::collections::HashMap<String, usize> {
    let mut totals: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let Some(files) = data.get("files").and_then(|f| f.as_array()) else {
        return totals;
    };
    for file in files {
        // Read-only shape.
        accumulate_rule_groups(file, "rule_groups", &mut totals);
        // Fix-mode shapes (count all including fixed for dominance analysis).
        accumulate_rule_groups(file, "remaining_groups", &mut totals);
        accumulate_rule_groups(file, "fixed_groups", &mut totals);
    }
    totals
}

pub(super) fn hints_for_lint(
    ctx: &HintContext,
    data: &serde_json::Value,
    _total: Option<u64>,
) -> Vec<Hint> {
    let mut hints: Vec<Hint> = Vec::new();

    let is_fix_mode = ctx.lint_is_fix;
    let is_dry_run = ctx.dry_run
        || data
            .get("dry_run")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

    // -----------------------------------------------------------------------
    // Show-all hint when output is truncated.
    // -----------------------------------------------------------------------
    // Suppressed after `--fix` apply: the `files_with_issues` count reflects
    // the pre-fix state, but the hinted `hyalo lint --limit 0` drops `--fix`
    // and re-lints the now-fixed vault, so the count no longer matches (BUG-9
    // carryover / "Post-`lint --fix` output drops the stale hint"). Read-only
    // and dry-run modes keep it because their counts are still accurate.
    let is_apply = is_fix_mode && !is_dry_run;
    let is_limited = data
        .get("files_truncated")
        .or_else(|| data.get("limited"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !ctx.has_limit && is_limited && !is_apply {
        let total_violations = data
            .get("files_with_violations")
            .or_else(|| data.get("files_with_issues"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        // UX-4 (dogfood v0.20.0): when errors exist anywhere in the run but
        // the truncated `files[]` slice shows none (or not all) of them,
        // say so in the hint — a "4 errors" count in the summary line with
        // zero visible error lines reads like a bug otherwise. With the
        // errors-first display sort this only fires when there are more
        // error-carrying files than the cap, but it keeps the listing honest
        // in that case too.
        // Review M-1 (PR #277): read-only mode only — fix-mode JSON shapes
        // the per-file groups as `fixed_groups`/`remaining_groups` (no
        // `rule_groups`), so this computation would wrongly report every
        // error as hidden. Per-file error totals are derived from each
        // group's `count` (the per-rule display cap truncates `violations[]`
        // but never `count`), so the hidden figure reflects the *file* cap.
        let listed_errors: u64 = if is_fix_mode {
            u64::MAX // read-only-only suffix: never claims hidden errors in fix mode (total_errors > u64::MAX is never true)
        } else {
            data.get("files")
                .and_then(|f| f.as_array())
                .map_or(0, |files| {
                    files
                        .iter()
                        .map(|file| {
                            file.get("rule_groups").and_then(|rg| rg.as_array()).map_or(
                                0,
                                |groups| {
                                    groups
                                        .iter()
                                        .filter(|g| {
                                            g.get("severity").and_then(serde_json::Value::as_str)
                                                == Some("error")
                                        })
                                        .map(|g| {
                                            g.get("count")
                                                .and_then(serde_json::Value::as_u64)
                                                .unwrap_or(0)
                                        })
                                        .sum::<u64>()
                                },
                            )
                        })
                        .sum()
                })
        };
        let total_errors = data
            .get("errors")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let description = if total_errors > listed_errors {
            let hidden = total_errors - listed_errors;
            let err_label = if hidden == 1 { "error" } else { "errors" };
            format!(
                "Show all {total_violations} files with issues ({hidden} {err_label} hidden by the file cap)"
            )
        } else {
            format!("Show all {total_violations} files with issues (no limit)")
        };
        hints.push(Hint::new(
            description,
            build_command_with_glob_and_files(ctx, &["lint", "--limit", "0"]),
        ));
    }

    // -----------------------------------------------------------------------
    // UX-7: Smart fix/dry-run hints.
    // -----------------------------------------------------------------------
    if !is_fix_mode {
        // Not in fix mode: suggest preview (don't suggest apply directly).
        let has_violations = data
            .get("files")
            .and_then(|f| f.as_array())
            .is_some_and(|files| {
                files.iter().any(|file| {
                    file.get("rule_groups")
                        .and_then(|rg| rg.as_array())
                        .is_some_and(|groups| {
                            groups.iter().any(|g| {
                                g.get("autofixable")
                                    .and_then(serde_json::Value::as_bool)
                                    .unwrap_or(false)
                            })
                        })
                        || file
                            .get("violations")
                            .and_then(|v| v.as_array())
                            .is_some_and(|v| !v.is_empty())
                })
            });
        if has_violations && hints.len() < MAX_HINTS {
            // Preserve --rule / --rule-prefix / --fix-rule from the current
            // invocation so the suggested preview doesn't widen scope.
            hints.push(Hint::new(
                "Preview auto-fixes",
                build_lint_with_filter_flags(ctx, &["lint", "--fix", "--dry-run"]),
            ));
        }
    } else if is_dry_run {
        // Dry-run: if there are fixes that would be applied, suggest actually applying.
        let total_fixed = data
            .get("total_fixed")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        // Also check old-shape `fixes` for backward compat.
        let has_fixes = total_fixed > 0
            || data
                .get("fixes")
                .and_then(|f| f.as_array())
                .is_some_and(|a| !a.is_empty());
        if has_fixes && hints.len() < MAX_HINTS {
            // Apply hint mirrors the previewed scope — preserve --rule,
            // --rule-prefix, and --fix-rule via the lint-aware builder.
            hints.push(Hint::new(
                "Apply auto-fixes",
                build_lint_with_filter_flags(ctx, &["lint", "--fix"]),
            ));
        }
    }
    // If is_fix_mode && !is_dry_run: don't suggest fix hints (already applied).

    // -----------------------------------------------------------------------
    // UX-1: per-rule hints for HYALO001 and HYALO002.
    // -----------------------------------------------------------------------
    // Find the "worst-offender" file (first in files array — already sorted by total desc).
    let worst_file = data
        .get("files")
        .and_then(|f| f.as_array())
        .and_then(|arr| arr.first())
        .and_then(|f| f.get("file"))
        .and_then(serde_json::Value::as_str);

    let rule_totals = collect_rule_totals(data);
    let mut per_rule_hint_rules: Vec<String> = Vec::new();
    for rule_id in &["HYALO001", "HYALO002"] {
        if rule_totals.get(*rule_id).is_some_and(|&c| c > 0) {
            per_rule_hint_rules.push(rule_id.to_string());
        }
    }
    for rule_id in &per_rule_hint_rules {
        if hints.len() >= MAX_HINTS {
            break;
        }
        // De-dupe: don't add if we already have a hint with this rule.
        let already = hints.iter().any(|h| h.cmd.contains(rule_id.as_str()));
        if already {
            continue;
        }
        if let Some(hint) = per_rule_hint(ctx, rule_id, worst_file) {
            hints.push(hint);
        }
    }

    // -----------------------------------------------------------------------
    // UX-2: rule dominance hint.
    // -----------------------------------------------------------------------
    let grand_total: usize = rule_totals.values().sum();
    if grand_total > 0
        && hints.len() < MAX_HINTS
        && let Some((dominant_rule, dominant_count_ref)) =
            rule_totals.iter().max_by_key(|(_, c)| *c)
    {
        let dominant_count = *dominant_count_ref;
        #[allow(clippy::cast_precision_loss)]
        let ratio = dominant_count as f64 / grand_total as f64;
        if ratio >= RULE_DOMINANCE_RATIO && dominant_count >= RULE_DOMINANCE_MIN {
            let already = hints.iter().any(|h| {
                h.cmd.contains("lint-rules show") && h.cmd.contains(dominant_rule.as_str())
            });
            if !already {
                hints.push(Hint::new(
                    format!("Tune {dominant_rule} if too noisy on this KB"),
                    build_command_no_glob(ctx, &["lint-rules", "show", dominant_rule]),
                ));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Parse-error hint.
    // -----------------------------------------------------------------------
    let has_parse_errors = data
        .get("files")
        .and_then(|f| f.as_array())
        .is_some_and(|files| {
            files.iter().any(|file| {
                file.get("rule_groups")
                    .and_then(|rg| rg.as_array())
                    .is_some_and(|groups| {
                        groups.iter().any(|g| {
                            g.get("violations")
                                .and_then(|v| v.as_array())
                                .is_some_and(|vs| {
                                    vs.iter().any(|v| {
                                        v.get("message")
                                            .and_then(|m| m.as_str())
                                            .is_some_and(|m| m.starts_with(PARSE_ERROR_PREFIX))
                                    })
                                })
                        })
                    })
                    || file
                        .get("violations")
                        .and_then(|v| v.as_array())
                        .is_some_and(|v| {
                            v.iter().any(|violation| {
                                violation
                                    .get("message")
                                    .and_then(|m| m.as_str())
                                    .is_some_and(|m| m.starts_with(PARSE_ERROR_PREFIX))
                            })
                        })
            })
        });
    if has_parse_errors && hints.len() < MAX_HINTS {
        hints.push(Hint::new(
            "Show all files with unfixable frontmatter errors",
            build_command_with_glob_and_files(ctx, &["lint", "--limit", "0"]),
        ));
    }

    // -----------------------------------------------------------------------
    // SCHEMA → `types show <T>` (iter-143). Surface a per-type hint when
    // SCHEMA violations land on files that declared a `type:`. Skip when
    // the user is already focused on schema rules (--rule SCHEMA or
    // --rule-prefix HYALO).
    // -----------------------------------------------------------------------
    let already_schema_focused = ctx.lint_rule.as_deref() == Some("SCHEMA")
        || ctx
            .lint_rule_prefix
            .as_deref()
            .is_some_and(|p| p.starts_with("HYALO"));
    if !already_schema_focused && let Some(files) = data.get("files").and_then(|f| f.as_array()) {
        // Collect distinct types that have at least one SCHEMA violation.
        // Preserve first-seen order; cap at 2 distinct types to avoid noise.
        //
        // Inspect both the read-only mode shape (`rule_groups`) and the
        // fix-mode shape (`remaining_groups`) so the hint fires regardless
        // of which lint mode produced the output.
        let mut schema_types: Vec<String> = Vec::new();
        for file in files {
            let has_schema_in = |key: &str| {
                file.get(key)
                    .and_then(|rg| rg.as_array())
                    .is_some_and(|groups| {
                        groups.iter().any(|g| {
                            g.get("rule").and_then(serde_json::Value::as_str) == Some("SCHEMA")
                        })
                    })
            };
            if !has_schema_in("rule_groups") && !has_schema_in("remaining_groups") {
                continue;
            }
            let Some(t) = file.get("type").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if !schema_types.iter().any(|x| x == t) {
                schema_types.push(t.to_owned());
            }
            if schema_types.len() >= 2 {
                break;
            }
        }
        for t in &schema_types {
            if hints.len() >= MAX_HINTS {
                break;
            }
            hints.push(Hint::new(
                format!("Show schema for type: {t}"),
                build_command_no_glob(ctx, &["types", "show", t]),
            ));
        }
    }

    // -----------------------------------------------------------------------
    // Always suggest listing defined types.
    // -----------------------------------------------------------------------
    if hints.len() < MAX_HINTS {
        hints.push(Hint::new(
            "See defined type schemas",
            build_command_no_glob(ctx, &["types", "list"]),
        ));
    }

    hints
}
