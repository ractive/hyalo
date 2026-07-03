/// `hyalo lint` — validate frontmatter properties against the `.hyalo.toml` schema.
///
/// Reads each file's frontmatter, applies the type-specific schema (or the
/// default schema if `type` is absent), and reports violations at two severity
/// levels:
///
///   - **error**  — schema violation (missing required field, wrong value type,
///     invalid enum value, failed pattern match, empty value on a list-typed
///     required property)
///   - **warn**   — soft issue (no `type` property, property not declared in
///     schema)
///
/// Exit code: 0 = clean, 1 = errors found, 2 = internal error.
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use hyalo_core::filename_template::FilenameTemplate;
use hyalo_core::frontmatter::{check_mtime, read_frontmatter, read_mtime, write_frontmatter};
use hyalo_core::scanner;
use hyalo_core::schema::{
    self, PropertyConstraint, SchemaConfig, TypeSchema, parse_required_section_entry,
};
use hyalo_core::util::is_iso8601_date;

use crate::commands::section_scanner::SectionScanner;

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

/// Stable identifier for schema warnings that strict mode promotes to errors.
///
/// Strict-mode promotion matches on these constants rather than the
/// user-facing `message`, so reworded messages don't silently disable
/// the promotion logic.
pub const VIOLATION_KIND_MISSING_TYPE: &str = "schema/missing-type";
pub const VIOLATION_KIND_UNDECLARED_PROPERTY: &str = "schema/undeclared-property";

/// A single lint violation found in a file.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub severity: Severity,
    pub message: String,
    /// Stable kind identifier for programmatic dispatch (e.g. strict-mode
    /// promotion in `lint_file`). `None` for ad-hoc violations that don't
    /// need to be matched programmatically.
    #[serde(skip)]
    pub kind: Option<&'static str>,
}

impl Default for Violation {
    fn default() -> Self {
        Self {
            severity: Severity::Warn,
            message: String::new(),
            kind: None,
        }
    }
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
    lint_files_with_options(files, schema, FixMode::Off, None, &mut None, None)
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
                kind: None,
                message: format!(
                    "view '{name}' has no narrowing filter — `fields` controls display columns only, \
                     not filtering. Did you mean `orphan = true` or `dead_end = true`?"
                ),
            });
        } else if !has_narrowing {
            violations.push(Violation {
                severity: Severity::Warn,
                kind: None,
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
    snapshot_index: &mut Option<hyalo_core::index::SnapshotIndex>,
    index_path: Option<&Path>,
) -> Result<(CommandOutcome, LintCounts)> {
    let mut results: Vec<FileLintResult> = Vec::new();
    let mut counts = LintCounts::default();
    let mut fix_results: Vec<FileFixResult> = Vec::new();
    let mut index_dirty = false;

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
            // If fixes were actually applied, update the snapshot index entry.
            if matches!(fix, FixMode::Apply) {
                let props = read_frontmatter(full_path)
                    .with_context(|| format!("reading fixed frontmatter from {rel_path}"))?;
                super::mutation::update_index_entry(
                    snapshot_index,
                    rel_path,
                    props,
                    full_path,
                    &mut index_dirty,
                )?;
            }
            fix_results.push(file_fixes);
        }
        if !file_result.violations.is_empty() {
            results.push(file_result);
        }
    }

    super::mutation::save_index_if_dirty(snapshot_index, index_path, index_dirty)?;

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
/// The `index_entries` iterator yields `(rel_path, properties)` tuples.
pub(crate) fn lint_counts_from_properties<'a>(
    entries: impl Iterator<Item = (&'a str, &'a IndexMap<String, Value>)>,
    schema: &SchemaConfig,
) -> LintCounts {
    let mut counts = LintCounts::default();
    for (rel_path, properties) in entries {
        let violations = validate_properties(rel_path, properties, schema);
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
                        kind: None,
                        message: format!("{}: {e}", crate::hints::PARSE_ERROR_PREFIX),
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

    let mut violations = validate_properties(rel_path, &final_props, schema);

    // Validate required_sections against the body outline.
    let doc_type: Option<String> = final_props.get("type").and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        _ => None,
    });
    let effective_schema: TypeSchema = match &doc_type {
        Some(t) => schema.merged_schema_for_type(t),
        None => schema.default_schema().clone(),
    };

    if !effective_schema.required_sections.is_empty() {
        let section_violations =
            validate_required_sections(full_path, rel_path, &effective_schema.required_sections)?;
        violations.extend(section_violations);
    }

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
// Required-sections body validation
// ---------------------------------------------------------------------------

