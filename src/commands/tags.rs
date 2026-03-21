#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use serde_json::json;
use serde_yaml_ng::Value;
use std::collections::BTreeMap;
use std::path::Path;

use crate::commands::properties::{
    ListOpResult, add_values_to_list_property, remove_values_from_list_property,
};
use crate::commands::{
    FilesOrOutcome, build_find_json, build_list_mutation_json, collect_files, require_file_or_glob,
};
use crate::frontmatter;
use crate::output::{CommandOutcome, Format};

// ---------------------------------------------------------------------------
// Tag format validation
// ---------------------------------------------------------------------------

/// Validate an Obsidian-compatible tag name.
/// Rules:
/// - Only letters, digits, underscores (`_`), hyphens (`-`), forward slashes (`/`)
/// - Must contain at least one non-numeric character
/// - Must not be empty
/// - Forward slashes are allowed for hierarchy (e.g. `inbox/processing`)
pub fn validate_tag(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("tag name must not be empty".to_owned());
    }

    for ch in name.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-' && ch != '/' {
            return Err(format!(
                "invalid character '{ch}' in tag name; allowed: letters, digits, _, -, /"
            ));
        }
    }

    // Must contain at least one non-digit character
    if name.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "tag '{name}' is all numeric; tags must contain at least one non-numeric character (e.g. 'y{name}')"
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Nested tag matching
// ---------------------------------------------------------------------------

/// Returns true if `tag` matches the query under Obsidian's nested tag rules.
/// A tag matches if it equals the query or starts with `query/` (case-insensitive).
///
/// Uses byte-level ASCII comparison — safe because tag names are validated to only
/// contain ASCII characters (letters, digits, `_`, `-`, `/`).
#[must_use]
pub fn tag_matches(tag: &str, query: &str) -> bool {
    tag.eq_ignore_ascii_case(query)
        || (tag.len() > query.len()
            && tag.as_bytes()[query.len()] == b'/'
            && tag[..query.len()].eq_ignore_ascii_case(query))
}

// ---------------------------------------------------------------------------
// Tag extraction
// ---------------------------------------------------------------------------

/// Extract the `tags` list from a parsed frontmatter map.
/// Handles:
/// - Missing `tags` key → empty vec
/// - `tags` as a YAML sequence → collect string items
/// - `tags` as a scalar string → single-element vec
/// - `tags` as empty sequence → empty vec
#[must_use]
pub fn extract_tags(props: &BTreeMap<String, Value>) -> Vec<String> {
    match props.get("tags") {
        Some(Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .collect(),
        Some(Value::String(s)) => {
            if s.is_empty() {
                vec![]
            } else {
                vec![s.clone()]
            }
        }
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// `hyalo tags summary` — aggregate: unique tags with counts
// ---------------------------------------------------------------------------

/// Aggregate summary: unique tag names with file counts.
/// Scope is filtered by `--file` / `--glob` (or all files if both are None).
pub fn tags_summary(
    dir: &Path,
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    // Aggregate case-insensitively: use lowercase key, preserve first-seen casing for display
    let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();

    for (full_path, _) in &files {
        let props = frontmatter::read_frontmatter(full_path)?;
        for tag in extract_tags(&props) {
            let key = tag.to_ascii_lowercase();
            counts
                .entry(key)
                .and_modify(|entry| entry.1 += 1)
                .or_insert((tag, 1));
        }
    }

    let tags_json: Vec<serde_json::Value> = counts
        .into_iter()
        .map(|(_, (name, count))| json!({"name": name, "count": count}))
        .collect();

    let total = tags_json.len();
    let result = json!({"tags": tags_json, "total": total});

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
    )))
}

// ---------------------------------------------------------------------------
// `hyalo tags list` — per-file detail: each file with its tags array
// ---------------------------------------------------------------------------

