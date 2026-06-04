//! HYALO004 — datetime-typed frontmatter property has a non-ISO-8601 value.
//!
//! Fires when a property that the active schema declares as `datetime` is
//! present in frontmatter with a string value that does not match the
//! `YYYY-MM-DDThh:mm:ss` shape (with a valid calendar date and wall-clock
//! time).
//!
//! Default severity: `warn`. Escalated to `error` by `--strict`.
//!
//! Unlike [`hyalo003`](super::hyalo003), HYALO004 has no conventional-key
//! fallback — it only fires for fields the user has explicitly declared as
//! `datetime` in their schema. This avoids the false-positive risk of
//! pattern-matching on names like `created_at`/`updated_at`, which are
//! commonly stored as dates rather than full datetimes.

/// Validate a list of `(name, value)` pairs known to be schema-declared as
/// `datetime`. Returns the pairs whose values do not match
/// `YYYY-MM-DDThh:mm:ss`.
///
/// Callers are responsible for filtering frontmatter properties down to the
/// schema-declared datetime fields and extracting their string values; this
/// keeps the rule decoupled from the schema crate.
pub fn check_datetime_properties(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .filter_map(|(k, v)| {
            if hyalo_core::util::is_iso8601_datetime(v) {
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
        let v = check_datetime_properties(&[("when", "2026-06-04T14:30:00")]);
        assert!(v.is_empty());
    }

    #[test]
    fn date_only_fires() {
        let v = check_datetime_properties(&[("when", "2026-06-04")]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "when");
        assert_eq!(v[0].1, "2026-06-04");
    }

    #[test]
    fn invalid_components_fire() {
        let v = check_datetime_properties(&[
            ("a", "2026-06-04T25:00:00"),
            ("b", "2026-13-04T00:00:00"),
            ("c", "2026-06-04T14:30:00"),
        ]);
        assert_eq!(v.len(), 2);
        let keys: Vec<&str> = v.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"a"));
        assert!(keys.contains(&"b"));
    }
}
