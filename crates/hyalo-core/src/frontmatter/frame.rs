//! Shared, bounded framing for Markdown documents with YAML frontmatter.
//!
//! This module owns delimiter recognition and byte/line accounting.  Parsed
//! YAML values deliberately live elsewhere: a frame describes authored source
//! bytes and can therefore be reused by scanners, readers, lint and writers
//! without losing offsets or formatting.

use std::io::BufRead;
use std::ops::Range;

use anyhow::{Context, Result};

use super::parse::opening_delimiter;
use super::{FrontmatterError, MAX_FRONTMATTER_BYTES, MAX_FRONTMATTER_LINES};

// Delimiter whitespace is supported but independently bounded so it cannot be
// used to bypass the framing allocation limit. This allowance does not reduce
// the YAML content budget.
const MAX_DELIMITER_LINE_BYTES: usize = MAX_FRONTMATTER_BYTES + 8;

/// Newline used by the opening frontmatter delimiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewlineStyle {
    /// Unix line feed.
    Lf,
    /// Windows carriage-return plus line feed.
    CrLf,
}

impl NewlineStyle {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

/// A non-fatal property of the authored frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramingDiagnostic {
    /// At least one line in the frontmatter uses a different newline from the
    /// opening delimiter. Writers may preserve the source or report that a
    /// fallback serialization will normalize it.
    MixedNewlines,
}

/// Exact source ranges for a recognized YAML frontmatter block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontmatterFrame {
    opening: Range<usize>,
    content: Range<usize>,
    closing: Range<usize>,
    opening_trailing_ws: Range<usize>,
    closing_trailing_ws: Range<usize>,
    closing_has_newline: bool,
}

impl FrontmatterFrame {
    /// Opening delimiter line, including its line ending when present.
    #[must_use]
    pub fn opening_span(&self) -> Range<usize> {
        self.opening.clone()
    }

    /// YAML content between the delimiter lines, including authored line
    /// endings and excluding both delimiters.
    #[must_use]
    pub fn content_span(&self) -> Range<usize> {
        self.content.clone()
    }

    /// Closing delimiter line, including its line ending when present.
    #[must_use]
    pub fn closing_span(&self) -> Range<usize> {
        self.closing.clone()
    }

    /// Whitespace authored after the opening `---`.
    #[must_use]
    pub fn opening_trailing_ws_span(&self) -> Range<usize> {
        self.opening_trailing_ws.clone()
    }

    /// Whitespace authored after the closing `---`.
    #[must_use]
    pub fn closing_trailing_ws_span(&self) -> Range<usize> {
        self.closing_trailing_ws.clone()
    }

    #[must_use]
    pub fn closing_has_newline(&self) -> bool {
        self.closing_has_newline
    }
}

/// The syntax frame of one Markdown document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentFrame {
    frontmatter: Option<FrontmatterFrame>,
    body_offset: usize,
    newline: NewlineStyle,
    has_bom: bool,
    diagnostics: Vec<FramingDiagnostic>,
}

impl DocumentFrame {
    /// Frame an in-memory document with the normal frontmatter limits.
    pub fn parse(source: &str) -> Result<Self> {
        frame_prefix(source.as_bytes(), false)
    }

    /// Recognized frontmatter source ranges, if present.
    #[must_use]
    pub fn frontmatter(&self) -> Option<&FrontmatterFrame> {
        self.frontmatter.as_ref()
    }

    /// First byte of the Markdown body. Zero means no frontmatter.
    #[must_use]
    pub fn body_offset(&self) -> usize {
        self.body_offset
    }

    #[must_use]
    pub fn newline_style(&self) -> NewlineStyle {
        self.newline
    }

    #[must_use]
    pub fn has_bom(&self) -> bool {
        self.has_bom
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[FramingDiagnostic] {
        &self.diagnostics
    }

    /// YAML bytes for this frame.
    #[must_use]
    pub fn yaml<'a>(&self, source: &'a [u8]) -> Option<&'a [u8]> {
        self.frontmatter
            .as_ref()
            .and_then(|fm| source.get(fm.content.clone()))
    }

    /// Markdown body bytes for this frame.
    #[must_use]
    pub fn body<'a>(&self, source: &'a [u8]) -> Option<&'a [u8]> {
        source.get(self.body_offset..)
    }
}

