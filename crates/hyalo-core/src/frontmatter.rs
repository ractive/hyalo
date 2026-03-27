#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use serde_json::Value;
use serde_saphyr::{Budget, DuplicateKeyPolicy, Options};
use std::collections::BTreeMap;
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

/// Represents parsed frontmatter and the remaining body content.
#[derive(Debug, Clone)]
pub struct Document {
    properties: BTreeMap<String, Value>,
    body: String,
}

impl Document {
    #[must_use]
    pub fn properties(&self) -> &BTreeMap<String, Value> {
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

        let properties: BTreeMap<String, Value> = match yaml_str {
            Some(yaml) if !yaml.trim().is_empty() => {
                serde_saphyr::from_str_with_options(yaml, hyalo_options())
                    .context("failed to parse YAML frontmatter")?
            }
            _ => BTreeMap::new(),
        };

        Ok(Self {
            properties,
            body: body.to_owned(),
        })
    }

    /// Serialize the document back to a string with YAML frontmatter.
    pub fn serialize(&self) -> Result<String> {
        let mut out = String::new();

        if !self.properties.is_empty() {
            out.push_str("---\n");
            let yaml =
                serde_saphyr::to_string(&self.properties).context("failed to serialize YAML")?;
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
        self.properties.remove(name)
    }
}

/// Returns true if the error is a frontmatter parse/structure error (bad YAML, frontmatter
/// too large) as opposed to an I/O error. Parse errors can be safely skipped when processing
/// multiple files; I/O errors should be propagated.
pub fn is_parse_error(err: &anyhow::Error) -> bool {
    !err.chain()
        .any(|cause| cause.downcast_ref::<std::io::Error>().is_some())
}

/// Read only the YAML frontmatter from a file, stopping as soon as the closing `---` is found.
/// The body is never read into memory. Use this for read-only property operations.
pub fn read_frontmatter(path: &Path) -> Result<BTreeMap<String, Value>> {
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
pub fn write_frontmatter(path: &Path, props: &BTreeMap<String, Value>) -> Result<()> {
    // --- Step 1: find the byte offset where the body starts ---
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;

    let body_offset = find_body_offset(&mut file)?;

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
        let yaml = serde_saphyr::to_string(props).context("failed to serialize YAML")?;
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
fn read_frontmatter_from_reader<R: BufRead>(reader: R) -> Result<BTreeMap<String, Value>> {
    const MAX_FRONTMATTER_LINES: usize = 200;
    const MAX_FRONTMATTER_BYTES: usize = 8 * 1024;

    let mut lines = reader.lines();

    // First line must be `---`
    match lines.next() {
        Some(Ok(line)) if line.trim() == "---" => {}
        _ => return Ok(BTreeMap::new()),
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
            return Ok(BTreeMap::new());
        }
        anyhow::bail!(
            "unclosed frontmatter: file starts with `---` but no closing `---` was found"
        );
    }

    if yaml.trim().is_empty() {
        return Ok(BTreeMap::new());
    }

    serde_saphyr::from_str_with_options(&yaml, hyalo_options())
        .context("failed to parse YAML frontmatter")
}

/// Extract frontmatter YAML string and the body from a markdown document.
/// Returns `Ok((Some(yaml_content), body))` if frontmatter is found,
/// `Ok((None, full_content))` if no frontmatter is present, or an error if the file
/// starts with `---` but has no closing delimiter (which would cause corruption on write).
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

/// Infer the Obsidian property type from a YAML value.
#[must_use]
pub fn infer_type(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "checkbox",
        Value::Number(_) => "number",
        Value::Array(_) => "list",
        Value::String(s) => infer_string_type(s),
        Value::Null | Value::Object(_) => "text",
    }
}

/// Infer the type of a string value (date, datetime, or text).
fn infer_string_type(s: &str) -> &'static str {
    if is_date(s) {
        "date"
    } else if is_datetime(s) {
        "datetime"
    } else {
        "text"
    }
}

/// Check if a string matches `YYYY-MM-DD`.
fn is_date(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }
    let b = s.as_bytes();
    b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit)
}

/// Check if a string matches `YYYY-MM-DDThh:mm:ss`.
fn is_datetime(s: &str) -> bool {
    if s.len() != 19 {
        return false;
    }
    let b = s.as_bytes();
    b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit)
        && b[11..13].iter().all(u8::is_ascii_digit)
        && b[14..16].iter().all(u8::is_ascii_digit)
        && b[17..19].iter().all(u8::is_ascii_digit)
}

