use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::Deserialize;

use hyalo_core::case_index::CaseInsensitiveMode;
use hyalo_core::schema::{RawSchemaConfig, SchemaConfig};
use hyalo_mdlint::RuleOverride;

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
    /// Case-insensitive link resolution mode.
    ///
    /// Accepted values: `"auto"` (default), `"true"`, `"false"`.
    /// - `"auto"` — enables fallback only on case-insensitive filesystems.
    /// - `"true"` — always enable case-insensitive fallback.
    /// - `"false"` — always disable; exact-match only.
    #[serde(default)]
    case_insensitive: Option<String>,
}

/// OKF generator configuration from `[okf]` in `.hyalo.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OkfConfig {
    /// Vault-relative glob patterns whose files `okf index`/`okf log` neither
    /// index nor generate into. Independent of `[lint] ignore`: use it to keep
    /// the generators out of template/fixture trees (`_template/**`,
    /// `test/fixture-vault/**`). Matched against forward-slash paths.
    #[serde(default)]
    ignore: Vec<String>,
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
    /// Per-rule output cap (default 3).
    max_violations_per_rule: Option<usize>,
    /// Worst-offender file cap (default 50).
    max_files: Option<usize>,
    /// Per-rule overrides. Stored as raw TOML to handle both scalar (`MD013 = false`)
    /// and table (`[lint.rules.MD013]`) forms.
    #[serde(default)]
    rules: Option<toml::Value>,
    /// When `true`, promote "no 'type' property" and "undeclared property in
    /// frontmatter" from `Severity::Warn` to `Severity::Error`, causing lint
    /// to exit non-zero on those warnings.  Overridable per-invocation with
    /// `hyalo lint --strict`.
    #[serde(default)]
    strict: bool,
    /// Active conformance profiles materialized into this config (e.g.
    /// `["okf", "madr"]`), written by `hyalo init --profile <name>`. When set,
    /// plain `hyalo lint` runs *every* listed profile's advisory rules without
    /// needing `--profile` on the CLI — so an initialized vault behaves
    /// identically to `hyalo lint --profile <name>` for each active profile
    /// (idempotent overlay). Multiple profiles compose here.
    #[serde(default)]
    profiles: Vec<String>,
    /// Deprecated single-profile alias for [`LintConfig::profiles`]. Accepted as
    /// a one-element compat form: `profile = "okf"` behaves like
    /// `profiles = ["okf"]`. `hyalo init --profile` now writes `profiles`.
    #[serde(default)]
    profile: Option<String>,
}

impl LintConfig {
    /// The effective list of active conformance profiles: the `profiles` list
    /// plus the deprecated singular `profile` alias (appended if not already
    /// present), preserving order.
    fn active_profiles(&self) -> Vec<String> {
        let mut out = self.profiles.clone();
        if let Some(single) = &self.profile
            && !out.iter().any(|p| p == single)
        {
            out.push(single.clone());
        }
        out
    }
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
    /// OKF generator configuration (`[okf]` section).
    okf: Option<OkfConfig>,
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
    /// Explicit format from `.hyalo.toml`, or `None` if not set.
    /// When `None`, format resolution falls back to TTY detection at runtime.
    pub(crate) format: Option<String>,
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
    /// Vault-relative globs the OKF generators skip. From `[okf] ignore`.
    pub(crate) okf_ignore: Vec<String>,
    /// Markdown linting config (max caps, per-rule overrides).
    pub(crate) md_lint: hyalo_mdlint::LintConfig,
    /// Parsed schema configuration from `[schema.*]` sections.
    pub(crate) schema: SchemaConfig,
    /// Default output limit for list commands.
    /// `None` = use hardcoded default (50).
    /// `Some(0)` = unlimited.
    /// `Some(n)` = limit to n.
    pub(crate) default_limit: Option<usize>,
    /// Case-insensitive link resolution mode from `[links] case_insensitive`.
    pub(crate) case_insensitive_mode: CaseInsensitiveMode,
    /// When `true`, "no 'type' property" and "undeclared property in frontmatter"
    /// warnings are promoted to errors.  From `[lint] strict = true` in `.hyalo.toml`.
    /// Can be overridden per-invocation with `hyalo lint --strict`.
    pub(crate) lint_strict: bool,
    /// `true` when a `.hyalo.toml` file was found AND parsed successfully.
    /// `false` when the file was missing, unreadable, or malformed (in which
    /// case all other fields are hardcoded defaults).
    pub(crate) loaded_from_file: bool,
    /// Active conformance profiles from `[lint] profiles` (or the deprecated
    /// `[lint] profile` alias) in `.hyalo.toml` (e.g. `["okf", "madr"]`).
    /// Enables every listed profile's advisory lint rules for plain
    /// `hyalo lint`, matching the ephemeral `--profile` overlay. Multiple
    /// profiles compose.
    pub(crate) lint_profiles: Vec<String>,
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
            && self.case_insensitive_mode == other.case_insensitive_mode
    }
}

