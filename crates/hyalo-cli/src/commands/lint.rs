/// `hyalo lint` — validate frontmatter properties against the `.hyalo.toml` schema.
///
/// Reads each file's frontmatter, applies the type-specific schema (or the
/// default schema if `type` is absent), and reports violations at two severity
/// levels:
///
///   - **error**  — schema violation (missing required field, wrong value type,
///     invalid enum value, failed pattern match)
///   - **warn**   — soft issue (no `type` property, no `tags` property, property
///     not declared in schema)
///
/// Exit code: 0 = clean, 1 = errors found, 2 = internal error.
use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use hyalo_core::filename_template::FilenameTemplate;
use hyalo_core::frontmatter::{read_frontmatter, write_frontmatter};
use hyalo_core::schema::{self, PropertyConstraint, SchemaConfig, TypeSchema};
use hyalo_core::util::is_iso8601_date;

use crate::output::{CommandOutcome, Format, format_success};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Severity of a single lint violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warn,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => f.write_str("error"),
            Self::Warn => f.write_str("warn"),
        }
    }
}

/// A single lint violation found in a file.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub severity: Severity,
    pub message: String,
}

/// Lint results for a single file.
#[derive(Debug, Serialize)]
pub struct FileLintResult {
    pub file: String,
    pub violations: Vec<Violation>,
}

/// A single auto-fix that was (or would be) applied.
#[derive(Debug, Clone, Serialize)]
pub struct FixAction {
    /// Kind of fix: "insert-default", "fix-enum-typo", "normalize-date", "infer-type".
    pub kind: String,
    /// Frontmatter property affected.
    pub property: String,
    /// Old value (if any) — omitted for inserted properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    /// New value applied (or previewed with --dry-run).
    pub new: String,
}

/// Aggregated lint output.
///
/// The `files` field is renamed from the internal `results` to avoid a
/// confusing `results.results` nesting once the CLI envelope wraps the payload.
#[derive(Debug, Serialize)]
pub struct LintOutput {
    pub files: Vec<FileLintResult>,
    /// Total number of violations found across all files.
    pub total: usize,
    /// Number of error-severity violations across all files (not limited by `--limit`).
    pub errors: usize,
    /// Number of warn-severity violations across all files (not limited by `--limit`).
    pub warnings: usize,
    /// Number of files with at least one violation (not limited by `--limit`).
    pub files_with_issues: usize,
    /// Number of files that were checked.
    pub files_checked: usize,
    /// Fixes that were applied (or previewed) per file. Omitted when no
    /// `--fix` run produced any changes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<FileFixResult>,
    /// `true` when `--dry-run` was passed and fixes were not written.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dry_run: bool,
    /// `true` when `--limit` truncated the file list.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub limited: bool,
}

/// Fixes applied to a single file.
#[derive(Debug, Clone, Serialize)]
pub struct FileFixResult {
    pub file: String,
    pub actions: Vec<FixAction>,
}