/// Parse a string value into an appropriate YAML Value, optionally forced to a specific type.
pub fn parse_value(raw: &str, forced_type: Option<&str>) -> Result<Value> {
    match forced_type {
        Some("text") => Ok(Value::String(raw.to_owned())),
        Some("number") => {
            if let Ok(i) = raw.parse::<i64>() {
                Ok(Value::Number(i.into()))
            } else {
                let f: f64 = raw.parse().context("value is not a valid number")?;
                anyhow::ensure!(f.is_finite(), "value is not a finite number");
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .ok_or_else(|| anyhow::anyhow!("value is not a finite number"))
            }
        }
        Some("checkbox") => {
            let b = match raw {
                "true" | "yes" | "1" => true,
                "false" | "no" | "0" => false,
                _ => anyhow::bail!("value is not a valid checkbox (use true/false)"),
            };
            Ok(Value::Bool(b))
        }
        Some("date") => {
            anyhow::ensure!(is_date(raw), "value is not a valid date (YYYY-MM-DD)");
            Ok(Value::String(raw.to_owned()))
        }
        Some("datetime") => {
            anyhow::ensure!(
                is_datetime(raw),
                "value is not a valid datetime (YYYY-MM-DDThh:mm:ss)"
            );
            Ok(Value::String(raw.to_owned()))
        }
        Some("list") => {
            // Parse comma-separated values
            let items: Vec<Value> = raw
                .split(',')
                .map(|s| Value::String(s.trim().to_owned()))
                .collect();
            Ok(Value::Array(items))
        }
        Some(other) => anyhow::bail!("unknown type: {other}"),
        None => Ok(infer_value(raw)),
    }
}

