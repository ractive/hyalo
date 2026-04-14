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
    pub total: usize,
    /// Fixes that were applied (or previewed) per file. Omitted when no
    /// `--fix` run produced any changes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<FileFixResult>,
    /// `true` when `--dry-run` was passed and fixes were not written.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dry_run: bool,
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
    lint_files_with_options(files, schema, FixMode::Off)
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
        results.push(file_result);
    }

    let total = files.len();
    let output = LintOutput {
        files: results,
        total,
        fixes: fix_results,
        dry_run: matches!(fix, FixMode::DryRun),
    };

    let val = serde_json::to_value(&output).context("failed to serialize lint output")?;
    let outcome = CommandOutcome::success(format_success(Format::Json, &val));

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
}
