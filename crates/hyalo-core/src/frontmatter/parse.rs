#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde_json::Value;
use serde_saphyr::{Budget, DuplicateKeyPolicy, Options, SerializerOptions};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Shared parser options for all YAML frontmatter parsing in hyalo.
///
/// Enforces tight limits via a `Budget` to harden the parser against
/// pathological inputs (deep nesting, alias bombs, huge scalars).
/// Also enables strict YAML 1.2 booleans (`true`/`false` only) and
/// rejects duplicate keys.
pub fn hyalo_options() -> Options {
    Options {
        budget: Some(Budget {
            max_events: 10_000,
            max_depth: 20,
            max_aliases: 0,
            max_anchors: 0,
            max_nodes: 5_000,
            max_total_scalar_bytes: 8192,
            max_documents: 1,
            ..Budget::default()
        }),
        duplicate_keys: DuplicateKeyPolicy::Error,
        strict_booleans: true,
        ..Options::default()
    }
}

/// Build `SerializerOptions` that preserve the detected list indentation style.
fn hyalo_serializer_options(compact_list_indent: bool) -> SerializerOptions {
    SerializerOptions {
        compact_list_indent,
        ..SerializerOptions::default()
    }
}

/// Detect whether the YAML content uses compact list indentation.
///
/// Scans for the first sequence indicator (`- `) and checks whether it is indented
/// further than its parent mapping key. Returns `true` for compact (flush) style,
/// `false` for indented style. Defaults to `false` if no sequences are found.
pub(super) fn detect_list_indent_style(yaml: &str) -> bool {
    // Look for a mapping key followed by a newline and then a sequence indicator.
    // Pattern: a line like "key:\n- item" (compact) vs "key:\n  - item" (indented).
    let mut prev_key_indent: Option<usize> = None;

    for line in yaml.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        // Skip blank lines and comment-only lines — they don't reset the
        // preceding-key state (e.g. `tags:\n  # note\n  - a`).
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Check if this line is a sequence indicator
        if trimmed.starts_with("- ") || trimmed == "-" {
            if let Some(key_indent) = prev_key_indent {
                // Compact: sequence indicator is at the same level as the key
                // Indented: sequence indicator is indented further than the key
                return indent <= key_indent;
            }
            // Sequence without a preceding key — treat as compact
            return true;
        }

        // Check if this line is a mapping key (ends with `:` or `: value`)
        if let Some(colon_pos) = trimmed.find(':') {
            let before_colon = &trimmed[..colon_pos];
            // Basic check: the part before `:` looks like a key (no spaces except in quoted strings)
            if !before_colon.is_empty()
                && !before_colon.starts_with('-')
                && (trimmed.len() == colon_pos + 1 || trimmed.as_bytes()[colon_pos + 1] == b' ')
            {
                // If the value after `:` is empty or only a comment, next line might be a sequence
                let after_colon = trimmed[colon_pos + 1..].trim();
                if after_colon.is_empty() || after_colon.starts_with('#') {
                    prev_key_indent = Some(indent);
                    continue;
                }
            }
        }

        prev_key_indent = None;
    }

    // No sequences found — default to indented (non-compact)
    false
}

/// Represents parsed frontmatter and the remaining body content.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Used in tests only
pub struct Document {
    properties: IndexMap<String, Value>,
    body: String,
    /// Whether the original YAML used compact list indentation (flush `- item`).
    /// `false` means indented style (`  - item` under its parent key).
    /// Defaults to `false` (indented) when no sequences are present.
    compact_list_indent: bool,
}

#[allow(dead_code)] // All methods used in tests only
impl Document {
    #[must_use]
    pub fn properties(&self) -> &IndexMap<String, Value> {
        &self.properties
    }

    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Parse a markdown document, extracting YAML frontmatter if present.
    /// Returns an error if the file starts with `---` but has no closing delimiter,
    /// which would cause corruption on write (a new frontmatter block on top of an unclosed one).
    pub fn parse(content: &str) -> Result<Self> {
        let (yaml_str, body) = extract_frontmatter(content)?;

        let (properties, compact_list_indent) = match yaml_str {
            Some(yaml) if !yaml.trim().is_empty() => {
                let compact = detect_list_indent_style(yaml);
                let props: IndexMap<String, Value> =
                    serde_saphyr::from_str_with_options(yaml, hyalo_options())
                        .context("failed to parse YAML frontmatter")?;
                (props, compact)
            }
            _ => (IndexMap::new(), false),
        };

        Ok(Self {
            properties,
            body: body.to_owned(),
            compact_list_indent,
        })
    }

