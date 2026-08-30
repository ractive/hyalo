//! HYALO007 — frontmatter `title` is a collection rather than a scalar.
//!
//! Every scalar `title` promotes to the `find --fields title` value,
//! stringified as written (`title: 42` → `"42"`, `title: true` → `"true"`) —
//! see `promoted_title_string` in the CLI. A list or a map has no honest
//! string form, so it cannot promote: the result's `title` silently falls back
//! to the first H1 and the authored value is only reachable under
//! `properties`. That is almost always a typo (`title: [Draft] Notes` parses
//! as a one-element list) rather than an intent, so it is worth a warning.
//!
//! Null, empty and whitespace-only titles are *not* flagged: they are an
//! ordinary "no title yet", already covered by the schema's required-property
//! check when a type declares `title` as required.
//!
//! Default severity: `warn`. Escalated to `error` by `--strict`.

/// The JSON kind name reported for a non-scalar `title`, or `None` when the
/// value is a scalar (or absent) and the rule does not fire.
#[must_use]
pub fn non_scalar_title_kind(
    properties: &indexmap::IndexMap<String, serde_json::Value>,
) -> Option<&'static str> {
    match properties.get("title")? {
        serde_json::Value::Array(_) => Some("list"),
        serde_json::Value::Object(_) => Some("map"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use serde_json::{Value, json};

    fn props(value: Value) -> IndexMap<String, Value> {
        let mut p = IndexMap::new();
        p.insert("title".to_owned(), value);
        p
    }

    #[test]
    fn list_title_fires_as_list() {
        assert_eq!(
            non_scalar_title_kind(&props(json!(["a", "b"]))),
            Some("list")
        );
    }

    #[test]
    fn map_title_fires_as_map() {
        assert_eq!(
            non_scalar_title_kind(&props(json!({"k": "v"}))),
            Some("map")
        );
    }

    #[test]
    fn scalar_titles_do_not_fire() {
        for v in [json!("Notes"), json!(42), json!(1.0), json!(true)] {
            assert_eq!(non_scalar_title_kind(&props(v.clone())), None, "{v}");
        }
    }

    #[test]
    fn absent_null_and_blank_titles_do_not_fire() {
        assert_eq!(non_scalar_title_kind(&IndexMap::new()), None);
        assert_eq!(non_scalar_title_kind(&props(Value::Null)), None);
        assert_eq!(non_scalar_title_kind(&props(json!("   "))), None);
    }
}
