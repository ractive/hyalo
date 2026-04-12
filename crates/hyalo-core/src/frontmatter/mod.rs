#![allow(clippy::missing_errors_doc)]

mod parse;
mod types;

pub use parse::{body_only, hyalo_options, read_frontmatter, skip_frontmatter, write_frontmatter};
pub use types::{infer_type, parse_value};

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

#[cfg(test)]
mod tests {
    use super::*;
    use parse::{Document, detect_list_indent_style, read_frontmatter_from_reader};
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
}
