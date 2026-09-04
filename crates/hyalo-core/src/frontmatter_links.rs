//! Wikilinks written inside YAML frontmatter values (iter-262, dogfood BUG-1).
//!
//! Obsidian treats every `[[wikilink]]` in a frontmatter value as a graph edge,
//! wherever it appears: `related:`, `categories: ["[[Books]]"]`,
//! `type: "[[Author]]"`, a nested map, a block list. hyalo used to scan a fixed
//! four-property allow-list ([`DEFAULT_FRONTMATTER_LINK_PROPERTIES`]), so on a
//! real Obsidian vault `backlinks Categories/Books.md` came back empty while
//! three files pointed at it through `categories:`.
//!
//! [`DEFAULT_FRONTMATTER_LINK_PROPERTIES`]: crate::link_graph::DEFAULT_FRONTMATTER_LINK_PROPERTIES
//!
//! # Why both the parsed map and the raw text
//!
//! The **parsed** frontmatter says what is actually a string value — so
//! `"[[not closed"` yields nothing, a `# comment` yields nothing, and a value
//! nested three maps deep is found without a YAML re-implementation here. The
//! **raw** frontmatter text says which *line* each occurrence sits on, which
//! the parsed map cannot tell (serde hands over values, not spans). Matching
//! the two up is a forward scan: link occurrences come out of the parsed map in
//! document order, so their raw `[[…]]` byte sequences appear in the raw block
//! in that same order.
//!
//! A value whose raw bytes differ from its parsed bytes (an escaped or folded
//! scalar) simply fails the scan and falls back to the first frontmatter
//! content line — the link is still an edge, only its reported line is
//! approximate.

use indexmap::IndexMap;
use serde_json::Value;

use crate::links::{Link, LinkKind, extract_link_spans_with_original};

/// A frontmatter value string paired with the dotted key path it lives under.
struct KeyedValue<'a> {
    key: String,
    value: &'a str,
}

/// Collect every string scalar in `props` — at any nesting depth, inside lists
/// and maps alike — paired with the dotted key path it was found under.
///
/// `only` restricts the walk to the named **top-level** properties (the legacy
/// `[links] frontmatter_properties` behaviour); `None` walks everything.
fn collect_string_values<'a>(
    props: &'a IndexMap<String, Value>,
    only: Option<&[String]>,
) -> Vec<KeyedValue<'a>> {
    let mut out = Vec::new();
    for (key, value) in props {
        if let Some(allowed) = only
            && !allowed.iter().any(|p| p == key)
        {
            continue;
        }
        walk_value(key, value, &mut out);
    }
    out
}

/// Recurse into one frontmatter value, appending every string scalar found.
///
/// A list keeps its parent's key (`tags[0]` and `tags[1]` are both `tags`);
/// a nested map extends the path with a dot, because there the sub-key is what
/// identifies the value.
fn walk_value<'a>(key: &str, value: &'a Value, out: &mut Vec<KeyedValue<'a>>) {
    match value {
        Value::String(s) => out.push(KeyedValue {
            key: key.to_owned(),
            value: s,
        }),
        Value::Array(items) => {
            for item in items {
                walk_value(key, item, out);
            }
        }
        Value::Object(map) => {
            for (sub, sub_value) in map {
                walk_value(&format!("{key}.{sub}"), sub_value, out);
            }
        }
        // Numbers, booleans and nulls can never carry `[[`.
        _ => {}
    }
}

/// Byte offsets of every line start in `text`, for offset → line lookups.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    starts.extend(memchr::memchr_iter(b'\n', text.as_bytes()).map(|pos| pos + 1));
    starts
}

/// 1-based line index within `text` for byte offset `at`, given precomputed
/// [`line_starts`].
fn line_of(starts: &[usize], at: usize) -> usize {
    match starts.binary_search(&at) {
        Ok(i) => i + 1,
        Err(i) => i,
    }
}

