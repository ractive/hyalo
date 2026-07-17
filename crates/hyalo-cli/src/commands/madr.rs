#![allow(clippy::missing_errors_doc)]
//! `hyalo madr toc` — Markdown Architecture Decision Record artifact generator.
//!
//! A deterministic, LLM-free generator that maintains an ADR table of contents /
//! status dashboard (parity with `adr generate toc`). It scans the ADR directory
//! (default `docs/decisions/`, resolvable via `--dir`), reads each ADR's number,
//! title, status and date from frontmatter/filename, and renders a Markdown
//! table inside a managed region of `<adr-dir>/README.md` so hand-written prose
//! around it is preserved.
//!
//! Like `okf index`, it defaults to `--dry-run` and mutates only with `--apply`;
//! in dry-run it exits non-zero when the on-disk TOC differs from the generated
//! output, so it doubles as a CI drift check.

use anyhow::{Context, Result};
use std::fmt::Write as _;
use std::path::Path;

use crate::commands::managed_region::{GeneratePlan, Markers, apply_plan, read_old_content};
use crate::output::{CommandOutcome, Format, format_error};

/// Managed-region marker prefix for the ADR TOC.
const TOC_PREFIX: &str = "madr:toc";

/// Default ADR directory (vault-relative) when `--dir` is not given.
const DEFAULT_ADR_DIR: &str = "docs/decisions";

/// One ADR row in the generated TOC.
struct AdrEntry {
    /// The `NNNN` number parsed from the filename (for sorting/display).
    number: Option<u32>,
    /// Display title: frontmatter `title` else the first `# ` heading else stem.
    title: String,
    /// Link target, relative to the TOC file's directory (forward slashes).
    link: String,
    /// The `status` frontmatter value, if any.
    status: Option<String>,
    /// The `date` frontmatter value, if any.
    date: Option<String>,
}

/// Generate/refresh the ADR table of contents.
///
/// `adr_dir` optionally overrides the ADR directory (vault-relative); defaults
/// to `docs/decisions`. `apply` writes the change; otherwise it is a dry run
/// that returns exit code 1 when the TOC would change (the CI drift signal).
pub fn run_toc(
    dir: &Path,
    adr_dir: Option<&str>,
    apply: bool,
    format: Format,
) -> Result<(CommandOutcome, Option<i32>)> {
    let adr_rel_normalized = adr_dir.unwrap_or(DEFAULT_ADR_DIR).replace('\\', "/");
    let adr_rel = adr_rel_normalized.trim_end_matches('/');
    let adr_full = dir.join(adr_rel);

    if !adr_full.is_dir() {
        return Ok((
            CommandOutcome::UserError(format_error(
                format,
                &format!("ADR directory '{adr_rel}' not found"),
                Some(adr_rel),
                Some("pass --dir <path> or create the directory first"),
                None,
            )),
            None,
        ));
    }

    let entries = collect_adrs(&adr_full, adr_rel)?;
    let body = render_toc_body(&entries);

    let toc_rel = format!("{adr_rel}/README.md");
    let old_content = read_old_content(dir, &toc_rel)?;
    let new_content =
        Markers::new(TOC_PREFIX).splice(&old_content, &body, "# Architecture Decision Records");

    let plan = GeneratePlan {
        rel_path: toc_rel,
        new_content,
        old_content,
    };

    let changed = plan.changed();
    if apply && changed {
        apply_plan(dir, &plan)?;
    }

    let payload = serde_json::json!({
        "command": "madr toc",
        "apply": apply,
        "adr_dir": adr_rel,
        "adrs": entries.len(),
        "changed": changed,
        "file": plan.rel_path,
        "action": if changed { if plan.is_new() { "create" } else { "update" } } else { "unchanged" },
        "hint": "hyalo lint --profile madr  # validate ADR conformance",
    });

    let exit_override = if !apply && changed { Some(1) } else { None };

    Ok((
        CommandOutcome::success_with_total(payload.to_string(), u64::from(changed)),
        exit_override,
    ))
}