/// Scan the body of `full_path` and check that each `required_sections` entry
/// appears in document order. Returns a `Violation` for each missing entry.
fn validate_required_sections(
    full_path: &Path,
    rel_path: &str,
    required_sections: &[String],
) -> Result<Vec<Violation>> {
    let mut ss = SectionScanner::new();
    scanner::scan_file_multi(full_path, &mut [&mut ss])
        .with_context(|| format!("scanning sections of {rel_path}"))?;
    let sections = ss.into_sections();

    let mut violations = Vec::new();
    let mut cursor = 0usize;
    for (ordinal, entry) in required_sections.iter().enumerate() {
        // parse_required_section_entry was validated at schema-load time, so this should
        // not fail here; treat errors as a lint violation rather than a hard error.
        let (level, text) = match parse_required_section_entry(entry) {
            Ok(t) => t,
            Err(e) => {
                violations.push(Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("invalid required-sections entry {entry:?} in schema: {e}"),
                });
                continue;
            }
        };

        // Walk sections forward from cursor, looking for matching level + trimmed text.
        let found = sections[cursor..].iter().enumerate().find(|(_, s)| {
            s.level == level
                && s.heading
                    .as_deref()
                    .is_some_and(|h| h.trim() == text.as_str())
        });

        if let Some((offset, _)) = found {
            cursor += offset + 1;
        } else {
            let hash_prefix = "#".repeat(level as usize);
            violations.push(Violation {
                severity: Severity::Error,
                kind: None,
                message: format!(
                    "missing required section: expected \"{hash_prefix} {text}\" at or after position {} in the outline",
                    ordinal + 1
                ),
            });
        }
    }

    Ok(violations)
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
            kind: None,
            message: format!("property \"type\" expected string, got {v}"),
        });
    }

    // Warn when no `type` property is present.
    if type_value.is_none() && !schema.is_empty() {
        violations.push(Violation {
            severity: Severity::Warn,
            kind: Some(VIOLATION_KIND_MISSING_TYPE),
            message: "no 'type' property — validating against default schema only".to_owned(),
        });
    }

    // Determine the effective schema for this file.
    let effective_schema: TypeSchema = match &doc_type {
        Some(t) => schema.merged_schema_for_type(t),
        None => schema.default_schema().clone(),
    };

    // Check required properties.
    //
    // A required property must be both present AND carry a meaningful value.
    // Null (`tags: ~`) and an empty array (`tags: []`) are treated as
    // semantically equivalent to absent — they convey no information and a
    // required key whose value is "nothing here" should fail the same gate as
    // a missing key. Atomic-typed required properties only need to be present
    // (an empty string or zero satisfies them); checking those is a separate
    // constraint and not handled here.
    let type_hint = doc_type
        .as_deref()
        .map(|t| format!(" (type: {t})"))
        .unwrap_or_default();
    for req in &effective_schema.required {
        match properties.get(req.as_str()) {
            None => {
                violations.push(Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("missing required property \"{req}\"{type_hint}"),
                });
            }
            Some(v) if v.is_null() || v.as_array().is_some_and(Vec::is_empty) => {
                violations.push(Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("required property \"{req}\" must not be empty{type_hint}"),
                });
            }
            _ => {}
        }
    }

    // Build a per-call regex cache so the same pattern isn't recompiled across
    // properties (this matters in `hyalo summary`, which runs lint over the full
    // index).
    let mut regex_cache: HashMap<String, Result<Regex, String>> = HashMap::new();

    // Type-specific property constraint validation.
    for (name, value) in properties {
        // `tags` is validated against its declared constraint if present, but it
        // is never reported as an undeclared property: presence of a `tags` key
        // without a schema entry for it is intentional, not a misconfiguration.
        if name == "tags" {
            if let Some(constraint) = effective_schema.properties.get(name.as_str()) {
                violations.extend(validate_constraint(
                    name,
                    value,
                    constraint,
                    &mut regex_cache,
                ));
            }
            // Check for comma-joined tags (e.g. "cli,ux" instead of ["cli", "ux"]).
            if let Value::Array(items) = value {
                for item in items {
                    if let Value::String(tag) = item
                        && tag.contains(',')
                    {
                        violations.push(Violation {
                            severity: Severity::Warn,
                            kind: None,
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
            violations.extend(validate_constraint(
                name,
                value,
                constraint,
                &mut regex_cache,
            ));
        } else if !effective_schema.properties.is_empty() && !implicitly_accepted {
            // Property not declared in schema — warn only when the schema declares
            // some properties. Schemas that only specify `required` remain
            // intentionally permissive about extra fields.
            violations.push(Violation {
                severity: Severity::Warn,
                kind: Some(VIOLATION_KIND_UNDECLARED_PROPERTY),
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
) -> Vec<Violation> {
    match constraint {
        PropertyConstraint::String { pattern } => {
            let Some(s) = value_as_str(value) else {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected string, got {value}"),
                }];
            };
            if let Some(pat) = pattern {
                // Compile (or look up) the regex once per pattern per call.
                let entry = regex_cache
                    .entry(pat.clone())
                    .or_insert_with(|| Regex::new(pat).map_err(|e| e.to_string()));
                match entry {
                    Ok(re) => {
                        if !re.is_match(s) {
                            return vec![Violation {
                                severity: Severity::Error,
                                kind: None,
                                message: format!(
                                    "property \"{name}\" value {s:?} does not match pattern {pat:?}"
                                ),
                            }];
                        }
                    }
                    Err(e) => {
                        return vec![Violation {
                            severity: Severity::Error,
                            kind: None,
                            message: format!("property \"{name}\": invalid pattern {pat:?}: {e}"),
                        }];
                    }
                }
            }
            vec![]
        }
        PropertyConstraint::Date => {
            let Some(s) = value_as_str(value) else {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected date (YYYY-MM-DD), got {value}"),
                }];
            };
            if !is_iso8601_date(s) {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected date (YYYY-MM-DD), got \"{s}\""),
                }];
            }
            vec![]
        }
        PropertyConstraint::DateTime => {
            let Some(s) = value_as_str(value) else {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!(
                        "property \"{name}\" expected datetime (YYYY-MM-DDThh:mm:ss), got {value}"
                    ),
                }];
            };
            if !hyalo_core::util::is_iso8601_datetime(s) {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!(
                        "property \"{name}\" expected datetime (YYYY-MM-DDThh:mm:ss), got \"{s}\""
                    ),
                }];
            }
            vec![]
        }
        PropertyConstraint::Number => {
            if !matches!(value, Value::Number(_)) {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected number, got {value}"),
                }];
            }
            vec![]
        }
        PropertyConstraint::Boolean => {
            if !matches!(value, Value::Bool(_)) {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected boolean, got {value}"),
                }];
            }
            vec![]
        }
        PropertyConstraint::List => {
            if !matches!(value, Value::Array(_)) {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected list, got {value}"),
                }];
            }
            vec![]
        }
        PropertyConstraint::Enum { values } => {
            let Some(s) = value_as_str(value) else {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!(
                        "property \"{name}\" expected one of [{}], got {value}",
                        values.join(", ")
                    ),
                }];
            };
            if values.contains(&s.to_owned()) {
                return vec![];
            }
            // Find nearest suggestion via Levenshtein.
            let suggestion = values
                .iter()
                .min_by_key(|v| strsim::levenshtein(s, v.as_str()))
                .map(|v| format!(" (did you mean \"{v}\"?)"))
                .unwrap_or_default();
            vec![Violation {
                severity: Severity::Error,
                kind: None,
                message: format!(
                    "property \"{name}\" value \"{s}\" not in [{}]{suggestion}",
                    values.join(", ")
                ),
            }]
        }
        PropertyConstraint::StringList { item_pattern } => {
            let Value::Array(items) = value else {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected string-list, got {value}"),
                }];
            };
            let Some(pat) = item_pattern else {
                // No per-item pattern — collect a violation for every non-string item.
                return items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| !matches!(item, Value::String(_)))
                    .map(|(i, item)| Violation {
                        severity: Severity::Error,
                        kind: None,
                        message: format!(
                            "property \"{name}\" item {i}: expected string, got {item}"
                        ),
                    })
                    .collect();
            };
            // Compile (or look up) the regex once per pattern per call.
            let entry = regex_cache
                .entry(pat.clone())
                .or_insert_with(|| Regex::new(pat).map_err(|e| e.to_string()));
            let re = match entry {
                Err(e) => {
                    return vec![Violation {
                        severity: Severity::Error,
                        kind: None,
                        message: format!("property \"{name}\": invalid item_pattern {pat:?}: {e}"),
                    }];
                }
                Ok(re) => re,
            };
            // Collect a violation for every item that is not a string or fails the pattern.
            items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    let Value::String(s) = item else {
                        return Some(Violation {
                            severity: Severity::Error,
                            kind: None,
                            message: format!(
                                "property \"{name}\" item {i}: expected string, got {item}"
                            ),
                        });
                    };
                    if re.is_match(s) {
                        None
                    } else {
                        Some(Violation {
                            severity: Severity::Error,
                            kind: None,
                            message: format!(
                                "property \"{name}\" item {i}: value {s:?} does not match pattern {pat:?}"
                            ),
                        })
                    }
                })
                .collect()
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
    validate_constraint(name, value, constraint, &mut cache)
        .into_iter()
        .next()
        .map(|v| v.message)
}

