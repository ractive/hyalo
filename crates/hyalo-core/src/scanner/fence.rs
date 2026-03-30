/// Tracks fenced code block state while iterating over lines.
///
/// Call [`process_line`](Self::process_line) for each line. It returns `true`
/// when the line is inside (or opens/closes) a fenced code block, meaning it
/// should typically be skipped for heading or content analysis.
#[derive(Debug, Default)]
pub struct FenceTracker {
    fence: Option<(char, usize)>,
}

impl FenceTracker {
    /// Create a new tracker with no active fence.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if currently inside a fenced code block.
    #[must_use]
    pub fn in_fence(&self) -> bool {
        self.fence.is_some()
    }

    /// Process a line and update fence state.
    ///
    /// Returns `true` if the line is part of a fenced code block (opening,
    /// body, or closing fence line). The caller should typically skip such
    /// lines for heading/content analysis.
    pub fn process_line(&mut self, line: &str) -> bool {
        if let Some((fence_char, fence_count)) = self.fence {
            if is_closing_fence(line, fence_char, fence_count) {
                self.fence = None;
            }
            return true;
        }
        if let Some(f) = detect_opening_fence(line) {
            self.fence = Some(f);
            return true;
        }
        false
    }
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

/// Extract the info-string (language tag) from a fenced code block opening line.
/// E.g. `` ```rust `` → `"rust"`, `~~~` → `""`
pub fn extract_fence_language(line: &str, fence_char: char, fence_count: usize) -> String {
    let trimmed = line.trim_start();
    let after_fence = &trimmed[fence_count * fence_char.len_utf8()..];
    after_fence.trim().to_owned()
}
