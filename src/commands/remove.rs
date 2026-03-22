#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use serde::Serialize;
use serde_yaml_ng::Value;
use std::path::Path;

use crate::commands::properties::{ListOpResult, remove_values_from_list_property};
use crate::commands::tags::validate_tag;
use crate::commands::{FilesOrOutcome, collect_files, require_file_or_glob};
use crate::frontmatter;
use crate::output::{CommandOutcome, Format};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Result of a `remove --property K` (or `K=V`) operation across files.
#[derive(Debug, Serialize)]
pub struct RemovePropertyResult {
    pub property: String,
    /// Present when `remove --property K=V` was used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub modified: Vec<String>,
    pub skipped: Vec<String>,
    pub total: usize,
}

/// Result of a `remove --tag T` operation across files.
#[derive(Debug, Serialize)]
pub struct RemoveTagResult {
    pub tag: String,
    pub modified: Vec<String>,
    pub skipped: Vec<String>,
    pub total: usize,
}

// ---------------------------------------------------------------------------
// Parsing helper
// ---------------------------------------------------------------------------

/// Parse a `K` or `K=V` property-removal argument.
///
/// Returns `(name, Some(value))` when an `=` is present, `(name, None)` otherwise.
pub fn parse_kv_optional(s: &str) -> (&str, Option<&str>) {
    match s.find('=') {
        Some(pos) => (&s[..pos], Some(&s[pos + 1..])),
        None => (s, None),
    }
}

// ---------------------------------------------------------------------------
// Core per-file removal helpers
// ---------------------------------------------------------------------------

/// Remove a scalar property `name` from each file; skip files where it is absent.
fn remove_property_key(
    files: &[(std::path::PathBuf, String)],
    name: &str,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut modified = Vec::new();
    let mut skipped = Vec::new();

    for (full_path, rel_path) in files {
        let mut props = frontmatter::read_frontmatter(full_path)?;
        if props.remove(name).is_some() {
            frontmatter::write_frontmatter(full_path, &props)?;
            modified.push(rel_path.clone());
        } else {
            skipped.push(rel_path.clone());
        }
    }

    Ok((modified, skipped))
}

/// Remove value `target` from property `name` in each file.
///
/// Semantics:
/// - If the property is a list:      remove `target` from the list (like `remove-from-list`).
/// - If the property is a scalar and matches `target` (case-insensitive):  remove the key.
/// - If the property is a scalar and does not match: skip the file.
/// - If the property is absent: skip the file.
fn remove_property_value(
    files: &[(std::path::PathBuf, String)],
    name: &str,
    target: &str,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut modified = Vec::new();
    let mut skipped = Vec::new();

    for (full_path, rel_path) in files {
        let mut props = frontmatter::read_frontmatter(full_path)?;

        match props.get(name).cloned() {
            None => {
                skipped.push(rel_path.clone());
            }
            Some(Value::Sequence(_)) => {
                // Delegate to the list-removal helper for this single file.
                let single = vec![(full_path.clone(), rel_path.clone())];
                let ListOpResult { modified: m, .. } =
                    remove_values_from_list_property(&single, name, &[target.to_owned()])?;
                if m.is_empty() {
                    skipped.push(rel_path.clone());
                } else {
                    modified.push(rel_path.clone());
                }
            }
            Some(Value::String(s)) => {
                if s.eq_ignore_ascii_case(target) {
                    props.remove(name);
                    frontmatter::write_frontmatter(full_path, &props)?;
                    modified.push(rel_path.clone());
                } else {
                    skipped.push(rel_path.clone());
                }
            }
            Some(Value::Number(n)) => {
                if n.to_string().eq_ignore_ascii_case(target) {
                    props.remove(name);
                    frontmatter::write_frontmatter(full_path, &props)?;
                    modified.push(rel_path.clone());
                } else {
                    skipped.push(rel_path.clone());
                }
            }
            Some(Value::Bool(b)) => {
                if b.to_string().eq_ignore_ascii_case(target) {
                    props.remove(name);
                    frontmatter::write_frontmatter(full_path, &props)?;
                    modified.push(rel_path.clone());
                } else {
                    skipped.push(rel_path.clone());
                }
            }
            Some(_) => {
                // Null, Mapping, Tagged — skip silently
                skipped.push(rel_path.clone());
            }
        }
    }

    Ok((modified, skipped))
}

// ---------------------------------------------------------------------------
// `hyalo remove` command
// ---------------------------------------------------------------------------

