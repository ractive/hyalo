use std::borrow::Cow;

use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value;

use super::parse::{FilterOp, PropertyFilter};

// ---------------------------------------------------------------------------
// Tag extraction and matching
// ---------------------------------------------------------------------------

/// Extract the `tags` list from a parsed frontmatter map.
/// Handles:
/// - Missing `tags` key → empty vec
/// - `tags` as a YAML sequence → collect string items
/// - `tags` as a scalar string → single-element vec
/// - `tags` as empty sequence → empty vec
#[must_use]
pub fn extract_tags(props: &IndexMap<String, Value>) -> Vec<String> {
    match props.get("tags") {
        Some(Value::Array(seq)) => seq
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .collect(),
        Some(Value::String(s)) => {
            if s.is_empty() {
                vec![]
            } else {
                vec![s.clone()]
            }
        }
        _ => vec![],
    }
}

/// Returns true if `tag` matches the query under Obsidian's nested tag rules.
/// A tag matches if it equals the query or starts with `query/` (case-insensitive,
/// using ASCII-only case folding via `eq_ignore_ascii_case`).
///
/// Matching is performed at the byte level. ASCII letters case-fold; all other
/// codepoints (including Unicode letters and emoji) must match exactly, which
/// is the intended behaviour — tag identity is codepoint-equal and no
/// NFC/NFD normalisation is applied.
#[must_use]
pub fn tag_matches(tag: &str, query: &str) -> bool {
    tag.eq_ignore_ascii_case(query)
        || (tag.len() > query.len()
            && tag.as_bytes()[query.len()] == b'/'
            && tag[..query.len()].eq_ignore_ascii_case(query))
}

// ---------------------------------------------------------------------------

impl PropertyFilter {
    /// Return true if the given property map satisfies this filter.
    ///
    /// Resolution goes through [`resolve_prop`], so nested dot-paths (maps and
    /// sequences of maps) are handled here; the comparison itself is shared
    /// with [`PropertyFilter::matches_value`].
    pub fn matches(&self, props: &IndexMap<String, Value>) -> bool {
        match self {
            PropertyFilter::Absent { key } => resolve_prop(props, key).is_none(),
            PropertyFilter::RegexMatch { key, .. } => {
                resolve_prop(props, key).is_some_and(|resolved| self.matches_value(&resolved))
            }
            PropertyFilter::Scalar { name, .. } => {
                resolve_prop(props, name).is_some_and(|resolved| self.matches_value(&resolved))
            }
        }
    }

