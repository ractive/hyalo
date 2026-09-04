//! The per-file extended lint pass.
//!
//! Split out of the single 4,005-line `commands/lint.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `commands::lint::...` paths and behaviour are
//! unchanged.

use super::{
    FixOutcome, InternalViolation, PerFileLintResult, apply_body_fixes, find_body_line_offset,
    find_body_start, push_fix_write_error_violation,
};
use anyhow::{Context, Result};
use hyalo_core::frontmatter::{check_mtime, read_mtime, write_frontmatter_within};
use hyalo_core::scanner;
use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};
use hyalo_mdlint::schema::{
    FixAction, FixMode, RULE_ID_BROKEN_LINK, RULE_ID_FRONTMATTER_PARSE_ERROR, Severity,
    VIOLATION_KIND_CONSTRAINT_VIOLATION, VIOLATION_KIND_MISSING_REQUIRED_NO_DEFAULT,
    VIOLATION_KIND_MISSING_TYPE, VIOLATION_KIND_UNDECLARED_PROPERTY, Violation, apply_fixes,
    terse_root_cause, validate_properties, validate_required_sections,
};
use std::borrow::Cow;
use std::path::Path;

/// Lint a single file (frontmatter + body). Returns a `PerFileLintResult`.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(super) fn lint_one_file_extended(
    full_path: &Path,
    rel_path: &str,
    schema: &SchemaConfig,
    engine: &hyalo_mdlint::HyaloLintEngine,
    md_lint_config: &hyalo_mdlint::LintConfig,
    rule_filter: &[String],
    schema_has_completed: bool,
    fix: FixMode,
    fix_rules: &[String],
    max_per_rule: usize,
    strict: bool,
    okf_profile: bool,
    madr_profile: bool,
    skills_profile: bool,
    changelog_profile: bool,
    vault_dir: &Path,
    case_insensitive: bool,
    link_ctx: Option<&hyalo_mdlint::profiles::link::LinkLintContext>,
) -> Result<PerFileLintResult> {
    // One rule's fix can expose a fresh violation for another rule (e.g. a
    // trimmed line changing what counts as a duplicate blank line), so a
    // single lint→fix pass over the body does not always converge. Bounds
    // the lint→fix→re-lint loop below.
    const MAX_BODY_FIX_PASSES: usize = 5;

    // Stat before reading: oversized files are skipped rather than loaded
    // whole into memory (mirrors `scanner::scan_file_multi`'s own guard).
    let meta =
        std::fs::metadata(full_path).with_context(|| format!("failed to stat {rel_path}"))?;
    if meta.len() > scanner::MAX_FILE_SIZE {
        eprintln!(
            "warning: skipping {} ({} MiB exceeds {} MiB limit)",
            full_path.display(),
            meta.len() / (1024 * 1024),
            scanner::MAX_FILE_SIZE / (1024 * 1024)
        );
        let mut violations_by_rule = indexmap::IndexMap::new();
        violations_by_rule.insert(
            "FILE".to_owned(),
            vec![InternalViolation {
                line: 1,
                column: 1,
                message: format!(
                    "file exceeds {} MiB size limit — skipped, not linted",
                    scanner::MAX_FILE_SIZE / (1024 * 1024)
                ),
                severity: "warn".to_owned(),
                fix: None,
                fixed: false,
                autofixable: None,
            }],
        );
        return Ok(PerFileLintResult {
            rel_path: rel_path.to_owned(),
            doc_type: None,
            violations_by_rule,
            total_violations: 1,
            body_modified: false,
            fix_actions: Vec::new(),
            body_fix_outcomes: Vec::new(),
            post_fix_schema_remaining: None,
        });
    }

    // Baseline mtime fingerprint for TOCTOU detection around fix-mode
    // writes below. Derived from the stat above instead of a second
    // `read_mtime` round-trip.
    let mut mtime0: (std::time::SystemTime, u64) = (
        meta.modified()
            .with_context(|| format!("mtime not available for {rel_path}"))?,
        meta.len(),
    );

    // Read the file content once. A single unreadable file (invalid UTF-8,
    // permission error, etc.) must not abort the whole vault-wide run — the
    // caller's merge loop propagates any `Err` here via `?`, which used to
    // kill `lint`/`lint --fix` entirely on one corrupt file (M-1,
    // adversarial-review-2026-08-23.md). Report it once and skip just this
    // file, mirroring the size-limit skip above and the lossy-decode
    // skip+warn precedent in `scanner/mod.rs`.
    let content = match std::fs::read_to_string(full_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("warning: skipping {} ({e})", full_path.display());
            let mut violations_by_rule = indexmap::IndexMap::new();
            violations_by_rule.insert(
                "FILE".to_owned(),
                vec![InternalViolation {
                    line: 1,
                    column: 1,
                    message: format!("could not read file ({e}) — skipped, not linted"),
                    severity: "error".to_owned(),
                    fix: None,
                    fixed: false,
                    autofixable: None,
                }],
            );
            return Ok(PerFileLintResult {
                rel_path: rel_path.to_owned(),
                doc_type: None,
                violations_by_rule,
                total_violations: 1,
                body_modified: false,
                fix_actions: Vec::new(),
                body_fix_outcomes: Vec::new(),
                post_fix_schema_remaining: None,
            });
        }
    };

    // Find where the frontmatter ends so we can split body.
    let body_start = find_body_start(&content);
    let body_content = &content[body_start..];

    // Frontmatter pass: use existing logic but convert to new shape.
    let properties = match hyalo_core::frontmatter::read_frontmatter(full_path) {
        Ok(p) => p,
        Err(e) if hyalo_core::frontmatter::is_parse_error(&e) => {
            // Malformed frontmatter — report as a single error-severity
            // violation under the stable HYALO005 rule id so it shows up in
            // lint output, `lint-rules list`, and CI. A file whose frontmatter
            // cannot be parsed is otherwise invisible to every other rule, so
            // this must never silently vanish (RB-3 / df-own-kb B3).
            //
            // Severity is user-configurable via `[lint.rules.HYALO005]
            // severity = "warn"`, but the rule always emits (it is not
            // disable-able) and no profile can downgrade it.
            let severity = md_lint_config
                .rules
                .get(RULE_ID_FRONTMATTER_PARSE_ERROR)
                .and_then(hyalo_mdlint::RuleOverride::severity)
                .filter(|s| s.eq_ignore_ascii_case("warn"))
                .map_or("error", |_| "warn");
            let mut violations_by_rule = indexmap::IndexMap::new();
            violations_by_rule.insert(
                RULE_ID_FRONTMATTER_PARSE_ERROR.to_owned(),
                vec![InternalViolation {
                    line: 1,
                    column: 1,
                    message: format!(
                        "{}: {}",
                        crate::hints::PARSE_ERROR_PREFIX,
                        terse_root_cause(&e)
                    ),
                    severity: severity.to_owned(),
                    fix: None,
                    fixed: false,
                    autofixable: None,
                }],
            );
            return Ok(PerFileLintResult {
                rel_path: rel_path.to_owned(),
                doc_type: None,
                violations_by_rule,
                total_violations: 1,
                body_modified: false,
                fix_actions: Vec::new(),
                body_fix_outcomes: Vec::new(),
                post_fix_schema_remaining: None,
            });
        }
        Err(e) => return Err(e).context(format!("reading frontmatter from {rel_path}")),
    };

    let mut violations_by_rule: indexmap::IndexMap<String, Vec<InternalViolation>> =
        indexmap::IndexMap::new();
    // Set when the frontmatter fix write fails (TOCTOU or I/O) below, so the
    // body-fix write is skipped for this file rather than attempted against
    // a `mtime0` baseline that may no longer be trustworthy (Finding 4,
    // review round on PR #254).
    let mut frontmatter_write_failed = false;

    // Frontmatter violations → use the existing `validate_properties` but map to new shape.
    // Only emit if the rule isn't filtered out.
    let should_include_frontmatter = rule_filter.is_empty()
        || rule_filter
            .iter()
            .any(|r| r.starts_with("FRONTMATTER") || r == "SCHEMA");
    if should_include_frontmatter {
        let mut fm_violations =
            validate_properties(rel_path, &properties, schema, case_insensitive);

        // Under --strict, the missing-type warning is promoted to an error.
        // `validate_properties` only emits it when the schema is non-empty, but
        // strict mode should catch missing `type` regardless — inject it when
        // the schema was empty and the property is absent (BUG-3 / iter-133).
        let already_has_missing_type = fm_violations
            .iter()
            .any(|v| v.kind == Some(VIOLATION_KIND_MISSING_TYPE));
        // Exempt / reserved files are bound to no schema, so `--strict` must
        // not inject a missing-`type` error for them either. A path-bound file
        // (bind = typing) is fully typed by its binding, so it is likewise
        // exempt from the missing-`type` injection.
        if strict
            && !already_has_missing_type
            && !properties.contains_key("type")
            && !schema.exempt.is_exempt_ci(rel_path, case_insensitive)
            && schema.bound_type_for(rel_path).is_none()
        {
            fm_violations.push(Violation {
                severity: Severity::Warn,
                kind: Some(VIOLATION_KIND_MISSING_TYPE),
                message: "no 'type' property — validating against default schema only".to_owned(),
            });
        }

        for v in fm_violations {
            // In strict mode, promote the two targeted schema warnings to errors.
            // Match on the stable `kind` identifier rather than message text so
            // future message rewordings don't silently disable promotion.
            let effective_severity = if strict
                && v.severity == Severity::Warn
                && matches!(
                    v.kind,
                    Some(VIOLATION_KIND_MISSING_TYPE | VIOLATION_KIND_UNDECLARED_PROPERTY)
                ) {
                Severity::Error
            } else {
                v.severity
            };
            let sev = match effective_severity {
                Severity::Error => "error",
                Severity::Warn => "warn",
            };
            // Some SCHEMA violations have no fixer at all, so reporting them
            // `autofixable: true` promises a fix `--fix` never applies:
            // - a missing/empty required property with no declared default
            //   cannot be synthesized (mapl BUG-3);
            // - a constraint violation (object-list shape, `pattern` /
            //   `item_pattern` mismatch) has no fixer either (DEC-286).
            // Tag both not-autofixable so the SCHEMA group reports
            // `autofixable: false` unless some other SCHEMA violation in the
            // file really is fixable.
            let autofixable = Some(!matches!(
                v.kind,
                Some(
                    VIOLATION_KIND_MISSING_REQUIRED_NO_DEFAULT
                        | VIOLATION_KIND_CONSTRAINT_VIOLATION
                )
            ));
            violations_by_rule
                .entry("SCHEMA".to_owned())
                .or_default()
                .push(InternalViolation {
                    line: 1,
                    column: 1,
                    message: v.message,
                    severity: sev.to_owned(),
                    fix: None,
                    fixed: false,
                    autofixable,
                });
        }
    }

    // HYALO003 — date-format: frontmatter date-typed keys must hold YYYY-MM-DD values.
    for diag in
        engine.lint_frontmatter_hyalo003(rel_path, &properties, md_lint_config, rule_filter, strict)
    {
        let sev = match diag.severity {
            hyalo_mdlint::DiagSeverity::Error => "error",
            hyalo_mdlint::DiagSeverity::Warn => "warn",
        };
        violations_by_rule
            .entry("HYALO003".to_owned())
            .or_default()
            .push(InternalViolation {
                line: diag.line,
                column: diag.column,
                message: diag.message,
                severity: sev.to_owned(),
                fix: None,
                fixed: false,
                autofixable: None,
            });
    }

    // HYALO007 — title-not-scalar: a list/map `title` cannot be promoted.
    for diag in
        engine.lint_frontmatter_hyalo007(rel_path, &properties, md_lint_config, rule_filter, strict)
    {
        let sev = match diag.severity {
            hyalo_mdlint::DiagSeverity::Error => "error",
            hyalo_mdlint::DiagSeverity::Warn => "warn",
        };
        violations_by_rule
            .entry("HYALO007".to_owned())
            .or_default()
            .push(InternalViolation {
                line: diag.line,
                column: diag.column,
                message: diag.message,
                severity: sev.to_owned(),
                fix: None,
                fixed: false,
                autofixable: None,
            });
    }

    // HYALO004 — datetime-format: schema-declared datetime properties must
    // hold `YYYY-MM-DDThh:mm:ss` values.
    let doc_type_for_dt: Option<String> = properties
        .get("type")
        .and_then(hyalo_core::schema::normalize_type_value);
    let effective_schema_for_dt: TypeSchema = match doc_type_for_dt.as_deref() {
        Some(t) => schema.merged_schema_for_type(t),
        None => schema.default_schema().clone(),
    };
    let datetime_pairs: Vec<(&str, &str, bool)> = effective_schema_for_dt
        .properties
        .iter()
        .filter_map(|(name, c)| {
            let is_tz = match c {
                PropertyConstraint::DateTime => false,
                PropertyConstraint::DateTimeTz => true,
                _ => return None,
            };
            let v = properties.get(name.as_str())?;
            let s = v.as_str()?;
            Some((name.as_str(), s, is_tz))
        })
        .collect();
    for diag in engine.lint_frontmatter_hyalo004(
        rel_path,
        &datetime_pairs,
        md_lint_config,
        rule_filter,
        strict,
    ) {
        let sev = match diag.severity {
            hyalo_mdlint::DiagSeverity::Error => "error",
            hyalo_mdlint::DiagSeverity::Warn => "warn",
        };
        violations_by_rule
            .entry("HYALO004".to_owned())
            .or_default()
            .push(InternalViolation {
                line: diag.line,
                column: diag.column,
                message: diag.message,
                severity: sev.to_owned(),
                fix: None,
                fixed: false,
                autofixable: None,
            });
    }

    // Apply frontmatter fixes if requested.
    let mut body_modified = false;
    let mut fix_actions: Vec<FixAction> = Vec::new();
    let mut post_fix_schema_remaining: Option<Vec<InternalViolation>> = None;
    // Post-fix type (used for required_sections validation below). Defaults to the
    // type in the unfixed frontmatter; apply_fixes may infer/insert a type via
    // FRONTMATTER003, in which case we want to validate against that.
    let mut post_fix_doc_type: Option<String> = properties
        .get("type")
        .and_then(hyalo_core::schema::normalize_type_value);
    if matches!(fix, FixMode::Apply | FixMode::DryRun) {
        let fix_all_rules = fix_rules.is_empty();
        let should_fix_frontmatter = fix_all_rules
            || fix_rules
                .iter()
                .any(|r| r == "SCHEMA" || r.starts_with("FRONTMATTER"));
        if should_fix_frontmatter {
            let mut mutable = properties.clone();
            let actions = apply_fixes(rel_path, &mut mutable, schema);
            if !actions.is_empty() {
                let mut applied = true;
                if matches!(fix, FixMode::Apply) {
                    // A TOCTOU failure here (external edit during --fix) or
                    // a write error is a real, expected runtime condition —
                    // not exotic — and must not abort the whole batch via
                    // `?` (M-1 follow-up, Finding 4, review round on PR
                    // #254: only the initial read was hardened, not this
                    // write path, so `lint_files_extended`'s merge loop
                    // still killed the run on one file's write failure).
                    // Catch it, report it as a diagnostic on this file, and
                    // skip fixing (this file only) instead.
                    let write_result = check_mtime(full_path, mtime0).and_then(|()| {
                        write_frontmatter_within(vault_dir, full_path, &mutable)
                            .with_context(|| format!("writing fixed frontmatter to {rel_path}"))
                    });
                    match write_result {
                        Ok(()) => {
                            // Re-baseline: the write above legitimately
                            // changed the file's mtime, and a later
                            // body-fix write in this same call must not
                            // mistake it for a concurrent modification. If
                            // *this* re-read fails, the frontmatter write
                            // itself already succeeded — still count the
                            // fix as applied, but skip attempting the body
                            // fix below (no trustworthy baseline for its
                            // own TOCTOU check).
                            match read_mtime(full_path) {
                                Ok(fresh) => mtime0 = fresh,
                                Err(e) => {
                                    frontmatter_write_failed = true;
                                    push_fix_write_error_violation(
                                        &mut violations_by_rule,
                                        &format!(
                                            "fixed frontmatter written to {rel_path}, but could \
                                             not re-read its mtime for the body-fix TOCTOU \
                                             check: {e}"
                                        ),
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            applied = false;
                            frontmatter_write_failed = true;
                            push_fix_write_error_violation(
                                &mut violations_by_rule,
                                &format!("could not write fixed frontmatter to {rel_path}: {e}"),
                            );
                        }
                    }
                }
                if applied {
                    fix_actions = actions;
                }
            }
            if should_include_frontmatter {
                // Re-validate the mutated properties to get the actual
                // post-fix SCHEMA remaining set. Avoids guessing via
                // `fix_actions.len()`, which is not 1:1 with resolved
                // diagnostics (one fix action can clear multiple violations,
                // or insert defaults that don't clear any).
                let post = validate_properties(rel_path, &mutable, schema, case_insensitive);
                let remaining: Vec<InternalViolation> = post
                    .into_iter()
                    .map(|v| {
                        let sev = match v.severity {
                            Severity::Error => "error",
                            Severity::Warn => "warn",
                        };
                        InternalViolation {
                            line: 1,
                            column: 1,
                            message: v.message,
                            severity: sev.to_owned(),
                            fix: None,
                            fixed: false,
                            autofixable: None,
                        }
                    })
                    .collect();
                post_fix_schema_remaining = Some(remaining);
            }
            // Update post_fix_doc_type from the (possibly-inferred) mutable properties.
            post_fix_doc_type = mutable
                .get("type")
                .and_then(hyalo_core::schema::normalize_type_value);
        }
    }

    // Required-sections pass — only when rule_filter is empty or includes SCHEMA.
    if should_include_frontmatter {
        let effective_schema = match post_fix_doc_type.as_deref() {
            Some(t) => schema.merged_schema_for_type(t),
            None => schema.default_schema().clone(),
        };
        if !effective_schema.required_sections.is_empty() {
            let section_violations = validate_required_sections(
                full_path,
                rel_path,
                &effective_schema.required_sections,
            )?;
            for v in section_violations {
                let sev = match v.severity {
                    Severity::Error => "error",
                    Severity::Warn => "warn",
                };
                violations_by_rule
                    .entry("SCHEMA".to_owned())
                    .or_default()
                    .push(InternalViolation {
                        line: 1,
                        column: 1,
                        message: v.message,
                        severity: sev.to_owned(),
                        fix: None,
                        fixed: false,
                        autofixable: None,
                    });
            }
        }
    }

    // Body pass — extract frontmatter fields needed for HYALO rules.
    let frontmatter_status = properties
        .get("status")
        .and_then(|v| v.as_str())
        .map(str::to_owned);

    // Track per-diagnostic outcomes for the new fix-mode JSON shape.
    let mut body_fix_outcomes: Vec<(String, usize, FixOutcome)> = Vec::new();
    // Diagnostics that were fixed, accumulated across every pass below
    // (each carries its own line/column, valid against the body revision it
    // was found in — fine for display, since only `fixed`-derived counts
    // and messages are surfaced, never the byte offsets).
    let mut fixed_diagnostics: Vec<hyalo_mdlint::Diagnostic> = Vec::new();
    // The body text, mutated in place across fix passes.
    let mut working_body: String = body_content.to_owned();

    // Re-lint and re-fix the working body up to `MAX_BODY_FIX_PASSES` times,
    // or until nothing more changes — whichever comes first. Read-only mode
    // and DryRun both run this same in-memory loop; only `FixMode::Apply`
    // writes the result to disk (below), so DryRun still previews the
    // fully-converged outcome.
    let mut current_diagnostics = engine.lint_body(
        &working_body,
        rel_path,
        frontmatter_status.as_deref(),
        schema_has_completed,
        md_lint_config,
        rule_filter,
    )?;

    if matches!(fix, FixMode::Apply | FixMode::DryRun) {
        let fix_all_rules = fix_rules.is_empty();

        for _ in 0..MAX_BODY_FIX_PASSES {
            if current_diagnostics.is_empty() {
                break;
            }
            let fixable_indices: Vec<usize> = current_diagnostics
                .iter()
                .enumerate()
                .filter(|(_, d)| d.fix.is_some())
                .filter(|(_, d)| fix_all_rules || fix_rules.iter().any(|r| r == &d.rule_id))
                .map(|(i, _)| i)
                .collect();
            if fixable_indices.is_empty() {
                break;
            }

            let fixable_refs: Vec<&hyalo_mdlint::Diagnostic> = fixable_indices
                .iter()
                .map(|&i| &current_diagnostics[i])
                .collect();
            let (new_body, outcomes) = apply_body_fixes(&working_body, &fixable_refs);

            let mut applied_this_pass: std::collections::HashSet<usize> =
                std::collections::HashSet::new();
            for (slot, &orig_idx) in fixable_indices.iter().enumerate() {
                let rule_id = current_diagnostics[orig_idx].rule_id.clone();
                // The diagnostic's own line travels with the outcome so a
                // conflict can be explained as `<rule> line <n>` instead of
                // the bare `conflicts N` the text output used to print
                // (dogfood v0.22.0 UX-16).
                let line = current_diagnostics[orig_idx].line;
                match &outcomes[slot] {
                    FixOutcome::Applied => {
                        applied_this_pass.insert(orig_idx);
                        body_fix_outcomes.push((rule_id, line, FixOutcome::Applied));
                    }
                    FixOutcome::Conflict { blocking_rule } => {
                        body_fix_outcomes.push((
                            rule_id,
                            line,
                            FixOutcome::Conflict {
                                blocking_rule: blocking_rule.clone(),
                            },
                        ));
                    }
                    FixOutcome::NoFix => {
                        body_fix_outcomes.push((rule_id, line, FixOutcome::NoFix));
                    }
                }
            }

            if applied_this_pass.is_empty() {
                // Every fixable diagnostic this pass hit a conflict or
                // turned out to be a no-op — no progress possible, stop.
                break;
            }

            for (i, d) in current_diagnostics.into_iter().enumerate() {
                if applied_this_pass.contains(&i) {
                    fixed_diagnostics.push(d);
                }
                // Diagnostics that weren't applied are dropped rather than
                // carried forward: their byte offsets are stale against
                // `new_body`, and any still-unresolved issue is rediscovered
                // with correct positions by the re-lint below.
            }

            working_body = new_body;
            current_diagnostics = engine.lint_body(
                &working_body,
                rel_path,
                frontmatter_status.as_deref(),
                schema_has_completed,
                md_lint_config,
                rule_filter,
            )?;
        }
    }

    // A TOCTOU failure (external edit during --fix) or an I/O error on any
    // of the three fallible steps below (re-reading fresh frontmatter,
    // `check_mtime`, or the write itself) is a real, expected runtime
    // condition — not exotic — and must not abort the whole batch via `?`
    // (M-1 follow-up, Finding 4, review round on PR #254: only the initial
    // read was hardened, not this write path, so `lint_files_extended`'s
    // merge loop still killed the run on one file's write failure here).
    // Skipping the body write when the frontmatter write already failed
    // above avoids compounding two partial-state failures on one file; the
    // frontmatter failure's own diagnostic already covers it.
    let mut body_write_failed = false;
    if matches!(fix, FixMode::Apply) && working_body != body_content && !frontmatter_write_failed {
        // Re-derive the frontmatter bytes fresh from disk when a
        // frontmatter fix already landed above — `content[..body_start]` is
        // a snapshot from before that write and would silently revert it if
        // reused here.
        let frontmatter_part: Result<Cow<'_, str>> = if fix_actions.is_empty() {
            Ok(Cow::Borrowed(&content[..body_start]))
        } else {
            std::fs::read_to_string(full_path)
                .with_context(|| format!("re-reading {rel_path} after frontmatter fix"))
                .map(|fresh| {
                    let fresh_body_start = find_body_start(&fresh);
                    Cow::Owned(fresh[..fresh_body_start].to_owned())
                })
        };
        let write_result = frontmatter_part.and_then(|frontmatter_part| {
            check_mtime(full_path, mtime0)?;
            let new_content = format!("{frontmatter_part}{working_body}");
            hyalo_core::atomic_write_within(vault_dir, full_path, new_content.as_bytes())
                .with_context(|| format!("writing fixed body to {rel_path}"))
        });
        match write_result {
            Ok(()) => body_modified = true,
            Err(e) => {
                body_write_failed = true;
                push_fix_write_error_violation(
                    &mut violations_by_rule,
                    &format!("could not write fixed body to {rel_path}: {e}"),
                );
            }
        }
    }

    // Body rules lint the post-frontmatter slice, so their diagnostics carry
    // body-relative line numbers. Translate them to file-absolute lines so a
    // reported `line N` matches the raw file (BUG-6). `body_line_offset` is the
    // 1-based file line on which the body begins; body-relative line `L` maps
    // to `L + offset - 1`. With no frontmatter `offset == 1`, i.e. a no-op.
    let body_line_offset = find_body_line_offset(&content, body_start);
    let to_file_line =
        |body_line: usize| body_line.saturating_add(body_line_offset.saturating_sub(1));

    // Group body diagnostics by rule: violations fixed across any pass,
    // followed by whatever remains after the loop above (or the single
    // read-only lint pass, when fix-mode is off).
    let diag_to_violation = |d: hyalo_mdlint::Diagnostic, fixed: bool| {
        let fix = d.fix.as_ref().map(|f| {
            serde_json::json!({
                "description": f.description,
                "start": f.start,
                "end": f.end,
                "replacement": f.replacement,
            })
        });
        InternalViolation {
            line: to_file_line(d.line),
            column: d.column,
            message: d.message,
            severity: format!("{}", d.severity),
            fix,
            fixed,
            autofixable: None,
        }
    };
    for d in fixed_diagnostics {
        let rule_id = d.rule_id.clone();
        // `fixed_diagnostics` was populated by the in-memory fix loop above,
        // before it was known whether the write to disk would succeed. If
        // it didn't (`body_write_failed`), nothing was actually fixed — the
        // report must not claim `fixed: true` for a change that only ever
        // existed in memory (Finding 4, review round on PR #254).
        violations_by_rule
            .entry(rule_id)
            .or_default()
            .push(diag_to_violation(d, !body_write_failed));
    }
    for d in current_diagnostics {
        let rule_id = d.rule_id.clone();
        violations_by_rule
            .entry(rule_id)
            .or_default()
            .push(diag_to_violation(d, false));
    }

    // HYALO006 (broken-link) — vault-aware rule. Runs only when a shared
    // LinkLintContext was built for this invocation (it is skipped when the
    // rule is disabled or filtered out, so no context is constructed). Each of
    // the file's links is resolved through the single shared resolver entry
    // point; unresolved ones become findings. Severity follows the
    // configurable / --strict pattern used by the other HYALO rules.
    if let Some(link_ctx) = link_ctx {
        let base_sev = md_lint_config
            .rules
            .get(RULE_ID_BROKEN_LINK)
            .and_then(hyalo_mdlint::RuleOverride::severity)
            .map_or("warn", |s| {
                if s.eq_ignore_ascii_case("error") {
                    "error"
                } else {
                    "warn"
                }
            });
        // --strict promotes the default warn to error (unless the user pinned
        // an explicit severity via `[lint.rules.HYALO006] severity`).
        let has_explicit_sev = md_lint_config
            .rules
            .get(RULE_ID_BROKEN_LINK)
            .and_then(hyalo_mdlint::RuleOverride::severity)
            .is_some();
        let severity = if strict && !has_explicit_sev {
            "error"
        } else {
            base_sev
        };
        for f in
            hyalo_mdlint::profiles::link::check_broken_links(link_ctx, content.as_bytes(), rel_path)
        {
            violations_by_rule
                .entry(RULE_ID_BROKEN_LINK.to_owned())
                .or_default()
                .push(InternalViolation {
                    // iter-211 / BUG-9: HYALO006 scans the WHOLE file (frontmatter
                    // included) through `scan_slice_multi`, whose visitor line
                    // numbers are already file-absolute. The body-rule
                    // `to_file_line` translation must NOT be applied here — doing
                    // so added the frontmatter length a second time, so a link on
                    // line 5 of a file with 3 frontmatter lines was reported at
                    // line 8. MD rules keep `to_file_line` because they lint the
                    // post-frontmatter slice.
                    line: f.line,
                    column: 1,
                    message: f.message,
                    severity: severity.to_owned(),
                    fix: None,
                    fixed: false,
                    autofixable: None,
                });
        }
    }

    // OKF conformance profile — advisory (warn-level) rules layered on top of
    // the schema pass. Only runs under `hyalo lint --profile okf`. Every OKF
    // rule respects `[lint.rules]` enable/disable overrides and the
    // `--rule`/`--rule-prefix` filters, exactly like the HYALO rules above.
    if okf_profile {
        let is_enabled = |rule_id: &str| -> bool {
            // `[lint.rules.<id>] enabled = false` disables it; default on.
            let cfg_enabled = md_lint_config
                .rules
                .get(rule_id)
                .and_then(hyalo_mdlint::RuleOverride::enabled)
                .unwrap_or(true);
            if !cfg_enabled {
                return false;
            }
            // Honor --rule / --rule-prefix (rule_filter is the resolved id set).
            rule_filter.is_empty() || rule_filter.iter().any(|r| r == rule_id)
        };
        let okf_doc_type = properties.get("type").and_then(|v| v.as_str());
        let findings = hyalo_mdlint::profiles::okf::run_okf_rules(
            rel_path,
            full_path,
            &content,
            body_content,
            find_body_line_offset(&content, body_start),
            okf_doc_type,
            &is_enabled,
            vault_dir,
            case_insensitive,
        );
        for f in findings {
            // Apply per-rule severity override; OKF rules default to warn.
            let severity = md_lint_config
                .rules
                .get(f.rule_id)
                .and_then(hyalo_mdlint::RuleOverride::severity)
                .unwrap_or("warn")
                .to_owned();
            violations_by_rule
                .entry(f.rule_id.to_owned())
                .or_default()
                .push(InternalViolation {
                    line: f.line,
                    column: 1,
                    message: f.message,
                    severity,
                    fix: None,
                    fixed: false,
                    autofixable: None,
                });
        }
    }

    // MADR conformance profile — advisory (warn-level) rules layered on top of
    // the schema pass. Only runs under `hyalo lint --profile madr` (or a vault
    // whose `.hyalo.toml` sets `[lint] profile = "madr"`). Same override/filter
    // discipline as the OKF block above.
    if madr_profile {
        let is_enabled = |rule_id: &str| -> bool {
            let cfg_enabled = md_lint_config
                .rules
                .get(rule_id)
                .and_then(hyalo_mdlint::RuleOverride::enabled)
                .unwrap_or(true);
            if !cfg_enabled {
                return false;
            }
            rule_filter.is_empty() || rule_filter.iter().any(|r| r == rule_id)
        };
        // Effective type: explicit `type:` frontmatter else the path binding.
        let explicit_type = properties.get("type").and_then(|v| v.as_str());
        let effective_type = explicit_type.or_else(|| schema.bound_type_for(rel_path));
        let status = properties.get("status").and_then(|v| v.as_str());
        let findings = hyalo_mdlint::profiles::madr::run_madr_rules(
            rel_path,
            full_path,
            effective_type,
            status,
            &is_enabled,
        );
        for f in findings {
            let severity = md_lint_config
                .rules
                .get(f.rule_id)
                .and_then(hyalo_mdlint::RuleOverride::severity)
                .unwrap_or("warn")
                .to_owned();
            violations_by_rule
                .entry(f.rule_id.to_owned())
                .or_default()
                .push(InternalViolation {
                    line: f.line,
                    column: 1,
                    message: f.message,
                    severity,
                    fix: None,
                    fixed: false,
                    autofixable: None,
                });
        }
    }

    // Agent Skills conformance profile — rules layered on top of the schema
    // pass. Only runs under `hyalo lint --profile skills` (or a vault whose
    // `.hyalo.toml` sets `[lint] profile = "skills"`). Mostly advisory
    // (warn-level): `SKILL-RESERVED-NAME` is the exception and defaults to
    // error severity (a reserved `name` is a hard spec violation). Same
    // override/filter discipline as the OKF/MADR blocks above.
    if skills_profile {
        let is_enabled = |rule_id: &str| -> bool {
            let cfg_enabled = md_lint_config
                .rules
                .get(rule_id)
                .and_then(hyalo_mdlint::RuleOverride::enabled)
                .unwrap_or(true);
            if !cfg_enabled {
                return false;
            }
            rule_filter.is_empty() || rule_filter.iter().any(|r| r == rule_id)
        };
        // Effective type: explicit `type:` frontmatter else the path binding.
        let explicit_type = properties.get("type").and_then(|v| v.as_str());
        let effective_type = explicit_type.or_else(|| schema.bound_type_for(rel_path));
        let name = properties.get("name").and_then(|v| v.as_str());
        // Line budget counts the markdown body only (frontmatter excluded). A
        // trailing newline does not add a phantom final line.
        let body_line_count = body_content.lines().count();
        let findings = hyalo_mdlint::profiles::skills::run_skill_rules(
            rel_path,
            effective_type,
            name,
            body_line_count,
            &is_enabled,
        );
        for f in findings {
            let severity = md_lint_config
                .rules
                .get(f.rule_id)
                .and_then(hyalo_mdlint::RuleOverride::severity)
                .unwrap_or(f.default_severity)
                .to_owned();
            violations_by_rule
                .entry(f.rule_id.to_owned())
                .or_default()
                .push(InternalViolation {
                    line: f.line,
                    column: 1,
                    message: f.message,
                    severity,
                    fix: None,
                    fixed: false,
                    autofixable: None,
                });
        }
    }

    // Keep a Changelog conformance profile — the grammar rules layered on top
    // of the schema pass. Only runs under `hyalo lint --profile changelog` (or a
    // vault whose `.hyalo.toml` sets `[lint] profile = "changelog"`). Rules
    // dispatch only on the file whose effective type is `changelog` (the literal
    // `CHANGELOG.md` binding). Severity is *mixed*: the grammar/ordering rules
    // default to error, the empty-section and link-ref cross-check to warn — the
    // per-finding `default_severity` is the source of truth, honoured below.
    // Same override/filter discipline as the OKF/MADR/skills blocks above.
    if changelog_profile {
        let explicit_type = properties.get("type").and_then(|v| v.as_str());
        let effective_type = explicit_type.or_else(|| schema.bound_type_for(rel_path));
        if matches!(effective_type, Some(t) if t.eq_ignore_ascii_case("changelog")) {
            let is_enabled = |rule_id: &str| -> bool {
                let cfg_enabled = md_lint_config
                    .rules
                    .get(rule_id)
                    .and_then(hyalo_mdlint::RuleOverride::enabled)
                    .unwrap_or(true);
                if !cfg_enabled {
                    return false;
                }
                rule_filter.is_empty() || rule_filter.iter().any(|r| r == rule_id)
            };
            let findings = hyalo_mdlint::profiles::changelog::run_changelog_rules(
                &content,
                body_content,
                find_body_line_offset(&content, body_start),
                &is_enabled,
            );
            for f in findings {
                let severity = md_lint_config
                    .rules
                    .get(f.rule_id)
                    .and_then(hyalo_mdlint::RuleOverride::severity)
                    .unwrap_or(f.default_severity)
                    .to_owned();
                violations_by_rule
                    .entry(f.rule_id.to_owned())
                    .or_default()
                    .push(InternalViolation {
                        line: f.line,
                        column: 1,
                        message: f.message,
                        severity,
                        fix: None,
                        fixed: false,
                        autofixable: None,
                    });
            }
        }
    }

    // Mirror the `fixed_diagnostics` correction above: `body_fix_outcomes`
    // was recorded by the in-memory fix loop before the write's outcome was
    // known, so an `Applied` entry here must be downgraded to `NoFix` when
    // the write to disk never actually happened (Finding 4, review round on
    // PR #254) — otherwise fix-mode's totals (`fix_mode_file_totals`) would
    // count a change that was never persisted.
    if body_write_failed {
        for (_, _, outcome) in &mut body_fix_outcomes {
            if matches!(outcome, FixOutcome::Applied) {
                *outcome = FixOutcome::NoFix;
            }
        }
    }

    let total_violations = violations_by_rule.values().map(Vec::len).sum();

    let _ = max_per_rule; // applied during output construction

    Ok(PerFileLintResult {
        rel_path: rel_path.to_owned(),
        doc_type: post_fix_doc_type,
        violations_by_rule,
        total_violations,
        body_modified,
        fix_actions,
        body_fix_outcomes,
        post_fix_schema_remaining,
    })
}
