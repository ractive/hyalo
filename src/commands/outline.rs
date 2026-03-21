#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::commands::{FilesOrOutcome, collect_files, unwrap_single_file_result};
use crate::frontmatter;
use crate::links;
use crate::output::{CommandOutcome, Format};
use crate::scanner;
use crate::types::{FileOutline, OutlineSection, PropertyInfo, TaskCount};

// ---------------------------------------------------------------------------
// Public command entry point
// ---------------------------------------------------------------------------

/// Build a full outline for each targeted file.
///
/// - `--file`  → returns a bare `FileOutline` object
/// - `--glob`  → returns an array of `FileOutline` objects
/// - neither   → all `.md` files under `dir`, returns an array
pub fn outline(
    dir: &Path,
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let mut results: Vec<serde_json::Value> = Vec::new();
    for (full_path, rel_path) in &files {
        let outline = build_file_outline(full_path, rel_path)?;
        results.push(serde_json::to_value(outline).unwrap_or_default());
    }

    let json_output = unwrap_single_file_result(file, results);

    Ok(CommandOutcome::Success(crate::output::format_output(
        format,
        &json_output,
    )))
}

// ---------------------------------------------------------------------------
// Core outline builder
// ---------------------------------------------------------------------------

/// Build a `FileOutline` for a single file.
fn build_file_outline(path: &Path, rel_path: &str) -> Result<FileOutline> {
    // Read frontmatter properties
    let props_raw = frontmatter::read_frontmatter(path)?;

    let properties: Vec<PropertyInfo> = props_raw
        .iter()
        .map(|(name, value)| PropertyInfo {
            name: name.clone(),
            prop_type: frontmatter::infer_type(value).to_owned(),
            value: frontmatter::yaml_to_json(value),
        })
        .collect();

    // Extract tags from frontmatter
    let tags = crate::commands::tags::extract_tags(&props_raw);

    // Scan the file body for sections
    let sections = scan_sections(path)?;

    Ok(FileOutline {
        file: rel_path.to_owned(),
        properties,
        tags,
        sections,
    })
}

// ---------------------------------------------------------------------------
// Section scanning
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

/// Scan file body line by line, building `OutlineSection` values.
/// Skips frontmatter, tracks fenced code block state, detects ATX headings,
/// extracts links, counts task checkboxes, and records code block languages.
fn scan_sections(path: &Path) -> Result<Vec<OutlineSection>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = BufReader::new(file);

    let mut line_num: usize = 0;
    let mut buf = String::new();

    // --- Skip frontmatter ---
    buf.clear();
    let n = reader.read_line(&mut buf).context("failed to read line")?;
    if n == 0 {
        // Empty file
        return Ok(Vec::new());
    }
    line_num += 1;

    let first_trimmed = buf.trim_end_matches(['\n', '\r']).to_owned();
    let fm_lines = frontmatter::skip_frontmatter(&mut reader, &first_trimmed)?;
    if fm_lines > 0 {
        line_num = fm_lines;
    }

    // --- Scan body ---

    // Pre-heading section (level 0): collects content before the first heading
    let mut current = SectionBuilder::new(0, None, 1);
    let mut sections: Vec<OutlineSection> = Vec::new();
    // Fenced code block state: (fence_char, fence_count, language)
    let mut fence: Option<(char, usize, String)> = None;

    // If the first line was not frontmatter, process it now
    if fm_lines == 0 {
        process_body_line(
            &first_trimmed,
            line_num,
            &mut current,
            &mut fence,
            &mut sections,
        );
    }

    loop {
        buf.clear();
        let n = reader.read_line(&mut buf).context("failed to read line")?;
        if n == 0 {
            break; // EOF
        }
        line_num += 1;
        let line = buf.trim_end_matches(['\n', '\r']);

        process_body_line(line, line_num, &mut current, &mut fence, &mut sections);
    }

    // Flush the last section
    let last = current.finish();
    // Only emit the pre-heading section (level 0) if it has any content
    let should_emit = last.level > 0
        || !last.links.is_empty()
        || last.tasks.is_some()
        || !last.code_blocks.is_empty();
    if should_emit {
        sections.push(last);
    }

    Ok(sections)
}

