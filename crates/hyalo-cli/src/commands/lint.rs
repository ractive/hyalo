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
use std::path::Path;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use hyalo_core::frontmatter::read_frontmatter;
use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};

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

/// Aggregated lint output.
#[derive(Debug, Serialize)]
pub struct LintOutput {
    pub results: Vec<FileLintResult>,
    pub total: usize,
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
    format: Format,
) -> Result<(CommandOutcome, LintCounts)> {
    let mut results: Vec<FileLintResult> = Vec::new();
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
        results.push(file_result);
    }

    let total = files.len();
    let output = LintOutput { results, total };

    let outcome = match format {
        Format::Json => {
            let val = serde_json::to_value(&output).context("failed to serialize lint output")?;
            CommandOutcome::success(format_success(Format::Json, &val))
        }
        Format::Text => {
            let text = format_text_output(&output, &counts);
            CommandOutcome::RawOutput(text)
        }
    };

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
    let properties = match read_frontmatter(full_path) {
        Ok(props) => props,
        Err(e) if hyalo_core::frontmatter::is_parse_error(&e) => {
            // Malformed frontmatter — report as a single error violation.
            return Ok(FileLintResult {
                file: rel_path.to_owned(),
                violations: vec![Violation {
                    severity: Severity::Error,
                    message: format!("could not parse frontmatter: {e}"),
                }],
            });
        }
        Err(e) => return Err(e).context(format!("reading {rel_path}")),
    };

    let has_tags = properties.contains_key("tags");
    let violations = validate_properties(rel_path, &properties, has_tags, schema);
    Ok(FileLintResult {
        file: rel_path.to_owned(),
        violations,
    })
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
    let doc_type: Option<String> = properties.get("type").and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        _ => None,
    });

    // Warn when no `type` property is present.
    if doc_type.is_none() && !schema.is_empty() {
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

    // Type-specific property constraint validation.
    for (name, value) in properties {
        if name == "tags" {
            continue; // tags handled separately
        }
        // Never warn about "type" (type discriminator) or properties listed in `required`
        // — they're implicitly accepted even if not in the `properties` map.
        let implicitly_accepted =
            name == "type" || effective_schema.required.contains(&name.clone());

        if let Some(constraint) = effective_schema.properties.get(name.as_str()) {
            if let Some(v) = validate_constraint(name, value, constraint) {
                violations.push(v);
            }
        } else if !effective_schema.properties.is_empty() && !implicitly_accepted {
            // Property not declared in schema — warn if the schema has any constraints.
            // We only warn when the schema is non-trivial (has some declared properties)
            // to avoid noisy warnings on minimal schemas that only specify `required`.
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
) -> Option<Violation> {
    match constraint {
        PropertyConstraint::String { pattern } => {
            let s = value_as_str(value)?;
            if let Some(pat) = pattern {
                match Regex::new(pat) {
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

/// Returns `true` for YYYY-MM-DD formatted dates.
fn is_iso8601_date(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }
    let b = s.as_bytes();
    b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit)
}

// ---------------------------------------------------------------------------
// Text formatter
// ---------------------------------------------------------------------------

fn format_text_output(output: &LintOutput, counts: &LintCounts) -> String {
    use std::fmt::Write as _;

    let mut s = String::new();
    for file in &output.results {
        if file.violations.is_empty() {
            continue;
        }
        let _ = writeln!(s, "{}:", file.file);
        for v in &file.violations {
            let pad = if v.severity == Severity::Error {
                "error"
            } else {
                "warn "
            };
            let _ = writeln!(s, "  {pad}  {}", v.message);
        }
    }

    // Summary line.
    let files_checked = output.total;
    if counts.errors == 0 && counts.warnings == 0 {
        let _ = write!(s, "{files_checked} files checked, no issues");
    } else {
        let _ = write!(
            s,
            "{files_checked} files checked, {} with issues ({} errors, {} warnings)",
            counts.files_with_issues, counts.errors, counts.warnings,
        );
    }

    s
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
    fn invalid_date_format() {
        assert!(!is_iso8601_date("April 13"));
        assert!(!is_iso8601_date("13-04-2026"));
        assert!(!is_iso8601_date("2026/04/13"));
    }

    // --- validate_constraint ---

    #[test]
    fn date_constraint_valid() {
        let v = validate_constraint(
            "date",
            &Value::String("2026-04-13".into()),
            &PropertyConstraint::Date,
        );
        assert!(v.is_none());
    }

    #[test]
    fn date_constraint_invalid() {
        let v = validate_constraint(
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
        let v = validate_constraint(
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
        let v = validate_constraint(
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
        let v = validate_constraint(
            "priority",
            &Value::Number(5.into()),
            &PropertyConstraint::Number,
        );
        assert!(v.is_none());
    }

    #[test]
    fn number_constraint_invalid() {
        let v = validate_constraint(
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
        let v = validate_constraint("draft", &Value::Bool(true), &PropertyConstraint::Boolean);
        assert!(v.is_none());
    }

    #[test]
    fn boolean_constraint_invalid() {
        let v = validate_constraint(
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
        let v = validate_constraint("tags", &Value::Array(vec![]), &PropertyConstraint::List);
        assert!(v.is_none());
    }

    #[test]
    fn list_constraint_invalid() {
        let v = validate_constraint(
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
        let v = validate_constraint(
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
        let v = validate_constraint(
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
        let (_, counts) = lint_files(&files, &schema, Format::Text).unwrap();
        assert_eq!(counts.errors, 0);
        assert_eq!(counts.warnings, 0);
    }

    #[test]
    fn format_text_output_clean() {
        let output = LintOutput {
            results: vec![],
            total: 3,
        };
        let counts = LintCounts::default();
        let text = format_text_output(&output, &counts);
        assert!(text.contains("3 files checked"));
        assert!(text.contains("no issues"));
    }

    #[test]
    fn format_text_output_with_violations() {
        let output = LintOutput {
            results: vec![FileLintResult {
                file: "note.md".to_owned(),
                violations: vec![
                    Violation {
                        severity: Severity::Error,
                        message: "missing required property \"date\"".to_owned(),
                    },
                    Violation {
                        severity: Severity::Warn,
                        message: "no tags defined".to_owned(),
                    },
                ],
            }],
            total: 1,
        };
        let counts = LintCounts {
            errors: 1,
            warnings: 1,
            files_with_issues: 1,
        };
        let text = format_text_output(&output, &counts);
        assert!(text.contains("note.md:"));
        assert!(text.contains("error"));
        assert!(text.contains("warn"));
        assert!(text.contains("1 files checked, 1 with issues"));
    }
}
