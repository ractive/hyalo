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

use crate::heading::parse_atx_heading;

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

        // Auto-add required properties that lack an explicit definition as string.
        for r in &required {
            properties
                .entry(r.clone())
                .or_insert(PropertyConstraint::String { pattern: None });
        }

        // Merge required_sections: default sections first, then type-specific ones.
        let mut required_sections = self.default.required_sections.clone();
        if let Some(ts) = type_schema {
            required_sections.extend(ts.required_sections.iter().cloned());
        }

        TypeSchema {
            required,
            filename_template: type_schema.and_then(|ts| ts.filename_template.clone()),
            defaults: type_schema
                .map(|ts| ts.defaults.clone())
                .unwrap_or_default(),
            properties,
            required_sections,
        }
    }

    /// Returns the default-only schema (used for files without a `type` property).
    pub fn default_schema(&self) -> &TypeSchema {
        &self.default
    }
}

/// Schema definition for a single document type (or the global default).
#[derive(Debug, Clone, Default)]
pub struct TypeSchema {
    /// Property keys that must be present in every file of this type.
    pub required: Vec<String>,

    /// Optional filename template for new files of this type.
    /// Tokens: `{n}` (sequence number), `{slug}` (title-derived slug).
    pub filename_template: Option<String>,

    /// Default values used when creating new files; `$today` expands to YYYY-MM-DD.
    pub defaults: HashMap<String, String>,

    /// Per-property type constraints keyed by property name.
    pub properties: HashMap<String, PropertyConstraint>,

    /// Required body sections in order. Each entry is `"<hashes> <text>"`,
    /// e.g. `"## Tasks"`. Validated at schema-load time via `parse_required_section_entry`.
    pub required_sections: Vec<String>,
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

/// Parse a `required-sections` entry string like `"## Tasks"` into `(level, text)`.
///
/// The entry must start with one to six `#` characters followed by a space and
/// the heading text. Returns an error string if the format is invalid.
///
/// # Examples
/// ```
/// use hyalo_core::schema::parse_required_section_entry;
/// assert_eq!(parse_required_section_entry("## Tasks").unwrap(), (2, "Tasks".to_owned()));
/// ```
pub fn parse_required_section_entry(entry: &str) -> Result<(u8, String), String> {
    match parse_atx_heading(entry) {
        Some((level, text)) => Ok((level, text.trim().to_owned())),
        None => Err(format!(
            "not a valid ATX heading: expected 1–6 '#' characters followed by a space and heading text, got {entry:?}"
        )),
    }
}

/// Current UTC date in YYYY-MM-DD format.
pub fn today_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
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
#[derive(Debug, Clone)]
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
    /// A YAML list of strings, with optional per-item regex validation.
    StringList {
        /// Optional regex each list item must match.
        item_pattern: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Raw TOML deserialization helpers
// ---------------------------------------------------------------------------

/// Flat raw shape for a single property constraint, capturing all possible
/// fields across all constraint variants. Converted to `PropertyConstraint`
/// via `TryFrom`, which validates field combinations.
///
/// Using a flat struct avoids the issue where `#[serde(tag = "type")]`
/// silently drops unknown fields, which would hide configuration errors like
/// `item_pattern` on a `string` property.
#[derive(Debug, Deserialize, Default)]
pub struct RawPropertyConstraint {
    #[serde(rename = "type")]
    pub constraint_type: Option<String>,
    pub pattern: Option<String>,
    pub item_pattern: Option<String>,
    pub values: Option<Vec<String>>,
}

impl TryFrom<RawPropertyConstraint> for PropertyConstraint {
    type Error = String;

    fn try_from(raw: RawPropertyConstraint) -> Result<Self, Self::Error> {
        // Validate mutually exclusive fields early.
        if raw.pattern.is_some() && raw.item_pattern.is_some() {
            return Err(
                "cannot set both 'pattern' and 'item_pattern' on the same property".to_owned(),
            );
        }

        let constraint_type = raw.constraint_type.as_deref().unwrap_or("string");

        match constraint_type {
            "string" => {
                if raw.item_pattern.is_some() {
                    return Err(format!(
                        "'item_pattern' is only valid on 'string-list' properties, not '{constraint_type}'"
                    ));
                }
                Ok(PropertyConstraint::String {
                    pattern: raw.pattern,
                })
            }
            "date" => {
                if raw.pattern.is_some() {
                    return Err(format!(
                        "'pattern' is only valid on 'string' properties, not '{constraint_type}'"
                    ));
                }
                if raw.item_pattern.is_some() {
                    return Err(format!(
                        "'item_pattern' is only valid on 'string-list' properties, not '{constraint_type}'"
                    ));
                }
                Ok(PropertyConstraint::Date)
            }
            "number" => {
                if raw.pattern.is_some() {
                    return Err(format!(
                        "'pattern' is only valid on 'string' properties, not '{constraint_type}'"
                    ));
                }
                if raw.item_pattern.is_some() {
                    return Err(format!(
                        "'item_pattern' is only valid on 'string-list' properties, not '{constraint_type}'"
                    ));
                }
                Ok(PropertyConstraint::Number)
            }
            "boolean" => {
                if raw.pattern.is_some() {
                    return Err(format!(
                        "'pattern' is only valid on 'string' properties, not '{constraint_type}'"
                    ));
                }
                if raw.item_pattern.is_some() {
                    return Err(format!(
                        "'item_pattern' is only valid on 'string-list' properties, not '{constraint_type}'"
                    ));
                }
                Ok(PropertyConstraint::Boolean)
            }
            "list" => {
                if raw.pattern.is_some() {
                    return Err(format!(
                        "'pattern' is only valid on 'string' properties, not '{constraint_type}'"
                    ));
                }
                if raw.item_pattern.is_some() {
                    return Err(format!(
                        "'item_pattern' is only valid on 'string-list' properties, not '{constraint_type}'"
                    ));
                }
                Ok(PropertyConstraint::List)
            }
            "enum" => {
                if raw.pattern.is_some() {
                    return Err(format!(
                        "'pattern' is only valid on 'string' properties, not '{constraint_type}'"
                    ));
                }
                if raw.item_pattern.is_some() {
                    return Err(format!(
                        "'item_pattern' is only valid on 'string-list' properties, not '{constraint_type}'"
                    ));
                }
                Ok(PropertyConstraint::Enum {
                    values: raw.values.unwrap_or_default(),
                })
            }
            "string-list" => {
                if raw.pattern.is_some() {
                    return Err(format!(
                        "'pattern' is only valid on 'string' properties, not '{constraint_type}'"
                    ));
                }
                Ok(PropertyConstraint::StringList {
                    item_pattern: raw.item_pattern,
                })
            }
            other => Err(format!(
                "unknown property constraint type '{other}': expected one of \
                 string, date, number, boolean, list, enum, string-list"
            )),
        }
    }
}

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
    pub properties: HashMap<String, RawPropertyConstraint>,
    /// Required body sections (ordered). Each entry is `"<hashes> <text>"`, e.g. `"## Tasks"`.
    #[serde(rename = "required-sections", default)]
    pub required_sections: Vec<String>,
}

impl TryFrom<RawTypeSchema> for TypeSchema {
    type Error = String;

