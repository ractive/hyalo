#![allow(clippy::missing_errors_doc)]

use crate::output::{CommandOutcome, Format, format_error, format_success};
use anyhow::{Context, Result};
use hyalo_core::discovery;
use hyalo_core::frontmatter;
use hyalo_core::heading::{SectionFilter, parse_atx_heading};
use hyalo_core::scanner;
use std::io::BufRead;
use std::path::Path;

// ---------------------------------------------------------------------------
// Line range parsing
// ---------------------------------------------------------------------------

/// Inclusive 1-based line range.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LineRange {
    /// 1-based start (inclusive). `None` means from the beginning.
    start: Option<usize>,
    /// 1-based end (inclusive). `None` means to the end.
    end: Option<usize>,
}

/// Parse a line range string like `5:10`, `5:`, `:10`, or `5`.
fn parse_line_range(s: &str) -> Result<LineRange, String> {
    if let Some((left, right)) = s.split_once(':') {
        let start = if left.is_empty() {
            None
        } else {
            let n = left
                .parse::<usize>()
                .map_err(|_| format!("invalid line number: {left}"))?;
            if n == 0 {
                return Err("line numbers are 1-based".to_owned());
            }
            Some(n)
        };
        let end = if right.is_empty() {
            None
        } else {
            let n = right
                .parse::<usize>()
                .map_err(|_| format!("invalid line number: {right}"))?;
            if n == 0 {
                return Err("line numbers are 1-based".to_owned());
            }
            Some(n)
        };
        if let (Some(s), Some(e)) = (start, end)
            && s > e
        {
            return Err(format!("start ({s}) must be <= end ({e})"));
        }
        Ok(LineRange { start, end })
    } else {
        let n = s
            .parse::<usize>()
            .map_err(|_| format!("invalid line number: {s}"))?;
        if n == 0 {
            return Err("line numbers are 1-based".to_owned());
        }
        Ok(LineRange {
            start: Some(n),
            end: Some(n),
        })
    }
}

/// Apply a line range to a slice of lines, returning the selected sub-slice.
/// Line numbers are 1-based and inclusive. Out-of-range values are clamped.
fn apply_line_range<'a>(lines: &'a [String], range: &LineRange) -> &'a [String] {
    let len = lines.len();
    if len == 0 {
        return lines;
    }
    let start_idx = range.start.unwrap_or(1).saturating_sub(1).min(len);
    let end_idx = range.end.unwrap_or(len).min(len);
    if start_idx >= end_idx {
        return &lines[0..0];
    }
    &lines[start_idx..end_idx]
}

// ---------------------------------------------------------------------------
// Section extraction
// ---------------------------------------------------------------------------

/// Extract all sections matching `filter` (case-insensitive substring match on heading text,
/// optional level pinning). Returns a `Vec<Vec<String>>`, where each inner `Vec<String>`
/// contains the lines of a matched section, from the heading through to (but not including)
/// the next heading of equal or higher level.
fn extract_sections(body_lines: &[String], filter: &SectionFilter) -> Vec<Vec<String>> {
    let mut sections: Vec<Vec<String>> = Vec::new();
    let mut current_section: Option<(u8, Vec<String>)> = None;
    let mut fence = scanner::FenceTracker::new();

    for line in body_lines {
        // Track code fences — headings inside code blocks are not real headings
        if fence.process_line(line) {
            if let Some((_, ref mut lines)) = current_section {
                lines.push(line.clone());
            }
            continue;
        }

        if let Some((level, text)) = parse_atx_heading(line) {
            // Flush current section if a heading of equal or higher level is encountered
            if let Some((sec_level, sec_lines)) = current_section.take() {
                if level <= sec_level {
                    sections.push(sec_lines);
                } else {
                    // Lower-level heading (deeper nesting) — still part of current section
                    let mut lines = sec_lines;
                    lines.push(line.clone());
                    current_section = Some((sec_level, lines));
                    continue;
                }
            }

            if filter.matches(level, text) {
                current_section = Some((level, vec![line.clone()]));
            }
        } else if let Some((_, ref mut lines)) = current_section {
            lines.push(line.clone());
        }
    }

    // Flush final section
    if let Some((_, lines)) = current_section {
        sections.push(lines);
    }

    sections
}

