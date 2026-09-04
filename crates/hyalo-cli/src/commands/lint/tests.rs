//! Unit tests for the lint command.
//!
//! Split out of `commands/lint.rs` in iteration 247: the module body is
//! unchanged, it just lives in its own file now. The
//! `clippy::items_after_test_module` allow the inline module used to carry is
//! gone with it -- the dispatch handler that followed it now has its own file.

use super::*;
use hyalo_core::is_iso8601_date;
use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};
use indexmap::IndexMap;
use serde_json::Value;
use std::collections::HashMap;

fn make_schema(
    default_required: &[&str],
    type_name: &str,
    type_required: &[&str],
    type_properties: HashMap<&str, PropertyConstraint>,
) -> SchemaConfig {
    let default = TypeSchema {
        required: default_required.iter().map(ToString::to_string).collect(),
        ..Default::default()
    };
    let mut props: HashMap<String, PropertyConstraint> = HashMap::new();
    for (k, v) in type_properties {
        props.insert(k.to_owned(), v);
    }
    let type_schema = TypeSchema {
        required: type_required.iter().map(ToString::to_string).collect(),
        properties: props,
        ..Default::default()
    };
    let mut types = HashMap::new();
    types.insert(type_name.to_owned(), type_schema);
    SchemaConfig {
        default,
        types,
        ..Default::default()
    }
}

// --- validate_schema_config (review round finding 2) ---

#[test]
fn validate_schema_config_none_when_valid() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".hyalo.toml"),
        "dir = \".\"\n[schema.types.task.properties.priority]\ntype = \"number\"\nminimum = 1\nmaximum = 5\n",
    )
    .unwrap();
    assert!(validate_schema_config(dir.path(), false).is_none());
    assert!(validate_schema_config(dir.path(), true).is_none());
}

#[test]
fn validate_schema_config_none_when_no_schema_block() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    assert!(validate_schema_config(dir.path(), false).is_none());
}

#[test]
fn validate_schema_config_none_when_no_config_file() {
    let dir = tempfile::tempdir().unwrap();
    assert!(validate_schema_config(dir.path(), false).is_none());
}

#[test]
fn validate_schema_config_warn_severity_without_strict() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".hyalo.toml"),
        "dir = \".\"\n[schema.types.task.properties.title]\ntype = \"string\"\npatterns = \".*\"\n",
    )
    .unwrap();
    let result = validate_schema_config(dir.path(), false).expect("malformed schema");
    assert_eq!(result.file, ".hyalo.toml");
    assert_eq!(result.violations.len(), 1);
    assert_eq!(result.violations[0].severity, Severity::Warn);
    assert_eq!(
        result.violations[0].kind,
        Some(VIOLATION_KIND_SCHEMA_MALFORMED)
    );
    assert!(result.violations[0].message.contains("patterns"));
}

#[test]
fn validate_schema_config_error_severity_under_strict() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".hyalo.toml"),
        "dir = \".\"\n[schema.types.task.properties.title]\ntype = \"string\"\npatterns = \".*\"\n",
    )
    .unwrap();
    let result = validate_schema_config(dir.path(), true).expect("malformed schema");
    assert_eq!(result.violations[0].severity, Severity::Error);
}

// --- is_iso8601_date ---

#[test]
fn valid_date() {
    assert!(is_iso8601_date("2026-04-13"));
}

#[test]
fn normalize_date_padding_and_calendar() {
    // Short month/day get zero-padded.
    assert_eq!(normalize_date("2026-4-9"), Some("2026-04-09".to_owned()));
    // Feb 29 is valid in leap years only.
    assert_eq!(normalize_date("2024-2-29"), Some("2024-02-29".to_owned()));
    assert_eq!(normalize_date("2023-2-29"), None);
    // Out-of-range days/months are rejected, not silently normalized.
    assert_eq!(normalize_date("2026-02-31"), None);
    assert_eq!(normalize_date("2026-04-31"), None);
    assert_eq!(normalize_date("2026-13-01"), None);
}

#[test]
fn invalid_date_format() {
    assert!(!is_iso8601_date("April 13"));
    assert!(!is_iso8601_date("13-04-2026"));
    assert!(!is_iso8601_date("2026/04/13"));
}

// Test helper: wraps `validate_constraint` with a throwaway regex cache.
// Returns the first violation (or None) for constraints that produce at most one.
fn vc(name: &str, value: &Value, c: &PropertyConstraint) -> Option<Violation> {
    let mut cache = HashMap::new();
    validate_constraint(name, value, c, &mut cache)
        .into_iter()
        .next()
}

// Test helper: returns all violations from `validate_constraint`.
fn vc_all(name: &str, value: &Value, c: &PropertyConstraint) -> Vec<Violation> {
    let mut cache = HashMap::new();
    validate_constraint(name, value, c, &mut cache)
}

// --- validate_constraint ---

#[test]
fn date_constraint_valid() {
    let v = vc(
        "date",
        &Value::String("2026-04-13".into()),
        &PropertyConstraint::Date,
    );
    assert!(v.is_none());
}

#[test]
fn date_constraint_invalid() {
    let v = vc(
        "date",
        &Value::String("April 13".into()),
        &PropertyConstraint::Date,
    );
    assert!(matches!(
        v,
        Some(Violation {
            severity: Severity::Error,
            ..
        })
    ));
}

#[test]
fn enum_constraint_valid() {
    let v = vc(
        "status",
        &Value::String("planned".into()),
        &PropertyConstraint::Enum {
            values: vec!["planned".into(), "done".into()],
        },
    );
    assert!(v.is_none());
}

#[test]
fn enum_constraint_invalid_with_suggestion() {
    let v = vc(
        "status",
        &Value::String("planed".into()),
        &PropertyConstraint::Enum {
            values: vec!["planned".into(), "done".into()],
        },
    );
    let viol = v.expect("expected violation");
    assert_eq!(viol.severity, Severity::Error);
    assert!(viol.message.contains("did you mean \"planned\""));
}