/// Summary counts returned to callers (e.g. `hyalo summary`).
#[derive(Debug, Clone, Default)]
pub struct LintCounts {
    pub errors: usize,
    pub warnings: usize,
    /// Number of files with at least one violation.
    pub files_with_issues: usize,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Run `hyalo lint` against a list of `(full_path, rel_path)` file pairs.
///
/// Returns the formatted output and the set of counts; the caller is
/// responsible for translating counts into an exit code.
pub fn lint_files(
    files: &[(std::path::PathBuf, String)],
    schema: &SchemaConfig,
) -> Result<(CommandOutcome, LintCounts)> {
    lint_files_with_options(files, schema, FixMode::Off, None)
}

/// Prepend an additional `FileLintResult` (e.g. `.hyalo.toml` view violations)
/// to the outcome produced by [`lint_files_with_options`]. Adjusts the totals
/// and the `files_with_issues` counter in the serialized payload to stay
/// consistent with the new entry.
pub fn prepend_file_result(
    outcome: CommandOutcome,
    extra: &FileLintResult,
) -> Result<CommandOutcome> {
    let (payload, total) = match outcome {
        CommandOutcome::Success { output, total } => (output, total),
        other => return Ok(other),
    };

    let mut value: serde_json::Value =
        serde_json::from_str(&payload).context("failed to re-parse lint output JSON")?;

    if let Some(obj) = value.as_object_mut() {
        let extra_errors = extra
            .violations
            .iter()
            .filter(|v| matches!(v.severity, Severity::Error))
            .count();
        let extra_warnings = extra.violations.len() - extra_errors;

        if let Some(files) = obj.get_mut("files").and_then(|f| f.as_array_mut()) {
            let extra_value = serde_json::to_value(extra)
                .context("failed to serialize .hyalo.toml lint result")?;
            files.insert(0, extra_value);
        }
        if let Some(n) = obj.get_mut("total").and_then(|v| v.as_u64()) {
            obj.insert(
                "total".to_string(),
                serde_json::Value::from(n + extra.violations.len() as u64),
            );
        }
        if let Some(n) = obj.get_mut("errors").and_then(|v| v.as_u64()) {
            obj.insert(
                "errors".to_string(),
                serde_json::Value::from(n + extra_errors as u64),
            );
        }
        if let Some(n) = obj.get_mut("warnings").and_then(|v| v.as_u64()) {
            obj.insert(
                "warnings".to_string(),
                serde_json::Value::from(n + extra_warnings as u64),
            );
        }
        if let Some(n) = obj.get_mut("files_with_issues").and_then(|v| v.as_u64()) {
            obj.insert(
                "files_with_issues".to_string(),
                serde_json::Value::from(n + 1),
            );
        }
    }

    let new_payload = format_success(Format::Json, &value);
    // The outcome's `total` (used by `--count`) tracks files-with-issues —
    // bump it by 1 when the prepended pseudo-file has at least one violation,
    // so `--count` stays in sync with `files_with_issues` in the JSON payload.
    let extra_counts_toward_total = !extra.violations.is_empty();
    Ok(match total {
        Some(t) => CommandOutcome::success_with_total(
            new_payload,
            if extra_counts_toward_total { t + 1 } else { t },
        ),
        None => CommandOutcome::success(new_payload),
    })
}

/// Validate `.hyalo.toml` view definitions and return a pseudo-file lint
/// result when at least one view looks suspicious.
///
/// Current checks:
/// - Views whose only narrowing mechanism is `fields = ["backlinks"]` or
///   similar — `fields` controls display columns, not filtering, so such a
///   view matches every file. The likely intent is `orphan = true`.
///
/// Returns `None` when there is nothing to report.
pub fn validate_views(dir: &Path) -> Option<FileLintResult> {
    // Keys that actually *narrow* the result set.
    const NARROWING_KEYS: &[&str] = &[
        "pattern",
        "regexp",
        "properties",
        "tag",
        "task",
        "sections",
        "file",
        "glob",
        "broken_links",
        "orphan",
        "dead_end",
        "title",
        "language",
    ];

    let toml_path = dir.join(".hyalo.toml");
    let contents = std::fs::read_to_string(&toml_path).ok()?;
    let table: toml::Table = toml::from_str(&contents).ok()?;
    let Some(toml::Value::Table(views_table)) = table.get("views") else {
        return None;
    };

    let mut violations: Vec<Violation> = Vec::new();
    for (name, value) in views_table {
        let Some(view_tbl) = value.as_table() else {
            continue;
        };

        let has_narrowing = view_tbl.iter().any(|(k, v)| {
            if !NARROWING_KEYS.contains(&k.as_str()) {
                return false;
            }
            // Treat `orphan = false` / `dead_end = false` as non-narrowing.
            if matches!(k.as_str(), "orphan" | "dead_end" | "broken_links") {
                return matches!(v, toml::Value::Boolean(true));
            }
            // List-typed narrowing keys with empty values don't narrow either.
            if let toml::Value::Array(a) = v {
                return !a.is_empty();
            }
            true
        });

        let has_fields = view_tbl.contains_key("fields");

        if !has_narrowing && has_fields {
            violations.push(Violation {
                severity: Severity::Warn,
                message: format!(
                    "view '{name}' has no narrowing filter — `fields` controls display columns only, \
                     not filtering. Did you mean `orphan = true` or `dead_end = true`?"
                ),
            });
        } else if !has_narrowing {
            violations.push(Violation {
                severity: Severity::Warn,
                message: format!(
                    "view '{name}' has no narrowing filter — add at least one of: \
                     tag, properties, task, orphan, dead_end, broken_links, glob, file, title"
                ),
            });
        }
    }

    if violations.is_empty() {
        None
    } else {
        Some(FileLintResult {
            file: ".hyalo.toml".to_string(),
            violations,
        })
    }
}

/// Whether — and how — `lint_files_with_options` should apply auto-fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixMode {
    /// Read-only: do not attempt to fix anything.
    Off,
    /// Apply fixes in memory and write them back to disk.
    Apply,
    /// Compute the fixes that would be applied but don't write any files.
    DryRun,
}

