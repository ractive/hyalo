#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use serde::Serialize;
use serde_yaml_ng::Value;
use std::path::Path;

use crate::commands::{FilesOrOutcome, collect_files, require_file_or_glob};
use crate::frontmatter;
use crate::output::{CommandOutcome, Format};

// ---------------------------------------------------------------------------
// Output type
// ---------------------------------------------------------------------------

/// Result of an `append --property K=V` operation across files.
#[derive(Debug, Serialize)]
pub struct AppendPropertyResult {
    pub property: String,
    pub value: String,
    pub modified: Vec<String>,
    pub skipped: Vec<String>,
    pub total: usize,
}

// ---------------------------------------------------------------------------
// Core per-file append helper
// ---------------------------------------------------------------------------

/// Append `new_value` to the list property `name` in each file.
///
/// Promotion rules:
/// - Property absent or null: creates `[new_value]`
/// - Property is a sequence: appends if not already present (case-insensitive for strings)
/// - Property is a scalar string/number/bool: promotes to `[existing, new_value]`
/// - Any other type (Mapping, Tagged): bail with an error
fn append_value_to_property(
    files: &[(std::path::PathBuf, String)],
    name: &str,
    raw_value: &str,
) -> Result<(Vec<String>, Vec<String>)> {
    let new_val = frontmatter::parse_value(raw_value, None)
        .map_err(|e| anyhow::anyhow!("failed to parse value for property '{name}': {e}"))?;
    let new_str = raw_value; // used for case-insensitive duplicate detection

    let mut modified = Vec::new();
    let mut skipped = Vec::new();

    for (full_path, rel_path) in files {
        let mut props = frontmatter::read_frontmatter(full_path)?;

        match props.get(name).cloned() {
            None | Some(Value::Null) => {
                // Create a new single-element list
                props.insert(name.to_owned(), Value::Sequence(vec![new_val.clone()]));
                frontmatter::write_frontmatter(full_path, &props)?;
                modified.push(rel_path.clone());
            }
            Some(Value::Sequence(mut seq)) => {
                // Duplicate detection: case-insensitive for strings, stringified for numbers/bools
                let already_present = seq.iter().any(|v| match v {
                    Value::String(s) => s.eq_ignore_ascii_case(new_str),
                    Value::Number(n) => n.to_string().eq_ignore_ascii_case(new_str),
                    Value::Bool(b) => b.to_string().eq_ignore_ascii_case(new_str),
                    _ => false,
                });
                if already_present {
                    skipped.push(rel_path.clone());
                } else {
                    seq.push(new_val.clone());
                    props.insert(name.to_owned(), Value::Sequence(seq));
                    frontmatter::write_frontmatter(full_path, &props)?;
                    modified.push(rel_path.clone());
                }
            }
            Some(Value::String(existing)) => {
                // Promote scalar string → list
                if existing.eq_ignore_ascii_case(new_str) {
                    skipped.push(rel_path.clone());
                } else {
                    let list = Value::Sequence(vec![Value::String(existing), new_val.clone()]);
                    props.insert(name.to_owned(), list);
                    frontmatter::write_frontmatter(full_path, &props)?;
                    modified.push(rel_path.clone());
                }
            }
            Some(Value::Number(n)) => {
                if n.to_string().eq_ignore_ascii_case(new_str) {
                    skipped.push(rel_path.clone());
                } else {
                    let list = Value::Sequence(vec![Value::Number(n), new_val.clone()]);
                    props.insert(name.to_owned(), list);
                    frontmatter::write_frontmatter(full_path, &props)?;
                    modified.push(rel_path.clone());
                }
            }
            Some(Value::Bool(b)) => {
                if b.to_string().eq_ignore_ascii_case(new_str) {
                    skipped.push(rel_path.clone());
                } else {
                    let list = Value::Sequence(vec![Value::Bool(b), new_val.clone()]);
                    props.insert(name.to_owned(), list);
                    frontmatter::write_frontmatter(full_path, &props)?;
                    modified.push(rel_path.clone());
                }
            }
            Some(other) => {
                // Mapping, Tagged — refuse to silently overwrite
                let kind = match &other {
                    Value::Mapping(_) => "mapping",
                    Value::Tagged(_) => "tagged",
                    _ => "unknown",
                };
                anyhow::bail!(
                    "property '{name}' in '{rel_path}' is a {kind} value — \
                     cannot append to it"
                );
            }
        }
    }

    Ok((modified, skipped))
}