    fn try_from(raw: RawTypeSchema) -> Result<Self, Self::Error> {
        let mut properties = HashMap::new();
        for (name, raw_constraint) in raw.properties {
            let constraint = PropertyConstraint::try_from(raw_constraint)
                .map_err(|e| format!("property '{name}': {e}"))?;
            properties.insert(name, constraint);
        }

        // Validate required_sections entries: each must parse as a valid ATX heading.
        for entry in &raw.required_sections {
            parse_required_section_entry(entry)
                .map_err(|e| format!("required-sections entry {entry:?}: {e}"))?;
        }

        Ok(TypeSchema {
            required: raw.required,
            filename_template: raw.filename_template,
            defaults: raw.defaults,
            properties,
            required_sections: raw.required_sections,
        })
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

impl TryFrom<RawSchemaConfig> for SchemaConfig {
    type Error = String;

    fn try_from(raw: RawSchemaConfig) -> Result<Self, Self::Error> {
        let default = match raw.default {
            Some(d) => TypeSchema::try_from(d).map_err(|e| format!("[schema.default]: {e}"))?,
            None => TypeSchema::default(),
        };
        let mut types = HashMap::new();
        for (name, raw_type) in raw.types {
            let ts = TypeSchema::try_from(raw_type)
                .map_err(|e| format!("[schema.types.{name}]: {e}"))?;
            types.insert(name, ts);
        }
        Ok(Self { default, types })
    }
}

impl SchemaConfig {
    /// Infallible conversion from raw config. Discards schema validation errors
    /// (emits no warning). Used where error propagation is not possible.
    ///
    /// Prefer [`SchemaConfig::try_from`] at call sites that can return errors.
    pub fn from_raw_lossy(raw: RawSchemaConfig) -> Self {
        SchemaConfig::try_from(raw).unwrap_or_default()
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
        let cfg = SchemaConfig::from_raw_lossy(raw_schema);
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
        let cfg = SchemaConfig::from_raw_lossy(raw_schema);

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
        let cfg = SchemaConfig::from_raw_lossy(raw_schema);

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
        let cfg = SchemaConfig::from_raw_lossy(raw_schema);

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
        let cfg = SchemaConfig::from_raw_lossy(raw_schema);

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
        let cfg = SchemaConfig::from_raw_lossy(raw_schema);

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
        let cfg = SchemaConfig::from_raw_lossy(raw_schema);

        let merged = cfg.merged_schema_for_type("note");
        // "title" must appear exactly once (no duplicate from both default and type)
        assert_eq!(merged.required.iter().filter(|r| *r == "title").count(), 1);
        assert_eq!(merged.required.len(), 2);
    }

    #[test]
    fn merged_schema_auto_adds_string_for_required_without_property() {
        let toml = r#"
[schema.default]
required = ["title", "type"]

[schema.types.docs]
required = ["title", "type", "date", "status"]

[schema.types.docs.properties.date]
type = "date"

[schema.types.docs.properties.status]
type = "enum"
values = ["active", "archived", "draft"]
"#;
        let raw: toml::Value = toml::from_str(toml).expect("valid toml");
        let raw_schema: RawSchemaConfig = raw
            .get("schema")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_else(|| RawSchemaConfig {
                default: None,
                types: HashMap::new(),
            });
        let cfg = SchemaConfig::from_raw_lossy(raw_schema);

        let merged = cfg.merged_schema_for_type("docs");
        // All 4 required fields should have property definitions
        assert_eq!(merged.properties.len(), 4);
        // title and type should be auto-added as string
        assert!(matches!(
            merged.properties.get("title"),
            Some(PropertyConstraint::String { pattern: None })
        ));
        assert!(matches!(
            merged.properties.get("type"),
            Some(PropertyConstraint::String { pattern: None })
        ));
        // Explicit definitions should be preserved
        assert!(matches!(
            merged.properties.get("date"),
            Some(PropertyConstraint::Date)
        ));
        assert!(matches!(
            merged.properties.get("status"),
            Some(PropertyConstraint::Enum { .. })
        ));
    }

    // ---------------------------------------------------------------------------
    // New tests: string-list, required_sections, schema-load error detection
    // ---------------------------------------------------------------------------

    fn parse_cfg(toml: &str) -> Result<SchemaConfig, String> {
        let raw: toml::Value = toml::from_str(toml).expect("valid toml");
        let raw_schema: RawSchemaConfig = raw
            .get("schema")
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or(RawSchemaConfig {
                default: None,
                types: HashMap::new(),
            });
        SchemaConfig::try_from(raw_schema)
    }

    #[test]
    fn parse_string_list_with_item_pattern() {
        let toml = r#"
[schema.types.note.properties.tags]
type = "string-list"
item_pattern = "^[a-z]+"
"#;
        let cfg = parse_cfg(toml).expect("should parse");
        match cfg.types["note"].properties.get("tags") {
            Some(PropertyConstraint::StringList {
                item_pattern: Some(p),
            }) => {
                assert_eq!(p, "^[a-z]+");
            }
            other => panic!("expected string-list with item_pattern, got {other:?}"),
        }
    }

    #[test]
    fn parse_required_sections() {
        let toml = "
[schema.types.note]\n\
required-sections = [\"# Title\", \"## Tasks\"]
";
        let cfg = parse_cfg(toml).expect("should parse");
        let ts = &cfg.types["note"];
        assert_eq!(ts.required_sections, vec!["# Title", "## Tasks"]);
    }

    #[test]
    fn reject_pattern_on_non_string() {
        let toml = r#"
[schema.types.note.properties.due]
type = "date"
pattern = "foo"
"#;
        let err = parse_cfg(toml).expect_err("should reject");
        assert!(
            err.contains("'pattern'"),
            "expected 'pattern' in error, got: {err}"
        );
        assert!(err.contains("date"), "expected 'date' in error, got: {err}");
    }

    #[test]
    fn reject_item_pattern_on_non_list() {
        let toml = r#"
[schema.types.note.properties.title]
type = "string"
item_pattern = "foo"
"#;
        let err = parse_cfg(toml).expect_err("should reject");
        assert!(
            err.contains("'item_pattern'"),
            "expected 'item_pattern' in error, got: {err}"
        );
    }

    #[test]
    fn reject_both_pattern_and_item_pattern() {
        let toml = r#"
[schema.types.note.properties.x]
type = "string"
pattern = "foo"
item_pattern = "bar"
"#;
        let err = parse_cfg(toml).expect_err("should reject");
        assert!(
            err.contains("pattern") && err.contains("item_pattern"),
            "expected both 'pattern' and 'item_pattern' mentioned, got: {err}"
        );
    }

    #[test]
    fn merge_required_sections_from_default_and_type() {
        let toml = "[schema.default]\nrequired-sections = [\"# Title\"]\n\n[schema.types.note]\nrequired-sections = [\"## Tasks\"]\n";
        let cfg = parse_cfg(toml).expect("should parse");
        let merged = cfg.merged_schema_for_type("note");
        assert_eq!(
            merged.required_sections,
            vec!["# Title", "## Tasks"],
            "default sections come first, type sections after"
        );
    }

    #[test]
    fn required_sections_invalid_entry_rejected() {
        let toml = "[schema.types.note]\nrequired-sections = [\"not a heading\"]\n";
        let err = parse_cfg(toml).expect_err("should reject invalid heading");
        assert!(
            err.contains("required-sections"),
            "expected 'required-sections' in error, got: {err}"
        );
    }
}
