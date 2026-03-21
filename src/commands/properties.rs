#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use serde_json::json;
use serde_yaml_ng::Value;
use std::path::Path;

use crate::commands::{
    FilesOrOutcome, build_find_json, build_list_mutation_json, collect_files, require_file_or_glob,
    resolve_error_to_outcome,
};
use crate::discovery;
use crate::frontmatter;
use crate::output::{CommandOutcome, Format};

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
                let already_present = seq.iter().any(|v| match v {
                    Value::String(s) => s.eq_ignore_ascii_case(value),
                    _ => v.as_str().is_some_and(|s| s.eq_ignore_ascii_case(value)),
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
        let current = extract_list_property(&props, property_name);
        let new_list: Vec<String> = current
            .iter()
            .filter(|v| !values.iter().any(|rm| rm.eq_ignore_ascii_case(v)))
            .cloned()
            .collect();

        if new_list.len() == current.len() {
            // Nothing was removed
            skipped.push(rel_path.clone());
        } else {
            if new_list.is_empty() {
                props.remove(property_name);
            } else {
                let yaml_list = Value::Sequence(new_list.into_iter().map(Value::String).collect());
                props.insert(property_name.to_owned(), yaml_list);
            }
            frontmatter::write_frontmatter(full_path, &props)?;
            modified.push(rel_path.clone());
        }
    }

    Ok(ListOpResult { modified, skipped })
}

// ---------------------------------------------------------------------------
// Public CLI-facing list-property commands
// ---------------------------------------------------------------------------

/// Add values to a list property in file(s). Creates the list if absent.
/// Skips duplicates (case-insensitive). Returns a `CommandOutcome` with JSON.
///
/// JSON output: `{"property": name, "values": [...], "modified": [...], "skipped": [...], "total": N}`
pub fn property_add_to_list(
    dir: &Path,
    name: &str,
    values: &[String],
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    if let Some(outcome) = require_file_or_glob(file, glob, "property add-to-list", format) {
        return Ok(outcome);
    }

    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let ListOpResult { modified, skipped } = add_values_to_list_property(&files, name, values)?;

    let result = build_list_mutation_json(
        "property",
        name,
        Some("values"),
        Some(values),
        &modified,
        &skipped,
    );

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
    )))
}

/// Remove values from a list property in file(s). Removes the key if list becomes empty.
///
/// JSON output: `{"property": name, "values": [...], "modified": [...], "skipped": [...], "total": N}`
pub fn property_remove_from_list(
    dir: &Path,
    name: &str,
    values: &[String],
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    if let Some(outcome) = require_file_or_glob(file, glob, "property remove-from-list", format) {
        return Ok(outcome);
    }

    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let ListOpResult { modified, skipped } =
        remove_values_from_list_property(&files, name, values)?;

    let result = build_list_mutation_json(
        "property",
        name,
        Some("values"),
        Some(values),
        &modified,
        &skipped,
    );

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
    )))
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

    let result: Vec<serde_json::Value> = agg
        .into_iter()
        .map(|(name, (typ, count))| json!({"name": name, "type": typ, "count": count}))
        .collect();

    Ok(CommandOutcome::Success(crate::output::format_success(
        format,
        &json!(result),
    )))
}

/// Per-file detail: each file with its full property key/value pairs.
/// Scope is filtered by `--file` / `--glob` (or all files if both are None).
pub fn properties_list(
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

    let mut results = Vec::new();
    for (full_path, rel_path) in &files {
        let props = frontmatter::read_frontmatter(full_path)?;

        let prop_map: serde_json::Map<String, serde_json::Value> = props
            .iter()
            .map(|(k, v)| {
                let typ = frontmatter::infer_type(v);
                let json_val = frontmatter::yaml_to_json(v);
                (k.clone(), json!({"value": json_val, "type": typ}))
            })
            .collect();

        results.push(json!({
            "path": rel_path,
            "properties": prop_map,
        }));
    }

    Ok(CommandOutcome::Success(crate::output::format_success(
        format,
        &json!(results),
    )))
}

