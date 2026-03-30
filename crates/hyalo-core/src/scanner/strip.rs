use std::borrow::Cow;

/// Strip inline code spans from a line, replacing their content with spaces
/// to preserve byte positions for link parsing.
/// Returns a borrowed reference when no backticks are present (zero allocation).
///
/// # Safety constraint
///
/// The `unsafe` block at the end of this function relies on the fact that
/// backtick (0x60) and space (0x20) are both single-byte ASCII characters.
/// Any future change to the delimiter or replacement byte must preserve this
/// single-byte-ASCII invariant to keep the UTF-8 validity proof sound.
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

    // SAFETY: `result` starts as an exact byte-for-byte copy of the valid UTF-8
    // input `line`. We only mutate `result` by overwriting contiguous spans
    // `start..i` with ASCII space bytes (0x20). Both `start` and `i` are
    // indices of backtick delimiters (0x60), which are single-byte ASCII
    // characters and therefore always lie on UTF-8 code-point boundaries.
    // Each modified span is completely replaced by a run of ASCII bytes (valid
    // single-byte UTF-8 code points), while the prefix and suffix outside the
    // span remain unchanged valid UTF-8. Concatenating unchanged valid UTF-8
    // segments with runs of ASCII bytes yields valid UTF-8 overall.
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
///
/// # Safety constraint
///
/// The `unsafe` block at the end of this function relies on the fact that
/// percent (0x25) and space (0x20) are both single-byte ASCII characters.
/// Any future change to the delimiter or replacement byte must preserve this
/// single-byte-ASCII invariant to keep the UTF-8 validity proof sound.
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
        // SAFETY: `result` starts as an exact byte-for-byte copy of the valid
        // UTF-8 input `line`. We only mutate `result` by overwriting contiguous
        // spans `open..i+2` with ASCII space bytes (0x20). Both `open` and the
        // closing `%%` position are indices of percent-sign delimiters (0x25),
        // which are single-byte ASCII characters and therefore always lie on
        // UTF-8 code-point boundaries. Each modified span is completely replaced
        // by a run of ASCII bytes (valid single-byte UTF-8 code points), while
        // the prefix and suffix outside the span remain unchanged valid UTF-8.
        // Concatenating unchanged valid UTF-8 segments with runs of ASCII bytes
        // yields valid UTF-8 overall.
        Cow::Owned(unsafe { String::from_utf8_unchecked(result) })
    }
}