/// Extract every `[[wikilink]]` written in a file's frontmatter.
///
/// * `yaml` — the raw frontmatter **content** (everything between the `---`
///   delimiters), exactly as the scanner accumulated it.
/// * `first_line` — the 1-based file line the first byte of `yaml` sits on
///   (always `2` for a file that opens with `---`).
/// * `props` — the parsed frontmatter map for the same block.
/// * `only` — restrict the walk to these top-level keys, or `None` for every
///   value (the iter-262 default).
///
/// Appends `(file_line, link)` pairs to `out`, each link carrying
/// [`Link::property`] set to the key path it came from. External URIs
/// (`[[obsidian://…]]`) are inventoried like body links; markdown-syntax links
/// inside frontmatter are **not** collected — Obsidian does not treat them as
/// edges either.
pub fn extract_frontmatter_links(
    yaml: &str,
    first_line: usize,
    props: &IndexMap<String, Value>,
    only: Option<&[String]>,
    out: &mut Vec<(usize, Link)>,
) {
    if props.is_empty() || !yaml.contains("[[") {
        return;
    }
    let values = collect_string_values(props, only);
    if values.is_empty() {
        return;
    }

    let starts = line_starts(yaml);
    // Forward cursor into the raw block: occurrences are matched in document
    // order, so a repeated `[[Books]]` on two different lines maps to two
    // different lines rather than both to the first.
    let mut cursor = 0usize;

    for kv in values {
        if !kv.value.contains("[[") {
            continue;
        }
        for span in extract_link_spans_with_original(kv.value, kv.value) {
            if span.kind != LinkKind::Wikilink {
                continue;
            }
            let raw = &kv.value[span.full_start..span.full_end];
            let at = yaml[cursor..]
                .find(raw)
                .map(|p| cursor + p)
                .or_else(|| yaml.find(raw));
            let line = match at {
                Some(offset) => {
                    cursor = offset + raw.len();
                    first_line + line_of(&starts, offset) - 1
                }
                // The parsed value does not appear verbatim in the raw block
                // (an escaped or folded scalar). The edge is real; only the
                // line is approximate.
                None => first_line,
            };
            let mut link = span.link;
            link.property = Some(kv.key.clone());
            out.push((line, link));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> IndexMap<String, Value> {
        serde_saphyr::from_str_with_options(yaml, crate::frontmatter::hyalo_options())
            .expect("test frontmatter parses")
    }

    fn links(yaml: &str, only: Option<&[String]>) -> Vec<(usize, Link)> {
        let props = parse(yaml);
        let mut out = Vec::new();
        extract_frontmatter_links(yaml, 2, &props, only, &mut out);
        out
    }

    fn targets(yaml: &str) -> Vec<(usize, String, String)> {
        links(yaml, None)
            .into_iter()
            .map(|(line, l)| (line, l.property.unwrap_or_default(), l.target))
            .collect()
    }

    #[test]
    fn scalar_value_is_a_link() {
        assert_eq!(
            targets("type: \"[[Books]]\"\n"),
            vec![(2, "type".to_owned(), "Books".to_owned())]
        );
    }

    #[test]
    fn unquoted_scalar_is_a_link() {
        // Bare `[[x]]` parses as a YAML flow sequence of a flow sequence, so
        // the value is a list-of-list of strings — still walked to the leaves.
        let got = targets("related: [[Books]]\n");
        assert_eq!(got.len(), 1, "expected one link, got {got:?}");
        assert_eq!(got[0].2, "Books");
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
        let yaml = "title: A\nrelated:\n  - \"[[One]]\"\n  - \"[[Two]]\"\n";
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
    fn alias_and_anchor_are_parsed() {
        let got = links("related: \"[[Notes/Log#DEC-1|the log]]\"\n", None);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1.target, "Notes/Log");
        assert_eq!(got[0].1.fragment.as_deref(), Some("DEC-1"));
        assert_eq!(got[0].1.label.as_deref(), Some("the log"));
        assert_eq!(got[0].1.property.as_deref(), Some("related"));
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
    fn only_restricts_to_named_properties() {
        let yaml = "related: \"[[One]]\"\ncategories: \"[[Two]]\"\n";
        let only = vec!["related".to_owned()];
        let got: Vec<String> = links(yaml, Some(&only))
            .into_iter()
            .map(|(_, l)| l.target)
            .collect();
        assert_eq!(got, vec!["One".to_owned()]);
    }

    #[test]
    fn repeated_target_maps_to_distinct_lines() {
        let yaml = "related:\n  - \"[[Same]]\"\n  - \"[[Same]]\"\n";
        let lines: Vec<usize> = links(yaml, None).into_iter().map(|(l, _)| l).collect();
        assert_eq!(lines, vec![3, 4]);
    }

    #[test]
    fn deeply_nested_lists_are_walked() {
        let yaml = "matrix:\n  - - \"[[Deep]]\"\n";
        assert_eq!(
            targets(yaml),
            vec![(3, "matrix".to_owned(), "Deep".to_owned())]
        );
    }
}
