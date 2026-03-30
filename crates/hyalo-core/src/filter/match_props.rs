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
/// Matching is performed at the byte level and is intended for tags that use
/// ASCII-compatible characters (letters, digits, `_`, `-`, `/`).
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
    pub fn matches(&self, props: &IndexMap<String, Value>) -> bool {
        match self {
            PropertyFilter::Absent { key } => !props.contains_key(key),
            PropertyFilter::RegexMatch { key, pattern } => {
                let Some(yaml_val) = props.get(key) else {
                    return false;
                };
                yaml_value_regex_match(yaml_val, pattern)
            }
            PropertyFilter::Scalar { name, op, value } => {
                if *op == FilterOp::Exists {
                    return props.contains_key(name);
                }

                let Some(yaml_val) = props.get(name) else {
                    return false;
                };
                let filter_val = value.as_deref().unwrap_or("");

                match op {
                    FilterOp::Eq => yaml_value_eq(yaml_val, filter_val),
                    FilterOp::NotEq => !yaml_value_eq(yaml_val, filter_val),
                    FilterOp::Gt => {
                        yaml_cmp(yaml_val, filter_val) == Some(std::cmp::Ordering::Greater)
                    }
                    FilterOp::Gte => matches!(
                        yaml_cmp(yaml_val, filter_val),
                        Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                    ),
                    FilterOp::Lt => {
                        yaml_cmp(yaml_val, filter_val) == Some(std::cmp::Ordering::Less)
                    }
                    FilterOp::Lte => matches!(
                        yaml_cmp(yaml_val, filter_val),
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
