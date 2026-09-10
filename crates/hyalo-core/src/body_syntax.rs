//! Shared Markdown body visibility and source spans.
//!
//! Structural consumers and lint rules use this service to agree about code
//! blocks, HTML comments and source offsets. It intentionally is not a full
//! Markdown AST; raw full-text consumers remain free to inspect every byte.

use std::ops::Range;

/// An HTML comment that is wholly contained on one physical line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlCommentSpan {
    line: usize,
    content: Range<usize>,
}

impl HtmlCommentSpan {
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }

    /// Body-relative byte range inside `<!--` and `-->`.
    #[must_use]
    pub fn content_span(&self) -> Range<usize> {
        self.content.clone()
    }
}

/// Body-wide source classification shared by scanners and lint.
#[derive(Debug, Default)]
pub struct BodySyntax {
    in_code: Vec<bool>,
    indented_code: Vec<bool>,
    in_html_comment: Vec<bool>,
    unterminated_fence_open: Vec<bool>,
    fence_delimiter: Vec<bool>,
    fence_open: Vec<Option<FenceOpen>>,
    fence_close: Vec<bool>,
    percent_comment: Vec<bool>,
    line_starts: Vec<usize>,
    html_comments: Vec<HtmlCommentSpan>,
    visible: String,
}

#[derive(Debug, Clone, Copy)]
struct CodeFence {
    ch: u8,
    len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FenceOpen {
    ch: char,
    len: usize,
    language: String,
}

#[derive(Debug, Default)]
struct InlineState {
    code_run: Option<usize>,
    html_comment: bool,
}

struct LineSpan<'a> {
    raw: &'a str,
    start: usize,
    physical_end: usize,
}

impl BodySyntax {
    #[must_use]
    pub fn new(body: &str) -> Self {
        let lines = physical_lines(body);
        let n = lines.len();
        let line_starts = lines.iter().map(|line| line.start).collect();
        let mut syntax = Self {
            in_code: vec![false; n],
            indented_code: vec![false; n],
            in_html_comment: vec![false; n],
            unterminated_fence_open: vec![false; n],
            fence_delimiter: vec![false; n],
            fence_open: vec![None; n],
            fence_close: vec![false; n],
            percent_comment: vec![false; n],
            line_starts,
            html_comments: Vec::new(),
            visible: body.to_owned(),
        };
        let mut visible = body.as_bytes().to_vec();

        let mut open_fence: Option<(CodeFence, usize, usize)> = None;
        let mut previous_blank = true;
        let mut in_indented = false;
        let mut list_content_columns = Vec::new();
        let mut inline = InlineState::default();
        let mut in_percent_block = false;

        for (i, physical) in lines.iter().enumerate() {
            let raw_line = physical.raw;
            let line = strip_block_quote(raw_line);
            let prefix = raw_line.len() - line.len();
            let absolute = physical.start + prefix;
            let rest = &body[physical.physical_end..];

            if let Some((fence, _, allowance)) = open_fence {
                if is_fence_close_within(line, fence, allowance) {
                    open_fence = None;
                    syntax.fence_delimiter[i] = true;
                    syntax.fence_close[i] = true;
                } else {
                    syntax.in_code[i] = true;
                }
                visible[physical.start..physical.start + raw_line.len()].fill(b' ');
                previous_blank = false;
                continue;
            }

            if in_percent_block {
                syntax.percent_comment[i] = true;
                visible[physical.start..physical.start + raw_line.len()].fill(b' ');
                if crate::scanner::is_comment_fence(line) {
                    in_percent_block = false;
                }
                continue;
            }

            // Cross-line inline constructs own the line until their real
            // closer. Markdown-looking text inside them is never a block
            // delimiter.
            if inline.code_run.is_some() && crate::scanner::is_block_boundary(line) {
                inline.code_run = None;
            }
            let inline_was_open = inline.code_run.is_some() || inline.html_comment;
            if inline_was_open {
                let result = mask_inline_line(
                    line,
                    rest,
                    absolute,
                    i + 1,
                    &mut inline,
                    &mut visible,
                    &mut syntax.html_comments,
                );
                syntax.in_html_comment[i] = result.had_html
                    && visible[absolute..absolute + line.len()]
                        .iter()
                        .all(u8::is_ascii_whitespace);
                previous_blank = line.trim().is_empty();
                continue;
            }
            {
                let fence_allowance = list_content_columns.last().map_or(3, |column| column + 3);
                if let Some(fence) = fence_open_within(line, fence_allowance) {
                    open_fence = Some((fence, i, fence_allowance));
                    syntax.fence_delimiter[i] = true;
                    let indent = line.len() - line.trim_start_matches(' ').len();
                    syntax.fence_open[i] = Some(FenceOpen {
                        ch: char::from(fence.ch),
                        len: fence.len,
                        language: line[indent + fence.len..].trim().to_owned(),
                    });
                    visible[physical.start..physical.start + raw_line.len()].fill(b' ');
                    in_indented = false;
                    previous_blank = false;
                    continue;
                }
                if crate::scanner::is_comment_fence(line) {
                    syntax.percent_comment[i] = true;
                    visible[physical.start..physical.start + raw_line.len()].fill(b' ');
                    in_percent_block = true;
                    continue;
                }
            }

            let blank = line.trim().is_empty();
            // Decide indented-code status against the container that existed
            // before this line. Otherwise an indented code example beginning
            // with `- ` would incorrectly create its own list container.
            let container_column = list_content_columns.last().copied().unwrap_or(0);
            // Preserve the established GitHub Docs dialect for an ordered
            // list authored at four-space indentation. Its marker opens a
            // real container whose later indented fence must stay structural;
            // the equivalent `    - [ ]` example remains indented code.
            let starts_indented_ordered_list = list_content_columns.is_empty()
                && previous_blank
                && ordered_list_marker_content_column(line).is_some();
            if in_indented {
                if blank || is_indented_code_line(line, container_column) {
                    syntax.in_code[i] = true;
                    syntax.indented_code[i] = true;
                } else {
                    in_indented = false;
                }
            } else if !blank
                && previous_blank
                && !starts_indented_ordered_list
                && is_indented_code_line(line, container_column)
            {
                in_indented = true;
                syntax.in_code[i] = true;
                syntax.indented_code[i] = true;
            }

            if !blank && !syntax.in_code[i] {
                update_list_containers(line, &mut list_content_columns);
            }

            if syntax.in_code[i] {
                visible[physical.start..physical.start + raw_line.len()].fill(b' ');
            } else {
                let result = mask_inline_line(
                    line,
                    rest,
                    absolute,
                    i + 1,
                    &mut inline,
                    &mut visible,
                    &mut syntax.html_comments,
                );
                syntax.in_html_comment[i] = result.had_html
                    && visible[absolute..absolute + line.len()]
                        .iter()
                        .all(u8::is_ascii_whitespace);
            }
            previous_blank = blank;
        }

        if let Some((_, opener, _)) = open_fence {
            syntax.unterminated_fence_open[opener] = true;
        }
        syntax.visible = String::from_utf8(visible).expect("masking preserves UTF-8");
        syntax
    }

