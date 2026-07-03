#![allow(clippy::missing_errors_doc)]

use crate::output::{CommandOutcome, Format, format_error};
use anyhow::{Context, Result};
use hyalo_core::frontmatter;
use hyalo_core::heading::{SectionFilter, parse_atx_heading};
use hyalo_core::scanner;
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

/// Placeholder pushed in place of a body line that exceeds
/// [`scanner::MAX_BODY_LINE_BYTES`] (mirrors the scanner's own per-line cap;
/// see [`scanner::read_line_capped`]).
///
/// A real dropped line would shift every subsequent 1-based line number, which
/// would silently corrupt `--lines`/`--section` addressing — so the line's
/// *position* is preserved even though its content is not.
fn oversized_line_placeholder() -> String {
    format!(
        "<line skipped: exceeds {} MiB per-line limit>",
        scanner::MAX_BODY_LINE_BYTES / (1024 * 1024)
    )
}

/// Read the raw body lines from a markdown file, skipping frontmatter.
/// Returns the lines with their trailing newlines stripped.
///
/// Uses [`scanner::read_line_capped`] rather than `BufRead::lines()` so a
/// single pathological line (e.g. a minified blob with no newlines) cannot
/// balloon memory — such a line is replaced with [`oversized_line_placeholder`]
/// instead of being buffered in full.
fn read_body_lines(path: &Path) -> Result<Vec<String>> {
    let file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = std::io::BufReader::new(file);

    // Read first line (capped) to check for frontmatter.
    let mut first_line = String::new();
    let (n, first_truncated) =
        scanner::read_line_capped(&mut reader, &mut first_line, scanner::MAX_BODY_LINE_BYTES)
            .with_context(|| format!("failed to read {}", path.display()))?;
    if n == 0 {
        return Ok(Vec::new());
    }

    let mut lines = Vec::new();

    if first_truncated {
        // A real `---` frontmatter delimiter is 3 bytes, so a line this long
        // can only be body content.
        lines.push(oversized_line_placeholder());
    } else {
        let first_trimmed = first_line.trim_end_matches(['\n', '\r']);
        let fm_lines = frontmatter::skip_frontmatter(&mut reader, first_trimmed)?;
        if fm_lines == 0 {
            // No frontmatter — first line is body content
            lines.push(first_trimmed.to_owned());
        }
    }

    loop {
        let mut buf = String::new();
        let (n, truncated) =
            scanner::read_line_capped(&mut reader, &mut buf, scanner::MAX_BODY_LINE_BYTES)
                .with_context(|| format!("failed to read {}", path.display()))?;
        if n == 0 {
            break;
        }
        if truncated {
            lines.push(oversized_line_placeholder());
        } else {
            lines.push(buf.trim_end_matches(['\n', '\r']).to_owned());
        }
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
    user_format: Format,
) -> Result<CommandOutcome> {
    // Resolve file
    let (full_path, rel_path) = match super::resolve_file_user(dir, file) {
        Ok(r) => r,
        Err(e) => return Ok(super::resolve_error_to_outcome(e, format)),
    };

    // `read` targets one explicit file, so — unlike the read-only scanner,
    // which silently skips oversized files across a whole vault — an
    // oversized target here must be a hard, clear error rather than a
    // silent skip or an unbounded read.
    let file_size = std::fs::metadata(&full_path)
        .with_context(|| format!("failed to stat {}", full_path.display()))?
        .len();
    if file_size > scanner::MAX_FILE_SIZE {
        return Ok(CommandOutcome::UserError(format_error(
            format,
            &format!(
                "file too large to read ({} MiB exceeds {} MiB limit)",
                file_size / (1024 * 1024),
                scanner::MAX_FILE_SIZE / (1024 * 1024)
            ),
            Some(&rel_path),
            None,
            None,
        )));
    }

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

    // Apply line range — truncate/drain in place to avoid cloning.
    if let Some(ref range) = line_range {
        let len = content_lines.len();
        let start_idx = range.start.unwrap_or(1).saturating_sub(1).min(len);
        let end_idx = range.end.unwrap_or(len).min(len);
        if start_idx >= end_idx {
            content_lines.clear();
        } else {
            content_lines.truncate(end_idx);
            content_lines.drain(..start_idx);
        }
    }

    // Format output
    let content_str = content_lines.join("\n");

    // Build the JSON representation (used for both JSON output and the internal pipeline).
    let mut obj = serde_json::json!({ "file": rel_path });
    if let Some(ref props) = fm_value {
        let json_val =
            serde_json::to_value(props).context("failed to serialize frontmatter as JSON")?;
        obj["frontmatter"] = json_val;
    }
    if need_body {
        obj["content"] = serde_json::json!(content_str);
    }

    // For JSON user format: return structured JSON (pipeline wraps in envelope).
    // For text user format: return raw text (bypasses pipeline).
    if user_format == Format::Json {
        return Ok(CommandOutcome::success(
            serde_json::to_string_pretty(&obj).context("failed to serialize")?,
        ));
    }

    // Text format: raw output — frontmatter as YAML, body as plain text.
    let mut out = String::new();
    if let Some(ref props) = fm_value {
        // Render frontmatter as YAML
        out.push_str("---\n");
        if !props.is_empty() {
            let yaml = serde_saphyr::to_string(props)
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
    Ok(CommandOutcome::RawOutput(out))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- read_body_lines: per-line memory cap --

    #[test]
    fn read_body_lines_normal_file_unchanged() {
        // Pin exact output for an ordinary small file — the capped reader must
        // be byte-for-byte equivalent to the old `BufRead::lines()` loop.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("normal.md");
        std::fs::write(
            &path,
            "---\ntitle: T\n---\nLine one\nLine two\nLine three\n",
        )
        .unwrap();

        let lines = read_body_lines(&path).unwrap();
        assert_eq!(lines, vec!["Line one", "Line two", "Line three"]);
    }

    #[test]
    fn read_body_lines_no_frontmatter_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("normal.md");
        std::fs::write(&path, "# Heading\n\nBody text\n").unwrap();

        let lines = read_body_lines(&path).unwrap();
        assert_eq!(lines, vec!["# Heading", "", "Body text"]);
    }

    #[test]
    fn read_body_lines_oversized_middle_line_is_placeholdered() {
        // A line exceeding MAX_BODY_LINE_BYTES must not be buffered whole —
        // it is replaced with a placeholder, and surrounding lines survive
        // with their positions intact (mirrors the scanner's per-line cap).
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("huge-line.md");
        let huge: String = "x".repeat(scanner::MAX_BODY_LINE_BYTES + 1);
        let content = format!("before\n{huge}\nafter\n");
        std::fs::write(&path, &content).unwrap();

        let lines = read_body_lines(&path).unwrap();
        assert_eq!(lines.len(), 3, "line count must be preserved: {lines:?}");
        assert_eq!(lines[0], "before");
        assert_ne!(lines[1], huge, "oversized line must not be buffered whole");
        assert!(
            lines[1].contains("skipped"),
            "expected placeholder, got: {}",
            lines[1]
        );
        assert_eq!(
            lines[2], "after",
            "line position after the skip must survive"
        );
    }

    #[test]
    fn read_body_lines_oversized_first_line_no_frontmatter_confusion() {
        // A file whose very first line alone exceeds the cap can't possibly be
        // a valid `---` frontmatter delimiter — it must be treated as body.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("huge-first.md");
        let huge: String = "y".repeat(scanner::MAX_BODY_LINE_BYTES + 1);
        std::fs::write(&path, format!("{huge}\nafter\n")).unwrap();

        let lines = read_body_lines(&path).unwrap();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("skipped"));
        assert_eq!(lines[1], "after");
    }

    #[test]
    fn read_body_lines_line_exactly_at_cap_passes_through() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("exact.md");
        let exact: String = "z".repeat(scanner::MAX_BODY_LINE_BYTES);
        std::fs::write(&path, format!("{exact}\nnext\n")).unwrap();

        let lines = read_body_lines(&path).unwrap();
        assert_eq!(
            lines[0], exact,
            "line at the limit must pass through intact"
        );
        assert_eq!(lines[1], "next");
    }

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

    // -- inline line-range slicing (truncate + drain) --

    /// Mirror the inlined truncate/drain logic for testability.
    fn apply_range(lines: &[String], range: &LineRange) -> Vec<String> {
        let mut v = lines.to_vec();
        let len = v.len();
        let start_idx = range.start.unwrap_or(1).saturating_sub(1).min(len);
        let end_idx = range.end.unwrap_or(len).min(len);
        if start_idx >= end_idx {
            v.clear();
        } else {
            v.truncate(end_idx);
            v.drain(..start_idx);
        }
        v
    }

    #[test]
    fn line_range_middle_slice() {
        let lines: Vec<String> = (1..=10).map(|i| format!("line {i}")).collect();
        let range = LineRange {
            start: Some(3),
            end: Some(5),
        };
        let result = apply_range(&lines, &range);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "line 3");
        assert_eq!(result[2], "line 5");
    }

    #[test]
    fn line_range_clamps_high_end() {
        let lines: Vec<String> = (1..=3).map(|i| format!("line {i}")).collect();
        let range = LineRange {
            start: Some(2),
            end: Some(100),
        };
        let result = apply_range(&lines, &range);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn line_range_empty_input() {
        let lines: Vec<String> = Vec::new();
        let range = LineRange {
            start: Some(1),
            end: Some(5),
        };
        let result = apply_range(&lines, &range);
        assert!(result.is_empty());
    }

    #[test]
    fn line_range_open_start() {
        let lines: Vec<String> = (1..=5).map(|i| format!("line {i}")).collect();
        let range = LineRange {
            start: None,
            end: Some(3),
        };
        let result = apply_range(&lines, &range);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "line 1");
    }

    #[test]
    fn line_range_open_end() {
        let lines: Vec<String> = (1..=5).map(|i| format!("line {i}")).collect();
        let range = LineRange {
            start: Some(3),
            end: None,
        };
        let result = apply_range(&lines, &range);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "line 3");
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