#[test]
fn number_constraint_valid() {
    let v = vc(
        "priority",
        &Value::Number(5.into()),
        &PropertyConstraint::Number {
            minimum: None,
            maximum: None,
        },
    );
    assert!(v.is_none());
}

#[test]
fn number_constraint_invalid() {
    let v = vc(
        "priority",
        &Value::String("five".into()),
        &PropertyConstraint::Number {
            minimum: None,
            maximum: None,
        },
    );
    assert!(matches!(
        v,
        Some(Violation {
            severity: Severity::Error,
            ..
        })
    ));
}

#[test]
fn number_constraint_minimum_violation() {
    let v = vc(
        "priority",
        &Value::Number(0.into()),
        &PropertyConstraint::Number {
            minimum: Some(1.0),
            maximum: None,
        },
    );
    let viol = v.expect("expected a minimum violation");
    assert_eq!(viol.severity, Severity::Error);
    assert!(viol.message.contains("minimum"));
}

#[test]
fn number_constraint_maximum_violation() {
    let v = vc(
        "priority",
        &Value::Number(99.into()),
        &PropertyConstraint::Number {
            minimum: None,
            maximum: Some(5.0),
        },
    );
    let viol = v.expect("expected a maximum violation");
    assert_eq!(viol.severity, Severity::Error);
    assert!(viol.message.contains("maximum"));
}

#[test]
fn number_constraint_within_min_max_range_is_valid() {
    let v = vc(
        "priority",
        &Value::Number(3.into()),
        &PropertyConstraint::Number {
            minimum: Some(1.0),
            maximum: Some(5.0),
        },
    );
    assert!(v.is_none());
}

#[test]
fn number_constraint_at_min_max_boundary_is_valid() {
    // Bounds are inclusive.
    assert!(
        vc(
            "priority",
            &Value::Number(1.into()),
            &PropertyConstraint::Number {
                minimum: Some(1.0),
                maximum: Some(5.0),
            },
        )
        .is_none()
    );
    assert!(
        vc(
            "priority",
            &Value::Number(5.into()),
            &PropertyConstraint::Number {
                minimum: Some(1.0),
                maximum: Some(5.0),
            },
        )
        .is_none()
    );
}

#[test]
fn boolean_constraint_valid() {
    let v = vc("draft", &Value::Bool(true), &PropertyConstraint::Boolean);
    assert!(v.is_none());
}

#[test]
fn boolean_constraint_invalid() {
    let v = vc(
        "draft",
        &Value::String("yes".into()),
        &PropertyConstraint::Boolean,
    );
    assert!(matches!(
        v,
        Some(Violation {
            severity: Severity::Error,
            ..
        })
    ));
}

#[test]
fn list_constraint_valid() {
    let v = vc("tags", &Value::Array(vec![]), &PropertyConstraint::List);
    assert!(v.is_none());
}

#[test]
fn list_constraint_invalid() {
    let v = vc(
        "tags",
        &Value::String("rust".into()),
        &PropertyConstraint::List,
    );
    assert!(matches!(
        v,
        Some(Violation {
            severity: Severity::Error,
            ..
        })
    ));
}

#[test]
fn string_pattern_constraint_valid() {
    let v = vc(
        "branch",
        &Value::String("iter-42/my-feature".into()),
        &PropertyConstraint::String {
            pattern: Some(r"^iter-\d+/".into()),
            min_length: None,
            max_length: None,
        },
    );
    assert!(v.is_none());
}

#[test]
fn string_pattern_constraint_invalid() {
    let v = vc(
        "branch",
        &Value::String("feature/my-branch".into()),
        &PropertyConstraint::String {
            pattern: Some(r"^iter-\d+/".into()),
            min_length: None,
            max_length: None,
        },
    );
    assert!(matches!(
        v,
        Some(Violation {
            severity: Severity::Error,
            ..
        })
    ));
}

// --- lint_file via a temp file ---

#[test]
fn lint_file_missing_required() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "---\ntitle: Hello\n---\nBody\n").unwrap();

    let schema = make_schema(&["title", "date"], "note", &[], HashMap::new());
    let result = lint_file(&path, "note.md", &schema, false).unwrap();
    // date is in default required, but only "title" is present.
    // No type -> warn about no type. date missing -> error.
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.severity == Severity::Error
                && v.message.contains("missing required property \"date\""))
    );
}

#[test]
fn missing_required_no_default_is_not_autofixable() {
    // mapl BUG-3: a missing required property with no schema default cannot
    // be synthesized by --fix, so its violation is tagged not-autofixable.
    let schema = make_schema(&["title", "date"], "note", &[], HashMap::new());
    let props: IndexMap<String, Value> = IndexMap::new(); // nothing present
    let violations = validate_properties("note.md", &props, &schema, false);
    let date_v = violations
        .iter()
        .find(|v| v.message.contains("missing required property \"date\""))
        .expect("date must be flagged");
    assert_eq!(
        date_v.kind,
        Some(VIOLATION_KIND_MISSING_REQUIRED_NO_DEFAULT),
        "no-default missing-required is tagged not-autofixable"
    );
}

#[test]
fn missing_required_with_default_is_autofixable() {
    // When the schema declares a default, --fix CAN synthesize the value, so
    // the violation is NOT tagged with the no-default kind.
    let mut default = TypeSchema {
        required: vec!["title".to_owned(), "status".to_owned()],
        ..Default::default()
    };
    default
        .defaults
        .insert("status".to_owned(), "draft".to_owned());
    let schema = SchemaConfig {
        default,
        ..Default::default()
    };
    let props: IndexMap<String, Value> = IndexMap::new();
    let violations = validate_properties("note.md", &props, &schema, false);
    let status_v = violations
        .iter()
        .find(|v| v.message.contains("missing required property \"status\""))
        .expect("status must be flagged");
    assert_ne!(
        status_v.kind,
        Some(VIOLATION_KIND_MISSING_REQUIRED_NO_DEFAULT),
        "a defaulted missing-required stays autofixable"
    );
}