impl ResolvedDefaults {
    fn hardcoded() -> Self {
        Self {
            dir: PathBuf::from("."),
            config_dir: PathBuf::from("."),
            format: None,
            hints: true,
            site_prefix: None,
            search_language: None,
            frontmatter_link_props: None,
            validate_on_write: false,
            lint_ignore: Vec::new(),
            okf_ignore: Vec::new(),
            md_lint: hyalo_mdlint::LintConfig::default(),
            schema: SchemaConfig::default(),
            default_limit: None,
            case_insensitive_mode: CaseInsensitiveMode::Auto,
            lint_strict: false,
            loaded_from_file: false,
            lint_profiles: Vec::new(),
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

/// Parse the `[links] case_insensitive` value into a [`CaseInsensitiveMode`].
///
/// Returns `Ok(None)` when the key is absent, `Ok(Some(mode))` on success,
/// and `Err(...)` when the value is not one of `"auto"`, `"true"`, or `"false"`.
fn parse_case_insensitive_mode(raw: Option<&str>) -> anyhow::Result<CaseInsensitiveMode> {
    match raw {
        None => Ok(CaseInsensitiveMode::Auto),
        Some(s) => CaseInsensitiveMode::parse(s)
            .with_context(|| format!("[links] case_insensitive = {s:?}")),
    }
}

/// Load configuration from `.hyalo.toml` inside `dir`.
///
/// Walks `[schema.types.*]` tables in a parsed `.hyalo.toml` looking for a real
/// `required-sections` key (the deprecated kebab-case alias). Used to gate the
/// deprecation warning so we don't false-positive on the string appearing in a
/// comment, doc string, or unrelated value.
fn schema_table_has_required_sections_key(raw: &toml::Value) -> bool {
    let Some(types) = raw
        .get("schema")
        .and_then(|s| s.get("types"))
        .and_then(toml::Value::as_table)
    else {
        return false;
    };
    types.values().any(|t| {
        t.as_table()
            .is_some_and(|tbl| tbl.contains_key("required-sections"))
    })
}

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

    // Deprecation: warn when the kebab-case `required-sections` key is used.
    // The canonical key is `required_sections`; the alias is kept for one release.
    // Parse as generic TOML so a string value or comment containing the literal
    // text "required-sections" doesn't trigger a false positive.
    if let Ok(raw) = toml::from_str::<toml::Value>(&contents)
        && schema_table_has_required_sections_key(&raw)
    {
        crate::warn::warn(
            "deprecated: 'required-sections' in .hyalo.toml — rename to 'required_sections'",
        );
    }

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
            // Skip warning when dir points back at itself (e.g. dir = ".") —
            // the nested path resolves to the same file as the root config.
            let is_self = nested
                .canonicalize()
                .and_then(|n| dir.join(".hyalo.toml").canonicalize().map(|r| n == r))
                .unwrap_or(false);
            if !is_self {
                crate::warn::warn(format!(
                    "ignoring nested config {}/.hyalo.toml (shadowed by {}/.hyalo.toml)",
                    sub.trim_end_matches('/'),
                    dir.display()
                ));
            }
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

    // Parse [links] fields — borrow before moving.
    let case_insensitive_mode = match parse_case_insensitive_mode(
        cfg.links
            .as_ref()
            .and_then(|l| l.case_insensitive.as_deref()),
    ) {
        Ok(m) => m,
        Err(e) => {
            crate::warn::warn(format!(
                "invalid [links] case_insensitive in .hyalo.toml: {e}"
            ));
            CaseInsensitiveMode::Auto
        }
    };

    let lint_strict = cfg.lint.as_ref().is_some_and(|l| l.strict);
    // Deprecation: the singular `[lint] profile = "..."` is a compat alias for
    // the `profiles` list. Warn so vaults migrate, but keep honoring it.
    if cfg.lint.as_ref().is_some_and(|l| l.profile.is_some()) {
        crate::warn::warn(
            "deprecated: '[lint] profile' in .hyalo.toml — use the list form \
             'profiles = [\"<name>\"]' (multiple profiles compose)",
        );
    }
    let lint_profiles = cfg
        .lint
        .as_ref()
        .map(LintConfig::active_profiles)
        .unwrap_or_default();

    ResolvedDefaults {
        dir: cfg.dir.map(PathBuf::from).unwrap_or(defaults.dir),
        config_dir: dir.to_path_buf(),
        format: cfg.format,
        hints: cfg.hints.unwrap_or(defaults.hints),
        site_prefix: cfg.site_prefix,
        search_language: cfg.search.and_then(|s| s.language),
        frontmatter_link_props: cfg.links.and_then(|l| l.frontmatter_properties),
        validate_on_write,
        lint_ignore: cfg
            .lint
            .as_ref()
            .map(|l| l.ignore.clone())
            .unwrap_or_default(),
        okf_ignore: cfg.okf.map(|o| o.ignore).unwrap_or_default(),
        md_lint: parse_md_lint_config(cfg.lint.as_ref()),
        schema,
        default_limit: cfg.default_limit,
        case_insensitive_mode,
        lint_strict,
        loaded_from_file: true,
        lint_profiles,
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
/// On malformed schema TOML (or invalid field combinations like `pattern` on a
/// non-string property), emits a warning and returns an empty schema (no
/// validation), consistent with how malformed `.hyalo.toml` is handled
/// throughout the rest of the config loading pipeline.
fn parse_schema_from_toml(raw: Option<&toml::Value>) -> SchemaConfig {
    let Some(val) = raw else {
        return SchemaConfig::default();
    };
    let raw_cfg: RawSchemaConfig = match val.clone().try_into() {
        Ok(c) => c,
        Err(e) => {
            crate::warn::warn(format!("malformed [schema] in .hyalo.toml: {e}"));
            return SchemaConfig::default();
        }
    };
    match SchemaConfig::try_from(raw_cfg) {
        Ok(cfg) => {
            // A `[[schema.bind]]` whose target names an undeclared type binds
            // nothing — warn so a typo doesn't fail silently.
            let unknown = cfg.unknown_bind_targets();
            if !unknown.is_empty() {
                crate::warn::warn(format!(
                    "[[schema.bind]] targets an unknown type (no matching [schema.types.*]): {}",
                    unknown.join(", ")
                ));
            }
            cfg
        }
        Err(e) => {
            crate::warn::warn(format!("invalid [schema] in .hyalo.toml: {e}"));
            SchemaConfig::default()
        }
    }
}

/// Parse `[lint]` into a `hyalo_mdlint::LintConfig` for markdown body linting.
///
/// Rule IDs are not validated against the catalog here — any string is
/// accepted as a key, so forward-compat with newer rule IDs is preserved.
/// Only unexpected value types (neither bool nor table) emit a warning.
fn parse_md_lint_config(raw: Option<&LintConfig>) -> hyalo_mdlint::LintConfig {
    let Some(lc) = raw else {
        return hyalo_mdlint::LintConfig::default();
    };
    let mut config = hyalo_mdlint::LintConfig {
        max_violations_per_rule: lc.max_violations_per_rule,
        max_files: lc.max_files,
        rules: HashMap::new(),
    };

    // Parse [lint.rules] which can be a mix of scalar (bool) and table entries.
    let Some(rules_val) = &lc.rules else {
        return config;
    };
    let Some(rules_table) = rules_val.as_table() else {
        crate::warn::warn("[lint.rules] is not a TOML table — ignoring");
        return config;
    };

    for (rule_id, value) in rules_table {
        let override_val = match value {
            toml::Value::Boolean(b) => RuleOverride::Enabled(*b),
            toml::Value::Table(tbl) => {
                let enabled = tbl.get("enabled").and_then(toml::Value::as_bool);
                let severity = tbl
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                if tbl.contains_key("mode") {
                    crate::warn::warn(format!(
                        "[lint.rules.{rule_id}].mode is no longer supported (the title↔H1 rule \
                         was removed in iter-127); ignoring"
                    ));
                }
                RuleOverride::Table { enabled, severity }
            }
            _ => {
                crate::warn::warn(format!(
                    "[lint.rules.{rule_id}] has unexpected type — expected bool or table"
                ));
                continue;
            }
        };
        config.rules.insert(rule_id.clone(), override_val);
    }

    config
}

/// The lint-relevant slice of config re-derived after overlaying a `--profile`
/// fragment onto the effective `.hyalo.toml`.
///
/// A `--profile <name>` overlay is *ephemeral*: it never writes `.hyalo.toml`.
/// It merges the profile's embedded TOML fragment (the same one
/// `hyalo init --profile <name>` materializes) into the raw config **in memory**
/// via [`crate::commands::profiles::merge_into_config`], then re-parses the
/// merged result. On a vault already initialized with that profile the merge is
/// idempotent, so the overlay yields the same schema/rules the on-disk config
/// already had — plain `hyalo lint` and `hyalo lint --profile <name>` behave
/// identically there.
pub(crate) struct ProfileOverlay {
    pub(crate) schema: SchemaConfig,
    pub(crate) md_lint: hyalo_mdlint::LintConfig,
    pub(crate) validate_on_write: bool,
    pub(crate) lint_strict: bool,
    /// Active profile markers from the merged `[lint] profiles` list (the
    /// fragment itself contributes its name), so the overlay enables every
    /// active profile's advisory rules even on a vault with no `.hyalo.toml` on
    /// disk. The requested `--profile <name>` is always present.
    pub(crate) lint_profiles: Vec<String>,
}

/// Build a [`ProfileOverlay`] by merging the named profile's fragment into the
/// `.hyalo.toml` found in `config_dir` (empty config if none exists).
///
/// Returns an error only when the profile name is unknown or the merge fails
/// (e.g. an existing `.hyalo.toml` that is not valid TOML). Schema/lint parse
/// problems degrade to defaults with a warning, matching [`load_config_from`].
pub(crate) fn overlay_profile(
    config_dir: &Path,
    profile_name: &str,
) -> anyhow::Result<ProfileOverlay> {
    let profile = crate::commands::profiles::lookup(profile_name)?;

    let existing_raw = match std::fs::read_to_string(config_dir.join(".hyalo.toml")) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            crate::warn::warn(format!(
                "could not read .hyalo.toml for --profile overlay: {e}; using an empty base"
            ));
            String::new()
        }
    };

    let merged_raw =
        crate::commands::profiles::merge_into_config(&existing_raw, profile.toml_fragment)?;

    // Re-parse the merged TOML through the same pipeline the file loader uses so
    // the overlay honors every schema/lint feature without a forked code path.
    let cfg: ConfigFile =
        toml::from_str(&merged_raw).context("merged --profile config is not valid TOML")?;

    let schema_validate_on_write = extract_schema_validate_on_write(cfg.schema.as_ref());
    let validate_on_write = schema_validate_on_write
        .or(cfg.validate_on_write)
        .unwrap_or(false);
    let schema = parse_schema_from_toml(cfg.schema.as_ref());
    let md_lint = parse_md_lint_config(cfg.lint.as_ref());
    let lint_strict = cfg.lint.as_ref().is_some_and(|l| l.strict);
    // The fragment contributes its name to `[lint] profiles`; ensure the
    // requested `--profile <name>` is always present even if a future fragment
    // omits the key. The union preserves any profiles the on-disk config
    // already activated (composed overlay).
    let mut lint_profiles = cfg
        .lint
        .as_ref()
        .map(LintConfig::active_profiles)
        .unwrap_or_default();
    if !lint_profiles.iter().any(|p| p == profile_name) {
        lint_profiles.push(profile_name.to_owned());
    }

    Ok(ProfileOverlay {
        schema,
        md_lint,
        validate_on_write,
        lint_strict,
        lint_profiles,
    })
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
        assert_eq!(resolved.format, Some("text".to_owned()));
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
        // format is None when not set in config (TTY detection applies at runtime).
        assert_eq!(resolved.format, None);
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
        assert_eq!(resolved.format, Some("xml".to_owned()));
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
    fn nested_config_dir_dot_no_warning() {
        // When dir = ".", the nested path resolves to the same .hyalo.toml —
        // this should NOT trigger a shadow warning.
        let _guard = crate::warn::WARN_TEST_LOCK.lock().unwrap();
        crate::warn::reset_for_test();
        crate::warn::init(false);
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
        let _ = load_config_from(dir.path());
        let tracked = crate::warn::any_tracked_starts_with("ignoring nested config");
        assert!(
            !tracked,
            "dir = '.' should not trigger nested-config warning"
        );
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
    // [okf] ignore config
    // ---------------------------------------------------------------------------

    #[test]
    fn okf_ignore_loaded() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[okf]\nignore = [\"_template/**\", \"test/fixture-vault/**\"]\n",
        )
        .unwrap();
        let resolved = load_config_from(dir.path());
        assert_eq!(
            resolved.okf_ignore,
            vec![
                "_template/**".to_owned(),
                "test/fixture-vault/**".to_owned()
            ]
        );
    }

    #[test]
    fn okf_ignore_defaults_empty() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
        let resolved = load_config_from(dir.path());
        assert!(resolved.okf_ignore.is_empty());
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

    // ---------------------------------------------------------------------------
    // [links] case_insensitive config
    // ---------------------------------------------------------------------------

    #[test]
    fn case_insensitive_missing_key_defaults_to_auto() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "dir = \"notes\"\n").unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(
            resolved.case_insensitive_mode,
            CaseInsensitiveMode::Auto,
            "missing key should default to Auto"
        );
    }