    /// Serialize the document back to a string with YAML frontmatter.
    pub fn serialize(&self) -> Result<String> {
        let mut out = String::new();

        if !self.properties.is_empty() {
            out.push_str("---\n");
            let yaml = serde_saphyr::to_string_with_options(
                &self.properties,
                hyalo_serializer_options(self.compact_list_indent),
            )
            .context("failed to serialize YAML")?;
            out.push_str(&yaml);
            // The YAML serializer adds a trailing newline, but let's ensure
            if !yaml.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("---\n");
        }

        out.push_str(&self.body);
        Ok(out)
    }

    /// Get a property value by name.
    #[must_use]
    pub fn get_property(&self, name: &str) -> Option<&Value> {
        self.properties.get(name)
    }

    /// Set a property value.
    pub fn set_property(&mut self, name: String, value: Value) {
        self.properties.insert(name, value);
    }

    /// Remove a property, returning the old value if it existed.
    pub fn remove_property(&mut self, name: &str) -> Option<Value> {
        self.properties.shift_remove(name)
    }
}

/// Read only the YAML frontmatter from a file, stopping as soon as the closing `---` is found.
/// The body is never read into memory. Use this for read-only property operations.
pub fn read_frontmatter(path: &Path) -> Result<IndexMap<String, Value>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    read_frontmatter_from_reader(reader)
}

/// Write updated frontmatter to a file while leaving the body bytes untouched.
///
/// This is the preferred mutation path: it reads only the frontmatter portion, finds
/// the byte offset where the body starts, serializes the new properties, and writes
/// `new_frontmatter + original_body_bytes` back to the file.  The body is never
/// decoded as UTF-8, so there is no risk of re-encoding corruption.
///
/// If `props` is empty (all properties removed), no frontmatter block is written —
/// the file starts directly with the body.
pub fn write_frontmatter(path: &Path, props: &IndexMap<String, Value>) -> Result<()> {
    // --- Step 1: find the byte offset where the body starts and detect indent style ---
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;

    let body_offset = find_body_offset(&mut file)?;

    // Detect list indent style from the existing frontmatter before overwriting
    let compact_list_indent = if body_offset > 0 {
        file.seek(SeekFrom::Start(0))
            .with_context(|| format!("failed to seek in {}", path.display()))?;
        // body_offset is the byte position within the file; on 32-bit targets a
        // frontmatter section larger than 4 GiB would truncate here, but that is
        // unreachable in practice.
        #[allow(clippy::cast_possible_truncation)]
        let mut fm_bytes = vec![0u8; body_offset as usize];
        file.read_exact(&mut fm_bytes)
            .with_context(|| format!("failed to read frontmatter of {}", path.display()))?;
        let fm_str = String::from_utf8_lossy(&fm_bytes);
        // Extract just the YAML content between the --- delimiters
        let yaml_content = fm_str
            .strip_prefix("---\n")
            .or_else(|| fm_str.strip_prefix("---\r\n"))
            .unwrap_or(&fm_str);
        detect_list_indent_style(yaml_content)
    } else {
        false
    };

    // --- Step 2: read the body bytes from that offset ---
    file.seek(SeekFrom::Start(body_offset))
        .with_context(|| format!("failed to seek in {}", path.display()))?;
    let mut body_bytes = Vec::new();
    file.read_to_end(&mut body_bytes)
        .with_context(|| format!("failed to read body of {}", path.display()))?;
    drop(file);

    // --- Step 3: serialize new frontmatter ---
    let mut out: Vec<u8> = Vec::new();
    if !props.is_empty() {
        out.extend_from_slice(b"---\n");
        let yaml = serde_saphyr::to_string_with_options(
            props,
            hyalo_serializer_options(compact_list_indent),
        )
        .context("failed to serialize YAML")?;
        out.extend_from_slice(yaml.as_bytes());
        if !yaml.ends_with('\n') {
            out.push(b'\n');
        }
        out.extend_from_slice(b"---\n");
    }
    out.extend_from_slice(&body_bytes);

    // --- Step 4: write atomically ---
    crate::fs_util::atomic_write(path, &out)
        .with_context(|| format!("failed to write {}", path.display()))?;

    Ok(())
}

