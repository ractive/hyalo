use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value;

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

/// A parsed `--property` filter.
///
/// Variants:
/// - `Scalar`      — comparison filter: `K=V`, `K!=V`, `K>V`, `K>=V`, `K<V`, `K<=V`, or bare `K` (existence)
/// - `Absent`      — matches files that do NOT have property K: `!K`
/// - `RegexMatch`  — matches if property value matches pattern: `K~=pattern`, `K=~pattern`, or delimited forms
#[derive(Debug, Clone)]
pub enum PropertyFilter {
    /// A scalar comparison filter (includes the Exists op).
    Scalar {
        name: String,
        op: FilterOp,
        /// Pre-lowercased for Eq/NotEq; original casing for ordering ops.
        value: Option<String>,
    },
    /// Matches files where property `key` is absent (not present in frontmatter).
    Absent { key: String },
    /// Matches files where property `key`'s value matches `pattern`.
    RegexMatch { key: String, pattern: Regex },
}

/// Parse a property filter expression.
///
/// Supported formats:
/// - `name`              → Exists (property is present)
/// - `!name`             → Absent (property is NOT present)
/// - `name=value`        → Eq
/// - `name!=value`       → NotEq
/// - `name>=value`       → Gte
/// - `name<=value`       → Lte
/// - `name>value`        → Gt
/// - `name<value`        → Lt
/// - `name~=pattern`     → RegexMatch (bare pattern, unanchored)
/// - `name~=/pattern/`   → RegexMatch (delimited, unanchored)
/// - `name~=/pattern/i`  → RegexMatch (delimited, case-insensitive flag)
/// - `name=~pattern`     → RegexMatch (Perl/Ruby-style alias for `~=`)
/// - `name=~/pattern/`   → RegexMatch (Perl/Ruby-style alias, delimited)
/// - `name=~/pattern/i`  → RegexMatch (Perl/Ruby-style alias, case-insensitive)
pub fn parse_property_filter(input: &str) -> Result<PropertyFilter> {
    // Normalize `\!K` → `!K` so that zsh-escaped absence filters work.
    // zsh escapes `!` to `\!` even in single quotes in some contexts.
    let normalized;
    let input = if let Some(rest) = input.strip_prefix("\\!") {
        normalized = format!("!{rest}");
        normalized.as_str()
    } else {
        input
    };

    // --- Absence filter: `!key` ---
    // Must start with `!` and contain no operator characters after the `!`.
    // Be careful not to confuse with `key!=value` which has `!` in the middle.
    if let Some(key) = input.strip_prefix('!') {
        // `!K` is valid only if there is no `=` in what follows (which would
        // mean someone typed `!key=value`, an ambiguous/unsupported form).
        if !key.contains('=') && !key.contains('>') && !key.contains('<') && !key.contains('~') {
            if key.is_empty() {
                bail!("property filter name must not be empty");
            }
            return Ok(PropertyFilter::Absent {
                key: key.to_owned(),
            });
        }
    }

    // --- Regex filter: `key~=pattern` or `key=~pattern` (and delimited forms) ---
    //
    // Both `~=` (hyalo-native) and `=~` (Perl/Ruby-style alias) are accepted.
    // `=~` is checked first so that `key=~/pattern/` is not mistaken for an
    // equality filter against the literal value `~/pattern/`.
    let regex_op_pos = input
        .find("=~")
        .filter(|&p| {
            // Reject if the '=' is actually the tail of !=, >=, or <=
            p == 0 || !matches!(input.as_bytes()[p - 1], b'!' | b'>' | b'<')
        })
        .map(|p| (p, "=~"))
        .or_else(|| input.find("~=").map(|p| (p, "~=")));

    if let Some((op_pos, op)) = regex_op_pos {
        let key = &input[..op_pos];
        let pattern_part = &input[op_pos + op.len()..];

        if key.is_empty() {
            bail!("property filter name must not be empty");
        }

        let re = parse_regex_pattern(pattern_part)
            .with_context(|| format!("invalid regex in property filter: {input:?}"))?;

        return Ok(PropertyFilter::RegexMatch {
            key: key.to_owned(),
            pattern: re,
        });
    }

    // --- Scalar filters (equality, ordering, existence) ---

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

        return Ok(PropertyFilter::Scalar {
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
        return Ok(PropertyFilter::Scalar {
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
        return Ok(PropertyFilter::Scalar {
            name: name.to_owned(),
            op: FilterOp::Lt,
            value: Some(value.to_owned()),
        });
    }

    // Existence check.
    if input.is_empty() {
        bail!("property filter must not be empty");
    }

    if input.contains('!') || input.contains('~') {
        bail!(
            "invalid property filter {input:?}: contains operator-like characters; \
             supported operators: =, !=, >=, <=, >, <, ~=, =~, ! (absence)"
        );
    }

    Ok(PropertyFilter::Scalar {
        name: input.to_owned(),
        op: FilterOp::Exists,
        value: None,
    })
}

/// Parse a regex pattern from the part after `~=`.
///
/// Accepts:
/// - `/pattern/flags` — delimited form; closing `/` is required; flags: `i` (case-insensitive)
/// - `pattern`        — bare form; treated as unanchored, case-sensitive
///
/// In both forms the pattern is compiled with a 1 MiB size limit to prevent
/// pathological regex compilation.
fn parse_regex_pattern(s: &str) -> Result<Regex> {
    const SIZE_LIMIT: usize = 1 << 20; // 1 MiB

    if let Some(rest) = s.strip_prefix('/') {
        // Delimited form: `/pattern/flags`
        // Find the closing `/` (last occurrence, to allow `/` inside the pattern).
        let close = rest.rfind('/').with_context(|| {
            format!("regex pattern starting with '/' must end with '/' (e.g. /pattern/ or /pattern/i), got: /{rest}")
        })?;
        let pattern = &rest[..close];
        let flags = &rest[close + 1..];

        let mut builder = regex::RegexBuilder::new(pattern);
        builder.size_limit(SIZE_LIMIT);
        for ch in flags.chars() {
            match ch {
                'i' => {
                    builder.case_insensitive(true);
                }
                other => bail!("unsupported regex flag {:?}: only 'i' is supported", other),
            }
        }
        builder
            .build()
            .with_context(|| format!("invalid regex pattern: /{pattern}/"))
    } else {
        // Bare form: unanchored, case-sensitive.
        regex::RegexBuilder::new(s)
            .size_limit(SIZE_LIMIT)
            .build()
            .with_context(|| format!("invalid regex pattern: {s:?}"))
    }
}

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
                        Some(std::cmp::Ordering::Greater) | Some(std::cmp::Ordering::Equal)
                    ),
                    FilterOp::Lt => {
                        yaml_cmp(yaml_val, filter_val) == Some(std::cmp::Ordering::Less)
                    }
                    FilterOp::Lte => matches!(
                        yaml_cmp(yaml_val, filter_val),
                        Some(std::cmp::Ordering::Less) | Some(std::cmp::Ordering::Equal)
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
        Value::Array(seq) => seq.iter().any(|item| yaml_value_eq(item, filter)),
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
    pub properties_typed: bool,
    pub tags: bool,
    pub sections: bool,
    pub tasks: bool,
    pub links: bool,
    /// Backlinks are opt-in only: building the link graph requires scanning all files.
    pub backlinks: bool,
    /// Title extracted from frontmatter `title` property or first H1 heading.
    pub title: bool,
}

impl Default for Fields {
    fn default() -> Self {
        Self {
            properties: true,
            properties_typed: false,
            tags: true,
            sections: true,
            tasks: true,
            links: true,
            backlinks: false,
            title: false,
        }
    }
}

impl Fields {
    /// Parse a fields selection from a list of `--fields` argument values.
    ///
    /// Each element may be a comma-separated list of field names. An empty
    /// slice returns the default (all standard fields enabled; `properties-typed` and `backlinks`
    /// are opt-in).
    pub fn parse(input: &[String]) -> Result<Fields> {
        if input.is_empty() {
            return Ok(Fields::default());
        }

        let mut fields = Fields {
            properties: false,
            properties_typed: false,
            tags: false,
            sections: false,
            tasks: false,
            links: false,
            backlinks: false,
            title: false,
        };

        for item in input {
            for part in item.split(',') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                match part {
                    "all" => {
                        fields.properties = true;
                        fields.properties_typed = true;
                        fields.tags = true;
                        fields.sections = true;
                        fields.tasks = true;
                        fields.links = true;
                        fields.backlinks = true;
                        fields.title = true;
                    }
                    "properties" => fields.properties = true,
                    "properties-typed" => fields.properties_typed = true,
                    "tags" => fields.tags = true,
                    "sections" => fields.sections = true,
                    "tasks" => fields.tasks = true,
                    "links" => fields.links = true,
                    "backlinks" => fields.backlinks = true,
                    "title" => fields.title = true,
                    unknown => bail!(
                        "unknown field {:?}: valid fields are all, properties, properties-typed, tags, sections, tasks, links, backlinks, title",
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
    BacklinksCount,
    LinksCount,
    /// Sort by the resolved title (frontmatter `title` property, then first H1).
    Title,
    /// Sort by a frontmatter property value (e.g. `date`, or any key via `property:KEY`).
    Property(String),
}

/// Parse a sort field from a string.
///
/// Accepts built-in fields (`file`, `modified`, `backlinks_count`, `links_count`)
/// and frontmatter property names via the `property:<KEY>` syntax.
/// `title` and `date` are convenient aliases for `property:title` and `property:date`.
pub fn parse_sort(input: &str) -> Result<SortField> {
    match input {
        "file" => Ok(SortField::File),
        "modified" => Ok(SortField::Modified),
        "backlinks_count" => Ok(SortField::BacklinksCount),
        "links_count" => Ok(SortField::LinksCount),
        "title" => Ok(SortField::Title),
        "date" => Ok(SortField::Property("date".to_owned())),
        other => {
            if let Some(key) = other.strip_prefix("property:") {
                if key.is_empty() {
                    bail!("property sort key must not be empty: use 'property:<KEY>'");
                }
                Ok(SortField::Property(key.to_owned()))
            } else {
                bail!(
                    "unknown sort field {:?}: valid values are 'file', 'modified', \
                     'backlinks_count', 'links_count', 'title', 'date', or 'property:<KEY>'",
                    other
                )
            }
        }
    }
}

// Extract a `YYYY-MM-DD` prefix from an ISO 8601 date or datetime string.
// Returns `Some(prefix)` when the first 10 characters form a valid date,
// `None` otherwise.  Only ISO format is recognised — locale-dependent
// formats like `MM/DD/YYYY` are intentionally ignored.
fn try_as_iso_date(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    // All other positions must be ASCII digits.
    for &i in &[0, 1, 2, 3, 5, 6, 8, 9] {
        if !bytes[i].is_ascii_digit() {
            return None;
        }
    }
    // Basic range check: month 01–12, day 01–31.
    let month = (bytes[5] - b'0') * 10 + (bytes[6] - b'0');
    let day = (bytes[8] - b'0') * 10 + (bytes[9] - b'0');
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(&s[..10])
}

/// Compare two `serde_json::Value`s for sorting purposes.
///
/// Ordering rules:
/// - `Null` / missing sorts **last** (greater than any non-null value).
/// - Strings are compared lexicographically (case-sensitive).
/// - Numbers are compared as f64 (may lose precision for very large integers).
/// - Booleans: `false` < `true`.
/// - All other cases (including mixed primitive types like string vs number,
///   arrays, and objects) fall back to comparing their JSON string
///   representations, ensuring a total ordering across all JSON value types.
pub fn compare_property_values(
    a: Option<&serde_json::Value>,
    b: Option<&serde_json::Value>,
) -> std::cmp::Ordering {
    use serde_json::Value;
    use std::cmp::Ordering;

    match (a, b) {
        (None | Some(Value::Null), None | Some(Value::Null)) => Ordering::Equal,
        (None | Some(Value::Null), _) => Ordering::Greater, // missing sorts last
        (_, None | Some(Value::Null)) => Ordering::Less,
        (Some(Value::String(sa)), Some(Value::String(sb))) => {
            if let (Some(da), Some(db)) = (try_as_iso_date(sa), try_as_iso_date(sb)) {
                da.cmp(db)
            } else {
                sa.cmp(sb)
            }
        }
        (Some(Value::Number(na)), Some(Value::Number(nb))) => {
            let fa = na.as_f64().unwrap_or(f64::NAN);
            let fb = nb.as_f64().unwrap_or(f64::NAN);
            fa.partial_cmp(&fb).unwrap_or(Ordering::Equal)
        }
        (Some(Value::Bool(ba)), Some(Value::Bool(bb))) => ba.cmp(bb),
        (Some(va), Some(vb)) => {
            // Fallback: compare JSON representations.
            let sa = va.to_string();
            let sb = vb.to_string();
            sa.cmp(&sb)
        }
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use serde_json::{Value, json};

    // -----------------------------------------------------------------------
    // Property filter parsing
    // -----------------------------------------------------------------------

    // Helper: assert the filter is a Scalar variant with the given fields.
    fn assert_scalar(
        f: &PropertyFilter,
        exp_name: &str,
        exp_op: FilterOp,
        exp_value: Option<&str>,
    ) {
        match f {
            PropertyFilter::Scalar { name, op, value } => {
                assert_eq!(name, exp_name);
                assert_eq!(op, &exp_op);
                assert_eq!(value.as_deref(), exp_value);
            }
            other => panic!("expected Scalar, got {other:?}"),
        }
    }

    #[test]
    fn parse_eq() {
        let f = parse_property_filter("status=planned").unwrap();
        assert_scalar(&f, "status", FilterOp::Eq, Some("planned"));
    }

    #[test]
    fn parse_not_eq() {
        let f = parse_property_filter("status!=superseded").unwrap();
        assert_scalar(&f, "status", FilterOp::NotEq, Some("superseded"));
    }

    #[test]
    fn parse_gte() {
        let f = parse_property_filter("priority>=3").unwrap();
        assert_scalar(&f, "priority", FilterOp::Gte, Some("3"));
    }

    #[test]
    fn parse_lte() {
        let f = parse_property_filter("priority<=5").unwrap();
        assert_scalar(&f, "priority", FilterOp::Lte, Some("5"));
    }

    #[test]
    fn parse_gt() {
        let f = parse_property_filter("priority>3").unwrap();
        assert_scalar(&f, "priority", FilterOp::Gt, Some("3"));
    }

    #[test]
    fn parse_lt() {
        let f = parse_property_filter("priority<5").unwrap();
        assert_scalar(&f, "priority", FilterOp::Lt, Some("5"));
    }

    #[test]
    fn parse_exists() {
        let f = parse_property_filter("status").unwrap();
        assert_scalar(&f, "status", FilterOp::Exists, None);
    }

    #[test]
    fn parse_value_contains_equals() {
        // Value itself contains `=`; only the first `=` is the delimiter.
        let f = parse_property_filter("key=a=b").unwrap();
        assert_scalar(&f, "key", FilterOp::Eq, Some("a=b"));
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
    // Absence filter parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_absence_simple() {
        let f = parse_property_filter("!status").unwrap();
        match f {
            PropertyFilter::Absent { key } => assert_eq!(key, "status"),
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn parse_absence_empty_key_errors() {
        assert!(parse_property_filter("!").is_err());
    }

    #[test]
    fn parse_absence_backslash_escaped() {
        // zsh escapes `!` to `\!` — `\!status` should be treated as `!status`
        let f = parse_property_filter("\\!status").unwrap();
        match f {
            PropertyFilter::Absent { key } => assert_eq!(key, "status"),
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn parse_absence_not_confused_with_not_eq() {
        // `status!=completed` is NotEq, not absence
        let f = parse_property_filter("status!=completed").unwrap();
        assert_scalar(&f, "status", FilterOp::NotEq, Some("completed"));
    }

    // -----------------------------------------------------------------------
    // Regex filter parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_regex_bare() {
        let f = parse_property_filter("status~=compl").unwrap();
        match &f {
            PropertyFilter::RegexMatch { key, pattern } => {
                assert_eq!(key, "status");
                assert!(pattern.is_match("completed"));
                assert!(!pattern.is_match("planned"));
            }
            other => panic!("expected RegexMatch, got {other:?}"),
        }
    }

    #[test]
    fn parse_regex_delimited() {
        let f = parse_property_filter(r"status~=/^draft$/").unwrap();
        match &f {
            PropertyFilter::RegexMatch { key, pattern } => {
                assert_eq!(key, "status");
                assert!(pattern.is_match("draft"));
                assert!(!pattern.is_match("drafts"));
                assert!(!pattern.is_match("some draft here"));
            }
            other => panic!("expected RegexMatch, got {other:?}"),
        }
    }

    #[test]
    fn parse_regex_delimited_case_insensitive_flag() {
        let f = parse_property_filter("title~=/foo/i").unwrap();
        match &f {
            PropertyFilter::RegexMatch { key, pattern } => {
                assert_eq!(key, "title");
                assert!(pattern.is_match("FOO bar"));
                assert!(pattern.is_match("foo bar"));
                assert!(!pattern.is_match("bar baz"));
            }
            other => panic!("expected RegexMatch, got {other:?}"),
        }
    }

    // --- =~ alias (Perl/Ruby-style) ---

    #[test]
    fn parse_regex_eq_tilde_bare() {
        // `=~` bare pattern should behave identically to `~=`
        let f = parse_property_filter("status=~compl").unwrap();
        match &f {
            PropertyFilter::RegexMatch { key, pattern } => {
                assert_eq!(key, "status");
                assert!(pattern.is_match("completed"));
                assert!(!pattern.is_match("planned"));
            }
            other => panic!("expected RegexMatch, got {other:?}"),
        }
    }

    #[test]
    fn parse_regex_eq_tilde_delimited() {
        let f = parse_property_filter(r"status=~/^draft$/").unwrap();
        match &f {
            PropertyFilter::RegexMatch { key, pattern } => {
                assert_eq!(key, "status");
                assert!(pattern.is_match("draft"));
                assert!(!pattern.is_match("drafts"));
            }
            other => panic!("expected RegexMatch, got {other:?}"),
        }
    }

    #[test]
    fn parse_regex_eq_tilde_case_insensitive_flag() {
        let f = parse_property_filter("title=~/foo/i").unwrap();
        match &f {
            PropertyFilter::RegexMatch { key, pattern } => {
                assert_eq!(key, "title");
                assert!(pattern.is_match("FOO bar"));
                assert!(!pattern.is_match("bar baz"));
            }
            other => panic!("expected RegexMatch, got {other:?}"),
        }
    }

    #[test]
    fn parse_regex_eq_tilde_empty_key_errors() {
        assert!(parse_property_filter("=~foo").is_err());
    }

    #[test]
    fn parse_not_eq_value_starting_with_tilde() {
        let f = parse_property_filter("status!=~foo").unwrap();
        assert_scalar(&f, "status", FilterOp::NotEq, Some("~foo"));
    }

    #[test]
    fn parse_gte_value_starting_with_tilde() {
        let f = parse_property_filter("count>=~3").unwrap();
        assert_scalar(&f, "count", FilterOp::Gte, Some("~3"));
    }

    #[test]
    fn parse_lte_value_starting_with_tilde() {
        let f = parse_property_filter("count<=~3").unwrap();
        assert_scalar(&f, "count", FilterOp::Lte, Some("~3"));
    }

    #[test]
    fn parse_regex_empty_key_errors() {
        assert!(parse_property_filter("~=foo").is_err());
    }

    #[test]
    fn parse_regex_invalid_pattern_errors() {
        assert!(parse_property_filter("status~=[invalid").is_err());
    }

    #[test]
    fn parse_regex_missing_closing_slash_errors() {
        assert!(parse_property_filter("status~=/unclosed").is_err());
    }

    #[test]
    fn parse_regex_unsupported_flag_errors() {
        assert!(parse_property_filter("status~=/foo/x").is_err());
    }

    // -----------------------------------------------------------------------
    // Absence filter matching
    // -----------------------------------------------------------------------

    #[test]
    fn match_absent_key_not_present() {
        let p = props(&[("status", Value::String("planned".into()))]);
        let f = parse_property_filter("!priority").unwrap();
        assert!(f.matches(&p), "priority absent — should match");
    }

    #[test]
    fn match_absent_key_present_no_match() {
        let p = props(&[("status", Value::String("planned".into()))]);
        let f = parse_property_filter("!status").unwrap();
        assert!(!f.matches(&p), "status present — should NOT match");
    }

    #[test]
    fn match_absent_empty_frontmatter() {
        let p = props(&[]);
        let f = parse_property_filter("!priority").unwrap();
        assert!(f.matches(&p));
    }

    // -----------------------------------------------------------------------
    // Regex filter matching
    // -----------------------------------------------------------------------

    #[test]
    fn match_regex_bare_substring() {
        let p = props(&[("status", Value::String("completed".into()))]);
        let f = parse_property_filter("status~=compl").unwrap();
        assert!(f.matches(&p));
    }

    #[test]
    fn match_regex_no_match() {
        let p = props(&[("status", Value::String("planned".into()))]);
        let f = parse_property_filter("status~=compl").unwrap();
        assert!(!f.matches(&p));
    }

    #[test]
    fn match_regex_missing_key_no_match() {
        let p = props(&[]);
        let f = parse_property_filter("status~=compl").unwrap();
        assert!(!f.matches(&p));
    }

    #[test]
    fn match_regex_list_any_element() {
        let p = props(&[(
            "tags",
            Value::Array(vec![
                Value::String("rust".into()),
                Value::String("cli-tool".into()),
            ]),
        )]);
        let f = parse_property_filter("tags~=cli").unwrap();
        assert!(f.matches(&p));
        let f2 = parse_property_filter("tags~=python").unwrap();
        assert!(!f2.matches(&p));
    }

    #[test]
    fn match_regex_case_insensitive_flag() {
        let p = props(&[("title", Value::String("My Foo Project".into()))]);
        let f = parse_property_filter("title~=/foo/i").unwrap();
        assert!(f.matches(&p));
    }

    #[test]
    fn match_regex_case_sensitive_by_default() {
        let p = props(&[("title", Value::String("My Foo Project".into()))]);
        let f = parse_property_filter("title~=foo").unwrap();
        // bare pattern is case-sensitive; "Foo" != "foo"
        assert!(!f.matches(&p));
        let f2 = parse_property_filter("title~=Foo").unwrap();
        assert!(f2.matches(&p));
    }

    #[test]
    fn match_regex_anchored_exact() {
        let p = props(&[("status", Value::String("draft".into()))]);
        let f_exact = parse_property_filter(r"status~=/^draft$/").unwrap();
        assert!(f_exact.matches(&p));
        let f_no = parse_property_filter(r"status~=/^drafts$/").unwrap();
        assert!(!f_no.matches(&p));
    }

    #[test]
    fn match_regex_mapping_key() {
        // versions: {fpt: "*", ghes: "*", ghec: "*"}
        let p = props(&[("versions", json!({"fpt": "*", "ghes": "*", "ghec": "*"}))]);
        let f = parse_property_filter("versions~=ghes").unwrap();
        assert!(f.matches(&p));
        let f2 = parse_property_filter("versions~=nonexistent").unwrap();
        assert!(!f2.matches(&p));
    }

    #[test]
    fn match_regex_mapping_value() {
        let p = props(&[("versions", json!({"ghes": ">=3.10"}))]);
        // Match on the value, not the key
        let f = parse_property_filter("versions~=3\\.10").unwrap();
        assert!(f.matches(&p));
    }

    // -----------------------------------------------------------------------
    // Property filter matching
    // -----------------------------------------------------------------------

    fn props(pairs: &[(&str, Value)]) -> IndexMap<String, Value> {
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
            Value::Array(vec![
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
            Value::Array(vec![
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
    fn fields_properties_typed() {
        let input = vec!["properties-typed".to_owned()];
        let f = Fields::parse(&input).unwrap();
        assert!(!f.properties);
        assert!(f.properties_typed);
        assert!(!f.tags);
        assert!(!f.sections);
        assert!(!f.tasks);
        assert!(!f.links);
    }

    #[test]
    fn fields_properties_and_properties_typed_together() {
        let input = vec!["properties,properties-typed".to_owned()];
        let f = Fields::parse(&input).unwrap();
        assert!(f.properties);
        assert!(f.properties_typed);
    }

    #[test]
    fn fields_all_keyword_enables_everything() {
        let input = vec!["all".to_owned()];
        let f = Fields::parse(&input).unwrap();
        assert!(f.properties, "properties should be set");
        assert!(f.properties_typed, "properties_typed should be set");
        assert!(f.tags, "tags should be set");
        assert!(f.sections, "sections should be set");
        assert!(f.tasks, "tasks should be set");
        assert!(f.links, "links should be set");
        assert!(f.backlinks, "backlinks should be set");
        assert!(f.title, "title should be set");
    }

    #[test]
    fn fields_title_only() {
        let input = vec!["title".to_owned()];
        let f = Fields::parse(&input).unwrap();
        assert!(f.title, "title should be set");
        assert!(!f.properties, "properties should not be set");
        assert!(!f.tags, "tags should not be set");
        assert!(!f.sections, "sections should not be set");
        assert!(!f.tasks, "tasks should not be set");
        assert!(!f.links, "links should not be set");
        assert!(!f.backlinks, "backlinks should not be set");
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
    fn sort_title() {
        assert_eq!(parse_sort("title").unwrap(), SortField::Title);
    }

    #[test]
    fn sort_date_alias() {
        assert_eq!(
            parse_sort("date").unwrap(),
            SortField::Property("date".to_owned())
        );
    }

    #[test]
    fn sort_property_generic() {
        assert_eq!(
            parse_sort("property:priority").unwrap(),
            SortField::Property("priority".to_owned())
        );
    }

    #[test]
    fn sort_property_empty_key_errors() {
        assert!(parse_sort("property:").is_err());
    }

    #[test]
    fn sort_unknown_errors() {
        assert!(parse_sort("name").is_err());
        assert!(parse_sort("").is_err());
    }

    // -----------------------------------------------------------------------
    // compare_property_values
    // -----------------------------------------------------------------------

    #[test]
    fn compare_null_sorts_last() {
        use std::cmp::Ordering;
        let s = Value::String("alpha".into());
        // non-null < null (null sorts last)
        assert_eq!(compare_property_values(Some(&s), None), Ordering::Less);
        assert_eq!(compare_property_values(None, Some(&s)), Ordering::Greater);
        assert_eq!(compare_property_values(None, None), Ordering::Equal);
        assert_eq!(
            compare_property_values(Some(&Value::Null), None),
            Ordering::Equal
        );
    }

    #[test]
    fn compare_strings() {
        use std::cmp::Ordering;
        let a = Value::String("alpha".into());
        let b = Value::String("beta".into());
        assert_eq!(compare_property_values(Some(&a), Some(&b)), Ordering::Less);
        assert_eq!(
            compare_property_values(Some(&b), Some(&a)),
            Ordering::Greater
        );
        assert_eq!(compare_property_values(Some(&a), Some(&a)), Ordering::Equal);
    }

    #[test]
    fn compare_numbers() {
        use std::cmp::Ordering;
        let a = json!(1);
        let b = json!(2);
        assert_eq!(compare_property_values(Some(&a), Some(&b)), Ordering::Less);
    }

    #[test]
    fn compare_booleans() {
        use std::cmp::Ordering;
        let f = json!(false);
        let t = json!(true);
        assert_eq!(compare_property_values(Some(&f), Some(&t)), Ordering::Less);
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
        // Value is a YAML array — filter matches any element.
        let p = props(&[(
            "tags",
            Value::Array(vec![
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
            Value::Array(vec![Value::String("inbox/processing".into())]),
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
            ("tags", Value::Array(vec![Value::String("rust".into())])),
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

    // -----------------------------------------------------------------------
    // Bug 1: existence-check fallback rejects operator-like chars
    // -----------------------------------------------------------------------

    #[test]
    fn parse_existence_with_bang_errors() {
        assert!(parse_property_filter("title!!!broken").is_err());
    }

    #[test]
    fn parse_existence_with_tilde_errors() {
        assert!(parse_property_filter("name~bad").is_err());
    }

    #[test]
    fn parse_existence_valid_name_succeeds() {
        let f = parse_property_filter("valid_name").unwrap();
        assert_scalar(&f, "valid_name", FilterOp::Exists, None);
    }

    // -----------------------------------------------------------------------
    // Bug 4: try_as_iso_date
    // -----------------------------------------------------------------------

    #[test]
    fn try_as_iso_date_valid() {
        assert_eq!(try_as_iso_date("2023-01-18"), Some("2023-01-18"));
        assert_eq!(try_as_iso_date("2026-02-04T08:00:00"), Some("2026-02-04"));
    }

    #[test]
    fn try_as_iso_date_invalid_separator() {
        assert_eq!(try_as_iso_date("2023/01/18"), None);
        assert_eq!(try_as_iso_date("20230118"), None);
    }

    #[test]
    fn try_as_iso_date_invalid_month_or_day() {
        assert_eq!(try_as_iso_date("2023-00-01"), None);
        assert_eq!(try_as_iso_date("2023-13-01"), None);
        assert_eq!(try_as_iso_date("2023-01-00"), None);
        assert_eq!(try_as_iso_date("2023-01-32"), None);
    }

    #[test]
    fn try_as_iso_date_too_short() {
        assert_eq!(try_as_iso_date("2023-01"), None);
        assert_eq!(try_as_iso_date(""), None);
    }

    #[test]
    fn try_as_iso_date_non_digit() {
        assert_eq!(try_as_iso_date("YYYY-MM-DD"), None);
    }

    // -----------------------------------------------------------------------
    // Bug 4: compare_property_values date-aware string sort
    // -----------------------------------------------------------------------

    #[test]
    fn compare_iso_dates_correct_order() {
        use std::cmp::Ordering;
        let a = Value::String("2023-01-18".into());
        let b = Value::String("2026-02-04".into());
        assert_eq!(compare_property_values(Some(&a), Some(&b)), Ordering::Less);
        assert_eq!(
            compare_property_values(Some(&b), Some(&a)),
            Ordering::Greater
        );
        assert_eq!(compare_property_values(Some(&a), Some(&a)), Ordering::Equal);
    }

    #[test]
    fn compare_iso_datetimes_correct_order() {
        use std::cmp::Ordering;
        let a = Value::String("2023-01-18T10:00:00".into());
        let b = Value::String("2026-02-04T08:00:00".into());
        assert_eq!(compare_property_values(Some(&a), Some(&b)), Ordering::Less);
    }

    #[test]
    fn compare_non_date_strings_lexicographic() {
        use std::cmp::Ordering;
        let a = Value::String("alpha".into());
        let b = Value::String("beta".into());
        assert_eq!(compare_property_values(Some(&a), Some(&b)), Ordering::Less);
    }

    #[test]
    fn compare_mixed_date_and_non_date_fallback_lexicographic() {
        use std::cmp::Ordering;
        // "2023-01-18" is a valid date; "not-a-date" is not.
        // Falls back to lexicographic: "2" < "n" in ASCII.
        let a = Value::String("2023-01-18".into());
        let b = Value::String("not-a-date".into());
        assert_eq!(compare_property_values(Some(&a), Some(&b)), Ordering::Less);
    }
}