/// Read a single property from a file.
pub fn property_read(
    dir: &Path,
    name: &str,
    path_arg: &str,
    format: Format,
) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match resolve_or_error(dir, path_arg, format) {
        Ok(r) => r,
        Err(outcome) => return Ok(outcome),
    };

    let props = frontmatter::read_frontmatter(&full_path)?;

    if let Some(value) = props.get(name) {
        let typ = frontmatter::infer_type(value);
        let json_val = frontmatter::yaml_to_json(value);
        let result = json!({"name": name, "value": json_val, "type": typ});
        Ok(CommandOutcome::Success(crate::output::format_success(
            format, &result,
        )))
    } else {
        let out =
            crate::output::format_error(format, "property not found", Some(&rel_path), None, None);
        Ok(CommandOutcome::UserError(out))
    }
}

/// Set a property on a file.
pub fn property_set(
    dir: &Path,
    name: &str,
    raw_value: &str,
    forced_type: Option<&str>,
    path_arg: &str,
    format: Format,
) -> Result<CommandOutcome> {
    let (full_path, _rel_path) = match resolve_or_error(dir, path_arg, format) {
        Ok(r) => r,
        Err(outcome) => return Ok(outcome),
    };

    let value = frontmatter::parse_value(raw_value, forced_type)?;
    let typ = frontmatter::infer_type(&value);
    let json_val = frontmatter::yaml_to_json(&value);

    let mut props = frontmatter::read_frontmatter(&full_path)?;
    props.insert(name.to_owned(), value);
    frontmatter::write_frontmatter(&full_path, &props)?;

    let result = json!({"name": name, "value": json_val, "type": typ});

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
    )))
}

/// Remove a property from a file.
pub fn property_remove(
    dir: &Path,
    name: &str,
    path_arg: &str,
    format: Format,
) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match resolve_or_error(dir, path_arg, format) {
        Ok(r) => r,
        Err(outcome) => return Ok(outcome),
    };

    let mut props = frontmatter::read_frontmatter(&full_path)?;

    if props.remove(name).is_some() {
        frontmatter::write_frontmatter(&full_path, &props)?;
        let result = json!({"removed": name, "path": rel_path});
        Ok(CommandOutcome::Success(crate::output::format_success(
            format, &result,
        )))
    } else {
        let out =
            crate::output::format_error(format, "property not found", Some(&rel_path), None, None);
        Ok(CommandOutcome::UserError(out))
    }
}

/// Find files that contain a specific frontmatter property, optionally filtering by value.
pub fn property_find(
    dir: &Path,
    name: &str,
    value: Option<&str>,
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let mut matching_paths: Vec<String> = Vec::new();

    for (full_path, rel_path) in &files {
        let props = frontmatter::read_frontmatter(full_path)?;
        if let Some(yaml_val) = props.get(name) {
            let matches = match value {
                None => true,
                Some(query) => value_matches(yaml_val, query),
            };
            if matches {
                matching_paths.push(rel_path.clone());
            }
        }
    }

    let result = build_find_json("property", name, value, &matching_paths);

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
    )))
}

/// Check if a YAML value matches a query string, using type-aware comparison.
fn value_matches(yaml_val: &serde_yaml_ng::Value, query: &str) -> bool {
    use serde_yaml_ng::Value;
    match yaml_val {
        Value::String(s) => s.eq_ignore_ascii_case(query),
        Value::Number(n) => {
            // Try integer match first, then float
            if let Some(i) = n.as_i64() {
                query.parse::<i64>() == Ok(i)
            } else if let Some(f) = n.as_f64() {
                // Exact float equality — acceptable for simple YAML frontmatter values
                query.parse::<f64>() == Ok(f)
            } else {
                false
            }
        }
        Value::Bool(b) => {
            // Accept the same spellings the CLI uses for boolean flags, case-insensitively.
            let q = query.to_ascii_lowercase();
            let is_truthy = matches!(q.as_str(), "true" | "yes" | "1");
            let is_falsy = matches!(q.as_str(), "false" | "no" | "0");
            if is_truthy {
                *b
            } else if is_falsy {
                !*b
            } else {
                false
            }
        }
        Value::Sequence(seq) => seq.iter().any(|item| value_matches(item, query)),
        Value::Null | Value::Mapping(_) | Value::Tagged(_) => false,
    }
}

