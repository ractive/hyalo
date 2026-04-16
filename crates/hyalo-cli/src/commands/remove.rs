#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

use crate::commands::{FilesOrOutcome, collect_files, mutation, require_file_or_glob};
use crate::output::{CommandOutcome, Format};
use hyalo_core::filter::{self, PropertyFilter};
use hyalo_core::frontmatter;
use hyalo_core::index::SnapshotIndex;

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Result of a `remove --property K` (or `K=V`) operation across files.
#[derive(Debug, Serialize)]
pub(crate) struct RemovePropertyResult {
    pub(crate) property: String,
    /// Present when `remove --property K=V` was used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<String>,
    pub(crate) modified: Vec<String>,
    pub(crate) skipped: Vec<String>,
    pub(crate) total: usize,
    pub(crate) scanned: usize,
    pub(crate) dry_run: bool,
}

/// Result of a `remove --tag T` operation across files.
#[derive(Debug, Serialize)]
pub(crate) struct RemoveTagResult {
    pub(crate) tag: String,
    pub(crate) modified: Vec<String>,
    pub(crate) skipped: Vec<String>,
    pub(crate) total: usize,
    pub(crate) scanned: usize,
    pub(crate) dry_run: bool,
}

// ---------------------------------------------------------------------------
// Parsing helper
// ---------------------------------------------------------------------------

/// Parse a `K` or `K=V` property-removal argument.
///
/// Returns `(name, Some(value))` when an `=` is present, `(name, None)` otherwise.
/// Returns an error if an `=` is present but the key portion is empty or all whitespace.
pub fn parse_kv_optional(s: &str) -> Result<(&str, Option<&str>), String> {
    match s.find('=') {
        Some(pos) => {
            let key = &s[..pos];
            if key.trim().is_empty() {
                return Err(format!(
                    "invalid property argument '{s}': property name cannot be empty"
                ));
            }
            Ok((key, Some(&s[pos + 1..])))
        }
        None => Ok((s, None)),
    }
}

// ---------------------------------------------------------------------------
// In-memory removal helpers
// ---------------------------------------------------------------------------

/// Remove scalar property `name` from `props` (in memory, no I/O).
///
/// Returns `true` if the key was present and removed, `false` if absent.
fn remove_key_in_memory(props: &mut indexmap::IndexMap<String, Value>, name: &str) -> bool {
    props.shift_remove(name).is_some()
}

/// Remove value `target` from property `name` in `props` (in memory, no I/O).
///
/// Semantics:
/// - If the property is a list: remove `target` from the list; remove the key if list is empty.
/// - If the property is a scalar and matches `target` (case-insensitive): remove the key.
/// - If the property is a scalar and does not match: no-op.
/// - If the property is absent: no-op.
///
/// Returns `true` if a mutation occurred, `false` otherwise.
fn remove_value_in_memory(
    props: &mut indexmap::IndexMap<String, Value>,
    name: &str,
    target: &str,
) -> bool {
    // Check what kind of value we have without cloning.
    let is_sequence = matches!(props.get(name), Some(Value::Array(_)));

    if is_sequence {
        // Sequence arm: mutate in place, no clone needed.
        let Some(Value::Array(seq)) = props.get_mut(name) else {
            unreachable!()
        };
        let before = seq.len();
        seq.retain(|v| match v {
            Value::String(s) => !s.eq_ignore_ascii_case(target),
            Value::Number(n) => !n.to_string().eq_ignore_ascii_case(target),
            Value::Bool(b) => !b.to_string().eq_ignore_ascii_case(target),
            _ => true, // keep unrecognised element types
        });
        let after = seq.len();
        if after < before {
            if after == 0 {
                props.shift_remove(name);
            }
            return true;
        }
        return false;
    }

    // Scalar arms: clone only the scalar (cheap) to release the borrow on props.
    match props.get(name).cloned() {
        Some(Value::String(s)) => {
            if s.eq_ignore_ascii_case(target) {
                props.shift_remove(name);
                true
            } else {
                false
            }
        }
        Some(Value::Number(n)) => {
            if n.to_string().eq_ignore_ascii_case(target) {
                props.shift_remove(name);
                true
            } else {
                false
            }
        }
        Some(Value::Bool(b)) => {
            if b.to_string().eq_ignore_ascii_case(target) {
                props.shift_remove(name);
                true
            } else {
                false
            }
        }
        // None: property absent; Some(_): Null, Mapping, Tagged, Sequence — no-op
        None | Some(_) => false,
    }
}

