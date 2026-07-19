#![allow(clippy::missing_errors_doc)]

mod parse;
mod types;

use anyhow::Context as _;

pub use parse::{
    FrontmatterBudgetError, body_only, check_frontmatter_size_budget, hyalo_options,
    read_frontmatter, skip_frontmatter, write_frontmatter,
};
pub(crate) use parse::{is_closing_delimiter, is_opening_delimiter};
pub use types::{infer_type, parse_value};

/// Maximum number of content lines in a YAML frontmatter block.
///
/// Applies to both the read-side parser budget and the write-side pre-flight
/// check. Any frontmatter that would exceed this limit is rejected at write
/// time; any file that already exceeds it is skipped with a warning on read.
pub const MAX_FRONTMATTER_LINES: usize = 2000;

/// Maximum byte size of YAML frontmatter content (excluding `---` delimiters).
///
/// Applies to both the read-side parser budget and the write-side pre-flight
/// check. 64 KiB is generous — real-world frontmatter is well under 8 KiB at
/// the 99th percentile.
pub const MAX_FRONTMATTER_BYTES: usize = 64 * 1024;

/// Read a file's modification time and size for TOCTOU detection.
///
/// Returns `(mtime, file_size_bytes)`. Using both components strengthens the
/// fingerprint: on filesystems with coarse mtime granularity (e.g. FAT32 with
/// 2 s resolution), a file that is written and reverted within the same clock
/// tick would have the same mtime but a different size, and vice versa.
pub fn read_mtime(path: &std::path::Path) -> anyhow::Result<(std::time::SystemTime, u64)> {
    let meta =
        std::fs::metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    let mtime = meta
        .modified()
        .with_context(|| format!("mtime not available for {}", path.display()))?;
    Ok((mtime, meta.len()))
}

/// Verify that a file's fingerprint (mtime + size) hasn't changed since `expected`.
/// Returns an error if the file was modified concurrently.
pub fn check_mtime(
    path: &std::path::Path,
    expected: (std::time::SystemTime, u64),
) -> anyhow::Result<()> {
    let current = read_mtime(path)?;
    if current != expected {
        anyhow::bail!(
            "file {} was modified by another process during operation",
            path.display()
        );
    }
    Ok(())
}

/// A typed error for frontmatter parse and structural failures.
///
/// Covers bad YAML, unclosed `---` delimiters, oversized frontmatter blocks, and
/// any other condition where the file content itself is the problem. These errors
/// can be safely skipped (with a warning) when processing multiple files.
///
/// I/O errors are **not** represented here — they propagate as plain `anyhow::Error`
/// wrapping `std::io::Error`, so callers can still distinguish them via
/// [`is_parse_error`] or by downcasting directly.
#[derive(Debug)]
pub struct FrontmatterError(pub(crate) String);

impl std::fmt::Display for FrontmatterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FrontmatterError {}

/// Returns `true` if the error originates from a frontmatter parse or structural problem
/// (bad YAML, unclosed delimiter, oversized frontmatter) rather than an I/O failure.
///
/// Parse errors can be safely skipped when processing multiple files; I/O errors
/// should be propagated to the caller.
pub fn is_parse_error(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|cause| cause.downcast_ref::<FrontmatterError>().is_some())
}

