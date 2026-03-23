#![allow(clippy::missing_errors_doc)]
//! Task detection, extraction, counting, and mutation.
//!
//! Provides utilities for working with markdown task checkboxes (`- [ ] ...`).
//! Used by the `tasks` command family and the `summary` command.

use anyhow::{Context, Result, bail};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::frontmatter;
use crate::heading::parse_atx_heading;
use crate::scanner::{self, FileVisitor, ScanAction};
use crate::types::{TaskCount, TaskInfo};

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Detect a markdown task checkbox on a line.
/// Returns `(status_char, is_done)` if the line is a task, or `None`.
///
/// A task line matches: optional whitespace, then `- [C] ` (or `* [C] ` or `+ [C] `)
/// where C is any single character. Only `'x'` and `'X'` are considered "done".
pub fn detect_task_checkbox(line: &str) -> Option<(char, bool)> {
    let trimmed = line.trim_start();

    // Must start with a list marker: `-`, `*`, or `+` followed by a space
    let rest = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))?;

    // Must be followed by `[` then one char then `]`
    let inner = rest.strip_prefix('[')?;
    let mut chars = inner.chars();
    let marker = chars.next()?;
    let close = chars.next()?;
    if close != ']' {
        return None;
    }

    let done = marker == 'x' || marker == 'X';
    Some((marker, done))
}

// ---------------------------------------------------------------------------
// Text extraction
// ---------------------------------------------------------------------------

/// Extract the task text from a task line (the content after `] `).
/// Returns an empty string if the format does not match.
fn extract_task_text(line: &str) -> &str {
    let trimmed = line.trim_start();
    // Strip list marker
    let rest = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))
        .unwrap_or("");
    // Strip `[C] ` — marker is `[`, one char, `]`, then optional space
    if rest.len() < 3 {
        return "";
    }
    // Find the `]` after `[`
    let after_bracket = match rest.strip_prefix('[') {
        Some(s) => s,
        None => return "",
    };
    let mut chars = after_bracket.char_indices();
    let _ = chars.next(); // skip status char
    let (close_idx, close_char) = match chars.next() {
        Some(pair) => pair,
        None => return "",
    };
    if close_char != ']' {
        return "";
    }
    // close_idx is 0-based index of `]` inside after_bracket
    let after_close = &after_bracket[close_idx + 1..];
    // Strip the optional space after `]`
    after_close.strip_prefix(' ').unwrap_or(after_close)
}

// ---------------------------------------------------------------------------
// Visitors
// ---------------------------------------------------------------------------

/// Visitor that collects all tasks with full detail.
pub struct TaskCollector {
    tasks: Vec<TaskInfo>,
}

impl Default for TaskCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskCollector {
    #[must_use]
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    /// Consume and return collected tasks.
    #[must_use]
    pub fn into_tasks(self) -> Vec<TaskInfo> {
        self.tasks
    }
}

impl FileVisitor for TaskCollector {
    fn on_body_line(&mut self, raw: &str, line_num: usize) -> ScanAction {
        if let Some((status_char, done)) = detect_task_checkbox(raw) {
            self.tasks.push(TaskInfo {
                line: line_num,
                status: status_char.to_string(),
                text: extract_task_text(raw).to_owned(),
                done,
            });
        }
        ScanAction::Continue
    }
}

/// Visitor that counts tasks without collecting details. Lightweight.
pub struct TaskCounter {
    total: usize,
    done: usize,
}

impl Default for TaskCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskCounter {
    #[must_use]
    pub fn new() -> Self {
        Self { total: 0, done: 0 }
    }

    /// Return the counted totals.
    #[must_use]
    pub fn into_count(self) -> TaskCount {
        TaskCount {
            total: self.total,
            done: self.done,
        }
    }
}

impl FileVisitor for TaskCounter {
    fn on_body_line(&mut self, raw: &str, _line_num: usize) -> ScanAction {
        if let Some((_status, is_done)) = detect_task_checkbox(raw) {
            self.total += 1;
            if is_done {
                self.done += 1;
            }
        }
        ScanAction::Continue
    }
}

