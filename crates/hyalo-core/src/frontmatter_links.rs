//! Wikilinks written inside YAML frontmatter values (iter-262, dogfood BUG-1).
//!
//! Obsidian treats every `[[wikilink]]` in a frontmatter value as a graph edge,
//! wherever it appears: `related:`, `categories: ["[[Books]]"]`,
//! `type: "[[Author]]"`, a nested map, a block list. hyalo used to scan a fixed
//! four-property allow-list ([`DEFAULT_FRONTMATTER_LINK_PROPERTIES`]) and read
//! the *parsed* value, so on a real Obsidian vault `backlinks
//! Categories/Books.md` came back empty while three files pointed at it through
//! `categories:`.
//!
//! [`DEFAULT_FRONTMATTER_LINK_PROPERTIES`]: crate::link_graph::DEFAULT_FRONTMATTER_LINK_PROPERTIES
//!
//! # Why the raw block and not the parsed map
//!
//! Reading parsed values cannot see two things that matter here.
//!
//! * **Line numbers.** serde hands over values, not spans, and every consumer
//!   of a link — `backlinks`, `find --fields links`, HYALO006 — reports the
//!   1-based source line.
//! * **Unquoted wikilinks.** `related: [[Books]]` is not a string to YAML: it
//!   parses as a sequence containing a sequence containing `Books`, so the
//!   brackets are gone by the time a value walk sees it, and the link
//!   disappears.
//!
//! So the scan is line-oriented over the raw block, the same deliberately
//! simple bracket scan `mv` and `links fix` already use to *rewrite*
//! frontmatter wikilinks ([`crate::link_rewrite`]) — which is what keeps
//! "hyalo counted this link" and "hyalo rewrote this link" the same set. Keys
//! are inferred from indentation, which is enough to attribute a link to the
//! property it was written under and to honour a property allow-list.

use crate::links::{Link, LinkKind, extract_link_spans_with_original};

/// One entry of the key stack: the indentation column a key was written at and
/// the key itself.
struct KeyFrame {
    indent: usize,
    key: String,
}

/// Byte offset of the first `:` that terminates a YAML key on `line`, if any.
///
/// A key colon is one at bracket depth zero, outside quotes, followed by a
/// space or the end of the line — so `url: https://x` sees the first colon and
/// `- "[[a:b]]"` sees none.
fn key_colon(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b'[' | b'{' if !in_single && !in_double => depth += 1,
            b']' | b'}' if !in_single && !in_double => depth -= 1,
            b':' if !in_single
                && !in_double
                && depth <= 0
                && (i + 1 == bytes.len() || bytes[i + 1] == b' ') =>
            {
                return Some(i);
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Drop a trailing `# comment` from a YAML line.
///
/// Only a `#` at bracket depth zero, outside quotes, and at the start of the
/// line or preceded by whitespace starts a comment — which is what keeps a
/// wikilink anchor (`[[note#heading]]`) and a tag value (`status: #wip`) out of
/// the way.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b'[' | b'{' if !in_single && !in_double => depth += 1,
            b']' | b'}' if !in_single && !in_double => depth -= 1,
            b'#' if !in_single && !in_double && depth <= 0 => {
                let preceded_by_space = i == 0 || bytes[i - 1].is_ascii_whitespace();
                if preceded_by_space {
                    return &line[..i];
                }
            }
            _ => {}
        }
    }
    line
}

/// Update the key stack for one frontmatter line and return the dotted key path
/// that any value on that line belongs to.
///
/// Returns `None` for a line that carries no value context at all (a blank line
/// before the first key).
fn key_for_line(stack: &mut Vec<KeyFrame>, line: &str) -> Option<String> {
    let indent = line.len() - line.trim_start().len();
    let mut trimmed = line.trim_start();
    let mut effective_indent = indent;

    // A `- ` list item does not introduce a key of its own; its content sits
    // two columns further in and may itself open a map (`- name: "[[X]]"`).
    while let Some(rest) = trimmed.strip_prefix("- ") {
        effective_indent += trimmed.len() - rest.len();
        trimmed = rest;
    }
    if trimmed == "-" {
        return stack.last().map(|_| dotted(stack));
    }

    if let Some(colon) = key_colon(trimmed) {
        let key = trimmed[..colon].trim().trim_matches(['"', '\'']).to_owned();
        if !key.is_empty() {
            while stack.last().is_some_and(|f| f.indent >= effective_indent) {
                stack.pop();
            }
            stack.push(KeyFrame {
                indent: effective_indent,
                key,
            });
            return Some(dotted(stack));
        }
    }

    // A continuation line — a block-scalar body, a wrapped flow sequence, a
    // bare list item — stays under whatever key is currently open.
    if stack.is_empty() {
        None
    } else {
        Some(dotted(stack))
    }
}