/// Helper to resolve a file path or produce a user error outcome.
fn resolve_or_error(
    dir: &Path,
    path_arg: &str,
    format: Format,
) -> Result<(std::path::PathBuf, String), CommandOutcome> {
    match discovery::resolve_file(dir, path_arg) {
        Ok(r) => Ok(r),
        Err(e) => Err(resolve_error_to_outcome(e, format)),
    }
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

    #[test]
    fn properties_list_single_file() {
        let tmp = setup_dir();
        let (out, ok) = unwrap_output(
            properties_list(tmp.path(), Some("note.md"), None, Format::Json).unwrap(),
        );
        assert!(ok);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["path"], "note.md");
        assert_eq!(parsed[0]["properties"]["priority"]["type"], "number");
        assert_eq!(parsed[0]["properties"]["tags"]["type"], "list");
    }

    #[test]
    fn property_read_existing() {
        let tmp = setup_dir();
        let (out, ok) =
            unwrap_output(property_read(tmp.path(), "status", "note.md", Format::Json).unwrap());
        assert!(ok);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["value"], "draft");
        assert_eq!(parsed["type"], "text");
    }

    #[test]
    fn property_read_missing() {
        let tmp = setup_dir();
        let (out, ok) = unwrap_output(
            property_read(tmp.path(), "nonexistent", "note.md", Format::Json).unwrap(),
        );
        assert!(!ok);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["error"], "property not found");
    }

    #[test]
    fn property_set_new() {
        let tmp = setup_dir();
        let (out, ok) = unwrap_output(
            property_set(tmp.path(), "author", "Alice", None, "note.md", Format::Json).unwrap(),
        );
        assert!(ok);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["value"], "Alice");

        // Verify it persisted
        let (out2, _) =
            unwrap_output(property_read(tmp.path(), "author", "note.md", Format::Json).unwrap());
        let p2: serde_json::Value = serde_json::from_str(&out2).unwrap();
        assert_eq!(p2["value"], "Alice");
    }

    #[test]
    fn property_set_with_type() {
        let tmp = setup_dir();
        let (out, ok) = unwrap_output(
            property_set(
                tmp.path(),
                "count",
                "42",
                Some("text"),
                "note.md",
                Format::Json,
            )
            .unwrap(),
        );
        assert!(ok);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["type"], "text");
        assert_eq!(parsed["value"], "42");
    }

    #[test]
    fn property_remove_existing() {
        let tmp = setup_dir();
        let (out, ok) =
            unwrap_output(property_remove(tmp.path(), "status", "note.md", Format::Json).unwrap());
        assert!(ok);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["removed"], "status");

        // Verify it's gone
        let (_, ok2) =
            unwrap_output(property_read(tmp.path(), "status", "note.md", Format::Json).unwrap());
        assert!(!ok2);
    }

    #[test]
    fn property_remove_missing() {
        let tmp = setup_dir();
        let (_, ok) = unwrap_output(
            property_remove(tmp.path(), "nonexistent", "note.md", Format::Json).unwrap(),
        );
        assert!(!ok);
    }

    #[test]
    fn file_not_found_error() {
        let tmp = setup_dir();
        let (out, ok) =
            unwrap_output(property_read(tmp.path(), "x", "nope.md", Format::Json).unwrap());
        assert!(!ok);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["error"], "file not found");
    }

    #[test]
    fn missing_extension_hint() {
        let tmp = setup_dir();
        let (out, ok) =
            unwrap_output(property_read(tmp.path(), "x", "note", Format::Json).unwrap());
        assert!(!ok);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["error"], "file not found");
        assert!(parsed["hint"].as_str().unwrap().contains("note.md"));
    }

    #[test]
    fn property_set_creates_frontmatter() {
        let tmp = setup_dir();
        let (_, ok) = unwrap_output(
            property_set(tmp.path(), "status", "new", None, "empty.md", Format::Json).unwrap(),
        );
        assert!(ok);

        let content = fs::read_to_string(tmp.path().join("empty.md")).unwrap();
        assert!(content.starts_with("---\n"));
    }

    #[test]
    fn property_set_preserves_body() {
        let tmp = tempfile::tempdir().unwrap();
        let body = md!(r"
# Heading

Body content.
");
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: Test
---
")
            .to_owned()
                + body,
        )
        .unwrap();

        property_set(tmp.path(), "status", "done", None, "note.md", Format::Json).unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains(body), "body was corrupted:\n{content}");
    }

    #[test]
    fn property_remove_preserves_body() {
        let tmp = tempfile::tempdir().unwrap();
        let body = md!(r"
# Heading

Body content.
");
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: Test
status: draft
---
")
            .to_owned()
                + body,
        )
        .unwrap();

        property_remove(tmp.path(), "status", "note.md", Format::Json).unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains(body), "body was corrupted:\n{content}");
    }

    // ---------------------------------------------------------------------------
    // property_find tests
    // ---------------------------------------------------------------------------

    fn setup_find_vault() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        // a.md: status=draft (string), priority=3 (number), draft=true (bool), tags=[rust,cli]
        fs::write(
            tmp.path().join("a.md"),
            md!(r"
---
status: draft
priority: 3
draft: true
tags:
  - rust
  - cli
---
"),
        )
        .unwrap();
        // b.md: status=done (string), priority=5 (number), draft=false (bool)
        fs::write(
            tmp.path().join("b.md"),
            md!(r"
---
status: done
priority: 5
draft: false
---
"),
        )
        .unwrap();
        // c.md: no frontmatter
        fs::write(tmp.path().join("c.md"), "No frontmatter.\n").unwrap();
        tmp
    }

    fn unwrap_success(outcome: CommandOutcome) -> serde_json::Value {
        match outcome {
            CommandOutcome::Success(s) => serde_json::from_str(&s).unwrap(),
            CommandOutcome::UserError(s) => panic!("unexpected user error: {s}"),
        }
    }

    fn unwrap_user_error(outcome: CommandOutcome) -> serde_json::Value {
        match outcome {
            CommandOutcome::UserError(s) => serde_json::from_str(&s).unwrap(),
            CommandOutcome::Success(s) => panic!("expected user error, got success: {s}"),
        }
    }

    #[test]
    fn property_find_by_existence() {
        let tmp = setup_find_vault();
        let parsed = unwrap_success(
            property_find(tmp.path(), "status", None, None, None, Format::Json).unwrap(),
        );
        assert_eq!(parsed["total"], 2);
        let files: Vec<&str> = parsed["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(files.iter().any(|f| f.contains("a.md")));
        assert!(files.iter().any(|f| f.contains("b.md")));
        assert!(!files.iter().any(|f| f.contains("c.md")));
    }

    #[test]
    fn property_find_by_string_value() {
        let tmp = setup_find_vault();
        let parsed = unwrap_success(
            property_find(
                tmp.path(),
                "status",
                Some("draft"),
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(parsed["total"], 1);
        assert!(parsed["files"][0].as_str().unwrap().contains("a.md"));
    }

    #[test]
    fn property_find_by_number_value() {
        let tmp = setup_find_vault();
        let parsed = unwrap_success(
            property_find(tmp.path(), "priority", Some("3"), None, None, Format::Json).unwrap(),
        );
        assert_eq!(parsed["total"], 1);
        assert!(parsed["files"][0].as_str().unwrap().contains("a.md"));
    }

    #[test]
    fn property_find_by_bool_value() {
        let tmp = setup_find_vault();
        let parsed = unwrap_success(
            property_find(tmp.path(), "draft", Some("true"), None, None, Format::Json).unwrap(),
        );
        assert_eq!(parsed["total"], 1);
        assert!(parsed["files"][0].as_str().unwrap().contains("a.md"));
    }

    #[test]
    fn property_find_in_list() {
        let tmp = setup_find_vault();
        let parsed = unwrap_success(
            property_find(tmp.path(), "tags", Some("rust"), None, None, Format::Json).unwrap(),
        );
        assert_eq!(parsed["total"], 1);
        assert!(parsed["files"][0].as_str().unwrap().contains("a.md"));
    }

    #[test]
    fn property_find_case_insensitive() {
        let tmp = setup_find_vault();
        // "Draft" should match "draft" case-insensitively
        let parsed = unwrap_success(
            property_find(
                tmp.path(),
                "status",
                Some("Draft"),
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(parsed["total"], 1);
        assert!(parsed["files"][0].as_str().unwrap().contains("a.md"));
    }

    #[test]
    fn property_find_with_glob() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(
            tmp.path().join("sub/x.md"),
            md!(r"
---
status: draft
---
"),
        )
        .unwrap();
        fs::write(
            tmp.path().join("root.md"),
            md!(r"
---
status: draft
---
"),
        )
        .unwrap();

        let parsed = unwrap_success(
            property_find(
                tmp.path(),
                "status",
                None,
                None,
                Some("sub/*.md"),
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(parsed["total"], 1);
        assert!(parsed["files"][0].as_str().unwrap().contains("sub/x.md"));
    }

    #[test]
    fn property_find_with_file() {
        let tmp = setup_find_vault();
        let parsed = unwrap_success(
            property_find(tmp.path(), "status", None, Some("a.md"), None, Format::Json).unwrap(),
        );
        assert_eq!(parsed["total"], 1);
    }

    #[test]
    fn property_find_no_match() {
        let tmp = setup_find_vault();
        let parsed = unwrap_success(
            property_find(
                tmp.path(),
                "status",
                Some("nonexistent-value"),
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(parsed["total"], 0);
        assert!(parsed["files"].as_array().unwrap().is_empty());
    }

    #[test]
    fn property_find_value_no_match() {
        let tmp = setup_find_vault();
        // Property exists but value doesn't match any file
        let parsed = unwrap_success(
            property_find(tmp.path(), "priority", Some("99"), None, None, Format::Json).unwrap(),
        );
        assert_eq!(parsed["total"], 0);
    }

    #[test]
    fn property_find_nonexistent_property() {
        let tmp = setup_find_vault();
        let parsed = unwrap_success(
            property_find(tmp.path(), "does_not_exist", None, None, None, Format::Json).unwrap(),
        );
        assert_eq!(parsed["total"], 0);
    }

    #[test]
    fn property_find_file_not_found() {
        let tmp = setup_find_vault();
        let parsed = unwrap_user_error(
            property_find(
                tmp.path(),
                "status",
                None,
                Some("nope.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(parsed["error"], "file not found");
    }

    #[test]
    fn property_find_no_frontmatter_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        // Only a file with no frontmatter
        fs::write(tmp.path().join("plain.md"), "Just text.\n").unwrap();
        let parsed = unwrap_success(
            property_find(tmp.path(), "status", None, None, None, Format::Json).unwrap(),
        );
        assert_eq!(parsed["total"], 0);
    }

    #[test]
    fn property_find_empty_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let parsed = unwrap_success(
            property_find(tmp.path(), "status", None, None, None, Format::Json).unwrap(),
        );
        assert_eq!(parsed["total"], 0);
        assert!(parsed["files"].as_array().unwrap().is_empty());
    }

    // Issue 3 — extended boolean matching for value_matches
    #[test]
    fn value_matches_bool_yes_no() {
        let t = Value::Bool(true);
        let f = Value::Bool(false);
        assert!(value_matches(&t, "yes"));
        assert!(value_matches(&t, "YES"));
        assert!(!value_matches(&t, "no"));
        assert!(value_matches(&f, "no"));
        assert!(value_matches(&f, "NO"));
        assert!(!value_matches(&f, "yes"));
    }

    #[test]
    fn value_matches_bool_zero_one() {
        let t = Value::Bool(true);
        let f = Value::Bool(false);
        assert!(value_matches(&t, "1"));
        assert!(!value_matches(&t, "0"));
        assert!(value_matches(&f, "0"));
        assert!(!value_matches(&f, "1"));
    }

    // ---------------------------------------------------------------------------
    // property_add_to_list / property_remove_from_list unit tests
    // ---------------------------------------------------------------------------

    fn write_note(dir: &std::path::Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    // --- Happy paths: add ---

    #[test]
    fn property_add_to_list_creates_new_list() {
        let tmp = tempfile::tempdir().unwrap();
        write_note(tmp.path(), "note.md", "---\ntitle: T\n---\n");

        let parsed = unwrap_success(
            property_add_to_list(
                tmp.path(),
                "aliases",
                &["foo".to_owned()],
                Some("note.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(parsed["property"], "aliases");
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["skipped"].as_array().unwrap().len(), 0);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("foo"));
    }

    #[test]
    fn property_add_to_list_appends_to_existing() {
        let tmp = tempfile::tempdir().unwrap();
        write_note(tmp.path(), "note.md", "---\naliases:\n  - bar\n---\n");

        unwrap_success(
            property_add_to_list(
                tmp.path(),
                "aliases",
                &["baz".to_owned()],
                Some("note.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("bar"));
        assert!(content.contains("baz"));
    }

    #[test]
    fn property_add_to_list_skips_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        write_note(tmp.path(), "note.md", "---\naliases:\n  - Foo\n---\n");

        let parsed = unwrap_success(
            property_add_to_list(
                tmp.path(),
                "aliases",
                &["foo".to_owned()], // different case
                Some("note.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["skipped"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn property_add_to_list_multiple_values() {
        let tmp = tempfile::tempdir().unwrap();
        write_note(tmp.path(), "note.md", "---\ntitle: T\n---\n");

        let parsed = unwrap_success(
            property_add_to_list(
                tmp.path(),
                "authors",
                &["alice".to_owned(), "bob".to_owned()],
                Some("note.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("alice"));
        assert!(content.contains("bob"));
    }

    #[test]
    fn property_add_to_list_to_scalar_string() {
        let tmp = tempfile::tempdir().unwrap();
        // Property exists as a scalar string
        write_note(tmp.path(), "note.md", "---\naliases: old\n---\n");

        unwrap_success(
            property_add_to_list(
                tmp.path(),
                "aliases",
                &["new".to_owned()],
                Some("note.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("old"));
        assert!(content.contains("new"));
    }

    // --- Happy paths: remove ---

    #[test]
    fn property_remove_from_list_removes_values() {
        let tmp = tempfile::tempdir().unwrap();
        write_note(
            tmp.path(),
            "note.md",
            "---\naliases:\n  - foo\n  - bar\n---\n",
        );

        let parsed = unwrap_success(
            property_remove_from_list(
                tmp.path(),
                "aliases",
                &["foo".to_owned()],
                Some("note.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["skipped"].as_array().unwrap().len(), 0);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(!content.contains("foo"));
        assert!(content.contains("bar"));
    }

    #[test]
    fn property_remove_from_list_empties_key() {
        let tmp = tempfile::tempdir().unwrap();
        write_note(
            tmp.path(),
            "note.md",
            "---\ntitle: T\naliases:\n  - solo\n---\n",
        );

        unwrap_success(
            property_remove_from_list(
                tmp.path(),
                "aliases",
                &["solo".to_owned()],
                Some("note.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(!content.contains("aliases:"));
        assert!(content.contains("title:"));
    }

    #[test]
    fn property_remove_from_list_case_insensitive() {
        let tmp = tempfile::tempdir().unwrap();
        write_note(tmp.path(), "note.md", "---\naliases:\n  - Rust\n---\n");

        let parsed = unwrap_success(
            property_remove_from_list(
                tmp.path(),
                "aliases",
                &["rust".to_owned()], // lowercase
                Some("note.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn property_remove_from_list_multiple_values() {
        let tmp = tempfile::tempdir().unwrap();
        write_note(
            tmp.path(),
            "note.md",
            "---\naliases:\n  - a\n  - b\n  - c\n---\n",
        );

        unwrap_success(
            property_remove_from_list(
                tmp.path(),
                "aliases",
                &["a".to_owned(), "b".to_owned()],
                Some("note.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(!content.contains("- a\n") && !content.contains("- b\n"));
        assert!(content.contains('c'));
    }

    // --- Unhappy paths ---

    #[test]
    fn property_add_to_list_requires_file_or_glob() {
        let tmp = tempfile::tempdir().unwrap();
        write_note(tmp.path(), "note.md", "---\ntitle: T\n---\n");

        let parsed = unwrap_user_error(
            property_add_to_list(
                tmp.path(),
                "aliases",
                &["foo".to_owned()],
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert!(
            parsed["error"].as_str().unwrap().contains("--file")
                || parsed["error"].as_str().unwrap().contains("--glob")
        );
    }

    #[test]
    fn property_remove_from_list_requires_file_or_glob() {
        let tmp = tempfile::tempdir().unwrap();
        write_note(tmp.path(), "note.md", "---\naliases:\n  - foo\n---\n");

        let parsed = unwrap_user_error(
            property_remove_from_list(
                tmp.path(),
                "aliases",
                &["foo".to_owned()],
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert!(
            parsed["error"].as_str().unwrap().contains("--file")
                || parsed["error"].as_str().unwrap().contains("--glob")
        );
    }

    #[test]
    fn property_add_to_list_file_not_found() {
        let tmp = tempfile::tempdir().unwrap();

        let parsed = unwrap_user_error(
            property_add_to_list(
                tmp.path(),
                "aliases",
                &["foo".to_owned()],
                Some("nonexistent.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(parsed["error"], "file not found");
    }

    #[test]
    fn property_remove_from_list_absent_values_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        write_note(tmp.path(), "note.md", "---\naliases:\n  - bar\n---\n");

        let parsed = unwrap_success(
            property_remove_from_list(
                tmp.path(),
                "aliases",
                &["nothere".to_owned()],
                Some("note.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["skipped"].as_array().unwrap().len(), 1);
    }

    // Issue 1 — non-list scalar types must not be silently overwritten
    #[test]
    fn property_add_to_list_rejects_bool_property() {
        let tmp = tempfile::tempdir().unwrap();
        // `draft` is a boolean — add-to-list must return an error
        write_note(tmp.path(), "note.md", "---\ndraft: true\n---\n");

        let files = vec![(tmp.path().join("note.md"), "note.md".to_owned())];
        let result = add_values_to_list_property(&files, "draft", &["foo".to_owned()]);
        assert!(result.is_err(), "expected error for boolean property");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("boolean"),
            "error should mention the type: {msg}"
        );
    }

    #[test]
    fn property_add_to_list_rejects_number_property() {
        let tmp = tempfile::tempdir().unwrap();
        write_note(tmp.path(), "note.md", "---\npriority: 3\n---\n");

        let files = vec![(tmp.path().join("note.md"), "note.md".to_owned())];
        let result = add_values_to_list_property(&files, "priority", &["foo".to_owned()]);
        assert!(result.is_err(), "expected error for number property");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("number"),
            "error should mention the type: {msg}"
        );
    }

    // Issue 2 — existing numeric list elements must be preserved as numbers after append
    #[test]
    fn property_add_to_list_preserves_numeric_elements() {
        let tmp = tempfile::tempdir().unwrap();
        // `scores` contains numbers; appending a string must not stringify the numbers
        write_note(tmp.path(), "note.md", "---\nscores:\n  - 42\n  - 7\n---\n");

        unwrap_success(
            property_add_to_list(
                tmp.path(),
                "scores",
                &["best".to_owned()],
                Some("note.md"),
                None,
                Format::Json,
            )
            .unwrap(),
        );

        // Read back and verify that 42 and 7 are still YAML numbers (not quoted strings)
        let props = frontmatter::read_frontmatter(&tmp.path().join("note.md")).unwrap();
        let seq = match props.get("scores") {
            Some(Value::Sequence(s)) => s,
            other => panic!("expected sequence, got: {other:?}"),
        };
        assert_eq!(seq.len(), 3);
        assert!(
            matches!(&seq[0], Value::Number(n) if n.as_i64() == Some(42)),
            "first element should remain a number 42, got: {:?}",
            seq[0]
        );
        assert!(
            matches!(&seq[1], Value::Number(n) if n.as_i64() == Some(7)),
            "second element should remain a number 7, got: {:?}",
            seq[1]
        );
        assert!(
            matches!(&seq[2], Value::String(s) if s == "best"),
            "third element should be string 'best', got: {:?}",
            seq[2]
        );
    }
}
