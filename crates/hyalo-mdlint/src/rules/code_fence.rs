//! Shared fenced-code-block tracking for line-based HYALO rules.
//!
//! Several HYALO rules scan line by line for a bracket-shaped pattern
//! (`[]`, `- [ ]`, ...). None of those patterns are meaningful *inside* a
//! fenced code block — a JS/regex/markdown sample that happens to contain
//! `[]` or a literal `- [ ]` is documentation, not a task list. This module
//! centralizes CommonMark §4.5 fence open/close detection so every rule
//! suppresses code-block contents the same way (BUG-5).

/// An open fenced code block: the fence character (`` ` `` or `~`) and the
/// run length of the opening fence. A closing fence must use the same
/// character and be at least as long (CommonMark §4.5).
pub struct CodeFence {
    ch: u8,
    len: usize,
}

/// If `line` opens a fenced code block, return the fence descriptor.
///
/// A fence is a run of at least three `` ` `` or `~` characters, optionally
/// indented up to three spaces, optionally followed by an info string.
/// Backtick fences may not contain a backtick in their info string
/// (CommonMark §4.5); tilde fences have no such restriction.
#[must_use]
pub fn fence_open(line: &str) -> Option<CodeFence> {
    fence_open_within(line, 3)
}

/// [`fence_open`], with the maximum leading indent supplied by the caller.
///
/// CommonMark's "up to three spaces" is measured from the *container's*
/// content column, not from column 0: a fence written inside a list item may
/// carry the item's own indentation as well (BUG-5, dogfood v0.22.0 — GitHub
/// Docs indents ```` ``` ```` four spaces under a numbered item, and every
/// prose rule fired inside the sample because the fence was invisible).
/// [`super::spans::BodySpans`] tracks the open list containers and passes
/// `content_column + 3` here; everything else keeps the column-0 rule.
#[must_use]
pub fn fence_open_within(line: &str, max_indent: usize) -> Option<CodeFence> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    // Deeper than the container allows makes it an indented code block.
    if indent > max_indent {
        return None;
    }
    let rest = &line[indent..];
    let ch = match rest.as_bytes().first() {
        Some(&b) if b == b'`' || b == b'~' => b,
        _ => return None,
    };
    let len = rest.bytes().take_while(|&b| b == ch).count();
    if len < 3 {
        return None;
    }
    // Backtick fences forbid a backtick anywhere in the info string.
    if ch == b'`' && rest[len..].contains('`') {
        return None;
    }
    Some(CodeFence { ch, len })
}

/// Whether `line` closes the currently open fenced code block `open`.
///
/// A closing fence is a run of the same fence character, at least as long as
/// the opener, indented up to three spaces, with only trailing whitespace
/// after it (CommonMark §4.5).
#[must_use]
pub fn is_fence_close(line: &str, open: &CodeFence) -> bool {
    is_fence_close_within(line, open, 3)
}

/// [`is_fence_close`], with the maximum leading indent supplied by the caller.
///
/// A fence opened inside a list item is closed by one indented to the same
/// container, so the closing probe needs the same allowance the opener got.
#[must_use]
pub fn is_fence_close_within(line: &str, open: &CodeFence, max_indent: usize) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > max_indent {
        return false;
    }
    let rest = &line[indent..];
    let len = rest.bytes().take_while(|&b| b == open.ch).count();
    if len < open.len {
        return false;
    }
    rest[len..].bytes().all(|b| b == b' ' || b == b'\t')
}

/// Whether the byte at `col` in `line` lies inside a backtick-delimited inline
/// code span. Spans are delimited by matched runs of backticks of equal length
/// (CommonMark §6.3); text between a matched open/close run is code.
///
/// This is a pragmatic scan — good enough to suppress the false-positive where
/// a bare `[]` is documented inside `` `[]` ``.
#[must_use]
pub fn in_inline_code(line: &str, col: usize) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            // Find a matching closing run of exactly `run` backticks.
            let mut j = i + run;
            while j < bytes.len() {
                if bytes[j] == b'`' {
                    let close = bytes[j..].iter().take_while(|&&b| b == b'`').count();
                    if close == run {
                        // The span content and its delimiters both count as code.
                        if col >= i && col < j + close {
                            return true;
                        }
                        i = j + close;
                        break;
                    }
                    j += close;
                } else {
                    j += 1;
                }
            }
            if j >= bytes.len() {
                // Unterminated run — not a code span; move past it.
                i += run;
            }
        } else {
            i += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backtick_fence_open_and_close() {
        let open = fence_open("```js").expect("``` opens a fence");
        assert!(is_fence_close("```", &open));
        assert!(is_fence_close("```   ", &open), "trailing space allowed");
        assert!(!is_fence_close("``", &open), "shorter run does not close");
    }

    #[test]
    fn tilde_fence_needs_matching_char() {
        let open = fence_open("~~~").expect("~~~ opens a fence");
        assert!(
            !is_fence_close("```", &open),
            "backticks cannot close tilde"
        );
        assert!(is_fence_close("~~~", &open));
    }

    #[test]
    fn backtick_fence_rejects_backtick_in_info() {
        assert!(fence_open("``` `nested` ```").is_none());
    }

    #[test]
    fn indented_more_than_three_is_not_a_fence() {
        assert!(fence_open("    ```").is_none());
    }

    #[test]
    fn inline_code_span_detection() {
        let line = "Use `[]` here.";
        let bracket = line.find('[').unwrap();
        assert!(in_inline_code(line, bracket));
        let outside = line.find("here").unwrap();
        assert!(!in_inline_code(line, outside));
    }

    #[test]
    fn unterminated_backtick_is_not_code() {
        let line = "a ` [] b";
        let bracket = line.find('[').unwrap();
        assert!(!in_inline_code(line, bracket));
    }
}