/// Run lint with the given fix mode.
///
/// When `fix` is `Apply`, repairable violations are written back to each file
/// before the final counts are computed, so the returned counts reflect only
/// the violations that *remain* after fixing. With `DryRun`, counts reflect
/// the post-fix state but files are untouched.
pub fn lint_files_with_options(
    files: &[(std::path::PathBuf, String)],
    schema: &SchemaConfig,
    fix: FixMode,
    limit: Option<usize>,
) -> Result<(CommandOutcome, LintCounts)> {
    let mut results: Vec<FileLintResult> = Vec::new();
    let mut counts = LintCounts::default();
    let mut fix_results: Vec<FileFixResult> = Vec::new();

    for (full_path, rel_path) in files {
        let (file_result, file_fixes) = lint_file_with_fix(full_path, rel_path, schema, fix)?;
        for v in &file_result.violations {
            match v.severity {
                Severity::Error => counts.errors += 1,
                Severity::Warn => counts.warnings += 1,
            }
        }
        if !file_result.violations.is_empty() {
            counts.files_with_issues += 1;
        }
        if !file_fixes.actions.is_empty() {
            fix_results.push(file_fixes);
        }
        if !file_result.violations.is_empty() {
            results.push(file_result);
        }
    }

    let files_checked = files.len();
    let total = counts.errors + counts.warnings;
    let limited = limit.is_some_and(|n| results.len() > n);
    if let Some(n) = limit {
        results.truncate(n);
    }
    let output = LintOutput {
        files: results,
        total,
        errors: counts.errors,
        warnings: counts.warnings,
        files_with_issues: counts.files_with_issues,
        files_checked,
        fixes: fix_results,
        dry_run: matches!(fix, FixMode::DryRun),
        limited,
    };

    let val = serde_json::to_value(&output).context("failed to serialize lint output")?;
    // Use success_with_total so that `--count` returns the number of files with issues.
    let outcome = CommandOutcome::success_with_total(
        format_success(Format::Json, &val),
        counts.files_with_issues as u64,
    );

    Ok((outcome, counts))
}

/// Compute lint counts for `hyalo summary` without formatting output.
pub fn lint_counts_only(
    files: &[(std::path::PathBuf, String)],
    schema: &SchemaConfig,
) -> Result<LintCounts> {
    let mut counts = LintCounts::default();
    for (full_path, rel_path) in files {
        let file_result = lint_file(full_path, rel_path, schema)?;
        for v in &file_result.violations {
            match v.severity {
                Severity::Error => counts.errors += 1,
                Severity::Warn => counts.warnings += 1,
            }
        }
        if !file_result.violations.is_empty() {
            counts.files_with_issues += 1;
        }
    }
    Ok(counts)
}

/// Compute lint counts from pre-indexed `IndexEntry` properties.
///
/// Used by `hyalo summary` to avoid re-reading files from disk.
/// The `index_entries` iterator yields `(rel_path, properties, has_tags)` tuples.
pub fn lint_counts_from_properties<'a>(
    entries: impl Iterator<Item = (&'a str, &'a IndexMap<String, Value>, bool)>,
    schema: &SchemaConfig,
) -> LintCounts {
    let mut counts = LintCounts::default();
    for (rel_path, properties, has_tags) in entries {
        let violations = validate_properties(rel_path, properties, has_tags, schema);
        for v in &violations {
            match v.severity {
                Severity::Error => counts.errors += 1,
                Severity::Warn => counts.warnings += 1,
            }
        }
        if !violations.is_empty() {
            counts.files_with_issues += 1;
        }
    }
    counts
}

// ---------------------------------------------------------------------------
// Per-file validation
// ---------------------------------------------------------------------------

fn lint_file(full_path: &Path, rel_path: &str, schema: &SchemaConfig) -> Result<FileLintResult> {
    let (result, _) = lint_file_with_fix(full_path, rel_path, schema, FixMode::Off)?;
    Ok(result)
}

/// Lint a single file, optionally applying auto-fixes.
fn lint_file_with_fix(
    full_path: &Path,
    rel_path: &str,
    schema: &SchemaConfig,
    fix: FixMode,
) -> Result<(FileLintResult, FileFixResult)> {
    let properties = match read_frontmatter(full_path) {
        Ok(props) => props,
        Err(e) if hyalo_core::frontmatter::is_parse_error(&e) => {
            // Malformed frontmatter — report as a single error violation.
            return Ok((
                FileLintResult {
                    file: rel_path.to_owned(),
                    violations: vec![Violation {
                        severity: Severity::Error,
                        message: format!("could not parse frontmatter: {e}"),
                    }],
                },
                FileFixResult {
                    file: rel_path.to_owned(),
                    actions: Vec::new(),
                },
            ));
        }
        Err(e) => return Err(e).context(format!("reading {rel_path}")),
    };

    // Apply fixes in memory (or dry-run) before final validation.
    let (final_props, actions) = if matches!(fix, FixMode::Apply | FixMode::DryRun) {
        let mut mutable = properties.clone();
        let actions = apply_fixes(rel_path, &mut mutable, schema);
        if matches!(fix, FixMode::Apply) && !actions.is_empty() {
            write_frontmatter(full_path, &mutable)
                .with_context(|| format!("writing fixed frontmatter to {rel_path}"))?;
        }
        (mutable, actions)
    } else {
        (properties, Vec::new())
    };

    let has_tags = final_props.contains_key("tags");
    let violations = validate_properties(rel_path, &final_props, has_tags, schema);
    Ok((
        FileLintResult {
            file: rel_path.to_owned(),
            violations,
        },
        FileFixResult {
            file: rel_path.to_owned(),
            actions,
        },
    ))
}

