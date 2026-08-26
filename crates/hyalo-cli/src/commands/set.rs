#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

use crate::commands::{FilesOrOutcome, collect_files, mutation, require_file_or_glob};
use crate::output::{CommandOutcome, Format};
use hyalo_core::filter::{self, PropertyFilter};
use hyalo_core::frontmatter;
use hyalo_core::schema::SchemaConfig;

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Result of a `set --property K=V` operation across files.
#[derive(Debug, Serialize)]
pub(crate) struct SetPropertyResult {
    pub(crate) property: String,
    /// The coerced value that was written to the frontmatter, not the raw input
    /// string. For a list assignment like `x=[a, b]` this echoes the parsed YAML
    /// list `["a", "b"]` rather than the literal `"[a, b]"` the user typed
    /// (iter-181 task 3).
    pub(crate) value: Value,
    pub(crate) modified: Vec<String>,
    pub(crate) skipped: Vec<String>,
    /// `skipped.len()`, restated as a scalar.
    ///
    /// iter-216 D-1: `properties rename` / `tags rename` report the skip set
    /// as a count only (their skip set is "every scanned file that lacks the
    /// property", which on a large vault is the whole vault and carries no
    /// information). Emitting `skipped_count` here too gives the whole
    /// mutation family one key that answers "how many were skipped".
    pub(crate) skipped_count: usize,
    pub(crate) total: usize,
    pub(crate) scanned: usize,
    pub(crate) dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
}

/// Result of a `set --tag T` operation across files.
#[derive(Debug, Serialize)]
pub(crate) struct SetTagResult {
    pub(crate) tag: String,
    pub(crate) modified: Vec<String>,
    pub(crate) skipped: Vec<String>,
    /// `skipped.len()`, restated as a scalar — see [`SetPropertyResult::skipped_count`].
    pub(crate) skipped_count: usize,
    pub(crate) total: usize,
    pub(crate) scanned: usize,
    pub(crate) dry_run: bool,
}

// ---------------------------------------------------------------------------
// Date-typed property validation (BUG-B)
// ---------------------------------------------------------------------------

/// Property names that are treated as date-typed.
///
/// When a value is set for one of these properties and it does not parse as
/// a valid ISO 8601 date (`YYYY-MM-DD`), a `note:` is emitted to inform the
/// user that the value will sort lexicographically rather than chronologically.
const DATE_TYPED_PROPERTIES: &[&str] = &["date", "created", "modified", "updated"];

/// Returns `true` when `name` is a known date-typed property key.
pub(crate) fn is_date_typed_property(name: &str) -> bool {
    DATE_TYPED_PROPERTIES
        .iter()
        .any(|k| k.eq_ignore_ascii_case(name))
}

/// Returns `true` when `value` is exactly a YYYY-MM-DD ISO 8601 date.
///
/// Delegates to `hyalo_core::is_iso8601_date` so `set` and the
/// HYALO003 lint rule agree on what counts as a date.
pub(crate) fn looks_like_date(value: &str) -> bool {
    hyalo_core::is_iso8601_date(value)
}

/// Returns `true` when `value` has the `YYYY-MM-DD` structural shape
/// (exactly 10 chars, digits in the right positions, dashes as separators)
/// but without validating the calendar (month/day range).
///
/// Used to distinguish "looks like a date but is invalid" from "not a date
/// at all" so we can emit a hard error in the former case and a soft note
/// in the latter.
fn has_date_shape(value: &str) -> bool {
    if value.len() != 10 {
        return false;
    }
    let b = value.as_bytes();
    b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit)
}

// ---------------------------------------------------------------------------
// Advisory notes (non-blocking; lint remains the enforcement gate)
// ---------------------------------------------------------------------------

