use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use hyalo_core::schema::{RawSchemaConfig, SchemaConfig};

/// Search-specific configuration from `[search]` in `.hyalo.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchConfig {
    language: Option<String>,
}

/// Link-extraction configuration from `[links]` in `.hyalo.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LinksConfig {
    /// Frontmatter property names whose values are scanned for `[[wikilink]]`
    /// strings and included in the link graph. Overrides the built-in defaults
    /// (`related`, `depends-on`, `supersedes`, `superseded-by`).
    frontmatter_properties: Option<Vec<String>>,
}

/// Lint configuration from `[lint]` in `.hyalo.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LintConfig {
    /// Vault-relative paths or glob patterns to skip during `hyalo lint`.
    /// Files matching any entry are excluded from lint output. Entries without
    /// glob meta-characters are matched literally against the normalized
    /// vault-relative path (`/` separators); other entries use the standard
    /// globset semantics (`**/*.md`, `dir/*.md`, etc.). This only affects the
    /// `lint` command — read-only commands still surface their own frontmatter
    /// parse-error warnings for these files.
    #[serde(default)]
    ignore: Vec<String>,
}

/// Raw deserialized representation of `.hyalo.toml`.
///
/// All fields are optional so that a partial config file is valid.
/// Unknown fields are rejected via `deny_unknown_fields` so that typos
/// are caught early rather than silently ignored.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    dir: Option<String>,
    format: Option<String>,
    hints: Option<bool>,
    /// Explicit override for the site prefix used when resolving absolute links
    /// (e.g. `/docs/page.md`).  When set, this takes precedence over the
    /// auto-derived value (last component of the resolved `dir`).
    site_prefix: Option<String>,
    /// Named find-filter sets. Stored so `deny_unknown_fields` does not reject
    /// configs that contain `[views.*]` tables. The views module reads these
    /// directly from the TOML file; they are not propagated to `ResolvedDefaults`.
    #[allow(dead_code)]
    views: Option<HashMap<String, toml::Value>>,
    /// Search configuration (BM25 stemming language, etc.)
    search: Option<SearchConfig>,
    /// Link extraction configuration (frontmatter property names to scan).
    links: Option<LinksConfig>,
    /// When `true`, schema validation runs automatically on every `set`/`append`.
    /// Accepted as a top-level key for backwards compatibility; the documented
    /// location is `[schema] validate_on_write`.
    validate_on_write: Option<bool>,
    /// Lint-specific configuration (`[lint]` section).
    lint: Option<LintConfig>,
    /// Schema configuration for document type validation.
    /// Stored as raw TOML value to avoid `deny_unknown_fields` issues with
    /// the deeply nested schema structure. Also hosts `validate_on_write` —
    /// see `extract_schema_validate_on_write`.
    #[serde(default)]
    schema: Option<toml::Value>,
    /// Default output limit for list commands (0 = unlimited).
    default_limit: Option<usize>,
}

/// Resolved configuration with all defaults applied.
#[derive(Debug)]
pub(crate) struct ResolvedDefaults {
    pub(crate) dir: PathBuf,
    /// The directory where `.hyalo.toml` was found.  Views and types are stored
    /// in this file, so mutations must target `config_dir/.hyalo.toml` — not the
    /// vault directory (which may be a subdirectory specified via `dir = "…"`).
    pub(crate) config_dir: PathBuf,
    pub(crate) format: String,
    pub(crate) hints: bool,
    /// Explicit site-prefix override from `.hyalo.toml`, if any.
    pub(crate) site_prefix: Option<String>,
    /// Default stemming language for BM25 search from `[search] language` in `.hyalo.toml`.
    pub(crate) search_language: Option<String>,
    /// Frontmatter property names scanned for `[[wikilink]]` values in the link graph.
    /// `None` = use built-in defaults (`related`, `depends-on`, etc.).
    pub(crate) frontmatter_link_props: Option<Vec<String>>,
    /// When `true`, schema validation is applied on every `set`/`append` operation.
    /// From `validate_on_write = true` in `.hyalo.toml`.
    pub(crate) validate_on_write: bool,
    /// Vault-relative paths excluded from `hyalo lint`. From `[lint] ignore`.
    pub(crate) lint_ignore: Vec<String>,
    /// Parsed schema configuration from `[schema.*]` sections.
    pub(crate) schema: SchemaConfig,
    /// Default output limit for list commands.
    /// `None` = use hardcoded default (50).
    /// `Some(0)` = unlimited.
    /// `Some(n)` = limit to n.
    pub(crate) default_limit: Option<usize>,
}