/// Bounded bytes consumed while framing a buffered reader.
///
/// When frontmatter exists, `bytes` ends immediately after its closing line.
/// With no frontmatter it contains at most the bounded first line. The reader
/// may have prefetched more into its own internal buffer, but no body bytes are
/// consumed after a complete frame.
#[derive(Debug)]
pub struct ReadFrame {
    frame: DocumentFrame,
    bytes: Vec<u8>,
    first_line_complete: bool,
}

impl ReadFrame {
    #[must_use]
    pub fn frame(&self) -> &DocumentFrame {
        &self.frame
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether the captured first body line is complete. This is always true
    /// for a recognized frontmatter frame. Body-oriented adapters use this to
    /// emit their normal oversized-line placeholder without allocating the
    /// discarded suffix.
    #[must_use]
    pub fn first_line_complete(&self) -> bool {
        self.first_line_complete
    }

    #[must_use]
    pub fn into_parts(self) -> (DocumentFrame, Vec<u8>) {
        (self.frame, self.bytes)
    }
}

/// Frame and capture only the bounded document prefix needed to decide and
/// parse frontmatter. This never uses `BufRead::read_line`, whose allocation
/// occurs before a caller can enforce a line limit.
pub fn read_frame<R: BufRead>(reader: &mut R) -> Result<ReadFrame> {
    // The opening line is not YAML content, but accepting unbounded trailing
    // whitespace there would defeat the framing allocation bound. The normal
    // frontmatter byte limit is a generous and deterministic cap.
    let mut bytes = Vec::new();
    let opening = read_opening_line(reader, &mut bytes, MAX_DELIMITER_LINE_BYTES, false)?;
    if bytes.is_empty() {
        return Ok(ReadFrame {
            frame: no_frontmatter(),
            bytes,
            first_line_complete: true,
        });
    }

    if !opening.opens {
        return Ok(ReadFrame {
            frame: no_frontmatter(),
            bytes,
            first_line_complete: true,
        });
    }

    read_open_frame(reader, bytes)
}

/// Frame a document while capturing and draining its complete first logical
/// line up to `capture_limit` when it is body text. Recognition and delimiter
/// budgets are identical to [`read_frame`]; this adapter exists for streaming
/// body consumers that must retain line numbering and placeholder behavior.
pub fn read_frame_for_body<R: BufRead>(reader: &mut R, capture_limit: usize) -> Result<ReadFrame> {
    if capture_limit < MAX_DELIMITER_LINE_BYTES {
        return Err(frontmatter_error(format!(
            "body capture limit {capture_limit} is smaller than the {MAX_DELIMITER_LINE_BYTES}-byte framing limit"
        )));
    }
    let storage_limit = capture_limit.saturating_add(1); // optional terminating LF
    let mut bytes = Vec::new();
    let opening = read_opening_line(reader, &mut bytes, storage_limit, true)?;
    if bytes.is_empty() {
        return Ok(ReadFrame {
            frame: no_frontmatter(),
            bytes,
            first_line_complete: true,
        });
    }
    if !opening.opens {
        return Ok(ReadFrame {
            frame: no_frontmatter(),
            bytes,
            first_line_complete: opening.captured_complete,
        });
    }
    read_open_frame(reader, bytes)
}

/// Continue framing after a caller has already consumed the complete first
/// line. This keeps legacy streaming adapters on the same delimiter and
/// budget implementation as [`read_frame`] without asking them to seek.
pub(super) fn read_frame_after_first_line<R: BufRead>(
    reader: &mut R,
    first_line: &str,
) -> Result<ReadFrame> {
    if opening_delimiter(first_line).is_none() {
        return Ok(ReadFrame {
            frame: no_frontmatter(),
            bytes: first_line.as_bytes().to_vec(),
            first_line_complete: true,
        });
    }
    if first_line.len() > MAX_DELIMITER_LINE_BYTES {
        return Err(frontmatter_too_large());
    }
    read_open_frame(reader, first_line.as_bytes().to_vec())
}

fn read_open_frame<R: BufRead>(reader: &mut R, mut bytes: Vec<u8>) -> Result<ReadFrame> {
    let first_len = bytes.len();
    let mut content_lines = 0usize;
    let mut content_bytes = 0usize;
    loop {
        let line_start = bytes.len();
        let n = read_line_bounded(reader, &mut bytes, MAX_DELIMITER_LINE_BYTES)?;
        if n == 0 {
            if content_lines == 0 {
                bytes.truncate(first_len);
                return Ok(ReadFrame {
                    frame: no_frontmatter(),
                    bytes,
                    first_line_complete: true,
                });
            }
            return Err(frontmatter_error(
                "unclosed frontmatter: file starts with `---` but no closing `---` was found",
            ));
        }
        let raw =
            std::str::from_utf8(&bytes[line_start..]).context("frontmatter is not valid UTF-8")?;
        if super::is_closing_delimiter(raw.trim_end_matches(['\n', '\r'])) {
            let frame = frame_prefix(&bytes, true)?;
            return Ok(ReadFrame {
                frame,
                bytes,
                first_line_complete: true,
            });
        }
        content_lines += 1;
        content_bytes += n;
        check_limits(content_bytes, content_lines)?;
    }
}

/// Consume only enough of the first line to decide whether it can be the
/// opening delimiter. Ordinary body lines normally cost one byte; a line that
/// begins with the exact delimiter prefix is bounded while its allowed
/// trailing whitespace is examined.
struct OpeningRead {
    opens: bool,
    captured_complete: bool,
}

fn read_opening_line<R: BufRead>(
    reader: &mut R,
    out: &mut Vec<u8>,
    capture_limit: usize,
    finish_body_line: bool,
) -> Result<OpeningRead> {
    const BOM_BYTES: &[u8] = "\u{feff}".as_bytes();
    let mut expected = 0usize;
    let mut after_dashes = false;
    let mut saw_cr = false;
    let mut total = 0usize;
    let mut candidate = true;
    loop {
        let available = reader.fill_buf().context("failed to read first line")?;
        if available.is_empty() {
            if candidate && after_dashes && !saw_cr && total > MAX_DELIMITER_LINE_BYTES {
                return Err(frontmatter_too_large());
            }
            return Ok(OpeningRead {
                opens: false,
                captured_complete: total <= capture_limit,
            });
        }
        let byte = available[0];
        reader.consume(1);
        total += 1;
        if out.len() < capture_limit {
            out.push(byte);
        }
        if !candidate {
            if !finish_body_line || byte == b'\n' {
                return Ok(OpeningRead {
                    opens: false,
                    captured_complete: finish_body_line && byte == b'\n' && total <= capture_limit,
                });
            }
            continue;
        }

        if expected < BOM_BYTES.len() && out[0] == BOM_BYTES[0] {
            if byte != BOM_BYTES[expected] {
                candidate = false;
                if !finish_body_line {
                    return Ok(OpeningRead {
                        opens: false,
                        captured_complete: false,
                    });
                }
                continue;
            }
            expected += 1;
            if expected < BOM_BYTES.len() {
                continue;
            }
            expected = BOM_BYTES.len();
            continue;
        }

        let prefix_start = usize::from(out.starts_with(BOM_BYTES)) * BOM_BYTES.len();
        let dash_pos = out.len().saturating_sub(prefix_start + 1);
        if dash_pos < 3 {
            if byte != b'-' {
                candidate = false;
                if !finish_body_line {
                    return Ok(OpeningRead {
                        opens: false,
                        captured_complete: false,
                    });
                }
                continue;
            }
            if dash_pos == 2 {
                after_dashes = true;
            }
            continue;
        }

        if saw_cr {
            if byte == b'\n' {
                if total > MAX_DELIMITER_LINE_BYTES {
                    return Err(frontmatter_too_large());
                }
                return Ok(OpeningRead {
                    opens: true,
                    captured_complete: true,
                });
            }
            candidate = false;
            if !finish_body_line {
                return Ok(OpeningRead {
                    opens: false,
                    captured_complete: false,
                });
            }
            continue;
        }
        match byte {
            b' ' | b'\t' => {}
            b'\n' => {
                if total > MAX_DELIMITER_LINE_BYTES {
                    return Err(frontmatter_too_large());
                }
                return Ok(OpeningRead {
                    opens: true,
                    captured_complete: true,
                });
            }
            b'\r' => saw_cr = true,
            _ => {
                candidate = false;
                if !finish_body_line {
                    return Ok(OpeningRead {
                        opens: false,
                        captured_complete: false,
                    });
                }
            }
        }
    }
}

fn read_line_bounded<R: BufRead>(reader: &mut R, out: &mut Vec<u8>, limit: usize) -> Result<usize> {
    let start = out.len();
    loop {
        let available = reader.fill_buf().context("failed to read line")?;
        if available.is_empty() {
            return Ok(out.len() - start);
        }
        let take = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |pos| pos + 1);
        if out.len() - start + take > limit {
            return Err(frontmatter_too_large());
        }
        out.extend_from_slice(&available[..take]);
        reader.consume(take);
        if out.last() == Some(&b'\n') {
            return Ok(out.len() - start);
        }
    }
}

