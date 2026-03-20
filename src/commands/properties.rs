use anyhow::{Context, Result};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::discovery::{self, FileResolveError};
use crate::frontmatter::{self, Document};
use crate::output::Format;

/// List all properties across all files, or properties of a single file / glob match.
pub fn properties(dir: &Path, path: Option<&str>, format: Format) -> Result<(String, i32)> {
    match path {
        Some(p) if discovery::is_glob(p) => properties_glob(dir, p, format),
        Some(p) => properties_single(dir, p, format),
        None => properties_all(dir, format),
    }
}

/// List all unique property names across all `.md` files.
fn properties_all(dir: &Path, format: Format) -> Result<(String, i32)> {
    let files = discovery::discover_files(dir)?;

    // Aggregate: name -> (type, count)
    let mut agg: BTreeMap<String, (String, usize)> = BTreeMap::new();

    for file in &files {
        let props = frontmatter::read_frontmatter(file)?;
        for (key, value) in &props {
            let typ = frontmatter::infer_type(value).to_owned();
            let entry = agg.entry(key.clone()).or_insert_with(|| (typ.clone(), 0));
            entry.1 += 1;
            // If types differ across files, keep the first seen
        }
    }

    let result: Vec<serde_json::Value> = agg
        .into_iter()
        .map(|(name, (typ, count))| json!({"name": name, "type": typ, "count": count}))
        .collect();

    Ok((crate::output::format_success(format, &json!(result)), 0))
}

