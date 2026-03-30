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