/// Collect all heading texts from body lines (for error messages).
fn collect_headings(body_lines: &[String]) -> Vec<String> {
    let mut fence = scanner::FenceTracker::new();
    let mut headings = Vec::new();
    for line in body_lines {
        if fence.process_line(line) {
            continue;
        }
        if let Some((level, text)) = parse_atx_heading(line) {
            let hashes = "#".repeat(level as usize);
            headings.push(format!("{hashes} {text}"));
        }
    }
    headings
}

// ---------------------------------------------------------------------------
// Read body lines from file (raw, no stripping)
// ---------------------------------------------------------------------------

/// Read the raw body lines from a markdown file, skipping frontmatter.
/// Returns the lines with their trailing newlines stripped.
fn read_body_lines(path: &Path) -> Result<Vec<String>> {
    let file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = std::io::BufReader::new(file);

    // Read first line to check for frontmatter
    let mut first_line = String::new();
    let n = reader
        .read_line(&mut first_line)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if n == 0 {
        return Ok(Vec::new());
    }

    let first_trimmed = first_line.trim_end_matches(['\n', '\r']);
    let fm_lines = frontmatter::skip_frontmatter(&mut reader, first_trimmed)?;

    let mut lines = Vec::new();

    if fm_lines == 0 {
        // No frontmatter — first line is body content
        lines.push(first_trimmed.to_owned());
    }

    for line_result in reader.lines() {
        let line = line_result.with_context(|| format!("failed to read {}", path.display()))?;
        lines.push(line);
    }

    Ok(lines)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn run(
    dir: &Path,
    file: &str,
    section: Option<&str>,
    lines: Option<&str>,
    frontmatter_flag: bool,
    format: Format,
) -> Result<CommandOutcome> {
    // Resolve file
    let (full_path, rel_path) = match discovery::resolve_file(dir, file) {
        Ok(r) => r,
        Err(e) => return Ok(super::resolve_error_to_outcome(e, format)),
    };

    // Parse line range early so we fail fast on bad input
    let line_range = if let Some(range_str) = lines {
        match parse_line_range(range_str) {
            Ok(r) => Some(r),
            Err(msg) => {
                return Ok(CommandOutcome::UserError(format_error(
                    format,
                    &msg,
                    None,
                    Some("expected format: 5:10, 5:, :10, or 5"),
                    None,
                )));
            }
        }
    } else {
        None
    };

    // Read frontmatter if requested.  Parse errors (e.g. unclosed `---`) are
    // user errors (exit 1), not internal errors (exit 2).
    let fm_value = if frontmatter_flag {
        match frontmatter::read_frontmatter(&full_path) {
            Ok(props) => Some(props),
            Err(e) if frontmatter::is_parse_error(&e) => {
                return Ok(CommandOutcome::UserError(format_error(
                    format,
                    &e.to_string(),
                    Some(&rel_path),
                    None,
                    None,
                )));
            }
            Err(e) => return Err(e),
        }
    } else {
        None
    };

    // Determine what content to return
    let need_body = section.is_some() || !frontmatter_flag || line_range.is_some();

    let mut content_lines: Vec<String> = if need_body {
        read_body_lines(&full_path)?
    } else {
        Vec::new()
    };

    // Apply section filter
    if let Some(query) = section {
        let filter = match SectionFilter::parse(query) {
            Ok(f) => f,
            Err(e) => {
                return Ok(CommandOutcome::UserError(format_error(
                    format,
                    &e,
                    Some(&rel_path),
                    None,
                    None,
                )));
            }
        };
        let sections = extract_sections(&content_lines, &filter);
        if sections.is_empty() {
            let available = collect_headings(&content_lines);
            let hint = if available.is_empty() {
                "this file has no headings".to_owned()
            } else {
                format!("available sections: {}", available.join(", "))
            };
            return Ok(CommandOutcome::UserError(format_error(
                format,
                &format!("section not found: {query}"),
                Some(&rel_path),
                Some(&hint),
                None,
            )));
        }

        // Join multiple matching sections with a blank line separator
        content_lines = Vec::new();
        for (i, sec) in sections.iter().enumerate() {
            if i > 0 {
                content_lines.push(String::new());
            }
            content_lines.extend_from_slice(sec);
        }
    }

    // Apply line range
    if let Some(ref range) = line_range {
        let sliced = apply_line_range(&content_lines, range);
        content_lines = sliced.to_vec();
    }

    // Format output
    let content_str = content_lines.join("\n");

    match format {
        Format::Text => {
            let mut out = String::new();
            if let Some(ref props) = fm_value {
                // Render frontmatter as YAML
                out.push_str("---\n");
                if !props.is_empty() {
                    let yaml = serde_yaml_ng::to_string(props)
                        .context("failed to serialize frontmatter as YAML")?;
                    out.push_str(&yaml);
                }
                out.push_str("---\n");
                if need_body && !content_str.is_empty() {
                    out.push('\n');
                }
            }
            if need_body {
                out.push_str(&content_str);
                // Ensure trailing newline for pipe-friendliness
                if !out.ends_with('\n') {
                    out.push('\n');
                }
            }
            Ok(CommandOutcome::Success(out))
        }
        Format::Json => {
            let mut obj = serde_json::json!({ "file": rel_path });
            if let Some(props) = fm_value {
                let json_val = serde_json::to_value(&props)
                    .context("failed to serialize frontmatter as JSON")?;
                obj["frontmatter"] = json_val;
            }
            if need_body {
                obj["content"] = serde_json::json!(content_str);
            }
            Ok(CommandOutcome::Success(format_success(format, &obj)))
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_line_range --

    #[test]
    fn range_single_line() {
        assert_eq!(
            parse_line_range("5").unwrap(),
            LineRange {
                start: Some(5),
                end: Some(5)
            }
        );
    }

    #[test]
    fn range_full() {
        assert_eq!(
            parse_line_range("5:10").unwrap(),
            LineRange {
                start: Some(5),
                end: Some(10)
            }
        );
    }

    #[test]
    fn range_open_start() {
        assert_eq!(
            parse_line_range(":10").unwrap(),
            LineRange {
                start: None,
                end: Some(10)
            }
        );
    }

    #[test]
    fn range_open_end() {
        assert_eq!(
            parse_line_range("5:").unwrap(),
            LineRange {
                start: Some(5),
                end: None
            }
        );
    }

    #[test]
    fn range_zero_is_error() {
        assert!(parse_line_range("0").is_err());
        assert!(parse_line_range("0:5").is_err());
        assert!(parse_line_range("5:0").is_err());
    }

    #[test]
    fn range_inverted_is_error() {
        assert!(parse_line_range("10:5").is_err());
    }

    #[test]
    fn range_non_numeric_is_error() {
        assert!(parse_line_range("abc").is_err());
        assert!(parse_line_range("a:b").is_err());
    }

    // -- apply_line_range --

    #[test]
    fn apply_range_to_lines() {
        let lines: Vec<String> = (1..=10).map(|i| format!("line {i}")).collect();
        let range = LineRange {
            start: Some(3),
            end: Some(5),
        };
        let result = apply_line_range(&lines, &range);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "line 3");
        assert_eq!(result[2], "line 5");
    }

    #[test]
    fn apply_range_clamps_high_end() {
        let lines: Vec<String> = (1..=3).map(|i| format!("line {i}")).collect();
        let range = LineRange {
            start: Some(2),
            end: Some(100),
        };
        let result = apply_line_range(&lines, &range);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn apply_range_empty_lines() {
        let lines: Vec<String> = Vec::new();
        let range = LineRange {
            start: Some(1),
            end: Some(5),
        };
        let result = apply_line_range(&lines, &range);
        assert!(result.is_empty());
    }

    // -- extract_sections --

    #[test]
    fn extract_section_exact_match() {
        let lines: Vec<String> = vec![
            "# Title".into(),
            "intro".into(),
            "## Problem".into(),
            "problem text".into(),
            "## Solution".into(),
            "solution text".into(),
        ];
        let filter = SectionFilter::parse("Problem").unwrap();
        let sections = extract_sections(&lines, &filter);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].len(), 2);
        assert_eq!(sections[0][0], "## Problem");
        assert_eq!(sections[0][1], "problem text");
    }

    #[test]
    fn extract_section_case_insensitive() {
        let lines: Vec<String> = vec!["## Problem".into(), "text".into(), "## Other".into()];
        let filter = SectionFilter::parse("problem").unwrap();
        let sections = extract_sections(&lines, &filter);
        assert_eq!(sections.len(), 1);
    }

    #[test]
    fn extract_section_with_hashes() {
        let lines: Vec<String> = vec!["## Problem".into(), "text".into(), "## Other".into()];
        let filter = SectionFilter::parse("## Problem").unwrap();
        let sections = extract_sections(&lines, &filter);
        assert_eq!(sections.len(), 1);
    }

    #[test]
    fn extract_section_substring_matches_longer_heading() {
        // "Problem" is a substring of "Problems" — should now match
        let lines: Vec<String> = vec!["## Problems".into(), "text".into()];
        let filter = SectionFilter::parse("Problem").unwrap();
        let sections = extract_sections(&lines, &filter);
        assert_eq!(sections.len(), 1);
    }

    #[test]
    fn extract_section_no_match_unrelated() {
        // "Design" is not a substring of "Problems"
        let lines: Vec<String> = vec!["## Problems".into(), "text".into()];
        let filter = SectionFilter::parse("Design").unwrap();
        let sections = extract_sections(&lines, &filter);
        assert!(sections.is_empty());
    }

    #[test]
    fn extract_section_suffix_count() {
        // "Tasks" matches "Tasks [4/4]"
        let lines: Vec<String> = vec!["## Tasks [4/4]".into(), "- [x] Done".into()];
        let filter = SectionFilter::parse("Tasks").unwrap();
        let sections = extract_sections(&lines, &filter);
        assert_eq!(sections.len(), 1);
    }

    #[test]
    fn extract_section_includes_nested() {
        let lines: Vec<String> = vec![
            "## Section".into(),
            "text".into(),
            "### Subsection".into(),
            "sub text".into(),
            "## Next".into(),
        ];
        let filter = SectionFilter::parse("Section").unwrap();
        let sections = extract_sections(&lines, &filter);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].len(), 4); // heading + text + sub heading + sub text
    }

    #[test]
    fn extract_section_multiple_matches() {
        let lines: Vec<String> = vec![
            "## Notes".into(),
            "first notes".into(),
            "## Other".into(),
            "other".into(),
            "## Notes".into(),
            "second notes".into(),
        ];
        let filter = SectionFilter::parse("Notes").unwrap();
        let sections = extract_sections(&lines, &filter);
        assert_eq!(sections.len(), 2);
    }

    #[test]
    fn extract_section_at_end_of_file() {
        let lines: Vec<String> = vec![
            "## First".into(),
            "text".into(),
            "## Last".into(),
            "last text".into(),
        ];
        let filter = SectionFilter::parse("Last").unwrap();
        let sections = extract_sections(&lines, &filter);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].len(), 2);
    }

    #[test]
    fn extract_section_skips_headings_in_code_blocks() {
        let lines: Vec<String> = vec![
            "## Proposal".into(),
            "intro".into(),
            "```sh".into(),
            "# This is a comment, not a heading".into(),
            "echo hello".into(),
            "```".into(),
            "after code".into(),
            "## Next".into(),
        ];
        let filter = SectionFilter::parse("Proposal").unwrap();
        let sections = extract_sections(&lines, &filter);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].len(), 7); // heading + intro + code block (4 lines) + after code
        assert!(sections[0].contains(&"# This is a comment, not a heading".to_owned()));
    }
}
