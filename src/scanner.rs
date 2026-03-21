use anyhow::{Context, Result};
use std::borrow::Cow;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::frontmatter;

/// Controls whether the scanner should continue or stop early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanAction {
    Continue,
    Stop,
}

/// Callback-based scanner that streams through a markdown file.
/// Skips frontmatter, fenced code blocks, and inline code spans.
/// Calls the visitor function for each text segment with its 1-based line number.
pub fn scan_file<F>(path: &Path, visitor: F) -> Result<()>
where
    F: FnMut(&str, usize) -> ScanAction,
{
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    scan_reader(reader, visitor)
}

/// Scan from any buffered reader (useful for testing without file I/O).
pub fn scan_reader<R: BufRead, F>(mut reader: R, mut visitor: F) -> Result<()>
where
    F: FnMut(&str, usize) -> ScanAction,
{
    let mut line_num: usize = 0;
    let mut buf = String::new();

    // Track fenced code block state — declared early so it's in scope for the first-line check
    let mut fence: Option<(char, usize)> = None; // (fence_char, fence_count)

    // Read first line to check for frontmatter
    buf.clear();
    let n = reader.read_line(&mut buf).context("failed to read line")?;
    if n == 0 {
        return Ok(()); // empty file
    }
    line_num += 1;

    let trimmed = buf.trim_end_matches(['\n', '\r']);
    let fm_lines = frontmatter::skip_frontmatter(&mut reader, trimmed)?;
    if fm_lines == 0 {
        // First line is not frontmatter — check if it opens a code fence
        if let Some(f) = detect_opening_fence(trimmed) {
            fence = Some(f);
        } else {
            let cleaned = strip_inline_code(trimmed);
            if visitor(cleaned.as_ref(), line_num) == ScanAction::Stop {
                return Ok(());
            }
        }
    } else {
        line_num = fm_lines;
    }

    loop {
        buf.clear();
        let n = reader.read_line(&mut buf).context("failed to read line")?;
        if n == 0 {
            break; // EOF
        }
        line_num += 1;
        let line = buf.trim_end_matches(['\n', '\r']);

        // Check for fenced code block boundaries
        if let Some((fence_char, fence_count)) = fence {
            if is_closing_fence(line, fence_char, fence_count) {
                fence = None;
            }
            continue; // skip lines inside code blocks
        }

        if let Some(f) = detect_opening_fence(line) {
            fence = Some(f);
            continue;
        }

        // Normal text line — strip inline code spans before passing to visitor
        let cleaned = strip_inline_code(line);
        if visitor(cleaned.as_ref(), line_num) == ScanAction::Stop {
            return Ok(());
        }
    }

    Ok(())
}

/// Detect an opening fence (``` or ~~~) at the start of a line.
/// Returns the fence character and count if found.
fn detect_opening_fence(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    let fence_char = trimmed.as_bytes().first().copied()?;
    if fence_char != b'`' && fence_char != b'~' {
        return None;
    }
    let fence_char = fence_char as char;
    let count = trimmed.chars().take_while(|&c| c == fence_char).count();
    if count >= 3 {
        Some((fence_char, count))
    } else {
        None
    }
}

/// Check if a line is a closing fence matching the opening fence.
fn is_closing_fence(line: &str, fence_char: char, min_count: usize) -> bool {
    let trimmed = line.trim_start();
    let count = trimmed.chars().take_while(|&c| c == fence_char).count();
    if count < min_count {
        return false;
    }
    // After the fence chars, only whitespace is allowed
    trimmed[count * fence_char.len_utf8()..].trim().is_empty()
}