/// Compute an advisory `note` for a `set --property K=V` result, or `None`.
///
/// Two independent advisories may fire (date takes precedence when both apply):
///
///  1. A date-typed property (`date`, `created`, …) receiving a non-date value —
///     it will sort lexicographically rather than chronologically (BUG-B).
///  2. A value that violates the property's enum/pattern constraint in the
///     effective schema (iter-181 task 1). The write still proceeds — this only
///     surfaces the same guidance a `--validate` run or a later `lint` would give.
///
/// The schema constraint is resolved against the type being written in the same
/// batch (a `--property type=X` assignment) or the default schema when no type
/// is set. Per-file `type:` overrides are not probed here — advisories are
/// best-effort hints, not a substitute for `lint`.
///
/// A third, independent advisory (iter-219 NEW-16c) fires when `previous_value`
/// (the property's value in the first mutated file that already had it set,
/// before this write) was a string and the newly inferred value is a number
/// or boolean — the CLI arg is always a bare string, so type inference has no
/// way to know the property was deliberately quoted to stay text.
fn advisory_note(
    name: &str,
    raw_value: &str,
    parsed_value: &Value,
    schema: Option<&SchemaConfig>,
    batch_type: Option<&str>,
    previous_value: Option<&Value>,
) -> Option<String> {
    // (1) Date-typed lexicographic-sort advisory.
    if is_date_typed_property(name) && !raw_value.is_empty() && !looks_like_date(raw_value) {
        return Some(format!(
            "value {raw_value:?} is not a valid ISO 8601 date (YYYY-MM-DD); \
             the property will sort lexicographically rather than chronologically"
        ));
    }

    // (2) Enum/pattern schema-constraint advisory. Resolve the effective schema
    // for the type being written (an explicit `type=X` in the batch, or the
    // file's own declared type threaded in as `batch_type`), falling back to the
    // default schema. `merged_schema_for_type` returns an owned `TypeSchema`, so
    // bind it to a local before borrowing the constraint out of it.
    if let Some(schema) = schema {
        let owned;
        let effective = match batch_type {
            Some(t) => {
                owned = schema.merged_schema_for_type(t);
                &owned
            }
            None => schema.default_schema(),
        };
        if let Some(constraint) = effective.properties.get(name)
            && let Some(violation) =
                crate::commands::lint::validate_constraint_simple(name, parsed_value, constraint)
        {
            return Some(format!(
                "{violation}; write proceeds — run `hyalo lint` to enforce, or `set --validate` to reject"
            ));
        }
    }

    // (3) Type-inference retype advisory.
    if let Some(Value::String(_)) = previous_value {
        let new_kind = match parsed_value {
            Value::Number(_) => Some("a number"),
            Value::Bool(_) => Some("a boolean"),
            _ => None,
        };
        if let Some(new_kind) = new_kind {
            return Some(format!(
                "value {raw_value:?} is now inferred as {new_kind}, but at least one matched \
                 file previously stored this property as a string; pass a schema type to keep \
                 it a string (other matched files may differ)"
            ));
        }
    }

    None
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
/// - `validate`: when `true`, validates new property values against the schema
///   before writing; rejects violations with a `UserError`.
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
    journal: &mut crate::commands::journal::MutationJournal<'_>,
    dry_run: bool,
    validate: bool,
    schema: Option<&SchemaConfig>,
    case_insensitive_mode: hyalo_core::CaseInsensitiveMode,
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

    // Mutation commands require --file or --glob, UNLESS --where-property or --where-tag
    // is provided — in that case, default to all vault files and apply the filters.
    let has_where = !where_property_filters.is_empty() || !where_tag_filters.is_empty();
    if !has_where && let Some(outcome) = require_file_or_glob(files, globs, "set", format) {
        return Ok(outcome);
    }

    // Validate all K=V args before touching files
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

    // Validate tag names
    for tag in tag_args {
        if let Err(msg) = crate::commands::tags::validate_tag(tag) {
            let out = crate::output::format_error(
                format,
                &msg,
                None,
                Some(
                    "tag names may contain Unicode letters and digits, _, -, /, and emoji, and must have at least one non-numeric character",
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
            let (name, raw_value) =
                parse_kv(arg).map_err(|e| anyhow::anyhow!("invalid property argument: {e}"))?;
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
            // Reject values that look like YYYY-MM-DD but fail calendar
            // validation on date-typed properties (BUG-2 / iter-133).
            if is_date_typed_property(name)
                && has_date_shape(raw_value)
                && !looks_like_date(raw_value)
            {
                let out = crate::output::format_error(
                    format,
                    &format!(
                        "value {raw_value:?} is not a valid ISO 8601 date \
                         (YYYY-MM-DD) for property '{name}'"
                    ),
                    None,
                    Some("check month (01–12) and day ranges for the given month"),
                    None,
                );
                return Ok(CommandOutcome::UserError(out));
            }
            v.push((name, raw_value, value));
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
        vec![(Vec::new(), Vec::new()); parsed_props.len()];
    // Per-tag result accumulators: (modified, skipped)
    let mut tag_results: Vec<(Vec<String>, Vec<String>)> =
        vec![(Vec::new(), Vec::new()); tag_args.len()];

    // --- Dotted-key collision pre-pass (iter-219 M5): reject the whole batch
    //     before any file is modified, not mid-loop. A mid-loop `return` (the
    //     original iter-219 shape) left files already written in an earlier
    //     iteration on disk, and skipped the end-of-loop
    //     `journal flush` entirely — a partial write plus a stale
    //     on-disk index for whichever file happened to trip the guard.
    for (full_path, rel_path) in &files {
        let props = match frontmatter::read_frontmatter(full_path) {
            Ok(p) => p,
            // Parse errors are reported as warnings during the write loop; skip here.
            Err(e) if frontmatter::is_parse_error(&e) => continue,
            Err(e) => return Err(e),
        };
        if !filter::matches_frontmatter_filters(&props, where_property_filters, where_tag_filters) {
            continue;
        }
        for (name, _, _) in &parsed_props {
            if let Some(outcome) =
                super::reject_dotted_property_collision(name, &props, rel_path, format)
            {
                return Ok(outcome);
            }
        }
    }

    // --- Pre-validation pass (BUG-D): validate all proposed writes before any file
    //     is modified. This keeps batch mutations atomic — if any file would fail
    //     validation, no files are written. The schema is chosen from the merged
    //     `type` property (post-mutation), so `--property type=X` selects X's schema.
    if validate && let Some(schema) = schema {
        // Resolved once for the whole batch (not re-probed per file) so
        // `[schema] exempt` globs fold case the same way `hyalo okf index`
        // treats `INDEX.md` on case-insensitive filesystems (macOS/Windows
        // default).
        let case_insensitive = hyalo_core::mode_enabled(case_insensitive_mode, dir);
        for (full_path, rel_path) in &files {
            // Reserved / exempt files (e.g. OKF `index.md`, `log.md`) are not
            // subject to schema validation.
            if schema.exempt.is_exempt_ci(rel_path, case_insensitive) {
                continue;
            }
            let props = match frontmatter::read_frontmatter(full_path) {
                Ok(p) => p,
                // Parse errors are reported as warnings during the write loop; skip here.
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
            // Apply set mutations in-memory to compute the post-mutation props.
            let mut merged = props.clone();
            for (name, _, value) in &parsed_props {
                merged.insert((*name).to_owned(), value.clone());
            }
            let doc_type = merged.get("type").and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.as_str()),
                _ => None,
            });
            // Explicit `type:` wins; otherwise a `[schema.bind]` path binding
            // supplies the effective type for validation-on-write.
            let effective_type = doc_type.or_else(|| schema.bound_type_for(rel_path));
            let effective_schema = match effective_type {
                Some(t) => schema.merged_schema_for_type(t),
                None => schema.default_schema().clone(),
            };
            for (name, raw_value, _) in &parsed_props {
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

    // Effective type for schema-constraint advisories (iter-181 task 1). Prefer
    // an explicit `type=X` set in this batch; otherwise fall back to the `type:`
    // of the first mutated file (a `set` batch is virtually always homogeneous).
    let batch_type_from_args: Option<String> = parsed_props.iter().find_map(|(name, _, value)| {
        if name.eq_ignore_ascii_case("type") {
            value.as_str().map(str::to_owned)
        } else {
            None
        }
    });
    let mut batch_type_from_file: Option<String> = None;

    // First observed pre-mutation value for each property (iter-219 NEW-16c),
    // used by the retype advisory below. Captured from the first file where
    // the property already existed, mirroring how `batch_type_from_file`
    // samples a single representative file rather than every file in the batch.
    let mut first_old_value: Vec<Option<Value>> = vec![None; parsed_props.len()];

    // L-2: relative paths skipped because their frontmatter would not parse.
    let mut skipped_unparseable: Vec<String> = Vec::new();

    // Outer loop: one read-modify-write per file
    for (full_path, rel_path) in &files {
        let mtime = frontmatter::read_mtime(full_path)?;
        let mut props = match frontmatter::read_frontmatter(full_path) {
            Ok(p) => p,
            Err(e) if frontmatter::is_parse_error(&e) => {
                super::report_unparseable_skip(files_arg, globs, rel_path, &e);
                skipped_unparseable.push(rel_path.clone());
                continue;
            }
            Err(e) => return Err(e),
        };

        // Apply --where-* filters: skip files that don't match
        if !filter::matches_frontmatter_filters(&props, where_property_filters, where_tag_filters) {
            continue;
        }

        // Record the first mutated file's declared type for the advisory pass.
        if batch_type_from_file.is_none()
            && let Some(Value::String(t)) = props.get("type")
        {
            batch_type_from_file = Some(t.clone());
        }

        let mut file_changed = false;

        // Apply all --property mutations. The dotted-key collision guard
        // already ran as a whole-batch pre-pass above, so no per-file check
        // (and no mid-loop `return`) is needed here.
        for (i, (name, _, value)) in parsed_props.iter().enumerate() {
            if first_old_value[i].is_none() {
                first_old_value[i] = props.get(*name).cloned();
            }
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
        super::single_named_file_unparseable(files_arg, globs, &skipped_unparseable, format)
    {
        return Ok(outcome);
    }

    if !dry_run {
        journal.flush()?;
    }

    let mut results: Vec<serde_json::Value> = Vec::new();

    let batch_type = batch_type_from_args
        .as_deref()
        .or(batch_type_from_file.as_deref());

    for (i, ((name, raw_value, parsed_value), (modified, skipped))) in
        parsed_props.iter().zip(prop_results).enumerate()
    {
        let total = modified.len() + skipped.len();
        // Advisory note (write still proceeds; lint remains the enforcement gate):
        //   1. BUG-B: a date-typed property receiving a non-date value.
        //   2. iter-181 task 1: an enum/pattern value the schema would reject.
        //   3. iter-219 NEW-16c: type inference silently retyping a string.
        let note = advisory_note(
            name,
            raw_value,
            parsed_value,
            schema,
            batch_type,
            first_old_value[i].as_ref(),
        );
        let skipped_count = skipped.len();
        let result = SetPropertyResult {
            property: (*name).to_owned(),
            value: parsed_value.clone(),
            modified,
            skipped,
            skipped_count,
            total,
            scanned,
            dry_run,
            note,
        };
        results
            .push(serde_json::to_value(&result).expect("derived Serialize impl should not fail"));
    }

    for (tag, (modified, skipped)) in tag_args.iter().zip(tag_results) {
        let total = modified.len() + skipped.len();
        let skipped_count = skipped.len();
        let result = SetTagResult {
            tag: tag.clone(),
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

    // Return array if multiple mutations, single object if one
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        let out = match outcome {
            CommandOutcome::Success { output: s, .. } | CommandOutcome::RawOutput(s) => s,
            CommandOutcome::RawBytes(b) => String::from_utf8_lossy(&b).into_owned(),
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
        use hyalo_core::filter::parse_property_filter;
        // Files that don't match --where-property are not mutated.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("match.md"), "---\nstatus: draft\n---\n").unwrap();
        fs::write(
            tmp.path().join("no-match.md"),
            "---\nstatus: published\n---\n",
        )
        .unwrap();

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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
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
            &mut MutationJournal::new(&mut None, None),
            false,
            false,
            None,
            hyalo_core::CaseInsensitiveMode::Off,
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
        assert!(tagged_content.contains("status: reviewed"));
        let untagged_content = fs::read_to_string(tmp.path().join("untagged.md")).unwrap();
        assert!(!untagged_content.contains("status"));
    }

    // --- filter guard ---

    #[test]
    fn set_rejects_gte_filter_in_property() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = set(
            tmp.path(),
            &["priority>=3".to_owned()],
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
            hyalo_core::CaseInsensitiveMode::Off,
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
    fn set_rejects_lte_filter_in_property() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = set(
            tmp.path(),
            &["priority<=3".to_owned()],
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
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn set_rejects_neq_filter_in_property() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = set(
            tmp.path(),
            &["status!=draft".to_owned()],
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
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn set_rejects_regex_filter_in_property() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: x\n---\n").unwrap();
        let outcome = set(
            tmp.path(),
            &["name~=pattern".to_owned()],
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
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    // ---------------------------------------------------------------------------
    // BUG-D: --validate rejects values violating schema constraints
    // ---------------------------------------------------------------------------

    #[test]
    fn set_validate_rejects_invalid_enum_value() {
        use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};
        use std::collections::HashMap;

        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: My Note
type: post
---
"),
        )
        .unwrap();

        // Schema: post.status must be one of [draft, published]
        let mut type_props = HashMap::new();
        type_props.insert(
            "status".to_owned(),
            PropertyConstraint::Enum {
                values: vec!["draft".to_owned(), "published".to_owned()],
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

        let outcome = set(
            tmp.path(),
            &["status=archived".to_owned()], // not in enum
            &[],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut MutationJournal::new(&mut None, None),
            false,
            true, // validate = true
            Some(&schema),
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        assert!(
            matches!(outcome, CommandOutcome::UserError(_)),
            "expected UserError for invalid enum value"
        );
    }

    #[test]
    fn set_validate_accepts_valid_enum_value() {
        use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};
        use std::collections::HashMap;

        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: My Note
type: post
---
"),
        )
        .unwrap();

        let mut type_props = HashMap::new();
        type_props.insert(
            "status".to_owned(),
            PropertyConstraint::Enum {
                values: vec!["draft".to_owned(), "published".to_owned()],
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

        let outcome = set(
            tmp.path(),
            &["status=published".to_owned()], // valid
            &[],
            &["note.md".to_owned()],
            &[],
            &[],
            &[],
            Format::Json,
            &mut MutationJournal::new(&mut None, None),
            false,
            true, // validate = true
            Some(&schema),
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        assert!(
            matches!(outcome, CommandOutcome::Success { .. }),
            "expected success for valid enum value"
        );
    }

    // ---------------------------------------------------------------------------
    // Dotted-key collision guard (iter-219 NEW-16b)
    // ---------------------------------------------------------------------------

    #[test]
    fn set_rejects_dotted_property_colliding_with_existing_map() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
versions:
  fpt: '*'
  ghec: '*'
---
"),
        )
        .unwrap();

        let outcome = set(
            tmp.path(),
            &["versions.fpt=X".to_owned()],
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
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        match outcome {
            CommandOutcome::UserError(msg) => {
                assert!(msg.contains("versions"), "msg: {msg}");
                assert!(msg.contains("mapping"), "msg: {msg}");
            }
            other => panic!("expected UserError, got: {other:?}"),
        }

        // The file must be untouched — the command aborted before writing.
        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(!content.contains("versions.fpt"), "content:\n{content}");
    }

    #[test]
    fn set_allows_dotted_property_with_no_colliding_map() {
        // No `versions` map exists, so the dotted key is just an unusual but
        // literal top-level key name — allowed (nested-path support itself
        // is out of scope, only the collision is guarded against).
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: Note\n---\n").unwrap();

        let outcome = set(
            tmp.path(),
            &["versions.fpt=X".to_owned()],
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
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::Success { .. }));
        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("versions.fpt"), "content:\n{content}");
    }

    // ---------------------------------------------------------------------------
    // Type-inference retype advisory (iter-219 NEW-16c)
    // ---------------------------------------------------------------------------

    #[test]
    fn set_notes_when_a_previously_string_value_is_retyped_as_a_number() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ncode: '42'\n---\n").unwrap();

        let outcome = set(
            tmp.path(),
            &["code=42".to_owned()],
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
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let note = parsed["note"].as_str().unwrap_or_default();
        assert!(
            note.contains("previously stored this property as a string"),
            "note: {note:?}"
        );
        assert!(note.contains("number"), "note: {note:?}");
    }

    #[test]
    fn set_no_retype_advisory_when_property_is_new() {
        // The property didn't exist before, so there is nothing to "retype" —
        // this is ordinary type inference, not a surprise.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: Note\n---\n").unwrap();

        let outcome = set(
            tmp.path(),
            &["priority=3".to_owned()],
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
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(parsed.get("note").is_none() || parsed["note"].is_null());
    }
}

// ---------------------------------------------------------------------------
// Dispatch handler (ARCH-1, iter-225)
// ---------------------------------------------------------------------------

/// The `hyalo set` dispatch arm, extracted verbatim from `dispatch.rs`.
/// `files_from` and `index_flags` were consumed earlier in `run.rs`
/// (snapshot loading) and never reach here.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::needless_pass_by_value)] // args moved verbatim from the clap variant
#[allow(clippy::items_after_statements)] // extracted handler keeps its mid-fn imports (ARCH-1, iter-225)
pub(crate) fn run(
    ctx: &mut crate::dispatch::CommandContext<'_>,
    file_positional: Vec<String>,
    properties: Vec<String>,
    tag: Vec<String>,
    mut file: Vec<String>,
    glob: Vec<String>,
    iteration: Option<String>,
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
    // iter-235: `--iteration <ID>` resolves the file to mutate from the
    // type schema's filename_template, then must select exactly one
    // file (set is a single-target mutation — ambiguous matches are an
    // error, unlike find which returns all). Resolved here, before the
    // generic `set` call, so set sees a normal file list and `--where-*`
    // still filters within it.
    let mut resolved_files: Option<Vec<String>> = None;
    if let Some(id_str) = iteration {
        match hyalo_core::iteration_id::parse_iteration_id(&id_str) {
            Ok(id) => {
                match crate::commands::iteration::resolve_iteration_globs(
                    ctx.schema,
                    &id,
                    effective_format,
                ) {
                    crate::commands::iteration::IterationGlobs::Globs(g) => {
                        match crate::commands::collect_files(dir, &[], &g, effective_format)? {
                            crate::commands::FilesOrOutcome::Files(pairs) => {
                                let paths: Vec<String> =
                                    pairs.into_iter().map(|(_, rel)| rel).collect();
                                match paths.len() {
                                    0 => {
                                        return Ok(CommandOutcome::UserError(
                                            crate::output::format_error(
                                                effective_format,
                                                &format!(
                                                    "no file found for iteration {id} \
                                                         (resolved globs: {})",
                                                    g.join(", ")
                                                ),
                                                Some(&id_str),
                                                Some(
                                                    "check the iteration number, or list candidates with `hyalo find --iteration <ID>`",
                                                ),
                                                None,
                                            ),
                                        ));
                                    }
                                    1 => {
                                        resolved_files = Some(paths);
                                    }
                                    _ => {
                                        let mut listed = paths.clone();
                                        listed.sort();
                                        return Ok(CommandOutcome::UserError(
                                            crate::output::format_error(
                                                effective_format,
                                                &format!(
                                                    "iteration {id} matches multiple files — \
                                                         pass a letter suffix to disambiguate, \
                                                         or use --file/--glob to target one directly"
                                                ),
                                                Some(&id_str),
                                                Some(&format!(
                                                    "candidates:\n{}",
                                                    listed
                                                        .iter()
                                                        .map(|p| format!("  - {p}"))
                                                        .collect::<Vec<_>>()
                                                        .join("\n")
                                                )),
                                                None,
                                            ),
                                        ));
                                    }
                                }
                            }
                            crate::commands::FilesOrOutcome::Outcome(o) => return Ok(o),
                        }
                    }
                    crate::commands::iteration::IterationGlobs::Outcome(o) => {
                        return Ok(o);
                    }
                }
            }
            Err(e) => {
                return Ok(CommandOutcome::UserError(crate::output::format_error(
                    effective_format,
                    &e.to_string(),
                    Some(&id_str),
                    Some(
                        "pass a bare integer (206), zero-padded integer (01), or integer + letter suffix (16b)",
                    ),
                    None,
                )));
            }
        }
    }
    let (set_files, set_globs): (&[String], &[String]) = match resolved_files {
        Some(ref paths) => (paths.as_slice(), &[]),
        None => (&file, &glob),
    };
    set(
        dir,
        &properties,
        &tag,
        set_files,
        set_globs,
        &where_prop_filters,
        &where_tags,
        effective_format,
        &mut journal,
        dry_run,
        do_validate,
        // Always pass the schema: `do_validate` still gates the blocking
        // pre-validation pass, but the (non-blocking) enum/pattern
        // advisory note needs the schema even without --validate
        // (iter-181 task 1).
        Some(ctx.schema),
        ctx.case_insensitive_mode,
    )
}