/// Remove properties and/or tags across matched files.
///
/// - `property_args`: zero or more `"K"` (remove key) or `"K=V"` (remove value) strings
/// - `tag_args`:      zero or more tag name strings to remove
/// - Requires `--file` or `--glob`
/// - At least one of `property_args` or `tag_args` must be non-empty
pub fn remove(
    dir: &Path,
    property_args: &[String],
    tag_args: &[String],
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    if property_args.is_empty() && tag_args.is_empty() {
        let out = crate::output::format_error(
            format,
            "remove requires at least one --property K or --tag T",
            None,
            Some("example: hyalo remove --property status --file note.md"),
            None,
        );
        return Ok(CommandOutcome::UserError(out));
    }

    if let Some(outcome) = require_file_or_glob(file, glob, "remove", format) {
        return Ok(outcome);
    }

    // Validate tag names before touching files
    for tag in tag_args {
        if let Err(msg) = validate_tag(tag) {
            let out = crate::output::format_error(
                format,
                &msg,
                None,
                Some(
                    "tag names may contain letters, digits, _, -, / and must have at least one non-numeric character",
                ),
                None,
            );
            return Ok(CommandOutcome::UserError(out));
        }
    }

    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let mut results: Vec<serde_json::Value> = Vec::new();

    // Handle --property K or K=V
    for arg in property_args {
        let (name, opt_value) = parse_kv_optional(arg);

        let (modified, skipped) = match opt_value {
            None => remove_property_key(&files, name)?,
            Some(target) => remove_property_value(&files, name, target)?,
        };

        let total = modified.len() + skipped.len();
        let result = RemovePropertyResult {
            property: name.to_owned(),
            value: opt_value.map(str::to_owned),
            modified,
            skipped,
            total,
        };
        results
            .push(serde_json::to_value(&result).expect("derived Serialize impl should not fail"));
    }

    // Handle --tag T
    for tag in tag_args {
        let ListOpResult { modified, skipped } =
            remove_values_from_list_property(&files, "tags", std::slice::from_ref(tag))?;
        let total = modified.len() + skipped.len();
        let result = RemoveTagResult {
            tag: tag.clone(),
            modified,
            skipped,
            total,
        };
        results
            .push(serde_json::to_value(&result).expect("derived Serialize impl should not fail"));
    }

    let output = if results.len() == 1 {
        results.pop().unwrap_or_default()
    } else {
        serde_json::json!(results)
    };

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &output,
    )))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
    }

    // --- parse_kv_optional ---

    #[test]
    fn parse_kv_optional_key_only() {
        assert_eq!(parse_kv_optional("status"), ("status", None));
    }

    #[test]
    fn parse_kv_optional_key_value() {
        assert_eq!(parse_kv_optional("status=done"), ("status", Some("done")));
    }

    #[test]
    fn parse_kv_optional_value_with_equals() {
        assert_eq!(
            parse_kv_optional("url=http://x=y"),
            ("url", Some("http://x=y"))
        );
    }

    // --- remove --property K (key removal) ---

    #[test]
    fn remove_property_key_existing() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: Note
status: draft
---
"),
        )
        .unwrap();

        let outcome = remove(
            tmp.path(),
            &["status".to_owned()],
            &[],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["property"], "status");
        assert!(parsed.get("value").is_none() || parsed["value"].is_null());
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(!content.contains("status:"));
        assert!(content.contains("title:"));
    }

    #[test]
    fn remove_property_key_missing_skips() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: Note
---
"),
        )
        .unwrap();

        let outcome = remove(
            tmp.path(),
            &["status".to_owned()],
            &[],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["skipped"].as_array().unwrap().len(), 1);
    }

    // --- remove --property K=V (value removal from scalar) ---

    #[test]
    fn remove_property_value_scalar_match() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
status: draft
---
"),
        )
        .unwrap();

        let outcome = remove(
            tmp.path(),
            &["status=draft".to_owned()],
            &[],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(!content.contains("status:"));
    }

    #[test]
    fn remove_property_value_scalar_no_match_skips() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
status: published
---
"),
        )
        .unwrap();

        let outcome = remove(
            tmp.path(),
            &["status=draft".to_owned()],
            &[],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["skipped"].as_array().unwrap().len(), 1);
        // File should be unchanged
        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("published"));
    }

    // --- remove --property K=V (value removal from list) ---

    #[test]
    fn remove_property_value_list_removes_element() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
aliases:
  - old-name
  - other
---
"),
        )
        .unwrap();

        let outcome = remove(
            tmp.path(),
            &["aliases=old-name".to_owned()],
            &[],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(!content.contains("old-name"));
        assert!(content.contains("other"));
    }

    // --- remove --tag T ---

    #[test]
    fn remove_tag_existing() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
tags:
  - rust
  - cli
---
"),
        )
        .unwrap();

        let outcome = remove(
            tmp.path(),
            &[],
            &["rust".to_owned()],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["tag"], "rust");
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(!content.contains("rust"));
        assert!(content.contains("cli"));
    }

    #[test]
    fn remove_tag_not_present_skips() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
tags:
  - cli
---
"),
        )
        .unwrap();

        let outcome = remove(
            tmp.path(),
            &[],
            &["rust".to_owned()],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["skipped"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn remove_requires_file_or_glob() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = remove(
            tmp.path(),
            &["status".to_owned()],
            &[],
            None,
            None,
            Format::Json,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn remove_requires_at_least_one_arg() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = remove(tmp.path(), &[], &[], Some("note.md"), None, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn remove_multiple_mutations_returns_array() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
status: draft
tags:
  - rust
---
"),
        )
        .unwrap();

        let outcome = remove(
            tmp.path(),
            &["status".to_owned()],
            &["rust".to_owned()],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn remove_preserves_body() {
        let tmp = tempfile::tempdir().unwrap();
        let body = "# Heading\n\nSome content.\n";
        fs::write(
            tmp.path().join("note.md"),
            format!("---\nstatus: draft\ntitle: Note\n---\n{body}"),
        )
        .unwrap();

        remove(
            tmp.path(),
            &["status".to_owned()],
            &[],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains(body), "body was corrupted:\n{content}");
    }
}
