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
    /// `skipped.len()`, restated as a scalar (iter-216 D-1) — the whole
    /// mutation family exposes this key so one query answers "how many were
    /// skipped" regardless of which command produced the result.
    pub(crate) skipped_count: usize,
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
    journal: &mut crate::commands::journal::MutationJournal<'_>,
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

    let files_arg = files;
    let files = collect_files(dir, files, globs, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };
    let scanned = files.len();

    // Per-property result accumulators: (modified, skipped)
    let mut prop_results: Vec<(Vec<String>, Vec<String>)> =
        vec![(Vec::new(), Vec::new()); parsed_args.len()];

    // --- Dotted-key collision pre-pass (iter-219 M5): reject the whole batch
    //     before any file is modified, not mid-loop (a mid-loop `return` left
    //     earlier files in the batch already written, and skipped the
    //     end-of-loop journal flush entirely).
    for (full_path, rel_path) in &files {
        let props = match frontmatter::read_frontmatter(full_path) {
            Ok(p) => p,
            Err(e) if frontmatter::is_parse_error(&e) => continue,
            Err(e) => return Err(e),
        };
        if !filter::matches_frontmatter_filters(&props, where_property_filters, where_tag_filters) {
            continue;
        }
        for (name, _, _) in &parsed_args {
            if let Some(outcome) =
                super::reject_dotted_property_collision(name, &props, rel_path, format)
            {
                return Ok(outcome);
            }
        }
    }

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
            let doc_type = merged
                .get("type")
                .and_then(hyalo_core::schema::normalize_type_value);
            let doc_type = doc_type.as_deref();
            // Explicit `type:` wins; otherwise a `[schema.bind]` path binding
            // supplies the effective type for validation-on-write.
            let effective_type = doc_type.or_else(|| schema.bound_type_for(rel_path));
            let effective_schema = match effective_type {
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

    // L-2: relative paths skipped because their frontmatter would not parse.
    let mut skipped_unparseable: Vec<String> = Vec::new();
    // BUG-35 (iter-276): the YAML diagnostic for a single named file rides
    // in the error envelope's `cause` instead of a bare stderr line.
    let mut unparseable_cause: Option<String> = None;

    // Outer loop: one read-modify-write per file
    for (full_path, rel_path) in &files {
        let mtime = frontmatter::read_mtime(full_path)?;
        let mut props = match frontmatter::read_frontmatter(full_path) {
            Ok(p) => p,
            Err(e) if frontmatter::is_parse_error(&e) => {
                if let Some(detail) =
                    super::report_unparseable_skip(files_arg, globs, rel_path, &e)
                {
                    unparseable_cause = Some(detail);
                }
                skipped_unparseable.push(rel_path.clone());
                continue;
            }
            Err(e) => return Err(e),
        };

        // BUG-2 (iter-255): the command has just `stat`ed and read this file
        // while holding the snapshot index open, so a file that changed on
        // disk since the last `create-index` gets its entry repaired here —
        // whether or not the mutation below turns out to be a no-op. Without
        // it, a `set` that finds the property already at its target value
        // reports `0 modified` and leaves `find --index` describing a body
        // that is no longer on disk. Costs no extra I/O: the staleness check
        // reuses the `mtime`/size fingerprint read above.
        if !dry_run {
            journal.refresh_if_stale(rel_path, full_path, mtime)?;
        }

        // Apply --where-* filters: skip files that don't match
        if !filter::matches_frontmatter_filters(&props, where_property_filters, where_tag_filters) {
            continue;
        }

        let mut file_changed = false;

        // The dotted-key collision guard already ran as a whole-batch
        // pre-pass above, so no per-file check (and no mid-loop `return`)
        // is needed here.
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
            match frontmatter::write_frontmatter_within(dir, full_path, &props) {
                Ok(()) => {}
                Err(ref e) if frontmatter::as_budget_error(e).is_some() => {
                    let budget_err = frontmatter::as_budget_error(e).unwrap();
                    let out = crate::output::format_budget_error(format, budget_err);
                    return Ok(CommandOutcome::UserError(out));
                }
                Err(e) => return Err(e),
            }
            journal.update_entry(rel_path, props, full_path)?;
        }
    }

    // L-2: the single file the user named by hand was unparseable — report it
    // as an error rather than a 0-modified success.
    if let Some(outcome) =
        super::single_named_file_unparseable(
            files_arg,
            globs,
            &skipped_unparseable,
            unparseable_cause.as_deref(),
            format,
        )
    {
        return Ok(outcome);
    }

    if !dry_run {
        journal.flush()?;
    }

    let mut results: Vec<serde_json::Value> = Vec::new();

    for ((name, raw_value, _), (modified, skipped)) in parsed_args.iter().zip(prop_results) {
        let total = modified.len() + skipped.len();
        let skipped_count = skipped.len();
        let result = AppendPropertyResult {
            property: (*name).to_owned(),
            value: (*raw_value).to_owned(),
            modified,
            skipped,
            skipped_count,
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
#[allow(clippy::items_after_test_module)] // dispatch handler appended below (ARCH-1, iter-225)
mod tests {
    use super::*;
    use crate::commands::journal::MutationJournal;
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
            exempt: hyalo_core::schema::ExemptGlobs::default(),
            bind: hyalo_core::schema::SchemaBind::default(),
            types: {
                let mut m = HashMap::new();
                m.insert(
                    "post".to_owned(),
                    TypeSchema {
                        required: vec![],
                        properties: type_props,
                        ..Default::default()
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
            &mut MutationJournal::new(&mut None, None),
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
            PropertyConstraint::String {
                pattern: None,
                min_length: None,
                max_length: None,
            },
        );
        let schema = SchemaConfig {
            default: TypeSchema::default(),
            exempt: hyalo_core::schema::ExemptGlobs::default(),
            bind: hyalo_core::schema::SchemaBind::default(),
            types: {
                let mut m = HashMap::new();
                m.insert(
                    "post".to_owned(),
                    TypeSchema {
                        required: vec![],
                        properties: type_props,
                        ..Default::default()
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
            &mut MutationJournal::new(&mut None, None),
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

    // ---------------------------------------------------------------------------
    // Dotted-key collision guard (iter-219 NEW-16b)
    // ---------------------------------------------------------------------------

    #[test]
    fn append_rejects_dotted_property_colliding_with_existing_map() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
versions:
  fpt: '*'
---
"),
        )
        .unwrap();

        let outcome = append(
            tmp.path(),
            &["versions.fpt=X".to_owned()],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
        )
        .unwrap();
        match outcome {
            CommandOutcome::UserError(msg) => {
                assert!(msg.contains("versions"), "msg: {msg}");
            }
            other => panic!("expected UserError, got: {other:?}"),
        }
        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(!content.contains("versions.fpt"), "content:\n{content}");
    }
}

// ---------------------------------------------------------------------------
// Dispatch handler (ARCH-1, iter-225)
// ---------------------------------------------------------------------------

/// The `hyalo append` dispatch arm, extracted verbatim from `dispatch.rs`.
/// `files_from` and `index_flags` were consumed earlier in `run.rs`
/// (snapshot loading) and never reach here.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::needless_pass_by_value)] // args moved verbatim from the clap variant
#[allow(clippy::items_after_statements)] // extracted handler keeps its mid-fn imports (ARCH-1, iter-225)
pub(crate) fn run(
    ctx: &mut crate::dispatch::CommandContext<'_>,
    file_positional: Vec<String>,
    properties: Vec<String>,
    mut file: Vec<String>,
    glob: Vec<String>,
    where_properties: Vec<String>,
    where_tags: Vec<String>,
    dry_run: bool,
    validate: bool,
) -> Result<CommandOutcome> {
    let dir = ctx.dir;
    let effective_format = ctx.effective_format;
    let mut journal =
        crate::commands::journal::MutationJournal::new(&mut *ctx.snapshot_index, ctx.index_path);
    use crate::dispatch::parse_where_filters;

    if !file_positional.is_empty() {
        file = file_positional;
    }
    let where_prop_filters = match parse_where_filters(&where_properties, &where_tags) {
        Ok(f) => f,
        Err(e) => {
            return Ok(CommandOutcome::UserError(crate::output::format_error(
                effective_format,
                &e,
                None,
                None,
                None,
            )));
        }
    };
    let do_validate = validate || ctx.validate_on_write;
    // DEC-290: same refusal as `set` — validating against a schema that failed
    // to load rejects nothing, so the promise is kept by refusing instead.
    if let Some(outcome) = crate::commands::reject_write_with_unloadable_schema(
        do_validate,
        ctx.schema_invalid,
        effective_format,
    ) {
        return Ok(outcome);
    }
    append(
        dir,
        &properties,
        &file,
        &glob,
        &where_prop_filters,
        &where_tags,
        effective_format,
        &mut journal,
        dry_run,
        do_validate,
        if do_validate { Some(ctx.schema) } else { None },
    )
}
