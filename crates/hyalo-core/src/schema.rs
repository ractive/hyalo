/// Schema data model for document type validation.
///
/// Parsed from `[schema.*]` sections in `.hyalo.toml`:
///
/// ```toml
/// [schema.default]
/// required = ["title"]
///
/// [schema.types.iteration]
/// required = ["title", "date", "status", "branch", "tags"]
/// filename-template = "iterations/iteration-{n}-{slug}.md"
///
/// [schema.types.iteration.defaults]
/// status = "planned"
/// date = "$today"
///
/// [schema.types.iteration.properties.status]
/// type = "enum"
/// values = ["planned", "in-progress", "completed"]
///
/// [schema.types.iteration.properties.branch]
/// type = "string"
/// pattern = "^iter-\\d+/"
/// ```
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

/// The fully-resolved schema configuration for a vault.
///
/// Constructed from the `[schema]` section of `.hyalo.toml`.  When the
/// section is absent, `SchemaConfig::default()` (which has no types and no
/// required properties) represents "no validation".
#[derive(Debug, Clone, Default)]
pub struct SchemaConfig {
    /// Global defaults applied to every file, regardless of type.
    pub default: TypeSchema,
    /// Per-type schemas, keyed by the value of the `type` frontmatter property.
    pub types: HashMap<String, TypeSchema>,
}

impl SchemaConfig {
    /// Returns `true` when no schema configuration was provided at all.
    ///
    /// When this returns `true`, `hyalo lint` produces zero violations and
    /// exits 0 immediately.
    pub fn is_empty(&self) -> bool {
        self.default.required.is_empty()
            && self.default.properties.is_empty()
            && self.types.is_empty()
    }

    /// Merge the default schema with a named type schema.
    ///
    /// - `required` lists are combined (type extends default, no duplicates).
    /// - `properties` are merged: type-specific constraints override defaults
    ///   for the same property name; defaults fill in any gaps.
    /// - `filename_template` and `defaults` come from the type schema only.
    pub fn merged_schema_for_type(&self, type_name: &str) -> TypeSchema {
        let type_schema = self.types.get(type_name);
        let mut required: Vec<String> = self.default.required.clone();
        // Extend with type-specific required fields, deduplicated.
        if let Some(ts) = type_schema {
            for r in &ts.required {
                if !required.contains(r) {
                    required.push(r.clone());
                }
            }
        }

        // Merge properties: defaults first, then type overrides.
        let mut properties = self.default.properties.clone();
        if let Some(ts) = type_schema {
            for (k, v) in &ts.properties {
                properties.insert(k.clone(), v.clone());
            }
        }

        TypeSchema {
            required,
            filename_template: type_schema.and_then(|ts| ts.filename_template.clone()),
            defaults: type_schema
                .map(|ts| ts.defaults.clone())
                .unwrap_or_default(),
            properties,
        }
    }

    /// Returns the default-only schema (used for files without a `type` property).
    pub fn default_schema(&self) -> &TypeSchema {
        &self.default
    }
}

/// Schema definition for a single document type (or the global default).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TypeSchema {
    /// Property keys that must be present in every file of this type.
    #[serde(default)]
    pub required: Vec<String>,

    /// Optional filename template for new files of this type.
    /// Tokens: `{n}` (sequence number), `{slug}` (title-derived slug).
    #[serde(rename = "filename-template")]
    pub filename_template: Option<String>,

    /// Default values used when creating new files; `$today` expands to YYYY-MM-DD.
    #[serde(default)]
    pub defaults: HashMap<String, String>,

    /// Per-property type constraints keyed by property name.
    #[serde(default)]
    pub properties: HashMap<String, PropertyConstraint>,
}

/// Expand a schema-default template into a concrete value.
///
/// Currently the only supported token is `$today`, which expands to the
/// current UTC date in YYYY-MM-DD format.
pub fn expand_default(raw: &str) -> String {
    if raw == "$today" {
        return today_iso8601();
    }
    raw.to_owned()
}

