use anyhow::{Context, Result, bail};
use regex::Regex;

/// Comparison operator for property filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// `K=null` — property present with a YAML null value (`~`, `null`, or an
    /// empty value). A list *containing* a null does not match (iter-264,
    /// DEC-274).
    IsNull,
    /// `K!=null` — property present with a non-null value.
    NotNull,
    /// `K=[]` — property present and holding an empty list.
    IsEmptyList,
    /// `K!=[]` — property present and not an empty list.
    NotEmptyList,
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

impl PropertyFilter {
    /// Return the property key this filter targets, if any.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        match self {
            PropertyFilter::Scalar { name, .. } => Some(name),
            PropertyFilter::Absent { key } | PropertyFilter::RegexMatch { key, .. } => Some(key),
        }
    }
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
/// - `name=null`         → `IsNull` (present with a YAML null value)
/// - `name!=null`        → `NotNull` (present with a non-null value)
/// - `name=[]`           → `IsEmptyList` (present and an empty list)
/// - `name!=[]`          → `NotEmptyList` (present and not an empty list)
///
/// Rejected (iter-264, DEC-276): `name=~pattern` — `=~` was never an operator,
/// it only worked because `=` split first and `~pattern` was then compared as a
/// literal value. It is now a hard error naming `~=`.
///
/// An empty regex (`name~=` or `name~=//`) is rejected too: it matches every
/// value, which is never what the caller meant — bare `name` tests presence.
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

    // --- Regex filter: `key~=pattern` (and delimited forms) ---
    //
    // `~=` is the one regex operator. `=~` (the Perl/Ruby spelling) is rejected
    // below rather than silently accepted (DEC-276): it used to "work" only
    // because `=` split first and `~pattern` became a literal equality value,
    // which quietly matched YAML nulls across a whole vault.
    let tilde_eq_pos = input.find("~=");
    let perl_eq_pos = input.find("=~").filter(|&p| {
        // Not the tail of `!=`, `>=` or `<=`, whose value may legitimately
        // start with `~` (`k!=~foo` compares against the literal `~foo`).
        p == 0 || !matches!(input.as_bytes()[p - 1], b'!' | b'>' | b'<')
    });
    if let Some(p) = perl_eq_pos
        && tilde_eq_pos.is_none_or(|t| p < t)
    {
        let key = &input[..p];
        let pattern = &input[p + 2..];
        bail!(
            "unknown operator '=~' in property filter {input:?}: use '~=' for a regex match \
             (e.g. '{key}~={pattern}')"
        );
    }

    if let Some(op_pos) = tilde_eq_pos {
        let key = &input[..op_pos];
        let pattern_part = &input[op_pos + 2..];

        if key.is_empty() {
            bail!("property filter name must not be empty");
        }
        if regex_pattern_is_empty(pattern_part) {
            bail!(
                "empty regex in property filter {input:?}: an empty pattern matches every value; \
                 use bare '{key}' to test for presence, or give the pattern (e.g. '{key}~=/draft/')"
            );
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

        // `K=null` / `K!=null` / `K=[]` / `K!=[]` are value *syntax*, not
        // string comparisons (iter-264, DEC-274): a YAML null has no text form
        // a string compare could match, and `[]` is a shape, not a value.
        if matches!(op, FilterOp::Eq | FilterOp::NotEq) {
            let special = match value.trim() {
                "null" | "~" => Some((FilterOp::IsNull, FilterOp::NotNull)),
                "[]" => Some((FilterOp::IsEmptyList, FilterOp::NotEmptyList)),
                _ => None,
            };
            if let Some((positive, negative)) = special {
                return Ok(PropertyFilter::Scalar {
                    name: name.to_owned(),
                    op: if op == FilterOp::Eq {
                        positive
                    } else {
                        negative
                    },
                    value: None,
                });
            }
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
             supported operators: =, !=, >=, <=, >, <, ~= (regex), ! (absence)"
        );
    }

    Ok(PropertyFilter::Scalar {
        name: input.to_owned(),
        op: FilterOp::Exists,
        value: None,
    })
}

/// Returns `true` when the text after `~=` compiles to a pattern that matches
/// every value: the bare empty pattern (`k~=`) or the empty delimited pattern
/// (`k~=//`, `k~=//i`).
///
/// A lone `/` is *not* reported here — it is an unterminated delimited pattern,
/// and [`parse_regex_pattern`]'s own "must end with '/'" error says so better.
fn regex_pattern_is_empty(s: &str) -> bool {
    match s.strip_prefix('/') {
        None => s.is_empty(),
        // `//` and `//i` both leave an empty pattern before the closing `/`.
        Some(rest) => rest.rfind('/') == Some(0),
    }
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
                other => bail!("unsupported regex flag {other:?}: only 'i' is supported"),
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