/// Per-file detail: each file with its tags array.
/// Scope is filtered by `--file` / `--glob` (or all files if both are None).
pub fn tags_list(
    dir: &Path,
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let mut results = Vec::new();
    for (full_path, rel_path) in &files {
        let props = frontmatter::read_frontmatter(full_path)?;
        let tags = extract_tags(&props);
        let tags_json: Vec<serde_json::Value> =
            tags.into_iter().map(serde_json::Value::String).collect();
        results.push(json!({
            "path": rel_path,
            "tags": tags_json,
        }));
    }

    let json_output = crate::commands::unwrap_single_file_result(file, results);

    Ok(CommandOutcome::Success(crate::output::format_success(
        format,
        &json_output,
    )))
}

// ---------------------------------------------------------------------------
// `hyalo tag find` — find files containing a specific tag
// ---------------------------------------------------------------------------

pub fn tag_find(
    dir: &Path,
    name: &str,
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let mut matching_paths: Vec<String> = Vec::new();

    for (full_path, rel_path) in &files {
        let props = frontmatter::read_frontmatter(full_path)?;
        let tags = extract_tags(&props);
        if tags.iter().any(|t| tag_matches(t, name)) {
            matching_paths.push(rel_path.clone());
        }
    }

    let result = build_find_json("tag", name, None, &matching_paths);

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
    )))
}

// ---------------------------------------------------------------------------
// `hyalo tag add` — add a tag to file(s)
// ---------------------------------------------------------------------------

pub fn tag_add(
    dir: &Path,
    name: &str,
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    // Validate tag name first
    if let Err(msg) = validate_tag(name) {
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

    // Mutation commands require --file or --glob to avoid accidentally touching every file
    if let Some(outcome) = require_file_or_glob(file, glob, "tag add", format) {
        return Ok(outcome);
    }

    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let ListOpResult { modified, skipped } =
        add_values_to_list_property(&files, "tags", &[name.to_owned()])?;

    let result = build_list_mutation_json("tag", name, None, None, &modified, &skipped);

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
    )))
}

// ---------------------------------------------------------------------------
// `hyalo tag remove` — remove a tag from file(s)
// ---------------------------------------------------------------------------

