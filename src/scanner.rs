#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use serde_yaml_ng::Value;
use std::borrow::Cow;
use std::collections::BTreeMap;
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

    // Track fenced code block and comment block state
    let mut fence: Option<(char, usize)> = None; // (fence_char, fence_count)
    let mut in_comment = false;

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
        // First line is not frontmatter — check if it opens a code fence or comment
        if let Some(f) = detect_opening_fence(trimmed) {
            fence = Some(f);
        } else if is_comment_fence(trimmed) {
            in_comment = true;
        } else {
            let cleaned = strip_inline_code(trimmed);
            let cleaned = strip_inline_comments(&cleaned);
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

        // Check for fenced code block boundaries (highest priority)
        if let Some((fence_char, fence_count)) = fence {
            if is_closing_fence(line, fence_char, fence_count) {
                fence = None;
            }
            continue; // skip lines inside code blocks
        }

        // Check for comment block boundaries
        if in_comment {
            if is_comment_fence(line) {
                in_comment = false;
            }
            continue; // skip lines inside comment blocks
        }

        if let Some(f) = detect_opening_fence(line) {
            fence = Some(f);
            continue;
        }

        if is_comment_fence(line) {
            in_comment = true;
            continue;
        }

        // Normal text line — strip inline code spans and comments before passing to visitor
        let cleaned = strip_inline_code(line);
        let cleaned = strip_inline_comments(&cleaned);
        if visitor(cleaned.as_ref(), line_num) == ScanAction::Stop {
            return Ok(());
        }
    }

    Ok(())
}

/// Detect an opening fence (triple backtick or `~~~`) at the start of a line.
/// Returns the fence character and count if found.
pub(crate) fn detect_opening_fence(line: &str) -> Option<(char, usize)> {
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
pub(crate) fn is_closing_fence(line: &str, fence_char: char, min_count: usize) -> bool {
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
pub fn strip_inline_code(line: &str) -> Cow<'_, str> {
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

    // SAFETY: The input `line` is valid UTF-8. We iterate byte-by-byte starting
    // from ASCII backtick delimiters, replacing bytes within matched code spans
    // (delimiters and content between them) with ASCII spaces (0x20). Each
    // replacement position is a valid byte boundary because: (1) the scan starts
    // at ASCII bytes which are always single-byte in UTF-8, and (2) we advance
    // one byte at a time through the span. Replacing any byte in a valid UTF-8
    // sequence with an ASCII byte (0x00–0x7F) preserves validity, because ASCII
    // bytes never appear as continuation bytes (0x80–0xBF) in multi-byte sequences.
    Cow::Owned(unsafe { String::from_utf8_unchecked(result) })
}

/// Check if a line is an Obsidian comment fence (`%%` on its own line).
///
/// Returns `true` when the trimmed line is exactly `%%`. Lines containing
/// `%%` with other content (e.g. inline comments like `%%text%%`) are NOT
/// comment fences.
pub(crate) fn is_comment_fence(line: &str) -> bool {
    line.trim() == "%%"
}

/// Strip inline Obsidian comments (`%%text%%`) from a line, replacing them
/// (markers inclusive) with spaces to preserve byte positions for downstream
/// parsing.
///
/// Returns a borrowed reference when no `%%` is present (zero allocation).
/// Unmatched opening `%%` is treated as literal text.
pub fn strip_inline_comments(line: &str) -> Cow<'_, str> {
    if !line.contains("%%") {
        return Cow::Borrowed(line);
    }

    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut result = bytes.to_vec();
    let mut i = 0;

    while i + 1 < len {
        if bytes[i] == b'%' && bytes[i + 1] == b'%' {
            let open = i;
            i += 2; // skip opening %%

            // If the rest of the line is only whitespace, this is a block fence
            // marker, not an inline comment — leave it alone.
            if line[i..].trim().is_empty() {
                break;
            }

            // Scan for closing %%
            let mut found_close = false;
            while i + 1 < len {
                if bytes[i] == b'%' && bytes[i + 1] == b'%' {
                    // Replace open..=i+1 with spaces
                    for b in &mut result[open..i + 2] {
                        *b = b' ';
                    }
                    i += 2;
                    found_close = true;
                    break;
                }
                i += 1;
            }
            if !found_close {
                // No closing %% — treat the opening as literal
                i = open + 2;
            }
        } else {
            i += 1;
        }
    }

    if result == bytes {
        Cow::Borrowed(line)
    } else {
        // Comment regions are overwritten with ASCII spaces, regardless of their
        // original contents, so the resulting byte sequence is always valid UTF-8.
        // SAFETY: The input `line` is valid UTF-8. We iterate byte-by-byte starting
        // from ASCII `%%` delimiters, replacing bytes within matched comment spans
        // (delimiters and content between them) with ASCII spaces (0x20). Each
        // replacement position is a valid byte boundary because: (1) the scan starts
        // at ASCII percent bytes which are always single-byte in UTF-8, and (2) we
        // advance one byte at a time through the span. Replacing any byte in a valid
        // UTF-8 sequence with an ASCII byte (0x00–0x7F) preserves validity, because
        // ASCII bytes never appear as continuation bytes (0x80–0xBF) in multi-byte
        // sequences.
        Cow::Owned(unsafe { String::from_utf8_unchecked(result) })
    }
}

