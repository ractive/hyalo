#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

use crate::commands::set::parse_kv;
use crate::commands::{FilesOrOutcome, collect_files, mutation, require_file_or_glob};
use crate::output::{CommandOutcome, Format};
use hyalo_core::filter::{self, PropertyFilter};
use hyalo_core::frontmatter;
use hyalo_core::index::SnapshotIndex;
use hyalo_core::schema::SchemaConfig;

// ---------------------------------------------------------------------------
// Output type
// ---------------------------------------------------------------------------

/// Result of an `append --property K=V` operation across files.
#[derive(Debug, Serialize)]
pub(crate) struct AppendPropertyResult {
    pub(crate) property: String,
    pub(crate) value: String,
    pub(crate) modified: Vec<String>,
    pub(crate) skipped: Vec<String>,
    pub(crate) total: usize,
    pub(crate) scanned: usize,
    pub(crate) dry_run: bool,
}

// ---------------------------------------------------------------------------
// In-memory append helper
// ---------------------------------------------------------------------------

/// Append `raw_value` to property `name` in already-loaded `props` (no I/O).
///
/// Returns `true` if the value was actually appended (i.e. was not a duplicate),
/// or an error if the property type prevents appending.
///
/// Promotion rules (same as the previous per-file helper):
/// - Property absent or null: creates `[new_value]`
/// - Property is a sequence: appends if not already present (case-insensitive for strings)
/// - Property is a scalar string/number/bool: promotes to `[existing, new_value]`
/// - Any other type (Mapping, Tagged): bail with an error
fn append_value_in_memory(
    props: &mut indexmap::IndexMap<String, Value>,
    name: &str,
    raw_value: &str,
    new_val: &Value,
) -> Result<bool> {
    match props.get(name).cloned() {
        None | Some(Value::Null) => {
            props.insert(name.to_owned(), Value::Array(vec![new_val.clone()]));
            Ok(true)
        }
        Some(Value::Array(mut seq)) => {
            let already_present = seq.iter().any(|v| match v {
                Value::String(s) => s.eq_ignore_ascii_case(raw_value),
                Value::Number(n) => n.to_string().eq_ignore_ascii_case(raw_value),
                Value::Bool(b) => b.to_string().eq_ignore_ascii_case(raw_value),
                _ => false,
            });
            if already_present {
                Ok(false)
            } else {
                seq.push(new_val.clone());
                props.insert(name.to_owned(), Value::Array(seq));
                Ok(true)
            }
        }
        Some(Value::String(existing)) => {
            if existing.eq_ignore_ascii_case(raw_value) {
                Ok(false)
            } else {
                let list = Value::Array(vec![Value::String(existing), new_val.clone()]);
                props.insert(name.to_owned(), list);
                Ok(true)
            }
        }
        Some(Value::Number(n)) => {
            if n.to_string().eq_ignore_ascii_case(raw_value) {
                Ok(false)
            } else {
                let list = Value::Array(vec![Value::Number(n), new_val.clone()]);
                props.insert(name.to_owned(), list);
                Ok(true)
            }
        }
        Some(Value::Bool(b)) => {
            if b.to_string().eq_ignore_ascii_case(raw_value) {
                Ok(false)
            } else {
                let list = Value::Array(vec![Value::Bool(b), new_val.clone()]);
                props.insert(name.to_owned(), list);
                Ok(true)
            }
        }
        Some(other) => {
            let kind = match &other {
                Value::Object(_) => "mapping",
                _ => "unknown",
            };
            anyhow::bail!("property '{name}' is a {kind} value — cannot append to it");
        }
    }
}

// ---------------------------------------------------------------------------
// `hyalo append` command
// ---------------------------------------------------------------------------