/// Process a single body line, updating the current section builder and
/// emitting a finished section to `sections` when a new heading is detected.
fn process_body_line(
    line: &str,
    line_num: usize,
    current: &mut SectionBuilder,
    fence: &mut Option<(char, usize, String)>,
    sections: &mut Vec<OutlineSection>,
) {
    // --- Fenced code block handling ---
    if let Some((fence_char, fence_count, _)) = fence.as_ref() {
        if scanner::is_closing_fence(line, *fence_char, *fence_count) {
            *fence = None;
        }
        // Lines inside a code block are not processed for links/tasks/headings
        return;
    }

    // Detect opening fence
    if let Some((fc, count)) = scanner::detect_opening_fence(line) {
        let lang = extract_fence_language(line, fc, count);
        current.code_blocks.push(lang.clone());
        *fence = Some((fc, count, lang));
        return;
    }

    // --- ATX heading detection ---
    if let Some((level, heading_text)) = parse_atx_heading(line) {
        // Finish the current section, emitting it only if it has content
        // (or if it is a named section, i.e. level > 0)
        let finished = std::mem::replace(
            current,
            SectionBuilder::new(level, Some(heading_text), line_num),
        );

        // Level-0 (pre-heading) sections are only emitted if they have
        // links, tasks, or code blocks — plain text doesn't count.
        let should_emit = finished.level > 0
            || !finished.links.is_empty()
            || finished.task_total > 0
            || !finished.code_blocks.is_empty();

        if should_emit {
            sections.push(finished.finish());
        }

        return;
    }

    // --- Normal text line ---

    // Strip inline code spans before extracting links
    let cleaned = scanner::strip_inline_code(line);
    let mut line_links: Vec<links::Link> = Vec::new();
    links::extract_links_from_text(cleaned.as_ref(), &mut line_links);

    for link in line_links {
        let formatted = format_link_string(&link);
        current.links.push(formatted);
    }

    // Count task checkboxes: lines of the form `- [ ] ...` or `- [x] ...` etc.
    if let Some(done) = detect_task_checkbox(line) {
        current.task_total += 1;
        if done {
            current.task_done += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the info-string (language tag) from a fenced code block opening line.
/// E.g. `` ```rust `` → `"rust"`, `~~~` → `""`
fn extract_fence_language(line: &str, fence_char: char, fence_count: usize) -> String {
    let trimmed = line.trim_start();
    // Skip past the fence chars
    let after_fence = &trimmed[fence_count * fence_char.len_utf8()..];
    // The info string runs to end-of-line; trim whitespace
    after_fence.trim().to_owned()
}

/// Parse an ATX heading line (`# Heading`, `## Sub`, etc.).
/// Returns `(level, heading_text)` if the line is a valid ATX heading,
/// or `None` otherwise.
fn parse_atx_heading(line: &str) -> Option<(u8, String)> {
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

/// Detect a task checkbox on a line.
/// Returns `Some(true)` for a completed task, `Some(false)` for an open task,
/// or `None` if the line is not a task checkbox.
///
/// Recognises: `- [ ] ...`, `- [x] ...`, `- [X] ...` (and `*` / `+` bullets).
fn detect_task_checkbox(line: &str) -> Option<bool> {
    let trimmed = line.trim_start();

    // Must start with a list marker: `-`, `*`, or `+`
    let rest = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))?;

    // Must be followed by `[` then one char then `]`
    let inner = rest.strip_prefix('[')?;
    // The checkbox marker is a single character followed by `]`
    let mut chars = inner.chars();
    let marker = chars.next()?;
    let close = chars.next()?;
    if close != ']' {
        return None;
    }

    let done = marker == 'x' || marker == 'X';
    Some(done)
}

/// Format a `Link` into a human-readable string for storage in the outline.
/// Wikilinks: `[[target]]`
/// Markdown links: `[label](target)` or `[](target)` when no label
fn format_link_string(link: &links::Link) -> String {
    // Heuristic: if the target contains `.md` or `/` it's likely a markdown link
    // that came from `[text](target)` syntax. But we don't have the original syntax
    // available here — the Link struct doesn't distinguish between wikilinks and
    // markdown links. We use the label field:
    // - links::extract_links_from_text sets label for both wikilinks (from `|`) and
    //   markdown links (from `[text]`).
    // The simplest consistent representation: wikilinks have no label or a pipe label,
    // markdown links always have the display text as label.
    // Without the original syntax we fall back to a simple format:
    //   [[target]] for no-label links, [[target|label]] for labeled wikilinks
    //   [label](target) for markdown links.
    // Since we can't distinguish from the Link struct alone, emit `[[target]]` style
    // for wikilinks (no label or pipe-separated) and `[label](target)` for markdown.
    // The underlying Link is produced by the same parser for both — we emit wikilink
    // format by default (matches the outline spec: `"[[iteration-02-links]]"`).
    match &link.label {
        Some(label) if !label.is_empty() => {
            // Could be from a wikilink [[t|label]] or markdown [label](t).
            // We can't tell them apart from the Link struct, so use the markdown format
            // which is more informative for labeled links.
            format!("[{}]({})", label, link.target)
        }
        _ => format!("[[{}]]", link.target),
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

    fn unwrap_success(outcome: CommandOutcome) -> String {
        match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("expected success, got user error: {s}"),
        }
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

    // --- detect_task_checkbox ---

    #[test]
    fn open_task() {
        assert_eq!(detect_task_checkbox("- [ ] Do something"), Some(false));
    }

    #[test]
    fn done_task_lowercase() {
        assert_eq!(detect_task_checkbox("- [x] Done"), Some(true));
    }

    #[test]
    fn done_task_uppercase() {
        assert_eq!(detect_task_checkbox("- [X] Done"), Some(true));
    }

    #[test]
    fn task_with_star_bullet() {
        assert_eq!(detect_task_checkbox("* [ ] Star bullet"), Some(false));
    }

    #[test]
    fn task_with_plus_bullet() {
        assert_eq!(detect_task_checkbox("+ [ ] Plus bullet"), Some(false));
    }

    #[test]
    fn indented_task() {
        assert_eq!(detect_task_checkbox("  - [ ] Indented"), Some(false));
    }

    #[test]
    fn not_a_task() {
        assert!(detect_task_checkbox("- Just a bullet").is_none());
        assert!(detect_task_checkbox("Regular text").is_none());
    }

    // --- extract_fence_language ---

    #[test]
    fn fence_language_rust() {
        assert_eq!(extract_fence_language("```rust", '`', 3), "rust");
    }

    #[test]
    fn fence_no_language() {
        assert_eq!(extract_fence_language("```", '`', 3), "");
    }

    #[test]
    fn fence_language_with_spaces() {
        assert_eq!(extract_fence_language("```  sh  ", '`', 3), "sh");
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
    fn format_link_with_label() {
        let link = links::Link {
            target: "my-note".to_owned(),
            label: Some("My Note".to_owned()),
        };
        assert_eq!(format_link_string(&link), "[My Note](my-note)");
    }

    // --- scan_sections ---

    #[test]
    fn empty_file_produces_no_sections() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("empty.md");
        fs::write(&path, "").unwrap();
        let sections = scan_sections(&path).unwrap();
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
        let sections = scan_sections(&path).unwrap();
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
        let sections = scan_sections(&path).unwrap();
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
        let sections = scan_sections(&path).unwrap();
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
        let sections = scan_sections(&path).unwrap();
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
        let sections = scan_sections(&path).unwrap();
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
        let sections = scan_sections(&path).unwrap();
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
        let sections = scan_sections(&path).unwrap();
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
        let sections = scan_sections(&path).unwrap();
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
        let sections = scan_sections(&path).unwrap();
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
        let sections = scan_sections(&path).unwrap();
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
        let sections = scan_sections(&path).unwrap();
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
        let sections = scan_sections(&path).unwrap();
        assert_eq!(sections.len(), 2);
        // "---", "title: Test", "---" = 3 lines of frontmatter. First heading is line 4.
        assert_eq!(sections[0].line, 4);
        // Blank line at 5, second heading at 6
        assert_eq!(sections[1].line, 6);
    }

    // --- Full outline command ---

    fn setup_vault() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: My Note
tags:
  - rust
  - cli
---
# Introduction

See [[other-note]] for context.

## Tasks

- [ ] Write tests
- [x] Write code

```rust
fn main() {}
```
"),
        )
        .unwrap();
        tmp
    }

    #[test]
    fn outline_single_file() {
        let tmp = setup_vault();
        let out = unwrap_success(outline(tmp.path(), Some("note.md"), None, Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

        // Top-level fields
        assert!(parsed["file"].as_str().unwrap().ends_with("note.md"));

        // Properties
        let props = parsed["properties"].as_array().unwrap();
        assert!(props.iter().any(|p| p["name"] == "title"));
        let title_prop = props.iter().find(|p| p["name"] == "title").unwrap();
        assert_eq!(title_prop["type"], "text");
        assert_eq!(title_prop["value"], "My Note");

        // Tags
        let tags = parsed["tags"].as_array().unwrap();
        assert!(tags.contains(&serde_json::json!("rust")));
        assert!(tags.contains(&serde_json::json!("cli")));

        // Sections
        let sections = parsed["sections"].as_array().unwrap();
        assert_eq!(sections.len(), 2); // Introduction + Tasks

        let intro = &sections[0];
        assert_eq!(intro["level"], 1);
        assert_eq!(intro["heading"], "Introduction");
        let intro_links = intro["links"].as_array().unwrap();
        assert_eq!(intro_links.len(), 1);
        assert_eq!(intro_links[0], "[[other-note]]");

        let tasks_section = &sections[1];
        assert_eq!(tasks_section["level"], 2);
        assert_eq!(tasks_section["heading"], "Tasks");
        let tasks = &tasks_section["tasks"];
        assert_eq!(tasks["total"], 2);
        assert_eq!(tasks["done"], 1);
        let code_blocks = tasks_section["code_blocks"].as_array().unwrap();
        assert_eq!(code_blocks.len(), 1);
        assert_eq!(code_blocks[0], "rust");
    }

    #[test]
    fn outline_file_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let result = outline(tmp.path(), Some("nope.md"), None, Format::Json).unwrap();
        assert!(matches!(result, CommandOutcome::UserError(_)));
    }

    #[test]
    fn outline_all_files_returns_array() {
        let tmp = setup_vault();
        let out = unwrap_success(outline(tmp.path(), None, None, Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(parsed.is_array());
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
    }

    #[test]
    fn outline_no_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("plain.md"),
            md!(r"
# Heading

Just text here.
"),
        )
        .unwrap();
        let out =
            unwrap_success(outline(tmp.path(), Some("plain.md"), None, Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(parsed["properties"].as_array().unwrap().is_empty());
        assert!(parsed["tags"].as_array().unwrap().is_empty());
        let sections = parsed["sections"].as_array().unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0]["heading"], "Heading");
    }

    #[test]
    fn outline_pre_heading_code_block_emitted() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
```sh
echo hello
```

# Section
"),
        )
        .unwrap();
        let out = unwrap_success(outline(tmp.path(), Some("note.md"), None, Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let sections = parsed["sections"].as_array().unwrap();
        // pre-heading section (level 0) with code block + heading section
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0]["level"], 0);
        let cb = sections[0]["code_blocks"].as_array().unwrap();
        assert_eq!(cb.len(), 1);
        assert_eq!(cb[0], "sh");
    }
}