// ---------------------------------------------------------------------------
// Multi-visitor scanner
// ---------------------------------------------------------------------------

/// Trait for visitors that receive events from a single-pass file scan.
///
/// All methods have default no-op implementations, so visitors only need
/// to override the events they care about.
pub trait FileVisitor {
    /// Called with parsed frontmatter properties (empty `BTreeMap` if none).
    /// Return `ScanAction::Stop` to skip the body scan for this visitor.
    fn on_frontmatter(&mut self, _props: &BTreeMap<String, Value>) -> ScanAction {
        ScanAction::Continue
    }

    /// Called for each body line outside fenced code blocks and comment blocks.
    /// Inline `%%comment%%` spans are stripped; inline code spans are **not**.
    fn on_body_line(&mut self, _raw: &str, _line_num: usize) -> ScanAction {
        ScanAction::Continue
    }

    /// Called when a fenced code block opens (e.g. `` ```rust ``).
    fn on_code_fence_open(&mut self, _raw: &str, _language: &str, _line_num: usize) -> ScanAction {
        ScanAction::Continue
    }

    /// Called when a fenced code block closes.
    fn on_code_fence_close(&mut self, _line_num: usize) -> ScanAction {
        ScanAction::Continue
    }

    /// Whether this visitor needs body events (`on_body_line`, `on_code_fence_*`).
    /// If `false`, the visitor only receives `on_frontmatter` and is then stopped.
    /// Default: `true`.
    fn needs_body(&self) -> bool {
        true
    }
}

/// Extract the info-string (language tag) from a fenced code block opening line.
/// E.g. `` ```rust `` → `"rust"`, `~~~` → `""`
pub fn extract_fence_language(line: &str, fence_char: char, fence_count: usize) -> String {
    let trimmed = line.trim_start();
    let after_fence = &trimmed[fence_count * fence_char.len_utf8()..];
    after_fence.trim().to_owned()
}

/// Scan a file with multiple visitors in a single pass.
///
/// Opens the file once, parses frontmatter, then streams body lines to all
/// active visitors. Stops early when all visitors have returned `ScanAction::Stop`.
///
/// **Optimization**: if no visitor needs body events (all return `Stop` from
/// `on_frontmatter` or have `needs_body() == false`), the body is never read.
pub fn scan_file_multi(path: &Path, visitors: &mut [&mut dyn FileVisitor]) -> Result<()> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    scan_reader_multi(reader, visitors)
}