/// Visitor that collects tasks with section context for the `find` command.
/// Tracks the current ATX heading to populate `FindTaskInfo.section`.
pub struct TaskExtractor {
    current_section: String,
    tasks: Vec<crate::types::FindTaskInfo>,
}

impl Default for TaskExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskExtractor {
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_section: String::new(),
            tasks: Vec::new(),
        }
    }

    /// Consume and return collected tasks.
    #[must_use]
    pub fn into_tasks(self) -> Vec<crate::types::FindTaskInfo> {
        self.tasks
    }

    /// Whether any tasks were collected.
    #[must_use]
    pub fn has_tasks(&self) -> bool {
        !self.tasks.is_empty()
    }
}

impl FileVisitor for TaskExtractor {
    fn on_body_line(&mut self, raw: &str, line_num: usize) -> ScanAction {
        // Track current heading
        if raw.starts_with('#')
            && let Some((level, text)) = parse_atx_heading(raw)
        {
            self.current_section = format!("{} {}", "#".repeat(level as usize), text);
        }

        if let Some((status_char, done)) = detect_task_checkbox(raw) {
            self.tasks.push(crate::types::FindTaskInfo {
                line: line_num,
                section: self.current_section.clone(),
                status: status_char.to_string(),
                text: extract_task_text(raw).to_owned(),
                done,
            });
        }
        ScanAction::Continue
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Extract all tasks from a file, streaming through line by line.
/// Returns tasks with 1-based line numbers, status chars, text, and done state.
/// Skips frontmatter and fenced code blocks.
pub fn extract_tasks(path: &Path) -> Result<Vec<TaskInfo>> {
    let mut collector = TaskCollector::new();
    scanner::scan_file_multi(path, &mut [&mut collector])?;
    Ok(collector.into_tasks())
}

/// Count tasks in a file without collecting details. Lightweight.
pub fn count_tasks(path: &Path) -> Result<TaskCount> {
    let mut counter = TaskCounter::new();
    scanner::scan_file_multi(path, &mut [&mut counter])?;
    Ok(counter.into_count())
}

/// Read a single task at a specific 1-based line number.
/// Returns `None` if the line is not a task (or out of range).
pub fn read_task(path: &Path, line: usize) -> Result<Option<TaskInfo>> {
    // Do a raw scan, stop early once we pass the target line
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = BufReader::new(file);

    let mut line_num: usize = 0;
    let mut buf = String::new();

    // First line: check for frontmatter
    buf.clear();
    let n = reader
        .read_line(&mut buf)
        .context("failed to read first line")?;
    if n == 0 {
        return Ok(None);
    }
    line_num += 1;

    let first_trimmed = buf.trim_end_matches(['\n', '\r']).to_owned();
    let fm_lines = frontmatter::skip_frontmatter(&mut reader, &first_trimmed)?;
    if fm_lines > 0 {
        line_num = fm_lines;
        // If the target line is inside the frontmatter, it can't be a task
        if line <= fm_lines {
            return Ok(None);
        }
    }

    let mut fence: Option<(char, usize)> = None;
    let mut in_comment = false;

    // Process first line if not frontmatter
    if fm_lines == 0 {
        if line_num == line {
            if let Some(f) = scanner::detect_opening_fence(&first_trimmed) {
                fence = Some(f);
                // A fence opener is never a task
            } else if scanner::is_comment_fence(&first_trimmed) {
                in_comment = true;
                // A comment fence is never a task
            } else {
                let info =
                    detect_task_checkbox(&first_trimmed).map(|(status_char, done)| TaskInfo {
                        line: line_num,
                        status: status_char.to_string(),
                        text: extract_task_text(&first_trimmed).to_owned(),
                        done,
                    });
                return Ok(info);
            }
        } else if let Some(f) = scanner::detect_opening_fence(&first_trimmed) {
            fence = Some(f);
        } else if scanner::is_comment_fence(&first_trimmed) {
            in_comment = true;
        }
    }

    loop {
        buf.clear();
        let n = reader.read_line(&mut buf).context("failed to read line")?;
        if n == 0 {
            break;
        }
        line_num += 1;
        let line_str = buf.trim_end_matches(['\n', '\r']);

        // Handle fenced code block (highest priority)
        if let Some((fence_char, fence_count)) = fence {
            if scanner::is_closing_fence(line_str, fence_char, fence_count) {
                fence = None;
            }
            if line_num == line {
                return Ok(None); // inside code block
            }
            if line_num > line {
                break;
            }
            continue;
        }

        // Handle comment block
        if in_comment {
            if scanner::is_comment_fence(line_str) {
                in_comment = false;
            }
            if line_num == line {
                return Ok(None); // inside comment block
            }
            if line_num > line {
                break;
            }
            continue;
        }

        if let Some(f) = scanner::detect_opening_fence(line_str) {
            if line_num == line {
                return Ok(None); // fence opener is not a task
            }
            fence = Some(f);
            if line_num > line {
                break;
            }
            continue;
        }

        if scanner::is_comment_fence(line_str) {
            if line_num == line {
                return Ok(None); // comment fence is not a task
            }
            in_comment = true;
            if line_num > line {
                break;
            }
            continue;
        }

        if line_num == line {
            let info = detect_task_checkbox(line_str).map(|(status_char, done)| TaskInfo {
                line: line_num,
                status: status_char.to_string(),
                text: extract_task_text(line_str).to_owned(),
                done,
            });
            return Ok(info);
        }

        if line_num > line {
            break;
        }
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// Mutation helpers
// ---------------------------------------------------------------------------

/// Replace the status char in a task line.
/// Returns `(modified_line, TaskInfo)` if the line is a valid task, or `None`.
fn mutate_task_line(line: &str, line_num: usize, new_status: char) -> Option<(String, TaskInfo)> {
    let trimmed = line.trim_start();

    // Find list marker length (including leading whitespace)
    let leading = line.len() - trimmed.len();
    let marker_len =
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            2usize
        } else {
            return None;
        };

    // After marker: must be `[C]`
    let after_marker = &trimmed[marker_len..];
    if !after_marker.starts_with('[') {
        return None;
    }
    let inner = &after_marker[1..]; // skip `[`
    let mut chars = inner.char_indices();
    let (_, _status_char) = chars.next()?; // the current status char
    let (close_idx, close_char) = chars.next()?;
    if close_char != ']' {
        return None;
    }

    // Build the modified line: preserve leading whitespace + marker + `[` + new_status + rest from `]`
    // close_idx is the byte index of `]` inside `inner`
    let bracket_open_pos = leading + marker_len; // position of `[` in original line
    let status_pos = bracket_open_pos + 1; // position of the status char in original line
    let close_pos = bracket_open_pos + 1 + close_idx; // position of `]`

    // Verify we're replacing one char (ASCII path is common, but handle multi-byte safely)
    let status_byte_len = inner[..close_idx].len(); // byte length of current status char
    let mut modified = line.to_owned();
    modified.replace_range(
        status_pos..status_pos + status_byte_len,
        &new_status.to_string(),
    );

    let done = new_status == 'x' || new_status == 'X';
    let text = extract_task_text(line).to_owned();

    let _ = close_pos; // suppress unused warning

    let info = TaskInfo {
        line: line_num,
        status: new_status.to_string(),
        text,
        done,
    };

    Some((modified, info))
}

/// Toggle task completion: `[ ]` → `[x]`, `[x]`/`[X]` → `[ ]`, custom → `[x]`.
/// Returns the updated `TaskInfo`. Writes the file in-place.
pub fn toggle_task(path: &Path, line: usize) -> Result<TaskInfo> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let lines: Vec<&str> = content.split('\n').collect();
    // split('\n') produces a trailing empty element for files ending in '\n';
    // exclude it from the line count so it matches 1-based scanner line numbers.
    let line_count = if lines.last() == Some(&"") {
        lines.len() - 1
    } else {
        lines.len()
    };

    if line == 0 || line > line_count {
        bail!(
            "line {} is out of range (file has {} lines)",
            line,
            line_count
        );
    }

    let target = lines[line - 1];
    let (current_status, _done) = detect_task_checkbox(target)
        .ok_or_else(|| anyhow::anyhow!("line {} is not a task checkbox", line))?;

    let new_status = if current_status == 'x' || current_status == 'X' {
        ' '
    } else {
        'x'
    };

    let (modified_line, info) = mutate_task_line(target, line, new_status)
        .ok_or_else(|| anyhow::anyhow!("failed to mutate task on line {}", line))?;

    let new_content = {
        let mut parts: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        parts[line - 1] = modified_line;
        parts.join("\n")
    };

    crate::fs_util::atomic_write(path, new_content.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;

    Ok(info)
}

/// Set a custom status character on a task.
/// Returns the updated `TaskInfo`. Writes the file in-place.
pub fn set_task_status(path: &Path, line: usize, status: char) -> Result<TaskInfo> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let lines: Vec<&str> = content.split('\n').collect();
    let line_count = if lines.last() == Some(&"") {
        lines.len() - 1
    } else {
        lines.len()
    };

    if line == 0 || line > line_count {
        bail!(
            "line {} is out of range (file has {} lines)",
            line,
            line_count
        );
    }

    let target = lines[line - 1];
    // Validate that the line is a task
    detect_task_checkbox(target)
        .ok_or_else(|| anyhow::anyhow!("line {} is not a task checkbox", line))?;

    let (modified_line, info) = mutate_task_line(target, line, status)
        .ok_or_else(|| anyhow::anyhow!("failed to mutate task on line {}", line))?;

    let new_content = {
        let mut parts: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        parts[line - 1] = modified_line;
        parts.join("\n")
    };

    crate::fs_util::atomic_write(path, new_content.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;

    Ok(info)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
    }

    // --- detect_task_checkbox ---

    #[test]
    fn detect_open_task() {
        let (ch, done) = detect_task_checkbox("- [ ] Do something").unwrap();
        assert_eq!(ch, ' ');
        assert!(!done);
    }

    #[test]
    fn detect_done_lowercase_x() {
        let (ch, done) = detect_task_checkbox("- [x] Done").unwrap();
        assert_eq!(ch, 'x');
        assert!(done);
    }

    #[test]
    fn detect_done_uppercase_x() {
        let (ch, done) = detect_task_checkbox("- [X] Done").unwrap();
        assert_eq!(ch, 'X');
        assert!(done);
    }

    #[test]
    fn detect_custom_status_dash() {
        let (ch, done) = detect_task_checkbox("- [-] Cancelled").unwrap();
        assert_eq!(ch, '-');
        assert!(!done);
    }

    #[test]
    fn detect_custom_status_question() {
        let (ch, done) = detect_task_checkbox("- [?] In review").unwrap();
        assert_eq!(ch, '?');
        assert!(!done);
    }

    #[test]
    fn detect_custom_status_exclamation() {
        let (ch, done) = detect_task_checkbox("- [!] Urgent").unwrap();
        assert_eq!(ch, '!');
        assert!(!done);
    }

    #[test]
    fn detect_star_bullet() {
        let (ch, done) = detect_task_checkbox("* [ ] Star bullet").unwrap();
        assert_eq!(ch, ' ');
        assert!(!done);
    }

    #[test]
    fn detect_plus_bullet() {
        let (ch, done) = detect_task_checkbox("+ [ ] Plus bullet").unwrap();
        assert_eq!(ch, ' ');
        assert!(!done);
    }

    #[test]
    fn detect_indented_task() {
        let (ch, done) = detect_task_checkbox("  - [ ] Indented").unwrap();
        assert_eq!(ch, ' ');
        assert!(!done);
    }

    #[test]
    fn detect_non_task_regular_bullet() {
        assert!(detect_task_checkbox("- Just a bullet").is_none());
    }

    #[test]
    fn detect_non_task_plain_text() {
        assert!(detect_task_checkbox("Regular text").is_none());
    }

    #[test]
    fn detect_non_task_heading() {
        assert!(detect_task_checkbox("# Heading").is_none());
    }

    #[test]
    fn detect_non_task_empty_line() {
        assert!(detect_task_checkbox("").is_none());
    }

    // --- extract_tasks ---

    #[test]
    fn extract_tasks_from_file_with_mixed_content() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(
            &path,
            md!(r"
---
title: Test
---
# Section

- [ ] Open task
- [x] Done task

Some regular text.

```
- [ ] This is inside code block
```

- [?] Custom status
"),
        )
        .unwrap();

        let tasks = extract_tasks(&path).unwrap();
        assert_eq!(tasks.len(), 3);

        assert_eq!(tasks[0].status, " ");
        assert_eq!(tasks[0].text, "Open task");
        assert!(!tasks[0].done);

        assert_eq!(tasks[1].status, "x");
        assert_eq!(tasks[1].text, "Done task");
        assert!(tasks[1].done);

        assert_eq!(tasks[2].status, "?");
        assert_eq!(tasks[2].text, "Custom status");
        assert!(!tasks[2].done);
    }

    #[test]
    fn extract_tasks_skips_code_blocks() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(
            &path,
            md!(r"
- [ ] Before code

```
- [ ] Inside code block — ignored
```

- [x] After code
"),
        )
        .unwrap();

        let tasks = extract_tasks(&path).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].text, "Before code");
        assert_eq!(tasks[1].text, "After code");
    }