// ---------------------------------------------------------------------------
// Text formatter
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Extended body-lint types (new output shape per plan)
// ---------------------------------------------------------------------------

/// A group of violations for one rule within one file.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RuleGroup {
    pub rule: String,
    pub count: usize,
    pub shown: usize,
    pub truncated: bool,
    pub severity: String,
    pub autofixable: bool,
    pub violations: Vec<BodyViolation>,
}

/// A single body violation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BodyViolation {
    pub line: usize,
    pub column: usize,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<serde_json::Value>,
}

/// Extended lint output for one file (read-only shape).
#[derive(Debug, serde::Serialize)]
pub struct ExtFileLintResult {
    pub file: String,
    /// Frontmatter `type:` discriminator, if the file declared one. Used by
    /// the hint layer to surface `hyalo types show <T>` for SCHEMA failures.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub doc_type: Option<String>,
    /// Frontmatter + body violations grouped by rule.
    pub rule_groups: Vec<RuleGroup>,
}

/// One entry in `fixed_groups`: a rule + count of violations that were fixed.
/// Includes `violations` so text renderers can show line/message details.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FixedGroup {
    pub rule: String,
    pub count: usize,
    /// Violations that were fixed (same shape as `RuleGroup.violations`).
    pub violations: Vec<BodyViolation>,
}

/// One entry in `conflicts`: a rule whose fix was skipped due to range overlap.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConflictEntry {
    pub rule: String,
    pub reason: String,
}

/// Extended lint output for one file in fix-mode.
#[derive(Debug, serde::Serialize)]
pub struct ExtFileLintFixResult {
    pub file: String,
    /// Frontmatter `type:` discriminator, if the file declared one. Mirrors
    /// [`ExtFileLintResult::doc_type`] so the iter-143 SCHEMA-→-`types show`
    /// hint also fires in `--fix` / `--fix --dry-run` output.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub doc_type: Option<String>,
    /// Rules that had fixes applied (or would be in DryRun).
    pub fixed_groups: Vec<FixedGroup>,
    /// Rules with violations that remain after fixing.
    pub remaining_groups: Vec<RuleGroup>,
    /// Rules whose fixes were skipped due to conflicts.
    pub conflicts: Vec<ConflictEntry>,
}

/// Full extended lint output (read-only mode).
#[derive(Debug, serde::Serialize)]
pub struct ExtLintOutput {
    pub files: Vec<ExtFileLintResult>,
    pub total: usize,
    pub rules_fired: usize,
    pub files_with_violations: usize,
    /// Total number of files that were examined (including clean files).
    pub files_checked: usize,
    pub files_truncated: bool,
    /// Number of error-severity violations.
    pub errors: usize,
    /// Number of warn-severity violations.
    pub warnings: usize,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dry_run: bool,
    /// Frontmatter fix actions applied (or previewed) per file.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<FileFixResult>,
}

/// Full extended lint output in fix-mode.
#[derive(Debug, serde::Serialize)]
pub struct ExtLintFixOutput {
    pub files: Vec<ExtFileLintFixResult>,
    pub total_fixed: usize,
    pub total_remaining: usize,
    pub total_conflicts: usize,
    pub rules_fired: usize,
    pub files_with_violations: usize,
    pub files_checked: usize,
    pub files_truncated: bool,
    pub errors: usize,
    pub warnings: usize,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dry_run: bool,
}

