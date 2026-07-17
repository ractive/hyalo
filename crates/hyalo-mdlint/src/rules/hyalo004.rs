//! HYALO004 — datetime-typed frontmatter property has a non-ISO-8601 value.
//!
//! Fires when a property that the active schema declares as `datetime` (naive)
//! or `datetime-tz` (timezone-aware) is present in frontmatter with a string
//! value that does not match the expected shape:
//! - `datetime`    → `YYYY-MM-DDThh:mm:ss` (naive, no offset)
//! - `datetime-tz` → `YYYY-MM-DDThh:mm:ss` plus a `Z` or `±hh:mm` offset
//!
//! Both require a valid calendar date and wall-clock time.
//!
//! Default severity: `warn`. Escalated to `error` by `--strict`.
//!
//! Unlike [`hyalo003`](super::hyalo003), HYALO004 has no conventional-key
//! fallback — it only fires for fields the user has explicitly declared as
//! `datetime`/`datetime-tz` in their schema. This avoids the false-positive
//! risk of pattern-matching on names like `created_at`/`updated_at`, which are
//! commonly stored as dates rather than full datetimes.

/// Validate a list of `(name, value, is_tz)` triples known to be
/// schema-declared as `datetime` (`is_tz == false`) or `datetime-tz`
/// (`is_tz == true`). Returns the triples whose values do not match the shape
/// required by their declared type.
///
/// Callers are responsible for filtering frontmatter properties down to the
/// schema-declared datetime fields and extracting their string values; this
/// keeps the rule decoupled from the schema crate.
pub fn check_datetime_properties(pairs: &[(&str, &str, bool)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .filter_map(|(k, v, is_tz)| {
            let ok = if *is_tz {
                hyalo_core::util::is_iso8601_datetime_tz(v)
            } else {
                hyalo_core::util::is_iso8601_datetime(v)
            };
            if ok {
                None
            } else {
                Some(((*k).to_owned(), (*v).to_owned()))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_datetime_no_violation() {
        let v = check_datetime_properties(&[("when", "2026-06-04T14:30:00", false)]);
        assert!(v.is_empty());
    }

    #[test]
    fn date_only_fires() {
        let v = check_datetime_properties(&[("when", "2026-06-04", false)]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "when");
        assert_eq!(v[0].1, "2026-06-04");
    }

    #[test]
    fn invalid_components_fire() {
        let v = check_datetime_properties(&[
            ("a", "2026-06-04T25:00:00", false),
            ("b", "2026-13-04T00:00:00", false),
            ("c", "2026-06-04T14:30:00", false),
        ]);
        assert_eq!(v.len(), 2);
        let keys: Vec<&str> = v.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"a"));
        assert!(keys.contains(&"b"));
    }

    #[test]
    fn tz_aware_value_no_violation() {
        // Both YAML spellings from the OKF material validate clean when tz-typed.
        let v = check_datetime_properties(&[
            ("ts", "2026-05-28T22:44:47+00:00", true),
            ("ts2", "2026-05-28T14:30:00Z", true),
        ]);
        assert!(v.is_empty());
    }

    #[test]
    fn naive_value_in_tz_property_fires() {
        // A naive value in a datetime-tz property is a violation.
        let v = check_datetime_properties(&[("ts", "2026-05-28T14:30:00", true)]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "ts");
    }

    #[test]
    fn tz_value_in_naive_property_fires() {
        // A tz value in a naive datetime property is a violation.
        let v = check_datetime_properties(&[("when", "2026-05-28T14:30:00Z", false)]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "when");
    }
}