/// Find the byte offset in `file` where the body starts (i.e. the byte immediately
/// after the closing `---\n` of the frontmatter block).
///
/// Returns `0` if the file has no frontmatter, which means the entire file is body.
fn find_body_offset(file: &mut File) -> Result<u64> {
    const MAX_FRONTMATTER_LINES: usize = 200;
    const MAX_FRONTMATTER_BYTES: usize = 8 * 1024;

    let mut reader = BufReader::new(&mut *file);
    let mut line = String::new();

    // Peek at the first line
    let n = reader.read_line(&mut line).context("failed to read line")?;
    if n == 0 || line.trim_end_matches(['\n', '\r']) != "---" {
        // No frontmatter — body starts at offset 0
        return Ok(0);
    }

    let mut content_bytes: usize = 0;
    let mut line_count: usize = 0;

    loop {
        line.clear();
        let n = reader.read_line(&mut line).context("failed to read line")?;
        if n == 0 {
            anyhow::bail!(
                "unclosed frontmatter: file starts with `---` but no closing `---` was found"
            );
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.trim() == "---" {
            // Consumed up to and including the closing `---\n`
            break;
        }
        line_count += 1;
        content_bytes += n;
        if line_count > MAX_FRONTMATTER_LINES || content_bytes > MAX_FRONTMATTER_BYTES {
            anyhow::bail!(
                "frontmatter too large (no closing `---` found within {MAX_FRONTMATTER_LINES} lines / {MAX_FRONTMATTER_BYTES} bytes)"
            );
        }
    }

    // The body offset is whatever position the BufReader is now at
    let pos = reader
        .stream_position()
        .context("failed to get stream position")?;
    Ok(pos)
}

/// Skip past frontmatter in a buffered reader. Returns the number of lines consumed
/// (including the opening and closing `---` delimiters). Returns 0 if no frontmatter is present.
/// The reader is left positioned at the first line after the closing `---`.
pub fn skip_frontmatter<R: BufRead>(reader: &mut R, first_line: &str) -> Result<usize> {
    const MAX_FRONTMATTER_LINES: usize = 200;
    const MAX_FRONTMATTER_BYTES: usize = 8 * 1024;

    if first_line.trim() != "---" {
        return Ok(0);
    }

    let mut line_count = 1; // count the opening `---`
    let mut total_bytes = 0;
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = reader.read_line(&mut buf).context("failed to read line")?;
        if n == 0 {
            anyhow::bail!(
                "unclosed frontmatter: file starts with `---` but no closing `---` was found"
            );
        }
        line_count += 1;
        let trimmed = buf.trim_end_matches(['\n', '\r']);
        if trimmed.trim() == "---" {
            break;
        }
        total_bytes += n;
        if line_count - 1 > MAX_FRONTMATTER_LINES || total_bytes > MAX_FRONTMATTER_BYTES {
            anyhow::bail!(
                "frontmatter too large (no closing `---` found within {MAX_FRONTMATTER_LINES} lines / {MAX_FRONTMATTER_BYTES} bytes)"
            );
        }
    }

    Ok(line_count)
}