// ---------------------------------------------------------------------------
// Auto-fix
// ---------------------------------------------------------------------------

/// Maximum Levenshtein distance accepted for an enum-typo fix.
/// Chosen so that single-letter slips (e.g. "planed" → "planned") are corrected
/// while unrelated values (e.g. "wip" vs. "in-progress") are left alone.
const ENUM_TYPO_MAX_DISTANCE: usize = 2;

/// Compute and apply in-memory auto-fixes to `props`. Returns the list of
/// actions that were taken. Caller is responsible for persisting `props` to
/// disk when appropriate.
fn apply_fixes(
    rel_path: &str,
    props: &mut IndexMap<String, Value>,
    schema: &SchemaConfig,
) -> Vec<FixAction> {
    let mut actions: Vec<FixAction> = Vec::new();

    // Step 1: infer `type` from filename-template if missing.
    if !props.contains_key("type")
        && let Some(inferred) = infer_type_from_path(rel_path, schema)
    {
        // Insert `type` at the front of the map so downstream logic picks it up.
        props.shift_insert(0, "type".to_owned(), Value::String(inferred.clone()));
        actions.push(FixAction {
            kind: "infer-type".to_owned(),
            property: "type".to_owned(),
            old: None,
            new: inferred,
        });
    }

    // Determine the effective schema after any type inference.
    let doc_type: Option<String> = props.get("type").and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        _ => None,
    });
    let effective_schema: TypeSchema = match &doc_type {
        Some(t) => schema.merged_schema_for_type(t),
        None => schema.default_schema().clone(),
    };

    // Step 2: insert defaults for missing properties.
    // Iterate in the schema's `required` order first, then any remaining defaults,
    // so the resulting frontmatter is ordered deterministically.
    let mut inserted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for req in &effective_schema.required {
        if !props.contains_key(req.as_str())
            && let Some(raw) = effective_schema.defaults.get(req.as_str())
        {
            let value = schema::expand_default(raw);
            props.insert(req.clone(), Value::String(value.clone()));
            inserted.insert(req.clone());
            actions.push(FixAction {
                kind: "insert-default".to_owned(),
                property: req.clone(),
                old: None,
                new: value,
            });
        }
    }
    // Also honour defaults for properties not listed in `required`.
    for (name, raw) in &effective_schema.defaults {
        if inserted.contains(name) || props.contains_key(name.as_str()) {
            continue;
        }
        let value = schema::expand_default(raw);
        props.insert(name.clone(), Value::String(value.clone()));
        actions.push(FixAction {
            kind: "insert-default".to_owned(),
            property: name.clone(),
            old: None,
            new: value,
        });
    }

    // Step 3: per-property fixes (enum typos, date normalization).
    let prop_names: Vec<String> = props.keys().cloned().collect();
    for name in prop_names {
        let Some(constraint) = effective_schema.properties.get(name.as_str()) else {
            continue;
        };
        // Snapshot the current value to avoid double-borrowing `props`.
        let Some(current) = props.get(name.as_str()).cloned() else {
            continue;
        };
        match constraint {
            PropertyConstraint::Enum { values } => {
                let Value::String(s) = &current else { continue };
                if values.iter().any(|v| v == s) {
                    continue;
                }
                if let Some((suggestion, dist)) = values
                    .iter()
                    .map(|v| (v, strsim::levenshtein(s, v.as_str())))
                    .min_by_key(|(_, d)| *d)
                    && dist <= ENUM_TYPO_MAX_DISTANCE
                {
                    let old = s.clone();
                    let new_value = suggestion.clone();
                    props.insert(name.clone(), Value::String(new_value.clone()));
                    actions.push(FixAction {
                        kind: "fix-enum-typo".to_owned(),
                        property: name.clone(),
                        old: Some(old),
                        new: new_value,
                    });
                }
            }
            PropertyConstraint::Date => {
                let Value::String(s) = &current else { continue };
                if is_iso8601_date(s) {
                    continue;
                }
                if let Some(normalized) = normalize_date(s) {
                    let old = s.clone();
                    props.insert(name.clone(), Value::String(normalized.clone()));
                    actions.push(FixAction {
                        kind: "normalize-date".to_owned(),
                        property: name.clone(),
                        old: Some(old),
                        new: normalized,
                    });
                }
            }
            _ => {}
        }
    }

    // Step 4: split comma-joined tags (e.g. ["cli,ux"] -> ["cli", "ux"]).
    if let Some(Value::Array(items)) = props.get("tags") {
        let needs_fix = items
            .iter()
            .any(|v| matches!(v, Value::String(s) if s.contains(',')));
        if needs_fix {
            let old_tags: Vec<Value> = items.clone();
            let new_tags: Vec<Value> = old_tags
                .iter()
                .flat_map(|v| match v {
                    Value::String(s) if s.contains(',') => s
                        .split(',')
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                        .map(|p| Value::String(p.to_owned()))
                        .collect::<Vec<_>>(),
                    other => vec![other.clone()],
                })
                .collect();
            let old_str = old_tags
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let new_str = new_tags
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            props.insert("tags".to_owned(), Value::Array(new_tags));
            actions.push(FixAction {
                kind: "split-comma-tags".to_owned(),
                property: "tags".to_owned(),
                old: Some(old_str),
                new: new_str,
            });
        }
    }

    actions
}

