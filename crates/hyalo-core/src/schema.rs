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
    /// Vault-relative glob patterns for files that are exempt from schema
    /// validation and from the `hyalo lint` required-`type` / frontmatter
    /// presence rules. Configured via `[schema] exempt = ["**/index.md", ...]`.
    ///
    /// Matching is vault-relative and cross-platform (paths are normalized to
    /// forward slashes before matching), so `**/index.md` matches
    /// `index.md`, `sub/index.md`, etc. on every OS.
    pub exempt: ExemptGlobs,
    /// Ordered path-glob → type bindings (`[schema.bind]`). When a file has no
    /// explicit `type:` frontmatter, the first binding whose glob matches its
    /// vault-relative path assigns the *effective* type. Explicit frontmatter
    /// always wins; see [`SchemaConfig::bound_type_for`].
    pub bind: SchemaBind,
}

/// Ordered, compiled `[schema.bind]` path-glob → type map.
///
/// Bindings are *ordered*: the first matching glob wins (authors put the most
/// specific paths first). Each entry is a `(pattern, type_name)` pair; the
/// patterns are compiled once into individual [`globset::GlobSet`]s so
/// first-match-wins can be evaluated without re-parsing globs per file.
#[derive(Debug, Clone, Default)]
pub struct SchemaBind {
    /// `(raw_glob, type_name)` in declaration order (preserved for display).
    entries: Vec<(String, String)>,
    /// One single-glob `GlobSet` per entry, index-aligned with `entries`.
    /// `None` for an entry whose glob failed to compile (skipped at match time;
    /// the config loader surfaces the error separately via `TryFrom`).
    sets: Vec<Option<globset::GlobSet>>,
}

impl SchemaBind {
    /// Build from ordered `(glob, type)` pairs. Invalid globs are kept in
    /// `entries` (for display) but compile to `None` and never match; the
    /// config loader reports them as errors via [`SchemaConfig::try_from`].
    #[must_use]
    pub fn new(entries: Vec<(String, String)>) -> Self {
        let sets = entries
            .iter()
            .map(|(pat, _)| {
                globset::GlobBuilder::new(pat)
                    .literal_separator(true)
                    .build()
                    .ok()
                    .and_then(|g| globset::GlobSetBuilder::new().add(g).build().ok())
            })
            .collect();
        Self { entries, sets }
    }

    /// Return the bound type for `rel_path` (vault-relative), or `None` when no
    /// binding matches. First declared match wins. Paths are normalized to
    /// forward slashes so Windows-style separators match the same globs.
    #[must_use]
    pub fn type_for(&self, rel_path: &str) -> Option<&str> {
        let normalized = rel_path.replace('\\', "/");
        for (idx, set) in self.sets.iter().enumerate() {
            if let Some(set) = set
                && set.is_match(normalized.as_str())
            {
                return Some(self.entries[idx].1.as_str());
            }
        }
        None
    }

    /// Whether any bindings are configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The configured `(glob, type)` entries, in declaration order (for display
    /// / `hyalo config`).
    #[must_use]
    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }
}

/// Compiled set of vault-relative exempt globs.
///
/// Wraps the raw patterns plus lazily-built [`globset::GlobSet`]s: one
/// case-sensitive, one case-insensitive. An empty set matches nothing. Cheap
/// to clone (the underlying `GlobSet`s are `Arc`-free but small, and patterns
/// are shared by clone).
#[derive(Debug, Clone, Default)]
pub struct ExemptGlobs {
    patterns: Vec<String>,
    set: Option<globset::GlobSet>,
    /// Same patterns compiled with `.case_insensitive(true)`, used by
    /// [`is_exempt_ci`](Self::is_exempt_ci) on filesystems that fold case
    /// (e.g. `INDEX.md` matching `**/index.md` on macOS/Windows).
    set_ci: Option<globset::GlobSet>,
}