/// List properties of a single file.
fn properties_single(dir: &Path, path_arg: &str, format: Format) -> Result<(String, i32)> {
    let (full_path, rel_path) = match discovery::resolve_file(dir, path_arg) {
        Ok(r) => r,
        Err(FileResolveError::MissingExtension { path, hint }) => {
            let out = crate::output::format_error(
                format,
                "file not found",
                Some(&path),
                Some(&format!("did you mean {hint}?")),
                None,
            );
            return Ok((out, 1));
        }
        Err(FileResolveError::NotFound { path }) => {
            let out =
                crate::output::format_error(format, "file not found", Some(&path), None, None);
            return Ok((out, 1));
        }
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

    Ok((crate::output::format_success(format, &result), 0))
}

/// List properties of files matching a glob pattern.
fn properties_glob(dir: &Path, pattern: &str, format: Format) -> Result<(String, i32)> {
    let files = discovery::discover_files(dir)?;
    let matched = discovery::match_glob(dir, &files, pattern)?;

    if matched.is_empty() {
        let out = crate::output::format_error(
            format,
            "no files match pattern",
            Some(pattern),
            None,
            None,
        );
        return Ok((out, 1));
    }

    let mut results = Vec::new();
    for (full_path, rel_path) in &matched {
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

    Ok((crate::output::format_success(format, &json!(results)), 0))
}

/// Read a single property from a file.
pub fn property_read(
    dir: &Path,
    name: &str,
    path_arg: &str,
    format: Format,
) -> Result<(String, i32)> {
    let (full_path, rel_path) = match resolve_or_error(dir, path_arg, format) {
        Ok(r) => r,
        Err(output) => return Ok(output),
    };

    let props = frontmatter::read_frontmatter(&full_path)?;

    match props.get(name) {
        Some(value) => {
            let typ = frontmatter::infer_type(value);
            let json_val = frontmatter::yaml_to_json(value);
            let result = json!({"name": name, "value": json_val, "type": typ});
            Ok((crate::output::format_success(format, &result), 0))
        }
        None => {
            let out = crate::output::format_error(
                format,
                "property not found",
                Some(&rel_path),
                None,
                None,
            );
            Ok((out, 1))
        }
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
) -> Result<(String, i32)> {
    let (full_path, _rel_path) = match resolve_or_error(dir, path_arg, format) {
        Ok(r) => r,
        Err(output) => return Ok(output),
    };

    let content = fs::read_to_string(&full_path)
        .with_context(|| format!("failed to read {}", full_path.display()))?;
    let mut doc = Document::parse(&content)?;

    let value = frontmatter::parse_value(raw_value, forced_type)?;
    doc.set_property(name.to_owned(), value);

    let serialized = doc.serialize()?;
    fs::write(&full_path, serialized)
        .with_context(|| format!("failed to write {}", full_path.display()))?;

    // Output the new value
    let new_value = doc.get_property(name).expect("just set this property");
    let typ = frontmatter::infer_type(new_value);
    let json_val = frontmatter::yaml_to_json(new_value);
    let result = json!({"name": name, "value": json_val, "type": typ});

    Ok((crate::output::format_success(format, &result), 0))
}

/// Remove a property from a file.
pub fn property_remove(
    dir: &Path,
    name: &str,
    path_arg: &str,
    format: Format,
) -> Result<(String, i32)> {
    let (full_path, rel_path) = match resolve_or_error(dir, path_arg, format) {
        Ok(r) => r,
        Err(output) => return Ok(output),
    };

    let content = fs::read_to_string(&full_path)
        .with_context(|| format!("failed to read {}", full_path.display()))?;
    let mut doc = Document::parse(&content)?;

    match doc.remove_property(name) {
        Some(_) => {
            let serialized = doc.serialize()?;
            fs::write(&full_path, serialized)
                .with_context(|| format!("failed to write {}", full_path.display()))?;
            let result = json!({"removed": name, "path": rel_path});
            Ok((crate::output::format_success(format, &result), 0))
        }
        None => {
            let out = crate::output::format_error(
                format,
                "property not found",
                Some(&rel_path),
                None,
                None,
            );
            Ok((out, 1))
        }
    }
}

/// Helper to resolve a file path or produce an error output tuple.
fn resolve_or_error(
    dir: &Path,
    path_arg: &str,
    format: Format,
) -> Result<(std::path::PathBuf, String), (String, i32)> {
    match discovery::resolve_file(dir, path_arg) {
        Ok(r) => Ok(r),
        Err(FileResolveError::MissingExtension { path, hint }) => Err((
            crate::output::format_error(
                format,
                "file not found",
                Some(&path),
                Some(&format!("did you mean {hint}?")),
                None,
            ),
            1,
        )),
        Err(FileResolveError::NotFound { path }) => Err((
            crate::output::format_error(format, "file not found", Some(&path), None, None),
            1,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_dir() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            "---\ntitle: Test\nstatus: draft\npriority: 3\ntags:\n  - rust\n  - cli\n---\n# Hello\n",
        )
        .unwrap();
        fs::write(tmp.path().join("empty.md"), "No frontmatter here.\n").unwrap();
        tmp
    }

    #[test]
    fn properties_all_aggregates() {
        let tmp = setup_dir();
        let (out, code) = properties(tmp.path(), None, Format::Json).unwrap();
        assert_eq!(code, 0);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        assert!(!parsed.is_empty());
        let names: Vec<&str> = parsed.iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"title"));
        assert!(names.contains(&"status"));
    }

    #[test]
    fn properties_single_file() {
        let tmp = setup_dir();
        let (out, code) = properties(tmp.path(), Some("note.md"), Format::Json).unwrap();
        assert_eq!(code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["path"], "note.md");
        assert_eq!(parsed["properties"]["priority"]["type"], "number");
        assert_eq!(parsed["properties"]["tags"]["type"], "list");
    }

    #[test]
    fn property_read_existing() {
        let tmp = setup_dir();
        let (out, code) = property_read(tmp.path(), "status", "note.md", Format::Json).unwrap();
        assert_eq!(code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["value"], "draft");
        assert_eq!(parsed["type"], "text");
    }

    #[test]
    fn property_read_missing() {
        let tmp = setup_dir();
        let (out, code) =
            property_read(tmp.path(), "nonexistent", "note.md", Format::Json).unwrap();
        assert_eq!(code, 1);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["error"], "property not found");
    }

    #[test]
    fn property_set_new() {
        let tmp = setup_dir();
        let (out, code) =
            property_set(tmp.path(), "author", "Alice", None, "note.md", Format::Json).unwrap();
        assert_eq!(code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["value"], "Alice");

        // Verify it persisted
        let (out2, _) = property_read(tmp.path(), "author", "note.md", Format::Json).unwrap();
        let p2: serde_json::Value = serde_json::from_str(&out2).unwrap();
        assert_eq!(p2["value"], "Alice");
    }

    #[test]
    fn property_set_with_type() {
        let tmp = setup_dir();
        let (out, code) = property_set(
            tmp.path(),
            "count",
            "42",
            Some("text"),
            "note.md",
            Format::Json,
        )
        .unwrap();
        assert_eq!(code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["type"], "text");
        assert_eq!(parsed["value"], "42");
    }

    #[test]
    fn property_remove_existing() {
        let tmp = setup_dir();
        let (out, code) = property_remove(tmp.path(), "status", "note.md", Format::Json).unwrap();
        assert_eq!(code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["removed"], "status");

        // Verify it's gone
        let (_, code2) = property_read(tmp.path(), "status", "note.md", Format::Json).unwrap();
        assert_eq!(code2, 1);
    }

    #[test]
    fn property_remove_missing() {
        let tmp = setup_dir();
        let (_, code) =
            property_remove(tmp.path(), "nonexistent", "note.md", Format::Json).unwrap();
        assert_eq!(code, 1);
    }

    #[test]
    fn file_not_found_error() {
        let tmp = setup_dir();
        let (out, code) = property_read(tmp.path(), "x", "nope.md", Format::Json).unwrap();
        assert_eq!(code, 1);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["error"], "file not found");
    }

    #[test]
    fn missing_extension_hint() {
        let tmp = setup_dir();
        let (out, code) = property_read(tmp.path(), "x", "note", Format::Json).unwrap();
        assert_eq!(code, 1);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["error"], "file not found");
        assert!(parsed["hint"].as_str().unwrap().contains("note.md"));
    }

    #[test]
    fn property_set_creates_frontmatter() {
        let tmp = setup_dir();
        let (_, code) =
            property_set(tmp.path(), "status", "new", None, "empty.md", Format::Json).unwrap();
        assert_eq!(code, 0);

        let content = fs::read_to_string(tmp.path().join("empty.md")).unwrap();
        assert!(content.starts_with("---\n"));
    }
}