    #[must_use]
    pub fn line_is_code(&self, line: usize) -> bool {
        Self::at(line, &self.in_code)
    }
    #[must_use]
    pub fn line_is_indented_code(&self, line: usize) -> bool {
        Self::at(line, &self.indented_code)
    }
    #[must_use]
    pub fn line_is_html_comment(&self, line: usize) -> bool {
        Self::at(line, &self.in_html_comment)
    }
    #[must_use]
    pub fn opens_unterminated_fence(&self, line: usize) -> bool {
        Self::at(line, &self.unterminated_fence_open)
    }
    #[must_use]
    pub fn line_start(&self, line: usize) -> Option<usize> {
        line.checked_sub(1)
            .and_then(|index| self.line_starts.get(index))
            .copied()
    }
    #[must_use]
    pub fn html_comments(&self) -> &[HtmlCommentSpan] {
        &self.html_comments
    }
    #[must_use]
    pub fn visible_body(&self) -> &str {
        &self.visible
    }
    #[must_use]
    pub fn visible_line(&self, line: usize) -> Option<&str> {
        let index = line.checked_sub(1)?;
        let start = *self.line_starts.get(index)?;
        let mut end = self
            .line_starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.visible.len());
        if self.visible.as_bytes().get(end.wrapping_sub(1)) == Some(&b'\n') {
            end -= 1;
        }
        if self.visible.as_bytes().get(end.wrapping_sub(1)) == Some(&b'\r') {
            end -= 1;
        }
        self.visible.get(start..end)
    }
    /// Whether a half-open body-byte range touches code or a wholly commented
    /// line. Autofix adapters use this after converting a rule's source span:
    /// a diagnostic may begin in prose while its proposed edit crosses into a
    /// protected block.
    #[must_use]
    pub fn range_intersects_protected(&self, range: Range<usize>) -> bool {
        if range.start > range.end || range.start > self.visible.len() {
            return false;
        }
        self.line_starts.iter().enumerate().any(|(index, start)| {
            let end = self
                .line_starts
                .get(index + 1)
                .copied()
                .unwrap_or(self.visible.len());
            let intersects = if range.is_empty() {
                *start <= range.start && range.start < end
            } else {
                *start < range.end && end > range.start
            };
            intersects
                && (self.in_code.get(index).copied().unwrap_or(false)
                    || self.in_html_comment.get(index).copied().unwrap_or(false))
        })
    }
    pub(crate) fn line_is_fence_delimiter(&self, line: usize) -> bool {
        Self::at(line, &self.fence_delimiter)
    }
    pub(crate) fn fence_open(&self, line: usize) -> Option<(char, usize, &str)> {
        let open = line
            .checked_sub(1)
            .and_then(|index| self.fence_open.get(index))?
            .as_ref()?;
        Some((open.ch, open.len, &open.language))
    }
    pub(crate) fn line_is_fence_close(&self, line: usize) -> bool {
        Self::at(line, &self.fence_close)
    }
    pub(crate) fn line_is_percent_comment(&self, line: usize) -> bool {
        Self::at(line, &self.percent_comment)
    }
    fn at(line: usize, flags: &[bool]) -> bool {
        line.checked_sub(1)
            .and_then(|index| flags.get(index))
            .copied()
            .unwrap_or(false)
    }
}