#[test]
fn lint_file_no_type_warn() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "---\ntitle: Hello\n---\nBody\n").unwrap();

    let schema = make_schema(&["title"], "note", &[], HashMap::new());
    let result = lint_file(&path, "note.md", &schema, false).unwrap();
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.severity == Severity::Warn && v.message.contains("no 'type' property"))
    );
}

/// Build a `SchemaConfig` from a TOML `[schema]` fragment (as it would
/// appear in `.hyalo.toml`), for tests that exercise bind = typing.
fn schema_from_toml(fragment: &str) -> SchemaConfig {
    let val: toml::Value = toml::from_str(fragment).expect("valid schema fragment");
    let raw: hyalo_core::schema::RawSchemaConfig = val
        .get("schema")
        .and_then(|v| v.clone().try_into().ok())
        .expect("schema section present");
    SchemaConfig::try_from(raw).expect("valid schema")
}

#[test]
fn bind_typed_frontmatterless_file_satisfies_required_type() {
    // iter-172 bind = typing: a file whose type comes from a `[schema.bind]`
    // path binding must satisfy `[schema.default] required = ["type"]`
    // WITHOUT explicit `type:` frontmatter — a spec-valid frontmatter-less
    // SKILL.md / ADR must lint clean under composed profiles.
    let schema = schema_from_toml(
        "\
[schema.default]
required = [\"type\"]

[schema.types.skill]
required = [\"name\", \"description\"]

[[schema.bind]]
glob = \"**/SKILL.md\"
type = \"skill\"
",
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("SKILL.md");
    // A skill with the two required fields but NO explicit `type:`.
    std::fs::write(
        &path,
        "---\nname: my-skill\ndescription: does a thing\n---\nBody\n",
    )
    .unwrap();

    let result = lint_file(&path, "foo/SKILL.md", &schema, false).unwrap();
    assert!(
        !result
            .violations
            .iter()
            .any(|v| v.message.contains("missing required property \"type\"")),
        "bound file must not require explicit type: {:?}",
        result.violations
    );
    assert!(
        !result
            .violations
            .iter()
            .any(|v| v.message.contains("no 'type' property")),
        "bound file must not warn about missing type: {:?}",
        result.violations
    );
    // The skill's OWN required props are still enforced (both present here),
    // so the file is clean.
    assert!(
        result
            .violations
            .iter()
            .all(|v| v.severity != Severity::Error),
        "frontmatter-less bound skill lints clean: {:?}",
        result.violations
    );
}

#[test]
fn bind_typed_file_still_enforces_type_specific_required() {
    // Bind = typing drops only the `type` requirement; a bound file missing
    // its type's OWN required property still errors.
    let schema = schema_from_toml(
        "\
[schema.default]
required = [\"type\"]

[schema.types.skill]
required = [\"name\", \"description\"]

[[schema.bind]]
glob = \"**/SKILL.md\"
type = \"skill\"
",
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("SKILL.md");
    // Missing `description` (a skill-required field).
    std::fs::write(&path, "---\nname: my-skill\n---\nBody\n").unwrap();

    let result = lint_file(&path, "foo/SKILL.md", &schema, false).unwrap();
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.severity == Severity::Error
                && v.message
                    .contains("missing required property \"description\"")),
        "type-specific required still enforced: {:?}",
        result.violations
    );
}

#[test]
fn lint_file_no_violations_clean_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(
        &path,
        "---\ntitle: Hello\ntype: note\ntags:\n  - rust\n---\nBody\n",
    )
    .unwrap();

    let schema = make_schema(&["title"], "note", &[], HashMap::new());
    let result = lint_file(&path, "note.md", &schema, false).unwrap();
    assert!(result.violations.is_empty());
}

#[test]
fn lint_no_schema_no_violations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "---\ntitle: Hello\n---\nBody\n").unwrap();

    let schema = SchemaConfig::default();
    let (_, counts) = lint_extended_strict(&path, "note.md", &schema, false);
    assert_eq!(counts.errors, 0);
    assert_eq!(counts.warnings, 0);
}

// --- UX-3: comma-joined tag detection and fix ---

#[test]
fn lint_warns_on_comma_joined_tag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(
        &path,
        "---\ntitle: Hello\ntags:\n  - cli,ux\n  - rust\n---\nBody\n",
    )
    .unwrap();

    let schema = SchemaConfig::default();
    let result = lint_file(&path, "note.md", &schema, false).unwrap();
    let comma_warn = result
        .violations
        .iter()
        .find(|v| v.severity == Severity::Warn && v.message.contains("cli,ux"));
    assert!(
        comma_warn.is_some(),
        "expected a warning about comma-joined tag, got: {:#?}",
        result.violations
    );
    assert!(
        comma_warn.unwrap().message.contains("comma-joined"),
        "message should mention comma-joined"
    );
}

// --- Strict mode unit tests ---

fn make_schema_with_declared_prop() -> SchemaConfig {
    // Schema with a declared `note` type that has `title` as required and
    // `date` as a declared property, so any other property is "undeclared".
    make_schema(&["title"], "note", &[], HashMap::new())
}

/// Helper: run lint in extended mode on a single file.
fn lint_extended_strict(
    path: &std::path::Path,
    rel: &str,
    schema: &SchemaConfig,
    strict: bool,
) -> (crate::output::CommandOutcome, LintCounts) {
    let engine = hyalo_mdlint::HyaloLintEngine::create().unwrap();
    let md_config = hyalo_mdlint::LintConfig::default();
    let files = vec![(path.to_path_buf(), rel.to_owned())];
    let mut snapshot: Option<hyalo_core::index::SnapshotIndex> = None;
    let vault_dir = path.parent().unwrap();
    let mut opts = ExtLintOptions {
        fix: FixMode::Off,
        detailed: false,
        rule_filter: None,
        rule_prefix: None,
        max_per_rule: 100,
        max_files: 100,
        fix_rules: &[],
        snapshot_index: &mut snapshot,
        index_path: None,
        vault_dir,
        strict,
        okf_profile: false,
        madr_profile: false,
        skills_profile: false,
        changelog_profile: false,
        case_insensitive: false,
        link_lint_ctx: None,
        files_ignored: 0,
    };
    lint_files_extended(&files, schema, &engine, &md_config, &mut opts).unwrap()
}

