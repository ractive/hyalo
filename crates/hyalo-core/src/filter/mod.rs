mod fields;
mod match_props;
mod parse;
mod sort;
mod tasks;

pub use fields::{DEFAULT_FIELD_NAMES, Fields};
pub use match_props::{
    extract_tags, matches_filters_with_tags, matches_frontmatter_filters, tag_matches,
};
pub use parse::{FilterOp, PropertyFilter, parse_property_filter};
pub use sort::{SortField, compare_property_values, parse_sort};
pub use tasks::{FindTaskFilter, parse_task_filter};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::parse::FilterOp;
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

    // ------------------------------------------------------------------
    // UX-3 (iter-244): nested dot-path property filters
    // ------------------------------------------------------------------

    #[test]
    fn dot_path_eq_traverses_nested_map() {
        let f = parse_property_filter("a.b=v").unwrap();
        let mut props = indexmap::IndexMap::new();
        let mut inner = serde_json::Map::new();
        inner.insert("b".to_owned(), serde_json::Value::String("v".to_owned()));
        props.insert("a".to_owned(), serde_json::Value::Object(inner));
        assert!(f.matches(&props));
    }

    #[test]
    fn dot_path_eq_wrong_value_does_not_match() {
        let f = parse_property_filter("a.b=v").unwrap();
        let mut props = indexmap::IndexMap::new();
        let mut inner = serde_json::Map::new();
        inner.insert("b".to_owned(), serde_json::Value::String("w".to_owned()));
        props.insert("a".to_owned(), serde_json::Value::Object(inner));
        assert!(!f.matches(&props));
    }

    #[test]
    fn dot_path_literal_dotted_key_wins_over_traversal() {
        // A flat map may genuinely contain a dotted key — the literal key is
        // checked first, before any traversal.
        let f = parse_property_filter("a.b=flat").unwrap();
        let mut props = indexmap::IndexMap::new();
        props.insert(
            "a.b".to_owned(),
            serde_json::Value::String("flat".to_owned()),
        );
        assert!(f.matches(&props));
    }

    #[test]
    fn dot_path_absent_matches_when_nested_key_missing() {
        let f = parse_property_filter("!a.b").unwrap();
        let mut props = indexmap::IndexMap::new();
        props.insert(
            "a".to_owned(),
            serde_json::Value::String("scalar".to_owned()),
        );
        assert!(f.matches(&props));
    }

    #[test]
    fn dot_path_exists_and_regex_traverse() {
        let f = parse_property_filter("a.b").unwrap();
        let mut props = indexmap::IndexMap::new();
        let mut inner = serde_json::Map::new();
        inner.insert("b".to_owned(), serde_json::Value::String("x".to_owned()));
        props.insert("a".to_owned(), serde_json::Value::Object(inner));
        assert!(f.matches(&props));

        let r = parse_property_filter("a.b~=^x$").unwrap();
        assert!(r.matches(&props));
    }

    #[test]
    fn dot_path_missing_segment_fails_scalar_filter() {
        let f = parse_property_filter("a.c=v").unwrap();
        let mut props = indexmap::IndexMap::new();
        let mut inner = serde_json::Map::new();
        inner.insert("b".to_owned(), serde_json::Value::String("v".to_owned()));
        props.insert("a".to_owned(), serde_json::Value::Object(inner));
        assert!(!f.matches(&props));
    }

    // ------------------------------------------------------------------
    // UX-3 follow-up (iter-245): dot-paths through sequences of maps
    // ------------------------------------------------------------------

    /// `contacts: [{name, email}, …]` — a non-numeric segment auto-descends
    /// into every element, so `contacts.email=v` matches when ANY element's
    /// `email` equals `v`.
    fn contacts_props() -> IndexMap<String, Value> {
        let mut props = IndexMap::new();
        props.insert(
            "contacts".to_owned(),
            json!([
                {"name": "Ada", "email": "ada@example.com"},
                {"name": "Grace", "email": "grace@example.com"},
            ]),
        );
        props
    }

    #[test]
    fn dot_path_array_of_maps_matches_any_element() {
        let props = contacts_props();
        for value in ["ada@example.com", "grace@example.com"] {
            let f = parse_property_filter(&format!("contacts.email={value}")).unwrap();
            assert!(f.matches(&props), "expected {value} to match any element");
        }
    }

    #[test]
    fn dot_path_array_of_maps_non_matching_value_fails() {
        let f = parse_property_filter("contacts.email=nobody@example.com").unwrap();
        assert!(!f.matches(&contacts_props()));
    }

    #[test]
    fn dot_path_array_of_maps_missing_key_fails() {
        let f = parse_property_filter("contacts.phone=555").unwrap();
        assert!(!f.matches(&contacts_props()));
    }

    #[test]
    fn dot_path_array_index_segment_selects_one_element() {
        let props = contacts_props();
        assert!(
            parse_property_filter("contacts.0.email=ada@example.com")
                .unwrap()
                .matches(&props)
        );
        assert!(
            parse_property_filter("contacts.1.email=grace@example.com")
                .unwrap()
                .matches(&props)
        );
        // An index pins the element: element 1 is not Ada.
        assert!(
            !parse_property_filter("contacts.1.email=ada@example.com")
                .unwrap()
                .matches(&props)
        );
    }

    #[test]
    fn dot_path_array_index_out_of_range_yields_no_match() {
        let props = contacts_props();
        assert!(
            !parse_property_filter("contacts.9.email=ada@example.com")
                .unwrap()
                .matches(&props)
        );
        assert!(
            !parse_property_filter("contacts.9.email")
                .unwrap()
                .matches(&props)
        );
    }

    #[test]
    fn dot_path_array_index_into_scalar_sequence() {
        let mut props = IndexMap::new();
        props.insert("tags".to_owned(), json!(["alpha", "beta"]));
        assert!(
            parse_property_filter("tags.0=alpha")
                .unwrap()
                .matches(&props)
        );
        assert!(
            parse_property_filter("tags.1=beta")
                .unwrap()
                .matches(&props)
        );
        assert!(
            !parse_property_filter("tags.0=beta")
                .unwrap()
                .matches(&props)
        );
        // A non-numeric segment cannot descend into scalar elements.
        assert!(
            !parse_property_filter("tags.name=alpha")
                .unwrap()
                .matches(&props)
        );
    }

    #[test]
    fn dot_path_array_not_eq_requires_no_element_to_match() {
        let props = contacts_props();
        // One element matches, so "not equal" is false — the same semantics a
        // flat list property already has.
        assert!(
            !parse_property_filter("contacts.email!=ada@example.com")
                .unwrap()
                .matches(&props)
        );
        assert!(
            parse_property_filter("contacts.email!=nobody@example.com")
                .unwrap()
                .matches(&props)
        );
    }

    #[test]
    fn dot_path_array_regex_matches_any_element() {
        let props = contacts_props();
        assert!(
            parse_property_filter("contacts.email~=^grace@")
                .unwrap()
                .matches(&props)
        );
        assert!(
            !parse_property_filter("contacts.email~=^nobody@")
                .unwrap()
                .matches(&props)
        );
    }

    #[test]
    fn dot_path_array_exists_and_absent() {
        let props = contacts_props();
        assert!(
            parse_property_filter("contacts.email")
                .unwrap()
                .matches(&props)
        );
        assert!(
            !parse_property_filter("!contacts.email")
                .unwrap()
                .matches(&props)
        );
        assert!(
            !parse_property_filter("contacts.phone")
                .unwrap()
                .matches(&props)
        );
        assert!(
            parse_property_filter("!contacts.phone")
                .unwrap()
                .matches(&props)
        );
    }

    #[test]
    fn dot_path_array_single_hit_supports_ordering_ops() {
        // Exactly one element yields a value, so it is returned bare and the
        // ordering operators (which cannot compare a sequence) still work.
        let mut props = IndexMap::new();
        props.insert(
            "scores".to_owned(),
            json!([{"name": "unit"}, {"name": "e2e", "value": 42}]),
        );
        assert!(
            parse_property_filter("scores.value>10")
                .unwrap()
                .matches(&props)
        );
        assert!(
            !parse_property_filter("scores.value>100")
                .unwrap()
                .matches(&props)
        );
        assert!(
            parse_property_filter("scores.value<=42")
                .unwrap()
                .matches(&props)
        );
    }

    #[test]
    fn dot_path_nested_sequences_flatten_to_leaves() {
        let mut props = IndexMap::new();
        props.insert(
            "groups".to_owned(),
            json!([
                {"members": [{"name": "Ada"}, {"name": "Grace"}]},
                {"members": [{"name": "Alan"}]},
            ]),
        );
        for name in ["Ada", "Grace", "Alan"] {
            assert!(
                parse_property_filter(&format!("groups.members.name={name}"))
                    .unwrap()
                    .matches(&props),
                "expected {name} to be reachable through both sequence levels"
            );
        }
        assert!(
            !parse_property_filter("groups.members.name=Nobody")
                .unwrap()
                .matches(&props)
        );
        // Mixed index + auto-descent segments compose.
        assert!(
            parse_property_filter("groups.1.members.0.name=Alan")
                .unwrap()
                .matches(&props)
        );
    }

    #[test]
    fn dot_path_map_then_sequence_composes() {
        let mut props = IndexMap::new();
        props.insert(
            "meta".to_owned(),
            json!({"contacts": [{"email": "ada@example.com"}]}),
        );
        assert!(
            parse_property_filter("meta.contacts.email=ada@example.com")
                .unwrap()
                .matches(&props)
        );
        assert!(
            !parse_property_filter("meta.contacts.email=nobody@example.com")
                .unwrap()
                .matches(&props)
        );
    }

    #[test]
    fn dot_path_literal_dotted_key_still_wins_over_sequence_descent() {
        let mut props = contacts_props();
        props.insert(
            "contacts.email".to_owned(),
            Value::String("flat".to_owned()),
        );
        assert!(
            parse_property_filter("contacts.email=flat")
                .unwrap()
                .matches(&props)
        );
        assert!(
            !parse_property_filter("contacts.email=ada@example.com")
                .unwrap()
                .matches(&props)
        );
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
    fn fields_empty_returns_default() {
        let f = Fields::parse(&[]).unwrap();
        // iter-252 default: properties, tags and title. Everything whose size
        // scales with the document body (sections, tasks, links) and the
        // whole-vault backlinks lookup are opt-in, via `--fields` or the
        // filter that implies them.
        assert!(f.properties && f.tags && f.title);
        assert!(!f.sections, "sections should be off by default");
        assert!(!f.links, "links should be off by default");
        assert!(!f.tasks, "tasks should be off by default");
        assert!(!f.backlinks, "backlinks should be off by default");
        assert!(
            !f.properties_typed,
            "properties-typed should be off by default"
        );
    }

    #[test]
    fn default_field_names_match_the_default_fields() {
        // The help/summary-line constant and `Fields::default()` are two
        // spellings of one decision; drift between them shows up as a text
        // summary that names fields the payload does not carry.
        let f = Fields::default();
        assert_eq!(
            DEFAULT_FIELD_NAMES,
            &[
                "file",
                "modified",
                "size",
                "lines",
                "title",
                "properties",
                "tags"
            ]
        );
        assert_eq!(f.properties, DEFAULT_FIELD_NAMES.contains(&"properties"));
        assert_eq!(f.tags, DEFAULT_FIELD_NAMES.contains(&"tags"));
        assert_eq!(f.title, DEFAULT_FIELD_NAMES.contains(&"title"));
        assert_eq!(f.sections, DEFAULT_FIELD_NAMES.contains(&"sections"));
        assert_eq!(f.links, DEFAULT_FIELD_NAMES.contains(&"links"));
        assert_eq!(f.tasks, DEFAULT_FIELD_NAMES.contains(&"tasks"));
        assert_eq!(f.backlinks, DEFAULT_FIELD_NAMES.contains(&"backlinks"));
        assert_eq!(f.modified, DEFAULT_FIELD_NAMES.contains(&"modified"));
        assert_eq!(f.size, DEFAULT_FIELD_NAMES.contains(&"size"));
        assert_eq!(f.lines, DEFAULT_FIELD_NAMES.contains(&"lines"));
    }

    #[test]
    fn an_explicit_selection_drops_the_default_set_members_it_omits() {
        // iter-254 (DEC-254): `modified`/`size`/`lines` are ordinary members of
        // the default set, so naming any field at all turns off the ones not
        // named. `file` is not a member — it is unconditional — but is accepted
        // as a name so `--fields file` can mean "just the paths".
        let f = Fields::parse(&["title".to_owned()]).unwrap();
        assert!(f.title);
        for (name, on) in [
            ("modified", f.modified),
            ("size", f.size),
            ("lines", f.lines),
            ("properties", f.properties),
            ("tags", f.tags),
        ] {
            assert!(!on, "{name} should be dropped by `--fields title`");
        }
    }

    #[test]
    fn the_droppable_default_members_can_be_asked_for_by_name() {
        let f = Fields::parse(&["size,lines".to_owned()]).unwrap();
        assert!(f.size && f.lines);
        assert!(!f.modified && !f.title && !f.properties && !f.tags);

        let f = Fields::parse(&["modified".to_owned()]).unwrap();
        assert!(f.modified);
        assert!(!f.size && !f.lines);
    }

    #[test]
    fn fields_file_is_accepted_and_selects_nothing_else() {
        let f = Fields::parse(&["file".to_owned()]).unwrap();
        assert!(!f.modified && !f.size && !f.lines);
        assert!(!f.title && !f.properties && !f.properties_typed && !f.tags);
        assert!(!f.sections && !f.tasks && !f.links && !f.backlinks);
    }

    #[test]
    fn fields_all_turns_on_the_droppable_default_members_too() {
        let f = Fields::parse(&["all".to_owned()]).unwrap();
        assert!(f.modified && f.size && f.lines);
        assert!(f.sections && f.tasks && f.links && f.backlinks && f.properties_typed);
    }

    #[test]
    fn the_unknown_field_error_states_the_projection_rule() {
        let err = Fields::parse(&["status".to_owned()])
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown field \"status\""), "{err}");
        assert!(
            err.contains("Without --fields: file, modified, size, lines, title, properties, tags."),
            "{err}"
        );
        assert!(
            err.contains("With --fields: exactly the named fields plus file"),
            "{err}"
        );
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
    // Bug 4: compare_property_values date-aware string sort
    // (try_as_iso_date unit tests live in sort.rs)
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