#[derive(Default)]
struct InlineResult {
    had_html: bool,
}

fn mask_inline_line(
    line: &str,
    rest: &str,
    absolute: usize,
    line_number: usize,
    state: &mut InlineState,
    visible: &mut [u8],
    html_comments: &mut Vec<HtmlCommentSpan>,
) -> InlineResult {
    if state.code_run.is_some() && crate::scanner::is_block_boundary(line) {
        state.code_run = None;
    }
    let source = line.as_bytes();
    let mut i = 0usize;
    // Empty lines never enter the loop below. Preserve ownership inherited
    // from an already-open HTML comment so lint fixes cannot treat blank
    // lines inside that comment as ordinary prose.
    let mut result = InlineResult {
        had_html: state.html_comment,
    };
    while i < source.len() {
        if state.html_comment {
            result.had_html = true;
            if let Some(relative) = line[i..].find("-->") {
                let end = i + relative + 3;
                visible[absolute + i..absolute + end].fill(b' ');
                state.html_comment = false;
                i = end;
            } else {
                visible[absolute + i..absolute + source.len()].fill(b' ');
                break;
            }
            continue;
        }
        if let Some(open_len) = state.code_run {
            if let Some((_, end)) = find_exact_code_run(source, i, open_len) {
                visible[absolute + i..absolute + end].fill(b' ');
                state.code_run = None;
                i = end;
            } else {
                visible[absolute + i..absolute + source.len()].fill(b' ');
                break;
            }
            continue;
        }
        if source[i..].starts_with(b"<!--") {
            result.had_html = true;
            let content_start = i + 4;
            if let Some(relative) = line[content_start..].find("-->") {
                let content_end = content_start + relative;
                let end = content_end + 3;
                html_comments.push(HtmlCommentSpan {
                    line: line_number,
                    content: absolute + content_start..absolute + content_end,
                });
                visible[absolute + i..absolute + end].fill(b' ');
                i = end;
            } else {
                visible[absolute + i..absolute + source.len()].fill(b' ');
                state.html_comment = true;
                break;
            }
            continue;
        }
        if source[i..].starts_with(b"%%") {
            if let Some(relative) = line[i + 2..].find("%%") {
                let end = i + 2 + relative + 2;
                visible[absolute + i..absolute + end].fill(b' ');
                i = end;
            } else {
                i += 2;
            }
            continue;
        }
        if source[i] == b'`' {
            let start = i;
            while i < source.len() && source[i] == b'`' {
                i += 1;
            }
            let run = i - start;
            if let Some((_, end)) = find_exact_code_run(source, i, run) {
                visible[absolute + start..absolute + end].fill(b' ');
                i = end;
            } else if crate::scanner::code_run_exists(crate::scanner::block_lookahead(rest), run) {
                visible[absolute + start..absolute + source.len()].fill(b' ');
                state.code_run = Some(run);
                break;
            }
            continue;
        }
        i += 1;
    }
    result
}

fn find_exact_code_run(bytes: &[u8], mut i: usize, wanted: usize) -> Option<(usize, usize)> {
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        if i - start == wanted {
            return Some((start, i));
        }
    }
    None
}

