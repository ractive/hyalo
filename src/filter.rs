use anyhow::{Result, bail};
use serde_yaml_ng::Value;
use std::collections::BTreeMap;

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
pub fn extract_tags(props: &BTreeMap<String, Value>) -> Vec<String> {
    match props.get("tags") {
        Some(Value::Sequence(seq)) => seq
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

/// Comparison operator for property filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterOp {
    /// Property exists (no value specified)
    Exists,
    /// Exact equality
    Eq,
    /// Not equal
    NotEq,
    /// Greater than
    Gt,
    /// Greater than or equal
    Gte,
    /// Less than
    Lt,
    /// Less than or equal
    Lte,
}

/// A parsed `--property K=V` filter.
#[derive(Debug, Clone)]
pub struct PropertyFilter {
    pub name: String,
    pub op: FilterOp,
    pub value: Option<String>,
}

/// Parse a property filter expression.
///
/// Supported formats:
/// - `name`         → Exists
/// - `name=value`   → Eq
/// - `name!=value`  → NotEq
/// - `name>=value`  → Gte
/// - `name<=value`  → Lte
/// - `name>value`   → Gt
/// - `name<value`   → Lt
pub fn parse_property_filter(input: &str) -> Result<PropertyFilter> {
    // Try splitting on the first `=`.
    if let Some(eq_pos) = input.find('=') {
        let raw_name = &input[..eq_pos];
        let value = input[eq_pos + 1..].to_owned();

        let (name, op) = if let Some(stripped) = raw_name.strip_suffix('!') {
            (stripped, FilterOp::NotEq)
        } else if let Some(stripped) = raw_name.strip_suffix('>') {
            (stripped, FilterOp::Gte)
        } else if let Some(stripped) = raw_name.strip_suffix('<') {
            (stripped, FilterOp::Lte)
        } else {
            (raw_name, FilterOp::Eq)
        };

        if name.is_empty() {
            bail!("property filter name must not be empty");
        }

        // Pre-lowercase the value for equality/inequality ops to avoid
        // per-comparison allocations. Ordering ops keep original casing so
        // that string comparisons are not asymmetrically folded.
        let stored_value = match op {
            FilterOp::Eq | FilterOp::NotEq => value.to_lowercase(),
            _ => value,
        };

        return Ok(PropertyFilter {
            name: name.to_owned(),
            op,
            value: Some(stored_value),
        });
    }

    // No `=` found — check for bare `>` or `<`.
    // Ordering ops preserve original casing (see note above).
    if let Some(gt_pos) = input.find('>') {
        let name = &input[..gt_pos];
        let value = &input[gt_pos + 1..];
        if name.is_empty() {
            bail!("property filter name must not be empty");
        }
        return Ok(PropertyFilter {
            name: name.to_owned(),
            op: FilterOp::Gt,
            value: Some(value.to_owned()),
        });
    }

    if let Some(lt_pos) = input.find('<') {
        let name = &input[..lt_pos];
        let value = &input[lt_pos + 1..];
        if name.is_empty() {
            bail!("property filter name must not be empty");
        }
        return Ok(PropertyFilter {
            name: name.to_owned(),
            op: FilterOp::Lt,
            value: Some(value.to_owned()),
        });
    }

    // Existence check.
    if input.is_empty() {
        bail!("property filter must not be empty");
    }

    Ok(PropertyFilter {
        name: input.to_owned(),
        op: FilterOp::Exists,
        value: None,
    })
}

impl PropertyFilter {
    /// Return true if the given property map satisfies this filter.
    pub fn matches(&self, props: &BTreeMap<String, Value>) -> bool {
        match self.op {
            FilterOp::Exists => props.contains_key(&self.name),
            _ => {
                let Some(yaml_val) = props.get(&self.name) else {
                    return false;
                };
                let filter_val = self.value.as_deref().unwrap_or("");

                match self.op {
                    FilterOp::Exists => unreachable!(),
                    FilterOp::Eq => yaml_value_eq(yaml_val, filter_val),
                    FilterOp::NotEq => !yaml_value_eq(yaml_val, filter_val),
                    FilterOp::Gt => {
                        yaml_cmp(yaml_val, filter_val) == Some(std::cmp::Ordering::Greater)
                    }
                    FilterOp::Gte => matches!(
                        yaml_cmp(yaml_val, filter_val),
                        Some(std::cmp::Ordering::Greater) | Some(std::cmp::Ordering::Equal)
                    ),
                    FilterOp::Lt => {
                        yaml_cmp(yaml_val, filter_val) == Some(std::cmp::Ordering::Less)
                    }
                    FilterOp::Lte => matches!(
                        yaml_cmp(yaml_val, filter_val),
                        Some(std::cmp::Ordering::Less) | Some(std::cmp::Ordering::Equal)
                    ),
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
    props: &BTreeMap<String, Value>,
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
    props: &BTreeMap<String, Value>,
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
                n.as_f64()
                    .map(|nv| (nv - fv).abs() < f64::EPSILON)
                    .unwrap_or(false)
            } else {
                false
            }
        }
        Value::Bool(b) => parse_bool_filter(filter)
            .map(|fv| fv == *b)
            .unwrap_or(false),
        Value::Sequence(seq) => seq.iter().any(|item| yaml_value_eq(item, filter)),
        _ => yaml
            .as_str()
            .map(|s| str_eq_ignore_case(s, filter))
            .unwrap_or(false),
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

// ---------------------------------------------------------------------------

/// Task presence/status filter for `find --task`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindTaskFilter {
    /// Files with any incomplete tasks (status = space)
    Todo,
    /// Files with any completed tasks (status = x or X)
    Done,
    /// Files with any tasks at all
    Any,
    /// Files with tasks matching this exact status character
    Status(char),
}

/// Parse a task filter from a string.
pub fn parse_task_filter(input: &str) -> Result<FindTaskFilter> {
    match input {
        "todo" => Ok(FindTaskFilter::Todo),
        "done" => Ok(FindTaskFilter::Done),
        "any" => Ok(FindTaskFilter::Any),
        s => {
            let mut chars = s.chars();
            let first = chars.next();
            let second = chars.next();
            match (first, second) {
                (Some(ch), None) => Ok(FindTaskFilter::Status(ch)),
                _ => bail!(
                    "invalid task filter {:?}: expected 'todo', 'done', 'any', or a single character",
                    input
                ),
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// Controls which fields are included in `find` output.
#[derive(Debug, Clone)]
pub struct Fields {
    pub properties: bool,
    pub tags: bool,
    pub sections: bool,
    pub tasks: bool,
    pub links: bool,
}

impl Default for Fields {
    fn default() -> Self {
        Self {
            properties: true,
            tags: true,
            sections: true,
            tasks: true,
            links: true,
        }
    }
}

impl Fields {
    /// Parse a fields selection from a list of `--fields` argument values.
    ///
    /// Each element may be a comma-separated list of field names. An empty
    /// slice returns the default (all fields enabled).
    pub fn parse(input: &[String]) -> Result<Fields> {
        if input.is_empty() {
            return Ok(Fields::default());
        }

        let mut fields = Fields {
            properties: false,
            tags: false,
            sections: false,
            tasks: false,
            links: false,
        };

        for item in input {
            for part in item.split(',') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                match part {
                    "properties" => fields.properties = true,
                    "tags" => fields.tags = true,
                    "sections" => fields.sections = true,
                    "tasks" => fields.tasks = true,
                    "links" => fields.links = true,
                    unknown => bail!(
                        "unknown field {:?}: valid fields are properties, tags, sections, tasks, links",
                        unknown
                    ),
                }
            }
        }

        Ok(fields)
    }
}

// ---------------------------------------------------------------------------

/// Controls result ordering for `find` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortField {
    File,
    Modified,
}

/// Parse a sort field from a string.
pub fn parse_sort(input: &str) -> Result<SortField> {
    match input {
        "file" => Ok(SortField::File),
        "modified" => Ok(SortField::Modified),
        other => bail!(
            "unknown sort field {:?}: valid values are 'file' and 'modified'",
            other
        ),
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::Value;
    use std::collections::BTreeMap;

    // -----------------------------------------------------------------------
    // Property filter parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_eq() {
        let f = parse_property_filter("status=planned").unwrap();
        assert_eq!(f.name, "status");
        assert_eq!(f.op, FilterOp::Eq);
        assert_eq!(f.value.as_deref(), Some("planned"));
    }

    #[test]
    fn parse_not_eq() {
        let f = parse_property_filter("status!=superseded").unwrap();
        assert_eq!(f.name, "status");
        assert_eq!(f.op, FilterOp::NotEq);
        assert_eq!(f.value.as_deref(), Some("superseded"));
    }

    #[test]
    fn parse_gte() {
        let f = parse_property_filter("priority>=3").unwrap();
        assert_eq!(f.name, "priority");
        assert_eq!(f.op, FilterOp::Gte);
        assert_eq!(f.value.as_deref(), Some("3"));
    }

    #[test]
    fn parse_lte() {
        let f = parse_property_filter("priority<=5").unwrap();
        assert_eq!(f.name, "priority");
        assert_eq!(f.op, FilterOp::Lte);
        assert_eq!(f.value.as_deref(), Some("5"));
    }

    #[test]
    fn parse_gt() {
        let f = parse_property_filter("priority>3").unwrap();
        assert_eq!(f.name, "priority");
        assert_eq!(f.op, FilterOp::Gt);
        assert_eq!(f.value.as_deref(), Some("3"));
    }

    #[test]
    fn parse_lt() {
        let f = parse_property_filter("priority<5").unwrap();
        assert_eq!(f.name, "priority");
        assert_eq!(f.op, FilterOp::Lt);
        assert_eq!(f.value.as_deref(), Some("5"));
    }

    #[test]
    fn parse_exists() {
        let f = parse_property_filter("status").unwrap();
        assert_eq!(f.name, "status");
        assert_eq!(f.op, FilterOp::Exists);
        assert!(f.value.is_none());
    }

    #[test]
    fn parse_value_contains_equals() {
        // Value itself contains `=`; only the first `=` is the delimiter.
        let f = parse_property_filter("key=a=b").unwrap();
        assert_eq!(f.name, "key");
        assert_eq!(f.op, FilterOp::Eq);
        assert_eq!(f.value.as_deref(), Some("a=b"));
    }

    #[test]
    fn parse_empty_name_eq_errors() {
        assert!(parse_property_filter("=value").is_err());
    }

    #[test]
    fn parse_empty_name_gt_errors() {
        assert!(parse_property_filter(">5").is_err());
    }

    #[test]
    fn parse_empty_input_errors() {
        assert!(parse_property_filter("").is_err());
    }

    // -----------------------------------------------------------------------
    // Property filter matching
    // -----------------------------------------------------------------------

    fn props(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn match_string_eq_case_insensitive() {
        let p = props(&[("status", Value::String("Planned".into()))]);
        let f = parse_property_filter("status=planned").unwrap();
        assert!(f.matches(&p));
        let f2 = parse_property_filter("status=PLANNED").unwrap();
        assert!(f2.matches(&p));
    }

    #[test]
    fn match_string_neq() {
        let p = props(&[("status", Value::String("completed".into()))]);
        let f = parse_property_filter("status!=superseded").unwrap();
        assert!(f.matches(&p));
        let f2 = parse_property_filter("status!=completed").unwrap();
        assert!(!f2.matches(&p));
    }

    #[test]
    fn match_numeric_eq() {
        let p = props(&[("priority", Value::Number(3.into()))]);
        let f = parse_property_filter("priority=3").unwrap();
        assert!(f.matches(&p));
    }

    #[test]
    fn match_numeric_gt_lt() {
        let p = props(&[("priority", Value::Number(5.into()))]);
        assert!(parse_property_filter("priority>3").unwrap().matches(&p));
        assert!(!parse_property_filter("priority>5").unwrap().matches(&p));
        assert!(parse_property_filter("priority>=5").unwrap().matches(&p));
        assert!(parse_property_filter("priority<10").unwrap().matches(&p));
        assert!(!parse_property_filter("priority<5").unwrap().matches(&p));
        assert!(parse_property_filter("priority<=5").unwrap().matches(&p));
    }

    #[test]
    fn match_boolean() {
        let p = props(&[("active", Value::Bool(true))]);
        assert!(parse_property_filter("active=true").unwrap().matches(&p));
        assert!(parse_property_filter("active=yes").unwrap().matches(&p));
        assert!(parse_property_filter("active=1").unwrap().matches(&p));
        assert!(!parse_property_filter("active=false").unwrap().matches(&p));
        assert!(!parse_property_filter("active=0").unwrap().matches(&p));
    }

    #[test]
    fn match_list_eq_any_element() {
        let p = props(&[(
            "tags",
            Value::Sequence(vec![
                Value::String("rust".into()),
                Value::String("cli".into()),
            ]),
        )]);
        assert!(parse_property_filter("tags=rust").unwrap().matches(&p));
        assert!(parse_property_filter("tags=CLI").unwrap().matches(&p));
        assert!(!parse_property_filter("tags=python").unwrap().matches(&p));
    }

    #[test]
    fn match_list_neq_none_match() {
        let p = props(&[(
            "tags",
            Value::Sequence(vec![
                Value::String("rust".into()),
                Value::String("cli".into()),
            ]),
        )]);
        // NotEq: true only when no element matches
        assert!(!parse_property_filter("tags!=rust").unwrap().matches(&p));
        assert!(parse_property_filter("tags!=python").unwrap().matches(&p));
    }

    #[test]
    fn match_exists_true_false() {
        let p = props(&[("status", Value::String("planned".into()))]);
        assert!(parse_property_filter("status").unwrap().matches(&p));
        assert!(!parse_property_filter("missing").unwrap().matches(&p));
    }

    #[test]
    fn match_missing_key_returns_false() {
        let p = props(&[]);
        let f = parse_property_filter("priority>3").unwrap();
        assert!(!f.matches(&p));
    }

    // -----------------------------------------------------------------------
    // Task filter parsing
    // -----------------------------------------------------------------------

    #[test]
    fn task_filter_todo() {
        assert_eq!(parse_task_filter("todo").unwrap(), FindTaskFilter::Todo);
    }

    #[test]
    fn task_filter_done() {
        assert_eq!(parse_task_filter("done").unwrap(), FindTaskFilter::Done);
    }

    #[test]
    fn task_filter_any() {
        assert_eq!(parse_task_filter("any").unwrap(), FindTaskFilter::Any);
    }

    #[test]
    fn task_filter_single_char() {
        assert_eq!(parse_task_filter("~").unwrap(), FindTaskFilter::Status('~'));
        assert_eq!(parse_task_filter("x").unwrap(), FindTaskFilter::Status('x'));
    }

    #[test]
    fn task_filter_multi_char_errors() {
        assert!(parse_task_filter("ab").is_err());
        assert!(parse_task_filter("xyz").is_err());
    }

    // -----------------------------------------------------------------------
    // Fields parsing
    // -----------------------------------------------------------------------

    #[test]
    fn fields_empty_returns_all() {
        let f = Fields::parse(&[]).unwrap();
        assert!(f.properties && f.tags && f.sections && f.tasks && f.links);
    }

    #[test]
    fn fields_specific_subset() {
        let input = vec!["tags".to_owned(), "tasks".to_owned()];
        let f = Fields::parse(&input).unwrap();
        assert!(!f.properties);
        assert!(f.tags);
        assert!(!f.sections);
        assert!(f.tasks);
        assert!(!f.links);
    }

    #[test]
    fn fields_comma_separated() {
        let input = vec!["tags,tasks,links".to_owned()];
        let f = Fields::parse(&input).unwrap();
        assert!(!f.properties);
        assert!(f.tags);
        assert!(!f.sections);
        assert!(f.tasks);
        assert!(f.links);
    }

    #[test]
    fn fields_unknown_errors() {
        let input = vec!["unknown_field".to_owned()];
        assert!(Fields::parse(&input).is_err());
    }

    // -----------------------------------------------------------------------
    // Sort parsing
    // -----------------------------------------------------------------------

    #[test]
    fn sort_file() {
        assert_eq!(parse_sort("file").unwrap(), SortField::File);
    }

    #[test]
    fn sort_modified() {
        assert_eq!(parse_sort("modified").unwrap(), SortField::Modified);
    }

    #[test]
    fn sort_unknown_errors() {
        assert!(parse_sort("name").is_err());
        assert!(parse_sort("").is_err());
    }

    // -----------------------------------------------------------------------
    // matches_frontmatter_filters
    // -----------------------------------------------------------------------

    #[test]
    fn matches_frontmatter_filters_empty_filters() {
        // No filters → always true, regardless of props content.
        let p = props(&[("status", Value::String("anything".into()))]);
        assert!(matches_frontmatter_filters(&p, &[], &[]));

        let empty = props(&[]);
        assert!(matches_frontmatter_filters(&empty, &[], &[]));
    }

    #[test]
    fn matches_frontmatter_filters_scalar_property() {
        let p = props(&[("status", Value::String("planned".into()))]);
        let filters = [parse_property_filter("status=planned").unwrap()];

        assert!(matches_frontmatter_filters(&p, &filters, &[]));

        let no_match = [parse_property_filter("status=completed").unwrap()];
        assert!(!matches_frontmatter_filters(&p, &no_match, &[]));
    }

    #[test]
    fn matches_frontmatter_filters_list_property() {
        // Value is a YAML sequence — filter matches any element.
        let p = props(&[(
            "tags",
            Value::Sequence(vec![
                Value::String("rust".into()),
                Value::String("cli".into()),
            ]),
        )]);
        let filters = [parse_property_filter("tags=rust").unwrap()];
        assert!(matches_frontmatter_filters(&p, &filters, &[]));

        let filters_cli = [parse_property_filter("tags=cli").unwrap()];
        assert!(matches_frontmatter_filters(&p, &filters_cli, &[]));

        let no_match = [parse_property_filter("tags=python").unwrap()];
        assert!(!matches_frontmatter_filters(&p, &no_match, &[]));
    }

    #[test]
    fn matches_frontmatter_filters_tag_match() {
        // Nested tag: query "inbox" matches tag "inbox/processing".
        let p = props(&[(
            "tags",
            Value::Sequence(vec![Value::String("inbox/processing".into())]),
        )]);
        let tag_filters = vec!["inbox".to_owned()];
        assert!(matches_frontmatter_filters(&p, &[], &tag_filters));

        // Exact match also works.
        let exact = vec!["inbox/processing".to_owned()];
        assert!(matches_frontmatter_filters(&p, &[], &exact));

        // Non-matching query.
        let miss = vec!["project".to_owned()];
        assert!(!matches_frontmatter_filters(&p, &[], &miss));
    }

    #[test]
    fn matches_frontmatter_filters_combined_and() {
        // Both property and tag filters must pass.
        let p = props(&[
            ("status", Value::String("done".into())),
            ("tags", Value::Sequence(vec![Value::String("rust".into())])),
        ]);
        let prop_filters = [parse_property_filter("status=done").unwrap()];
        let tag_filters = vec!["rust".to_owned()];

        assert!(matches_frontmatter_filters(&p, &prop_filters, &tag_filters));

        // Prop matches but tag doesn't.
        let wrong_tag = vec!["python".to_owned()];
        assert!(!matches_frontmatter_filters(&p, &prop_filters, &wrong_tag));

        // Tag matches but prop doesn't.
        let wrong_prop = [parse_property_filter("status=pending").unwrap()];
        assert!(!matches_frontmatter_filters(&p, &wrong_prop, &tag_filters));
    }

    #[test]
    fn matches_frontmatter_filters_no_match() {
        let p = props(&[("status", Value::String("active".into()))]);
        let filters = [parse_property_filter("status=archived").unwrap()];
        assert!(!matches_frontmatter_filters(&p, &filters, &[]));
    }
}
