//! HYALO003 — date-typed frontmatter key has a non-ISO-8601 value.
//!
//! Fires when a property whose name is a well-known date key (`date`,
//! `created`, `modified`, `updated`) is present in frontmatter with a value
//! that does not match the `YYYY-MM-DD` pattern.
//!
//! Default severity: `warn`.  Escalated to `error` by `--strict`.

/// Well-known date-typed frontmatter keys.
pub const DATE_KEYS: &[&str] = &["date", "created", "modified", "updated"];

/// Returns `true` when `key` is a well-known date-typed frontmatter property.
pub fn is_date_key(key: &str) -> bool {
    DATE_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key))
}

/// Check all `properties` (frontmatter) for HYALO003 violations.
///
/// Returns a list of `(key, bad_value)` pairs.  Callers translate these into
/// diagnostics with the appropriate severity.
pub fn check_date_keys(
    properties: &indexmap::IndexMap<String, serde_json::Value>,
) -> Vec<(&str, String)> {
    let mut violations = Vec::new();
    for (key, val) in properties {
        if !is_date_key(key) {
            continue;
        }
        // Accept scalar strings that look like YYYY-MM-DD.
        // Null / arrays / objects are ignored (let schema rules handle those).
        let Some(s) = val.as_str() else { continue };
        if !hyalo_core::util::is_iso8601_date(s) {
            violations.push((key.as_str(), (*s).to_owned()));
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use serde_json::Value;

    fn props(pairs: &[(&str, &str)]) -> IndexMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect()
    }

    #[test]
    fn clean_date_no_violation() {
        let p = props(&[("date", "2026-05-10")]);
        assert!(check_date_keys(&p).is_empty());
    }

    #[test]
    fn bad_date_fires() {
        let p = props(&[("date", "not-a-date")]);
        let v = check_date_keys(&p);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "date");
    }

    #[test]
    fn all_known_keys_checked() {
        let p = props(&[
            ("date", "bad"),
            ("created", "also-bad"),
            ("modified", "2026-01-01"),
            ("updated", "nope"),
        ]);
        let v = check_date_keys(&p);
        assert_eq!(v.len(), 3); // modified is fine
        let keys: Vec<&str> = v.iter().map(|(k, _)| *k).collect();
        assert!(keys.contains(&"date"));
        assert!(keys.contains(&"created"));
        assert!(keys.contains(&"updated"));
    }

    #[test]
    fn non_date_key_ignored() {
        let p = props(&[("title", "not-a-date"), ("status", "planned")]);
        assert!(check_date_keys(&p).is_empty());
    }

    #[test]
    fn null_value_ignored() {
        let mut p: IndexMap<String, Value> = IndexMap::new();
        p.insert("date".to_owned(), Value::Null);
        assert!(check_date_keys(&p).is_empty());
    }

    #[test]
    fn case_insensitive_key_match() {
        let p = props(&[("DATE", "oops")]);
        let v = check_date_keys(&p);
        assert_eq!(v.len(), 1);
    }
}
