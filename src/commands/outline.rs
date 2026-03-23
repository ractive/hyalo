#![allow(clippy::missing_errors_doc)]
use crate::links;
use crate::scanner::{self, FileVisitor, ScanAction};
use crate::types::{OutlineSection, TaskCount};

// ---------------------------------------------------------------------------
// SectionScanner visitor
// ---------------------------------------------------------------------------

/// State accumulated for the current section being built.
struct SectionBuilder {
    level: u8,
    heading: Option<String>,
    /// 1-based line number where this section starts (heading line, or 1 for pre-heading)
    line: usize,
    links: Vec<String>,
    task_total: usize,
    task_done: usize,
    code_blocks: Vec<String>,
}

impl SectionBuilder {
    fn new(level: u8, heading: Option<String>, line: usize) -> Self {
        Self {
            level,
            heading,
            line,
            links: Vec::new(),
            task_total: 0,
            task_done: 0,
            code_blocks: Vec::new(),
        }
    }

    fn finish(self) -> OutlineSection {
        let tasks = if self.task_total > 0 {
            Some(TaskCount {
                total: self.task_total,
                done: self.task_done,
            })
        } else {
            None
        };
        OutlineSection {
            level: self.level,
            heading: self.heading,
            line: self.line,
            links: self.links,
            tasks,
            code_blocks: self.code_blocks,
        }
    }
}

/// Visitor that builds outline sections from body events.
/// Tracks headings, links, tasks, and code blocks per section.
pub struct SectionScanner {
    current: SectionBuilder,
    sections: Vec<OutlineSection>,
}

impl Default for SectionScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl SectionScanner {
    #[must_use]
    pub fn new() -> Self {
        Self {
            current: SectionBuilder::new(0, None, 1),
            sections: Vec::new(),
        }
    }

    /// Consume and return all collected sections.
    #[must_use]
    pub fn into_sections(mut self) -> Vec<OutlineSection> {
        // Flush the last section
        let last = std::mem::replace(&mut self.current, SectionBuilder::new(0, None, 0));
        let finished = last.finish();
        let should_emit = finished.level > 0
            || !finished.links.is_empty()
            || finished.tasks.is_some()
            || !finished.code_blocks.is_empty();
        if should_emit {
            self.sections.push(finished);
        }
        self.sections
    }
}

impl FileVisitor for SectionScanner {
    fn on_body_line(&mut self, raw: &str, line_num: usize) -> ScanAction {
        // ATX heading detection
        if let Some((level, heading_text)) = parse_atx_heading(raw) {
            let finished = std::mem::replace(
                &mut self.current,
                SectionBuilder::new(level, Some(heading_text), line_num),
            );

            let should_emit = finished.level > 0
                || !finished.links.is_empty()
                || finished.task_total > 0
                || !finished.code_blocks.is_empty();

            if should_emit {
                self.sections.push(finished.finish());
            }

            return ScanAction::Continue;
        }

        // Normal text line — extract links and count tasks
        let cleaned = scanner::strip_inline_code(raw);
        let mut line_links: Vec<links::Link> = Vec::new();
        links::extract_links_from_text(cleaned.as_ref(), &mut line_links);

        for link in line_links {
            let formatted = format_link_string(&link);
            self.current.links.push(formatted);
        }

        if let Some((_status, done)) = crate::tasks::detect_task_checkbox(raw) {
            self.current.task_total += 1;
            if done {
                self.current.task_done += 1;
            }
        }

        ScanAction::Continue
    }