/// Scan from any buffered reader with multiple visitors.
pub fn scan_reader_multi<R: BufRead>(
    mut reader: R,
    visitors: &mut [&mut dyn FileVisitor],
) -> Result<()> {
    let num = visitors.len();
    if num == 0 {
        return Ok(());
    }

    let mut active: Vec<bool> = vec![true; num];
    let mut buf = String::new();
    let mut line_num: usize = 0;

    // --- Phase 1: Frontmatter ---
    buf.clear();
    let n = reader.read_line(&mut buf).context("failed to read line")?;
    if n == 0 {
        // Empty file — deliver empty frontmatter
        let empty = BTreeMap::new();
        for (i, v) in visitors.iter_mut().enumerate() {
            if v.on_frontmatter(&empty) == ScanAction::Stop {
                active[i] = false;
            }
        }
        return Ok(());
    }
    line_num += 1;

    let first_trimmed = buf.trim_end_matches(['\n', '\r']).to_owned();

    // Try to parse frontmatter
    let (fm_props, fm_lines) = if first_trimmed.trim() == "---" {
        // Collect YAML lines
        let mut yaml = String::new();
        let mut fm_line_count: usize = 1; // the opening `---`
        let mut found_close = false;
        loop {
            buf.clear();
            let n = reader.read_line(&mut buf).context("failed to read line")?;
            if n == 0 {
                break;
            }
            fm_line_count += 1;
            let trimmed = buf.trim_end_matches(['\n', '\r']);
            if trimmed.trim() == "---" {
                found_close = true;
                break;
            }
            yaml.push_str(trimmed);
            yaml.push('\n');
        }
        let props: BTreeMap<String, Value> = if found_close && !yaml.trim().is_empty() {
            serde_yaml_ng::from_str(&yaml).unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        (props, fm_line_count)
    } else {
        (BTreeMap::new(), 0usize)
    };

    // Deliver frontmatter to all visitors
    for (i, v) in visitors.iter_mut().enumerate() {
        if v.on_frontmatter(&fm_props) == ScanAction::Stop || !v.needs_body() {
            active[i] = false;
        }
    }

    // If all visitors are done, skip the body
    if !active.iter().any(|&a| a) {
        return Ok(());
    }

    // --- Phase 2: Body ---
    let mut fence: Option<(char, usize)> = None;
    let mut in_comment = false;

    if fm_lines > 0 {
        line_num = fm_lines;
    }

    // If the first line was not frontmatter, process it as a body line
    if fm_lines == 0 {
        dispatch_body_line(
            &first_trimmed,
            line_num,
            visitors,
            &mut active,
            &mut fence,
            &mut in_comment,
        );
        if !active.iter().any(|&a| a) {
            return Ok(());
        }
    }

    loop {
        buf.clear();
        let n = reader.read_line(&mut buf).context("failed to read line")?;
        if n == 0 {
            break;
        }
        line_num += 1;
        let line = buf.trim_end_matches(['\n', '\r']);

        dispatch_body_line(
            line,
            line_num,
            visitors,
            &mut active,
            &mut fence,
            &mut in_comment,
        );
        if !active.iter().any(|&a| a) {
            break;
        }
    }

    Ok(())
}

/// Dispatch a single body line to active visitors, handling code fence state.
fn dispatch_body_line(
    line: &str,
    line_num: usize,
    visitors: &mut [&mut dyn FileVisitor],
    active: &mut [bool],
    fence: &mut Option<(char, usize)>,
    in_comment: &mut bool,
) {
    // Code fences take highest priority — %% inside a code block is literal.
    if let Some((fence_char, fence_count)) = *fence {
        if is_closing_fence(line, fence_char, fence_count) {
            *fence = None;
            for (i, v) in visitors.iter_mut().enumerate() {
                if active[i] && v.on_code_fence_close(line_num) == ScanAction::Stop {
                    active[i] = false;
                }
            }
        }
        // Lines inside code blocks are not delivered as body lines
        return;
    }

    // Comment blocks — code fences inside comments are ignored.
    if *in_comment {
        if is_comment_fence(line) {
            *in_comment = false;
        }
        return;
    }

    if let Some((fc, count)) = detect_opening_fence(line) {
        let lang = extract_fence_language(line, fc, count);
        *fence = Some((fc, count));
        for (i, v) in visitors.iter_mut().enumerate() {
            if active[i] && v.on_code_fence_open(line, &lang, line_num) == ScanAction::Stop {
                active[i] = false;
            }
        }
        return;
    }

    if is_comment_fence(line) {
        *in_comment = true;
        return;
    }

    // Normal body line — strip inline comments before delivering to visitors.
    let cleaned = strip_inline_comments(line);
    for (i, v) in visitors.iter_mut().enumerate() {
        if active[i] && v.on_body_line(&cleaned, line_num) == ScanAction::Stop {
            active[i] = false;
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in visitors
// ---------------------------------------------------------------------------

/// Collects frontmatter properties from a file scan.
pub struct FrontmatterCollector {
    props: BTreeMap<String, Value>,
    body_needed: bool,
}

impl FrontmatterCollector {
    /// Create a new collector.
    /// If `body_needed` is false, signals the scanner to skip the body after frontmatter.
    #[must_use]
    pub fn new(body_needed: bool) -> Self {
        Self {
            props: BTreeMap::new(),
            body_needed,
        }
    }

    /// Consume the collector and return the parsed properties.
    #[must_use]
    pub fn into_props(self) -> BTreeMap<String, Value> {
        self.props
    }
}

impl FileVisitor for FrontmatterCollector {
    fn on_frontmatter(&mut self, props: &BTreeMap<String, Value>) -> ScanAction {
        self.props = props.clone();
        if self.body_needed {
            ScanAction::Continue
        } else {
            ScanAction::Stop
        }
    }

    fn needs_body(&self) -> bool {
        self.body_needed
    }
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

    // --- Multi-visitor scanner tests ---

    /// Test visitor that collects body lines (raw text).
    struct BodyCollector {
        lines: Vec<(String, usize)>,
    }

    impl BodyCollector {
        fn new() -> Self {
            Self { lines: Vec::new() }
        }
    }

    impl FileVisitor for BodyCollector {
        fn on_body_line(&mut self, raw: &str, line_num: usize) -> ScanAction {
            self.lines.push((raw.to_owned(), line_num));
            ScanAction::Continue
        }
    }

    /// Test visitor that counts code fence events.
    struct FenceCounter {
        opens: Vec<(String, usize)>,
        closes: Vec<usize>,
    }

    impl FenceCounter {
        fn new() -> Self {
            Self {
                opens: Vec::new(),
                closes: Vec::new(),
            }
        }
    }

    impl FileVisitor for FenceCounter {
        fn on_code_fence_open(
            &mut self,
            _raw: &str,
            language: &str,
            line_num: usize,
        ) -> ScanAction {
            self.opens.push((language.to_owned(), line_num));
            ScanAction::Continue
        }

        fn on_code_fence_close(&mut self, line_num: usize) -> ScanAction {
            self.closes.push(line_num);
            ScanAction::Continue
        }
    }

    #[test]
    fn multi_visitor_frontmatter_and_body() {
        let input = md!(r"
---
title: Test
tags:
  - a
---
Line 6
Line 7
");
        let mut fm = FrontmatterCollector::new(true);
        let mut body = BodyCollector::new();
        scan_reader_multi(input.as_bytes(), &mut [&mut fm, &mut body]).unwrap();

        let props = fm.into_props();
        assert_eq!(props.get("title").unwrap().as_str(), Some("Test"));

        assert_eq!(body.lines.len(), 2);
        assert_eq!(body.lines[0].0, "Line 6");
        assert_eq!(body.lines[0].1, 6);
        assert_eq!(body.lines[1].0, "Line 7");
        assert_eq!(body.lines[1].1, 7);
    }

    #[test]
    fn multi_visitor_frontmatter_only_skips_body() {
        let input = md!(r"
---
title: Test
---
Line 4
Line 5
");
        let mut fm = FrontmatterCollector::new(false);
        let mut body = BodyCollector::new();
        scan_reader_multi(input.as_bytes(), &mut [&mut fm, &mut body]).unwrap();

        let props = fm.into_props();
        assert_eq!(props.get("title").unwrap().as_str(), Some("Test"));

        // body collector has needs_body() == true, so it should still get body lines
        assert_eq!(body.lines.len(), 2);
    }

    #[test]
    fn multi_visitor_all_frontmatter_only_skips_body_read() {
        // When ALL visitors don't need body, the body should not be read.
        // We verify this by checking that FrontmatterCollector gets the right data
        // and no panics occur.
        let input = md!(r"
---
title: Test
---
Line 4
");
        let mut fm1 = FrontmatterCollector::new(false);
        let mut fm2 = FrontmatterCollector::new(false);
        scan_reader_multi(input.as_bytes(), &mut [&mut fm1, &mut fm2]).unwrap();

        assert_eq!(
            fm1.into_props().get("title").unwrap().as_str(),
            Some("Test")
        );
        assert_eq!(
            fm2.into_props().get("title").unwrap().as_str(),
            Some("Test")
        );
    }

    #[test]
    fn multi_visitor_code_fence_events() {
        let input = md!(r"
Line 1
```rust
code line
```
Line 5
");
        let mut body = BodyCollector::new();
        let mut fences = FenceCounter::new();
        scan_reader_multi(input.as_bytes(), &mut [&mut body, &mut fences]).unwrap();

        assert_eq!(body.lines.len(), 2);
        assert_eq!(body.lines[0].0, "Line 1");
        assert_eq!(body.lines[1].0, "Line 5");

        assert_eq!(fences.opens.len(), 1);
        assert_eq!(fences.opens[0].0, "rust");
        assert_eq!(fences.opens[0].1, 2);

        assert_eq!(fences.closes.len(), 1);
        assert_eq!(fences.closes[0], 4);
    }

    #[test]
    fn multi_visitor_no_frontmatter() {
        let input = md!(r"
Line 1
Line 2
");
        let mut fm = FrontmatterCollector::new(true);
        let mut body = BodyCollector::new();
        scan_reader_multi(input.as_bytes(), &mut [&mut fm, &mut body]).unwrap();

        assert!(fm.into_props().is_empty());
        assert_eq!(body.lines.len(), 2);
        assert_eq!(body.lines[0].0, "Line 1");
        assert_eq!(body.lines[0].1, 1);
    }

    #[test]
    fn multi_visitor_empty_file() {
        let mut fm = FrontmatterCollector::new(true);
        scan_reader_multi("".as_bytes(), &mut [&mut fm]).unwrap();
        assert!(fm.into_props().is_empty());
    }

    #[test]
    fn multi_visitor_no_visitors() {
        scan_reader_multi("hello\n".as_bytes(), &mut []).unwrap();
    }

    #[test]
    fn extract_fence_language_rust() {
        assert_eq!(extract_fence_language("```rust", '`', 3), "rust");
    }

    #[test]
    fn extract_fence_language_empty() {
        assert_eq!(extract_fence_language("```", '`', 3), "");
    }

    #[test]
    fn extract_fence_language_spaces() {
        assert_eq!(extract_fence_language("```  sh  ", '`', 3), "sh");
    }

    // --- Comment block tests (simple callback scanner) ---

    #[test]
    fn skips_multiline_comment_block() {
        let input = md!(r"
Before
%%
commented [[link]]
- [ ] hidden task
%%
After
");
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "Before");
        assert_eq!(lines[1].0, "After");
    }

    #[test]
    fn multiline_comment_preserves_line_numbers() {
        let input = md!(r"
Line 1
%%
skipped
skipped
%%
Line 6
");
        let lines = collect_lines(input);
        assert_eq!(lines[0], ("Line 1".to_string(), 1));
        assert_eq!(lines[1], ("Line 6".to_string(), 6));
    }

    #[test]
    fn inline_comment_stripped() {
        let input = "See %%[[not a link]]%% and [[real link]]\n";
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].0.contains("not a link"));
        assert!(lines[0].0.contains("[[real link]]"));
    }

    #[test]
    fn comment_inside_code_fence_ignored() {
        let input = md!(r"
Before
```
%%
not a comment
%%
```
After
");
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "Before");
        assert_eq!(lines[1].0, "After");
    }

    #[test]
    fn code_fence_inside_comment_ignored() {
        let input = md!(r"
Before
%%
```
not code
```
%%
After
");
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "Before");
        assert_eq!(lines[1].0, "After");
    }

    #[test]
    fn unmatched_inline_comment_treated_as_literal() {
        let input = "See %%open and [[link]]\n";
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].0.contains("[[link]]"));
    }

    #[test]
    fn comment_on_first_line() {
        let input = md!(r"
%%
hidden
%%
Visible
");
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, "Visible");
    }

    #[test]
    fn empty_inline_comment() {
        let input = "before %%%% after\n";
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 1);
        // %%%% = open %% + close %% with empty content, all replaced with spaces
        assert!(!lines[0].0.contains("%%"));
        assert!(lines[0].0.contains("before"));
        assert!(lines[0].0.contains("after"));
    }

    #[test]
    fn nested_percent_signs_in_inline_comment() {
        let input = "a %%content with % inside%% b\n";
        let lines = collect_lines(input);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].0.contains("content"));
        assert!(lines[0].0.contains("a "));
        assert!(lines[0].0.contains(" b"));
    }

    // --- Comment block tests (multi-visitor scanner) ---

    #[test]
    fn multi_visitor_skips_comment_block() {
        let input = md!(r"
Line 1
%%
commented [[link]]
- [ ] hidden task
%%
Line 6
");
        let mut body = BodyCollector::new();
        scan_reader_multi(input.as_bytes(), &mut [&mut body]).unwrap();

        assert_eq!(body.lines.len(), 2);
        assert_eq!(body.lines[0].0, "Line 1");
        assert_eq!(body.lines[0].1, 1);
        assert_eq!(body.lines[1].0, "Line 6");
        assert_eq!(body.lines[1].1, 6);
    }

    #[test]
    fn multi_visitor_comment_inside_fence_ignored() {
        let input = md!(r"
Line 1
```
%%
not a comment
%%
```
Line 8
");
        let mut body = BodyCollector::new();
        let mut fences = FenceCounter::new();
        scan_reader_multi(input.as_bytes(), &mut [&mut body, &mut fences]).unwrap();

        assert_eq!(body.lines.len(), 2);
        assert_eq!(body.lines[0].0, "Line 1");
        assert_eq!(body.lines[1].0, "Line 8");

        // Code fence events should still fire normally
        assert_eq!(fences.opens.len(), 1);
        assert_eq!(fences.closes.len(), 1);
    }

    #[test]
    fn multi_visitor_fence_inside_comment_ignored() {
        let input = md!(r"
Line 1
%%
```rust
not code
```
%%
Line 8
");
        let mut body = BodyCollector::new();
        let mut fences = FenceCounter::new();
        scan_reader_multi(input.as_bytes(), &mut [&mut body, &mut fences]).unwrap();

        assert_eq!(body.lines.len(), 2);
        assert_eq!(body.lines[0].0, "Line 1");
        assert_eq!(body.lines[1].0, "Line 8");

        // No fence events — the ``` lines were inside a comment
        assert_eq!(fences.opens.len(), 0);
        assert_eq!(fences.closes.len(), 0);
    }

    #[test]
    fn multi_visitor_inline_comment_stripped() {
        let input = "See %%[[hidden]]%% and [[visible]]\n";
        let mut body = BodyCollector::new();
        scan_reader_multi(input.as_bytes(), &mut [&mut body]).unwrap();

        assert_eq!(body.lines.len(), 1);
        assert!(!body.lines[0].0.contains("hidden"));
        assert!(body.lines[0].0.contains("[[visible]]"));
    }

    // --- is_comment_fence unit tests ---

    #[test]
    fn is_comment_fence_basic() {
        assert!(is_comment_fence("%%"));
        assert!(is_comment_fence("  %%  "));
        assert!(is_comment_fence("\t%%"));
    }

    #[test]
    fn is_comment_fence_rejects_inline() {
        assert!(!is_comment_fence("%%inline%%"));
        assert!(!is_comment_fence("text %% more"));
        assert!(!is_comment_fence("%%content"));
        assert!(!is_comment_fence("content%%"));
    }

    // --- strip_inline_comments unit tests ---

    #[test]
    fn strip_inline_comments_no_change() {
        let line = "no comments here";
        let result = strip_inline_comments(line);
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result.as_ref(), line);
    }

    #[test]
    fn strip_inline_comments_basic() {
        let result = strip_inline_comments("a %%hidden%% b");
        assert_eq!(result.as_ref(), "a            b");
    }

    #[test]
    fn strip_inline_comments_multiple() {
        let result = strip_inline_comments("%%a%% mid %%b%%");
        assert!(!result.contains('a'));
        assert!(result.contains("mid"));
        assert!(!result.contains('b'));
    }

    #[test]
    fn strip_inline_comments_unmatched() {
        let result = strip_inline_comments("a %%open");
        assert_eq!(result.as_ref(), "a %%open");
    }

    #[test]
    fn strip_inline_comments_trailing_double_percent() {
        // Trailing `%%` with nothing after it looks like a block fence marker,
        // not an inline comment opener — leave it as-is.
        let result = strip_inline_comments("text%%");
        assert_eq!(result.as_ref(), "text%%");
    }

    #[test]
    fn strip_inline_comments_triple_percent() {
        // `%%%` = opening `%%` + lone `%` — no matching close, treated as literal.
        let result = strip_inline_comments("a %%% b");
        assert_eq!(result.as_ref(), "a %%% b");
    }
}