pub fn tag_remove(
    dir: &Path,
    name: &str,
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    // Mutation commands require --file or --glob to avoid accidentally touching every file
    if let Some(outcome) = require_file_or_glob(file, glob, "tag remove", format) {
        return Ok(outcome);
    }

    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let ListOpResult { modified, skipped } =
        remove_values_from_list_property(&files, "tags", &[name.to_owned()])?;

    let result = build_list_mutation_json("tag", name, None, None, &modified, &skipped);

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
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

    // --- Tag validation ---

    #[test]
    fn valid_tag_simple() {
        assert!(validate_tag("inbox").is_ok());
        assert!(validate_tag("my-tag").is_ok());
        assert!(validate_tag("my_tag").is_ok());
        assert!(validate_tag("MyTag").is_ok());
        assert!(validate_tag("tag123").is_ok());
        assert!(validate_tag("y1984").is_ok());
    }

    #[test]
    fn valid_tag_nested() {
        assert!(validate_tag("inbox/processing").is_ok());
        assert!(validate_tag("project/hyalo/iteration").is_ok());
    }

    #[test]
    fn invalid_tag_empty() {
        assert!(validate_tag("").is_err());
    }

    #[test]
    fn invalid_tag_numeric_only() {
        let err = validate_tag("1984").unwrap_err();
        assert!(err.contains("non-numeric"), "got: {err}");
    }

    #[test]
    fn invalid_tag_with_space() {
        let err = validate_tag("my tag").unwrap_err();
        assert!(err.contains("invalid character"), "got: {err}");
    }

    #[test]
    fn invalid_tag_special_chars() {
        assert!(validate_tag("tag!").is_err());
        assert!(validate_tag("tag@name").is_err());
        assert!(validate_tag("#tag").is_err());
    }

    // --- Nested tag matching ---

    #[test]
    fn tag_matches_exact() {
        assert!(tag_matches("inbox", "inbox"));
    }

    #[test]
    fn tag_matches_child() {
        assert!(tag_matches("inbox/processing", "inbox"));
        assert!(tag_matches("inbox/to-read", "inbox"));
    }

    #[test]
    fn tag_no_match_prefix_without_slash() {
        assert!(!tag_matches("inboxes", "inbox"));
        assert!(!tag_matches("my-inbox", "inbox"));
    }

    #[test]
    fn tag_matches_case_insensitive() {
        assert!(tag_matches("Inbox", "inbox"));
        assert!(tag_matches("INBOX/PROCESSING", "inbox"));
        assert!(tag_matches("inbox", "INBOX"));
    }

    #[test]
    fn tag_no_match_different_tag() {
        assert!(!tag_matches("project", "inbox"));
    }

    // --- Tag extraction ---

    fn make_props(yaml: &str) -> BTreeMap<String, Value> {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    #[test]
    fn extract_tags_from_list() {
        let props = make_props(md!(r"
tags:
  - rust
  - cli
"));
        let tags = extract_tags(&props);
        assert_eq!(tags, vec!["rust", "cli"]);
    }

    #[test]
    fn extract_tags_from_scalar_string() {
        let props = make_props("tags: rust\n");
        let tags = extract_tags(&props);
        assert_eq!(tags, vec!["rust"]);
    }

    #[test]
    fn extract_tags_missing_key() {
        let props = make_props("title: Note\n");
        let tags = extract_tags(&props);
        assert!(tags.is_empty());
    }

    #[test]
    fn extract_tags_empty_list() {
        let props = make_props("tags: []\n");
        let tags = extract_tags(&props);
        assert!(tags.is_empty());
    }

    #[test]
    fn extract_tags_null() {
        let props = make_props("tags: ~\n");
        let tags = extract_tags(&props);
        assert!(tags.is_empty());
    }

    // --- tags_list command ---

    fn setup_vault() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("a.md"),
            md!(r"
---
tags:
  - rust
  - cli
---
# A
"),
        )
        .unwrap();
        fs::write(
            tmp.path().join("b.md"),
            md!(r"
---
tags:
  - rust
  - iteration
---
# B
"),
        )
        .unwrap();
        fs::write(tmp.path().join("c.md"), "No frontmatter.\n").unwrap();
        tmp
    }

    #[test]
    fn tags_summary_all_files() {
        let tmp = setup_vault();
        let outcome = tags_summary(tmp.path(), None, None, Format::Json).unwrap();
        let out = match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let tags = parsed["tags"].as_array().unwrap();
        assert_eq!(parsed["total"], 3); // rust, cli, iteration
        let rust = tags.iter().find(|t| t["name"] == "rust").unwrap();
        assert_eq!(rust["count"], 2);
    }

    #[test]
    fn tags_summary_single_file() {
        let tmp = setup_vault();
        let outcome = tags_summary(tmp.path(), Some("a.md"), None, Format::Json).unwrap();
        let out = match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["total"], 2);
    }

    // --- tags_list (per-file) command ---

    #[test]
    fn tags_list_per_file_all_files() {
        let tmp = setup_vault();
        let outcome = tags_list(tmp.path(), None, None, Format::Json).unwrap();
        let out = match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let entries = parsed.as_array().unwrap();
        // 3 files: a.md, b.md, c.md
        assert_eq!(entries.len(), 3);
        let a = entries
            .iter()
            .find(|e| e["path"].as_str().unwrap().ends_with("a.md"))
            .unwrap();
        let a_tags = a["tags"].as_array().unwrap();
        assert_eq!(a_tags.len(), 2);
    }

    #[test]
    fn tags_list_single_file() {
        let tmp = setup_vault();
        let outcome = tags_list(tmp.path(), Some("a.md"), None, Format::Json).unwrap();
        let out = match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(parsed["path"].as_str().unwrap().ends_with("a.md"));
        let tags = parsed["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 2);
    }

    // --- tag_find command ---

    #[test]
    fn tag_find_exact_match() {
        let tmp = setup_vault();
        let outcome = tag_find(tmp.path(), "cli", None, None, Format::Json).unwrap();
        let out = match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["total"], 1);
        assert!(parsed["files"][0].as_str().unwrap().contains("a.md"));
    }

    #[test]
    fn tag_find_nested_match() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
tags:
  - inbox/processing
---
"),
        )
        .unwrap();
        let outcome = tag_find(tmp.path(), "inbox", None, None, Format::Json).unwrap();
        let out = match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["total"], 1);
    }

    #[test]
    fn tag_find_no_match() {
        let tmp = setup_vault();
        let outcome = tag_find(tmp.path(), "nonexistent", None, None, Format::Json).unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["total"], 0);
    }

    // --- tag_add command ---

    #[test]
    fn tag_add_creates_new_property() {
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

        let outcome = tag_add(tmp.path(), "rust", Some("note.md"), None, Format::Json).unwrap();
        match outcome {
            CommandOutcome::Success(s) => {
                let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
                assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);
                assert_eq!(parsed["skipped"].as_array().unwrap().len(), 0);
            }
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        }

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("rust"));
    }

    #[test]
    fn tag_add_idempotent() {
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

        let outcome = tag_add(tmp.path(), "rust", Some("note.md"), None, Format::Json).unwrap();
        match outcome {
            CommandOutcome::Success(s) => {
                let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
                assert_eq!(parsed["modified"].as_array().unwrap().len(), 0);
                assert_eq!(parsed["skipped"].as_array().unwrap().len(), 1);
            }
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        }
    }

    #[test]
    fn tag_add_invalid_name_rejected() {
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

        let outcome = tag_add(tmp.path(), "1984", Some("note.md"), None, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn tag_add_requires_file_or_glob() {
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

        // Neither --file nor --glob → user error
        let outcome = tag_add(tmp.path(), "rust", None, None, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    // --- tag_remove command ---

    #[test]
    fn tag_remove_existing() {
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

        let outcome = tag_remove(tmp.path(), "rust", Some("note.md"), None, Format::Json).unwrap();
        match outcome {
            CommandOutcome::Success(s) => {
                let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
                assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);
                assert_eq!(parsed["skipped"].as_array().unwrap().len(), 0);
            }
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        }

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(!content.contains("rust"));
        assert!(content.contains("cli"));
    }

    #[test]
    fn tag_remove_idempotent() {
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

        let outcome = tag_remove(tmp.path(), "rust", Some("note.md"), None, Format::Json).unwrap();
        match outcome {
            CommandOutcome::Success(s) => {
                let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
                assert_eq!(parsed["modified"].as_array().unwrap().len(), 0);
                assert_eq!(parsed["skipped"].as_array().unwrap().len(), 1);
            }
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        }
    }

    #[test]
    fn tag_remove_empties_tags_property() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: Note
tags:
  - rust
---
"),
        )
        .unwrap();

        tag_remove(tmp.path(), "rust", Some("note.md"), None, Format::Json).unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        // tags property should be removed entirely
        assert!(!content.contains("tags:"));
        // title should still be present
        assert!(content.contains("title:"));
    }

    #[test]
    fn tag_remove_requires_file_or_glob() {
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

        // Neither --file nor --glob → user error
        let outcome = tag_remove(tmp.path(), "rust", None, None, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    // --- body preservation ---

    #[test]
    fn tag_add_preserves_body() {
        let tmp = tempfile::tempdir().unwrap();
        let body = md!(r"
# Heading

Some content with [[wikilinks]] and more text.
");
        fs::write(
            tmp.path().join("note.md"),
            format!("---\ntitle: Note\n---\n{body}"),
        )
        .unwrap();

        tag_add(tmp.path(), "rust", Some("note.md"), None, Format::Json).unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains(body), "body was corrupted:\n{content}");
    }

    #[test]
    fn tag_remove_preserves_body() {
        let tmp = tempfile::tempdir().unwrap();
        let body = md!(r"
# Heading

Some content.
");
        fs::write(
            tmp.path().join("note.md"),
            format!("---\ntags:\n  - rust\n  - cli\n---\n{body}"),
        )
        .unwrap();

        tag_remove(tmp.path(), "rust", Some("note.md"), None, Format::Json).unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains(body), "body was corrupted:\n{content}");
    }

    // --- discover_files used by tags_summary (read-only) still works without file/glob ---

    #[test]
    fn tags_summary_no_file_or_glob_reads_all() {
        let tmp = setup_vault();
        // tags_summary (read-only) still accepts no --file/--glob
        let outcome = tags_summary(tmp.path(), None, None, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::Success(_)));
    }
}
