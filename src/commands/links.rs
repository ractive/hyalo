use anyhow::Result;
use serde_json::json;
use std::path::Path;

use crate::discovery::{self, FileResolveError};
use crate::graph::FileIndex;
use crate::links::{self, LinkStyle};
use crate::output::{CommandOutcome, Format};

/// List outgoing links from a file or all files.
pub fn links(dir: &Path, path: Option<&str>, format: Format) -> Result<CommandOutcome> {
    match path {
        Some(p) => links_single(dir, p, format),
        None => links_all(dir, format),
    }
}

/// List unresolved links from a file or all files.
pub fn unresolved(dir: &Path, path: Option<&str>, format: Format) -> Result<CommandOutcome> {
    match path {
        Some(p) => unresolved_single(dir, p, format),
        None => unresolved_all(dir, format),
    }
}

fn links_single(dir: &Path, path_arg: &str, format: Format) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match discovery::resolve_file(dir, path_arg) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format)),
    };

    let extracted = links::extract_links_from_file(&full_path)?;

    let link_values: Vec<serde_json::Value> = extracted.iter().map(link_to_json).collect();
    let result = json!({
        "path": rel_path,
        "links": link_values,
    });

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
    )))
}

fn links_all(dir: &Path, format: Format) -> Result<CommandOutcome> {
    let files = discovery::discover_files(dir)?;
    let mut results = Vec::new();

    for file in &files {
        let rel = discovery::relative_path(dir, file);
        let extracted = links::extract_links_from_file(file)?;

        if !extracted.is_empty() {
            let link_values: Vec<serde_json::Value> = extracted.iter().map(link_to_json).collect();
            results.push(json!({
                "path": rel,
                "links": link_values,
            }));
        }
    }

    Ok(CommandOutcome::Success(crate::output::format_success(
        format,
        &json!(results),
    )))
}

fn unresolved_single(dir: &Path, path_arg: &str, format: Format) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match discovery::resolve_file(dir, path_arg) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format)),
    };

    let index = FileIndex::build(dir)?;
    let extracted = links::extract_links_from_file(&full_path)?;

    let unresolved_links: Vec<serde_json::Value> = extracted
        .iter()
        .filter(|link| index.resolve_target(&link.target).is_none())
        .map(link_to_json)
        .collect();

    let result = json!({
        "path": rel_path,
        "links": unresolved_links,
    });

    Ok(CommandOutcome::Success(crate::output::format_success(
        format, &result,
    )))
}

fn unresolved_all(dir: &Path, format: Format) -> Result<CommandOutcome> {
    let files = discovery::discover_files(dir)?;
    let index = FileIndex::build(dir)?;
    let mut results = Vec::new();

    for file in &files {
        let rel = discovery::relative_path(dir, file);
        let extracted = links::extract_links_from_file(file)?;

        let unresolved_links: Vec<serde_json::Value> = extracted
            .iter()
            .filter(|link| index.resolve_target(&link.target).is_none())
            .map(link_to_json)
            .collect();

        if !unresolved_links.is_empty() {
            results.push(json!({
                "path": rel,
                "links": unresolved_links,
            }));
        }
    }

    Ok(CommandOutcome::Success(crate::output::format_success(
        format,
        &json!(results),
    )))
}

fn link_to_json(link: &links::Link) -> serde_json::Value {
    let mut obj = json!({
        "target": link.target,
        "style": match link.style {
            LinkStyle::Wiki => "wiki",
            LinkStyle::Markdown => "markdown",
        },
        "line": link.line,
    });

    if let Some(ref display) = link.display {
        obj["display"] = json!(display);
    }
    if let Some(ref heading) = link.heading {
        obj["heading"] = json!(heading);
    }
    if let Some(ref block_ref) = link.block_ref {
        obj["block_ref"] = json!(block_ref);
    }
    if link.is_embed {
        obj["is_embed"] = json!(true);
    }

    obj
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_vault() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note-a.md"),
            "---\ntitle: A\n---\nSee [[note-b]] and [[nonexistent]]\n",
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
        let out = unwrap_success(links(tmp.path(), Some("note-a.md"), Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["path"], "note-a.md");
        let links_arr = parsed["links"].as_array().unwrap();
        assert_eq!(links_arr.len(), 2);
        assert_eq!(links_arr[0]["target"], "note-b");
        assert_eq!(links_arr[0]["style"], "wiki");
    }

    #[test]
    fn links_all_files() {
        let tmp = setup_vault();
        let out = unwrap_success(links(tmp.path(), None, Format::Json).unwrap());
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        // note-a has 2 links, note-b has 2 links, isolated has 0
        assert_eq!(parsed.len(), 2); // only files with links
    }

    #[test]
    fn links_markdown_style() {
        let tmp = setup_vault();
        let out = unwrap_success(links(tmp.path(), Some("note-b.md"), Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let links_arr = parsed["links"].as_array().unwrap();
        let md_link = links_arr.iter().find(|l| l["style"] == "markdown").unwrap();
        assert_eq!(md_link["target"], "note-a.md");
        assert_eq!(md_link["display"], "A");
    }

    #[test]
    fn unresolved_single_file() {
        let tmp = setup_vault();
        let out = unwrap_success(unresolved(tmp.path(), Some("note-a.md"), Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let links_arr = parsed["links"].as_array().unwrap();
        assert_eq!(links_arr.len(), 1);
        assert_eq!(links_arr[0]["target"], "nonexistent");
    }

    #[test]
    fn unresolved_all_files() {
        let tmp = setup_vault();
        let out = unwrap_success(unresolved(tmp.path(), None, Format::Json).unwrap());
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        // Only note-a has unresolved links
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["path"], "note-a.md");
    }

    #[test]
    fn links_file_not_found() {
        let tmp = setup_vault();
        let result = links(tmp.path(), Some("nope.md"), Format::Json).unwrap();
        match result {
            CommandOutcome::UserError(s) => {
                let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
                assert_eq!(parsed["error"], "file not found");
            }
            _ => panic!("expected user error"),
        }
    }

    #[test]
    fn links_with_heading_metadata() {
        let tmp = setup_vault();
        let out = unwrap_success(links(tmp.path(), Some("note-b.md"), Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let links_arr = parsed["links"].as_array().unwrap();
        let wiki_link = links_arr.iter().find(|l| l["style"] == "wiki").unwrap();
        assert_eq!(wiki_link["heading"], "heading");
    }
}