/// Read every `.md` file (except the generated `README.md`) directly under
/// `adr_full` into an [`AdrEntry`], sorted by number then filename.
fn collect_adrs(adr_full: &Path, adr_rel: &str) -> Result<Vec<AdrEntry>> {
    let mut entries: Vec<AdrEntry> = Vec::new();
    let read = std::fs::read_dir(adr_full)
        .with_context(|| format!("failed to read ADR directory {adr_rel}"))?;
    for dirent in read.flatten() {
        if !dirent.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let name = dirent.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.to_ascii_lowercase().ends_with(".md") {
            continue;
        }
        // Skip the reserved TOC file and any bare index.
        let lower = name.to_ascii_lowercase();
        if lower == "readme.md" || lower == "index.md" {
            continue;
        }
        let full = dirent.path();
        entries.push(read_adr_entry(&full, name)?);
    }
    entries.sort_by(|a, b| match (a.number, b.number) {
        (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.link.cmp(&b.link)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.link.cmp(&b.link),
    });
    Ok(entries)
}

/// Read one ADR file's frontmatter/heading into an [`AdrEntry`].
fn read_adr_entry(full: &Path, file_name: &str) -> Result<AdrEntry> {
    let props = hyalo_core::frontmatter::read_frontmatter(full)
        .with_context(|| format!("failed to parse frontmatter of {file_name}"))?;

    let number = leading_number(file_name);

    let title = props
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| first_heading(full))
        .unwrap_or_else(|| stem(file_name));

    let status = props
        .get("status")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    let date = props
        .get("date")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    Ok(AdrEntry {
        number,
        title,
        link: file_name.to_owned(),
        status,
        date,
    })
}

