#![allow(clippy::missing_errors_doc)]

mod fence;
mod frontmatter;
mod strip;
mod visitor;

pub use fence::{FenceTracker, extract_fence_language};
pub use strip::{strip_inline_code, strip_inline_comments};
pub use visitor::FileVisitor;

pub(crate) use fence::{detect_opening_fence, is_closing_fence};
pub(crate) use frontmatter::FrontmatterCollector;
pub(crate) use strip::is_comment_fence;

#[cfg(test)]
pub(crate) use visitor::{scan_file, scan_reader};

use crate::frontmatter::hyalo_options;
use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Controls whether the scanner should continue or stop early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanAction {
    Continue,
    Stop,
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
        // Empty file — deliver empty frontmatter.
        // Clone for all but the last visitor; take ownership for the last.
        let mut empty: IndexMap<String, Value> = IndexMap::new();
        let last = visitors.len() - 1;
        for (i, v) in visitors.iter_mut().enumerate() {
            let props = if i == last {
                std::mem::take(&mut empty)
            } else {
                empty.clone()
            };
            if v.on_frontmatter(props) == ScanAction::Stop {
                active[i] = false;
            }
        }
        return Ok(());
    }
    line_num += 1;

    let first_trimmed = buf.trim_end_matches(['\n', '\r']).to_owned();

    // Try to parse frontmatter
    let any_needs_fm = visitors.iter().any(|v| v.needs_frontmatter());
    let (mut fm_props, fm_lines) = if first_trimmed.trim() == "---" {
        const MAX_FRONTMATTER_LINES: usize = 200;
        const MAX_FRONTMATTER_BYTES: usize = 8 * 1024;

        // Read past frontmatter lines, optionally collecting YAML content
        let mut yaml = if any_needs_fm {
            Some(String::new())
        } else {
            None
        };
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
            // Content line count is fm_line_count - 1 (excludes the opening `---`).
            // Apply the line-count limit unconditionally so that files with huge
            // frontmatter are rejected even when no visitor needs the YAML content.
            if fm_line_count - 1 > MAX_FRONTMATTER_LINES {
                anyhow::bail!(
                    "frontmatter too large (no closing `---` found within {MAX_FRONTMATTER_LINES} lines / {MAX_FRONTMATTER_BYTES} bytes)"
                );
            }
            if let Some(ref mut y) = yaml {
                if y.len() + trimmed.len() > MAX_FRONTMATTER_BYTES {
                    anyhow::bail!(
                        "frontmatter too large (no closing `---` found within {MAX_FRONTMATTER_LINES} lines / {MAX_FRONTMATTER_BYTES} bytes)"
                    );
                }
                y.push_str(trimmed);
                y.push('\n');
            }
        }
        if !found_close {
            anyhow::bail!("unclosed frontmatter (no closing `---` found)");
        }
        let props: IndexMap<String, Value> = match yaml {
            Some(ref y) if !y.trim().is_empty() => {
                serde_saphyr::from_str_with_options(y, hyalo_options())
                    .context("failed to parse YAML frontmatter")?
            }
            _ => IndexMap::new(),
        };
        (props, fm_line_count)
    } else {
        (IndexMap::new(), 0usize)
    };

    // Deliver frontmatter to all visitors.
    // Clone for all but the last visitor; take ownership for the last.
    let last = visitors.len() - 1;
    for (i, v) in visitors.iter_mut().enumerate() {
        let props = if i == last {
            std::mem::take(&mut fm_props)
        } else {
            fm_props.clone()
        };
        if v.on_frontmatter(props) == ScanAction::Stop || !v.needs_body() {
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
        let (n, truncated) = read_line_capped(&mut reader, &mut buf, MAX_BODY_LINE_BYTES)
            .context("failed to read line")?;
        if n == 0 {
            break;
        }
        line_num += 1;
        if truncated {
            // Line either exceeded the per-line byte limit or contained
            // invalid UTF-8 — skip it entirely to prevent OOM on files with
            // no newlines (e.g. minified HTML/JSON accidentally placed in the
            // vault) and to avoid propagating malformed encoding. The line
            // counter still advances so that downstream line numbers remain
            // correct.
            continue;
        }
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

/// Per-line byte ceiling for body scanning.
///
/// Lines longer than this are skipped (the line counter still advances).
/// 1 MiB is ample for any real Markdown line; files with no newlines (e.g.
/// accidentally-added minified blobs) would otherwise exhaust memory.
const MAX_BODY_LINE_BYTES: usize = 1024 * 1024; // 1 MiB

/// Read one newline-terminated line into `buf`, but stop after `limit` bytes.
///
/// Returns `(bytes_consumed, truncated)`.  When `truncated` is `true` the
/// reader is positioned just after where the logical line ended (i.e. excess
/// bytes are drained until the next `\n` or EOF), and the caller should treat
/// the line as skipped.
fn read_line_capped<R: BufRead>(
    reader: &mut R,
    buf: &mut String,
    limit: usize,
) -> std::io::Result<(usize, bool)> {
    let mut total = 0usize;
    loop {
        // Inspect the internal buffer to find a newline and measure how many
        // bytes are available.  We extract the indices we need *before*
        // releasing the borrow so that we can then call `consume`.
        let (newline_pos, chunk_len) = loop {
            match reader.fill_buf() {
                Ok([]) => return Ok((total, false)),
                Ok(b) => {
                    let nl = b.iter().position(|&byte| byte == b'\n');
                    let len = b.len();
                    break (nl, len);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}

                Err(e) => return Err(e),
            }
        };

        // How many bytes we will consume from the reader this iteration.
        let consume = match newline_pos {
            Some(pos) => pos + 1, // include the '\n'
            None => chunk_len,
        };

        if buf.len() >= limit {
            // Already over quota — just drain.
            reader.consume(consume);
            total += consume;
            if newline_pos.is_some() {
                return Ok((total, true));
            }
            drain_until_newline(reader)?;
            return Ok((total, true));
        }

        // Within quota: copy up to `to_copy` bytes into a temporary Vec so we
        // can release the `fill_buf` borrow before calling `consume`.
        let remaining_quota = limit - buf.len();
        let to_copy = consume.min(remaining_quota);

        // Copy the bytes out while the immutable borrow is still live.
        let chunk: Vec<u8> = {
            let available = reader.fill_buf()?;
            available[..to_copy].to_vec()
        };
        // Now release the borrow and advance the reader.
        reader.consume(consume);
        total += consume;

        // Validate UTF-8; treat invalid bytes as a truncated/skipped line.
        if let Ok(s) = std::str::from_utf8(&chunk) {
            buf.push_str(s);
        } else {
            if newline_pos.is_none() {
                drain_until_newline(reader)?;
            }
            return Ok((total, true));
        }

        let truncated = to_copy < consume;
        if newline_pos.is_some() {
            // The newline was within the consumed range — line is complete.
            // If quota was hit before the newline, we already consumed past it,
            // so no further draining is needed.
            return Ok((total, truncated));
        }
        if truncated {
            // Quota hit on a chunk with no newline — drain the rest of the line.
            drain_until_newline(reader)?;
            return Ok((total, true));
        }
    }
}

/// Consume bytes from `reader` until (and including) a `\n`, or until EOF.
fn drain_until_newline<R: BufRead>(reader: &mut R) -> std::io::Result<()> {
    loop {
        let available = match reader.fill_buf() {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if available.is_empty() {
            return Ok(());
        }
        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            reader.consume(pos + 1);
            return Ok(());
        }
        let n = available.len();
        reader.consume(n);
    }
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
        } else {
            // Deliver code block content lines to interested visitors
            for (i, v) in visitors.iter_mut().enumerate() {
                if active[i] && v.on_code_block_line(line, line_num) == ScanAction::Stop {
                    active[i] = false;
                }
            }
        }
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

    // Normal body line — strip inline code spans first, then inline comments.
    // Inline code must be removed before comment stripping so that `%%` inside
    // a backtick span is not mistakenly treated as a comment delimiter.
    //
    // `line` (raw) is passed alongside `cleaned` so visitors that parse heading
    // text can use the original content (preserving code spans in headings).
    let cleaned = strip_inline_code(line);
    let cleaned = strip_inline_comments(&cleaned);
    for (i, v) in visitors.iter_mut().enumerate() {
        if active[i] && v.on_body_line(line, &cleaned, line_num) == ScanAction::Stop {
            active[i] = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

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
        fn on_body_line(&mut self, _raw: &str, cleaned: &str, line_num: usize) -> ScanAction {
            self.lines.push((cleaned.to_owned(), line_num));
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
    fn multi_visitor_malformed_yaml_returns_error() {
        let input = b"---\n: invalid [[[{\n---\nBody\n";
        let mut fm = FrontmatterCollector::new(true);
        let result = scan_reader_multi(input.as_slice(), &mut [&mut fm]);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("failed to parse YAML frontmatter"),
            "unexpected error: {err_msg}"
        );
    }

    #[test]
    fn multi_visitor_frontmatter_exceeds_budget_returns_error() {
        // Build a frontmatter block with 201 content lines and no closing `---`,
        // which exceeds the 200-line budget enforced by scan_reader_multi.
        let mut input = String::from("---\n");
        for i in 0..201usize {
            let _ = writeln!(input, "k{i}: v");
        }
        // Deliberately omit the closing `---` so the budget is hit before EOF.
        let mut fm = FrontmatterCollector::new(true);
        let result = scan_reader_multi(input.as_bytes(), &mut [&mut fm]);
        assert!(result.is_err(), "expected error for oversized frontmatter");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("frontmatter too large"),
            "unexpected error: {err_msg}"
        );
    }

    #[test]
    fn frontmatter_line_limit_enforced_when_no_visitor_needs_frontmatter() {
        // Regression test for DoS gap: the line-count limit must fire even when
        // every visitor has needs_frontmatter() = false (yaml accumulation is
        // skipped in that path, which previously caused the guard to be bypassed).
        struct BodyOnly {
            lines: Vec<String>,
        }
        impl FileVisitor for BodyOnly {
            fn on_body_line(&mut self, raw: &str, _cleaned: &str, _line_num: usize) -> ScanAction {
                self.lines.push(raw.to_owned());
                ScanAction::Continue
            }
            fn needs_frontmatter(&self) -> bool {
                false
            }
        }

        // 201 content lines, no closing `---` — must exceed the 200-line budget.
        let mut input = String::from("---\n");
        for i in 0..201usize {
            let _ = writeln!(input, "k{i}: v");
        }
        let mut v = BodyOnly { lines: Vec::new() };
        let result = scan_reader_multi(input.as_bytes(), &mut [&mut v]);
        assert!(
            result.is_err(),
            "expected error for oversized frontmatter even with needs_frontmatter=false"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("frontmatter too large"),
            "unexpected error: {err_msg}"
        );
    }

    #[test]
    fn multi_visitor_unclosed_frontmatter_returns_error() {
        // File starts with `---` but EOF is reached without a closing `---`.
        // This must error rather than silently returning an empty property map.
        let input = "---\ntitle: Test\n";
        let mut fm = FrontmatterCollector::new(true);
        let result = scan_reader_multi(input.as_bytes(), &mut [&mut fm]);
        assert!(result.is_err(), "expected error for unclosed frontmatter");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("unclosed frontmatter"),
            "unexpected error: {err_msg}"
        );
    }

    #[test]
    fn needs_frontmatter_false_skips_yaml_parse() {
        // Malformed YAML that would fail serde_saphyr if parsed,
        // but a body-only visitor with needs_frontmatter=false should succeed.
        struct BodyOnly {
            lines: Vec<(String, usize)>,
        }
        impl FileVisitor for BodyOnly {
            fn on_body_line(&mut self, raw: &str, _cleaned: &str, line_num: usize) -> ScanAction {
                self.lines.push((raw.to_owned(), line_num));
                ScanAction::Continue
            }
            fn needs_frontmatter(&self) -> bool {
                false
            }
        }

        let input = b"---\n: invalid [[[{\ntags: !!bad\n---\nBody line\n";
        let mut v = BodyOnly { lines: Vec::new() };
        scan_reader_multi(input.as_slice(), &mut [&mut v]).unwrap();
        assert_eq!(v.lines.len(), 1);
        assert_eq!(v.lines[0].0, "Body line");
        assert_eq!(v.lines[0].1, 5);
    }

    #[test]
    fn needs_frontmatter_mixed_visitors() {
        // One visitor needs frontmatter, one doesn't — YAML must still be parsed.
        struct BodyOnly {
            lines: Vec<String>,
        }
        impl FileVisitor for BodyOnly {
            fn on_body_line(&mut self, raw: &str, _cleaned: &str, _line_num: usize) -> ScanAction {
                self.lines.push(raw.to_owned());
                ScanAction::Continue
            }
            fn needs_frontmatter(&self) -> bool {
                false
            }
        }

        let input = md!(r"
---
title: Hello
---
Body
");
        let mut fm = FrontmatterCollector::new(true);
        let mut body = BodyOnly { lines: Vec::new() };
        scan_reader_multi(input.as_bytes(), &mut [&mut fm, &mut body]).unwrap();

        // Frontmatter visitor still gets parsed props
        let props = fm.into_props();
        assert_eq!(props.get("title").unwrap().as_str(), Some("Hello"));
        // Body visitor gets the body
        assert_eq!(body.lines, vec!["Body"]);
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

    // --- on_code_block_line tests ---

    /// Test visitor that collects code block body lines.
    struct CodeBlockCollector {
        lines: Vec<(String, usize)>,
    }

    impl CodeBlockCollector {
        fn new() -> Self {
            Self { lines: Vec::new() }
        }
    }

    impl FileVisitor for CodeBlockCollector {
        fn on_code_block_line(&mut self, raw: &str, line_num: usize) -> ScanAction {
            self.lines.push((raw.to_owned(), line_num));
            ScanAction::Continue
        }
    }

    #[test]
    fn code_block_line_called_for_lines_inside_fence() {
        let input = md!(r"
Line 1
```rust
let x = 1;
let y = 2;
```
Line 6
");
        let mut body = BodyCollector::new();
        let mut code = CodeBlockCollector::new();
        scan_reader_multi(input.as_bytes(), &mut [&mut body, &mut code]).unwrap();

        // Body visitor sees only non-code-block lines
        assert_eq!(body.lines.len(), 2);
        assert_eq!(body.lines[0].0, "Line 1");
        assert_eq!(body.lines[1].0, "Line 6");

        // Code block visitor sees interior lines (not the fence delimiters)
        assert_eq!(code.lines.len(), 2);
        assert_eq!(code.lines[0], ("let x = 1;".to_string(), 3));
        assert_eq!(code.lines[1], ("let y = 2;".to_string(), 4));
    }

    #[test]
    fn code_block_line_not_called_for_fence_delimiters() {
        // Opening and closing fence lines are NOT delivered via on_code_block_line
        let input = "```\ncode\n```\n";
        let mut code = CodeBlockCollector::new();
        scan_reader_multi(input.as_bytes(), &mut [&mut code]).unwrap();
        assert_eq!(code.lines.len(), 1);
        assert_eq!(code.lines[0].0, "code");
    }

    #[test]
    fn code_block_line_not_called_inside_comment_block() {
        // Lines inside Obsidian `%%` comment blocks are fully suppressed
        let input = md!(r"
%%
```
inside comment
```
%%
after
");
        let mut code = CodeBlockCollector::new();
        scan_reader_multi(input.as_bytes(), &mut [&mut code]).unwrap();
        assert!(code.lines.is_empty());
    }

    #[test]
    fn default_visitor_ignores_code_block_lines() {
        // A visitor that only implements on_body_line must not see code block lines
        let input = md!(r"
normal
```
code only
```
");
        let mut body = BodyCollector::new();
        scan_reader_multi(input.as_bytes(), &mut [&mut body]).unwrap();
        // "code only" must NOT appear in body lines
        assert_eq!(body.lines.len(), 1);
        assert_eq!(body.lines[0].0, "normal");
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
        assert!(matches!(result, std::borrow::Cow::Borrowed(_)));
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

    // --- per-line byte limit tests ---

    #[test]
    fn body_line_limit_skips_oversized_line() {
        // Build an input where the second line is oversized (no newline) and
        // normal lines surround it.
        let normal_before = "before oversized line\n";
        let huge: String = "x".repeat(MAX_BODY_LINE_BYTES + 1);
        let normal_after = "\nafter oversized line\n";
        let input = format!("{normal_before}{huge}{normal_after}");

        let lines = collect_lines(&input);
        // Only the normal lines should be visible; the huge line is skipped.
        assert!(
            lines.iter().all(|(t, _)| t != &huge),
            "oversized line must be dropped"
        );
        assert!(
            lines.iter().any(|(t, _)| t == "before oversized line"),
            "normal line before must survive"
        );
        assert!(
            lines.iter().any(|(t, _)| t == "after oversized line"),
            "normal line after must survive"
        );
    }

    #[test]
    fn body_line_limit_exact_boundary_passes() {
        // A line exactly at the limit (without newline) should be accepted.
        let exactly: String = "y".repeat(MAX_BODY_LINE_BYTES);
        let input = format!("{exactly}\nnext\n");
        let lines = collect_lines(&input);
        assert_eq!(lines[0].0, exactly, "line at limit must pass through");
        assert_eq!(lines[1].0, "next");
    }
}