/// If `err` is a [`FrontmatterBudgetError`] (frontmatter write would exceed the
/// size budget), return a reference to it so the caller can emit a structured
/// user error instead of an internal failure.
pub fn as_budget_error(err: &anyhow::Error) -> Option<&FrontmatterBudgetError> {
    err.chain()
        .find_map(|cause| cause.downcast_ref::<FrontmatterBudgetError>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use parse::{
        Document, LineEnding, detect_list_indent_style, opening_delimiter,
        read_frontmatter_from_reader,
    };
    use serde_json::Value;
    use std::fmt::Write as _;
    use std::path::Path;

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
        assert_eq!(infer_type(&Value::Null), "null");
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

    // --- skip_frontmatter must share the opening-delimiter policy ---

    #[test]
    fn skip_frontmatter_recognizes_bom_prefixed_opening() {
        let input = "\u{feff}---\ntitle: Note\n---\nBody.\n";
        let mut reader = input.as_bytes();
        let mut first_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
        let consumed = skip_frontmatter(&mut reader, &first_line).unwrap();
        assert_eq!(consumed, 3, "BOM + `---` must open frontmatter");
        // Reader is positioned at the body.
        let mut body = String::new();
        std::io::Read::read_to_string(&mut reader, &mut body).unwrap();
        assert_eq!(body, "Body.\n");
    }

    #[test]
    fn skip_frontmatter_rejects_leading_whitespace_opening() {
        let input = " ---\ntitle: Note\n---\nBody.\n";
        let mut reader = input.as_bytes();
        let mut first_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
        let consumed = skip_frontmatter(&mut reader, &first_line).unwrap();
        assert_eq!(consumed, 0, "` ---` must NOT open frontmatter");
    }

    // --- Budget boundary tests for skip_frontmatter ---

    fn make_frontmatter_with_n_lines(n: usize) -> String {
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
        // Exactly MAX_FRONTMATTER_LINES (2000) content lines — must succeed
        use super::MAX_FRONTMATTER_LINES;
        let input = make_frontmatter_with_n_lines(MAX_FRONTMATTER_LINES);
        let mut reader = input.as_bytes();
        // Read and discard the opening "---\n" line, then call skip_frontmatter
        let mut first_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
        let first = first_line.trim_end_matches(['\n', '\r']);
        let result = skip_frontmatter(&mut reader, first);
        assert!(
            result.is_ok(),
            "{MAX_FRONTMATTER_LINES} content lines should succeed: {result:?}"
        );
    }

    #[test]
    fn streaming_budget_boundary_lines_over_limit() {
        // MAX_FRONTMATTER_LINES + 1 content lines — must error
        use super::MAX_FRONTMATTER_LINES;
        let input = make_frontmatter_with_n_lines(MAX_FRONTMATTER_LINES + 1);
        let mut reader = input.as_bytes();
        let mut first_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
        let first = first_line.trim_end_matches(['\n', '\r']);
        let result = skip_frontmatter(&mut reader, first);
        assert!(
            result.is_err(),
            "{} content lines should fail",
            MAX_FRONTMATTER_LINES + 1
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("frontmatter too large")
        );
    }

    #[test]
    fn streaming_budget_boundary_bytes_at_limit() {
        // Build frontmatter whose content is exactly MAX_FRONTMATTER_BYTES.
        // skip_frontmatter counts raw bytes from read_line (including \n).
        // Use a single long line: "x: " (3 bytes) + value + "\n" = MAX_FRONTMATTER_BYTES
        use super::MAX_FRONTMATTER_BYTES;
        let value = "a".repeat(MAX_FRONTMATTER_BYTES - 4); // "x: " (3) + value + "\n" = limit
        let input = format!("---\nx: {value}\n---\n");
        let mut reader = input.as_bytes();
        let mut first_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
        let first = first_line.trim_end_matches(['\n', '\r']);
        let result = skip_frontmatter(&mut reader, first);
        assert!(
            result.is_ok(),
            "{MAX_FRONTMATTER_BYTES}-byte content should succeed: {result:?}"
        );
    }

    #[test]
    fn streaming_budget_boundary_bytes_over_limit() {
        // Content line of MAX_FRONTMATTER_BYTES + 1 bytes (including \n) — must error
        use super::MAX_FRONTMATTER_BYTES;
        let value = "a".repeat(MAX_FRONTMATTER_BYTES - 3); // "x: " (3) + value + "\n" = limit + 1
        let input = format!("---\nx: {value}\n---\n");
        let mut reader = input.as_bytes();
        let mut first_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
        let first = first_line.trim_end_matches(['\n', '\r']);
        let result = skip_frontmatter(&mut reader, first);
        assert!(
            result.is_err(),
            "{} byte content should fail",
            MAX_FRONTMATTER_BYTES + 1
        );
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
    fn is_parse_error_true_for_unclosed_frontmatter() {
        let err = read_frontmatter_from_reader("---\ntitle: Broken\n".as_bytes()).unwrap_err();
        assert!(
            is_parse_error(&err),
            "unclosed frontmatter should be a parse error: {err}"
        );
    }

    #[test]
    fn is_parse_error_false_for_io_error() {
        let err = read_frontmatter(Path::new("/nonexistent/path/file.md")).unwrap_err();
        assert!(!is_parse_error(&err), "expected I/O error: {err}");
    }

    #[test]
    fn frontmatter_error_is_directly_downcastable() {
        let err =
            read_frontmatter_from_reader("---\n: invalid [[[{\n---\n".as_bytes()).unwrap_err();
        let found = err
            .chain()
            .any(|cause| cause.downcast_ref::<FrontmatterError>().is_some());
        assert!(
            found,
            "FrontmatterError should be downcastable from anyhow::Error: {err}"
        );
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
            let _ = writeln!(yaml, "l{i}:");
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

    // --- Key order preservation tests ---

    #[test]
    fn roundtrip_preserves_key_order() {
        let content = md!(r"
---
title: Hello
type: iteration
date: 2026-03-27
status: planned
branch: iter-54/test
---
Body.
");
        let doc = Document::parse(content).unwrap();
        let serialized = doc.serialize().unwrap();
        let doc2 = Document::parse(&serialized).unwrap();
        let keys: Vec<&str> = doc2.properties().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec!["title", "type", "date", "status", "branch"],
            "key order should be preserved through roundtrip"
        );
    }

    #[test]
    fn roundtrip_preserves_key_order_after_mutation() {
        let content = md!(r"
---
title: Hello
type: iteration
date: 2026-03-27
status: planned
---
Body.
");
        let mut doc = Document::parse(content).unwrap();
        doc.set_property("status".into(), Value::String("completed".into()));
        let serialized = doc.serialize().unwrap();
        let doc2 = Document::parse(&serialized).unwrap();
        let keys: Vec<&str> = doc2.properties().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec!["title", "type", "date", "status"],
            "key order should be preserved after mutation"
        );
    }

    // --- List indent style detection tests ---

    #[test]
    fn detect_compact_list_style() {
        let yaml = "title: Test\ntags:\n- a\n- b\n";
        assert!(
            detect_list_indent_style(yaml),
            "flush `- item` should be detected as compact"
        );
    }

    #[test]
    fn detect_indented_list_style() {
        let yaml = "title: Test\ntags:\n  - a\n  - b\n";
        assert!(
            !detect_list_indent_style(yaml),
            "indented `  - item` should be detected as non-compact"
        );
    }

    #[test]
    fn detect_indented_list_with_comment_after_key() {
        let yaml = "tags: # my tags\n  - a\n  - b\n";
        assert!(
            !detect_list_indent_style(yaml),
            "comment after key colon should still detect indented style"
        );
    }

    #[test]
    fn detect_indented_list_with_blank_line_between() {
        let yaml = "tags:\n\n  - a\n  - b\n";
        assert!(
            !detect_list_indent_style(yaml),
            "blank line between key and sequence should not break detection"
        );
    }

    #[test]
    fn detect_indented_list_with_comment_line_between() {
        let yaml = "tags:\n  # note\n  - a\n  - b\n";
        assert!(
            !detect_list_indent_style(yaml),
            "comment line between key and sequence should not break detection"
        );
    }

    #[test]
    fn detect_no_sequences_defaults_to_non_compact() {
        let yaml = "title: Test\nstatus: draft\n";
        assert!(
            !detect_list_indent_style(yaml),
            "no sequences should default to non-compact"
        );
    }

    #[test]
    fn roundtrip_compact_list_style() {
        let content = md!(r"
---
title: Test
tags:
- a
- b
---
Body.
");
        let doc = Document::parse(content).unwrap();
        let serialized = doc.serialize().unwrap();
        assert!(
            serialized.contains("tags:\n- a\n- b"),
            "compact list style should be preserved: {serialized}"
        );
    }

    // --- Budget check unit tests ---

    #[test]
    fn check_budget_passes_under_limits() {
        let yaml = "title: Hello\nstatus: draft\n";
        let result = check_frontmatter_size_budget(yaml, std::path::Path::new("test.md"));
        assert!(result.is_ok(), "small frontmatter should pass budget check");
    }

    #[test]
    fn check_budget_fails_over_byte_limit() {
        // Construct YAML that's just over the byte budget
        let big_value = "a".repeat(MAX_FRONTMATTER_BYTES + 1);
        let yaml = format!("x: {big_value}\n");
        let result = check_frontmatter_size_budget(&yaml, std::path::Path::new("test.md"));
        assert!(result.is_err(), "over-budget YAML should be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.limit_bytes, MAX_FRONTMATTER_BYTES);
        assert!(err.would_be_bytes > MAX_FRONTMATTER_BYTES);
        assert_eq!(err.file, "test.md");
    }

    #[test]
    fn check_budget_fails_over_line_limit() {
        // Construct YAML with MAX_FRONTMATTER_LINES + 1 lines
        let mut yaml = String::new();
        for i in 0..=MAX_FRONTMATTER_LINES {
            let _ = writeln!(yaml, "k{i}: v");
        }
        let result = check_frontmatter_size_budget(&yaml, std::path::Path::new("test.md"));
        assert!(result.is_err(), "over-line-limit YAML should be rejected");
    }

    #[test]
    fn check_budget_at_exact_byte_limit_passes() {
        // Exactly MAX_FRONTMATTER_BYTES — should pass (limit is exclusive)
        let value = "a".repeat(MAX_FRONTMATTER_BYTES - 4); // "x: " (3) + value + "\n" = MAX
        let yaml = format!("x: {value}\n");
        assert_eq!(yaml.len(), MAX_FRONTMATTER_BYTES);
        let result = check_frontmatter_size_budget(&yaml, std::path::Path::new("test.md"));
        assert!(
            result.is_ok(),
            "exactly-at-limit YAML should pass: {result:?}"
        );
    }

    #[test]
    fn write_frontmatter_rejects_over_budget() {
        use indexmap::IndexMap;
        use serde_json::Value;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.md");
        // Write a minimal valid file first
        std::fs::write(&path, "---\ntitle: Hello\n---\nBody\n").unwrap();

        // Build a props map with an over-budget value
        let mut props: IndexMap<String, Value> = IndexMap::new();
        let big_val = "a".repeat(MAX_FRONTMATTER_BYTES + 100);
        props.insert("x".to_owned(), Value::String(big_val));

        let result = write_frontmatter(&path, &props);
        assert!(
            result.is_err(),
            "write_frontmatter should reject over-budget props"
        );
        let err = result.unwrap_err();
        assert!(
            err.downcast_ref::<FrontmatterBudgetError>().is_some()
                || err
                    .chain()
                    .any(|c| c.downcast_ref::<FrontmatterBudgetError>().is_some()),
            "error should be or contain FrontmatterBudgetError: {err}"
        );

        // File must be unmodified (no write occurred)
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("title: Hello"),
            "file must not be modified after rejected write: {content}"
        );
    }

    #[test]
    fn as_budget_error_extracts_from_anyhow() {
        use indexmap::IndexMap;
        use serde_json::Value;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.md");
        std::fs::write(&path, "---\ntitle: Hello\n---\nBody\n").unwrap();

        let mut props: IndexMap<String, Value> = IndexMap::new();
        let big_val = "a".repeat(MAX_FRONTMATTER_BYTES + 100);
        props.insert("x".to_owned(), Value::String(big_val));

        let err = write_frontmatter(&path, &props).unwrap_err();
        let budget = as_budget_error(&err);
        assert!(
            budget.is_some(),
            "as_budget_error should extract FrontmatterBudgetError"
        );
        let b = budget.unwrap();
        assert_eq!(b.limit_bytes, MAX_FRONTMATTER_BYTES);
        assert!(b.would_be_bytes > MAX_FRONTMATTER_BYTES);
    }

    #[test]
    fn roundtrip_indented_list_style() {
        let content = md!(r"
---
title: Test
tags:
  - a
  - b
---
Body.
");
        let doc = Document::parse(content).unwrap();
        let serialized = doc.serialize().unwrap();
        assert!(
            serialized.contains("tags:\n  - a\n  - b"),
            "indented list style should be preserved: {serialized}"
        );
    }

    // ---------------------------------------------------------------------------
    // Opening-delimiter policy tests (iter-158 C-1: read/write predicate drift)
    // ---------------------------------------------------------------------------

    #[test]
    fn opening_delimiter_plain_lf() {
        let d = opening_delimiter("---\n").unwrap();
        assert!(!d.has_bom);
        assert_eq!(d.line_ending, LineEnding::Lf);
    }

    #[test]
    fn opening_delimiter_plain_crlf() {
        let d = opening_delimiter("---\r\n").unwrap();
        assert!(!d.has_bom);
        assert_eq!(d.line_ending, LineEnding::CrLf);
    }

    #[test]
    fn opening_delimiter_bom_lf() {
        let d = opening_delimiter("\u{feff}---\n").unwrap();
        assert!(d.has_bom);
        assert_eq!(d.line_ending, LineEnding::Lf);
    }

    #[test]
    fn opening_delimiter_bom_crlf() {
        let d = opening_delimiter("\u{feff}---\r\n").unwrap();
        assert!(d.has_bom);
        assert_eq!(d.line_ending, LineEnding::CrLf);
    }

    #[test]
    fn opening_delimiter_bare_no_terminator() {
        // End-of-input with no trailing newline at all (e.g. a file that is
        // exactly `---`) still opens a delimiter line; callers decide what
        // "nothing follows" means.
        assert!(opening_delimiter("---").is_some());
    }

    #[test]
    fn opening_delimiter_rejects_leading_whitespace() {
        assert!(
            opening_delimiter(" ---\n").is_none(),
            "leading whitespace before `---` must not open frontmatter"
        );
    }

    #[test]
    fn opening_delimiter_rejects_extra_dashes() {
        assert!(opening_delimiter("----\n").is_none());
    }

    #[test]
    fn opening_delimiter_rejects_trailing_junk() {
        assert!(opening_delimiter("--- \n").is_none());
        assert!(opening_delimiter("---x\n").is_none());
    }

    #[test]
    fn opening_delimiter_rejects_non_dash_line() {
        assert!(opening_delimiter("title: x\n").is_none());
    }

    // ---------------------------------------------------------------------------
    // BOM handling (iter-158 C-1a)
    // ---------------------------------------------------------------------------

    #[test]
    fn streaming_bom_frontmatter_is_recognized() {
        let input = "\u{feff}---\ntitle: Note\nstatus: draft\n---\nBody.\n";
        let props = read_frontmatter_from_reader(input.as_bytes()).unwrap();
        assert_eq!(props.get("title"), Some(&Value::String("Note".into())));
        assert_eq!(props.get("status"), Some(&Value::String("draft".into())));
    }

    #[test]
    fn extract_frontmatter_bom_matches_streaming_reader() {
        // Document::parse (backed by extract_frontmatter) must agree with
        // read_frontmatter_from_reader on a BOM-prefixed document: both must
        // see the same properties, not "no frontmatter" for one and real
        // properties for the other.
        let content = "\u{feff}---\ntitle: Note\n---\nBody.\n";
        let doc = Document::parse(content).unwrap();
        let streamed = read_frontmatter_from_reader(content.as_bytes()).unwrap();
        assert_eq!(doc.properties(), &streamed);
        assert_eq!(
            doc.properties().get("title"),
            Some(&Value::String("Note".into()))
        );
        assert_eq!(doc.body(), "Body.\n");
    }

    #[test]
    fn write_frontmatter_preserves_bom_and_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bom.md");
        std::fs::write(
            &path,
            "\u{feff}---\ntitle: Note\nstatus: draft\n---\nBody.\n",
        )
        .unwrap();

        let mut props = read_frontmatter(&path).unwrap();
        assert_eq!(
            props.get("status"),
            Some(&Value::String("draft".into())),
            "read path must see the real frontmatter, not treat the BOM file as empty"
        );
        props.insert("status".to_owned(), Value::String("done".into()));
        write_frontmatter(&path, &props).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(
            bytes.starts_with("\u{feff}---\n".as_bytes()),
            "BOM must be preserved at the start of the file"
        );
        let content = String::from_utf8(bytes).unwrap();
        // Exactly one frontmatter block: only two `---` delimiter lines.
        assert_eq!(
            content.matches("---\n").count(),
            2,
            "expected exactly one frontmatter block, got:\n{content}"
        );
        assert!(content.contains("status: done"), "content:\n{content}");
        assert!(content.ends_with("Body.\n"), "body corrupted:\n{content}");

        // remove must also work against the BOM file and actually remove the key.
        let mut props = read_frontmatter(&path).unwrap();
        props.shift_remove("status");
        write_frontmatter(&path, &props).unwrap();
        let final_props = read_frontmatter(&path).unwrap();
        assert!(
            final_props.get("status").is_none(),
            "status should have been removed: {final_props:?}"
        );
        let final_bytes = std::fs::read(&path).unwrap();
        assert!(
            final_bytes.starts_with("\u{feff}".as_bytes()),
            "BOM must still be preserved after removing the last mutated property"
        );
    }

    // ---------------------------------------------------------------------------
    // Leading-whitespace pseudo-frontmatter (iter-158 C-1b)
    // ---------------------------------------------------------------------------

    #[test]
    fn streaming_leading_space_is_no_frontmatter() {
        let input = " ---\ntitle: Note\nstatus: draft\n---\nBody.\n";
        let props = read_frontmatter_from_reader(input.as_bytes()).unwrap();
        assert!(
            props.is_empty(),
            "leading whitespace before `---` must not be treated as frontmatter: {props:?}"
        );
    }

    #[test]
    fn write_frontmatter_leading_space_prepends_without_duplicating() {
        use indexmap::IndexMap;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("space.md");
        let original = " ---\ntitle: Note\nstatus: draft\n---\nBody.\n";
        std::fs::write(&path, original).unwrap();

        // read_frontmatter must agree with write_frontmatter: no frontmatter here.
        let props = read_frontmatter(&path).unwrap();
        assert!(props.is_empty());

        let mut new_props = IndexMap::new();
        new_props.insert("status".to_owned(), Value::String("done".into()));
        write_frontmatter(&path, &new_props).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // Exactly one new frontmatter block at the very top...
        assert!(
            content.starts_with("---\nstatus: done\n---\n"),
            "expected a single new frontmatter block at top:\n{content}"
        );
        // ...and the old pseudo-block survives untouched as body content: the
        // whole original file (byte-for-byte) is now the tail of the new one.
        assert!(
            content.ends_with(original),
            "old pseudo-block must be preserved verbatim in the body:\n{content}"
        );
        assert_eq!(
            content.len(),
            "---\nstatus: done\n---\n".len() + original.len(),
            "new block should be prepended, not merged/duplicated:\n{content}"
        );
    }

    // ---------------------------------------------------------------------------
    // Bare `---` with no closing delimiter and nothing else in the file
    // ---------------------------------------------------------------------------

    #[test]
    fn write_frontmatter_bare_dash_is_not_unclosed_error() {
        use indexmap::IndexMap;

        // A file that is exactly `---\n` has no frontmatter per
        // read_frontmatter_from_reader (see streaming_solo_dash_is_no_frontmatter).
        // find_body_offset must agree, or write_frontmatter would spuriously
        // fail with "unclosed frontmatter" on a file read_frontmatter reports
        // as having no properties at all.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bare.md");
        std::fs::write(&path, "---\n").unwrap();

        assert!(read_frontmatter(&path).unwrap().is_empty());

        let mut props = IndexMap::new();
        props.insert("status".to_owned(), Value::String("done".into()));
        write_frontmatter(&path, &props).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("---\nstatus: done\n---\n"), "{content}");
        assert!(
            content.ends_with("---\n"),
            "original bare dash lost: {content}"
        );
    }

    // ---------------------------------------------------------------------------
    // CRLF preservation (iter-158 C-1 MEDIUM)
    // ---------------------------------------------------------------------------

    #[test]
    #[allow(clippy::naive_bytecount)] // Test-only, tiny buffer; not worth a new dependency.
    fn write_frontmatter_crlf_round_trips_uniformly() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("crlf.md");
        std::fs::write(
            &path,
            "---\r\ntitle: Note\r\nstatus: draft\r\n---\r\nBody.\r\n",
        )
        .unwrap();

        let mut props = read_frontmatter(&path).unwrap();
        props.insert("status".to_owned(), Value::String("done".into()));
        write_frontmatter(&path, &props).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let lf_count = bytes.iter().filter(|&&b| b == b'\n').count();
        let crlf_count = bytes.windows(2).filter(|w| w == b"\r\n").count();
        assert_eq!(
            lf_count,
            crlf_count,
            "every LF must be part of a CRLF pair: {:?}",
            String::from_utf8_lossy(&bytes)
        );
        assert!(
            String::from_utf8_lossy(&bytes).contains("status: done\r\n"),
            "content: {:?}",
            String::from_utf8_lossy(&bytes)
        );
    }

    // ---------------------------------------------------------------------------
    // Oversized-file guard (iter-158 C-1 MEDIUM)
    // ---------------------------------------------------------------------------

    #[test]
    fn write_frontmatter_rejects_oversized_file() {
        use indexmap::IndexMap;
        use std::io::Read as _;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("big.md");
        std::fs::write(&path, "---\ntitle: Note\n---\nBody.\n").unwrap();
        // Sparse-extend well past MAX_FILE_SIZE without writing real data.
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(crate::scanner::MAX_FILE_SIZE + 1).unwrap();
        drop(file);

        let mut props = IndexMap::new();
        props.insert("status".to_owned(), Value::String("done".into()));
        let result = write_frontmatter(&path, &props);
        assert!(
            result.is_err(),
            "oversized file must be refused, not silently rewritten"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("MiB") && err.contains("exceeds") && err.contains("limit"),
            "expected a size-limit error message, got: {err}"
        );

        // File must be untouched: still its original length, and the readable
        // prefix must still be the original frontmatter (not corrupted).
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(meta.len(), crate::scanner::MAX_FILE_SIZE + 1);
        let original = b"---\ntitle: Note\n---\nBody.\n";
        let mut prefix = vec![0u8; original.len()];
        std::fs::File::open(&path)
            .unwrap()
            .read_exact(&mut prefix)
            .unwrap();
        assert_eq!(prefix, original);
    }
}