    fn on_code_fence_open(&mut self, _raw: &str, language: &str, _line_num: usize) -> ScanAction {
        if !language.is_empty() {
            self.current.code_blocks.push(language.to_owned());
        }
        ScanAction::Continue
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse an ATX heading line (`# Heading`, `## Sub`, etc.).
/// Returns `(level, heading_text)` if the line is a valid ATX heading,
/// or `None` otherwise.
pub(crate) fn parse_atx_heading(line: &str) -> Option<(u8, String)> {
    let bytes = line.as_bytes();
    if bytes.first() != Some(&b'#') {
        return None;
    }

    // Count leading `#` characters (maximum 6 for ATX headings)
    let level = bytes.iter().take_while(|&&b| b == b'#').count();
    if level > 6 {
        return None;
    }

    let after_hashes = &line[level..];

    // An ATX heading requires either:
    // - A space (or tab) after the hashes: `## Heading`
    // - Nothing after the hashes (empty heading): `##`
    let heading_text = if after_hashes.is_empty() {
        String::new()
    } else if after_hashes.starts_with(' ') || after_hashes.starts_with('\t') {
        // Strip the leading space/tab, then trim trailing spaces and optional closing `#`s
        let inner = after_hashes[1..].trim_end();
        // Remove optional trailing `#` sequence (ATX closing sequence)
        let inner = inner.trim_end_matches('#').trim_end();
        inner.to_owned()
    } else {
        // Characters directly after `#` with no space — not a valid ATX heading
        return None;
    };

    #[allow(clippy::cast_possible_truncation)] // level is guaranteed ≤ 6 by the check above
    Some((level as u8, heading_text))
}

/// Format a `Link` into a human-readable string for storage in the outline.
fn format_link_string(link: &links::Link) -> String {
    // Heuristic: targets containing '://' are URLs from markdown links.
    // Targets ending in common extensions with '/' are file paths from markdown links.
    // Everything else is treated as a wikilink.
    let is_markdown_link =
        link.target.contains("://") || (link.target.contains('/') && link.target.contains('.'));

    if is_markdown_link {
        match &link.label {
            Some(label) if !label.is_empty() => format!("[{}]({})", label, link.target),
            _ => format!("[]({})", link.target),
        }
    } else {
        match &link.label {
            Some(label) if !label.is_empty() => format!("[[{}|{}]]", link.target, label),
            _ => format!("[[{}]]", link.target),
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
    }

    /// Helper: scan a file and return its sections using the new visitor.
    fn scan_sections(path: &std::path::Path) -> Vec<OutlineSection> {
        let mut ss = SectionScanner::new();
        scanner::scan_file_multi(path, &mut [&mut ss]).unwrap();
        ss.into_sections()
    }

    // --- parse_atx_heading ---

    #[test]
    fn heading_level_1() {
        let h = parse_atx_heading("# Hello").unwrap();
        assert_eq!(h.0, 1);
        assert_eq!(h.1, "Hello");
    }

    #[test]
    fn heading_level_3() {
        let h = parse_atx_heading("### Sub section").unwrap();
        assert_eq!(h.0, 3);
        assert_eq!(h.1, "Sub section");
    }

    #[test]
    fn heading_max_level_6() {
        let h = parse_atx_heading("###### Deep").unwrap();
        assert_eq!(h.0, 6);
        assert_eq!(h.1, "Deep");
    }

    #[test]
    fn heading_7_hashes_not_heading() {
        assert!(parse_atx_heading("####### Too deep").is_none());
    }

    #[test]
    fn heading_no_space_not_heading() {
        assert!(parse_atx_heading("#NoSpace").is_none());
    }

    #[test]
    fn heading_empty() {
        let h = parse_atx_heading("##").unwrap();
        assert_eq!(h.0, 2);
        assert_eq!(h.1, "");
    }

    #[test]
    fn heading_with_closing_hashes() {
        let h = parse_atx_heading("## Section ##").unwrap();
        assert_eq!(h.0, 2);
        assert_eq!(h.1, "Section");
    }

    #[test]
    fn not_a_heading() {
        assert!(parse_atx_heading("Normal text").is_none());
        assert!(parse_atx_heading("").is_none());
    }

    // --- extract_fence_language ---

    #[test]
    fn fence_language_rust() {
        assert_eq!(scanner::extract_fence_language("```rust", '`', 3), "rust");
    }

    #[test]
    fn fence_no_language() {
        assert_eq!(scanner::extract_fence_language("```", '`', 3), "");
    }

    #[test]
    fn fence_language_with_spaces() {
        assert_eq!(scanner::extract_fence_language("```  sh  ", '`', 3), "sh");
    }

    // --- format_link_string ---

    #[test]
    fn format_wikilink_no_label() {
        let link = links::Link {
            target: "my-note".to_owned(),
            label: None,
        };
        assert_eq!(format_link_string(&link), "[[my-note]]");
    }

    #[test]
    fn format_wikilink_with_label() {
        let link = links::Link {
            target: "my-note".to_owned(),
            label: Some("My Note".to_owned()),
        };
        assert_eq!(format_link_string(&link), "[[my-note|My Note]]");
    }

    #[test]
    fn format_markdown_link_with_label() {
        let link = links::Link {
            target: "https://example.com".to_owned(),
            label: Some("Example".to_owned()),
        };
        assert_eq!(format_link_string(&link), "[Example](https://example.com)");
    }

    #[test]
    fn format_file_path_link_with_label() {
        let link = links::Link {
            target: "docs/some-note.md".to_owned(),
            label: Some("Some Note".to_owned()),
        };
        assert_eq!(format_link_string(&link), "[Some Note](docs/some-note.md)");
    }

    // --- scan_sections ---

    #[test]
    fn empty_file_produces_no_sections() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("empty.md");
        fs::write(&path, "").unwrap();
        let sections = scan_sections(&path);
        assert!(sections.is_empty());
    }

    #[test]
    fn file_with_only_frontmatter_produces_no_sections() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("fm_only.md");
        fs::write(
            &path,
            md!(r"
---
title: Test
---
"),
        )
        .unwrap();
        let sections = scan_sections(&path);
        assert!(sections.is_empty());
    }

    #[test]
    fn single_heading_produces_one_section() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(
            &path,
            md!(r"
# Hello

Some text.
"),
        )
        .unwrap();
        let sections = scan_sections(&path);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].level, 1);
        assert_eq!(sections[0].heading.as_deref(), Some("Hello"));
        assert_eq!(sections[0].line, 1);
    }

