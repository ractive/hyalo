use anyhow::Result;
use serde_json::json;
use std::path::Path;

use crate::commands::{FilesOrOutcome, collect_files, resolve_error_to_outcome};
use crate::discovery;
use crate::frontmatter;
use crate::output::{CommandOutcome, Format};

/// List all properties across all files, or properties of a single file / glob match.
pub fn properties(dir: &Path, path: Option<&str>, format: Format) -> Result<CommandOutcome> {
    match path {
        Some(p) if discovery::is_glob(p) => properties_glob(dir, p, format),
        Some(p) => properties_single(dir, p, format),
        None => properties_all(dir, format),
    }
}

/// List all unique property names across all `.md` files.
fn properties_all(dir: &Path, format: Format) -> Result<CommandOutcome> {
    let files = collect_files(dir, None, None, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    // Aggregate: name -> (type, count)
    let mut agg: std::collections::BTreeMap<String, (String, usize)> =
        std::collections::BTreeMap::new();

    for (file, _) in &files {
        let props = frontmatter::read_frontmatter(file)?;
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

/// List properties of a single file.
fn properties_single(dir: &Path, path_arg: &str, format: Format) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match discovery::resolve_file(dir, path_arg) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format)),
    };

    let props = frontmatter::read_frontmatter(&full_path)?;

    let prop_map: serde_json::Map<String, serde_json::Value> = props
        .iter()
        .map(|(k, v)| {
            let typ = frontmatter::infer_type(v);
            let json_val = frontmatter::yaml_to_json(v);
            (k.clone(), json!({"value": json_val, "type": typ}))
        })
        .collect();

    let result = json!({
        "path": rel_path,
        "properties": prop_map,
    });

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
    )))
}

/// List properties of files matching a glob pattern.
fn properties_glob(dir: &Path, pattern: &str, format: Format) -> Result<CommandOutcome> {
    let files = collect_files(dir, None, Some(pattern), format)?;
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

    let total = matching_paths.len();
    let result = serde_json::json!({
        "property": name,
        "value": value,
        "files": matching_paths,
        "total": total,
    });

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
                query.parse::<f64>() == Ok(f)
            } else {
                false
            }
        }
        Value::Bool(b) => match query {
            "true" => *b,
            "false" => !b,
            _ => false,
        },
        Value::Sequence(seq) => seq.iter().any(|item| value_matches(item, query)),
        Value::Null => false,
        Value::Mapping(_) | Value::Tagged(_) => false,
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
            md!(r#"
---
title: Test
status: draft
priority: 3
tags:
  - rust
  - cli
---
# Hello
"#),
        )
        .unwrap();
        fs::write(tmp.path().join("empty.md"), "No frontmatter here.\n").unwrap();
        tmp
    }

    /// Extract the output string from a CommandOutcome.
    fn unwrap_output(outcome: CommandOutcome) -> (String, bool) {
        match outcome {
            CommandOutcome::Success(s) => (s, true),
            CommandOutcome::UserError(s) => (s, false),
        }
    }

    #[test]
    fn properties_all_aggregates() {
        let tmp = setup_dir();
        let (out, ok) = unwrap_output(properties(tmp.path(), None, Format::Json).unwrap());
        assert!(ok);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        assert!(!parsed.is_empty());
        let names: Vec<&str> = parsed.iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"title"));
        assert!(names.contains(&"status"));
    }

    #[test]
    fn properties_single_file() {
        let tmp = setup_dir();
        let (out, ok) =
            unwrap_output(properties(tmp.path(), Some("note.md"), Format::Json).unwrap());
        assert!(ok);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["path"], "note.md");
        assert_eq!(parsed["properties"]["priority"]["type"], "number");
        assert_eq!(parsed["properties"]["tags"]["type"], "list");
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
        let body = md!(r#"
# Heading

Body content.
"#);
        fs::write(
            tmp.path().join("note.md"),
            md!(r#"
---
title: Test
---
"#)
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
        let body = md!(r#"
# Heading

Body content.
"#);
        fs::write(
            tmp.path().join("note.md"),
            md!(r#"
---
title: Test
status: draft
---
"#)
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
            md!(r#"
---
status: draft
priority: 3
draft: true
tags:
  - rust
  - cli
---
"#),
        )
        .unwrap();
        // b.md: status=done (string), priority=5 (number), draft=false (bool)
        fs::write(
            tmp.path().join("b.md"),
            md!(r#"
---
status: done
priority: 5
draft: false
---
"#),
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
            md!(r#"
---
status: draft
---
"#),
        )
        .unwrap();
        fs::write(
            tmp.path().join("root.md"),
            md!(r#"
---
status: draft
---
"#),
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
}