/// Try to infer a `type` value for a file at `rel_path` by matching it against
/// every `[schema.types.*].filename-template`. Returns `None` if zero or more
/// than one type matches (ambiguous).
fn infer_type_from_path(rel_path: &str, schema: &SchemaConfig) -> Option<String> {
    let mut matches: Vec<String> = Vec::new();
    for (type_name, ts) in &schema.types {
        let Some(template_str) = &ts.filename_template else {
            continue;
        };
        let Ok(template) = FilenameTemplate::parse(template_str) else {
            continue;
        };
        if template.matches(rel_path) {
            matches.push(type_name.clone());
        }
    }
    if matches.len() == 1 {
        matches.pop()
    } else {
        None
    }
}

/// Normalize a loose date string to `YYYY-MM-DD`.
///
/// Accepts inputs of the form `Y-M-D` where `Y`, `M`, `D` are decimal digit
/// runs and month/day are in the valid calendar ranges. Returns `None` for
/// inputs that are ambiguous (e.g. natural-language dates, non-ISO separators,
/// or out-of-range values); those are reported as violations instead.
fn normalize_date(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y = parts[0];
    let m = parts[1];
    let d = parts[2];
    if y.len() != 4 || !y.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if m.is_empty() || m.len() > 2 || !m.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if d.is_empty() || d.len() > 2 || !d.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let yi: i32 = y.parse().ok()?;
    let mi: u32 = m.parse().ok()?;
    let di: u32 = d.parse().ok()?;
    if !(1..=12).contains(&mi) {
        return None;
    }
    let max_day = match mi {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (yi % 4 == 0 && yi % 100 != 0) || (yi % 400 == 0);
            if leap { 29 } else { 28 }
        }
        _ => return None,
    };
    if !(1..=max_day).contains(&di) {
        return None;
    }
    Some(format!("{y}-{mi:02}-{di:02}"))
}

/// Core property validation logic.
///
/// Separated so it can be used both by the disk-reading path (`lint_file`) and
/// the index-based path (`lint_counts_from_properties`).
fn validate_properties(
    _rel_path: &str,
    properties: &IndexMap<String, Value>,
    has_tags: bool,
    schema: &SchemaConfig,
) -> Vec<Violation> {
    let mut violations: Vec<Violation> = Vec::new();

    // Determine the document type.
    let type_value = properties.get("type");
    let doc_type: Option<String> = type_value.and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        _ => None,
    });

    // If `type` is present but not a string, report an error. A non-string `type`
    // still satisfies a bare `required = ["type"]` check, so without this error
    // invalid type values would slip through silently.
    if let Some(v) = type_value
        && doc_type.is_none()
    {
        violations.push(Violation {
            severity: Severity::Error,
            message: format!("property \"type\" expected string, got {v}"),
        });
    }

    // Warn when no `type` property is present.
    if type_value.is_none() && !schema.is_empty() {
        violations.push(Violation {
            severity: Severity::Warn,
            message: "no 'type' property — validating against default schema only".to_owned(),
        });
    }

    // Determine the effective schema for this file.
    let effective_schema: TypeSchema = match &doc_type {
        Some(t) => schema.merged_schema_for_type(t),
        None => schema.default_schema().clone(),
    };

    // Check required properties.
    for req in &effective_schema.required {
        if !properties.contains_key(req.as_str()) {
            let type_hint = doc_type
                .as_deref()
                .map(|t| format!(" (type: {t})"))
                .unwrap_or_default();
            violations.push(Violation {
                severity: Severity::Error,
                message: format!("missing required property \"{req}\"{type_hint}"),
            });
        }
    }

    // Warn when no `tags` property is present and the schema has at least one type defined.
    if !has_tags && !schema.types.is_empty() {
        violations.push(Violation {
            severity: Severity::Warn,
            message: "no tags defined".to_owned(),
        });
    }

    // Build a per-call regex cache so the same pattern isn't recompiled across
    // properties (this matters in `hyalo summary`, which runs lint over the full
    // index).
    let mut regex_cache: HashMap<String, Result<Regex, String>> = HashMap::new();

    // Type-specific property constraint validation.
    for (name, value) in properties {
        // `tags` is validated against its declared constraint if present, but we
        // never emit an "undeclared property" warning for it (it has its own
        // "no tags defined" warning above).
        if name == "tags" {
            if let Some(constraint) = effective_schema.properties.get(name.as_str())
                && let Some(v) = validate_constraint(name, value, constraint, &mut regex_cache)
            {
                violations.push(v);
            }
            // Check for comma-joined tags (e.g. "cli,ux" instead of ["cli", "ux"]).
            if let Value::Array(items) = value {
                for item in items {
                    if let Value::String(tag) = item
                        && tag.contains(',')
                    {
                        violations.push(Violation {
                            severity: Severity::Warn,
                            message: format!(
                                "tag \"{tag}\" appears to be comma-joined -- should be separate list items"
                            ),
                        });
                    }
                }
            }
            continue;
        }
        // Never warn about "type" (type discriminator) or properties listed in `required`
        // — they're implicitly accepted even if not in the `properties` map.
        let implicitly_accepted = name == "type" || effective_schema.required.contains(name);

        if let Some(constraint) = effective_schema.properties.get(name.as_str()) {
            if let Some(v) = validate_constraint(name, value, constraint, &mut regex_cache) {
                violations.push(v);
            }
        } else if !effective_schema.properties.is_empty() && !implicitly_accepted {
            // Property not declared in schema — warn only when the schema declares
            // some properties. Schemas that only specify `required` remain
            // intentionally permissive about extra fields.
            violations.push(Violation {
                severity: Severity::Warn,
                message: format!("property \"{name}\" is not declared in schema"),
            });
        }
    }

    violations
}