/// Infer a YAML value from a raw string (try number, bool, date, then text).
fn infer_value(raw: &str) -> Value {
    // Try integer
    if let Ok(i) = raw.parse::<i64>() {
        return Value::Number(i.into());
    }
    // Try float (reject NaN/inf which parse successfully but aren't useful property values)
    if let Ok(f) = raw.parse::<f64>()
        && f.is_finite()
    {
        return serde_json::Number::from_f64(f)
            .map_or_else(|| Value::String(raw.to_owned()), Value::Number);
    }
    // Try bool
    match raw {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }
    // Try list: [a, b, c] syntax
    if raw.starts_with('[') && raw.ends_with(']') {
        let inner = &raw[1..raw.len() - 1];
        // Empty brackets = empty list
        if inner.trim().is_empty() {
            return Value::Array(Vec::new());
        }
        // Split by comma, trim each item, keep as strings
        let items: Vec<Value> = inner
            .split(',')
            .map(|s| Value::String(s.trim().to_owned()))
            .collect();
        return Value::Array(items);
    }
    Value::String(raw.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
    }

    #[test]
    fn parse_valid_frontmatter() {
        let content = md!(r"
---
title: Hello
status: draft
---
Body text here.
");
        let doc = Document::parse(content).unwrap();
        assert_eq!(doc.properties().len(), 2);
        assert_eq!(
            doc.get_property("title"),
            Some(&Value::String("Hello".into()))
        );
        assert_eq!(doc.body(), "Body text here.\n");
    }

    #[test]
    fn parse_no_frontmatter() {
        let content = "Just a regular markdown file.\n";
        let doc = Document::parse(content).unwrap();
        assert!(doc.properties().is_empty());
        assert_eq!(doc.body(), content);
    }

    #[test]
    fn parse_empty_frontmatter() {
        let content = md!(r"
---
---
Body.
");
        let doc = Document::parse(content).unwrap();
        assert!(doc.properties().is_empty());
        assert_eq!(doc.body(), "Body.\n");
    }

    #[test]
    fn parse_malformed_frontmatter() {
        // Missing closing delimiter — now returns an error to prevent corruption on write
        let content = md!(r"
---
title: Broken
No closing delimiter.
");
        let err = Document::parse(content).unwrap_err();
        assert!(err.to_string().contains("unclosed frontmatter"));
    }

    #[test]
    fn infer_type_text() {
        assert_eq!(infer_type(&Value::String("hello".into())), "text");
    }

    #[test]
    fn infer_type_number() {
        assert_eq!(infer_type(&Value::Number(42.into())), "number");
    }

    #[test]
    fn infer_type_bool() {
        assert_eq!(infer_type(&Value::Bool(true)), "checkbox");
    }

    #[test]
    fn infer_type_date() {
        assert_eq!(infer_type(&Value::String("2026-03-20".into())), "date");
    }

    #[test]
    fn infer_type_datetime() {
        assert_eq!(
            infer_type(&Value::String("2026-03-20T14:30:00".into())),
            "datetime"
        );
    }

    #[test]
    fn infer_type_list() {
        assert_eq!(
            infer_type(&Value::Array(vec![Value::String("a".into())])),
            "list"
        );
    }

    #[test]
    fn infer_type_null() {
        assert_eq!(infer_type(&Value::Null), "text");
    }

    #[test]
    fn roundtrip_preserves_body() {
        let content = md!(r"
---
title: Test
priority: 5
---
# Heading

Paragraph content.
");
        let doc = Document::parse(content).unwrap();
        let serialized = doc.serialize().unwrap();
        let doc2 = Document::parse(&serialized).unwrap();
        assert_eq!(doc.properties(), doc2.properties());
        assert_eq!(doc.body(), doc2.body());
    }

    #[test]
    fn serialize_no_properties_no_frontmatter() {
        let doc = Document::parse("Just body.\n").unwrap();
        let serialized = doc.serialize().unwrap();
        assert_eq!(serialized, "Just body.\n");
    }

    #[test]
    fn set_and_remove_property() {
        let mut doc = Document::parse(md!(r"
---
title: Hi
---
Body
"))
        .unwrap();
        doc.set_property("status".into(), Value::String("done".into()));
        assert!(doc.get_property("status").is_some());
        doc.remove_property("status");
        assert!(doc.get_property("status").is_none());
    }

    #[test]
    fn parse_value_infer() {
        // Number
        match parse_value("42", None).unwrap() {
            Value::Number(n) => assert_eq!(n.as_i64(), Some(42)),
            other => panic!("expected number, got {other:?}"),
        }
        // Bool
        assert_eq!(parse_value("true", None).unwrap(), Value::Bool(true));
        // Text
        match parse_value("hello", None).unwrap() {
            Value::String(s) => assert_eq!(s, "hello"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn parse_value_forced_type() {
        // Force text even for number-like string
        match parse_value("42", Some("text")).unwrap() {
            Value::String(s) => assert_eq!(s, "42"),
            other => panic!("expected string, got {other:?}"),
        }
        // Force list
        match parse_value("a, b, c", Some("list")).unwrap() {
            Value::Array(items) => assert_eq!(items.len(), 3),
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn file_with_only_frontmatter() {
        let content = md!(r"
---
title: Only FM
---
");
        let doc = Document::parse(content).unwrap();
        assert_eq!(doc.properties().len(), 1);
        assert_eq!(doc.body(), "");
    }

    // --- Streaming reader tests ---

    #[test]
    fn streaming_valid_frontmatter() {
        let input = md!("
---
title: Hello
status: draft
---
# Body that should not be read
Lots of content here.
");
        let props = read_frontmatter_from_reader(input.as_bytes()).unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props.get("title"), Some(&Value::String("Hello".into())));
        assert_eq!(props.get("status"), Some(&Value::String("draft".into())));
    }

    #[test]
    fn streaming_no_frontmatter() {
        let input = md!("
Just a regular file.
No frontmatter.
");
        let props = read_frontmatter_from_reader(input.as_bytes()).unwrap();
        assert!(props.is_empty());
    }

    #[test]
    fn streaming_empty_frontmatter() {
        let input = md!("
---
---
Body.
");
        let props = read_frontmatter_from_reader(input.as_bytes()).unwrap();
        assert!(props.is_empty());
    }

    #[test]
    fn streaming_no_closing_delimiter() {
        // No closing `---` must always error — even if the YAML content is valid.
        let input = md!("
---
title: Broken
Not valid yaml line
");
        let result = read_frontmatter_from_reader(input.as_bytes());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unclosed frontmatter"),
            "expected unclosed frontmatter error"
        );

        // Also errors even when the content happens to be valid YAML
        let input2 = md!("
---
title: Works
status: ok
");
        let result2 = read_frontmatter_from_reader(input2.as_bytes());
        assert!(result2.is_err());
        assert!(
            result2
                .unwrap_err()
                .to_string()
                .contains("unclosed frontmatter"),
            "expected unclosed frontmatter error for valid-YAML-but-unclosed file"
        );
    }

    #[test]
    fn streaming_solo_dash_is_no_frontmatter() {
        // A file whose entire content is exactly `---` (no trailing newline) must be
        // treated as "no frontmatter" — consistent with `extract_frontmatter`.
        let result = read_frontmatter_from_reader("---".as_bytes());
        assert!(
            result.is_ok(),
            "expected Ok for bare `---`, got: {result:?}"
        );
        assert!(
            result.unwrap().is_empty(),
            "expected empty map for bare `---`"
        );

        // Same for `---\n` — indistinguishable from `---` in a line-based reader.
        let result = read_frontmatter_from_reader("---\n".as_bytes());
        assert!(result.is_ok(), "expected Ok for `---\\n`, got: {result:?}");
        assert!(
            result.unwrap().is_empty(),
            "expected empty map for `---\\n`"
        );

        // A file with `---` followed by actual content but no closing delimiter
        // must still error (not silently become "no frontmatter").
        let result = read_frontmatter_from_reader("---\ntitle: X\n".as_bytes());
        assert!(
            result.is_err(),
            "expected Err for unclosed frontmatter with content"
        );
    }

    #[test]
    fn streaming_matches_full_parse() {
        let content = md!(r"
---
title: Test
priority: 5
tags:
  - a
  - b
---
# Heading

Body.
");
        let doc = Document::parse(content).unwrap();
        let streamed = read_frontmatter_from_reader(content.as_bytes()).unwrap();
        assert_eq!(doc.properties(), &streamed);
    }

    // --- Budget boundary tests for skip_frontmatter ---

    fn make_frontmatter_with_n_lines(n: usize) -> String {
        use std::fmt::Write as _;
        // Each content line is "k: v\n" (6 bytes). The closing --- is appended.
        let mut s = String::from("---\n");
        for i in 0..n {
            let _ = writeln!(s, "k{i}: v");
        }
        s.push_str("---\n");
        s
    }

    #[test]
    fn streaming_budget_boundary_lines_at_limit() {
        // Exactly 200 content lines — must succeed
        let input = make_frontmatter_with_n_lines(200);
        let mut reader = input.as_bytes();
        // Read and discard the opening "---\n" line, then call skip_frontmatter
        let mut first_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
        let first = first_line.trim_end_matches(['\n', '\r']);
        let result = skip_frontmatter(&mut reader, first);
        assert!(
            result.is_ok(),
            "200 content lines should succeed: {result:?}"
        );
    }

    #[test]
    fn streaming_budget_boundary_lines_over_limit() {
        // 201 content lines — must error
        let input = make_frontmatter_with_n_lines(201);
        let mut reader = input.as_bytes();
        let mut first_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
        let first = first_line.trim_end_matches(['\n', '\r']);
        let result = skip_frontmatter(&mut reader, first);
        assert!(result.is_err(), "201 content lines should fail");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("frontmatter too large")
        );
    }

    #[test]
    fn streaming_budget_boundary_bytes_at_limit() {
        // Build frontmatter whose content is just under or equal to 8192 bytes.
        // skip_frontmatter counts raw bytes from read_line (including \n).
        // Use a single long line of exactly 8192 bytes (including the \n).
        // "x: " (3 bytes) + value + "\n" = 8192 → value = 8188 bytes of 'a'
        let value = "a".repeat(8188);
        let input = format!("---\nx: {value}\n---\n");
        let mut reader = input.as_bytes();
        let mut first_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
        let first = first_line.trim_end_matches(['\n', '\r']);
        let result = skip_frontmatter(&mut reader, first);
        assert!(
            result.is_ok(),
            "8192-byte content should succeed: {result:?}"
        );
    }

    #[test]
    fn streaming_budget_boundary_bytes_over_limit() {
        // Content line of 8193 bytes (including \n) — must error
        let value = "a".repeat(8189); // "x: " (3) + 8189 + "\n" = 8193
        let input = format!("---\nx: {value}\n---\n");
        let mut reader = input.as_bytes();
        let mut first_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
        let first = first_line.trim_end_matches(['\n', '\r']);
        let result = skip_frontmatter(&mut reader, first);
        assert!(result.is_err(), "8193-byte content should fail");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("frontmatter too large")
        );
    }

    #[test]
    fn is_parse_error_true_for_yaml_error() {
        let err =
            read_frontmatter_from_reader("---\n: invalid [[[{\n---\n".as_bytes()).unwrap_err();
        assert!(is_parse_error(&err), "expected parse error: {err}");
    }

    #[test]
    fn is_parse_error_false_for_io_error() {
        let err = read_frontmatter(Path::new("/nonexistent/path/file.md")).unwrap_err();
        assert!(!is_parse_error(&err), "expected I/O error: {err}");
    }

    #[test]
    fn infer_value_list_basic() {
        match parse_value("[a, b, c]", None).unwrap() {
            Value::Array(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], Value::String("a".to_owned()));
                assert_eq!(items[1], Value::String("b".to_owned()));
                assert_eq!(items[2], Value::String("c".to_owned()));
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn infer_value_list_empty() {
        match parse_value("[]", None).unwrap() {
            Value::Array(items) => assert!(items.is_empty()),
            other => panic!("expected empty sequence, got {other:?}"),
        }
    }

    #[test]
    fn infer_value_list_single_item() {
        match parse_value("[single]", None).unwrap() {
            Value::Array(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0], Value::String("single".to_owned()));
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn infer_value_not_a_list() {
        // Value that contains brackets but doesn't start with [ should remain string
        match parse_value("not [a list]", None).unwrap() {
            Value::String(s) => assert_eq!(s, "not [a list]"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn infer_value_list_whitespace_trimmed() {
        match parse_value("[  a , b ,  c  ]", None).unwrap() {
            Value::Array(items) => {
                assert_eq!(items[0], Value::String("a".to_owned()));
                assert_eq!(items[1], Value::String("b".to_owned()));
                assert_eq!(items[2], Value::String("c".to_owned()));
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    // --- Hardened parser option tests ---

    #[test]
    fn rejects_deeply_nested_yaml() {
        // Depth > 20 must be rejected by the budget
        let mut yaml = String::from("---\n");
        for i in 0..25 {
            yaml.push_str(&"  ".repeat(i));
            yaml.push_str(&format!("l{i}:\n"));
        }
        yaml.push_str(&"  ".repeat(25));
        yaml.push_str("val: 1\n");
        yaml.push_str("---\nBody\n");

        let err = Document::parse(&yaml);
        assert!(err.is_err(), "deeply nested YAML should be rejected");
    }

    #[test]
    fn rejects_yaml_with_aliases() {
        // Aliases (max_aliases: 0) must be rejected
        let content = "---\nanchor: &a value\nalias: *a\n---\nBody\n";
        let err = Document::parse(content);
        assert!(err.is_err(), "YAML with aliases should be rejected");
    }

    #[test]
    fn rejects_duplicate_keys() {
        let content = "---\ntitle: First\ntitle: Second\n---\nBody\n";
        let err = Document::parse(content).unwrap_err();
        let chain = format!("{err:?}").to_lowercase();
        assert!(
            chain.contains("duplicate"),
            "error chain should mention duplicate, got: {err:?}"
        );
    }

    #[test]
    fn strict_booleans_yes_is_string() {
        // With strict_booleans, `yes` must parse as a string, not a boolean
        let content = "---\nflag: yes\n---\nBody\n";
        let doc = Document::parse(content).unwrap();
        assert_eq!(
            doc.get_property("flag"),
            Some(&Value::String("yes".into())),
            "`yes` should be parsed as string with strict booleans"
        );
    }

    #[test]
    fn strict_booleans_no_is_string() {
        let content = "---\nflag: no\n---\nBody\n";
        let doc = Document::parse(content).unwrap();
        assert_eq!(
            doc.get_property("flag"),
            Some(&Value::String("no".into())),
            "`no` should be parsed as string with strict booleans"
        );
    }

    #[test]
    fn strict_booleans_on_off_are_strings() {
        let content = "---\na: on\nb: off\n---\nBody\n";
        let doc = Document::parse(content).unwrap();
        assert_eq!(doc.get_property("a"), Some(&Value::String("on".into())));
        assert_eq!(doc.get_property("b"), Some(&Value::String("off".into())));
    }

    #[test]
    fn strict_booleans_true_false_still_booleans() {
        let content = "---\na: true\nb: false\n---\nBody\n";
        let doc = Document::parse(content).unwrap();
        assert_eq!(doc.get_property("a"), Some(&Value::Bool(true)));
        assert_eq!(doc.get_property("b"), Some(&Value::Bool(false)));
    }

    #[test]
    fn streaming_rejects_duplicate_keys() {
        let input = "---\ntitle: First\ntitle: Second\n---\nBody\n";
        let result = read_frontmatter_from_reader(input.as_bytes());
        assert!(
            result.is_err(),
            "streaming parser should reject duplicate keys"
        );
    }

    #[test]
    fn streaming_strict_booleans() {
        let input = "---\nflag: yes\n---\nBody\n";
        let props = read_frontmatter_from_reader(input.as_bytes()).unwrap();
        assert_eq!(
            props.get("flag"),
            Some(&Value::String("yes".into())),
            "streaming parser should treat `yes` as string"
        );
    }
}