// ---------------------------------------------------------------------------
// `hyalo append` command
// ---------------------------------------------------------------------------

/// Append values to list properties across matched files.
///
/// - `property_args`: one or more `"K=V"` strings
/// - Requires `--file` or `--glob`
/// - At least one `property_args` entry required
pub fn append(
    dir: &Path,
    property_args: &[String],
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    if property_args.is_empty() {
        let out = crate::output::format_error(
            format,
            "append requires at least one --property K=V",
            None,
            Some("example: hyalo append --property aliases=my-alias --file note.md"),
            None,
        );
        return Ok(CommandOutcome::UserError(out));
    }

    if let Some(outcome) = require_file_or_glob(file, glob, "append", format) {
        return Ok(outcome);
    }

    // Validate all K=V args upfront (must have `=`)
    for arg in property_args {
        if !arg.contains('=') {
            let out = crate::output::format_error(
                format,
                &format!(
                    "invalid property argument '{arg}': expected K=V format (e.g. aliases=my-alias)"
                ),
                None,
                None,
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

    for arg in property_args {
        // Already validated that `=` is present
        let eq = arg.find('=').expect("already validated");
        let name = &arg[..eq];
        let raw_value = &arg[eq + 1..];

        let (modified, skipped) = append_value_to_property(&files, name, raw_value)?;
        let total = modified.len() + skipped.len();
        let result = AppendPropertyResult {
            property: name.to_owned(),
            value: raw_value.to_owned(),
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

    // --- append to absent / null property ---

    #[test]
    fn append_creates_new_list() {
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

        let outcome = append(
            tmp.path(),
            &["aliases=my-note".to_owned()],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["property"], "aliases");
        assert_eq!(parsed["value"], "my-note");
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("my-note"));
    }

    // --- append to existing list ---

    #[test]
    fn append_to_existing_list() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
aliases:
  - old-name
---
"),
        )
        .unwrap();

        append(
            tmp.path(),
            &["aliases=new-name".to_owned()],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("old-name"));
        assert!(content.contains("new-name"));
    }

    #[test]
    fn append_to_list_skips_duplicate() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
aliases:
  - my-note
---
"),
        )
        .unwrap();

        let outcome = append(
            tmp.path(),
            &["aliases=my-note".to_owned()],
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
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 0);
    }

    // --- scalar promotion ---

    #[test]
    fn append_promotes_scalar_string() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
author: Alice
---
"),
        )
        .unwrap();

        append(
            tmp.path(),
            &["author=Bob".to_owned()],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("Alice"));
        assert!(content.contains("Bob"));
        // Should now be a YAML list
        assert!(content.contains("- "));
    }

    #[test]
    fn append_promotes_scalar_skips_duplicate() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
author: Alice
---
"),
        )
        .unwrap();

        let outcome = append(
            tmp.path(),
            &["author=Alice".to_owned()],
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

    // --- multiple --property args return array ---

    #[test]
    fn append_multiple_returns_array() {
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

        let outcome = append(
            tmp.path(),
            &["aliases=a".to_owned(), "tags=rust".to_owned()],
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

    // --- guards ---

    #[test]
    fn append_requires_file_or_glob() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = append(
            tmp.path(),
            &["aliases=x".to_owned()],
            None,
            None,
            Format::Json,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn append_requires_at_least_one_property() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = append(tmp.path(), &[], Some("note.md"), None, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn append_invalid_kv_returns_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = append(
            tmp.path(),
            &["no-equals-sign".to_owned()],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn append_preserves_body() {
        let tmp = tempfile::tempdir().unwrap();
        let body = "# Heading\n\nSome content.\n";
        fs::write(
            tmp.path().join("note.md"),
            format!("---\ntitle: Note\n---\n{body}"),
        )
        .unwrap();

        append(
            tmp.path(),
            &["aliases=my-note".to_owned()],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains(body), "body was corrupted:\n{content}");
    }
}
