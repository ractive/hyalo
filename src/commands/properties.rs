#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use std::path::Path;

use crate::commands::{FilesOrOutcome, collect_files};
use crate::frontmatter;
use crate::output::{CommandOutcome, Format, format_output};
use crate::types::PropertySummaryEntry;

/// Aggregate summary: unique property names with types and file counts.
/// Scope is filtered by `--file` / `--glob` (or all files if both are None).
pub fn properties_summary(
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

    // Aggregate: name -> (type, count)
    let mut agg: std::collections::BTreeMap<String, (String, usize)> =
        std::collections::BTreeMap::new();

    for (fp, rel) in &files {
        let props = match frontmatter::read_frontmatter(fp) {
            Ok(p) => p,
            Err(e) if frontmatter::is_parse_error(&e) => {
                eprintln!("warning: skipping {rel}: {e}");
                continue;
            }
            Err(e) => return Err(e),
        };
        for (key, value) in &props {
            agg.entry(key.clone())
                .and_modify(|entry| entry.1 += 1)
                .or_insert_with(|| (frontmatter::infer_type(value).to_owned(), 1));
        }
    }

    let result: Vec<PropertySummaryEntry> = agg
        .into_iter()
        .map(|(name, (prop_type, count))| PropertySummaryEntry {
            name,
            prop_type,
            count,
        })
        .collect();

    Ok(CommandOutcome::Success(format_output(format, &result)))
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

    fn setup_dir() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: Test
status: draft
priority: 3
tags:
  - rust
  - cli
---
# Hello
"),
        )
        .unwrap();
        fs::write(tmp.path().join("empty.md"), "No frontmatter here.\n").unwrap();
        tmp
    }

    /// Extract the output string from a `CommandOutcome`.
    fn unwrap_output(outcome: CommandOutcome) -> (String, bool) {
        match outcome {
            CommandOutcome::Success(s) => (s, true),
            CommandOutcome::UserError(s) => (s, false),
        }
    }

    #[test]
    fn properties_summary_aggregates() {
        let tmp = setup_dir();
        let (out, ok) =
            unwrap_output(properties_summary(tmp.path(), None, None, Format::Json).unwrap());
        assert!(ok);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        assert!(!parsed.is_empty());
        let names: Vec<&str> = parsed.iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"title"));
        assert!(names.contains(&"status"));
    }

    #[test]
    fn properties_summary_skips_malformed_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        // Valid file with a known property.
        fs::write(
            tmp.path().join("good.md"),
            md!(r"
---
title: Good Note
---
# Hello
"),
        )
        .unwrap();
        // Malformed YAML: a bare colon key is rejected by serde_yaml_ng.
        fs::write(
            tmp.path().join("bad.md"),
            "---\n: invalid yaml [[[{\n---\n# Bad\n",
        )
        .unwrap();

        let outcome = properties_summary(tmp.path(), None, None, Format::Json).unwrap();
        let (out, ok) = unwrap_output(outcome);
        assert!(ok, "expected Success, got UserError: {out}");

        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        let names: Vec<&str> = parsed.iter().map(|v| v["name"].as_str().unwrap()).collect();
        // The valid file's property must appear.
        assert!(names.contains(&"title"), "missing 'title' in {names:?}");
    }
}