/// In strict mode the "no 'type' property" warning becomes an error.
#[test]
fn strict_mode_promotes_no_type_to_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("no_type.md");
    std::fs::write(&path, "---\ntitle: Hello\n---\nBody\n").unwrap();

    let schema = make_schema_with_declared_prop();
    let (_, counts) = lint_extended_strict(&path, "no_type.md", &schema, true);
    assert!(counts.errors > 0, "strict mode: no-type should be an error");
}

/// Without strict mode the "no 'type' property" is still a warning.
#[test]
fn non_strict_no_type_stays_warn() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("no_type.md");
    std::fs::write(&path, "---\ntitle: Hello\n---\nBody\n").unwrap();

    let schema = make_schema_with_declared_prop();
    let (_, counts) = lint_extended_strict(&path, "no_type.md", &schema, false);
    assert_eq!(
        counts.errors, 0,
        "non-strict: no-type should remain a warning"
    );
    assert!(counts.warnings > 0, "non-strict: warnings expected");
}

/// In strict mode, undeclared properties become errors.
#[test]
fn strict_mode_promotes_undeclared_property_to_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("undeclared.md");
    // `type: note` present (avoids no-type warning), but `unknown_prop` is not
    // declared in the note schema's `properties` map.
    std::fs::write(
        &path,
        "---\ntitle: Hello\ntype: note\nunknown_prop: oops\n---\nBody\n",
    )
    .unwrap();

    let schema = {
        // Build a schema where `note` has declared `properties` so
        // the undeclared-property path fires.
        use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};
        let mut schema = SchemaConfig::default();
        let mut ts = TypeSchema::default();
        ts.required.push("title".to_owned());
        ts.properties.insert(
            "title".to_owned(),
            PropertyConstraint::String {
                pattern: None,
                min_length: None,
                max_length: None,
            },
        );
        schema.types.insert("note".to_owned(), ts);
        schema
    };

    let (_, counts) = lint_extended_strict(&path, "undeclared.md", &schema, true);
    assert!(
        counts.errors > 0,
        "strict: undeclared prop should be an error"
    );
}

/// A required property whose value is an empty `[]` is an error: an empty
/// list is semantically equivalent to absent for a required field. The
/// rule is value-shape driven and fires whether or not the property has a
/// List constraint declared.
#[test]
fn required_property_empty_array_is_error() {
    use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty_tags.md");
    std::fs::write(
        &path,
        "---\ntitle: Hello\ntype: note\ntags: []\n---\nBody\n",
    )
    .unwrap();

    let schema = {
        let mut schema = SchemaConfig::default();
        let mut ts = TypeSchema::default();
        ts.required.push("title".to_owned());
        ts.required.push("tags".to_owned());
        ts.properties
            .insert("tags".to_owned(), PropertyConstraint::List);
        schema.types.insert("note".to_owned(), ts);
        schema
    };

    let (outcome, counts) = lint_extended_strict(&path, "empty_tags.md", &schema, false);
    assert!(counts.errors > 0, "empty required list should error");
    let body = match outcome {
        crate::output::CommandOutcome::Success { output, .. } => output,
        other => panic!("expected Success outcome, got: {other:?}"),
    };
    assert!(
        body.contains("must not be empty") && body.contains("tags"),
        "expected empty-required error mentioning tags in output, got: {body}"
    );
}

/// A required property explicitly set to YAML null (`tags: ~`) is also
/// treated as empty — null carries no information, same as an absent key.
/// Without this, a typo or stripped value silently passes the required gate.
#[test]
fn required_property_null_value_is_error() {
    use hyalo_core::schema::{SchemaConfig, TypeSchema};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("null_tags.md");
    std::fs::write(&path, "---\ntitle: Hello\ntype: note\ntags: ~\n---\nBody\n").unwrap();

    let schema = {
        let mut schema = SchemaConfig::default();
        let mut ts = TypeSchema::default();
        ts.required.push("tags".to_owned());
        schema.types.insert("note".to_owned(), ts);
        schema
    };

    let (outcome, counts) = lint_extended_strict(&path, "null_tags.md", &schema, false);
    assert!(counts.errors > 0, "null required property should error");
    let body = match outcome {
        crate::output::CommandOutcome::Success { output, .. } => output,
        other => panic!("expected Success outcome, got: {other:?}"),
    };
    assert!(
        body.contains("must not be empty") && body.contains("tags"),
        "expected empty-required error mentioning tags in output, got: {body}"
    );
}

/// A required atomic-typed property satisfied by any value (including a
/// zero-ish one like `0` or `""`) is *not* an error from the
/// non-empty-list check — only sequence-typed required properties get the
/// extra emptiness gate.
#[test]
fn required_non_list_property_is_unaffected_by_empty_check() {
    use hyalo_core::schema::{PropertyConstraint, SchemaConfig, TypeSchema};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty_title.md");
    std::fs::write(&path, "---\ntitle: \"\"\ntype: note\n---\nBody\n").unwrap();

    let schema = {
        let mut schema = SchemaConfig::default();
        let mut ts = TypeSchema::default();
        ts.required.push("title".to_owned());
        ts.properties.insert(
            "title".to_owned(),
            PropertyConstraint::String {
                pattern: None,
                min_length: None,
                max_length: None,
            },
        );
        schema.types.insert("note".to_owned(), ts);
        schema
    };

    let (_, counts) = lint_extended_strict(&path, "empty_title.md", &schema, false);
    assert_eq!(
        counts.errors, 0,
        "empty required string is not flagged here"
    );
}