/// Remove `tag` from the `tags` list in `props` (in memory, no I/O).
///
/// Returns `true` if the tag was present and removed.
fn remove_tag_in_memory(props: &mut indexmap::IndexMap<String, Value>, tag: &str) -> bool {
    remove_value_in_memory(props, "tags", tag)
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
#[allow(clippy::too_many_arguments)]
pub fn remove(
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
    dry_run: bool,
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

    // Allow omitting --file/--glob when --where-property or --where-tag is provided;
    // in that case, the command defaults to all vault files.
    let has_where = !where_property_filters.is_empty() || !where_tag_filters.is_empty();
    if !has_where && let Some(outcome) = require_file_or_glob(files, globs, "remove", format) {
        return Ok(outcome);
    }

    // Note: tag names are NOT validated for removal because the user may need
    // to remove malformed tags that were created with comma-joined values (e.g.
    // "cli,ux"). Validation only applies when adding tags.

    // Validate all property args before touching files
    for arg in property_args {
        match parse_kv_optional(arg) {
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

    // Pre-parse property args: (name, opt_value)
    let parsed_props: Vec<(&str, Option<&str>)> = property_args
        .iter()
        .map(|arg| {
            parse_kv_optional(arg).map_err(|e| anyhow::anyhow!("invalid property argument: {e}"))
        })
        .collect::<Result<Vec<_>>>()?;

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

        // Apply all --property mutations
        for (i, (name, opt_value)) in parsed_props.iter().enumerate() {
            let changed = match opt_value {
                None => remove_key_in_memory(&mut props, name),
                Some(target) => remove_value_in_memory(&mut props, name, target),
            };
            if changed {
                prop_results[i].0.push(rel_path.clone()); // modified
                file_changed = true;
            } else {
                prop_results[i].1.push(rel_path.clone()); // skipped
            }
        }

        // Apply all --tag mutations
        for (i, tag) in tag_args.iter().enumerate() {
            if remove_tag_in_memory(&mut props, tag) {
                tag_results[i].0.push(rel_path.clone()); // modified
                file_changed = true;
            } else {
                tag_results[i].1.push(rel_path.clone()); // skipped
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

    // Build property results
    for ((name, opt_value), (modified, skipped)) in
        parsed_props.iter().zip(prop_results.into_iter())
    {
        let total = modified.len() + skipped.len();
        let result = RemovePropertyResult {
            property: (*name).to_owned(),
            value: opt_value.map(str::to_owned),
            modified,
            skipped,
            total,
            scanned,
            dry_run,
        };
        results
            .push(serde_json::to_value(&result).expect("derived Serialize impl should not fail"));
    }

    // Build tag results
    for (tag, (modified, skipped)) in tag_args.iter().zip(tag_results.into_iter()) {
        let total = modified.len() + skipped.len();
        let result = RemoveTagResult {
            tag: tag.clone(),
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

    // --- parse_kv_optional ---

    #[test]
    fn parse_kv_optional_key_only() {
        assert_eq!(parse_kv_optional("status").unwrap(), ("status", None));
    }

    #[test]
    fn parse_kv_optional_key_value() {
        assert_eq!(
            parse_kv_optional("status=done").unwrap(),
            ("status", Some("done"))
        );
    }

    #[test]
    fn parse_kv_optional_value_with_equals() {
        assert_eq!(
            parse_kv_optional("url=http://x=y").unwrap(),
            ("url", Some("http://x=y"))
        );
    }

    #[test]
    fn parse_kv_optional_empty_key_returns_error() {
        let err = parse_kv_optional("=value").unwrap_err();
        assert!(
            err.contains("property name cannot be empty"),
            "unexpected error: {err}"
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &[],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn remove_requires_at_least_one_arg() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = remove(
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
            false,
        )
        .unwrap();
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains(body), "body was corrupted:\n{content}");
    }

    #[test]
    fn remove_multiple_properties_single_read_write() {
        // Remove two properties in one cycle — both should be gone.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: Note
status: draft
priority: low
---
"),
        )
        .unwrap();

        let outcome = remove(
            tmp.path(),
            &["status".to_owned(), "priority".to_owned()],
            &[],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(parsed.is_array());
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr[0]["modified"].as_array().unwrap().len(), 1);
        assert_eq!(arr[1]["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(!content.contains("status:"));
        assert!(!content.contains("priority:"));
        assert!(content.contains("title:"));
    }

    #[test]
    fn remove_where_property_filter_skips_nonmatching() {
        use hyalo_core::filter::parse_property_filter;
        // Only files matching --where-property are mutated.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("match.md"),
            "---\nstatus: draft\npriority: low\n---\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("no-match.md"),
            "---\nstatus: published\npriority: low\n---\n",
        )
        .unwrap();

        let filter = parse_property_filter("status=draft").unwrap();
        let outcome = remove(
            tmp.path(),
            &["priority".to_owned()],
            &[],
            &[],
            &["*.md".to_owned()],
            &[filter],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
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
        assert!(!match_content.contains("priority:"));
        let no_match_content = fs::read_to_string(tmp.path().join("no-match.md")).unwrap();
        assert!(no_match_content.contains("priority:"));
    }

    #[test]
    fn remove_where_tag_filter_skips_nonmatching() {
        // Only files with the required tag are mutated.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("tagged.md"),
            "---\ntags:\n  - deprecated\nstatus: old\n---\n",
        )
        .unwrap();
        fs::write(tmp.path().join("untagged.md"), "---\nstatus: old\n---\n").unwrap();

        let outcome = remove(
            tmp.path(),
            &["status".to_owned()],
            &[],
            &[],
            &["*.md".to_owned()],
            &[],
            &["deprecated".to_owned()],
            Format::Json,
            &mut None,
            None,
            false,
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
        assert!(!tagged_content.contains("status:"));
        let untagged_content = fs::read_to_string(tmp.path().join("untagged.md")).unwrap();
        assert!(untagged_content.contains("status:"));
    }

    // --- filter guard ---

    #[test]
    fn remove_rejects_gte_filter_in_property() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = remove(
            tmp.path(),
            &["priority>=3".to_owned()],
            &[],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
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
    fn remove_rejects_neq_filter_in_property() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = remove(
            tmp.path(),
            &["status!=draft".to_owned()],
            &[],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn remove_rejects_regex_filter_in_property() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = remove(
            tmp.path(),
            &["name~=pattern".to_owned()],
            &[],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn remove_tag_with_comma_succeeds() {
        // Malformed comma-joined tags should be removable without validation errors.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
tags:
  - cli,ux
  - rust
---
"),
        )
        .unwrap();

        let outcome = remove(
            tmp.path(),
            &[],
            &["cli,ux".to_owned()],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success, got: {outcome:?}")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["tag"], "cli,ux");
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(!content.contains("cli,ux"));
        assert!(content.contains("rust"));
    }
}