/// Strip inline code spans from a line, replacing their content with spaces
/// to preserve byte positions for link parsing.
/// Returns a borrowed reference when no backticks are present (zero allocation).
fn strip_inline_code(line: &str) -> Cow<'_, str> {
    if !line.contains('`') {
        return Cow::Borrowed(line);
    }

    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut result = line.as_bytes().to_vec();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'`' {
            // Count backticks for the opening delimiter
            let start = i;
            let mut backtick_count = 0;
            while i < len && bytes[i] == b'`' {
                backtick_count += 1;
                i += 1;
            }

            // Find matching closing delimiter (same number of backticks)
            let content_start = i;
            let mut found_close = false;
            while i < len {
                if bytes[i] == b'`' {
                    let mut close_count = 0;
                    while i < len && bytes[i] == b'`' {
                        close_count += 1;
                        i += 1;
                    }
                    if close_count == backtick_count {
                        for b in &mut result[start..i] {
                            *b = b' ';
                        }
                        found_close = true;
                        break;
                    }
                    // Not a match, continue searching
                } else {
                    i += 1;
                }
            }

            if !found_close {
                // No closing backticks found — treat opening backticks as literal
                i = content_start;
            }
        } else {
            i += 1;
        }
    }

    // Only ASCII bytes (backticks and code content) were replaced with ASCII spaces,
    // so the result is always valid UTF-8.
    Cow::Owned(String::from_utf8(result).expect("replacing ASCII with spaces preserves UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
    }

    fn collect_lines(input: &str) -> Vec<(String, usize)> {
        let mut result = Vec::new();
        scan_reader(input.as_bytes(), |text, line| {
            result.push((text.to_string(), line));
            ScanAction::Continue
        })
        .unwrap();
        result
    }

    #[test]
    fn skips_frontmatter() {
        let input = md!(r"
---
title: Test
---
Hello world
");
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, "Hello world");
        assert_eq!(lines[0].1, 4);
    }

    #[test]
    fn no_frontmatter() {
        let input = md!(r"
Hello world
Second line
");
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "Hello world");
        assert_eq!(lines[0].1, 1);
        assert_eq!(lines[1].0, "Second line");
        assert_eq!(lines[1].1, 2);
    }

    #[test]
    fn skips_backtick_fenced_code_block() {
        let input = md!(r"
Before
```
code line
```
After
");
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "Before");
        assert_eq!(lines[1].0, "After");
    }

    #[test]
    fn skips_tilde_fenced_code_block() {
        let input = md!(r"
Before
~~~
code line
~~~
After
");
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "Before");
        assert_eq!(lines[1].0, "After");
    }

    #[test]
    fn fenced_code_with_info_string() {
        let input = md!(r"
Before
```rust
let x = 1;
```
After
");
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "Before");
        assert_eq!(lines[1].0, "After");
    }

    #[test]
    fn fence_requires_matching_char_and_count() {
        // Opening with 4 backticks, closing needs >= 4
        let input = md!(r"
Before
````
code
```
still code
````
After
");
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "Before");
        assert_eq!(lines[1].0, "After");
    }

    #[test]
    fn tilde_fence_not_closed_by_backticks() {
        let input = md!(r"
Before
~~~
code
```
still code
~~~
After
");
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "Before");
        assert_eq!(lines[1].0, "After");
    }

    #[test]
    fn strips_inline_code() {
        let input = "See `[[not a link]]` and [[real link]]\n";
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].0.contains("not a link"));
        assert!(lines[0].0.contains("[[real link]]"));
    }

    #[test]
    fn strips_double_backtick_inline_code() {
        let input = "See ``[[not a link]]`` and [[real]]\n";
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].0.contains("not a link"));
        assert!(lines[0].0.contains("[[real]]"));
    }

    #[test]
    fn early_abort_with_stop() {
        let input = md!(r"
Line 1
Line 2
Line 3
Line 4
");
        let mut result = Vec::new();
        scan_reader(input.as_bytes(), |text, line| {
            result.push((text.to_string(), line));
            if line >= 2 {
                ScanAction::Stop
            } else {
                ScanAction::Continue
            }
        })
        .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn line_numbers_accurate_with_frontmatter() {
        let input = md!(r"
---
title: T
tags:
  - a
---
Line 6
Line 7
");
        let lines = collect_lines(input);
        assert_eq!(lines[0].1, 6);
        assert_eq!(lines[1].1, 7);
    }

    #[test]
    fn line_numbers_accurate_with_code_block() {
        let input = md!(r"
Line 1
```
skipped
skipped
```
Line 6
");
        let lines = collect_lines(input);
        assert_eq!(lines[0], ("Line 1".to_string(), 1));
        assert_eq!(lines[1], ("Line 6".to_string(), 6));
    }

    #[test]
    fn empty_file() {
        let lines = collect_lines("");
        assert!(lines.is_empty());
    }

    #[test]
    fn unmatched_backtick_treated_as_literal() {
        let input = "See `open and [[link]]\n";
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 1);
        // Unmatched backtick is treated as literal, so [[link]] should still be visible
        assert!(lines[0].0.contains("[[link]]"));
    }

    #[test]
    fn non_utf8_file_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bad.md");
        std::fs::write(&path, b"\xff\xfe invalid utf-8 here").unwrap();
        let result = scan_file(&path, |_, _| ScanAction::Continue);
        assert!(result.is_err());
    }

    #[test]
    fn crlf_line_endings() {
        let input = "Line 1\r\nLine 2\r\n"; // CRLF: \r\n cannot be represented in raw strings portably
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "Line 1");
        assert_eq!(lines[1].0, "Line 2");
    }

    #[test]
    fn first_line_is_code_fence() {
        let input = md!(r"
```
[[not a link]]
```
After
");
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, "After");
    }

    #[test]
    fn very_long_line() {
        // A 100 000-character line with an embedded wikilink must be delivered to the
        // visitor intact (no panic, no truncation) so that link extraction can find it.
        let long_part = "a".repeat(100_000);
        let input = format!("{long_part} [[link]] {long_part}\n");
        let lines = collect_lines(&input);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].0.contains("[[link]]"));
    }
}