impl PartialEq for ResolvedDefaults {
    fn eq(&self, other: &Self) -> bool {
        // SchemaConfig doesn't implement PartialEq, so compare the other fields only.
        // Tests that care about schema equality check it separately.
        self.dir == other.dir
            && self.config_dir == other.config_dir
            && self.format == other.format
            && self.hints == other.hints
            && self.site_prefix == other.site_prefix
            && self.search_language == other.search_language
            && self.frontmatter_link_props == other.frontmatter_link_props
            && self.validate_on_write == other.validate_on_write
            && self.lint_ignore == other.lint_ignore
            && self.default_limit == other.default_limit
    }
}

impl ResolvedDefaults {
    fn hardcoded() -> Self {
        Self {
            dir: PathBuf::from("."),
            config_dir: PathBuf::from("."),
            format: "json".to_owned(),
            hints: true,
            site_prefix: None,
            search_language: None,
            frontmatter_link_props: None,
            validate_on_write: false,
            lint_ignore: Vec::new(),
            schema: SchemaConfig::default(),
            default_limit: None,
        }
    }

    /// Hardcoded defaults with `config_dir` set to the given directory.
    fn defaults_for(dir: &Path) -> Self {
        Self {
            config_dir: dir.to_path_buf(),
            ..Self::hardcoded()
        }
    }
}

/// Load configuration from `.hyalo.toml` in the current working directory.
///
/// Missing file → silent, returns hardcoded defaults.
/// I/O error (not NotFound) → prints a warning, returns defaults.
/// Malformed TOML or unknown fields → prints a warning, returns defaults.
/// Valid config → merges with defaults (config values take precedence).
pub(crate) fn load_config() -> ResolvedDefaults {
    match std::env::current_dir() {
        Ok(cwd) => load_config_from(&cwd),
        Err(e) => {
            crate::warn::warn(format!(
                "could not determine current directory to locate .hyalo.toml: {e}"
            ));
            ResolvedDefaults::hardcoded()
        }
    }
}

/// Load configuration from `.hyalo.toml` inside `dir`.
///
/// This variant accepts an explicit directory to make it testable without
/// relying on the process working directory.
pub(crate) fn load_config_from(dir: &Path) -> ResolvedDefaults {
    let path = dir.join(".hyalo.toml");

    let contents = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ResolvedDefaults::defaults_for(dir);
        }
        Err(e) => {
            crate::warn::warn(format!("could not read .hyalo.toml: {e}"));
            return ResolvedDefaults::defaults_for(dir);
        }
    };

    let cfg: ConfigFile = match toml::from_str(&contents) {
        Ok(c) => c,
        Err(e) => {
            crate::warn::warn(format!("malformed .hyalo.toml: {e}"));
            return ResolvedDefaults::defaults_for(dir);
        }
    };

    // Warn when the resolved config points its `dir` at a subdirectory that
    // itself contains a `.hyalo.toml`. The inner file is shadowed by this
    // parent config, and `hyalo` currently doesn't merge nested configs —
    // surfacing the shadow at least makes the silent shadowing visible.
    //
    // Routed through `warn::warn`, so `--quiet` suppresses it and the dedup
    // tracker prevents multiple prints per run. It's a warning (not a hint),
    // so `--no-hints` intentionally does *not* gate it.
    if let Some(ref sub) = cfg.dir {
        let nested = dir.join(sub).join(".hyalo.toml");
        if nested.is_file() {
            crate::warn::warn(format!(
                "ignoring nested config {}/.hyalo.toml (shadowed by {}/.hyalo.toml)",
                sub.trim_end_matches('/'),
                dir.display()
            ));
        }
    }

    let defaults = ResolvedDefaults::hardcoded();
    // Resolve `validate_on_write` from either `[schema] validate_on_write`
    // (documented location) or the top-level `validate_on_write` key
    // (backwards-compatible alternate). The `[schema]` table wins if both set.
    let schema_validate_on_write = extract_schema_validate_on_write(cfg.schema.as_ref());
    let validate_on_write = schema_validate_on_write
        .or(cfg.validate_on_write)
        .unwrap_or(false);
    let schema = parse_schema_from_toml(cfg.schema.as_ref());
    ResolvedDefaults {
        dir: cfg.dir.map(PathBuf::from).unwrap_or(defaults.dir),
        config_dir: dir.to_path_buf(),
        format: cfg.format.unwrap_or(defaults.format),
        hints: cfg.hints.unwrap_or(defaults.hints),
        site_prefix: cfg.site_prefix,
        search_language: cfg.search.and_then(|s| s.language),
        frontmatter_link_props: cfg.links.and_then(|l| l.frontmatter_properties),
        validate_on_write,
        lint_ignore: cfg.lint.map(|l| l.ignore).unwrap_or_default(),
        schema,
        default_limit: cfg.default_limit,
    }
}

