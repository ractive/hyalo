#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

use crate::commands::{FilesOrOutcome, collect_files, require_file_or_glob};
use crate::output::{CommandOutcome, Format};
use hyalo_core::filter::{self, PropertyFilter, extract_tags};
use hyalo_core::frontmatter;
use hyalo_core::index::{SnapshotIndex, format_modified};

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
    pub scanned: usize,
}

/// Result of a `set --tag T` operation across files.
#[derive(Debug, Serialize)]
pub struct SetTagResult {
    pub tag: String,
    pub modified: Vec<String>,
    pub skipped: Vec<String>,
    pub total: usize,
    pub scanned: usize,
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
            "invalid property argument '{s}': expected K=V format (e.g. status=completed)"
        )),
    }
}

// ---------------------------------------------------------------------------
// In-memory tag mutation helper
// ---------------------------------------------------------------------------

/// Add `tag` to the `tags` list in `props` (in memory only, no I/O).
///
/// Returns `true` if the tag was actually added (i.e. was not already present).
///
/// Mirrors the logic in `add_values_to_list_property` for the `tags` key, but
/// operates on an already-loaded `IndexMap` to avoid a second `read_frontmatter`
/// call when processing multiple mutations for the same file.
fn add_tag_in_memory(props: &mut indexmap::IndexMap<String, Value>, tag: &str) -> Result<bool> {
    const KEY: &str = "tags";

    // Guard: reject non-list scalar types that are neither string nor sequence.
    match props.get(KEY) {
        None | Some(Value::Null | Value::String(_) | Value::Array(_)) => {}
        Some(existing) => {
            let kind = match existing {
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::Object(_) => "mapping",
                _ => "unknown",
            };
            anyhow::bail!(
                "property 'tags' is a {kind} value, not a list — \
                 use `set --property` to overwrite it explicitly"
            );
        }
    }

    if let Some(Value::Array(seq)) = props.get_mut(KEY) {
        let already = seq.iter().any(|v| match v {
            Value::String(s) => s.eq_ignore_ascii_case(tag),
            Value::Number(n) => n.to_string().eq_ignore_ascii_case(tag),
            Value::Bool(b) => b.to_string().eq_ignore_ascii_case(tag),
            _ => false,
        });
        if already {
            return Ok(false);
        }
        seq.push(Value::String(tag.to_owned()));
        Ok(true)
    } else {
        // Absent / null / scalar-string: build a new list.
        let existing_str = match props.get(KEY) {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };

        // Duplicate check against existing scalar string (if any).
        if let Some(ref s) = existing_str
            && s.eq_ignore_ascii_case(tag)
        {
            return Ok(false);
        }

        let mut list: Vec<Value> = existing_str.map(Value::String).into_iter().collect();
        list.push(Value::String(tag.to_owned()));
        props.insert(KEY.to_owned(), Value::Array(list));
        Ok(true)
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
#[allow(clippy::too_many_arguments)]
pub fn set(
    dir: &Path,
    property_args: &[String],
    tag_args: &[String],
    files: &[String],
    globs: &[String],
    where_property_filters: &[PropertyFilter],
    where_tag_filters: &[String],
    format: Format,
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
) -> Result<CommandOutcome> {
    // At least one mutation target required
    if property_args.is_empty() && tag_args.is_empty() {
        let out = crate::output::format_error(
            format,
            "set requires at least one --property K=V or --tag T",
            None,
            Some("example: hyalo set --property status=completed --file note.md"),
            None,
        );
        return Ok(CommandOutcome::UserError(out));
    }

    // Mutation commands require --file or --glob
    if let Some(outcome) = require_file_or_glob(files, globs, "set", format) {
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

    // Pre-parse all property values before touching files
    // Each entry is (name, raw_value, parsed_value)
    let parsed_props: Vec<(&str, &str, Value)> = {
        let mut v = Vec::with_capacity(property_args.len());
        for arg in property_args {
            let (name, raw_value) = parse_kv(arg).expect("already validated");
            let value = match frontmatter::parse_value(raw_value, None) {
                Ok(val) => val,
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
            v.push((name, raw_value, value));
        }
        v
    };

    let files = collect_files(dir, files, globs, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };
    let scanned = files.len();

    // Per-property result accumulators: (modified, skipped)
    let mut prop_results: Vec<(Vec<String>, Vec<String>)> =
        vec![(Vec::new(), Vec::new()); parsed_props.len()];
    // Per-tag result accumulators: (modified, skipped)
    let mut tag_results: Vec<(Vec<String>, Vec<String>)> =
        vec![(Vec::new(), Vec::new()); tag_args.len()];

    let mut index_dirty = false;

    // Outer loop: one read-modify-write per file
    for (full_path, rel_path) in &files {
        let mut props = match frontmatter::read_frontmatter(full_path) {
            Ok(p) => p,
            Err(e) if frontmatter::is_parse_error(&e) => {
                crate::warn::warn(format!("skipping {rel_path}: {e}"));
                continue;
            }
            Err(e) => return Err(e),
        };

        // Apply --where-* filters: skip files that don't match
        if !filter::matches_frontmatter_filters(&props, where_property_filters, where_tag_filters) {
            continue;
        }

        let mut file_changed = false;

        // Apply all --property mutations
        for (i, (name, _, value)) in parsed_props.iter().enumerate() {
            let already_same = props.get(*name) == Some(value);
            if already_same {
                prop_results[i].1.push(rel_path.clone()); // skipped
            } else {
                props.insert((*name).to_owned(), value.clone());
                prop_results[i].0.push(rel_path.clone()); // modified
                file_changed = true;
            }
        }

        // Apply all --tag mutations
        for (i, tag) in tag_args.iter().enumerate() {
            match add_tag_in_memory(&mut props, tag) {
                Ok(true) => {
                    tag_results[i].0.push(rel_path.clone()); // modified
                    file_changed = true;
                }
                Ok(false) => {
                    tag_results[i].1.push(rel_path.clone()); // skipped
                }
                Err(e) => return Err(e),
            }
        }

        if file_changed {
            frontmatter::write_frontmatter(full_path, &props)?;
            if let Some(idx) = snapshot_index.as_mut()
                && let Some(entry) = idx.get_mut(rel_path)
            {
                entry.properties = props.clone();
                entry.tags = extract_tags(&props);
                entry.modified = format_modified(full_path)?;
                index_dirty = true;
            }
        }
    }

    if index_dirty && let (Some(idx), Some(idx_path)) = (snapshot_index.as_mut(), index_path) {
        idx.save_to(idx_path)?;
    }

    let mut results: Vec<serde_json::Value> = Vec::new();

    for ((name, raw_value, _), (modified, skipped)) in
        parsed_props.iter().zip(prop_results.into_iter())
    {
        let total = modified.len() + skipped.len();
        let result = SetPropertyResult {
            property: (*name).to_owned(),
            value: (*raw_value).to_owned(),
            modified,
            skipped,
            total,
            scanned,
        };
        results
            .push(serde_json::to_value(&result).expect("derived Serialize impl should not fail"));
    }

    for (tag, (modified, skipped)) in tag_args.iter().zip(tag_results.into_iter()) {
        let total = modified.len() + skipped.len();
        let result = SetTagResult {
            tag: tag.clone(),
            modified,
            skipped,
            total,
            scanned,
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
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
        assert_eq!(parsed["scanned"].as_u64().unwrap(), 1);
        assert_eq!(parsed["scanned"], parsed["total"]);

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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["skipped"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["scanned"], parsed["total"]);
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
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
            &[],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn set_requires_at_least_one_arg() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = set(
            tmp.path(),
            &[],
            &[],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains(body), "body was corrupted:\n{content}");
    }

    #[test]
    fn set_multiple_properties_single_read_write() {
        // Setting two properties on the same file should produce both mutations
        // from a single read-modify-write cycle.
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
            &["status=done".to_owned(), "priority=high".to_owned()],
            &[],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(parsed.is_array());
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        // Both properties modified
        assert_eq!(arr[0]["modified"].as_array().unwrap().len(), 1);
        assert_eq!(arr[1]["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("status: done"));
        assert!(content.contains("priority: high"));
    }

    #[test]
    fn set_property_and_tag_single_read_write() {
        // Setting a property and a tag on the same file: both applied in one cycle.
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(parsed.is_array());

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("status: done"));
        assert!(content.contains("rust"));
    }

    #[test]
    fn set_where_property_filter_skips_nonmatching() {
        // Files that don't match --where-property are not mutated.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("match.md"), "---\nstatus: draft\n---\n").unwrap();
        fs::write(
            tmp.path().join("no-match.md"),
            "---\nstatus: published\n---\n",
        )
        .unwrap();

        use hyalo_core::filter::parse_property_filter;
        let filter = parse_property_filter("status=draft").unwrap();
        let outcome = set(
            tmp.path(),
            &["priority=high".to_owned()],
            &[],
            &[],
            &["*.md".to_owned()],
            &[filter],
            &[],
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["skipped"].as_array().unwrap().len(), 0);
        // 2 files scanned, 1 passed the where-filter (total = modified + skipped)
        assert_eq!(parsed["scanned"].as_u64().unwrap(), 2);
        assert!(parsed["scanned"].as_u64().unwrap() > parsed["total"].as_u64().unwrap());

        let match_content = fs::read_to_string(tmp.path().join("match.md")).unwrap();
        assert!(match_content.contains("priority: high"));
        let no_match_content = fs::read_to_string(tmp.path().join("no-match.md")).unwrap();
        assert!(!no_match_content.contains("priority"));
    }

    #[test]
    fn set_where_tag_filter_skips_nonmatching() {
        // Files without the required tag are not mutated.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("tagged.md"), "---\ntags:\n  - rust\n---\n").unwrap();
        fs::write(tmp.path().join("untagged.md"), "---\ntitle: Other\n---\n").unwrap();

        let outcome = set(
            tmp.path(),
            &["status=reviewed".to_owned()],
            &[],
            &[],
            &["*.md".to_owned()],
            &[],
            &["rust".to_owned()],
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);
        // 2 files scanned, 1 passed the where-filter
        assert_eq!(parsed["scanned"].as_u64().unwrap(), 2);
        assert!(parsed["scanned"].as_u64().unwrap() > parsed["total"].as_u64().unwrap());

        let tagged_content = fs::read_to_string(tmp.path().join("tagged.md")).unwrap();
        assert!(tagged_content.contains("status: reviewed"));
        let untagged_content = fs::read_to_string(tmp.path().join("untagged.md")).unwrap();
        assert!(!untagged_content.contains("status"));
    }
}