    #[test]
    fn multiple_headings_produce_multiple_sections() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(
            &path,
            md!(r"
# First

Text A.

## Sub

Text B.
"),
        )
        .unwrap();
        let sections = scan_sections(&path);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].level, 1);
        assert_eq!(sections[0].heading.as_deref(), Some("First"));
        assert_eq!(sections[1].level, 2);
        assert_eq!(sections[1].heading.as_deref(), Some("Sub"));
    }

    #[test]
    fn pre_heading_section_emitted_when_has_links() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(
            &path,
            md!(r"
See [[some-note]] for details.

# Heading
"),
        )
        .unwrap();
        let sections = scan_sections(&path);
        // pre-heading section (level 0) + heading section
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].level, 0);
        assert_eq!(sections[0].heading, None);
        assert_eq!(sections[0].links.len(), 1);
        assert_eq!(sections[0].links[0], "[[some-note]]");
    }

    #[test]
    fn pre_heading_section_not_emitted_when_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(
            &path,
            md!(r"
# Heading

Text here.
"),
        )
        .unwrap();
        let sections = scan_sections(&path);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].level, 1);
    }

    #[test]
    fn links_extracted_per_section() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(
            &path,
            md!(r"
# Section A

See [[note-a]] and [[note-b]].

# Section B

See [[note-c]].
"),
        )
        .unwrap();
        let sections = scan_sections(&path);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].links.len(), 2);
        assert!(sections[0].links.contains(&"[[note-a]]".to_owned()));
        assert!(sections[0].links.contains(&"[[note-b]]".to_owned()));
        assert_eq!(sections[1].links.len(), 1);
        assert_eq!(sections[1].links[0], "[[note-c]]");
    }

    #[test]
    fn tasks_counted_per_section() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(
            &path,
            md!(r"
# Tasks

- [ ] Open task
- [x] Done task
- [X] Also done
- Regular bullet
"),
        )
        .unwrap();
        let sections = scan_sections(&path);
        assert_eq!(sections.len(), 1);
        let tasks = sections[0].tasks.as_ref().unwrap();
        assert_eq!(tasks.total, 3);
        assert_eq!(tasks.done, 2);
    }

    #[test]
    fn no_tasks_field_when_no_tasks() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(
            &path,
            md!(r"
# Section

Just text, no tasks.
"),
        )
        .unwrap();
        let sections = scan_sections(&path);
        assert!(sections[0].tasks.is_none());
    }

    #[test]
    fn code_blocks_tracked_per_section() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(
            &path,
            md!(r"
# Code Section

```rust
let x = 1;
```

~~~python
print('hello')
~~~
"),
        )
        .unwrap();
        let sections = scan_sections(&path);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].code_blocks.len(), 2);
        assert!(sections[0].code_blocks.contains(&"rust".to_owned()));
        assert!(sections[0].code_blocks.contains(&"python".to_owned()));
    }

    #[test]
    fn links_inside_code_blocks_not_extracted() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(
            &path,
            md!(r"
# Section

```
[[not-a-link]]
```

[[real-link]]
"),
        )
        .unwrap();
        let sections = scan_sections(&path);
        assert_eq!(sections[0].links.len(), 1);
        assert_eq!(sections[0].links[0], "[[real-link]]");
    }

    #[test]
    fn links_inside_inline_code_not_extracted() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(
            &path,
            md!(r"
# Section

Use `[[not-a-link]]` and [[real-link]].
"),
        )
        .unwrap();
        let sections = scan_sections(&path);
        assert_eq!(sections[0].links.len(), 1);
        assert_eq!(sections[0].links[0], "[[real-link]]");
    }

    #[test]
    fn line_numbers_correct_for_headings() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        fs::write(
            &path,
            md!(r"
---
title: Test
---
# First Heading

## Second Heading
"),
        )
        .unwrap();
        let sections = scan_sections(&path);
        assert_eq!(sections.len(), 2);
        // "---", "title: Test", "---" = 3 lines of frontmatter. First heading is line 4.
        assert_eq!(sections[0].line, 4);
        // Blank line at 5, second heading at 6
        assert_eq!(sections[1].line, 6);
    }
}