// ---------------------------------------------------------------------------
// Constraint validators
// ---------------------------------------------------------------------------

fn validate_constraint(
    name: &str,
    value: &Value,
    constraint: &PropertyConstraint,
    regex_cache: &mut HashMap<String, Result<Regex, String>>,
) -> Option<Violation> {
    match constraint {
        PropertyConstraint::String { pattern } => {
            let Some(s) = value_as_str(value) else {
                return Some(Violation {
                    severity: Severity::Error,
                    message: format!("property \"{name}\" expected string, got {value}"),
                });
            };
            if let Some(pat) = pattern {
                // Compile (or look up) the regex once per pattern per call.
                let entry = regex_cache
                    .entry(pat.clone())
                    .or_insert_with(|| Regex::new(pat).map_err(|e| e.to_string()));
                match entry {
                    Ok(re) => {
                        if !re.is_match(s) {
                            return Some(Violation {
                                severity: Severity::Error,
                                message: format!(
                                    "property \"{name}\" value {s:?} does not match pattern {pat:?}"
                                ),
                            });
                        }
                    }
                    Err(e) => {
                        return Some(Violation {
                            severity: Severity::Error,
                            message: format!("property \"{name}\": invalid pattern {pat:?}: {e}"),
                        });
                    }
                }
            }
            None
        }
        PropertyConstraint::Date => {
            let Some(s) = value_as_str(value) else {
                return Some(Violation {
                    severity: Severity::Error,
                    message: format!("property \"{name}\" expected date (YYYY-MM-DD), got {value}"),
                });
            };
            if !is_iso8601_date(s) {
                return Some(Violation {
                    severity: Severity::Error,
                    message: format!("property \"{name}\" expected date (YYYY-MM-DD), got \"{s}\""),
                });
            }
            None
        }
        PropertyConstraint::Number => {
            if !matches!(value, Value::Number(_)) {
                return Some(Violation {
                    severity: Severity::Error,
                    message: format!("property \"{name}\" expected number, got {value}"),
                });
            }
            None
        }
        PropertyConstraint::Boolean => {
            if !matches!(value, Value::Bool(_)) {
                return Some(Violation {
                    severity: Severity::Error,
                    message: format!("property \"{name}\" expected boolean, got {value}"),
                });
            }
            None
        }
        PropertyConstraint::List => {
            if !matches!(value, Value::Array(_)) {
                return Some(Violation {
                    severity: Severity::Error,
                    message: format!("property \"{name}\" expected list, got {value}"),
                });
            }
            None
        }
        PropertyConstraint::Enum { values } => {
            let Some(s) = value_as_str(value) else {
                return Some(Violation {
                    severity: Severity::Error,
                    message: format!(
                        "property \"{name}\" expected one of [{}], got {value}",
                        values.join(", ")
                    ),
                });
            };
            if values.contains(&s.to_owned()) {
                return None;
            }
            // Find nearest suggestion via Levenshtein.
            let suggestion = values
                .iter()
                .min_by_key(|v| strsim::levenshtein(s, v.as_str()))
                .map(|v| format!(" (did you mean \"{v}\"?)"))
                .unwrap_or_default();
            Some(Violation {
                severity: Severity::Error,
                message: format!(
                    "property \"{name}\" value \"{s}\" not in [{}]{suggestion}",
                    values.join(", ")
                ),
            })
        }
    }
}