/// Extract `[schema] validate_on_write` from the raw TOML if present. Returns
/// `None` if the key is absent or not a boolean (in which case the caller falls
/// back to the top-level `validate_on_write` key, then to the default `false`).
fn extract_schema_validate_on_write(raw: Option<&toml::Value>) -> Option<bool> {
    raw?.get("validate_on_write")?.as_bool()
}

/// Parse a `SchemaConfig` from the raw `[schema]` TOML value.
///
/// On malformed schema TOML, emits a warning and returns an empty schema
/// (no validation), consistent with how malformed `.hyalo.toml` is handled
/// throughout the rest of the config loading pipeline.
fn parse_schema_from_toml(raw: Option<&toml::Value>) -> SchemaConfig {
    let Some(val) = raw else {
        return SchemaConfig::default();
    };
    match val.clone().try_into::<RawSchemaConfig>() {
        Ok(raw_cfg) => SchemaConfig::from(raw_cfg),
        Err(e) => {
            crate::warn::warn(format!("malformed [schema] in .hyalo.toml: {e}"));
            SchemaConfig::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn make_temp() -> TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    #[test]
    fn missing_config_returns_defaults() {
        let dir = make_temp();
        let resolved = load_config_from(dir.path());
        assert_eq!(resolved, ResolvedDefaults::defaults_for(dir.path()));
    }

    #[test]
    fn valid_full_config() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            r#"
dir = "notes"
format = "text"
hints = true
"#,
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.dir, PathBuf::from("notes"));
        assert_eq!(resolved.format, "text");
        assert!(resolved.hints);
        assert_eq!(resolved.site_prefix, None);
    }

    #[test]
    fn site_prefix_config() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            r#"dir = "docs"
site_prefix = "docs"
"#,
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.dir, PathBuf::from("docs"));
        assert_eq!(resolved.site_prefix, Some("docs".to_owned()));
    }

    #[test]
    fn partial_config_merges_with_defaults() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "hints = false\n").unwrap();

        let resolved = load_config_from(dir.path());
        // Only hints overridden; dir and format stay at defaults.
        assert_eq!(resolved.dir, PathBuf::from("."));
        assert_eq!(resolved.format, "json");
        assert!(
            !resolved.hints,
            "config should override the default (true) to false"
        );
    }

    #[test]
    fn malformed_toml_returns_defaults() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "this is not { valid toml").unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved, ResolvedDefaults::defaults_for(dir.path()));
    }

    #[test]
    fn unknown_fields_returns_defaults() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "unknown_key = \"value\"\n").unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved, ResolvedDefaults::defaults_for(dir.path()));
    }

    #[test]
    fn invalid_format_value_passed_through() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "format = \"xml\"\n").unwrap();

        // config.rs does not validate the format string — that is the caller's job.
        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.format, "xml");
        assert_eq!(resolved.dir, PathBuf::from("."));
        assert!(resolved.hints);
    }

    #[test]
    fn search_language_config() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[search]\nlanguage = \"french\"\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.search_language, Some("french".to_owned()));
    }

    #[test]
    fn search_language_absent() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "dir = \"notes\"\n").unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.search_language, None);
    }

    #[test]
    fn search_language_empty_section() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "[search]\n").unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.search_language, None);
    }

    #[test]
    fn nested_config_emits_shadow_warning() {
        // Parent `.hyalo.toml` sets dir = "subkb" and `subkb/` contains its own
        // `.hyalo.toml`. The nested file is shadowed, so a warning must fire.
        let _guard = crate::warn::WARN_TEST_LOCK.lock().unwrap();
        crate::warn::reset_for_test();
        crate::warn::init(false);
        let dir = make_temp();
        fs::create_dir_all(dir.path().join("subkb")).unwrap();
        fs::write(dir.path().join(".hyalo.toml"), "dir = \"subkb\"\n").unwrap();
        fs::write(dir.path().join("subkb").join(".hyalo.toml"), "# nested\n").unwrap();
        let _ = load_config_from(dir.path());
        // The warning message is built with dir.display() which is a tempdir path,
        // so we verify the "ignoring nested config" fragment got tracked by
        // walking all recorded keys.
        let tracked =
            crate::warn::any_tracked_starts_with("ignoring nested config subkb/.hyalo.toml");
        assert!(tracked, "expected nested-config warning to fire");
    }

    #[test]
    fn config_dir_points_to_toml_location_not_vault_dir() {
        let dir = make_temp();
        fs::create_dir_all(dir.path().join("subdir")).unwrap();
        fs::write(dir.path().join(".hyalo.toml"), "dir = \"subdir\"\n").unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.dir, PathBuf::from("subdir"));
        assert_eq!(
            resolved.config_dir,
            dir.path().to_path_buf(),
            "config_dir should be where .hyalo.toml lives, not the vault subdir"
        );
    }

    // ---------------------------------------------------------------------------
    // UX-5: [lint] ignore list
    // ---------------------------------------------------------------------------

    #[test]
    fn lint_ignore_list_loaded() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[lint]\nignore = [\"templates/template.md\", \"_drafts/draft.md\"]\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(
            resolved.lint_ignore,
            vec![
                "templates/template.md".to_owned(),
                "_drafts/draft.md".to_owned()
            ]
        );
    }

    #[test]
    fn lint_ignore_empty_by_default() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "dir = \"notes\"\n").unwrap();

        let resolved = load_config_from(dir.path());
        assert!(resolved.lint_ignore.is_empty());
    }

    // ---------------------------------------------------------------------------
    // [links] frontmatter_properties config
    // ---------------------------------------------------------------------------

    #[test]
    fn links_frontmatter_properties_loaded() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[links]\nfrontmatter_properties = [\"related\", \"custom-ref\"]\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(
            resolved.frontmatter_link_props,
            Some(vec!["related".to_owned(), "custom-ref".to_owned()])
        );
    }

    // ---------------------------------------------------------------------------
    // validate_on_write config
    // ---------------------------------------------------------------------------

    #[test]
    fn validate_on_write_config() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "validate_on_write = true\n").unwrap();

        let resolved = load_config_from(dir.path());
        assert!(resolved.validate_on_write);
    }

    #[test]
    fn validate_on_write_under_schema_table() {
        // The documented location is `[schema] validate_on_write = true`.
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[schema]\nvalidate_on_write = true\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert!(
            resolved.validate_on_write,
            "`[schema] validate_on_write` should enable write-time validation"
        );
    }

    #[test]
    fn validate_on_write_schema_table_wins_over_top_level() {
        // If both are set, `[schema] validate_on_write` wins.
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "validate_on_write = false\n[schema]\nvalidate_on_write = true\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert!(resolved.validate_on_write);
    }

    #[test]
    fn validate_on_write_default_false() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "dir = \"notes\"\n").unwrap();

        let resolved = load_config_from(dir.path());
        assert!(!resolved.validate_on_write);
    }
}
