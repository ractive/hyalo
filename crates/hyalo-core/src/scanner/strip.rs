use std::borrow::Cow;

/// Strip inline code spans from a line, replacing their content with spaces
/// to preserve byte positions for link parsing.
/// Returns a borrowed reference when no backticks are present (zero allocation).
///
/// This is the **single-line** form: an unterminated opening backtick run at
/// the end of the line is treated as literal text (its content is *not*
/// blanked). For multi-line CommonMark code spans — where a run of N
/// backticks opened on one line is only closed by the next run of exactly N
/// backticks, possibly several lines later — use
/// [`strip_inline_code_stateful`], which threads the open run across lines.
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

    // ASCII-only substitutions preserve UTF-8 validity; re-validate to avoid unsafe.
    Cow::Owned(
        String::from_utf8(result)
            .expect("strip_inline_code: ASCII backtick→space substitution must preserve UTF-8"),
    )
}

/// Returns `true` if `text` contains a run of *exactly* `n` consecutive
/// backticks (a run bounded by non-backtick characters or the text edges).
///
/// Used as the multi-line-span lookahead (L-3): a backtick opener only starts
/// a cross-line code span when a matching closer of the same length exists
/// later in the document. A shorter or longer adjacent run does not close a
/// span, so only exact-length runs count.
fn code_run_exists(text: &str, n: usize) -> bool {
    if n == 0 {
        return false;
    }
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    while i < len {
        if bytes[i] == b'`' {
            let start = i;
            while i < len && bytes[i] == b'`' {
                i += 1;
            }
            if i - start == n {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// Strip inline code spans from a line, carrying an **open backtick run**
/// across lines (iter-183 L-3).
///
/// `open` holds the length of a currently-open backtick run (an inline code
/// span opened on a previous line that has not yet been closed). CommonMark's
/// closing rule is used: a run of exactly `N` backticks closes a span opened
/// by a run of `N` backticks, and the closer may appear several lines after
/// the opener. While a span is open, the entire line is blanked to spaces
/// (its content, including any `[[link]]` or `[t](x.md)`, must not be
/// extracted). When a closer is found mid-line, blanking stops at the closer
/// and normal single-line scanning resumes for the remainder.
///
/// `rest` is the remaining document text *after* this line (i.e. everything
/// the driver has not yet consumed). It is used only to decide, when a
/// backtick run on this line has no closer on the same line, whether that run
/// opens a genuine multi-line code span (a matching closer exists later) or is
/// just literal stray backticks (CommonMark: an opener with no closer anywhere
/// is literal). This keeps unclosed backticks in prose from silently
/// swallowing every following link. Pass `""` for single-line semantics
/// (unclosed opener stays literal).
///
/// On return, `open` is updated: `Some(n)` if a span is still open at the end
/// of the line (opened here or carried in), `None` if all spans on the line
/// closed.
pub fn strip_inline_code_stateful<'a>(
    line: &'a str,
    open: &mut Option<usize>,
    rest: &str,
) -> Cow<'a, str> {
    // Fast path: no backticks and nothing open → borrow unchanged.
    if open.is_none() && !line.contains('`') {
        return Cow::Borrowed(line);
    }

    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut result = bytes.to_vec();
    let mut i = 0usize;
    let mut changed = false;

    // If a span is open from a previous line, blank until we find the matching
    // closing run (exactly `open_len` backticks).
    if let Some(open_len) = *open {
        while i < len {
            if bytes[i] == b'`' {
                let run_start = i;
                let mut count = 0usize;
                while i < len && bytes[i] == b'`' {
                    count += 1;
                    i += 1;
                }
                if count == open_len {
                    // Closer found: blank everything from line start through
                    // the closing run, then resume normal scanning.
                    for b in &mut result[..i] {
                        *b = b' ';
                    }
                    changed = true;
                    *open = None;
                    break;
                }
                // A run of a different length is still inside the span —
                // continue scanning past it (it will be blanked below).
                let _ = run_start;
            } else {
                i += 1;
            }
        }
        if open.is_some() {
            // Never closed on this line: the whole line is inside the span.
            for b in &mut result[..len] {
                *b = b' ';
            }
            // ASCII space substitution preserves UTF-8.
            return Cow::Owned(String::from_utf8(result).expect(
                "strip_inline_code_stateful: ASCII backtick→space substitution must preserve UTF-8",
            ));
        }
    }

    // Normal single-line scan for the remainder of the line.
    while i < len {
        if bytes[i] == b'`' {
            let start = i;
            let mut backtick_count = 0usize;
            while i < len && bytes[i] == b'`' {
                backtick_count += 1;
                i += 1;
            }

            let content_start = i;
            let mut found_close = false;
            while i < len {
                if bytes[i] == b'`' {
                    let mut close_count = 0usize;
                    while i < len && bytes[i] == b'`' {
                        close_count += 1;
                        i += 1;
                    }
                    if close_count == backtick_count {
                        for b in &mut result[start..i] {
                            *b = b' ';
                        }
                        changed = true;
                        found_close = true;
                        break;
                    }
                } else {
                    i += 1;
                }
            }

            if !found_close {
                let _ = content_start;
                // No closer on this line. Only treat the opener as a genuine
                // multi-line code span if a matching closing run of exactly
                // `backtick_count` backticks exists somewhere in the remaining
                // document (CommonMark closing rule). Otherwise the backticks
                // are literal stray text — leave them and the rest of the line
                // untouched so real links are not swallowed.
                if code_run_exists(rest, backtick_count) {
                    for b in &mut result[start..len] {
                        *b = b' ';
                    }
                    changed = true;
                    *open = Some(backtick_count);
                }
                // Whether or not it opened a span, there is nothing more to
                // strip on this line past an unterminated run.
                break;
            }
        } else {
            i += 1;
        }
    }

    if changed {
        Cow::Owned(String::from_utf8(result).expect(
            "strip_inline_code_stateful: ASCII backtick→space substitution must preserve UTF-8",
        ))
    } else {
        Cow::Borrowed(line)
    }
}

/// Strip an HTML comment (`<!-- ... -->`) span from a line, carrying an
/// **open comment** across lines (iter-183 L-15).
///
/// `in_comment` is `true` when a `<!--` was seen on a previous line without a
/// matching `-->`. While inside a comment, the line content is blanked to
/// spaces so that any `[[link]]` or `[t](x.md)` inside an HTML comment is not
/// extracted. When `-->` is found, blanking stops after it and normal scanning
/// resumes. Multiple comments on one line are handled. On return `in_comment`
/// reflects whether a comment is still open at end of line.
///
/// Returns a borrowed slice when nothing was blanked (zero allocation).
pub fn strip_html_comments<'a>(line: &'a str, in_comment: &mut bool) -> Cow<'a, str> {
    if !*in_comment && !line.contains("<!--") {
        return Cow::Borrowed(line);
    }

    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut result = bytes.to_vec();
    let mut i = 0usize;
    let mut changed = false;

    while i < len {
        if *in_comment {
            // Look for the closing `-->`.
            if i + 2 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' && bytes[i + 2] == b'>' {
                for b in &mut result[..i + 3] {
                    *b = b' ';
                }
                changed = true;
                *in_comment = false;
                i += 3;
            } else {
                i += 1;
            }
        } else if i + 3 < len
            && bytes[i] == b'<'
            && bytes[i + 1] == b'!'
            && bytes[i + 2] == b'-'
            && bytes[i + 3] == b'-'
        {
            let open = i;
            *in_comment = true;
            i += 4;
            // Scan for the closer on this same line.
            let mut closed_at = None;
            while i < len {
                if i + 2 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' && bytes[i + 2] == b'>' {
                    closed_at = Some(i + 3);
                    break;
                }
                i += 1;
            }
            if let Some(end) = closed_at {
                for b in &mut result[open..end] {
                    *b = b' ';
                }
                changed = true;
                *in_comment = false;
                i = end;
            } else {
                // Comment runs to end of line — blank the rest and keep
                // `in_comment` open for the next line.
                for b in &mut result[open..len] {
                    *b = b' ';
                }
                changed = true;
                break;
            }
        } else {
            i += 1;
        }
    }

    // If still inside a comment (opened before this line, no closer here),
    // the entire line is blanked.
    if *in_comment && !changed {
        return Cow::Owned(" ".repeat(len));
    }

    if changed {
        Cow::Owned(
            String::from_utf8(result)
                .expect("strip_html_comments: ASCII substitution must preserve UTF-8"),
        )
    } else {
        Cow::Borrowed(line)
    }
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
        // ASCII-only substitutions preserve UTF-8 validity; re-validate to avoid unsafe.
        Cow::Owned(
            String::from_utf8(result)
                .expect("strip_inline_comments: ASCII %%→space substitution must preserve UTF-8"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_run_exists_exact_length_only() {
        assert!(code_run_exists("a `` b", 2));
        assert!(!code_run_exists("a `` b", 1));
        assert!(!code_run_exists("a `` b", 3));
        assert!(code_run_exists("x ` y", 1));
        assert!(!code_run_exists("no ticks", 1));
        assert!(!code_run_exists("```", 0));
    }

    #[test]
    fn stateful_unclosed_opener_without_closer_ahead_is_literal() {
        // No matching closer in `rest` → opener stays literal, `open` untouched.
        let mut open = None;
        let out = strip_inline_code_stateful("text `open [[link]]", &mut open, "");
        assert_eq!(out.as_ref(), "text `open [[link]]");
        assert_eq!(open, None);
    }

    #[test]
    fn stateful_unclosed_opener_with_closer_ahead_opens_span() {
        // A matching single-backtick closer exists later → span opens, tail blanked.
        let mut open = None;
        let out = strip_inline_code_stateful("text `open [[link]]", &mut open, "next line ` close");
        assert!(!out.contains("[[link]]"), "{out:?}");
        assert_eq!(open, Some(1));
    }

    #[test]
    fn stateful_carried_span_blanks_whole_line_until_closer() {
        // A span already open (len 2) blanks a full interior line...
        let mut open = Some(2);
        let interior = strip_inline_code_stateful("[[hidden]] stuff", &mut open, "closer `` here");
        assert!(!interior.contains("hidden"), "{interior:?}");
        assert_eq!(open, Some(2), "still open, no closer on this line");

        // ...and closes on the line carrying the matching `` run.
        let mut open2 = Some(2);
        let closing = strip_inline_code_stateful("done `` after [[real]]", &mut open2, "");
        assert!(closing.contains("[[real]]"), "{closing:?}");
        assert_eq!(open2, None);
    }

    #[test]
    fn html_comment_single_line() {
        let mut open = false;
        let out = strip_html_comments("a <!-- x [[no]] --> b [[yes]]", &mut open);
        assert!(!out.contains("no"), "{out:?}");
        assert!(out.contains("[[yes]]"));
        assert!(!open);
    }

    #[test]
    fn html_comment_opens_and_carries() {
        let mut open = false;
        let out1 = strip_html_comments("before <!-- open", &mut open);
        assert!(out1.contains("before"));
        assert!(open, "comment should be left open");

        // Interior line fully blanked.
        let out2 = strip_html_comments("[[hidden]] inside", &mut open);
        assert!(!out2.contains("hidden"), "{out2:?}");
        assert!(open);

        // Closing line resumes normal text after `-->`.
        let out3 = strip_html_comments("end --> [[visible]]", &mut open);
        assert!(out3.contains("[[visible]]"), "{out3:?}");
        assert!(!open);
    }
}