    #[test]
    fn case_insensitive_auto_value() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[links]\ncase_insensitive = \"auto\"\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.case_insensitive_mode, CaseInsensitiveMode::Auto);
    }

    #[test]
    fn case_insensitive_true_value() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[links]\ncase_insensitive = \"true\"\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.case_insensitive_mode, CaseInsensitiveMode::On);
    }

    #[test]
    fn case_insensitive_false_value() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[links]\ncase_insensitive = \"false\"\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.case_insensitive_mode, CaseInsensitiveMode::Off);
    }

    #[test]
    fn case_insensitive_invalid_value_falls_back_to_auto() {
        // Invalid values emit a warning and fall back to Auto.
        let _guard = crate::warn::WARN_TEST_LOCK.lock().unwrap();
        crate::warn::reset_for_test();
        crate::warn::init(false);
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[links]\ncase_insensitive = \"maybe\"\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(
            resolved.case_insensitive_mode,
            CaseInsensitiveMode::Auto,
            "invalid value should fall back to Auto"
        );
        let warned =
            crate::warn::any_tracked_starts_with("invalid [links] case_insensitive in .hyalo.toml");
        assert!(
            warned,
            "expected a warning for invalid case_insensitive value"
        );
    }

    // ---------------------------------------------------------------------------
    // iter-172: [lint] profiles list + compat alias
    // ---------------------------------------------------------------------------

    #[test]
    fn lint_profiles_list_loaded() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[lint]\nprofiles = [\"okf\", \"madr\"]\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(
            resolved.lint_profiles,
            vec!["okf".to_owned(), "madr".to_owned()],
            "both listed profiles active"
        );
    }

    #[test]
    fn lint_profile_singular_is_compat_alias() {
        // The deprecated `profile = "okf"` maps to a one-element list.
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[lint]\nprofile = \"okf\"\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.lint_profiles, vec!["okf".to_owned()]);
    }

    #[test]
    fn lint_profile_singular_emits_deprecation_warning() {
        let _guard = crate::warn::WARN_TEST_LOCK.lock().unwrap();
        crate::warn::reset_for_test();
        crate::warn::init(false);
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[lint]\nprofile = \"okf\"\n",
        )
        .unwrap();

        let _ = load_config_from(dir.path());
        assert!(
            crate::warn::any_tracked_starts_with("deprecated: '[lint] profile'"),
            "singular profile alias should warn"
        );
    }

    #[test]
    fn lint_profiles_and_alias_union_without_duplicates() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[lint]\nprofiles = [\"okf\"]\nprofile = \"okf\"\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(
            resolved.lint_profiles,
            vec!["okf".to_owned()],
            "duplicate alias is not appended twice"
        );
    }

    #[test]
    fn lint_profiles_empty_by_default() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "dir = \"notes\"\n").unwrap();

        let resolved = load_config_from(dir.path());
        assert!(resolved.lint_profiles.is_empty());
    }

    #[test]
    fn overlay_profile_composes_with_file_activated_profiles() {
        // A `--profile skills` overlay on a vault whose `.hyalo.toml` already
        // activates okf must yield BOTH profiles active (composed, not
        // replaced) — this is the flag-vs-file parity the plan requires.
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[lint]\nprofiles = [\"okf\"]\n[schema]\nexempt = [\"**/index.md\"]\n",
        )
        .unwrap();

        let overlay = overlay_profile(dir.path(), "skills").expect("skills overlay");
        assert!(
            overlay.lint_profiles.contains(&"okf".to_owned()),
            "file-activated okf survives the overlay: {:?}",
            overlay.lint_profiles
        );
        assert!(
            overlay.lint_profiles.contains(&"skills".to_owned()),
            "requested skills is active: {:?}",
            overlay.lint_profiles
        );
    }

    #[test]
    fn overlay_profile_honors_user_exempt_additions() {
        // mapl BUG-6: a `--profile` overlay must honor user `[schema] exempt`
        // additions exactly like the file path does (union, not clobber).
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[schema]\nexempt = [\"my/private/**\"]\n",
        )
        .unwrap();

        let overlay = overlay_profile(dir.path(), "okf").expect("okf overlay");
        assert!(
            overlay.schema.exempt.is_exempt("my/private/secret.md"),
            "user exempt addition honored by the --profile overlay"
        );
        assert!(
            overlay.schema.exempt.is_exempt("bundle/index.md"),
            "okf exempt also active"
        );
    }

    #[test]
    fn overlay_profile_lint_strict_reflects_merged_config_only() {
        // Regression: the caller in `run.rs` used to OR the overlay's
        // `lint_strict` with the pre-overlay config value, which could keep
        // strict mode on even when the merged (existing + fragment) config
        // does not set it. `overlay_profile` re-parses the merged config, so
        // its `lint_strict` field alone must be the source of truth — no OR
        // needed by callers. Here the base `.hyalo.toml` has no `[lint]`
        // section at all, so the merged/overlaid result must not be strict.
        let dir = make_temp();
        let overlay = overlay_profile(dir.path(), "okf").expect("okf profile must overlay");
        assert!(
            !overlay.lint_strict,
            "okf profile fragment does not set [lint] strict; overlay must not be strict"
        );
    }
}
