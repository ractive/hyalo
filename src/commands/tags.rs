use anyhow::{Context, Result};
use serde_json::json;
use serde_yaml_ng::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::discovery::{self, FileResolveError};
use crate::frontmatter::{self, Document};
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
        if !ch.is_alphanumeric() && ch != '_' && ch != '-' && ch != '/' {
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
pub fn tag_matches(tag: &str, query: &str) -> bool {
    let tag_lower = tag.to_lowercase();
    let query_lower = query.to_lowercase();
    tag_lower == query_lower || tag_lower.starts_with(&format!("{query_lower}/"))
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
pub fn extract_tags(props: &BTreeMap<String, Value>) -> Vec<String> {
    match props.get("tags") {
        None => vec![],
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
        Some(Value::Null) => vec![],
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// `hyalo tags` — list all unique tags with counts
// ---------------------------------------------------------------------------

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

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();

    for (full_path, _) in &files {
        let props = frontmatter::read_frontmatter(full_path)?;
        for tag in extract_tags(&props) {
            *counts.entry(tag).or_insert(0) += 1;
        }
    }

    let tags_json: Vec<serde_json::Value> = counts
        .into_iter()
        .map(|(name, count)| json!({"name": name, "count": count}))
        .collect();

    let total = tags_json.len();
    let result = json!({"tags": tags_json, "total": total});

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
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

    let total = matching_paths.len();
    let result = json!({"tag": name, "files": matching_paths, "total": total});

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

    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let mut modified: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    for (full_path, rel_path) in &files {
        let content = fs::read_to_string(full_path)
            .with_context(|| format!("failed to read {}", full_path.display()))?;
        let mut doc = Document::parse(&content)?;

        let tags = extract_tags(doc.properties());
        let already_has = tags.iter().any(|t| t.to_lowercase() == name.to_lowercase());

        if already_has {
            skipped.push(rel_path.clone());
        } else {
            // Build updated tags list
            let mut new_tags = tags;
            new_tags.push(name.to_owned());
            let yaml_list = Value::Sequence(new_tags.into_iter().map(Value::String).collect());
            doc.set_property("tags".to_owned(), yaml_list);

            let serialized = doc.serialize()?;
            fs::write(full_path, serialized)
                .with_context(|| format!("failed to write {}", full_path.display()))?;

            modified.push(rel_path.clone());
        }
    }

    let total = modified.len() + skipped.len();
    let result = json!({
        "tag": name,
        "modified": modified,
        "skipped": skipped,
        "total": total,
    });

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
    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let mut modified: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    for (full_path, rel_path) in &files {
        let content = fs::read_to_string(full_path)
            .with_context(|| format!("failed to read {}", full_path.display()))?;
        let mut doc = Document::parse(&content)?;

        let tags = extract_tags(doc.properties());
        let name_lower = name.to_lowercase();
        let new_tags: Vec<String> = tags
            .iter()
            .filter(|t| t.to_lowercase() != name_lower)
            .cloned()
            .collect();

        if new_tags.len() == tags.len() {
            // Tag was not present — idempotent skip
            skipped.push(rel_path.clone());
        } else {
            if new_tags.is_empty() {
                doc.remove_property("tags");
            } else {
                let yaml_list = Value::Sequence(new_tags.into_iter().map(Value::String).collect());
                doc.set_property("tags".to_owned(), yaml_list);
            }

            let serialized = doc.serialize()?;
            fs::write(full_path, serialized)
                .with_context(|| format!("failed to write {}", full_path.display()))?;

            modified.push(rel_path.clone());
        }
    }

    let total = modified.len() + skipped.len();
    let result = json!({
        "tag": name,
        "modified": modified,
        "skipped": skipped,
        "total": total,
    });

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
    )))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

enum FilesOrOutcome {
    Files(Vec<(std::path::PathBuf, String)>),
    Outcome(CommandOutcome),
}

/// Resolve the set of files to operate on based on --file / --glob / all.
/// Returns an error outcome for user errors (file not found, no glob matches).
fn collect_files(
    dir: &Path,
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<FilesOrOutcome> {
    match (file, glob) {
        (Some(f), None) => {
            let resolved = match discovery::resolve_file(dir, f) {
                Ok(r) => r,
                Err(e) => return Ok(FilesOrOutcome::Outcome(resolve_error_to_outcome(e, format))),
            };
            Ok(FilesOrOutcome::Files(vec![resolved]))
        }
        (None, Some(pattern)) => {
            let all = discovery::discover_files(dir)?;
            let matched = discovery::match_glob(dir, &all, pattern)?;
            if matched.is_empty() {
                let out = crate::output::format_error(
                    format,
                    "no files match pattern",
                    Some(pattern),
                    None,
                    None,
                );
                return Ok(FilesOrOutcome::Outcome(CommandOutcome::UserError(out)));
            }
            Ok(FilesOrOutcome::Files(matched))
        }
        (None, None) => {
            // Operate on all .md files
            let all = discovery::discover_files(dir)?;
            let with_rel: Vec<(std::path::PathBuf, String)> = all
                .into_iter()
                .map(|p| {
                    let rel = discovery::relative_path(dir, &p);
                    (p, rel)
                })
                .collect();
            Ok(FilesOrOutcome::Files(with_rel))
        }
        (Some(_), Some(_)) => {
            // Clap enforces mutual exclusivity; this branch is unreachable in practice
            let out = crate::output::format_error(
                format,
                "--file and --glob are mutually exclusive",
                None,
                None,
                None,
            );
            Ok(FilesOrOutcome::Outcome(CommandOutcome::UserError(out)))
        }
    }
}

fn resolve_error_to_outcome(err: FileResolveError, format: Format) -> CommandOutcome {
    match err {
        FileResolveError::MissingExtension { path, hint } => {
            CommandOutcome::UserError(crate::output::format_error(
                format,
                "file not found",
                Some(&path),
                Some(&format!("did you mean {hint}?")),
                None,
            ))
        }
        FileResolveError::NotFound { path } => CommandOutcome::UserError(
            crate::output::format_error(format, "file not found", Some(&path), None, None),
        ),
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
        let props = make_props("tags:\n  - rust\n  - cli\n");
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
            "---\ntags:\n  - rust\n  - cli\n---\n# A\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("b.md"),
            "---\ntags:\n  - rust\n  - iteration\n---\n# B\n",
        )
        .unwrap();
        fs::write(tmp.path().join("c.md"), "No frontmatter.\n").unwrap();
        tmp
    }

    #[test]
    fn tags_list_all_files() {
        let tmp = setup_vault();
        let outcome = tags_list(tmp.path(), None, None, Format::Json).unwrap();
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
    fn tags_list_single_file() {
        let tmp = setup_vault();
        let outcome = tags_list(tmp.path(), Some("a.md"), None, Format::Json).unwrap();
        let out = match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["total"], 2);
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
            "---\ntags:\n  - inbox/processing\n---\n",
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
        let out = match outcome {
            CommandOutcome::Success(s) => s,
            _ => panic!("expected success"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["total"], 0);
    }

    // --- tag_add command ---

    #[test]
    fn tag_add_creates_new_property() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "---\ntitle: Note\n---\n").unwrap();

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
        fs::write(tmp.path().join("note.md"), "---\ntags:\n  - rust\n---\n").unwrap();

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
        fs::write(tmp.path().join("note.md"), "---\ntitle: Note\n---\n").unwrap();

        let outcome = tag_add(tmp.path(), "1984", Some("note.md"), None, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    // --- tag_remove command ---

    #[test]
    fn tag_remove_existing() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            "---\ntags:\n  - rust\n  - cli\n---\n",
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
        fs::write(tmp.path().join("note.md"), "---\ntags:\n  - cli\n---\n").unwrap();

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
            "---\ntitle: Note\ntags:\n  - rust\n---\n",
        )
        .unwrap();

        tag_remove(tmp.path(), "rust", Some("note.md"), None, Format::Json).unwrap();

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        // tags property should be removed entirely
        assert!(!content.contains("tags:"));
        // title should still be present
        assert!(content.contains("title:"));
    }
}