/// Extract a `&str` from a `Value::String`, or `None` for other variants.
fn value_as_str(v: &Value) -> Option<&str> {
    if let Value::String(s) = v {
        Some(s.as_str())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Public validation helper (used by set/append --validate)
// ---------------------------------------------------------------------------

/// Validate a single property value against a constraint without a shared regex cache.
///
/// Returns `Some(error_message)` when the value violates the constraint, `None`
/// when it is valid. Regex patterns are compiled fresh for each call — use the
/// private [`validate_constraint`] with a shared cache in hot paths.
pub fn validate_constraint_simple(
    name: &str,
    value: &Value,
    constraint: &PropertyConstraint,
) -> Option<String> {
    let mut cache = HashMap::new();
    validate_constraint(name, value, constraint, &mut cache).map(|v| v.message)
}

// ---------------------------------------------------------------------------
// Text formatter
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};
    use std::collections::HashMap;

    fn make_schema(
        default_required: &[&str],
        type_name: &str,
        type_required: &[&str],
        type_properties: HashMap<&str, PropertyConstraint>,
    ) -> SchemaConfig {
        let default = TypeSchema {
            required: default_required.iter().map(ToString::to_string).collect(),
            ..Default::default()
        };
        let mut props: HashMap<String, PropertyConstraint> = HashMap::new();
        for (k, v) in type_properties {
            props.insert(k.to_owned(), v);
        }
        let type_schema = TypeSchema {
            required: type_required.iter().map(ToString::to_string).collect(),
            properties: props,
            ..Default::default()
        };
        let mut types = HashMap::new();
        types.insert(type_name.to_owned(), type_schema);
        SchemaConfig { default, types }
    }

    // --- is_iso8601_date ---

    #[test]
    fn valid_date() {
        assert!(is_iso8601_date("2026-04-13"));
    }

    #[test]
    fn normalize_date_padding_and_calendar() {
        // Short month/day get zero-padded.
        assert_eq!(normalize_date("2026-4-9"), Some("2026-04-09".to_owned()));
        // Feb 29 is valid in leap years only.
        assert_eq!(normalize_date("2024-2-29"), Some("2024-02-29".to_owned()));
        assert_eq!(normalize_date("2023-2-29"), None);
        // Out-of-range days/months are rejected, not silently normalized.
        assert_eq!(normalize_date("2026-02-31"), None);
        assert_eq!(normalize_date("2026-04-31"), None);
        assert_eq!(normalize_date("2026-13-01"), None);
    }

    #[test]
    fn invalid_date_format() {
        assert!(!is_iso8601_date("April 13"));
        assert!(!is_iso8601_date("13-04-2026"));
        assert!(!is_iso8601_date("2026/04/13"));
    }

    // Test helper: wraps `validate_constraint` with a throwaway regex cache.
    fn vc(name: &str, value: &Value, c: &PropertyConstraint) -> Option<Violation> {
        let mut cache = HashMap::new();
        validate_constraint(name, value, c, &mut cache)
    }

    // --- validate_constraint ---

    #[test]
    fn date_constraint_valid() {
        let v = vc(
            "date",
            &Value::String("2026-04-13".into()),
            &PropertyConstraint::Date,
        );
        assert!(v.is_none());
    }

    #[test]
    fn date_constraint_invalid() {
        let v = vc(
            "date",
            &Value::String("April 13".into()),
            &PropertyConstraint::Date,
        );
        assert!(matches!(
            v,
            Some(Violation {
                severity: Severity::Error,
                ..
            })
        ));
    }

    #[test]
    fn enum_constraint_valid() {
        let v = vc(
            "status",
            &Value::String("planned".into()),
            &PropertyConstraint::Enum {
                values: vec!["planned".into(), "done".into()],
            },
        );
        assert!(v.is_none());
    }

    #[test]
    fn enum_constraint_invalid_with_suggestion() {
        let v = vc(
            "status",
            &Value::String("planed".into()),
            &PropertyConstraint::Enum {
                values: vec!["planned".into(), "done".into()],
            },
        );
        let viol = v.expect("expected violation");
        assert_eq!(viol.severity, Severity::Error);
        assert!(viol.message.contains("did you mean \"planned\""));
    }

    #[test]
    fn number_constraint_valid() {
        let v = vc(
            "priority",
            &Value::Number(5.into()),
            &PropertyConstraint::Number,
        );
        assert!(v.is_none());
    }

    #[test]
    fn number_constraint_invalid() {
        let v = vc(
            "priority",
            &Value::String("five".into()),
            &PropertyConstraint::Number,
        );
        assert!(matches!(
            v,
            Some(Violation {
                severity: Severity::Error,
                ..
            })
        ));
    }

    #[test]
    fn boolean_constraint_valid() {
        let v = vc("draft", &Value::Bool(true), &PropertyConstraint::Boolean);
        assert!(v.is_none());
    }

    #[test]
    fn boolean_constraint_invalid() {
        let v = vc(
            "draft",
            &Value::String("yes".into()),
            &PropertyConstraint::Boolean,
        );
        assert!(matches!(
            v,
            Some(Violation {
                severity: Severity::Error,
                ..
            })
        ));
    }

    #[test]
    fn list_constraint_valid() {
        let v = vc("tags", &Value::Array(vec![]), &PropertyConstraint::List);
        assert!(v.is_none());
    }

    #[test]
    fn list_constraint_invalid() {
        let v = vc(
            "tags",
            &Value::String("rust".into()),
            &PropertyConstraint::List,
        );
        assert!(matches!(
            v,
            Some(Violation {
                severity: Severity::Error,
                ..
            })
        ));
    }

    #[test]
    fn string_pattern_constraint_valid() {
        let v = vc(
            "branch",
            &Value::String("iter-42/my-feature".into()),
            &PropertyConstraint::String {
                pattern: Some(r"^iter-\d+/".into()),
            },
        );
        assert!(v.is_none());
    }

    #[test]
    fn string_pattern_constraint_invalid() {
        let v = vc(
            "branch",
            &Value::String("feature/my-branch".into()),
            &PropertyConstraint::String {
                pattern: Some(r"^iter-\d+/".into()),
            },
        );
        assert!(matches!(
            v,
            Some(Violation {
                severity: Severity::Error,
                ..
            })
        ));
    }

    // --- lint_file via a temp file ---

    #[test]
    fn lint_file_missing_required() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "---\ntitle: Hello\n---\nBody\n").unwrap();

        let schema = make_schema(&["title", "date"], "note", &[], HashMap::new());
        let result = lint_file(&path, "note.md", &schema).unwrap();
        // date is in default required, but only "title" is present.
        // No type -> warn about no type. date missing -> error.
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.severity == Severity::Error
                    && v.message.contains("missing required property \"date\""))
        );
    }

    #[test]
    fn lint_file_no_type_warn() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "---\ntitle: Hello\n---\nBody\n").unwrap();

        let schema = make_schema(&["title"], "note", &[], HashMap::new());
        let result = lint_file(&path, "note.md", &schema).unwrap();
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.severity == Severity::Warn && v.message.contains("no 'type' property"))
        );
    }

    #[test]
    fn lint_file_no_violations_clean_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(
            &path,
            "---\ntitle: Hello\ntype: note\ntags:\n  - rust\n---\nBody\n",
        )
        .unwrap();

        let schema = make_schema(&["title"], "note", &[], HashMap::new());
        let result = lint_file(&path, "note.md", &schema).unwrap();
        assert!(result.violations.is_empty());
    }

    #[test]
    fn lint_no_schema_no_violations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "---\ntitle: Hello\n---\nBody\n").unwrap();

        let schema = SchemaConfig::default();
        let files = vec![(path, "note.md".to_owned())];
        let (_, counts) = lint_files(&files, &schema).unwrap();
        assert_eq!(counts.errors, 0);
        assert_eq!(counts.warnings, 0);
    }

    // --- UX-3: comma-joined tag detection and fix ---

    #[test]
    fn lint_warns_on_comma_joined_tag() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(
            &path,
            "---\ntitle: Hello\ntags:\n  - cli,ux\n  - rust\n---\nBody\n",
        )
        .unwrap();

        let schema = SchemaConfig::default();
        let result = lint_file(&path, "note.md", &schema).unwrap();
        let comma_warn = result
            .violations
            .iter()
            .find(|v| v.severity == Severity::Warn && v.message.contains("cli,ux"));
        assert!(
            comma_warn.is_some(),
            "expected a warning about comma-joined tag, got: {:#?}",
            result.violations
        );
        assert!(
            comma_warn.unwrap().message.contains("comma-joined"),
            "message should mention comma-joined"
        );
    }

    #[test]
    fn lint_fix_splits_comma_joined_tags() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(
            &path,
            "---\ntitle: Hello\ntags:\n  - cli,ux\n  - rust\n---\nBody\n",
        )
        .unwrap();

        let schema = SchemaConfig::default();
        let files = vec![(path.clone(), "note.md".to_owned())];
        let (_, counts) = lint_files_with_options(&files, &schema, FixMode::Apply, None).unwrap();

        // After fix, the comma-joined tag warning should be gone.
        assert_eq!(counts.warnings, 0, "comma-tag warning should be fixed");

        let content = std::fs::read_to_string(&path).unwrap();
        // Both parts of the split tag should be separate items.
        assert!(content.contains("- cli"), "expected 'cli' as separate tag");
        assert!(content.contains("- ux"), "expected 'ux' as separate tag");
        // The original comma-joined form must be gone.
        assert!(
            !content.contains("cli,ux"),
            "comma-joined tag should be removed"
        );
    }
}