impl ExemptGlobs {
    /// Build from raw glob patterns. Invalid globs are skipped (they are
    /// reported at config-load time by the caller if desired); an empty input
    /// yields a set that matches nothing.
    #[must_use]
    pub fn new(patterns: Vec<String>) -> Self {
        if patterns.is_empty() {
            return Self::default();
        }
        let mut builder = globset::GlobSetBuilder::new();
        let mut builder_ci = globset::GlobSetBuilder::new();
        let mut valid = Vec::with_capacity(patterns.len());
        for pat in patterns {
            // Skip invalid globs but keep the rest usable; config-load already
            // reports them as an error via `TryFrom`.
            if let Ok(g) = globset::GlobBuilder::new(&pat)
                .literal_separator(true)
                .build()
            {
                builder.add(g);
                valid.push(pat.clone());
            }
            if let Ok(g) = globset::GlobBuilder::new(&pat)
                .literal_separator(true)
                .case_insensitive(true)
                .build()
            {
                builder_ci.add(g);
            }
        }
        let set = builder.build().ok();
        let set_ci = builder_ci.build().ok();
        Self {
            patterns: valid,
            set,
            set_ci,
        }
    }

    /// Returns `true` when `rel_path` (a vault-relative path) matches any
    /// exempt glob. The path is normalized to forward slashes so Windows
    /// backslash-separated paths match the same globs as Unix paths.
    ///
    /// Matching is always case-sensitive here; use
    /// [`is_exempt_ci`](Self::is_exempt_ci) when the vault's filesystem folds
    /// case (e.g. under the resolved `[links] case_insensitive` mode).
    #[must_use]
    pub fn is_exempt(&self, rel_path: &str) -> bool {
        let Some(set) = &self.set else {
            return false;
        };
        let normalized = rel_path.replace('\\', "/");
        set.is_match(normalized.as_str())
    }

    /// Returns `true` when `rel_path` matches any exempt glob, honoring
    /// filesystem case-folding when `case_insensitive` is `true`.
    ///
    /// On a case-insensitive filesystem (macOS/Windows default), a file named
    /// `INDEX.md` and a pattern `**/index.md` refer to the same path on disk,
    /// so exempt matching should fold case too — otherwise `hyalo lint` and
    /// `hyalo okf index` disagree about which file is the reserved index file
    /// (`hyalo okf index` already treats `INDEX.md` as `index.md` via
    /// [`crate::case_index::mode_enabled`]). On a case-sensitive filesystem,
    /// pass `case_insensitive = false` — `INDEX.md` and `index.md` are
    /// genuinely different files there and only the latter should match.
    #[must_use]
    pub fn is_exempt_ci(&self, rel_path: &str, case_insensitive: bool) -> bool {
        if case_insensitive {
            let Some(set) = &self.set_ci else {
                return false;
            };
            let normalized = rel_path.replace('\\', "/");
            set.is_match(normalized.as_str())
        } else {
            self.is_exempt(rel_path)
        }
    }