/// Join the key stack into a dotted path (`meta.source`).
fn dotted(stack: &[KeyFrame]) -> String {
    let mut out = String::new();
    for frame in stack {
        if !out.is_empty() {
            out.push('.');
        }
        out.push_str(&frame.key);
    }
    out
}

/// Extract every `[[wikilink]]` written in a file's frontmatter.
///
/// * `yaml` — the raw frontmatter **content** (everything between the `---`
///   delimiters), exactly as the scanner accumulated it.
/// * `first_line` — the 1-based file line the first byte of `yaml` sits on
///   (always `2` for a file that opens with `---`).
/// * `only` — restrict the scan to these top-level properties, or `None` to
///   scan every value (the iter-262 default). `Some(&[])` scans nothing.
///
/// Appends `(file_line, link)` pairs to `out` in document order, each link
/// carrying [`Link::property`] set to the dotted key path it was written under.
/// Markdown-syntax links inside frontmatter are **not** collected — Obsidian
/// does not treat them as edges either.
pub fn extract_frontmatter_links(
    yaml: &str,
    first_line: usize,
    only: Option<&[String]>,
    out: &mut Vec<(usize, Link)>,
) {
    if only.is_some_and(<[String]>::is_empty) || !yaml.contains("[[") {
        return;
    }
    let mut stack: Vec<KeyFrame> = Vec::new();
    for (offset, raw_line) in yaml.lines().enumerate() {
        let line = strip_comment(raw_line);
        let Some(key) = key_for_line(&mut stack, line) else {
            continue;
        };
        if !line.contains("[[") {
            continue;
        }
        if let Some(allowed) = only {
            let top = key.split('.').next().unwrap_or(&key);
            if !allowed.iter().any(|p| p == top) {
                continue;
            }
        }
        for span in extract_link_spans_with_original(line, line) {
            if span.kind != LinkKind::Wikilink {
                continue;
            }
            let mut found = span.link;
            found.property = Some(key.clone());
            out.push((first_line + offset, found));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(yaml: &str, only: Option<&[String]>) -> Vec<(usize, Link)> {
        let mut out = Vec::new();
        extract_frontmatter_links(yaml, 2, only, &mut out);
        out
    }

    fn targets(yaml: &str) -> Vec<(usize, String, String)> {
        links(yaml, None)
            .into_iter()
            .map(|(line, l)| (line, l.property.unwrap_or_default(), l.target))
            .collect()
    }

    #[test]
    fn quoted_scalar_value_is_a_link() {
        assert_eq!(
            targets("type: \"[[Books]]\"\n"),
            vec![(2, "type".to_owned(), "Books".to_owned())]
        );
    }

    #[test]
    fn unquoted_scalar_is_a_link() {
        // YAML parses a bare `[[Books]]` as a nested flow sequence, so a walk
        // over parsed values never sees the brackets. The raw scan does.
        assert_eq!(
            targets("related: [[Books]]\n"),
            vec![(2, "related".to_owned(), "Books".to_owned())]
        );
    }

    #[test]
    fn flow_list_items_are_links() {
        assert_eq!(
            targets("categories: [\"[[Books]]\", \"[[Reading]]\"]\n"),
            vec![
                (2, "categories".to_owned(), "Books".to_owned()),
                (2, "categories".to_owned(), "Reading".to_owned()),
            ]
        );
    }

    #[test]
    fn block_list_items_carry_their_own_lines() {
        let yaml = "title: A\nrelated:\n  - \"[[One]]\"\n  - [[Two]]\n";
        assert_eq!(
            targets(yaml),
            vec![
                (4, "related".to_owned(), "One".to_owned()),
                (5, "related".to_owned(), "Two".to_owned()),
            ]
        );
    }

    #[test]
    fn nested_map_uses_a_dotted_key_path() {
        let yaml = "meta:\n  source: \"[[Origin]]\"\n";
        assert_eq!(
            targets(yaml),
            vec![(3, "meta.source".to_owned(), "Origin".to_owned())]
        );
    }

    #[test]
    fn list_of_maps_extends_the_key_path() {
        let yaml = "authors:\n  - name: \"[[Kepano]]\"\n";
        assert_eq!(
            targets(yaml),
            vec![(3, "authors.name".to_owned(), "Kepano".to_owned())]
        );
    }

    #[test]
    fn sibling_key_after_a_nested_map_pops_the_stack() {
        let yaml = "meta:\n  source: \"[[A]]\"\nrelated: \"[[B]]\"\n";
        assert_eq!(
            targets(yaml),
            vec![
                (3, "meta.source".to_owned(), "A".to_owned()),
                (4, "related".to_owned(), "B".to_owned()),
            ]
        );
    }

    #[test]
    fn alias_and_anchor_are_parsed() {
        let got = links("related: \"[[Notes/Log#DEC-1|the log]]\"\n", None);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1.target, "Notes/Log");
        assert_eq!(got[0].1.fragment.as_deref(), Some("DEC-1"));
        assert_eq!(got[0].1.label.as_deref(), Some("the log"));
        assert_eq!(got[0].1.property.as_deref(), Some("related"));
        assert!(got[0].1.is_frontmatter());
    }

    #[test]
    fn link_inside_a_longer_string_counts() {
        assert_eq!(
            targets("note: \"see [[Books]] for more\"\n"),
            vec![(2, "note".to_owned(), "Books".to_owned())]
        );
    }

    #[test]
    fn unclosed_brackets_are_not_links() {
        assert!(targets("note: \"[[not closed\"\n").is_empty());
    }

    #[test]
    fn non_string_values_are_ignored() {
        assert!(targets("count: 3\ndone: true\nempty:\n").is_empty());
    }

    #[test]
    fn comments_are_not_scanned() {
        assert!(targets("# see [[Ghost]]\ntitle: A\n").is_empty());
        assert!(targets("title: A  # see [[Ghost]]\n").is_empty());
    }

    #[test]
    fn anchor_hash_is_not_a_comment() {
        assert_eq!(
            targets("related: \"[[Log#DEC-1]]\"\n")
                .into_iter()
                .map(|(_, _, t)| t)
                .collect::<Vec<_>>(),
            vec!["Log".to_owned()]
        );
    }

    #[test]
    fn only_restricts_to_named_top_level_properties() {
        let yaml = "related: \"[[One]]\"\ncategories: \"[[Two]]\"\n";
        let only = vec!["related".to_owned()];
        let got: Vec<String> = links(yaml, Some(&only))
            .into_iter()
            .map(|(_, l)| l.target)
            .collect();
        assert_eq!(got, vec!["One".to_owned()]);
    }

    #[test]
    fn empty_allow_list_scans_nothing() {
        assert!(links("related: \"[[One]]\"\n", Some(&[])).is_empty());
    }

    #[test]
    fn block_scalar_body_stays_under_its_key() {
        let yaml = "summary: |\n  mentions [[Books]]\n";
        assert_eq!(
            targets(yaml),
            vec![(3, "summary".to_owned(), "Books".to_owned())]
        );
    }

    #[test]
    fn wrapped_flow_sequence_stays_under_its_key() {
        let yaml = "categories: [\n  \"[[A]]\",\n  \"[[B]]\",\n]\n";
        assert_eq!(
            targets(yaml),
            vec![
                (3, "categories".to_owned(), "A".to_owned()),
                (4, "categories".to_owned(), "B".to_owned()),
            ]
        );
    }

    #[test]
    fn repeated_target_maps_to_distinct_lines() {
        let yaml = "related:\n  - \"[[Same]]\"\n  - \"[[Same]]\"\n";
        let lines: Vec<usize> = links(yaml, None).into_iter().map(|(l, _)| l).collect();
        assert_eq!(lines, vec![3, 4]);
    }

    #[test]
    fn markdown_links_in_frontmatter_are_not_collected() {
        assert!(targets("source: \"[label](other.md)\"\n").is_empty());
    }

    #[test]
    fn a_url_value_is_not_mistaken_for_a_key_break() {
        let yaml = "url: https://example.com\nrelated: \"[[A]]\"\n";
        assert_eq!(
            targets(yaml),
            vec![(3, "related".to_owned(), "A".to_owned())]
        );
    }
}
