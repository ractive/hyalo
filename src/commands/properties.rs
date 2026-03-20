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
}