    #[test]
    fn extract_tasks_empty_file() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("empty.md");
        fs::write(&path, "").unwrap();
        let tasks = extract_tasks(&path).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn extract_tasks_no_tasks() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(&path, "# Heading\n\nJust text.\n").unwrap();
        let tasks = extract_tasks(&path).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn extract_tasks_line_numbers_accurate_with_frontmatter() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(
            &path,
            md!(r"
---
title: Test
---
- [ ] First task
- [x] Second task
"),
        )
        .unwrap();

        let tasks = extract_tasks(&path).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].line, 4);
        assert_eq!(tasks[1].line, 5);
    }

    // --- count_tasks ---

    #[test]
    fn count_tasks_accuracy() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(
            &path,
            md!(r"
- [ ] Task 1
- [x] Task 2
- [X] Task 3
- [-] Task 4
Regular text.
"),
        )
        .unwrap();

        let count = count_tasks(&path).unwrap();
        assert_eq!(count.total, 4);
        assert_eq!(count.done, 2);
    }

    #[test]
    fn count_tasks_empty_file() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("empty.md");
        fs::write(&path, "").unwrap();
        let count = count_tasks(&path).unwrap();
        assert_eq!(count.total, 0);
        assert_eq!(count.done, 0);
    }

    // --- read_task ---

    #[test]
    fn read_task_finds_task_at_line() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "Regular line\n- [ ] The task\nAnother line\n").unwrap();

        let task = read_task(&path, 2).unwrap();
        assert!(task.is_some());
        let task = task.unwrap();
        assert_eq!(task.line, 2);
        assert_eq!(task.status, " ");
        assert_eq!(task.text, "The task");
        assert!(!task.done);
    }

    #[test]
    fn read_task_returns_none_for_non_task_line() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "Regular line\n- [ ] The task\n").unwrap();

        let task = read_task(&path, 1).unwrap();
        assert!(task.is_none());
    }

    #[test]
    fn read_task_returns_none_out_of_range() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "- [ ] Task\n").unwrap();

        let task = read_task(&path, 99).unwrap();
        assert!(task.is_none());
    }

    #[test]
    fn read_task_returns_none_inside_code_block() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "```\n- [ ] Inside code\n```\n- [ ] Real task\n").unwrap();

        // Line 2 is inside the code block
        let task = read_task(&path, 2).unwrap();
        assert!(task.is_none());

        // Line 4 is a real task
        let task = read_task(&path, 4).unwrap();
        assert!(task.is_some());
    }

    // --- toggle_task ---

    #[test]
    fn toggle_task_open_to_done() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "- [ ] Open task\n").unwrap();

        let info = toggle_task(&path, 1).unwrap();
        assert_eq!(info.status, "x");
        assert!(info.done);

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("- [x] Open task"));
    }

    #[test]
    fn toggle_task_done_to_open_lowercase() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "- [x] Done task\n").unwrap();

        let info = toggle_task(&path, 1).unwrap();
        assert_eq!(info.status, " ");
        assert!(!info.done);

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("- [ ] Done task"));
    }

    #[test]
    fn toggle_task_done_to_open_uppercase() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "- [X] Done task\n").unwrap();

        let info = toggle_task(&path, 1).unwrap();
        assert_eq!(info.status, " ");
        assert!(!info.done);

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("- [ ] Done task"));
    }

    #[test]
    fn toggle_task_custom_to_done() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "- [-] Cancelled task\n").unwrap();

        let info = toggle_task(&path, 1).unwrap();
        assert_eq!(info.status, "x");
        assert!(info.done);
    }

    #[test]
    fn toggle_task_error_on_non_task_line() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "Regular line\n").unwrap();

        let result = toggle_task(&path, 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a task"));
    }

    #[test]
    fn toggle_task_preserves_other_lines() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "Line 1\n- [ ] Task\nLine 3\n").unwrap();

        toggle_task(&path, 2).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Line 1"));
        assert!(content.contains("- [x] Task"));
        assert!(content.contains("Line 3"));
    }

    // --- set_task_status ---

    #[test]
    fn set_task_status_custom_char() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "- [ ] Open task\n").unwrap();

        let info = set_task_status(&path, 1, '?').unwrap();
        assert_eq!(info.status, "?");
        assert!(!info.done);

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("- [?] Open task"));
    }

    #[test]
    fn set_task_status_to_done() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "- [ ] Open task\n").unwrap();

        let info = set_task_status(&path, 1, 'x').unwrap();
        assert_eq!(info.status, "x");
        assert!(info.done);
    }

    #[test]
    fn set_task_status_error_on_non_task() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "# Heading\n").unwrap();

        let result = set_task_status(&path, 1, 'x');
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a task"));
    }

    // --- Comment block handling ---

    #[test]
    fn extract_tasks_skips_comment_blocks() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(
            &path,
            md!(r"
- [ ] Before comment
%%
- [ ] Inside comment — ignored
- [x] Also inside — ignored
%%
- [x] After comment
"),
        )
        .unwrap();

        let tasks = extract_tasks(&path).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].text, "Before comment");
        assert_eq!(tasks[1].text, "After comment");
    }

    #[test]
    fn count_tasks_skips_comment_blocks() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(
            &path,
            md!(r"
- [ ] Visible 1
%%
- [ ] Hidden
- [x] Hidden done
%%
- [x] Visible 2
"),
        )
        .unwrap();

        let count = count_tasks(&path).unwrap();
        assert_eq!(count.total, 2);
        assert_eq!(count.done, 1);
    }

    #[test]
    fn read_task_returns_none_inside_comment() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "%%\n- [ ] Inside comment\n%%\n- [ ] Real task\n").unwrap();

        // Line 2 is inside the comment block
        let task = read_task(&path, 2).unwrap();
        assert!(task.is_none());

        // Line 4 is a real task
        let task = read_task(&path, 4).unwrap();
        assert!(task.is_some());
        assert_eq!(task.unwrap().text, "Real task");
    }

    #[test]
    fn read_task_returns_none_when_first_line_is_comment_fence() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("tasks.md");
        fs::write(&path, "%%\n- [ ] Inside comment\n%%\n").unwrap();

        // Line 1 is the opening comment fence — not a task
        let task = read_task(&path, 1).unwrap();
        assert!(task.is_none());
    }
}