/// Options for the extended lint run.
pub struct ExtLintOptions<'a> {
    pub fix: FixMode,
    pub detailed: bool,
    pub rule_filter: Option<&'a str>,
    pub rule_prefix: Option<&'a str>,
    pub max_per_rule: usize,
    pub max_files: usize,
    pub fix_rules: &'a [String],
    /// Snapshot index for patching after fixes.
    pub snapshot_index: &'a mut Option<hyalo_core::index::SnapshotIndex>,
    pub index_path: Option<&'a Path>,
    pub vault_dir: &'a Path,
    /// When `true`, promote "no 'type' property" and "undeclared property in
    /// frontmatter" from `Severity::Warn` to `Severity::Error`.
    pub strict: bool,
}

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
        (None, Some(prefix)) => md_lint_engine
            .available_rules()
            .iter()
            .filter(|e| e.id.starts_with(prefix))
            .map(|e| e.id.clone())
            .collect(),
        (None, None) => vec![],
    };

    // Determine if schema has `status: completed` in any type.
    let schema_has_completed = schema_has_completed_status(schema);

    let strict = opts.strict;

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

    // Handle frontmatter --fix index patching.
    for (full_path, rel_path) in files {
        // Check if frontmatter was modified (tracked by the frontmatter pass).
        // We check by re-reading if the file was written.
        // Actually the frontmatter pass writes inline — we need to patch for all modified.
        let _ = (full_path, rel_path); // covered by per_file above
    }

    // Patch index for body-modified files.
    if !modified_files.is_empty() {
        crate::dispatch::patch_index_for_modified_files_pub(
            opts.snapshot_index,
            opts.index_path,
            opts.vault_dir,
            &modified_files,
        )?;
    }

    // Sort by total violations descending (worst offenders first).
    all_results.sort_by_key(|r| std::cmp::Reverse(r.total_violations));

    // Cap files.
    let total_files_with_violations = all_results
        .iter()
        .filter(|r| r.total_violations > 0)
        .count();
    let files_checked_total = all_results.len();
    let files_truncated = all_results.len() > opts.max_files;
    all_results.truncate(opts.max_files);

    let is_fix_mode = matches!(opts.fix, FixMode::Apply | FixMode::DryRun);

    // Shared counters used by both output paths.
    let mut total_errors = 0usize;
    let mut total_warnings = 0usize;
    let mut rules_seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let val = if is_fix_mode {
        // -------------------------------------------------------------------
        // Fix-mode output: fixed_groups / remaining_groups / conflicts shape.
        // -------------------------------------------------------------------
        let mut output_fix_files: Vec<ExtFileLintFixResult> = Vec::new();
        let mut grand_total_fixed = 0usize;
        let mut grand_total_remaining = 0usize;
        let mut grand_total_conflicts = 0usize;

        for r in &all_results {
            // Build a set of (rule_id, order-within-rule) → FixOutcome from
            // body_fix_outcomes.  We use a Vec<(rule, outcome)> indexed by the
            // position among fixable diagnostics (same order as fixable_indices).
            // Easier to just build maps indexed by rule.
            let mut applied_by_rule: indexmap::IndexMap<String, Vec<BodyViolation>> =
                indexmap::IndexMap::new();
            let mut conflict_by_rule: indexmap::IndexMap<String, String> =
                indexmap::IndexMap::new();

            for (rule_id, outcome) in &r.body_fix_outcomes {
                match outcome {
                    FixOutcome::Applied => {
                        applied_by_rule.entry(rule_id.clone()).or_default();
                    }
                    FixOutcome::Conflict { blocking_rule } => {
                        conflict_by_rule
                            .entry(rule_id.clone())
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
                grand_total_fixed += schema_fix_count;
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
                    grand_total_fixed += count;
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
                                })
                                .collect()
                        };
                    let remaining = remaining_owned.len();
                    if remaining == 0 {
                        continue;
                    }
                    grand_total_remaining += remaining;
                    for v in &remaining_owned {
                        match v.severity.as_str() {
                            "error" => total_errors += 1,
                            _ => total_warnings += 1,
                        }
                    }
                    rules_seen.insert(rule_id.clone());
                    let shown = remaining.min(opts.max_per_rule);
                    let truncated = remaining > shown;
                    let body_violations = remaining_owned
                        .iter()
                        .take(shown)
                        .map(|v| BodyViolation {
                            line: v.line,
                            column: v.column,
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
                        autofixable: true,
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
                grand_total_remaining += remaining_count;
                for v in &remaining_violations {
                    match v.severity.as_str() {
                        "error" => total_errors += 1,
                        _ => total_warnings += 1,
                    }
                }
                rules_seen.insert(rule_id.clone());

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

            // conflicts: rules with at least one conflicting fix.
            let mut conflicts: Vec<ConflictEntry> = Vec::new();
            for (rule_id, blocking_rule) in &conflict_by_rule {
                grand_total_conflicts += 1;
                conflicts.push(ConflictEntry {
                    rule: rule_id.clone(),
                    reason: format!("range overlap with {blocking_rule}"),
                });
            }

            if !fixed_groups.is_empty() || !remaining_groups.is_empty() || !conflicts.is_empty() {
                output_fix_files.push(ExtFileLintFixResult {
                    file: r.rel_path.clone(),
                    doc_type: r.doc_type.clone(),
                    fixed_groups,
                    remaining_groups,
                    conflicts,
                });
            }
        }

        let fix_output = ExtLintFixOutput {
            files: output_fix_files,
            total_fixed: grand_total_fixed,
            total_remaining: grand_total_remaining,
            total_conflicts: grand_total_conflicts,
            rules_fired: rules_seen.len(),
            files_with_violations: total_files_with_violations,
            files_checked: files_checked_total,
            files_truncated,
            errors: total_errors,
            warnings: total_warnings,
            dry_run: matches!(opts.fix, FixMode::DryRun),
        };
        serde_json::to_value(&fix_output).context("failed to serialize fix lint output")?
    } else {
        // -------------------------------------------------------------------
        // Read-only output: unchanged rule_groups shape.
        // -------------------------------------------------------------------
        let mut total_violations = 0usize;
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
                total_violations += count;
                for v in violations {
                    match v.severity.as_str() {
                        "error" => total_errors += 1,
                        _ => total_warnings += 1,
                    }
                }
                rules_seen.insert(rule_id.clone());

                let autofixable = if rule_id == "SCHEMA" {
                    true
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
            total: total_violations,
            rules_fired: rules_seen.len(),
            files_with_violations: total_files_with_violations,
            files_checked: files_checked_total,
            files_truncated,
            errors: total_errors,
            warnings: total_warnings,
            dry_run: false,
            fixes: all_fix_actions,
        };
        serde_json::to_value(&output).context("failed to serialize extended lint output")?
    };

    // Recompute counts from what we tracked above.
    let counts = LintCounts {
        errors: total_errors,
        warnings: total_warnings,
        files_with_issues: total_files_with_violations,
    };

    let outcome = CommandOutcome::success_with_total(
        crate::output::format_success(Format::Json, &val),
        total_files_with_violations as u64,
    );

    Ok((outcome, counts))
}

/// Per-file violation entry (internal).
struct InternalViolation {
    line: usize,
    column: usize,
    message: String,
    severity: String,
    fix: Option<serde_json::Value>,
    /// True when this body diagnostic's fix was successfully applied during
    /// the current fix-mode run. Always `false` for read-only and frontmatter
    /// (SCHEMA) violations — frontmatter fixes are tracked separately via
    /// `fix_actions`.
    fixed: bool,
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
fn group_severity(violations: &[InternalViolation]) -> String {
    if violations.iter().any(|v| v.severity == "error") {
        "error".to_owned()
    } else {
        "warn".to_owned()
    }
}

/// Outcome for a single diagnostic's fix attempt (internal, used in fix-mode).
#[derive(Debug)]
enum FixOutcome {
    /// Fix was applied (or would be in DryRun).
    Applied,
    /// Fix conflicted with another fix's range.
    Conflict { blocking_rule: String },
    /// Diagnostic had no fix, or fix was not selected.
    NoFix,
}

/// Per-file lint result (internal, before grouping).
struct PerFileLintResult {
    rel_path: String,
    /// Frontmatter `type:` discriminator, if declared. Propagated into
    /// `ExtFileLintResult.doc_type` for the hint layer.
    doc_type: Option<String>,
    violations_by_rule: indexmap::IndexMap<String, Vec<InternalViolation>>,
    total_violations: usize,
    body_modified: bool,
    /// Frontmatter fix actions applied or previewed.
    fix_actions: Vec<FixAction>,
    /// Fix outcomes keyed by (rule_id, index_within_rule).
    /// Only populated in fix-mode.
    body_fix_outcomes: Vec<(String, FixOutcome)>,
    /// SCHEMA (frontmatter) violations remaining *after* applying fixes,
    /// computed by re-running `validate_properties` against the mutated
    /// frontmatter. `Some(vec![])` means all SCHEMA violations were resolved;
    /// `None` means fix-mode was off or no SCHEMA pass ran. Body rules use
    /// `InternalViolation.fixed` instead.
    post_fix_schema_remaining: Option<Vec<InternalViolation>>,
}

/// Lint a single file (frontmatter + body). Returns a `PerFileLintResult`.
#[allow(clippy::too_many_arguments)]
fn lint_one_file_extended(
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

    // Read the file content once.
    let content =
        std::fs::read_to_string(full_path).with_context(|| format!("reading {rel_path}"))?;

    // Find where the frontmatter ends so we can split body.
    let body_start = find_body_start(&content);
    let body_content = &content[body_start..];

    // Frontmatter pass: use existing logic but convert to new shape.
    let properties = match hyalo_core::frontmatter::read_frontmatter(full_path) {
        Ok(p) => p,
        Err(e) if hyalo_core::frontmatter::is_parse_error(&e) => {
            // Malformed frontmatter — report as a single violation.
            let mut violations_by_rule = indexmap::IndexMap::new();
            violations_by_rule.insert(
                "FRONTMATTER".to_owned(),
                vec![InternalViolation {
                    line: 1,
                    column: 1,
                    message: format!("{}: {e}", crate::hints::PARSE_ERROR_PREFIX),
                    severity: "error".to_owned(),
                    fix: None,
                    fixed: false,
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

    // Frontmatter violations → use the existing `validate_properties` but map to new shape.
    // Only emit if the rule isn't filtered out.
    let should_include_frontmatter = rule_filter.is_empty()
        || rule_filter
            .iter()
            .any(|r| r.starts_with("FRONTMATTER") || r == "SCHEMA");
    if should_include_frontmatter {
        let mut fm_violations = validate_properties(rel_path, &properties, schema);

        // Under --strict, the missing-type warning is promoted to an error.
        // `validate_properties` only emits it when the schema is non-empty, but
        // strict mode should catch missing `type` regardless — inject it when
        // the schema was empty and the property is absent (BUG-3 / iter-133).
        let already_has_missing_type = fm_violations
            .iter()
            .any(|v| v.kind == Some(VIOLATION_KIND_MISSING_TYPE));
        if strict && !already_has_missing_type && !properties.contains_key("type") {
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
            });
    }

    // HYALO004 — datetime-format: schema-declared datetime properties must
    // hold `YYYY-MM-DDThh:mm:ss` values.
    let doc_type_for_dt: Option<&str> = properties.get("type").and_then(|v| v.as_str());
    let effective_schema_for_dt: TypeSchema = match doc_type_for_dt {
        Some(t) => schema.merged_schema_for_type(t),
        None => schema.default_schema().clone(),
    };
    let datetime_pairs: Vec<(&str, &str)> = effective_schema_for_dt
        .properties
        .iter()
        .filter(|(_, c)| matches!(c, PropertyConstraint::DateTime))
        .filter_map(|(name, _)| {
            let v = properties.get(name.as_str())?;
            let s = v.as_str()?;
            Some((name.as_str(), s))
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
        .and_then(|v| v.as_str())
        .map(str::to_owned);
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
                if matches!(fix, FixMode::Apply) {
                    check_mtime(full_path, mtime0)?;
                    write_frontmatter(full_path, &mutable)
                        .with_context(|| format!("writing fixed frontmatter to {rel_path}"))?;
                    // Re-baseline: the write above legitimately changed the
                    // file's mtime, and a later body-fix write in this same
                    // call must not mistake it for a concurrent modification.
                    mtime0 = read_mtime(full_path).with_context(|| {
                        format!("re-reading mtime for {rel_path} after frontmatter fix")
                    })?;
                }
                fix_actions = actions;
            }
            if should_include_frontmatter {
                // Re-validate the mutated properties to get the actual
                // post-fix SCHEMA remaining set. Avoids guessing via
                // `fix_actions.len()`, which is not 1:1 with resolved
                // diagnostics (one fix action can clear multiple violations,
                // or insert defaults that don't clear any).
                let post = validate_properties(rel_path, &mutable, schema);
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
                        }
                    })
                    .collect();
                post_fix_schema_remaining = Some(remaining);
            }
            // Update post_fix_doc_type from the (possibly-inferred) mutable properties.
            post_fix_doc_type = mutable
                .get("type")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
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
    let mut body_fix_outcomes: Vec<(String, FixOutcome)> = Vec::new();
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
                match &outcomes[slot] {
                    FixOutcome::Applied => {
                        applied_this_pass.insert(orig_idx);
                        body_fix_outcomes.push((rule_id, FixOutcome::Applied));
                    }
                    FixOutcome::Conflict { blocking_rule } => {
                        body_fix_outcomes.push((
                            rule_id,
                            FixOutcome::Conflict {
                                blocking_rule: blocking_rule.clone(),
                            },
                        ));
                    }
                    FixOutcome::NoFix => body_fix_outcomes.push((rule_id, FixOutcome::NoFix)),
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

    if matches!(fix, FixMode::Apply) && working_body != body_content {
        // Re-derive the frontmatter bytes fresh from disk when a
        // frontmatter fix already landed above — `content[..body_start]` is
        // a snapshot from before that write and would silently revert it if
        // reused here.
        let frontmatter_part: Cow<'_, str> = if fix_actions.is_empty() {
            Cow::Borrowed(&content[..body_start])
        } else {
            let fresh = std::fs::read_to_string(full_path)
                .with_context(|| format!("re-reading {rel_path} after frontmatter fix"))?;
            let fresh_body_start = find_body_start(&fresh);
            Cow::Owned(fresh[..fresh_body_start].to_owned())
        };
        check_mtime(full_path, mtime0)?;
        let new_content = format!("{frontmatter_part}{working_body}");
        hyalo_core::fs_util::atomic_write(full_path, new_content.as_bytes())
            .with_context(|| format!("writing fixed body to {rel_path}"))?;
        body_modified = true;
    }

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
            line: d.line,
            column: d.column,
            message: d.message,
            severity: format!("{}", d.severity),
            fix,
            fixed,
        }
    };
    for d in fixed_diagnostics {
        let rule_id = d.rule_id.clone();
        violations_by_rule
            .entry(rule_id)
            .or_default()
            .push(diag_to_violation(d, true));
    }
    for d in current_diagnostics {
        let rule_id = d.rule_id.clone();
        violations_by_rule
            .entry(rule_id)
            .or_default()
            .push(diag_to_violation(d, false));
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
fn apply_body_fixes(body: &str, fixes: &[&hyalo_mdlint::Diagnostic]) -> (String, Vec<FixOutcome>) {
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
        if end > body.len() {
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
        if result[start..end] == *replacement {
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
fn find_body_start(content: &str) -> usize {
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

/// Check whether any schema type declares `status` as an enum with `completed`.
fn schema_has_completed_status(schema: &SchemaConfig) -> bool {
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

fn has_completed_in_type(props: &std::collections::HashMap<String, PropertyConstraint>) -> bool {
    if let Some(PropertyConstraint::Enum { values }) = props.get("status") {
        return values.iter().any(|v| v == "completed");
    }
    false
}

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
    // Returns the first violation (or None) for constraints that produce at most one.
    fn vc(name: &str, value: &Value, c: &PropertyConstraint) -> Option<Violation> {
        let mut cache = HashMap::new();
        validate_constraint(name, value, c, &mut cache)
            .into_iter()
            .next()
    }

    // Test helper: returns all violations from `validate_constraint`.
    fn vc_all(name: &str, value: &Value, c: &PropertyConstraint) -> Vec<Violation> {
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

    // --- Strict mode unit tests ---

    fn make_schema_with_declared_prop() -> SchemaConfig {
        // Schema with a declared `note` type that has `title` as required and
        // `date` as a declared property, so any other property is "undeclared".
        make_schema(&["title"], "note", &[], HashMap::new())
    }

    /// Helper: run lint in extended mode on a single file.
    fn lint_extended_strict(
        path: &std::path::Path,
        rel: &str,
        schema: &SchemaConfig,
        strict: bool,
    ) -> (crate::output::CommandOutcome, LintCounts) {
        let engine = hyalo_mdlint::HyaloLintEngine::create().unwrap();
        let md_config = hyalo_mdlint::LintConfig::default();
        let files = vec![(path.to_path_buf(), rel.to_owned())];
        let mut snapshot: Option<hyalo_core::index::SnapshotIndex> = None;
        let vault_dir = path.parent().unwrap();
        let mut opts = ExtLintOptions {
            fix: FixMode::Off,
            detailed: false,
            rule_filter: None,
            rule_prefix: None,
            max_per_rule: 100,
            max_files: 100,
            fix_rules: &[],
            snapshot_index: &mut snapshot,
            index_path: None,
            vault_dir,
            strict,
        };
        lint_files_extended(&files, schema, &engine, &md_config, &mut opts).unwrap()
    }

    /// In strict mode the "no 'type' property" warning becomes an error.
    #[test]
    fn strict_mode_promotes_no_type_to_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no_type.md");
        std::fs::write(&path, "---\ntitle: Hello\n---\nBody\n").unwrap();

        let schema = make_schema_with_declared_prop();
        let (_, counts) = lint_extended_strict(&path, "no_type.md", &schema, true);
        assert!(counts.errors > 0, "strict mode: no-type should be an error");
    }

    /// Without strict mode the "no 'type' property" is still a warning.
    #[test]
    fn non_strict_no_type_stays_warn() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no_type.md");
        std::fs::write(&path, "---\ntitle: Hello\n---\nBody\n").unwrap();

        let schema = make_schema_with_declared_prop();
        let (_, counts) = lint_extended_strict(&path, "no_type.md", &schema, false);
        assert_eq!(
            counts.errors, 0,
            "non-strict: no-type should remain a warning"
        );
        assert!(counts.warnings > 0, "non-strict: warnings expected");
    }

    /// In strict mode, undeclared properties become errors.
    #[test]
    fn strict_mode_promotes_undeclared_property_to_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("undeclared.md");
        // `type: note` present (avoids no-type warning), but `unknown_prop` is not
        // declared in the note schema's `properties` map.
        std::fs::write(
            &path,
            "---\ntitle: Hello\ntype: note\nunknown_prop: oops\n---\nBody\n",
        )
        .unwrap();

        let schema = {
            // Build a schema where `note` has declared `properties` so
            // the undeclared-property path fires.
            use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};
            let mut schema = SchemaConfig::default();
            let mut ts = TypeSchema::default();
            ts.required.push("title".to_owned());
            ts.properties.insert(
                "title".to_owned(),
                PropertyConstraint::String { pattern: None },
            );
            schema.types.insert("note".to_owned(), ts);
            schema
        };

        let (_, counts) = lint_extended_strict(&path, "undeclared.md", &schema, true);
        assert!(
            counts.errors > 0,
            "strict: undeclared prop should be an error"
        );
    }

    /// A required property whose value is an empty `[]` is an error: an empty
    /// list is semantically equivalent to absent for a required field. The
    /// rule is value-shape driven and fires whether or not the property has a
    /// List constraint declared.
    #[test]
    fn required_property_empty_array_is_error() {
        use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty_tags.md");
        std::fs::write(
            &path,
            "---\ntitle: Hello\ntype: note\ntags: []\n---\nBody\n",
        )
        .unwrap();

        let schema = {
            let mut schema = SchemaConfig::default();
            let mut ts = TypeSchema::default();
            ts.required.push("title".to_owned());
            ts.required.push("tags".to_owned());
            ts.properties
                .insert("tags".to_owned(), PropertyConstraint::List);
            schema.types.insert("note".to_owned(), ts);
            schema
        };

        let (outcome, counts) = lint_extended_strict(&path, "empty_tags.md", &schema, false);
        assert!(counts.errors > 0, "empty required list should error");
        let body = match outcome {
            crate::output::CommandOutcome::Success { output, .. } => output,
            other => panic!("expected Success outcome, got: {other:?}"),
        };
        assert!(
            body.contains("must not be empty") && body.contains("tags"),
            "expected empty-required error mentioning tags in output, got: {body}"
        );
    }

    /// A required property explicitly set to YAML null (`tags: ~`) is also
    /// treated as empty — null carries no information, same as an absent key.
    /// Without this, a typo or stripped value silently passes the required gate.
    #[test]
    fn required_property_null_value_is_error() {
        use hyalo_core::schema::{SchemaConfig, TypeSchema};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("null_tags.md");
        std::fs::write(&path, "---\ntitle: Hello\ntype: note\ntags: ~\n---\nBody\n").unwrap();

        let schema = {
            let mut schema = SchemaConfig::default();
            let mut ts = TypeSchema::default();
            ts.required.push("tags".to_owned());
            schema.types.insert("note".to_owned(), ts);
            schema
        };

        let (outcome, counts) = lint_extended_strict(&path, "null_tags.md", &schema, false);
        assert!(counts.errors > 0, "null required property should error");
        let body = match outcome {
            crate::output::CommandOutcome::Success { output, .. } => output,
            other => panic!("expected Success outcome, got: {other:?}"),
        };
        assert!(
            body.contains("must not be empty") && body.contains("tags"),
            "expected empty-required error mentioning tags in output, got: {body}"
        );
    }

    /// A required atomic-typed property satisfied by any value (including a
    /// zero-ish one like `0` or `""`) is *not* an error from the
    /// non-empty-list check — only sequence-typed required properties get the
    /// extra emptiness gate.
    #[test]
    fn required_non_list_property_is_unaffected_by_empty_check() {
        use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty_title.md");
        std::fs::write(&path, "---\ntitle: \"\"\ntype: note\n---\nBody\n").unwrap();

        let schema = {
            let mut schema = SchemaConfig::default();
            let mut ts = TypeSchema::default();
            ts.required.push("title".to_owned());
            ts.properties.insert(
                "title".to_owned(),
                PropertyConstraint::String { pattern: None },
            );
            schema.types.insert("note".to_owned(), ts);
            schema
        };

        let (_, counts) = lint_extended_strict(&path, "empty_title.md", &schema, false);
        assert_eq!(
            counts.errors, 0,
            "empty required string is not flagged here"
        );
    }

    /// A file with `type` but no `tags` produces zero violations against a
    /// schema that doesn't require `tags`. The previously-hardcoded "no tags
    /// defined" warning was removed in iter-156 — opt in via `required` if you
    /// want enforcement.
    #[test]
    fn missing_tags_is_not_a_violation_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no_tags.md");
        std::fs::write(&path, "---\ntitle: Hello\ntype: note\n---\nBody\n").unwrap();

        let schema = make_schema_with_declared_prop();
        let (_, counts) = lint_extended_strict(&path, "no_tags.md", &schema, false);
        assert_eq!(counts.errors, 0, "missing tags should not be an error");
        assert_eq!(counts.warnings, 0, "missing tags should not warn");
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
        let (_, counts) =
            lint_files_with_options(&files, &schema, FixMode::Apply, None, &mut None, None)
                .unwrap();

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

    // ---------------------------------------------------------------------------
    // item_pattern tests
    // ---------------------------------------------------------------------------

    #[test]
    fn item_pattern_validates_list_items() {
        // First item matches, second does not.
        let constraint = PropertyConstraint::StringList {
            item_pattern: Some(r"^[a-z]+$".to_owned()),
        };
        let v = vc(
            "tags",
            &Value::Array(vec![
                Value::String("rust".into()),
                Value::String("Rust123".into()), // uppercase — should fail
            ]),
            &constraint,
        );
        let viol = v.expect("expected a violation");
        assert_eq!(viol.severity, Severity::Error);
        assert!(
            viol.message.contains("item 1"),
            "expected item index in message, got: {}",
            viol.message
        );
        assert!(
            viol.message.contains(r"^[a-z]+$"),
            "expected pattern in message, got: {}",
            viol.message
        );
    }

    #[test]
    fn item_pattern_vacuous_on_empty_list() {
        let constraint = PropertyConstraint::StringList {
            item_pattern: Some(r"^[a-z]+$".to_owned()),
        };
        let v = vc("tags", &Value::Array(vec![]), &constraint);
        assert!(v.is_none(), "empty list should produce no violations");
    }

    #[test]
    fn item_pattern_non_string_item_errors() {
        let constraint = PropertyConstraint::StringList { item_pattern: None };
        let v = vc(
            "tags",
            &Value::Array(vec![Value::Number(42.into())]),
            &constraint,
        );
        let viol = v.expect("expected a violation");
        assert_eq!(viol.severity, Severity::Error);
        assert!(
            viol.message.contains("item 0"),
            "expected item index in message, got: {}",
            viol.message
        );
        assert!(
            viol.message.contains("expected string"),
            "expected type error message, got: {}",
            viol.message
        );
    }

    #[test]
    fn item_pattern_reports_all_violations() {
        // Three items: first valid, second and third fail the pattern.
        let constraint = PropertyConstraint::StringList {
            item_pattern: Some(r"^[a-z][a-z0-9-]*$".to_owned()),
        };
        let violations = vc_all(
            "tags",
            &Value::Array(vec![
                Value::String("good-tag".into()),
                Value::String("Bad".into()),  // uppercase start — fails
                Value::String("1bad".into()), // digit start — fails
                Value::String("also-good".into()),
                Value::String("Bar".into()), // uppercase start — fails
            ]),
            &constraint,
        );
        assert_eq!(
            violations.len(),
            3,
            "expected 3 violations, got: {:?}",
            violations.iter().map(|v| &v.message).collect::<Vec<_>>()
        );
        assert!(
            violations[0].message.contains("item 1"),
            "first violation should reference item 1"
        );
        assert!(
            violations[1].message.contains("item 2"),
            "second violation should reference item 2"
        );
        assert!(
            violations[2].message.contains("item 4"),
            "third violation should reference item 4"
        );
        for v in &violations {
            assert_eq!(v.severity, Severity::Error);
        }
    }

    #[test]
    fn item_pattern_multiple_non_string_items_all_reported() {
        // Without item_pattern, multiple non-string items should all be reported.
        let constraint = PropertyConstraint::StringList { item_pattern: None };
        let violations = vc_all(
            "tags",
            &Value::Array(vec![
                Value::String("ok".into()),
                Value::Number(1.into()),
                Value::Bool(true),
            ]),
            &constraint,
        );
        assert_eq!(
            violations.len(),
            2,
            "expected 2 violations for the two non-string items, got: {:?}",
            violations.iter().map(|v| &v.message).collect::<Vec<_>>()
        );
        assert!(violations[0].message.contains("item 1"));
        assert!(violations[1].message.contains("item 2"));
    }

    // ---------------------------------------------------------------------------
    // required_sections tests
    // ---------------------------------------------------------------------------

    fn make_schema_with_sections(sections: Vec<String>) -> SchemaConfig {
        let type_schema = TypeSchema {
            required_sections: sections,
            ..Default::default()
        };
        let mut types = HashMap::new();
        types.insert("doc".to_owned(), type_schema);
        SchemaConfig {
            default: TypeSchema::default(),
            types,
        }
    }

    #[test]
    fn required_sections_pass_when_all_present() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        std::fs::write(
            &path,
            "---\ntype: doc\ntitle: Hello\n---\n# Goal\n\nSome text.\n\n## Tasks\n\nDo stuff.\n",
        )
        .unwrap();

        let schema = make_schema_with_sections(vec!["# Goal".to_owned(), "## Tasks".to_owned()]);
        let result = lint_file(&path, "doc.md", &schema).unwrap();
        let section_viols: Vec<_> = result
            .violations
            .iter()
            .filter(|v| v.message.contains("missing required section"))
            .collect();
        assert!(
            section_viols.is_empty(),
            "expected no section violations, got: {section_viols:?}"
        );
    }

    #[test]
    fn required_sections_violation_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        std::fs::write(
            &path,
            "---\ntype: doc\ntitle: Hello\n---\n# Goal\n\nSome text.\n",
        )
        .unwrap();

        let schema = make_schema_with_sections(vec!["# Goal".to_owned(), "## Tasks".to_owned()]);
        let result = lint_file(&path, "doc.md", &schema).unwrap();
        let section_viols: Vec<_> = result
            .violations
            .iter()
            .filter(|v| v.message.contains("missing required section"))
            .collect();
        assert_eq!(
            section_viols.len(),
            1,
            "expected exactly one missing-section violation"
        );
        assert!(
            section_viols[0].message.contains("## Tasks"),
            "expected '## Tasks' in message, got: {}",
            section_viols[0].message
        );
    }

    #[test]
    fn required_sections_order_significant() {
        // Body has both headings but in reverse order.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        std::fs::write(
            &path,
            "---\ntype: doc\ntitle: Hello\n---\n## Tasks\n\nDo stuff.\n\n# Goal\n\nSome text.\n",
        )
        .unwrap();

        // Required: Goal then Tasks (but in body: Tasks then Goal).
        let schema = make_schema_with_sections(vec!["# Goal".to_owned(), "## Tasks".to_owned()]);
        let result = lint_file(&path, "doc.md", &schema).unwrap();
        let section_viols: Vec<_> = result
            .violations
            .iter()
            .filter(|v| v.message.contains("missing required section"))
            .collect();
        // "# Goal" is never matched because its cursor position (after ## Tasks) is after where it appears.
        assert!(
            !section_viols.is_empty(),
            "expected section violation when order is wrong"
        );
    }

    #[test]
    fn required_sections_extras_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        std::fs::write(
            &path,
            "---\ntype: doc\ntitle: Hello\n---\n# Goal\n\n## Extra One\n\nText.\n\n## Tasks\n\nDo stuff.\n\n## Extra Two\n\nMore.\n",
        )
        .unwrap();

        let schema = make_schema_with_sections(vec!["# Goal".to_owned(), "## Tasks".to_owned()]);
        let result = lint_file(&path, "doc.md", &schema).unwrap();
        let section_viols: Vec<_> = result
            .violations
            .iter()
            .filter(|v| v.message.contains("missing required section"))
            .collect();
        assert!(
            section_viols.is_empty(),
            "extra headings should not cause violations, got: {section_viols:?}"
        );
    }

    // --- apply_body_fixes ---

    fn mk_diag(
        rule_id: &str,
        severity: hyalo_mdlint::DiagSeverity,
        start: usize,
        end: usize,
        replacement: &str,
    ) -> hyalo_mdlint::Diagnostic {
        hyalo_mdlint::Diagnostic {
            rule_id: rule_id.to_owned(),
            rule_name: rule_id.to_owned(),
            message: String::new(),
            line: 1,
            column: 1,
            severity,
            fix: Some(hyalo_mdlint::DiagFix {
                description: String::new(),
                start,
                end,
                replacement: replacement.to_owned(),
            }),
        }
    }

    #[test]
    fn apply_body_fixes_error_wins_overlap_regardless_of_offset() {
        // Two fixes over the same range: a Warn fix that would sort first by
        // descending-offset (its start is >= the Error fix's start) must not
        // beat the overlapping Error fix.
        let warn = mk_diag("MD009", hyalo_mdlint::DiagSeverity::Warn, 0, 10, "warn-fix");
        let error = mk_diag(
            "HYALO001",
            hyalo_mdlint::DiagSeverity::Error,
            0,
            10,
            "error-fix",
        );
        let (result, outcomes) = apply_body_fixes("0123456789", &[&warn, &error]);
        assert_eq!(result, "error-fix");
        assert!(matches!(outcomes[1], FixOutcome::Applied));
        assert!(matches!(outcomes[0], FixOutcome::Conflict { .. }));
    }

    #[test]
    fn apply_body_fixes_no_op_replacement_is_not_applied() {
        let noop = mk_diag("MD047", hyalo_mdlint::DiagSeverity::Warn, 4, 5, "\n");
        let (result, outcomes) = apply_body_fixes("body\n", &[&noop]);
        assert_eq!(result, "body\n", "content must not change");
        assert!(
            matches!(outcomes[0], FixOutcome::NoFix),
            "byte-for-byte no-op must not be reported as Applied: {:?}",
            outcomes[0]
        );
    }

    #[test]
    fn apply_body_fixes_non_overlapping_fixes_both_apply() {
        let a = mk_diag("MD009", hyalo_mdlint::DiagSeverity::Warn, 0, 1, "A");
        let b = mk_diag("MD009", hyalo_mdlint::DiagSeverity::Warn, 2, 3, "B");
        let (result, outcomes) = apply_body_fixes("xyz", &[&a, &b]);
        assert_eq!(result, "AyB");
        assert!(matches!(outcomes[0], FixOutcome::Applied));
        assert!(matches!(outcomes[1], FixOutcome::Applied));
    }

    #[test]
    fn apply_body_fixes_adjacent_touching_ranges_both_apply() {
        // end == start is NOT an overlap (strict inequalities in the
        // conflict check): [0,2) and [2,4) must both apply.
        let a = mk_diag("MD009", hyalo_mdlint::DiagSeverity::Warn, 0, 2, "AA");
        let b = mk_diag("MD009", hyalo_mdlint::DiagSeverity::Warn, 2, 4, "BB");
        let (result, outcomes) = apply_body_fixes("wxyz", &[&a, &b]);
        assert_eq!(result, "AABB");
        assert!(matches!(outcomes[0], FixOutcome::Applied));
        assert!(matches!(outcomes[1], FixOutcome::Applied));
    }

    // --- find_body_start line-ending handling ---

    #[test]
    fn find_body_start_skips_crlf_after_closing_delimiter() {
        let content = "---\r\ntitle: T\r\n---\r\nbody line\r\n";
        let start = find_body_start(content);
        assert_eq!(
            &content[start..],
            "body line\r\n",
            "body must not start with a stray CR on CRLF files"
        );
    }

    #[test]
    fn find_body_start_bom_prefixed_frontmatter_is_split() {
        let content = "\u{feff}---\ntitle: T\n---\nbody line\n";
        let start = find_body_start(content);
        assert_eq!(&content[start..], "body line\n");
    }

    // --- group_severity ---

    fn iv(severity: &str) -> InternalViolation {
        InternalViolation {
            line: 1,
            column: 1,
            message: String::new(),
            severity: severity.to_owned(),
            fix: None,
            fixed: false,
        }
    }

    #[test]
    fn group_severity_is_error_when_any_violation_is_error() {
        let violations = vec![iv("warn"), iv("error"), iv("warn")];
        assert_eq!(group_severity(&violations), "error");
    }

    #[test]
    fn group_severity_is_warn_when_all_warn() {
        let violations = vec![iv("warn"), iv("warn")];
        assert_eq!(group_severity(&violations), "warn");
    }

    #[test]
    fn group_severity_empty_defaults_to_warn() {
        assert_eq!(group_severity(&[]), "warn");
    }
}