fn physical_lines(body: &str) -> Vec<LineSpan<'_>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    while start < body.len() {
        let physical_end = body[start..]
            .find('\n')
            .map_or(body.len(), |n| start + n + 1);
        let mut content_end = physical_end
            - usize::from(physical_end > start && body.as_bytes()[physical_end - 1] == b'\n');
        if content_end > start && body.as_bytes()[content_end - 1] == b'\r' {
            content_end -= 1;
        }
        out.push(LineSpan {
            raw: &body[start..content_end],
            start,
            physical_end,
        });
        start = physical_end;
    }
    out
}

fn is_indented_code_line(line: &str, container_column: usize) -> bool {
    let mut column = 0usize;
    for byte in line.bytes() {
        match byte {
            b' ' => column += 1,
            b'\t' => column += 4 - (column % 4),
            _ => break,
        }
    }
    column >= container_column + 4
}

fn strip_block_quote(mut line: &str) -> &str {
    loop {
        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent > 3 {
            return line;
        }
        let Some(after) = line[indent..].strip_prefix('>') else {
            return line;
        };
        line = after.strip_prefix(' ').unwrap_or(after);
    }
}

fn update_list_containers(line: &str, stack: &mut Vec<usize>) {
    let indent = line.len() - line.trim_start_matches(' ').len();
    while stack.last().is_some_and(|column| *column > indent) {
        stack.pop();
    }
    if let Some(content_column) = list_marker_content_column(line, indent)
        && stack.last().copied() != Some(content_column)
    {
        stack.push(content_column);
    }
}

fn list_marker_content_column(line: &str, indent: usize) -> Option<usize> {
    let rest = &line[indent..];
    let marker_len = match rest.as_bytes().first()? {
        b'-' | b'*' | b'+' => 1,
        b'0'..=b'9' => {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 9 || !matches!(rest.as_bytes().get(digits), Some(b'.' | b')')) {
                return None;
            }
            digits + 1
        }
        _ => return None,
    };
    let after = &rest[marker_len..];
    let spaces = after.bytes().take_while(|byte| *byte == b' ').count();
    if spaces == 0 && !after.is_empty() {
        return None;
    }
    let gap = if spaces == 0 || spaces > 4 { 1 } else { spaces };
    Some(indent + marker_len + gap)
}

fn ordered_list_marker_content_column(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent != 4 {
        return None;
    }
    let rest = &line[indent..];
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 9 || !matches!(rest.as_bytes().get(digits), Some(b'.' | b')')) {
        return None;
    }
    list_marker_content_column(line, indent)
}

fn fence_open_within(line: &str, max_indent: usize) -> Option<CodeFence> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > max_indent {
        return None;
    }
    let rest = &line[indent..];
    let ch = *rest.as_bytes().first()?;
    if !matches!(ch, b'`' | b'~') {
        return None;
    }
    let len = rest.bytes().take_while(|byte| *byte == ch).count();
    if len < 3 || (ch == b'`' && rest[len..].contains('`')) {
        return None;
    }
    Some(CodeFence { ch, len })
}

