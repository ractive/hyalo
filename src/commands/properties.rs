#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use serde_yaml_ng::Value;
use std::path::Path;

use crate::commands::{FilesOrOutcome, collect_files};
use crate::frontmatter;
use crate::output::{CommandOutcome, Format, format_output};
use crate::types::PropertySummaryEntry;

// ---------------------------------------------------------------------------
// Generic list-property helpers
// ---------------------------------------------------------------------------

/// Extract a list from any YAML value under `property_name`.
///
/// - Sequence → collect string and number items as strings
/// - String → single-element vec (empty string → empty vec)
/// - Null / missing → empty vec
/// - Any other type → empty vec
fn extract_list_property(
    props: &std::collections::BTreeMap<String, Value>,
    name: &str,
) -> Vec<String> {
    match props.get(name) {
        Some(Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .collect(),
        Some(Value::String(s)) => {
            if s.is_empty() {
                vec![]
            } else {
                vec![s.clone()]
            }
        }
        _ => vec![],
    }
}

/// Result type for list-mutation operations.
#[derive(Debug)]
pub(crate) struct ListOpResult {
    pub(crate) modified: Vec<String>,
    pub(crate) skipped: Vec<String>,
}

/// Core logic: add `values` to the list property `property_name` across `files`.
///
/// For each file:
/// - Checks that the property is absent, null, a string, or a sequence — returns an error for
///   other types (bool, number, mapping) to prevent silent data loss.
/// - Appends any value not already present (case-insensitive for strings).
/// - Writes back if at least one value was added; otherwise marks file as skipped.
pub(crate) fn add_values_to_list_property(
    files: &[(std::path::PathBuf, String)],
    property_name: &str,
    values: &[String],
) -> Result<ListOpResult> {
    let mut modified = Vec::new();
    let mut skipped = Vec::new();

    for (full_path, rel_path) in files {
        let mut props = frontmatter::read_frontmatter(full_path)?;

        // Guard against silently overwriting non-list scalar types.
        match props.get(property_name) {
            None | Some(Value::Null | Value::String(_) | Value::Sequence(_)) => {}
            Some(existing) => {
                let kind = match existing {
                    Value::Bool(_) => "boolean",
                    Value::Number(_) => "number",
                    Value::Mapping(_) => "mapping",
                    _ => "unknown",
                };
                anyhow::bail!(
                    "property '{property_name}' in '{rel_path}' is a {kind} value, not a list — \
                     use `property set` to overwrite it explicitly"
                );
            }
        }

        // If the property is already a sequence, append directly to preserve element types.
        // Otherwise fall back to the string-based approach for null / absent / scalar-string.
        let added_any = if let Some(Value::Sequence(seq)) = props.get_mut(property_name) {
            let before_len = seq.len();
            for value in values {
                // Duplicate detection: compare strings case-insensitively; for non-string scalars
                // (e.g. YAML number 42) stringify them so that adding "42" is detected as a
                // duplicate of the existing numeric element.
                let already_present = seq.iter().any(|v| match v {
                    Value::String(s) => s.eq_ignore_ascii_case(value),
                    Value::Number(n) => n.to_string().eq_ignore_ascii_case(value),
                    Value::Bool(b) => b.to_string().eq_ignore_ascii_case(value),
                    _ => false,
                });
                if !already_present {
                    seq.push(Value::String(value.clone()));
                }
            }
            seq.len() > before_len
        } else {
            // Build a fresh list from the existing string value (if any) plus new values.
            let mut current = extract_list_property(&props, property_name);
            let before_len = current.len();
            for value in values {
                let already_present = current.iter().any(|v| v.eq_ignore_ascii_case(value));
                if !already_present {
                    current.push(value.clone());
                }
            }
            if current.len() > before_len {
                let yaml_list = Value::Sequence(current.into_iter().map(Value::String).collect());
                props.insert(property_name.to_owned(), yaml_list);
                true
            } else {
                false
            }
        };

        if added_any {
            frontmatter::write_frontmatter(full_path, &props)?;
            modified.push(rel_path.clone());
        } else {
            skipped.push(rel_path.clone());
        }
    }

    Ok(ListOpResult { modified, skipped })
}

/// Core logic: remove `values` from the list property `property_name` across `files`.
///
/// For each file:
/// - Reads current list.
/// - Removes matching values (case-insensitive).
/// - If list becomes empty, removes the entire key.
/// - Writes back if at least one value was removed; otherwise marks file as skipped.
pub(crate) fn remove_values_from_list_property(
    files: &[(std::path::PathBuf, String)],
    property_name: &str,
    values: &[String],
) -> Result<ListOpResult> {
    let mut modified = Vec::new();
    let mut skipped = Vec::new();

    for (full_path, rel_path) in files {
        let mut props = frontmatter::read_frontmatter(full_path)?;

        // Helper: check if a YAML value matches any of the removal targets (case-insensitive).
        let should_remove = |v: &Value| -> bool {
            match v {
                Value::String(s) => values.iter().any(|rm| rm.eq_ignore_ascii_case(s)),
                Value::Number(n) => values
                    .iter()
                    .any(|rm| rm.eq_ignore_ascii_case(&n.to_string())),
                Value::Bool(b) => values
                    .iter()
                    .any(|rm| rm.eq_ignore_ascii_case(&b.to_string())),
                _ => false,
            }
        };

        // If the property is a sequence, filter it in-place, retaining original Value types.
        // For null / absent / scalar-string fall back to the string-based path.
        let (before_len, after_len) =
            if let Some(Value::Sequence(seq)) = props.get_mut(property_name) {
                let before = seq.len();
                seq.retain(|v| !should_remove(v));
                (before, seq.len())
            } else {
                let current = extract_list_property(&props, property_name);
                let before = current.len();
                let new_list: Vec<String> = current
                    .into_iter()
                    .filter(|v| !values.iter().any(|rm| rm.eq_ignore_ascii_case(v)))
                    .collect();
                let after = new_list.len();
                // Write back only if something changed (handled below).
                if after < before {
                    if new_list.is_empty() {
                        // Will be removed below.
                    } else {
                        let yaml_list =
                            Value::Sequence(new_list.into_iter().map(Value::String).collect());
                        props.insert(property_name.to_owned(), yaml_list);
                    }
                }
                (before, after)
            };

        if after_len == before_len {
            // Nothing was removed.
            skipped.push(rel_path.clone());
        } else {
            // If the sequence is now empty (either branch), remove the key entirely.
            if after_len == 0 {
                props.remove(property_name);
            }
            frontmatter::write_frontmatter(full_path, &props)?;
            modified.push(rel_path.clone());
        }
    }

    Ok(ListOpResult { modified, skipped })
}

/// Aggregate summary: unique property names with types and file counts.
/// Scope is filtered by `--file` / `--glob` (or all files if both are None).
pub fn properties_summary(
    dir: &Path,
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    // Aggregate: name -> (type, count)
    let mut agg: std::collections::BTreeMap<String, (String, usize)> =
        std::collections::BTreeMap::new();

    for (fp, _) in &files {
        let props = frontmatter::read_frontmatter(fp)?;
        for (key, value) in &props {
            agg.entry(key.clone())
                .and_modify(|entry| entry.1 += 1)
                .or_insert_with(|| (frontmatter::infer_type(value).to_owned(), 1));
        }
    }

    let result: Vec<PropertySummaryEntry> = agg
        .into_iter()
        .map(|(name, (prop_type, count))| PropertySummaryEntry {
            name,
            prop_type,
            count,
        })
        .collect();

    Ok(CommandOutcome::Success(format_output(format, &result)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
    }

    fn setup_dir() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: Test
status: draft
priority: 3
tags:
  - rust
  - cli
---
# Hello
"),
        )
        .unwrap();
        fs::write(tmp.path().join("empty.md"), "No frontmatter here.\n").unwrap();
        tmp
    }

    /// Extract the output string from a `CommandOutcome`.
    fn unwrap_output(outcome: CommandOutcome) -> (String, bool) {
        match outcome {
            CommandOutcome::Success(s) => (s, true),
            CommandOutcome::UserError(s) => (s, false),
        }
    }

    #[test]
    fn properties_summary_aggregates() {
        let tmp = setup_dir();
        let (out, ok) =
            unwrap_output(properties_summary(tmp.path(), None, None, Format::Json).unwrap());
        assert!(ok);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        assert!(!parsed.is_empty());
        let names: Vec<&str> = parsed.iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"title"));
        assert!(names.contains(&"status"));
    }
}