    /// Returns `true` when no exempt patterns are configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// The configured raw glob patterns (for display / `hyalo config`).
    #[must_use]
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }
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
                .or_insert(PropertyConstraint::String {
                    pattern: None,
                    min_length: None,
                    max_length: None,
                });
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

    /// Resolve the effective type of a file at `rel_path` (vault-relative) that
    /// carries no explicit `type:` frontmatter, using the `[schema.bind]`
    /// path-glob bindings. Returns the bound type only when it names a declared
    /// `[schema.types.*]`; an unknown target is ignored (the config loader warns
    /// about it separately). Returns `None` when nothing matches.
    #[must_use]
    pub fn bound_type_for(&self, rel_path: &str) -> Option<&str> {
        let bound = self.bind.type_for(rel_path)?;
        if self.types.contains_key(bound) {
            Some(bound)
        } else {
            None
        }
    }

    /// The raw `[schema.bind]` entries whose target type is *not* a declared
    /// `[schema.types.*]`. Surfaced by the config loader as a warning so a typo
    /// in a bind target doesn't silently do nothing.
    #[must_use]
    pub fn unknown_bind_targets(&self) -> Vec<&str> {
        self.bind
            .entries()
            .iter()
            .filter(|(_, t)| !self.types.contains_key(t))
            .map(|(_, t)| t.as_str())
            .collect()
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
        Some((level, text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(format!(
                    "heading text must be non-empty: expected 1–6 '#' characters followed by a space and heading text, got {entry:?}"
                ));
            }
            Ok((level, trimmed.to_owned()))
        }
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
    /// Any string value; optional regex pattern and length-bound validation.
    String {
        /// Optional regex that the value must match.
        pattern: Option<String>,
        /// Optional inclusive minimum length (in Unicode scalar values).
        min_length: Option<usize>,
        /// Optional inclusive maximum length (in Unicode scalar values).
        max_length: Option<usize>,
    },
    /// ISO 8601 date (YYYY-MM-DD).
    Date,
    /// ISO 8601 naive local datetime (YYYY-MM-DDThh:mm:ss).
    DateTime,
    /// RFC 3339 timezone-aware datetime (YYYY-MM-DDThh:mm:ss with a `Z` or
    /// `±hh:mm` offset). Distinct from `DateTime`: a naive value is not
    /// accepted here, and a tz-aware value is not accepted by `DateTime`.
    DateTimeTz,
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
    #[serde(rename = "min-length")]
    pub min_length: Option<usize>,
    #[serde(rename = "max-length")]
    pub max_length: Option<usize>,
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

        // `values` is only meaningful on enum properties; reject it elsewhere
        // so misconfigured TOML surfaces as an error rather than silently
        // discarding the configured values.
        if raw.values.is_some() && constraint_type != "enum" {
            return Err(format!(
                "'values' is only valid on 'enum' properties, not '{constraint_type}'"
            ));
        }

        // Length bounds (`min-length` / `max-length`) apply only to `string`
        // properties. Reject them elsewhere so a typo (e.g. on a `date`) is a
        // hard error rather than a silently ignored key.
        if (raw.min_length.is_some() || raw.max_length.is_some()) && constraint_type != "string" {
            return Err(format!(
                "'min-length'/'max-length' are only valid on 'string' properties, not '{constraint_type}'"
            ));
        }
        if let (Some(min), Some(max)) = (raw.min_length, raw.max_length)
            && min > max
        {
            return Err(format!(
                "'min-length' ({min}) must not exceed 'max-length' ({max})"
            ));
        }

        match constraint_type {
            "string" => {
                if raw.item_pattern.is_some() {
                    return Err(format!(
                        "'item_pattern' is only valid on 'string-list' properties, not '{constraint_type}'"
                    ));
                }
                Ok(PropertyConstraint::String {
                    pattern: raw.pattern,
                    min_length: raw.min_length,
                    max_length: raw.max_length,
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
            "datetime" => {
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
                Ok(PropertyConstraint::DateTime)
            }
            "datetime-tz" => {
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
                Ok(PropertyConstraint::DateTimeTz)
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
                 string, date, datetime, datetime-tz, number, boolean, list, enum, string-list"
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
    /// Canonical key is `required_sections`; `required-sections` is accepted as a deprecated alias.
    #[serde(alias = "required-sections", default)]
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
    /// Vault-relative glob patterns for files exempt from validation and the
    /// `hyalo lint` required-`type` / frontmatter-presence rules.
    #[serde(default)]
    pub exempt: Vec<String>,
    /// Ordered path-glob → type bindings. Declared as an array of tables so the
    /// declaration order (first-match-wins) is preserved:
    ///
    /// ```toml
    /// [[schema.bind]]
    /// glob = "docs/decisions/**/*.md"
    /// type = "adr"
    /// ```
    #[serde(default)]
    pub bind: Vec<RawSchemaBind>,
}

/// Raw TOML shape for one `[[schema.bind]]` entry.
#[derive(Debug, Deserialize)]
pub struct RawSchemaBind {
    /// Vault-relative glob (forward slashes) selecting the files to bind.
    pub glob: String,
    /// The `[schema.types.*]` type assigned to matching files that lack an
    /// explicit `type:` frontmatter.
    #[serde(rename = "type")]
    pub type_name: String,
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
        // Validate exempt globs eagerly so a malformed pattern surfaces as a
        // config error rather than being silently dropped.
        for pat in &raw.exempt {
            globset::GlobBuilder::new(pat)
                .literal_separator(true)
                .build()
                .map_err(|e| format!("[schema] exempt: invalid glob {pat:?}: {e}"))?;
        }
        let exempt = ExemptGlobs::new(raw.exempt);
        // Validate bind globs eagerly so a malformed pattern is a config error
        // rather than a silently-dropped binding.
        for b in &raw.bind {
            globset::GlobBuilder::new(&b.glob)
                .literal_separator(true)
                .build()
                .map_err(|e| format!("[[schema.bind]]: invalid glob {:?}: {e}", b.glob))?;
        }
        let bind = SchemaBind::new(
            raw.bind
                .into_iter()
                .map(|b| (b.glob, b.type_name))
                .collect(),
        );
        Ok(Self {
            default,
            types,
            exempt,
            bind,
        })
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

    // ---------------------------------------------------------------------
    // ExemptGlobs case-(in)sensitivity
    // ---------------------------------------------------------------------

    #[test]
    fn exempt_globs_case_sensitive_by_default() {
        let globs = ExemptGlobs::new(vec!["**/index.md".to_owned()]);
        assert!(globs.is_exempt("index.md"));
        assert!(globs.is_exempt("sub/index.md"));
        assert!(!globs.is_exempt("INDEX.md"));
    }

    #[test]
    fn is_exempt_ci_false_matches_only_exact_case() {
        let globs = ExemptGlobs::new(vec!["**/index.md".to_owned()]);
        assert!(globs.is_exempt_ci("index.md", false));
        assert!(!globs.is_exempt_ci("INDEX.md", false));
    }

    #[test]
    fn is_exempt_ci_true_folds_case() {
        let globs = ExemptGlobs::new(vec!["**/index.md".to_owned()]);
        assert!(globs.is_exempt_ci("INDEX.md", true), "uppercase must match");
        assert!(
            globs.is_exempt_ci("Index.Md", true),
            "mixed case must match"
        );
        assert!(
            globs.is_exempt_ci("index.md", true),
            "lowercase must still match"
        );
        assert!(!globs.is_exempt_ci("other.md", true));
    }

    #[test]
    fn is_exempt_ci_normalizes_windows_separators() {
        let globs = ExemptGlobs::new(vec!["**/index.md".to_owned()]);
        assert!(globs.is_exempt_ci(r"sub\INDEX.md", true));
        assert!(!globs.is_exempt_ci(r"sub\INDEX.md", false));
    }

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
                exempt: Vec::new(),
                bind: Vec::new(),
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
                exempt: Vec::new(),
                bind: Vec::new(),
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
                exempt: Vec::new(),
                bind: Vec::new(),
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
                exempt: Vec::new(),
                bind: Vec::new(),
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
                exempt: Vec::new(),
                bind: Vec::new(),
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
                exempt: Vec::new(),
                bind: Vec::new(),
            });
        let cfg = SchemaConfig::from_raw_lossy(raw_schema);

        match cfg.types["iteration"].properties.get("branch") {
            Some(PropertyConstraint::String {
                pattern: Some(p), ..
            }) => {
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
                exempt: Vec::new(),
                bind: Vec::new(),
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
                exempt: Vec::new(),
                bind: Vec::new(),
            });
        let cfg = SchemaConfig::from_raw_lossy(raw_schema);

        let merged = cfg.merged_schema_for_type("docs");
        // All 4 required fields should have property definitions
        assert_eq!(merged.properties.len(), 4);
        // title and type should be auto-added as string
        assert!(matches!(
            merged.properties.get("title"),
            Some(PropertyConstraint::String { pattern: None, .. })
        ));
        assert!(matches!(
            merged.properties.get("type"),
            Some(PropertyConstraint::String { pattern: None, .. })
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
                exempt: Vec::new(),
                bind: Vec::new(),
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
    fn parse_datetime_constraint() {
        let toml = r#"
[schema.types.note.properties.when]
type = "datetime"
"#;
        let cfg = parse_cfg(toml).expect("should parse");
        assert!(matches!(
            cfg.types["note"].properties.get("when"),
            Some(PropertyConstraint::DateTime)
        ));
    }

    #[test]
    fn parse_datetime_tz_constraint() {
        let toml = r#"
[schema.types.concept.properties.timestamp]
type = "datetime-tz"
"#;
        let cfg = parse_cfg(toml).expect("should parse");
        assert!(matches!(
            cfg.types["concept"].properties.get("timestamp"),
            Some(PropertyConstraint::DateTimeTz)
        ));
    }

    #[test]
    fn reject_pattern_on_datetime_tz() {
        let toml = r#"
[schema.types.concept.properties.timestamp]
type = "datetime-tz"
pattern = "foo"
"#;
        let err = parse_cfg(toml).expect_err("should reject");
        assert!(err.contains("'pattern'"));
        assert!(err.contains("datetime-tz"));
    }

    #[test]
    fn reject_pattern_on_datetime() {
        let toml = r#"
[schema.types.note.properties.when]
type = "datetime"
pattern = "foo"
"#;
        let err = parse_cfg(toml).expect_err("should reject");
        assert!(err.contains("'pattern'"));
        assert!(err.contains("datetime"));
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

    #[test]
    fn bind_first_match_wins() {
        // Two bindings can overlap; declaration order decides.
        let toml = "\
[schema.types.adr]
required = [\"type\"]

[schema.types.note]
required = [\"type\"]

[[schema.bind]]
glob = \"docs/decisions/**/*.md\"
type = \"adr\"

[[schema.bind]]
glob = \"docs/**/*.md\"
type = \"note\"
";
        let cfg = parse_cfg(toml).expect("should parse");
        assert_eq!(cfg.bound_type_for("docs/decisions/0001-x.md"), Some("adr"));
        assert_eq!(cfg.bound_type_for("docs/guide/intro.md"), Some("note"));
        assert_eq!(cfg.bound_type_for("other/loose.md"), None);
    }

    #[test]
    fn bind_normalizes_backslashes() {
        let toml = "\
[schema.types.adr]
required = [\"type\"]

[[schema.bind]]
glob = \"docs/decisions/**/*.md\"
type = \"adr\"
";
        let cfg = parse_cfg(toml).expect("should parse");
        assert_eq!(
            cfg.bound_type_for("docs\\decisions\\0001-x.md"),
            Some("adr"),
            "Windows-style separators must match the same glob"
        );
    }

    #[test]
    fn bind_unknown_target_is_ignored_but_reported() {
        // A binding to a non-existent type does not resolve, but is surfaced by
        // unknown_bind_targets so the loader can warn.
        let toml = "\
[schema.types.adr]
required = [\"type\"]

[[schema.bind]]
glob = \"docs/decisions/**/*.md\"
type = \"typo-type\"
";
        let cfg = parse_cfg(toml).expect("should parse");
        assert_eq!(cfg.bound_type_for("docs/decisions/0001-x.md"), None);
        assert_eq!(cfg.unknown_bind_targets(), vec!["typo-type"]);
    }

    #[test]
    fn bind_invalid_glob_is_config_error() {
        let toml = "\
[schema.types.adr]
required = [\"type\"]

[[schema.bind]]
glob = \"docs/[unterminated\"
type = \"adr\"
";
        let err = parse_cfg(toml).expect_err("should reject invalid bind glob");
        assert!(
            err.contains("schema.bind") && err.contains("invalid glob"),
            "expected bind glob error, got: {err}"
        );
    }
}