fn is_fence_close_within(line: &str, open: CodeFence, max_indent: usize) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > max_indent {
        return false;
    }
    let rest = &line[indent..];
    let len = rest.bytes().take_while(|byte| *byte == open.ch).count();
    len >= open.len && rest[len..].bytes().all(|byte| matches!(byte, b' ' | b'\t'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_corpus_classifies_literals_and_comments() {
        let body = "text\n\n    [] example\n\n1. item\n   ```md\n   - [ ] literal\n   ```\n`<!-- markdownlint-disable MD019 -->`\n<!--\n# Hidden\n- [ ] example\n-->\n# Visible\n";
        let syntax = BodySyntax::new(body);
        assert!(syntax.line_is_indented_code(3));
        assert!(syntax.line_is_code(7));
        assert!(syntax.line_is_html_comment(11));
        assert!(syntax.line_is_html_comment(12));
        assert!(syntax.line_is_html_comment(13));
        assert!(
            syntax.html_comments().is_empty(),
            "inline code and multiline comments are not single-line directives"
        );
        assert!(!syntax.line_is_code(15));
    }

    #[test]
    fn only_syntactic_single_line_comment_is_exposed() {
        let body = "`<!-- markdownlint-disable MD019 -->`\n<!-- markdownlint-disable MD019 -->\n";
        let syntax = BodySyntax::new(body);
        assert_eq!(syntax.html_comments().len(), 1);
        let span = syntax.html_comments()[0].content_span();
        assert_eq!(&body[span], " markdownlint-disable MD019 ");
    }

    #[test]
    fn crlf_unicode_spans_and_comment_fence_precedence_are_exact() {
        let body = "é\r\n<!-- markdownlint-disable MD019 -->\r\n<!--\r\n```md\r\n# hidden\r\n```\r\n-->\r\n# visible\r\n";
        let syntax = BodySyntax::new(body);
        assert_eq!(syntax.line_start(2), Some("é\r\n".len()));
        let span = syntax.html_comments()[0].content_span();
        assert_eq!(&body[span], " markdownlint-disable MD019 ");
        assert!(syntax.line_is_html_comment(4));
        assert!(syntax.line_is_html_comment(5));
        assert!(!syntax.line_is_code(5));
        assert!(!syntax.line_is_html_comment(8));
    }

    #[test]
    fn visible_view_masks_multiline_inline_code_partial_comments_and_percent_blocks() {
        let body =
            "Example `\n[] literal\n` end\n<!--\n# Hidden --> visible\n%%\n```\n%%\n# Visible\n";
        let syntax = BodySyntax::new(body);
        assert_eq!(syntax.visible_body().len(), body.len());
        assert!(!syntax.visible_line(2).unwrap().contains("[]"));
        assert!(!syntax.visible_line(5).unwrap().contains("# Hidden"));
        assert!(syntax.visible_line(5).unwrap().contains("visible"));
        assert!(syntax.visible_line(6).unwrap().trim().is_empty());
        assert!(syntax.visible_line(7).unwrap().trim().is_empty());
        assert!(syntax.visible_line(8).unwrap().trim().is_empty());
        assert_eq!(syntax.visible_line(9).unwrap(), "# Visible");
    }

    #[test]
    fn delimiter_lines_inside_multiline_inline_code_stay_literal() {
        let body = "Example `\n%%\n` end\n# Visible\n";
        let syntax = BodySyntax::new(body);
        assert!(!syntax.line_is_percent_comment(2));
        assert_eq!(syntax.visible_line(4), Some("# Visible"));
    }

    #[test]
    fn inline_percent_hides_html_openers_and_directives() {
        let body = "%% <!-- markdownlint-disable MD019 --> %%\n[[old]]\n";
        let syntax = BodySyntax::new(body);
        assert!(syntax.html_comments().is_empty());
        assert!(syntax.visible_line(1).unwrap().trim().is_empty());
        assert_eq!(syntax.visible_line(2), Some("[[old]]"));
    }

    #[test]
    fn html_comment_precedes_code_and_indentation_until_its_closer() {
        for body in ["<!--\n` -->\n[[old]] `\n", "<!--\n\n    -->\n[[old]]\n"] {
            let syntax = BodySyntax::new(body);
            assert!(syntax.visible_body().contains("[[old]]"), "{body:?}");
            assert!(!syntax.line_is_code(3), "{body:?}");
        }
    }

    #[test]
    fn blockquote_containers_do_not_make_html_comment_content_visible() {
        let body = "> <!--\n> # Hidden\n\n> -->\n> # Visible\n";
        let syntax = BodySyntax::new(body);
        assert!(syntax.line_is_html_comment(2));
        assert!(syntax.line_is_html_comment(3));
        assert!(syntax.visible_line(2).unwrap().trim_end().ends_with('>'));
        assert!(syntax.visible_line(5).unwrap().contains("# Visible"));
        let hidden = body.find("# Hidden").unwrap();
        assert!(syntax.range_intersects_protected(hidden..hidden + 1));
        let crossing = body.find('\n').unwrap()..hidden + 1;
        assert!(syntax.range_intersects_protected(crossing));
        let visible = body.rfind("# Visible").unwrap();
        assert!(!syntax.range_intersects_protected(visible..visible + 1));
    }

    #[test]
    fn list_container_distinguishes_nested_list_from_indented_code() {
        let nested = BodySyntax::new("- [x] parent\n\n    - [ ] child [[old]]\n");
        assert!(!nested.line_is_code(3));
        assert!(nested.visible_line(3).unwrap().contains("[[old]]"));

        for body in [
            "    - [ ] literal [[old]]\n",
            "text\n\n    - [ ] literal [[old]]\n",
            "[] run `cmd` <!-- keep -->\n\n    - [] literal `code` <!-- comment -->\n",
        ] {
            let syntax = BodySyntax::new(body);
            let line = body.lines().count();
            assert!(syntax.line_is_indented_code(line), "{body:?}");
            assert!(syntax.visible_line(line).unwrap().trim().is_empty());
        }

        let ordered =
            BodySyntax::new("    1. Add the registry\n     ```\n     literal [[old]]\n     ```\n");
        assert!(!ordered.line_is_code(1));
        assert!(ordered.line_is_fence_delimiter(2));
        assert!(ordered.line_is_code(3));
        assert!(ordered.line_is_fence_delimiter(4));
    }
}