/// A file with `type` but no `tags` produces zero violations against a
/// schema that doesn't require `tags`. The previously-hardcoded "no tags
/// defined" warning was removed in iter-156 — opt in via `required` if you
/// want enforcement.
#[test]
fn missing_tags_is_not_a_violation_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("no_tags.md");
    std::fs::write(&path, "---\ntitle: Hello\ntype: note\n---\nBody\n").unwrap();

    let schema = make_schema_with_declared_prop();
    let (_, counts) = lint_extended_strict(&path, "no_tags.md", &schema, false);
    assert_eq!(counts.errors, 0, "missing tags should not be an error");
    assert_eq!(counts.warnings, 0, "missing tags should not warn");
}

#[test]
fn lint_fix_splits_comma_joined_tags() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(
        &path,
        "---\ntitle: Hello\ntags:\n  - cli,ux\n  - rust\n---\nBody\n",
    )
    .unwrap();

    let schema = SchemaConfig::default();
    let engine = hyalo_mdlint::HyaloLintEngine::create().unwrap();
    let md_config = hyalo_mdlint::LintConfig::default();
    let files = vec![(path.clone(), "note.md".to_owned())];
    let mut snapshot: Option<hyalo_core::index::SnapshotIndex> = None;
    let mut opts = ExtLintOptions {
        fix: FixMode::Apply,
        detailed: false,
        rule_filter: None,
        rule_prefix: None,
        max_per_rule: 100,
        max_files: 100,
        fix_rules: &[],
        snapshot_index: &mut snapshot,
        index_path: None,
        vault_dir: dir.path(),
        strict: false,
        okf_profile: false,
        madr_profile: false,
        skills_profile: false,
        changelog_profile: false,
        case_insensitive: false,
        link_lint_ctx: None,
        files_ignored: 0,
    };
    let (_, counts) = lint_files_extended(&files, &schema, &engine, &md_config, &mut opts).unwrap();

    // After fix, the comma-joined tag warning should be gone.
    assert_eq!(counts.warnings, 0, "comma-tag warning should be fixed");

    let content = std::fs::read_to_string(&path).unwrap();
    // Both parts of the split tag should be separate items.
    assert!(content.contains("- cli"), "expected 'cli' as separate tag");
    assert!(content.contains("- ux"), "expected 'ux' as separate tag");
    // The original comma-joined form must be gone.
    assert!(
        !content.contains("cli,ux"),
        "comma-joined tag should be removed"
    );
}

// ---------------------------------------------------------------------------
// object-list tests (iteration 268 / DEC-287)
// ---------------------------------------------------------------------------

/// The motivating constraint from the iteration plan.
fn sources_constraint() -> PropertyConstraint {
    let mut key_patterns = indexmap::IndexMap::new();
    key_patterns.insert(
        "ref".to_owned(),
        "^(github|confluence|jira|slack|person|runtime|decision):|^https?://".to_owned(),
    );
    key_patterns.insert("commit".to_owned(), "^[0-9a-f]{7,40}$".to_owned());
    key_patterns.insert("read".to_owned(), r"^\d{4}-\d{2}-\d{2}$".to_owned());
    PropertyConstraint::ObjectList {
        required_keys: vec!["ref".to_owned()],
        allowed_keys: Some(
            ["ref", "commit", "version", "updated", "read"]
                .map(str::to_owned)
                .to_vec(),
        ),
        key_patterns,
    }
}

/// Build a single `{key: value, ...}` item.
fn item(pairs: &[(&str, Value)]) -> Value {
    let mut map = serde_json::Map::new();
    for (k, v) in pairs {
        map.insert((*k).to_owned(), v.clone());
    }
    Value::Object(map)
}

fn s(v: &str) -> Value {
    Value::String(v.to_owned())
}

#[test]
fn object_list_accepts_valid_items() {
    let violations = vc_all(
        "sources",
        &Value::Array(vec![
            item(&[("ref", s("github:comparis/neon")), ("commit", s("3c9e0f2"))]),
            item(&[
                ("ref", s("https://example.org/post")),
                ("read", s("2026-09-01")),
            ]),
        ]),
        &sources_constraint(),
    );
    assert!(
        violations.is_empty(),
        "valid object list should pass, got: {:?}",
        violations.iter().map(|v| &v.message).collect::<Vec<_>>()
    );
}

#[test]
fn object_list_string_item_gets_fix_it_hint() {
    let violations = vc_all(
        "sources",
        &Value::Array(vec![s("https://example.org/post")]),
        &sources_constraint(),
    );
    assert_eq!(violations.len(), 1, "{violations:?}");
    let msg = &violations[0].message;
    assert!(msg.contains("item 0"), "{msg}");
    assert!(msg.contains("must be a map, not a string"), "{msg}");
    assert!(
        msg.contains("did you mean `- ref: https://example.org/post`?"),
        "expected the fix-it text, got: {msg}"
    );
    assert_eq!(violations[0].severity, Severity::Error);
}