/// Render the managed TOC body (a Markdown table). Exclusive of the markers.
fn render_toc_body(entries: &[AdrEntry]) -> String {
    if entries.is_empty() {
        return "_No ADRs yet._".to_owned();
    }
    let mut out = String::new();
    out.push_str("| # | Title | Status | Date |\n");
    out.push_str("| --- | --- | --- | --- |\n");
    for e in entries {
        let num = e
            .number
            .map_or_else(|| "—".to_owned(), |n| format!("{n:04}"));
        let status = e.status.as_deref().unwrap_or("—");
        let date = e.date.as_deref().unwrap_or("—");
        let _ = writeln!(
            out,
            "| {} | [{}]({}) | {} | {} |",
            num,
            escape_cell(&e.title),
            e.link,
            escape_cell(status),
            escape_cell(date),
        );
    }
    // Trim the trailing newline the last writeln! added; splice re-adds one.
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Escape a `|` so it does not break the Markdown table cell.
fn escape_cell(s: &str) -> String {
    s.replace('|', "\\|")
}

/// Extract the leading `NNNN` number from a filename (`0007-x.md` → 7).
fn leading_number(file_name: &str) -> Option<u32> {
    let digits: String = file_name.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// The filename stem (`0007-use-pg.md` → `0007-use-pg`).
fn stem(file_name: &str) -> String {
    Path::new(file_name).file_stem().map_or_else(
        || file_name.to_owned(),
        |s| s.to_string_lossy().into_owned(),
    )
}

/// The first `# ` ATX heading text in the file body, if any. Reads the file
/// line-by-line (CRLF-tolerant) so a title-less frontmatter falls back to it.
fn first_heading(full: &Path) -> Option<String> {
    let content = std::fs::read_to_string(full).ok()?;
    // Skip a leading `---` frontmatter block.
    let body = strip_frontmatter(&content);
    for raw in body.lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = line.strip_prefix("# ") {
            let t = rest.trim();
            if !t.is_empty() {
                return Some(t.to_owned());
            }
        }
    }
    None
}

/// Return the body slice of `content` after a leading `---`-delimited YAML
/// frontmatter block, or the whole string when there is none.
fn strip_frontmatter(content: &str) -> &str {
    let Some(rest) = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))
    else {
        return content;
    };
    // Find the closing `---` line.
    for (idx, _) in rest.match_indices("\n---") {
        let after = &rest[idx + 4..];
        if after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n") {
            let start = after
                .strip_prefix('\n')
                .or_else(|| after.strip_prefix("\r\n"))
                .unwrap_or(after);
            return start;
        }
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_number_parses() {
        assert_eq!(leading_number("0007-x.md"), Some(7));
        assert_eq!(leading_number("readme.md"), None);
    }

    #[test]
    fn render_empty_toc() {
        assert_eq!(render_toc_body(&[]), "_No ADRs yet._");
    }

    #[test]
    fn render_table_rows_sorted_and_padded() {
        let entries = vec![
            AdrEntry {
                number: Some(7),
                title: "Use Postgres".into(),
                link: "0007-use-pg.md".into(),
                status: Some("accepted".into()),
                date: Some("2026-07-17".into()),
            },
            AdrEntry {
                number: Some(1),
                title: "Record decisions".into(),
                link: "0001-record.md".into(),
                status: Some("proposed".into()),
                date: None,
            },
        ];
        // collect_adrs sorts; simulate that here by sorting manually.
        let mut sorted = entries;
        sorted.sort_by_key(|e| e.number);
        let body = render_toc_body(&sorted);
        assert!(body.contains("| 0001 | [Record decisions](0001-record.md) | proposed | — |"));
        assert!(body.contains("| 0007 | [Use Postgres](0007-use-pg.md) | accepted | 2026-07-17 |"));
        // Header present.
        assert!(body.contains("| # | Title | Status | Date |"));
    }

    #[test]
    fn escape_cell_pipes() {
        assert_eq!(escape_cell("a|b"), "a\\|b");
    }

    #[test]
    fn strip_frontmatter_removes_block() {
        let content = "---\ntitle: X\n---\n# Heading\nbody\n";
        assert_eq!(strip_frontmatter(content), "# Heading\nbody\n");
    }

    #[test]
    fn first_heading_falls_back() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("0001-x.md");
        std::fs::write(&f, "---\nstatus: proposed\n---\n\n# My Decision\n\nbody\n").unwrap();
        assert_eq!(first_heading(&f).as_deref(), Some("My Decision"));
    }

    #[test]
    fn toc_generates_and_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let adr = tmp.path().join("docs/decisions");
        std::fs::create_dir_all(&adr).unwrap();
        std::fs::write(
            adr.join("0001-record.md"),
            "---\ntitle: Record decisions\nstatus: accepted\ndate: 2026-07-17\n---\n\n# Record decisions\n",
        )
        .unwrap();

        // First apply creates the README.
        let (_out, exit) = run_toc(tmp.path(), None, true, Format::Json).unwrap();
        assert_eq!(exit, None, "apply never sets a drift exit code");
        let readme = adr.join("README.md");
        assert!(readme.is_file());
        let content = std::fs::read_to_string(&readme).unwrap();
        assert!(content.contains("Record decisions"));
        assert!(content.contains("<!-- madr:toc:begin -->"));

        // Dry-run now reports no drift (exit None).
        let (_out2, exit2) = run_toc(tmp.path(), None, false, Format::Json).unwrap();
        assert_eq!(exit2, None, "no drift after apply");

        // Re-apply is a no-op (idempotent).
        let before = std::fs::read_to_string(&readme).unwrap();
        run_toc(tmp.path(), None, true, Format::Json).unwrap();
        let after = std::fs::read_to_string(&readme).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn toc_dry_run_signals_drift() {
        let tmp = tempfile::tempdir().unwrap();
        let adr = tmp.path().join("docs/decisions");
        std::fs::create_dir_all(&adr).unwrap();
        std::fs::write(adr.join("0001-x.md"), "---\nstatus: proposed\n---\n# X\n").unwrap();
        // No README yet → dry-run must signal drift (exit 1).
        let (_out, exit) = run_toc(tmp.path(), None, false, Format::Json).unwrap();
        assert_eq!(exit, Some(1));
    }

    #[test]
    fn missing_adr_dir_is_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        let (out, exit) = run_toc(tmp.path(), None, false, Format::Json).unwrap();
        assert!(matches!(out, CommandOutcome::UserError(_)));
        assert_eq!(exit, None);
    }

    #[test]
    fn backslash_adr_dir_produces_forward_slash_toc_path() {
        // A Windows-style `adr_dir` override (backslash separators, as a user
        // might type on that platform) must not leak into `toc_rel`/README's
        // path as mixed separators — the generator always deals in
        // forward-slash vault-relative paths internally.
        let tmp = tempfile::tempdir().unwrap();
        let adr = tmp.path().join("docs").join("adr");
        std::fs::create_dir_all(&adr).unwrap();
        std::fs::write(adr.join("0001-x.md"), "---\nstatus: proposed\n---\n# X\n").unwrap();

        let (out, exit) = run_toc(tmp.path(), Some("docs\\adr"), true, Format::Json).unwrap();
        assert_eq!(exit, None, "apply never sets a drift exit code: {out:?}");
        // The README must land at docs/adr/README.md, not a mixed-separator path.
        assert!(adr.join("README.md").is_file());
    }
}
