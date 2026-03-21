use anyhow::Result;
use serde_json::json;
use std::path::Path;

use crate::commands::resolve_error_to_outcome;
use crate::discovery;
use crate::links;
use crate::output::{CommandOutcome, Format};

/// Filter for link resolution status.
#[derive(Clone, Copy)]
pub enum LinkFilter {
    /// Return all links.
    All,
    /// Return only links that resolve to a file.
    Resolved,
    /// Return only links that don't resolve to any file.
    Unresolved,
}

/// List outgoing links from a single file, optionally filtered by resolution status.
pub fn links(dir: &Path, file: &str, filter: LinkFilter, format: Format) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match discovery::resolve_file(dir, file) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format)),
    };

    let extracted = links::extract_links_from_file(&full_path)?;

    let link_values: Vec<serde_json::Value> = extracted
        .iter()
        .map(|link| {
            let resolved = discovery::resolve_target(dir, &link.target);
            (link, resolved)
        })
        .filter(|(_, resolved)| match filter {
            LinkFilter::All => true,
            LinkFilter::Resolved => resolved.is_some(),
            LinkFilter::Unresolved => resolved.is_none(),
        })
        .map(|(link, resolved)| {
            json!({
                "target": &link.target,
                "path": resolved,
                "label": &link.label,
            })
        })
        .collect();
    let result = json!({
        "path": rel_path,
        "links": link_values,
    });

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
    }

    fn setup_vault() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note-a.md"),
            md!(r"
---
title: A
---
See [[note-b]] and [[nonexistent]]
"),
        )
        .unwrap();
        fs::write(
            tmp.path().join("note-b.md"),
            "Link to [A](note-a.md) and [[note-a#heading]]\n",
        )
        .unwrap();
        fs::write(tmp.path().join("isolated.md"), "No links here.\n").unwrap();
        tmp
    }

    fn unwrap_success(outcome: CommandOutcome) -> String {
        match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("expected success, got user error: {s}"),
        }
    }

    #[test]
    fn links_single_file() {
        let tmp = setup_vault();
        let out =
            unwrap_success(links(tmp.path(), "note-a.md", LinkFilter::All, Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["path"], "note-a.md");
        let links_arr = parsed["links"].as_array().unwrap();
        assert_eq!(links_arr.len(), 2);
        assert_eq!(links_arr[0]["target"], "note-b");
        assert_eq!(links_arr[0]["path"], "note-b.md");
    }

    #[test]
    fn links_path_populated() {
        let tmp = setup_vault();
        let out =
            unwrap_success(links(tmp.path(), "note-b.md", LinkFilter::All, Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let links_arr = parsed["links"].as_array().unwrap();
        // Both links resolve to note-a.md
        for link in links_arr {
            assert_eq!(link["path"], "note-a.md");
        }
    }

    #[test]
    fn links_label_field() {
        let tmp = setup_vault();
        let out =
            unwrap_success(links(tmp.path(), "note-b.md", LinkFilter::All, Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let links_arr = parsed["links"].as_array().unwrap();
        let md_link = links_arr
            .iter()
            .find(|l| l["label"].as_str() == Some("A"))
            .unwrap();
        assert_eq!(md_link["target"], "note-a.md");
    }

    #[test]
    fn unresolved_single_file() {
        let tmp = setup_vault();
        let out = unwrap_success(
            links(
                tmp.path(),
                "note-a.md",
                LinkFilter::Unresolved,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let links_arr = parsed["links"].as_array().unwrap();
        assert_eq!(links_arr.len(), 1);
        assert_eq!(links_arr[0]["target"], "nonexistent");
        assert!(links_arr[0]["path"].is_null());
    }

    #[test]
    fn resolved_single_file() {
        let tmp = setup_vault();
        let out = unwrap_success(
            links(tmp.path(), "note-a.md", LinkFilter::Resolved, Format::Json).unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let links_arr = parsed["links"].as_array().unwrap();
        assert_eq!(links_arr.len(), 1);
        assert_eq!(links_arr[0]["target"], "note-b");
        assert_eq!(links_arr[0]["path"], "note-b.md");
    }

    #[test]
    fn links_file_not_found() {
        let tmp = setup_vault();
        let result = links(tmp.path(), "nope.md", LinkFilter::All, Format::Json).unwrap();
        match result {
            CommandOutcome::UserError(s) => {
                let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
                assert_eq!(parsed["error"], "file not found");
            }
            _ => panic!("expected user error"),
        }
    }

    #[test]
    fn links_empty_file() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("empty.md"), "").unwrap();
        let out =
            unwrap_success(links(tmp.path(), "empty.md", LinkFilter::All, Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["path"], "empty.md");
        let links_arr = parsed["links"].as_array().unwrap();
        assert!(links_arr.is_empty());
    }
}