#[test]
fn object_list_unknown_key_lists_allowed_keys() {
    let violations = vc_all(
        "sources",
        &Value::Array(vec![item(&[
            ("ref", s("github:comparis/neon")),
            ("rev", s("3c9e0f2")),
        ])]),
        &sources_constraint(),
    );
    assert_eq!(violations.len(), 1, "{violations:?}");
    let msg = &violations[0].message;
    assert!(msg.contains("item 0"), "{msg}");
    assert!(msg.contains(r#"unknown key "rev""#), "{msg}");
    assert!(
        msg.contains("allowed: ref, commit, version, updated, read"),
        "{msg}"
    );
}

#[test]
fn object_list_key_pattern_mismatch_names_key_and_pattern() {
    let violations = vc_all(
        "sources",
        &Value::Array(vec![item(&[
            ("ref", s("github:comparis/neon")),
            ("commit", s("zzz")),
        ])]),
        &sources_constraint(),
    );
    assert_eq!(violations.len(), 1, "{violations:?}");
    let msg = &violations[0].message;
    assert!(msg.contains("item 0"), "{msg}");
    assert!(msg.contains(r#"key "commit""#), "{msg}");
    assert!(msg.contains("^[0-9a-f]{7,40}$"), "{msg}");
}

#[test]
fn object_list_non_scalar_under_pattern_key_errors() {
    let violations = vc_all(
        "sources",
        &Value::Array(vec![item(&[("ref", Value::Array(vec![s("a"), s("b")]))])]),
        &sources_constraint(),
    );
    assert_eq!(violations.len(), 1, "{violations:?}");
    let msg = &violations[0].message;
    assert!(msg.contains(r#"key "ref" must be a scalar"#), "{msg}");
    assert!(msg.contains("got a list"), "{msg}");
}

#[test]
fn object_list_missing_required_key_errors() {
    let violations = vc_all(
        "sources",
        &Value::Array(vec![item(&[("commit", s("3c9e0f2"))])]),
        &sources_constraint(),
    );
    assert_eq!(violations.len(), 1, "{violations:?}");
    let msg = &violations[0].message;
    assert!(msg.contains("item 0"), "{msg}");
    assert!(msg.contains(r#"missing required key "ref""#), "{msg}");
}

#[test]
fn object_list_reports_every_bad_item() {
    let violations = vc_all(
        "sources",
        &Value::Array(vec![
            item(&[("ref", s("github:comparis/neon"))]), // valid
            s("plain string"),                           // not a map
            item(&[("commit", s("3c9e0f2"))]),           // missing ref
            item(&[("ref", s("github:a/b")), ("rev", s("x"))]), // unknown key
        ]),
        &sources_constraint(),
    );
    assert_eq!(
        violations.len(),
        3,
        "expected one violation per bad item, got: {:?}",
        violations.iter().map(|v| &v.message).collect::<Vec<_>>()
    );
    assert!(violations[0].message.contains("item 1"));
    assert!(violations[1].message.contains("item 2"));
    assert!(violations[2].message.contains("item 3"));
}

#[test]
fn object_list_vacuous_on_empty_list() {
    let violations = vc_all("sources", &Value::Array(vec![]), &sources_constraint());
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn object_list_scalar_value_is_one_error() {
    let violations = vc_all("sources", &s("github:a/b"), &sources_constraint());
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].message.contains("expected a list of maps"),
        "{}",
        violations[0].message
    );
}

#[test]
fn object_list_number_item_has_no_fix_it_hint() {
    let violations = vc_all(
        "sources",
        &Value::Array(vec![Value::Number(42.into())]),
        &sources_constraint(),
    );
    assert_eq!(violations.len(), 1, "{violations:?}");
    let msg = &violations[0].message;
    assert!(msg.contains("must be a map, not a number"), "{msg}");
    assert!(
        !msg.contains("did you mean"),
        "the fix-it hint is only for string items, got: {msg}"
    );
}

#[test]
fn object_list_without_allowed_keys_permits_extras() {
    let constraint = PropertyConstraint::ObjectList {
        required_keys: vec!["ref".to_owned()],
        allowed_keys: None,
        key_patterns: indexmap::IndexMap::new(),
    };
    let violations = vc_all(
        "sources",
        &Value::Array(vec![item(&[
            ("ref", s("github:a/b")),
            ("anything", s("goes")),
        ])]),
        &constraint,
    );
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn object_list_matches_non_string_scalars_as_yaml_text() {
    // A number, bool or date value is matched against its text form.
    let mut key_patterns = indexmap::IndexMap::new();
    key_patterns.insert("n".to_owned(), "^4[0-9]$".to_owned());
    key_patterns.insert("b".to_owned(), "^true$".to_owned());
    let constraint = PropertyConstraint::ObjectList {
        required_keys: Vec::new(),
        allowed_keys: None,
        key_patterns,
    };
    let ok = vc_all(
        "meta",
        &Value::Array(vec![item(&[
            ("n", Value::Number(42.into())),
            ("b", Value::Bool(true)),
        ])]),
        &constraint,
    );
    assert!(ok.is_empty(), "{ok:?}");
    let bad = vc_all(
        "meta",
        &Value::Array(vec![item(&[("n", Value::Number(7.into()))])]),
        &constraint,
    );
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert!(bad[0].message.contains(r#"key "n" value "7""#), "{bad:?}");
}

#[test]
fn object_list_null_under_pattern_key_is_not_a_scalar() {
    let violations = vc_all(
        "sources",
        &Value::Array(vec![item(&[("ref", Value::Null)])]),
        &sources_constraint(),
    );
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].message.contains("must be a scalar"),
        "{}",
        violations[0].message
    );
}

#[test]
fn object_list_violations_are_not_autofixable() {
    let violations = vc_all(
        "sources",
        &Value::Array(vec![s("plain string")]),
        &sources_constraint(),
    );
    assert_eq!(
        violations[0].kind,
        Some(hyalo_mdlint::schema::VIOLATION_KIND_CONSTRAINT_VIOLATION),
        "object-list violations must carry the constraint-violation kind so the \
         SCHEMA group reports autofixable: false"
    );
}

// ---------------------------------------------------------------------------
// item_pattern tests
// ---------------------------------------------------------------------------

#[test]
fn item_pattern_validates_list_items() {
    // First item matches, second does not.
    let constraint = PropertyConstraint::StringList {
        item_pattern: Some(r"^[a-z]+$".to_owned()),
    };
    let v = vc(
        "tags",
        &Value::Array(vec![
            Value::String("rust".into()),
            Value::String("Rust123".into()), // uppercase — should fail
        ]),
        &constraint,
    );
    let viol = v.expect("expected a violation");
    assert_eq!(viol.severity, Severity::Error);
    assert!(
        viol.message.contains("item 1"),
        "expected item index in message, got: {}",
        viol.message
    );
    assert!(
        viol.message.contains(r"^[a-z]+$"),
        "expected pattern in message, got: {}",
        viol.message
    );
}

#[test]
fn item_pattern_vacuous_on_empty_list() {
    let constraint = PropertyConstraint::StringList {
        item_pattern: Some(r"^[a-z]+$".to_owned()),
    };
    let v = vc("tags", &Value::Array(vec![]), &constraint);
    assert!(v.is_none(), "empty list should produce no violations");
}

#[test]
fn item_pattern_non_string_item_errors() {
    let constraint = PropertyConstraint::StringList { item_pattern: None };
    let v = vc(
        "tags",
        &Value::Array(vec![Value::Number(42.into())]),
        &constraint,
    );
    let viol = v.expect("expected a violation");
    assert_eq!(viol.severity, Severity::Error);
    assert!(
        viol.message.contains("item 0"),
        "expected item index in message, got: {}",
        viol.message
    );
    assert!(
        viol.message.contains("expected string"),
        "expected type error message, got: {}",
        viol.message
    );
}

#[test]
fn item_pattern_reports_all_violations() {
    // Three items: first valid, second and third fail the pattern.
    let constraint = PropertyConstraint::StringList {
        item_pattern: Some(r"^[a-z][a-z0-9-]*$".to_owned()),
    };
    let violations = vc_all(
        "tags",
        &Value::Array(vec![
            Value::String("good-tag".into()),
            Value::String("Bad".into()),  // uppercase start — fails
            Value::String("1bad".into()), // digit start — fails
            Value::String("also-good".into()),
            Value::String("Bar".into()), // uppercase start — fails
        ]),
        &constraint,
    );
    assert_eq!(
        violations.len(),
        3,
        "expected 3 violations, got: {:?}",
        violations.iter().map(|v| &v.message).collect::<Vec<_>>()
    );
    assert!(
        violations[0].message.contains("item 1"),
        "first violation should reference item 1"
    );
    assert!(
        violations[1].message.contains("item 2"),
        "second violation should reference item 2"
    );
    assert!(
        violations[2].message.contains("item 4"),
        "third violation should reference item 4"
    );
    for v in &violations {
        assert_eq!(v.severity, Severity::Error);
    }
}

#[test]
fn item_pattern_multiple_non_string_items_all_reported() {
    // Without item_pattern, multiple non-string items should all be reported.
    let constraint = PropertyConstraint::StringList { item_pattern: None };
    let violations = vc_all(
        "tags",
        &Value::Array(vec![
            Value::String("ok".into()),
            Value::Number(1.into()),
            Value::Bool(true),
        ]),
        &constraint,
    );
    assert_eq!(
        violations.len(),
        2,
        "expected 2 violations for the two non-string items, got: {:?}",
        violations.iter().map(|v| &v.message).collect::<Vec<_>>()
    );
    assert!(violations[0].message.contains("item 1"));
    assert!(violations[1].message.contains("item 2"));
}

// ---------------------------------------------------------------------------
// required_sections tests
// ---------------------------------------------------------------------------

fn make_schema_with_sections(sections: Vec<String>) -> SchemaConfig {
    let type_schema = TypeSchema {
        required_sections: sections,
        ..Default::default()
    };
    let mut types = HashMap::new();
    types.insert("doc".to_owned(), type_schema);
    SchemaConfig {
        default: TypeSchema::default(),
        types,
        ..Default::default()
    }
}

#[test]
fn required_sections_pass_when_all_present() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    std::fs::write(
        &path,
        "---\ntype: doc\ntitle: Hello\n---\n# Goal\n\nSome text.\n\n## Tasks\n\nDo stuff.\n",
    )
    .unwrap();

    let schema = make_schema_with_sections(vec!["# Goal".to_owned(), "## Tasks".to_owned()]);
    let result = lint_file(&path, "doc.md", &schema, false).unwrap();
    let section_viols: Vec<_> = result
        .violations
        .iter()
        .filter(|v| v.message.contains("missing required section"))
        .collect();
    assert!(
        section_viols.is_empty(),
        "expected no section violations, got: {section_viols:?}"
    );
}

#[test]
fn required_sections_violation_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    std::fs::write(
        &path,
        "---\ntype: doc\ntitle: Hello\n---\n# Goal\n\nSome text.\n",
    )
    .unwrap();

    let schema = make_schema_with_sections(vec!["# Goal".to_owned(), "## Tasks".to_owned()]);
    let result = lint_file(&path, "doc.md", &schema, false).unwrap();
    let section_viols: Vec<_> = result
        .violations
        .iter()
        .filter(|v| v.message.contains("missing required section"))
        .collect();
    assert_eq!(
        section_viols.len(),
        1,
        "expected exactly one missing-section violation"
    );
    assert!(
        section_viols[0].message.contains("## Tasks"),
        "expected '## Tasks' in message, got: {}",
        section_viols[0].message
    );
}

#[test]
fn required_sections_order_significant() {
    // Body has both headings but in reverse order.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    std::fs::write(
        &path,
        "---\ntype: doc\ntitle: Hello\n---\n## Tasks\n\nDo stuff.\n\n# Goal\n\nSome text.\n",
    )
    .unwrap();

    // Required: Goal then Tasks (but in body: Tasks then Goal).
    let schema = make_schema_with_sections(vec!["# Goal".to_owned(), "## Tasks".to_owned()]);
    let result = lint_file(&path, "doc.md", &schema, false).unwrap();
    let section_viols: Vec<_> = result
        .violations
        .iter()
        .filter(|v| v.message.contains("missing required section"))
        .collect();
    // "# Goal" is never matched because its cursor position (after ## Tasks) is after where it appears.
    assert!(
        !section_viols.is_empty(),
        "expected section violation when order is wrong"
    );
}

#[test]
fn required_sections_extras_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.md");
    std::fs::write(
        &path,
        "---\ntype: doc\ntitle: Hello\n---\n# Goal\n\n## Extra One\n\nText.\n\n## Tasks\n\nDo stuff.\n\n## Extra Two\n\nMore.\n",
    )
    .unwrap();

    let schema = make_schema_with_sections(vec!["# Goal".to_owned(), "## Tasks".to_owned()]);
    let result = lint_file(&path, "doc.md", &schema, false).unwrap();
    let section_viols: Vec<_> = result
        .violations
        .iter()
        .filter(|v| v.message.contains("missing required section"))
        .collect();
    assert!(
        section_viols.is_empty(),
        "extra headings should not cause violations, got: {section_viols:?}"
    );
}

// --- apply_body_fixes ---

fn mk_diag(
    rule_id: &str,
    severity: hyalo_mdlint::DiagSeverity,
    start: usize,
    end: usize,
    replacement: &str,
) -> hyalo_mdlint::Diagnostic {
    hyalo_mdlint::Diagnostic {
        rule_id: rule_id.to_owned(),
        rule_name: rule_id.to_owned(),
        message: String::new(),
        line: 1,
        column: 1,
        severity,
        fix: Some(hyalo_mdlint::DiagFix {
            description: String::new(),
            start,
            end,
            replacement: replacement.to_owned(),
        }),
    }
}

#[test]
fn apply_body_fixes_error_wins_overlap_regardless_of_offset() {
    // Two fixes over the same range: a Warn fix that would sort first by
    // descending-offset (its start is >= the Error fix's start) must not
    // beat the overlapping Error fix.
    let warn = mk_diag("MD009", hyalo_mdlint::DiagSeverity::Warn, 0, 10, "warn-fix");
    let error = mk_diag(
        "HYALO001",
        hyalo_mdlint::DiagSeverity::Error,
        0,
        10,
        "error-fix",
    );
    let (result, outcomes) = apply_body_fixes("0123456789", &[&warn, &error]);
    assert_eq!(result, "error-fix");
    assert!(matches!(outcomes[1], FixOutcome::Applied));
    assert!(matches!(outcomes[0], FixOutcome::Conflict { .. }));
}

#[test]
fn apply_body_fixes_no_op_replacement_is_not_applied() {
    let noop = mk_diag("MD047", hyalo_mdlint::DiagSeverity::Warn, 4, 5, "\n");
    let (result, outcomes) = apply_body_fixes("body\n", &[&noop]);
    assert_eq!(result, "body\n", "content must not change");
    assert!(
        matches!(outcomes[0], FixOutcome::NoFix),
        "byte-for-byte no-op must not be reported as Applied: {:?}",
        outcomes[0]
    );
}

#[test]
fn apply_body_fixes_rejects_inverted_range() {
    // start > end: `body[start..end]` would panic. The fix must be
    // reported as a conflict and the body left untouched (iter-191).
    let inverted = mk_diag("MD009", hyalo_mdlint::DiagSeverity::Warn, 4, 1, "boom");
    let (result, outcomes) = apply_body_fixes("0123456789", &[&inverted]);
    assert_eq!(result, "0123456789", "content must not change");
    assert!(
        matches!(outcomes[0], FixOutcome::Conflict { .. }),
        "inverted range must be a Conflict, got: {:?}",
        outcomes[0]
    );
}

#[test]
fn apply_body_fixes_rejects_non_char_boundary() {
    // "é" is two bytes; end == 1 lands mid-character, which `str`
    // indexing rejects with a panic. In-bounds, so the old
    // `end > body.len()` check missed it entirely (iter-191).
    let body = "é abc";
    let mid_char = mk_diag("MD009", hyalo_mdlint::DiagSeverity::Warn, 0, 1, "x");
    let (result, outcomes) = apply_body_fixes(body, &[&mid_char]);
    assert_eq!(result, body, "content must not change");
    assert!(
        matches!(outcomes[0], FixOutcome::Conflict { .. }),
        "mid-char-boundary range must be a Conflict, got: {:?}",
        outcomes[0]
    );
}

#[test]
fn apply_body_fixes_non_overlapping_fixes_both_apply() {
    let a = mk_diag("MD009", hyalo_mdlint::DiagSeverity::Warn, 0, 1, "A");
    let b = mk_diag("MD009", hyalo_mdlint::DiagSeverity::Warn, 2, 3, "B");
    let (result, outcomes) = apply_body_fixes("xyz", &[&a, &b]);
    assert_eq!(result, "AyB");
    assert!(matches!(outcomes[0], FixOutcome::Applied));
    assert!(matches!(outcomes[1], FixOutcome::Applied));
}

#[test]
fn apply_body_fixes_adjacent_touching_ranges_both_apply() {
    // end == start is NOT an overlap (strict inequalities in the
    // conflict check): [0,2) and [2,4) must both apply.
    let a = mk_diag("MD009", hyalo_mdlint::DiagSeverity::Warn, 0, 2, "AA");
    let b = mk_diag("MD009", hyalo_mdlint::DiagSeverity::Warn, 2, 4, "BB");
    let (result, outcomes) = apply_body_fixes("wxyz", &[&a, &b]);
    assert_eq!(result, "AABB");
    assert!(matches!(outcomes[0], FixOutcome::Applied));
    assert!(matches!(outcomes[1], FixOutcome::Applied));
}

// --- find_body_start line-ending handling ---

#[test]
fn find_body_start_skips_crlf_after_closing_delimiter() {
    let content = "---\r\ntitle: T\r\n---\r\nbody line\r\n";
    let start = find_body_start(content);
    assert_eq!(
        &content[start..],
        "body line\r\n",
        "body must not start with a stray CR on CRLF files"
    );
}

#[test]
fn find_body_start_bom_prefixed_frontmatter_is_split() {
    let content = "\u{feff}---\ntitle: T\n---\nbody line\n";
    let start = find_body_start(content);
    assert_eq!(&content[start..], "body line\n");
}

// --- group_severity ---

fn iv(severity: &str) -> InternalViolation {
    InternalViolation {
        line: 1,
        column: 1,
        message: String::new(),
        severity: severity.to_owned(),
        fix: None,
        fixed: false,
        autofixable: None,
    }
}

#[test]
fn group_severity_is_error_when_any_violation_is_error() {
    let violations = vec![iv("warn"), iv("error"), iv("warn")];
    assert_eq!(group_severity(&violations), "error");
}

#[test]
fn group_severity_is_warn_when_all_warn() {
    let violations = vec![iv("warn"), iv("warn")];
    assert_eq!(group_severity(&violations), "warn");
}

#[test]
fn group_severity_empty_defaults_to_warn() {
    assert_eq!(group_severity(&[]), "warn");
}
