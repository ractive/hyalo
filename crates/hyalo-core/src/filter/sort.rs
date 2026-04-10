use anyhow::{Result, bail};

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
    /// Sort by BM25 relevance score (highest first). Set automatically when a PATTERN is provided.
    Score,
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
        "score" => Ok(SortField::Score),
        other => {
            if let Some(key) = other.strip_prefix("property:") {
                if key.is_empty() {
                    bail!("property sort key must not be empty: use 'property:<KEY>'");
                }
                Ok(SortField::Property(key.to_owned()))
            } else {
                bail!(
                    "unknown sort field {other:?}: valid values are 'file', 'modified', \
                     'backlinks_count', 'links_count', 'title', 'date', 'score', or 'property:<KEY>'"
                )
            }
        }
    }
}

// Extract a `YYYY-MM-DD` prefix from an ISO 8601 date or datetime string.
// Returns `Some(prefix)` when the first 10 characters look like a `YYYY-MM-DD`
// date with basic bounds (month 01-12, day 01-31), `None` otherwise. Only ISO
// format is recognised -- locale-dependent formats like `MM/DD/YYYY` are
// intentionally ignored. This does not validate actual calendar dates (e.g.
// it may accept `2023-02-31`).
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
/// - Strings: if both values look like ISO 8601 dates (`YYYY-MM-DD` prefix),
///   compare by date prefix first; otherwise compare lexicographically
///   (case-sensitive).
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