    /// Evaluate the filter against an already-resolved value, bypassing the
    /// property map lookup.
    ///
    /// This is used when the caller has already derived the value for a key
    /// (e.g. a synthetic title from an H1 heading) and wants to avoid cloning
    /// the entire map just to inject the value for filter evaluation.
    ///
    /// Semantics by variant:
    /// - `Absent`     — the value is present (caller supplied it), so returns `false`.
    /// - `RegexMatch` — evaluates `pattern` against `value`.
    /// - `Scalar`     — evaluates the comparison against `value`; `Exists` returns `true`
    ///   because the caller only calls this when a value exists.
    #[must_use]
    pub fn matches_value(&self, value: &Value) -> bool {
        match self {
            // The key is present (caller derived a value), so absence filter fails.
            PropertyFilter::Absent { .. } => false,
            PropertyFilter::RegexMatch { pattern, .. } => yaml_value_regex_match(value, pattern),
            PropertyFilter::Scalar {
                op,
                value: filter_value,
                ..
            } => {
                if *op == FilterOp::Exists {
                    // Key is present — existence check passes.
                    return true;
                }
                let filter_val = filter_value.as_deref().unwrap_or("");
                match op {
                    FilterOp::Eq => yaml_value_eq(value, filter_val),
                    FilterOp::NotEq => !yaml_value_eq(value, filter_val),
                    FilterOp::Gt => {
                        yaml_cmp(value, filter_val) == Some(std::cmp::Ordering::Greater)
                    }
                    FilterOp::Gte => matches!(
                        yaml_cmp(value, filter_val),
                        Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                    ),
                    FilterOp::Lt => yaml_cmp(value, filter_val) == Some(std::cmp::Ordering::Less),
                    FilterOp::Lte => matches!(
                        yaml_cmp(value, filter_val),
                        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                    ),
                    // SAFETY: Exists is handled by the early return above
                    FilterOp::Exists => unreachable!("Exists handled by early return"),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// Resolve a property key against a frontmatter map, supporting nested
/// dot-path traversal (UX-3, iter-244; arrays of maps added in iter-245).
///
/// `a.b=v` first tries the literal key `"a.b"` (a flat map may genuinely
/// contain dotted keys), then falls back to walking the nested value:
///
/// - **Maps** — `a.b` matches `{a: {b: …}}`; the segment is a key lookup.
/// - **Sequences** — a numeric segment indexes (`contacts.0.email` is the
///   first element's `email`); any other segment auto-descends into *every*
///   element and collects the hits, so `contacts.email` against
///   `contacts: [{email: a}, {email: b}]` resolves to `[a, b]`. Because the
///   collected values are returned as a sequence, the existing sequence
///   semantics apply unchanged: `=`/`~=` match when **any** element matches,
///   `!=` when **none** does, and a bare key exists when at least one element
///   yielded a value.
///
/// A missing segment yields `None`, which the callers turn into the same
/// verdict as a missing flat key (fails `Scalar`/`RegexMatch`, passes
/// `Absent`).
///
/// The return type is a [`Cow`] so the common flat/map path stays borrowed;
/// only auto-descent through a sequence allocates.
fn resolve_prop<'a>(props: &'a IndexMap<String, Value>, key: &str) -> Option<Cow<'a, Value>> {
    if let Some(v) = props.get(key) {
        return Some(Cow::Borrowed(v));
    }
    let (first, rest) = key.split_once('.')?;
    resolve_path(props.get(first)?, Some(rest))
}

/// Walk `path` (a dot-separated remainder, `None` once exhausted) from `value`.
///
/// See [`resolve_prop`] for the traversal rules. Recursion depth is bounded by
/// the number of path segments times the nesting depth of the frontmatter
/// value, both of which are user data of bounded size in practice.
fn resolve_path<'a>(value: &'a Value, path: Option<&str>) -> Option<Cow<'a, Value>> {
    let Some(path) = path else {
        return Some(Cow::Borrowed(value));
    };
    let (segment, rest) = match path.split_once('.') {
        Some((segment, rest)) => (segment, Some(rest)),
        None => (path, None),
    };
    match value {
        Value::Object(map) => resolve_path(map.get(segment)?, rest),
        Value::Array(items) => {
            // Indexed segment form: `contacts.0.email`.
            if let Ok(index) = segment.parse::<usize>()
                && let Some(item) = items.get(index)
            {
                return resolve_path(item, rest);
            }
            // Auto-descent: apply the *same* segment to every element and
            // collect the hits into a sequence ("any element matches").
            let mut collected: Vec<Value> = Vec::new();
            for item in items {
                if let Some(found) = resolve_path(item, Some(path)) {
                    match found.into_owned() {
                        // Flatten, so `a.b.c` over nested lists stays a flat
                        // list of leaves rather than a list of lists.
                        Value::Array(inner) => collected.extend(inner),
                        other => collected.push(other),
                    }
                }
            }
            match collected.len() {
                0 => None,
                // A single hit is returned bare so ordering ops (`>`, `<`)
                // still work; they cannot compare a sequence.
                1 => collected.pop().map(Cow::Owned),
                _ => Some(Cow::Owned(Value::Array(collected))),
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------

/// Returns `true` if the frontmatter properties satisfy all property and tag filters.
///
/// All property filters are evaluated with AND semantics (every filter must pass).
/// All tag filters are evaluated with AND semantics (every query tag must be present).
/// Empty filter slices always pass.
///
/// Extracts tags internally. If the caller already has tags (e.g. for output),
/// use [`matches_filters_with_tags`] to avoid double extraction.
pub fn matches_frontmatter_filters(
    props: &IndexMap<String, Value>,
    property_filters: &[PropertyFilter],
    tag_filters: &[String],
) -> bool {
    if !property_filters.iter().all(|f| f.matches(props)) {
        return false;
    }
    if !tag_filters.is_empty() {
        let tags = extract_tags(props);
        return matches_tag_filters(&tags, tag_filters);
    }
    true
}

/// Like [`matches_frontmatter_filters`] but accepts pre-extracted tags.
///
/// Use this when the caller needs the tags for other purposes (e.g. output)
/// to avoid extracting them twice.
pub fn matches_filters_with_tags(
    props: &IndexMap<String, Value>,
    property_filters: &[PropertyFilter],
    tags: &[String],
    tag_filters: &[String],
) -> bool {
    if !property_filters.iter().all(|f| f.matches(props)) {
        return false;
    }
    if !tag_filters.is_empty() {
        return matches_tag_filters(tags, tag_filters);
    }
    true
}

/// Check that all tag filter queries match at least one tag.
fn matches_tag_filters(tags: &[String], tag_filters: &[String]) -> bool {
    tag_filters
        .iter()
        .all(|q| tags.iter().any(|t| tag_matches(t, q)))
}

// ---------------------------------------------------------------------------

/// Returns `true` if any string representation of `yaml` matches `pattern`.
///
/// For sequences, at least one element must match.
fn yaml_value_regex_match(yaml: &Value, pattern: &Regex) -> bool {
    match yaml {
        Value::String(s) => pattern.is_match(s),
        Value::Number(n) => pattern.is_match(&n.to_string()),
        Value::Bool(b) => pattern.is_match(if *b { "true" } else { "false" }),
        Value::Array(seq) => seq.iter().any(|item| yaml_value_regex_match(item, pattern)),
        // For mappings, match against keys and recurse into values.
        // This allows `versions~=ghes` to match `{fpt: "*", ghes: "*"}`.
        Value::Object(map) => map
            .iter()
            .any(|(k, v)| pattern.is_match(k) || yaml_value_regex_match(v, pattern)),
        Value::Null => false,
    }
}

// ---------------------------------------------------------------------------

/// Case-insensitive equality check between a YAML value and a string filter value.
///
/// `filter` is pre-lowercased for equality/inequality ops. Uses an ASCII
/// fast-path (`eq_ignore_ascii_case`) and falls back to Unicode `to_lowercase()`
/// only when the value contains non-ASCII bytes.
fn yaml_value_eq(yaml: &Value, filter: &str) -> bool {
    match yaml {
        Value::String(s) => str_eq_ignore_case(s, filter),
        Value::Number(n) => {
            if let Ok(fv) = filter.parse::<f64>() {
                n.as_f64().is_some_and(|nv| (nv - fv).abs() < f64::EPSILON)
            } else {
                false
            }
        }
        Value::Bool(b) => parse_bool_filter(filter).is_some_and(|fv| fv == *b),
        Value::Array(seq) => seq.iter().any(|item| yaml_value_eq(item, filter)),
        _ => yaml.as_str().is_some_and(|s| str_eq_ignore_case(s, filter)),
    }
}

/// Case-insensitive string comparison. `filter` must be pre-lowercased.
///
/// ASCII fast-path avoids allocation; falls back to Unicode `to_lowercase()`
/// only when the value contains non-ASCII bytes.
fn str_eq_ignore_case(value: &str, filter: &str) -> bool {
    if value.is_ascii() {
        value.eq_ignore_ascii_case(filter)
    } else {
        value.to_lowercase() == filter
    }
}

/// Parse a bool from filter strings: true/false/yes/no/1/0.
/// Uses ASCII-only case folding (sufficient for these fixed keywords).
fn parse_bool_filter(s: &str) -> Option<bool> {
    if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("yes") || s == "1" {
        Some(true)
    } else if s.eq_ignore_ascii_case("false") || s.eq_ignore_ascii_case("no") || s == "0" {
        Some(false)
    } else {
        None
    }
}

/// Ordering comparison between a YAML value and a string filter value.
/// Tries numeric comparison first, then falls back to case-sensitive string
/// comparison. The filter value preserves its original casing for ordering ops.
fn yaml_cmp(yaml: &Value, filter: &str) -> Option<std::cmp::Ordering> {
    // Numeric comparison.
    if let Some(nv) = yaml.as_f64()
        && let Ok(fv) = filter.parse::<f64>()
    {
        return nv.partial_cmp(&fv);
    }
    // String fallback.
    let yaml_str = match yaml {
        Value::String(s) => s.as_str(),
        _ => return None,
    };
    Some(yaml_str.cmp(filter))
}