/// Current UTC date in YYYY-MM-DD format.
pub fn today_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Safety: secs / 86_400 fits well within i64 for any date in the next few million years.
    #[allow(clippy::cast_possible_wrap)]
    let (y, m, d) = days_to_ymd((secs / 86_400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Convert days since the Unix epoch to a `(year, month, day)` tuple in the
/// proleptic Gregorian calendar, via Howard Hinnant's civil_from_days.
///
/// All casts here are safe for any date representable in the Gregorian calendar
/// on a 64-bit system (the algorithm is bounded to reasonable calendar ranges).
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation
)]
fn days_to_ymd(days_since_epoch: i64) -> (i32, u32, u32) {
    // Shift so that day 0 == 0000-03-01 (era-based algorithm).
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Constraint on a single frontmatter property.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PropertyConstraint {
    /// Any string value; optional regex pattern validation.
    String {
        /// Optional regex that the value must match.
        pattern: Option<String>,
    },
    /// ISO 8601 date (YYYY-MM-DD).
    Date,
    /// Integer or floating-point number.
    Number,
    /// Boolean (`true` / `false`).
    Boolean,
    /// YAML sequence / list.
    List,
    /// String restricted to one of the given `values`.
    Enum {
        /// Valid values for this enum property.
        values: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// Raw TOML deserialization helpers
// ---------------------------------------------------------------------------

/// Raw TOML shape for a single `[schema.types.<name>]` block.
/// Intentionally lenient (`serde(default)`) so partial configs are valid.
#[derive(Debug, Deserialize)]
pub struct RawTypeSchema {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(rename = "filename-template")]
    pub filename_template: Option<String>,
    #[serde(default)]
    pub defaults: HashMap<String, String>,
    #[serde(default)]
    pub properties: HashMap<String, PropertyConstraint>,
}

impl From<RawTypeSchema> for TypeSchema {
    fn from(raw: RawTypeSchema) -> Self {
        Self {
            required: raw.required,
            filename_template: raw.filename_template,
            defaults: raw.defaults,
            properties: raw.properties,
        }
    }
}

/// Raw TOML shape for the entire `[schema]` section.
#[derive(Debug, Deserialize)]
pub struct RawSchemaConfig {
    #[serde(default)]
    pub default: Option<RawTypeSchema>,
    #[serde(default)]
    pub types: HashMap<String, RawTypeSchema>,
}

impl From<RawSchemaConfig> for SchemaConfig {
    fn from(raw: RawSchemaConfig) -> Self {
        Self {
            default: raw.default.map(TypeSchema::from).unwrap_or_default(),
            types: raw
                .types
                .into_iter()
                .map(|(k, v)| (k, TypeSchema::from(v)))
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_schema_is_empty() {
        let cfg = SchemaConfig::default();
        assert!(cfg.is_empty());
    }

    #[test]
    fn parse_default_required() {
        let toml = r#"
[schema.default]
required = ["title"]
"#;
        // Parse directly as a full TOML document
        let raw: toml::Value = toml::from_str(toml).expect("valid toml");
        let raw_schema: RawSchemaConfig = raw
            .get("schema")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_else(|| RawSchemaConfig {
                default: None,
                types: HashMap::new(),
            });
        let cfg = SchemaConfig::from(raw_schema);
        assert_eq!(cfg.default.required, vec!["title".to_owned()]);
        assert!(!cfg.is_empty());
    }

    #[test]
    fn parse_type_schema() {
        let toml = r#"
[schema.default]
required = ["title"]

[schema.types.iteration]
required = ["title", "date", "status"]

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed"]

[schema.types.iteration.properties.date]
type = "date"
"#;
        let raw: toml::Value = toml::from_str(toml).expect("valid toml");
        let raw_schema: RawSchemaConfig = raw
            .get("schema")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_else(|| RawSchemaConfig {
                default: None,
                types: HashMap::new(),
            });
        let cfg = SchemaConfig::from(raw_schema);

        assert!(cfg.types.contains_key("iteration"));
        let iter = &cfg.types["iteration"];
        assert_eq!(iter.required, vec!["title", "date", "status"]);
        assert!(matches!(
            iter.properties.get("date"),
            Some(PropertyConstraint::Date)
        ));
        match iter.properties.get("status") {
            Some(PropertyConstraint::Enum { values }) => {
                assert!(values.contains(&"planned".to_owned()));
            }
            _ => panic!("expected enum constraint"),
        }
    }

    #[test]
    fn merged_schema_extends_required() {
        let toml = r#"
[schema.default]
required = ["title"]

[schema.types.iteration]
required = ["date", "status"]
"#;
        let raw: toml::Value = toml::from_str(toml).expect("valid toml");
        let raw_schema: RawSchemaConfig = raw
            .get("schema")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_else(|| RawSchemaConfig {
                default: None,
                types: HashMap::new(),
            });
        let cfg = SchemaConfig::from(raw_schema);

        let merged = cfg.merged_schema_for_type("iteration");
        // "title" from default + "date", "status" from type
        assert!(merged.required.contains(&"title".to_owned()));
        assert!(merged.required.contains(&"date".to_owned()));
        assert!(merged.required.contains(&"status".to_owned()));
        assert_eq!(merged.required.len(), 3);
    }

    #[test]
    fn merged_schema_type_override_default_property() {
        let toml = r#"
[schema.default.properties.status]
type = "string"

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "completed"]
"#;
        let raw: toml::Value = toml::from_str(toml).expect("valid toml");
        let raw_schema: RawSchemaConfig = raw
            .get("schema")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_else(|| RawSchemaConfig {
                default: None,
                types: HashMap::new(),
            });
        let cfg = SchemaConfig::from(raw_schema);

        let merged = cfg.merged_schema_for_type("iteration");
        match merged.properties.get("status") {
            Some(PropertyConstraint::Enum { values }) => {
                assert_eq!(values.len(), 2);
            }
            other => panic!("expected enum, got {other:?}"),
        }
    }

    #[test]
    fn merged_schema_for_unknown_type_uses_default() {
        let toml = r#"
[schema.default]
required = ["title"]
"#;
        let raw: toml::Value = toml::from_str(toml).expect("valid toml");
        let raw_schema: RawSchemaConfig = raw
            .get("schema")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_else(|| RawSchemaConfig {
                default: None,
                types: HashMap::new(),
            });
        let cfg = SchemaConfig::from(raw_schema);

        let merged = cfg.merged_schema_for_type("nonexistent");
        assert_eq!(merged.required, vec!["title".to_owned()]);
    }

    #[test]
    fn parse_string_pattern_constraint() {
        let toml = r#"
[schema.types.iteration.properties.branch]
type = "string"
pattern = "^iter-\\d+/"
"#;
        let raw: toml::Value = toml::from_str(toml).expect("valid toml");
        let raw_schema: RawSchemaConfig = raw
            .get("schema")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_else(|| RawSchemaConfig {
                default: None,
                types: HashMap::new(),
            });
        let cfg = SchemaConfig::from(raw_schema);

        match cfg.types["iteration"].properties.get("branch") {
            Some(PropertyConstraint::String { pattern: Some(p) }) => {
                assert_eq!(p, "^iter-\\d+/");
            }
            other => panic!("expected string with pattern, got {other:?}"),
        }
    }

    #[test]
    fn today_is_iso8601() {
        let d = today_iso8601();
        assert_eq!(d.len(), 10);
        let b = d.as_bytes();
        assert_eq!(b[4], b'-');
        assert_eq!(b[7], b'-');
        assert!(b[..4].iter().all(u8::is_ascii_digit));
        assert!(b[5..7].iter().all(u8::is_ascii_digit));
        assert!(b[8..10].iter().all(u8::is_ascii_digit));
    }

    #[test]
    fn days_to_ymd_known_dates() {
        // 1970-01-01 is day 0.
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        // 2000-01-01 is day 10_957.
        assert_eq!(days_to_ymd(10_957), (2000, 1, 1));
        // 2026-04-13 is day 20_556.
        assert_eq!(days_to_ymd(20_556), (2026, 4, 13));
    }

    #[test]
    fn expand_default_today() {
        let expanded = expand_default("$today");
        assert_eq!(expanded.len(), 10);
        assert_eq!(expanded.as_bytes()[4], b'-');

        let literal = expand_default("planned");
        assert_eq!(literal, "planned");
    }

    #[test]
    fn parse_no_duplicates_in_merged_required() {
        let toml = r#"
[schema.default]
required = ["title"]

[schema.types.note]
required = ["title", "date"]
"#;
        let raw: toml::Value = toml::from_str(toml).expect("valid toml");
        let raw_schema: RawSchemaConfig = raw
            .get("schema")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_else(|| RawSchemaConfig {
                default: None,
                types: HashMap::new(),
            });
        let cfg = SchemaConfig::from(raw_schema);

        let merged = cfg.merged_schema_for_type("note");
        // "title" must appear exactly once (no duplicate from both default and type)
        assert_eq!(merged.required.iter().filter(|r| *r == "title").count(), 1);
        assert_eq!(merged.required.len(), 2);
    }
}