fn frame_prefix(source: &[u8], require_closed: bool) -> Result<DocumentFrame> {
    if source.is_empty() {
        return Ok(no_frontmatter());
    }
    let first_end = next_line_end(source, 0);
    let Some(first) = std::str::from_utf8(&source[..first_end])
        .ok()
        .and_then(opening_delimiter)
    else {
        return Ok(no_frontmatter());
    };
    if first_end > MAX_DELIMITER_LINE_BYTES {
        return Err(frontmatter_too_large());
    }
    if first_end == source.len() {
        if require_closed {
            return Err(frontmatter_error(
                "unclosed frontmatter: file starts with `---` but no closing `---` was found",
            ));
        }
        return Ok(no_frontmatter());
    }

    let newline = if source[..first_end].ends_with(b"\r\n") {
        NewlineStyle::CrLf
    } else {
        NewlineStyle::Lf
    };
    let bom_len = usize::from(first.has_bom) * '\u{feff}'.len_utf8();
    let first_without_eol = trim_eol(&source[..first_end]);
    let opening_ws = bom_len + 3..first_without_eol.len();
    let mut pos = first_end;
    let mut content_lines = 0usize;
    let mut diagnostics = Vec::new();
    loop {
        if pos >= source.len() {
            return Err(frontmatter_error(
                "unclosed frontmatter: file starts with `---` but no closing `---` was found",
            ));
        }
        let end = next_line_end(source, pos);
        let line = &source[pos..end];
        let line_no_eol = trim_eol(line);
        if std::str::from_utf8(line_no_eol).is_ok_and(super::is_closing_delimiter) {
            if line.len() > MAX_DELIMITER_LINE_BYTES {
                return Err(frontmatter_too_large());
            }
            let content_bytes = pos - first_end;
            check_limits(content_bytes, content_lines)?;
            if line_has_newline(line) && line_style(line) != newline {
                diagnostics.push(FramingDiagnostic::MixedNewlines);
            }
            let close_ws_start = pos + 3;
            let close_ws_end = pos + line_no_eol.len();
            return Ok(DocumentFrame {
                frontmatter: Some(FrontmatterFrame {
                    opening: 0..first_end,
                    content: first_end..pos,
                    closing: pos..end,
                    opening_trailing_ws: opening_ws,
                    closing_trailing_ws: close_ws_start..close_ws_end,
                    closing_has_newline: line_has_newline(line),
                }),
                body_offset: end,
                newline,
                has_bom: first.has_bom,
                diagnostics,
            });
        }
        content_lines += 1;
        check_limits(end - first_end, content_lines)?;
        if line_has_newline(line)
            && line_style(line) != newline
            && !diagnostics.contains(&FramingDiagnostic::MixedNewlines)
        {
            diagnostics.push(FramingDiagnostic::MixedNewlines);
        }
        pos = end;
    }
}

