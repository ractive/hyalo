#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use crate::commands::properties::{ListOpResult, add_values_to_list_property};
use crate::commands::{FilesOrOutcome, collect_files, require_file_or_glob};
use crate::frontmatter;
use crate::output::{CommandOutcome, Format};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Result of a `set --property K=V` operation across files.
#[derive(Debug, Serialize)]
pub struct SetPropertyResult {
    pub property: String,
    pub value: String,
    pub modified: Vec<String>,
    pub skipped: Vec<String>,
    pub total: usize,
}

/// Result of a `set --tag T` operation across files.
#[derive(Debug, Serialize)]
pub struct SetTagResult {
    pub tag: String,
    pub modified: Vec<String>,
    pub skipped: Vec<String>,
    pub total: usize,
}

// ---------------------------------------------------------------------------
// Parsing helper
// ---------------------------------------------------------------------------

/// Parse a `K=V` string into `(name, raw_value)`.
///
/// Returns a user-visible error if no `=` is found.
pub fn parse_kv(s: &str) -> Result<(&str, &str), String> {
    match s.find('=') {
        Some(pos) => {
            let key = &s[..pos];
            if key.trim().is_empty() {
                return Err(format!(
                    "invalid property argument '{s}': property name cannot be empty"
                ));
            }
            Ok((key, &s[pos + 1..]))
        }
        None => Err(format!(
            "invalid property argument '{s}': expected K=V format (e.g. status=done)"
        )),
    }
}

// ---------------------------------------------------------------------------
// `hyalo set` command
// ---------------------------------------------------------------------------

/// Set properties and/or tags across matched files.
///
/// - `property_args`: zero or more `"K=V"` strings (type is inferred from V)
/// - `tag_args`:      zero or more tag name strings
/// - Requires `--file` or `--glob`
/// - At least one of `property_args` or `tag_args` must be non-empty
pub fn set(
    dir: &Path,
    property_args: &[String],
    tag_args: &[String],
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    // At least one mutation target required
    if property_args.is_empty() && tag_args.is_empty() {
        let out = crate::output::format_error(
            format,
            "set requires at least one --property K=V or --tag T",
            None,
            Some("example: hyalo set --property status=done --file note.md"),
            None,
        );
        return Ok(CommandOutcome::UserError(out));
    }

    // Mutation commands require --file or --glob
    if let Some(outcome) = require_file_or_glob(file, glob, "set", format) {
        return Ok(outcome);
    }

    // Validate all K=V args before touching files
    for arg in property_args {
        if let Err(msg) = parse_kv(arg) {
            let out = crate::output::format_error(format, &msg, None, None, None);
            return Ok(CommandOutcome::UserError(out));
        }
    }

    // Validate tag names
    for tag in tag_args {
        if let Err(msg) = crate::commands::tags::validate_tag(tag) {
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

    // Handle --property K=V args
    for arg in property_args {
        // Already validated above; safe to unwrap the parse
        let (name, raw_value) = parse_kv(arg).expect("already validated");
        let value = match frontmatter::parse_value(raw_value, None) {
            Ok(v) => v,
            Err(e) => {
                let out = crate::output::format_error(
                    format,
                    &format!("failed to parse value for property '{name}': {e}"),
                    None,
                    None,
                    None,
                );
                return Ok(CommandOutcome::UserError(out));
            }
        };

        let mut modified = Vec::new();
        let mut skipped = Vec::new();

        for (full_path, rel_path) in &files {
            let mut props = frontmatter::read_frontmatter(full_path)?;
            // Check if value is identical to what's already there
            let already_same = props.get(name) == Some(&value);
            if already_same {
                skipped.push(rel_path.clone());
            } else {
                props.insert(name.to_owned(), value.clone());
                frontmatter::write_frontmatter(full_path, &props)?;
                modified.push(rel_path.clone());
            }
        }

        let total = modified.len() + skipped.len();
        let result = SetPropertyResult {
            property: name.to_owned(),
            value: raw_value.to_owned(),
            modified,
            skipped,
            total,
        };
        results
            .push(serde_json::to_value(&result).expect("derived Serialize impl should not fail"));
    }

    // Handle --tag T args
    for tag in tag_args {
        let ListOpResult { modified, skipped } =
            add_values_to_list_property(&files, "tags", std::slice::from_ref(tag))?;
        let total = modified.len() + skipped.len();
        let result = SetTagResult {
            tag: tag.clone(),
            modified,
            skipped,
            total,
        };
        results
            .push(serde_json::to_value(&result).expect("derived Serialize impl should not fail"));
    }

    // Return array if multiple mutations, single object if one
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

    // --- parse_kv ---

    #[test]
    fn parse_kv_simple() {
        assert_eq!(parse_kv("status=done").unwrap(), ("status", "done"));
    }

    #[test]
    fn parse_kv_first_equals_only() {
        // Only the first `=` is the separator; value may contain `=`
        assert_eq!(parse_kv("url=http://x=y").unwrap(), ("url", "http://x=y"));
    }

    #[test]
    fn parse_kv_no_equals() {
        assert!(parse_kv("nodot").is_err());
    }

    #[test]
    fn parse_kv_empty_key_returns_error() {
        let err = parse_kv("=value").unwrap_err();
        assert!(
            err.contains("property name cannot be empty"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_kv_empty_value() {
        assert_eq!(parse_kv("key=").unwrap(), ("key", ""));
    }

    // --- set command ---

    #[test]
    fn set_property_creates_new() {
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

        let outcome = set(
            tmp.path(),
            &["status=done".to_owned()],
            &[],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        let out = match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["property"], "status");
        assert_eq!(parsed["value"], "done");
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("status: done"));
    }

    #[test]
    fn set_property_overwrites_existing() {
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

        set(
            tmp.path(),
            &["status=published".to_owned()],
            &[],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("status: published"));
        assert!(!content.contains("draft"));
    }

    #[test]
    fn set_property_skips_when_identical() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
status: done
---
"),
        )
        .unwrap();

        let outcome = set(
            tmp.path(),
            &["status=done".to_owned()],
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

    #[test]
    fn set_tag_adds_tag() {
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

        let outcome = set(
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
        assert!(content.contains("rust"));
    }

    #[test]
    fn set_tag_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
tags:
  - rust
---
"),
        )
        .unwrap();

        let outcome = set(
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
    fn set_multiple_mutations_returns_array() {
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

        let outcome = set(
            tmp.path(),
            &["status=done".to_owned()],
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
        assert!(parsed.is_array(), "multiple mutations should return array");
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn set_requires_file_or_glob() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = set(
            tmp.path(),
            &["status=done".to_owned()],
            &[],
            None,
            None,
            Format::Json,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn set_requires_at_least_one_arg() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = set(tmp.path(), &[], &[], Some("note.md"), None, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn set_invalid_kv_returns_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = set(
            tmp.path(),
            &["no-equals-sign".to_owned()],
            &[],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn set_invalid_tag_returns_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = set(
            tmp.path(),
            &[],
            &["1984".to_owned()],
            Some("note.md"),
            None,
            Format::Json,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn set_preserves_body() {
        let tmp = tempfile::tempdir().unwrap();
        let body = "# Heading\n\nSome content.\n";
        fs::write(
            tmp.path().join("note.md"),
            format!("---\ntitle: Note\n---\n{body}"),
        )
        .unwrap();

        set(
            tmp.path(),
            &["status=done".to_owned()],
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