/// Append values to list properties across matched files.
///
/// - `property_args`: one or more `"K=V"` strings
/// - Requires `--file` or `--glob`
/// - At least one `property_args` entry required
/// - `validate`: when `true`, validates new values against schema constraints.
#[allow(clippy::too_many_arguments)]
pub fn append(
    dir: &Path,
    property_args: &[String],
    files: &[String],
    globs: &[String],
    where_property_filters: &[PropertyFilter],
    where_tag_filters: &[String],
    format: Format,
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
    dry_run: bool,
    validate: bool,
    schema: Option<&SchemaConfig>,
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

    // Allow omitting --file/--glob when --where-property or --where-tag is provided;
    // in that case, the command defaults to all vault files.
    let has_where = !where_property_filters.is_empty() || !where_tag_filters.is_empty();
    if !has_where && let Some(outcome) = require_file_or_glob(files, globs, "append", format) {
        return Ok(outcome);
    }

    // Validate all K=V args upfront (must have `=` and a non-empty key)
    for arg in property_args {
        match parse_kv(arg) {
            Err(msg) => {
                let out = crate::output::format_error(format, &msg, None, None, None);
                return Ok(CommandOutcome::UserError(out));
            }
            Ok((key, _)) => {
                if let Some(outcome) = super::reject_filter_in_mutation_property(key, format) {
                    return Ok(outcome);
                }
            }
        }
    }

    // Pre-parse all values before touching files: (name, raw_value, parsed_value)
    let parsed_args: Vec<(&str, &str, Value)> = {
        let mut v = Vec::with_capacity(property_args.len());
        for arg in property_args {
            let (name, raw_value) =
                parse_kv(arg).map_err(|e| anyhow::anyhow!("invalid property argument: {e}"))?;
            // Reject empty values for the tags property -- `tags=` would silently
            // insert an empty string into the list, which is never meaningful.
            if name == "tags" && raw_value.trim().is_empty() {
                let out = crate::output::format_error(
                    format,
                    "append --property tags= requires a non-empty tag value",
                    None,
                    Some("example: hyalo append --property tags=my-tag --file note.md"),
                    None,
                );
                return Ok(CommandOutcome::UserError(out));
            }
            let parsed = frontmatter::parse_value(raw_value, None)
                .map_err(|e| anyhow::anyhow!("failed to parse value for property '{name}': {e}"))?;
            v.push((name, raw_value, parsed));
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
        vec![(Vec::new(), Vec::new()); parsed_args.len()];

    // --- Pre-validation pass (BUG-D): validate all proposed writes before any file
    //     is modified. Unlike `set`, `append` validates the *merged post-append*
    //     value so that list constraints (e.g. `type = "list"`) see the resulting
    //     list rather than the individual element.
    if validate && let Some(schema) = schema {
        for (full_path, rel_path) in &files {
            let props = match frontmatter::read_frontmatter(full_path) {
                Ok(p) => p,
                Err(e) if frontmatter::is_parse_error(&e) => continue,
                Err(e) => return Err(e),
            };
            if !filter::matches_frontmatter_filters(
                &props,
                where_property_filters,
                where_tag_filters,
            ) {
                continue;
            }
            // Apply append mutations in-memory to compute the post-mutation props.
            let mut merged = props.clone();
            for (name, raw_value, new_val) in &parsed_args {
                // Errors here (e.g. appending to a mapping) are surfaced during
                // the write loop; validation only needs to run when the mutation
                // succeeds.
                let _ = append_value_in_memory(&mut merged, name, raw_value, new_val);
            }
            let doc_type = merged.get("type").and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.as_str()),
                _ => None,
            });
            let effective_schema = match doc_type {
                Some(t) => schema.merged_schema_for_type(t),
                None => schema.default_schema().clone(),
            };
            for (name, raw_value, _) in &parsed_args {
                if let Some(constraint) = effective_schema.properties.get(*name)
                    && let Some(merged_value) = merged.get(*name)
                    && let Some(violation) = crate::commands::lint::validate_constraint_simple(
                        name,
                        merged_value,
                        constraint,
                    )
                {
                    let out = crate::output::format_error(
                        format,
                        &format!("{rel_path}: {violation}"),
                        None,
                        Some(&format!(
                            "rerun without --validate or fix the value (provided: {raw_value:?})"
                        )),
                        None,
                    );
                    return Ok(CommandOutcome::UserError(out));
                }
            }
        }
    }

    let mut index_dirty = false;

    // Outer loop: one read-modify-write per file
    for (full_path, rel_path) in &files {
        let mtime = frontmatter::read_mtime(full_path)?;
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

        for (i, (name, raw_value, new_val)) in parsed_args.iter().enumerate() {
            match append_value_in_memory(&mut props, name, raw_value, new_val) {
                Ok(true) => {
                    prop_results[i].0.push(rel_path.clone()); // modified
                    file_changed = true;
                }
                Ok(false) => {
                    prop_results[i].1.push(rel_path.clone()); // skipped
                }
                Err(e) => return Err(e),
            }
        }

        if file_changed && !dry_run {
            frontmatter::check_mtime(full_path, mtime)?;
            frontmatter::write_frontmatter(full_path, &props)?;
            mutation::update_index_entry(
                snapshot_index,
                rel_path,
                props,
                full_path,
                &mut index_dirty,
            )?;
        }
    }

    if !dry_run {
        mutation::save_index_if_dirty(snapshot_index, index_path, index_dirty)?;
    }

    let mut results: Vec<serde_json::Value> = Vec::new();

    for ((name, raw_value, _), (modified, skipped)) in parsed_args.iter().zip(prop_results) {
        let total = modified.len() + skipped.len();
        let result = AppendPropertyResult {
            property: (*name).to_owned(),
            value: (*raw_value).to_owned(),
            modified,
            skipped,
            total,
            scanned,
            dry_run,
        };
        results
            .push(serde_json::to_value(&result).expect("derived Serialize impl should not fail"));
    }

    let output = mutation::unwrap_single_result(results);

    Ok(CommandOutcome::success(crate::output::format_success(
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &[],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn append_requires_at_least_one_property() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = append(
            tmp.path(),
            &[],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn append_invalid_kv_returns_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = append(
            tmp.path(),
            &["no-equals-sign".to_owned()],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn append_empty_key_returns_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = append(
            tmp.path(),
            &["=value".to_owned()],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains(body), "body was corrupted:\n{content}");
    }

    #[test]
    fn append_multiple_properties_single_read_write() {
        // Two appends on the same file — both should be present after one write cycle.
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
            &["aliases=a".to_owned(), "aliases=b".to_owned()],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(parsed.is_array());

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains('a'));
        assert!(content.contains('b'));
    }

    #[test]
    fn append_where_property_filter_skips_nonmatching() {
        use hyalo_core::filter::parse_property_filter;
        // Only files matching --where-property are mutated.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("match.md"), "---\nstatus: draft\n---\n").unwrap();
        fs::write(
            tmp.path().join("no-match.md"),
            "---\nstatus: published\n---\n",
        )
        .unwrap();

        let filter = parse_property_filter("status=draft").unwrap();
        let outcome = append(
            tmp.path(),
            &["aliases=draft-copy".to_owned()],
            &[],
            &["*.md".to_owned()],
            &[filter],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);
        // 2 files scanned, 1 passed the where-filter
        assert_eq!(parsed["scanned"].as_u64().unwrap(), 2);
        assert!(parsed["scanned"].as_u64().unwrap() > parsed["total"].as_u64().unwrap());

        let match_content = fs::read_to_string(tmp.path().join("match.md")).unwrap();
        assert!(match_content.contains("draft-copy"));
        let no_match_content = fs::read_to_string(tmp.path().join("no-match.md")).unwrap();
        assert!(!no_match_content.contains("draft-copy"));
    }

    #[test]
    fn append_where_tag_filter_skips_nonmatching() {
        // Only files with the required tag are mutated.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("tagged.md"), "---\ntags:\n  - rust\n---\n").unwrap();
        fs::write(tmp.path().join("untagged.md"), "---\ntitle: Other\n---\n").unwrap();

        let outcome = append(
            tmp.path(),
            &["aliases=rust-note".to_owned()],
            &[],
            &["*.md".to_owned()],
            &[],
            &["rust".to_owned()],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);
        // 2 files scanned, 1 passed the where-filter
        assert_eq!(parsed["scanned"].as_u64().unwrap(), 2);
        assert!(parsed["scanned"].as_u64().unwrap() > parsed["total"].as_u64().unwrap());

        let tagged_content = fs::read_to_string(tmp.path().join("tagged.md")).unwrap();
        assert!(tagged_content.contains("rust-note"));
        let untagged_content = fs::read_to_string(tmp.path().join("untagged.md")).unwrap();
        assert!(!untagged_content.contains("rust-note"));
    }

    // --- filter guard ---

    #[test]
    fn append_rejects_gte_filter_in_property() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = append(
            tmp.path(),
            &["priority>=3".to_owned()],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        match outcome {
            CommandOutcome::UserError(msg) => {
                assert!(msg.contains("--where-property"), "msg: {msg}");
            }
            other => panic!("expected UserError, got: {other:?}"),
        }
    }

    #[test]
    fn append_rejects_neq_filter_in_property() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = append(
            tmp.path(),
            &["status!=draft".to_owned()],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn append_rejects_regex_filter_in_property() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = append(
            tmp.path(),
            &["name~=pattern".to_owned()],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn append_tags_empty_value_returns_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = append(
            tmp.path(),
            &["tags=".to_owned()],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        match outcome {
            CommandOutcome::UserError(msg) => {
                assert!(
                    msg.contains("non-empty tag value"),
                    "unexpected error message: {msg}"
                );
            }
            other => panic!("expected UserError, got: {other:?}"),
        }
    }

    // ---------------------------------------------------------------------------
    // BUG-D: `append --validate` validates the merged (post-append) list value.
    // Appending a valid element to an existing list must pass even when the
    // per-element shape looks "incompatible" with a list-typed constraint.
    // ---------------------------------------------------------------------------

    #[test]
    fn append_validate_passes_with_merged_list_value() {
        use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};
        use std::collections::HashMap;

        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: My Note
type: post
tags:
  - alpha
---
"),
        )
        .unwrap();

        // Schema: post.tags is list-typed. Without the fix, validation would
        // run against the raw scalar "beta" and fail ("expected list, got
        // \"beta\""). With the fix, validation runs on the merged list value
        // ["alpha", "beta"], which satisfies the List constraint.
        let mut type_props = HashMap::new();
        type_props.insert("tags".to_owned(), PropertyConstraint::List);
        let schema = SchemaConfig {
            default: TypeSchema::default(),
            types: {
                let mut m = HashMap::new();
                m.insert(
                    "post".to_owned(),
                    TypeSchema {
                        required: vec![],
                        properties: type_props,
                        filename_template: None,
                        defaults: HashMap::new(),
                    },
                );
                m
            },
        };

        let outcome = append(
            tmp.path(),
            &["tags=beta".to_owned()],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            true, // validate = true — merged list ["alpha","beta"] must pass
            Some(&schema),
        )
        .unwrap();
        assert!(
            matches!(outcome, CommandOutcome::Success { .. }),
            "append of valid element should succeed under --validate"
        );
    }

    #[test]
    fn append_validate_rejects_when_merged_list_violates_constraint() {
        use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};
        use std::collections::HashMap;

        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: My Note
type: post
author: alice
---
"),
        )
        .unwrap();

        // `author` is a single-valued String property. Appending a second
        // value converts it into a list, which the merged-value validation
        // must reject ("expected string, got <array>").
        let mut type_props = HashMap::new();
        type_props.insert(
            "author".to_owned(),
            PropertyConstraint::String { pattern: None },
        );
        let schema = SchemaConfig {
            default: TypeSchema::default(),
            types: {
                let mut m = HashMap::new();
                m.insert(
                    "post".to_owned(),
                    TypeSchema {
                        required: vec![],
                        properties: type_props,
                        filename_template: None,
                        defaults: HashMap::new(),
                    },
                );
                m
            },
        };

        let outcome = append(
            tmp.path(),
            &["author=bob".to_owned()],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
            true,
            Some(&schema),
        )
        .unwrap();
        assert!(
            matches!(outcome, CommandOutcome::UserError(_)),
            "append that violates merged-value constraint should fail under --validate"
        );
    }
}