/// Parse frontmatter from any buffered reader. Stops reading after the closing `---`.
/// Bails out if the frontmatter exceeds a reasonable size (200 lines / 8 KB) to avoid
/// buffering an entire file when the closing delimiter is missing.
///
/// Defense-in-depth: the pre-read line/byte cap is kept even though the parser's
/// `Budget` now enforces its own limits. The pre-read cap stops reading early for
/// files with a missing closing `---`, which the parser budget cannot detect (it
/// only sees the YAML string that was already read).
pub(crate) fn read_frontmatter_from_reader<R: BufRead>(
    reader: R,
) -> Result<IndexMap<String, Value>> {
    const MAX_FRONTMATTER_LINES: usize = 200;
    const MAX_FRONTMATTER_BYTES: usize = 8 * 1024;

    let mut lines = reader.lines();

    // First line must be `---`
    match lines.next() {
        Some(Ok(line)) if line.trim() == "---" => {}
        _ => return Ok(IndexMap::new()),
    }

    let mut yaml = String::new();
    let mut line_count = 0;
    let mut closed = false;
    let mut has_content_lines = false;
    for line in lines {
        has_content_lines = true;
        let line = line.context("failed to read line")?;
        if line.trim() == "---" {
            closed = true;
            break;
        }
        line_count += 1;
        if line_count > MAX_FRONTMATTER_LINES || yaml.len() + line.len() > MAX_FRONTMATTER_BYTES {
            anyhow::bail!(
                "frontmatter too large (no closing `---` found within {MAX_FRONTMATTER_LINES} lines / {MAX_FRONTMATTER_BYTES} bytes)"
            );
        }
        yaml.push_str(&line);
        yaml.push('\n');
    }

    if !closed {
        // A file whose entire content is exactly `---` (with or without a trailing
        // newline) has no lines after the opening delimiter.  Both `"---"` and `"---\n"`
        // produce zero iterations in the `lines()` loop because the line iterator
        // consumes the terminator but yields nothing more.  This mirrors
        // `extract_frontmatter`'s treatment of those inputs as "no frontmatter".
        if !has_content_lines {
            return Ok(IndexMap::new());
        }
        anyhow::bail!(
            "unclosed frontmatter: file starts with `---` but no closing `---` was found"
        );
    }

    if yaml.trim().is_empty() {
        return Ok(IndexMap::new());
    }

    serde_saphyr::from_str_with_options(&yaml, hyalo_options())
        .context("failed to parse YAML frontmatter")
}

/// Extract frontmatter YAML string and the body from a markdown document.
/// Returns `Ok((Some(yaml_content), body))` if frontmatter is found,
/// `Ok((None, full_content))` if no frontmatter is present, or an error if the file
/// starts with `---` but has no closing delimiter (which would cause corruption on write).
#[allow(dead_code)] // Called by Document::parse, which is used in tests only
fn extract_frontmatter(content: &str) -> Result<(Option<&str>, &str)> {
    // Frontmatter must start with `---` on the very first line
    if !content.starts_with("---") {
        return Ok((None, content));
    }

    let after_opening = &content[3..];
    // The opening `---` must be followed by a newline (or be exactly `---`).
    // If after stripping the newline the rest is empty, the file is exactly
    // `---` or `---\n` — no frontmatter content or closing delimiter follows,
    // so treat as "no frontmatter" (consistent with the streaming reader path).
    let after_opening = if let Some(rest) = after_opening.strip_prefix('\n') {
        if rest.is_empty() {
            return Ok((None, content));
        }
        rest
    } else if let Some(rest) = after_opening.strip_prefix("\r\n") {
        if rest.is_empty() {
            return Ok((None, content));
        }
        rest
    } else if after_opening.is_empty() {
        // File is exactly `---` with nothing after
        return Ok((None, content));
    } else {
        return Ok((None, content));
    };

    // Find the closing `---`
    if let Some(pos) = find_closing_delimiter(after_opening) {
        let yaml = &after_opening[..pos];
        let rest = &after_opening[pos + 3..];
        // Skip the newline after closing `---`
        let body = if let Some(stripped) = rest.strip_prefix('\n') {
            stripped
        } else if let Some(stripped) = rest.strip_prefix("\r\n") {
            stripped
        } else {
            rest
        };
        Ok((Some(yaml), body))
    } else {
        // Opening `---` found but no closing delimiter — this is malformed frontmatter.
        // Returning an error prevents mutation commands from corrupting the file by
        // writing a new frontmatter block on top of the unclosed one.
        anyhow::bail!("unclosed frontmatter: file starts with `---` but no closing `---` was found")
    }
}

/// Find the position of `---` at the start of a line in the given string.
#[allow(dead_code)] // Called by extract_frontmatter, which is used in tests only
fn find_closing_delimiter(s: &str) -> Option<usize> {
    // Check if it starts right at position 0
    if s.starts_with("---")
        && (s.len() == 3
            || s.as_bytes().get(3) == Some(&b'\n')
            || s.as_bytes().get(3) == Some(&b'\r'))
    {
        return Some(0);
    }

    // Search for `\n---`
    let mut search_from = 0;
    while let Some(pos) = s[search_from..].find("\n---") {
        let abs_pos = search_from + pos + 1; // position of the first `-`
        let after = abs_pos + 3;
        if after == s.len()
            || s.as_bytes().get(after) == Some(&b'\n')
            || s.as_bytes().get(after) == Some(&b'\r')
        {
            return Some(abs_pos);
        }
        search_from = abs_pos + 3;
    }
    None
}