fn next_line_end(source: &[u8], start: usize) -> usize {
    source[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(source.len(), |pos| start + pos + 1)
}

fn trim_eol(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\n")
        .unwrap_or(line)
        .strip_suffix(b"\r")
        .unwrap_or_else(|| line.strip_suffix(b"\n").unwrap_or(line))
}

fn line_has_newline(line: &[u8]) -> bool {
    line.ends_with(b"\n")
}

fn line_style(line: &[u8]) -> NewlineStyle {
    if line.ends_with(b"\r\n") {
        NewlineStyle::CrLf
    } else {
        NewlineStyle::Lf
    }
}

fn check_limits(bytes: usize, lines: usize) -> Result<()> {
    if bytes > MAX_FRONTMATTER_BYTES || lines > MAX_FRONTMATTER_LINES {
        Err(frontmatter_too_large())
    } else {
        Ok(())
    }
}

pub(super) fn frontmatter_too_large() -> anyhow::Error {
    frontmatter_error(format!(
        "frontmatter too large (no closing `---` found within {MAX_FRONTMATTER_LINES} lines / {MAX_FRONTMATTER_BYTES} bytes); run `hyalo lint <file>` for details"
    ))
}

fn frontmatter_error(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(FrontmatterError(message.into()))
}

fn no_frontmatter() -> DocumentFrame {
    DocumentFrame {
        frontmatter: None,
        body_offset: 0,
        newline: NewlineStyle::Lf,
        has_bom: false,
        diagnostics: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor, Read};

    #[test]
    fn exact_ranges_bom_crlf_and_trailing_whitespace() {
        let source = "\u{feff}--- \r\ntitle: x\r\n---\t\r\nbody";
        let frame = DocumentFrame::parse(source).unwrap();
        let fm = frame.frontmatter().unwrap();
        assert_eq!(frame.newline_style(), NewlineStyle::CrLf);
        assert!(frame.has_bom());
        assert_eq!(frame.yaml(source.as_bytes()).unwrap(), b"title: x\r\n");
        assert_eq!(frame.body(source.as_bytes()).unwrap(), b"body");
        assert_eq!(&source.as_bytes()[fm.opening_trailing_ws_span()], b" ");
        assert_eq!(&source.as_bytes()[fm.closing_trailing_ws_span()], b"\t");
    }

    #[test]
    fn delimiter_like_key_is_content() {
        let source = "---\n---note: value\n---\nbody";
        let frame = DocumentFrame::parse(source).unwrap();
        assert_eq!(frame.yaml(source.as_bytes()).unwrap(), b"---note: value\n");
    }

    #[test]
    fn exact_byte_limit_is_inclusive() {
        let exact = format!("---\n{}\n---\n", "a".repeat(MAX_FRONTMATTER_BYTES - 1));
        assert!(DocumentFrame::parse(&exact).is_ok());
        let over = format!("---\n{}\n---\n", "a".repeat(MAX_FRONTMATTER_BYTES));
        assert!(DocumentFrame::parse(&over).is_err());
    }

    #[test]
    fn reader_and_slice_allow_closing_whitespace_without_spending_yaml_budget() {
        let source = format!(
            "---\n{}\n---                \nbody",
            "a".repeat(MAX_FRONTMATTER_BYTES - 1)
        );
        let parsed = DocumentFrame::parse(&source).unwrap();
        let mut reader = BufReader::with_capacity(7, Cursor::new(source.as_bytes()));
        let streamed = read_frame(&mut reader).unwrap();
        assert_eq!(streamed.frame(), &parsed);
        assert_eq!(streamed.frame().body_offset(), source.len() - 4);
    }

    #[test]
    fn reader_and_slice_agree_on_bounded_delimiters_and_eof_cr() {
        let huge_open = format!("---{}\nbody", " ".repeat(MAX_DELIMITER_LINE_BYTES));
        assert!(DocumentFrame::parse(&huge_open).is_err());
        assert!(read_frame(&mut BufReader::new(Cursor::new(&huge_open))).is_err());
        for source in ["---\r", "---\nkey: x\n---\r"] {
            let parsed = DocumentFrame::parse(source);
            let streamed = read_frame(&mut BufReader::new(Cursor::new(source)));
            assert_eq!(parsed.is_ok(), streamed.is_ok(), "{source:?}");
            if let (Ok(parsed), Ok(streamed)) = (parsed, streamed) {
                assert_eq!(&parsed, streamed.frame());
            }
        }
    }

    struct Counted<R> {
        inner: R,
        read: usize,
    }

    impl<R: Read> Read for Counted<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.inner.read(buf)?;
            self.read += n;
            Ok(n)
        }
    }

    #[test]
    fn buffered_frame_stops_before_large_body() {
        let source = format!("---\ntitle: x\n---\n{}", "b".repeat(128 * 1024));
        let counted = Counted {
            inner: Cursor::new(source.as_bytes()),
            read: 0,
        };
        // A one-byte buffer makes read-ahead explicit: `read_frame` consumes
        // exactly through the closing newline and does not demand body bytes.
        let mut reader = BufReader::with_capacity(1, counted);
        let framed = read_frame(&mut reader).unwrap();
        assert_eq!(framed.bytes(), b"---\ntitle: x\n---\n");
        assert_eq!(reader.get_ref().read, framed.bytes().len());
    }

    #[test]
    fn buffered_frame_may_prefetch_but_logically_leaves_body_unconsumed() {
        let source = b"---\ntitle: x\n---\nbody bytes";
        let counted = Counted {
            inner: Cursor::new(source),
            read: 0,
        };
        let mut reader = BufReader::with_capacity(8 * 1024, counted);
        let framed = read_frame(&mut reader).unwrap();
        assert_eq!(framed.bytes(), b"---\ntitle: x\n---\n");
        assert_eq!(reader.fill_buf().unwrap(), b"body bytes");
        assert_eq!(reader.get_ref().read, source.len());
    }

    #[test]
    fn huge_first_line_is_bounded() {
        let source = "x".repeat(MAX_FRONTMATTER_BYTES + 64);
        let mut reader = BufReader::with_capacity(1024, Cursor::new(source));
        let framed = read_frame(&mut reader).unwrap();
        assert!(framed.frame().frontmatter().is_none());
        assert_eq!(framed.bytes(), b"x");
    }

    #[test]
    fn huge_non_frontmatter_first_line_is_valid_in_memory() {
        let source = format!("{}\n# Visible\n", "x".repeat(70_000));
        let frame = DocumentFrame::parse(&source).unwrap();
        assert!(frame.frontmatter().is_none());
        assert_eq!(frame.body_offset(), 0);
    }

    #[test]
    fn reader_and_slice_accept_long_delimiter_like_body_lines() {
        for suffix in ["x\n# Visible\n", "\r"] {
            let source = format!("---{}{suffix}", " ".repeat(70_000));
            let parsed = DocumentFrame::parse(&source).unwrap();
            let mut reader = BufReader::with_capacity(11, Cursor::new(source.as_bytes()));
            let streamed = read_frame(&mut reader).unwrap();
            assert!(parsed.frontmatter().is_none(), "{suffix:?}");
            assert_eq!(streamed.frame(), &parsed, "{suffix:?}");
            assert!(streamed.bytes().len() <= MAX_DELIMITER_LINE_BYTES);
        }
        let source = format!("---{}", " ".repeat(70_000));
        assert!(DocumentFrame::parse(&source).is_err());
        assert!(read_frame(&mut BufReader::new(Cursor::new(source))).is_err());
    }

    #[test]
    fn long_delimiter_candidate_streams_to_decision_with_bounded_storage() {
        let source = format!("---{}x\n{}", " ".repeat(70_000), "body".repeat(1_000));
        let decision_offset = 3 + 70_000 + 1;
        let counted = Counted {
            inner: Cursor::new(source.as_bytes()),
            read: 0,
        };
        let mut reader = BufReader::with_capacity(1, counted);
        let framed = read_frame(&mut reader).unwrap();
        assert!(framed.frame().frontmatter().is_none());
        assert_eq!(framed.bytes().len(), MAX_DELIMITER_LINE_BYTES);
        // Exact recognition consumes through the candidate's first
        // disqualifying byte. Retained storage stays bounded, and neither the
        // following newline nor later body bytes are demanded.
        assert_eq!(reader.get_ref().read, decision_offset);
    }

    #[test]
    fn body_capture_rejects_too_small_limit_without_reading_or_panicking() {
        let mut reader = BufReader::new(Cursor::new("---\ntitle: x\n---\nbody"));
        assert!(read_frame_for_body(&mut reader, 0).is_err());
        assert!(reader.buffer().is_empty());
    }

    #[test]
    fn huge_line_after_opening_is_rejected_before_materialization() {
        let source = format!("---\n{}", "x".repeat(MAX_FRONTMATTER_BYTES + 64));
        let counted = Counted {
            inner: Cursor::new(source.as_bytes()),
            read: 0,
        };
        let mut reader = BufReader::with_capacity(17, counted);
        assert!(read_frame(&mut reader).is_err());
        assert!(reader.get_ref().read <= MAX_FRONTMATTER_BYTES + 32);
    }
}
