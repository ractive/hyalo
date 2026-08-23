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
    /// Persistent `hyalo links auto` preferences (`[links.auto]`).
    #[serde(default)]
    auto: Option<AutoLinksConfig>,
    /// Default confidence floor for `hyalo links fix --apply-fuzzy`
    /// (`[links] fuzzy_min_confidence`, iter-212).
    ///
    /// Must be in `[0.0, 1.0]`. Unset means the built-in
    /// [`hyalo_core::link_score::DEFAULT_FUZZY_MIN_CONFIDENCE`];
    /// `--min-confidence` on the command line wins over this value.
    #[serde(default)]
    fuzzy_min_confidence: Option<f64>,
}

/// Auto-link configuration from `[links.auto]` in `.hyalo.toml` (iter-195a).
///
/// Persists the exclusions and `first_only` preference that otherwise have to
/// be retyped on every `hyalo links auto` invocation. CLI flags are additive
/// for the two list keys (config ∪ flags) and `--first-only` turns the
/// behaviour on for a run regardless of the config value.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AutoLinksConfig {
    /// Titles never auto-linked (case-insensitive), same semantics as
    /// `--exclude-title`.
    #[serde(default)]
    exclude_titles: Vec<String>,
    /// Vault-relative globs whose pages are never auto-link *targets*, same
    /// semantics as `--exclude-target-glob`.
    #[serde(default)]
    exclude_target_globs: Vec<String>,
    /// When `true`, behaves as if `--first-only` had been passed.
    #[serde(default)]
    first_only: Option<bool>,
    /// When `false`, suppresses the advisory stderr note that names noisy
    /// candidate titles — common English words (iter-197) and titles that
    /// dominate the run (iter-205). Defaults to `true`;
    /// `--no-warn-common-titles` turns it off for a single run.
    #[serde(default)]
    warn_common_titles: Option<bool>,
}

/// Changelog configuration from `[changelog]` in `.hyalo.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangelogConfig {
    /// Path to the `CHANGELOG.md`, resolved relative to the config file's
    /// directory (`config_dir`). Defaults to `CHANGELOG.md` in the vault dir.
    /// May point outside the vault dir (e.g. `../CHANGELOG.md` for a repo-root
    /// changelog when the vault is a docs subdir), but never above the repo
    /// root — validated at resolution time. Used by `changelog add`,
    /// `changelog release`, and `lint --profile changelog`.
    path: Option<String>,
}

/// Vault-walker configuration from `[scan]` in `.hyalo.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScanConfig {
    /// Vault-relative globs whose (otherwise-skipped) hidden dot-paths the
    /// walker descends into. E.g. `[".claude/skills/**"]` makes the canonical
    /// Claude Code skill location reachable. `.git/**` is never re-included.
    /// Honored by every command that discovers vault files.
    #[serde(default)]
    include: Vec<String>,
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
    /// Vault-walker configuration (`[scan]` section).
    scan: Option<ScanConfig>,
    /// Changelog configuration (`[changelog]` section).
    changelog: Option<ChangelogConfig>,
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
#[derive(Debug, Clone)]
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
    /// Vault-relative globs the walker descends into despite being hidden
    /// dot-paths. From `[scan] include`. Installed process-wide at startup so
    /// every command's file discovery honors it.
    pub(crate) scan_include: Vec<String>,
    /// Raw `[changelog] path` value (config-dir-relative), if set. `None` means
    /// the default `CHANGELOG.md` in the vault dir. Resolution/validation into
    /// an absolute path happens in `run.rs` (needs `config_dir`).
    pub(crate) changelog_path: Option<String>,
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
    /// Titles `hyalo links auto` never links, from `[links.auto] exclude_titles`.
    /// Unioned with `--exclude-title` (flags extend, never replace).
    pub(crate) auto_link_exclude_titles: Vec<String>,
    /// Target-page globs `hyalo links auto` never links to, from
    /// `[links.auto] exclude_target_globs`. Unioned with `--exclude-target-glob`.
    pub(crate) auto_link_exclude_target_globs: Vec<String>,
    /// `[links.auto] first_only`. `true` makes `hyalo links auto` behave as if
    /// `--first-only` had been passed; the flag can still turn it on for a run
    /// when the config says `false`.
    pub(crate) auto_link_first_only: bool,
    /// `[links.auto] warn_common_titles`, default `true`. When `false`, `hyalo
    /// links auto` never emits the advisory note naming noisy candidate titles —
    /// common English words (iter-197) or titles that dominate the run
    /// (iter-205). `--no-warn-common-titles` turns it off for a single run; there
    /// is no flag to turn it back on, because the default already does.
    pub(crate) auto_link_warn_common_titles: bool,
    /// `[links] fuzzy_min_confidence` — the confidence floor `links fix
    /// --apply-fuzzy` applies when `--min-confidence` is not given (iter-212).
    /// `None` falls back to
    /// [`hyalo_core::link_score::DEFAULT_FUZZY_MIN_CONFIDENCE`].
    pub(crate) fuzzy_min_confidence: Option<f64>,
    /// When `true`, "no 'type' property" and "undeclared property in frontmatter"
    /// warnings are promoted to errors.  From `[lint] strict = true` in `.hyalo.toml`.
    /// Can be overridden per-invocation with `hyalo lint --strict`.
    pub(crate) lint_strict: bool,
    /// `true` when a `.hyalo.toml` file was found AND parsed successfully.
    /// `false` when the file was missing, unreadable, or malformed (in which
    /// case all other fields are hardcoded defaults).
    pub(crate) loaded_from_file: bool,
    /// The parse/read diagnostic when a `.hyalo.toml` **existed** but could not
    /// be used, `None` otherwise (iter-201, M-2).
    ///
    /// A missing file is not an error — it is the "no config" case and leaves
    /// this `None`. A file that is present but unusable is different in kind:
    /// every setting the user wrote (`dir`, `[lint] ignore`, schema, views) is
    /// gone, so a mutating command run in that state would operate on a vault
    /// and a rule set the user never configured. Callers use this to refuse the
    /// run instead of silently proceeding on defaults.
    pub(crate) malformed: Option<String>,
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
            && self.fuzzy_min_confidence == other.fuzzy_min_confidence
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
            scan_include: Vec::new(),
            changelog_path: None,
            md_lint: hyalo_mdlint::LintConfig::default(),
            schema: SchemaConfig::default(),
            default_limit: None,
            case_insensitive_mode: CaseInsensitiveMode::Auto,
            auto_link_exclude_titles: Vec::new(),
            auto_link_exclude_target_globs: Vec::new(),
            auto_link_first_only: false,
            auto_link_warn_common_titles: true,
            fuzzy_min_confidence: None,
            lint_strict: false,
            loaded_from_file: false,
            malformed: None,
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

    /// Defaults for a directory whose `.hyalo.toml` exists but is unusable.
    ///
    /// Records `diagnostic` in [`ResolvedDefaults::malformed`] so writers can
    /// refuse the run, and salvages the `dir` key when a lenient re-read can
    /// still find it. Salvaging `dir` matters even though every other setting
    /// is lost: it keeps read-only commands pointed at the vault the user
    /// configured instead of silently re-rooting them at the config directory.
    fn unusable_for(dir: &Path, diagnostic: String, salvaged_dir: Option<PathBuf>) -> Self {
        Self {
            dir: salvaged_dir.unwrap_or_else(|| PathBuf::from(".")),
            malformed: Some(diagnostic),
            ..Self::defaults_for(dir)
        }
    }
}

/// Best-effort extraction of the `dir` key from a `.hyalo.toml` that failed the
/// strict parse.
///
/// Used only on the error path. Returns `None` when the text is not even valid
/// TOML syntax, or when `dir` is absent or not a string.
fn salvage_dir(contents: &str) -> Option<PathBuf> {
    let raw: toml::Value = toml::from_str(contents).ok()?;
    let dir = raw.get("dir")?.as_str()?;
    Some(PathBuf::from(dir))
}

/// Best-effort read of the vault a candidate `.hyalo.toml` points at.
///
/// Deliberately lenient: this runs while *probing* ancestors, before hyalo has
/// committed to a config, so a parse failure must not print anything. An
/// unreadable or unparseable file yields the config directory itself, which is
/// the widest plausible vault — adopting it is what makes the malformed file
/// visible (via the strict load that follows) instead of silently skipped.
fn candidate_vault(config_dir: &Path) -> PathBuf {
    let sub = std::fs::read_to_string(config_dir.join(".hyalo.toml"))
        .ok()
        .and_then(|contents| salvage_dir(&contents))
        .unwrap_or_else(|| PathBuf::from("."));
    config_dir.join(sub)
}

/// Find the `.hyalo.toml` in an ancestor of `cwd` that governs `cwd`.
///
/// UX-1 (iter-213): before this, `.hyalo.toml` was read from the working
/// directory and nowhere else, so `cd docs && hyalo lint` re-rooted on built-in
/// defaults — no schema, no `[lint] ignore`, no views — and said nothing.
/// The `--dir` spelling of the same mistake had warned loudly since iter-201;
/// the far more common `cd` spelling was silent.
///
/// **Nearest config wins**: the walk stops at the first ancestor that has a
/// `.hyalo.toml`, and adopts it only when its configured vault contains `cwd`.
/// A nearer config that points somewhere else genuinely does not govern this
/// run, and walking past it to a further ancestor would make which file applies
/// depend on the contents of files in between.
fn discover_ancestor_config(cwd: &Path) -> Option<PathBuf> {
    let canonical_cwd = dunce::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let ancestor = canonical_cwd
        .ancestors()
        .skip(1)
        .find(|a| a.join(".hyalo.toml").is_file())?;
    let vault = dunce::canonicalize(candidate_vault(ancestor)).ok()?;
    canonical_cwd
        .starts_with(&vault)
        .then(|| ancestor.to_path_buf())
}

/// Tell the user when an adopted ancestor config widens the run beyond `cwd`.
///
/// Adoption is silent in the common case — `cd <vault> && hyalo …`, where the
/// configured vault *is* the working directory, so nothing about the run
/// changes except that the settings now apply. From a deeper subdirectory the
/// vault is genuinely wider than the directory the user is standing in, and
/// that is worth one line on stderr.
fn announce_ancestor_config(cwd: &Path, config_dir: &Path, config: &ResolvedDefaults) {
    let vault = config_dir.join(&config.dir);
    let same = match (dunce::canonicalize(&vault), dunce::canonicalize(cwd)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if same {
        return;
    }
    crate::warn::note(format!(
        "using {} from a parent directory: the vault is {}, not the current directory \
         — pass --dir . to scope this run to the current directory",
        config_dir.join(".hyalo.toml").display(),
        vault.display()
    ));
}

/// Load configuration for the current working directory.
///
/// Resolution order:
/// 1. `.hyalo.toml` in the working directory.
/// 2. Otherwise the nearest ancestor `.hyalo.toml` whose configured vault
///    contains the working directory (see [`discover_ancestor_config`]).
/// 3. Otherwise built-in defaults.
///
/// Missing file → silent, returns hardcoded defaults.
/// I/O error (not NotFound) → records a diagnostic, returns defaults.
/// Malformed TOML or unknown fields → records a diagnostic, returns defaults.
/// Valid config → merges with defaults (config values take precedence).
pub(crate) fn load_config() -> ResolvedDefaults {
    match std::env::current_dir() {
        Ok(cwd) => {
            if cwd.join(".hyalo.toml").is_file() {
                return load_config_from(&cwd);
            }
            match discover_ancestor_config(&cwd) {
                Some(ancestor) => {
                    let config = load_config_from(&ancestor);
                    announce_ancestor_config(&cwd, &ancestor, &config);
                    config
                }
                None => load_config_from(&cwd),
            }
        }
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
/// Walks `types.*` tables inside the already-parsed `[schema]` value looking for
/// a real `required-sections` key (the deprecated kebab-case alias). Used to
/// gate the deprecation warning so we don't false-positive on the string
/// appearing in a comment, doc string, or unrelated value.
///
/// Takes the `[schema]` value out of the parsed [`ConfigFile`] rather than a
/// second `toml::from_str` of the file text: the loader used to parse every
/// `.hyalo.toml` twice just to run this check (iter-201, M-2 double-parse).
fn schema_table_has_required_sections_key(schema: Option<&toml::Value>) -> bool {
    let Some(types) = schema
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

/// Turn a serde "unknown field" TOML error into the command that actually
/// creates the setting the user was reaching for.
///
/// `[types.note]` is the recurring case (dogfood UX-5): the real key is
/// `[schema.types.note]`, and the raw serde error only lists `schema` among the
/// accepted fields — enough to tell the reader they were wrong, not enough to
/// tell them what to run. Every entry here names a `hyalo` subcommand, because
/// hand-editing the TOML is what produced the broken file in the first place.
fn unknown_field_fix_path(error: &str) -> Option<&'static str> {
    const FIX_PATHS: &[(&str, &str)] = &[
        (
            "types",
            "type schemas live under [schema.types.<name>] — create one with \
             `hyalo types set <name> --required title,date` instead of editing the TOML",
        ),
        (
            "rules",
            "lint rule overrides live under [lint.rules] — set one with \
             `hyalo lint-rules set <RULE_ID> --severity error`",
        ),
        (
            "view",
            "saved queries live under [views] — create one with \
             `hyalo views set <name> --property status=draft`",
        ),
        (
            "profiles",
            "lint profiles live under [lint] as `profiles = [...]` — see `hyalo config`",
        ),
    ];

    // serde_toml renders this as: unknown field `types`, expected one of `dir`, …
    let field = error.split("unknown field `").nth(1)?.split('`').next()?;
    FIX_PATHS
        .iter()
        .find(|(name, _)| *name == field)
        .map(|(_, fix)| *fix)
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
            // The file exists but cannot be read — a config-integrity problem.
            // The diagnostic is *recorded*, not printed here: which config is
            // finally in effect is only known after `--dir` resolution, and
            // printing at load time announced a file a `--dir` override had
            // already switched away from (iter-213, UX-5). It is emitted by
            // `emit_config_diagnostics`, still `-q`-proof (iter-201, M-2).
            let diagnostic = format!("could not read .hyalo.toml: {e}");
            return ResolvedDefaults::unusable_for(dir, diagnostic, None);
        }
    };

    let mut cfg: ConfigFile = match toml::from_str(&contents) {
        Ok(c) => c,
        Err(e) => {
            let mut diagnostic = format!("malformed .hyalo.toml: {e}");
            if let Some(fix) = unknown_field_fix_path(&e.to_string()) {
                diagnostic.push_str("\n  fix: ");
                diagnostic.push_str(fix);
            }
            return ResolvedDefaults::unusable_for(dir, diagnostic, salvage_dir(&contents));
        }
    };

    // Deprecation: warn when the kebab-case `required-sections` key is used.
    // The canonical key is `required_sections`; the alias is kept for one
    // release. Checked against the already-parsed `[schema]` value (which the
    // loader keeps as raw TOML), so a string value or comment containing the
    // literal text "required-sections" still cannot trigger a false positive.
    if schema_table_has_required_sections_key(cfg.schema.as_ref()) {
        crate::warn::warn(
            "deprecated: 'required-sections' in .hyalo.toml — rename to 'required_sections'",
        );
    }

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

    // `[links] fuzzy_min_confidence` (iter-212) — an out-of-range value is a
    // config error the user should see, not a silent clamp, so warn and ignore.
    let fuzzy_min_confidence = match cfg.links.as_ref().and_then(|l| l.fuzzy_min_confidence) {
        Some(v) if (0.0..=1.0).contains(&v) => Some(v),
        Some(v) => {
            crate::warn::warn(format!(
                "invalid [links] fuzzy_min_confidence in .hyalo.toml: {v} is outside 0.0-1.0 — ignoring"
            ));
            None
        }
        None => None,
    };

    // `[links.auto]` — taken out of `cfg.links` before the struct is moved below.
    let links_auto = cfg
        .links
        .as_mut()
        .and_then(|l| l.auto.take())
        .unwrap_or_default();

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
        scan_include: cfg.scan.map(|s| s.include).unwrap_or_default(),
        changelog_path: cfg.changelog.and_then(|c| c.path),
        md_lint: parse_md_lint_config(cfg.lint.as_ref()),
        schema,
        default_limit: cfg.default_limit,
        case_insensitive_mode,
        auto_link_exclude_titles: links_auto.exclude_titles,
        auto_link_exclude_target_globs: links_auto.exclude_target_globs,
        auto_link_first_only: links_auto.first_only.unwrap_or(false),
        auto_link_warn_common_titles: links_auto.warn_common_titles.unwrap_or(true),
        fuzzy_min_confidence,
        lint_strict,
        loaded_from_file: true,
        malformed: None,
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

/// The `[scan] include` globs contributed by merging the named profile's
/// fragment into the `.hyalo.toml` found in `config_dir`.
///
/// Used to install the walker's dot-dir include list for an ephemeral
/// `--profile <name>` run (which never writes `.hyalo.toml`) so a profile that
/// ships `[scan] include` — e.g. `skills` reaching `.claude/skills/` — works
/// without first running `hyalo init --profile`. On any failure (unknown
/// profile, unreadable/invalid base config) returns an empty list rather than
/// erroring: dispatch surfaces the real profile error separately.
pub(crate) fn overlay_scan_include(config_dir: &Path, profile_name: &str) -> Vec<String> {
    let Ok(profile) = crate::commands::profiles::lookup(profile_name) else {
        return Vec::new();
    };
    let existing_raw = std::fs::read_to_string(config_dir.join(".hyalo.toml")).unwrap_or_default();
    let Ok(merged_raw) =
        crate::commands::profiles::merge_into_config(&existing_raw, profile.toml_fragment)
    else {
        return Vec::new();
    };
    let Ok(cfg) = toml::from_str::<ConfigFile>(&merged_raw) else {
        return Vec::new();
    };
    cfg.scan.map(|s| s.include).unwrap_or_default()
}

/// Where a run's effective configuration came from, and what an explicit
/// `--dir` did to it.
///
/// Built by [`resolve_effective`], which is the single place that answers "which
/// `.hyalo.toml` governs this invocation?". Before iter-201 that question had
/// two different answers — one in `run.rs` and one in `hyalo config` — and both
/// of them discarded the caller's config whenever `--dir` was present.
pub(crate) struct EffectiveConfig {
    /// The settings the run must use.
    pub(crate) config: ResolvedDefaults,
    /// The vault directory to operate on.
    pub(crate) dir: PathBuf,
    /// The `.hyalo.toml` actually in effect, or `None` when the run is on
    /// built-in defaults. Never `None` while a config file was read — that
    /// mismatch is exactly what made `hyalo config --dir X` lie.
    pub(crate) config_path: Option<PathBuf>,
    /// `true` when `--dir` named precisely the vault the CWD config already
    /// resolves to. The config still applies; the flag was just noise.
    pub(crate) dir_redundant: bool,
    /// `true` when `--dir` selected a *different* vault and the CWD did have a
    /// `.hyalo.toml`, so that file no longer applies to this run.
    pub(crate) cwd_config_shadowed: bool,
}

/// Path of the `.hyalo.toml` in `dir`, or `None` when there is no file there.
fn config_file_in(dir: &Path) -> Option<PathBuf> {
    let path = dir.join(".hyalo.toml");
    path.is_file().then_some(path)
}

/// Resolve the configuration that governs a run.
///
/// `cwd_config` is the already-loaded config for the process working directory;
/// `cli_dir` is the explicit `--dir` value, if any.
///
/// The `--dir` semantics (iter-201, H-4):
///
/// - **No `--dir`** — the CWD config applies, vault = its `dir`.
/// - **`--dir` naming the same directory the CWD config resolves to** — the CWD
///   config *still applies*. Previously hyalo reloaded `.hyalo.toml` from the
///   vault directory instead, which for the overwhelmingly common layout
///   (`dir = "docs"`, config at the repo root) meant no config at all: schema,
///   views, `[lint] ignore`, severity overrides and `site_prefix` were dropped
///   while the CLI printed "--dir is redundant". `lint --dir <same> --strict`
///   went vacuously green in CI as a result.
/// - **`--dir` naming a different directory** — that directory's own
///   `.hyalo.toml` applies if it has one, else built-in defaults. The caller
///   announces which, because this is the case where the user's config really
///   does stop applying.
pub(crate) fn resolve_effective(
    cwd_config: ResolvedDefaults,
    cli_dir: Option<&Path>,
) -> EffectiveConfig {
    let cwd_config_path = config_file_in(&cwd_config.config_dir);

    let Some(cli_dir) = cli_dir else {
        let dir = cwd_config.dir.clone();
        return EffectiveConfig {
            config: cwd_config,
            dir,
            config_path: cwd_config_path,
            dir_redundant: false,
            cwd_config_shadowed: false,
        };
    };

    // "Same vault" is decided on canonicalized paths so `--dir ./kb`,
    // `--dir kb/` and an absolute path all count as the configured vault.
    let cwd_vault = cwd_config.config_dir.join(&cwd_config.dir);
    let same_vault = cwd_config.loaded_from_file
        && match (
            dunce::canonicalize(cli_dir),
            dunce::canonicalize(&cwd_vault),
        ) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };

    if same_vault {
        return EffectiveConfig {
            config: cwd_config,
            dir: cli_dir.to_path_buf(),
            config_path: cwd_config_path,
            dir_redundant: true,
            cwd_config_shadowed: false,
        };
    }

    let target = load_config_from(cli_dir);
    EffectiveConfig {
        config: target,
        dir: cli_dir.to_path_buf(),
        config_path: config_file_in(cli_dir),
        dir_redundant: false,
        cwd_config_shadowed: cwd_config_path.is_some(),
    }
}

/// The stderr note that names the configuration a `--dir` override switched to.
///
/// Returns `None` when nothing was switched away from (no `--dir`, a redundant
/// `--dir`, or a CWD with no config to lose). Kept as a pure function so the
/// exact wording is unit-testable without spawning a process.
/// Where the effective `site_prefix` came from.
///
/// `hyalo config` reports this alongside the value so users can tell an
/// explicitly configured prefix from one hyalo inferred for them — the prefix
/// decides what a site-absolute link like `/foo` means, so a silent
/// auto-derived value is the difference between a vault of resolved links and
/// a vault of broken ones (iter-203, dogfood UX-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SitePrefixSource {
    /// From the `--site-prefix` CLI flag.
    Flag,
    /// From `site_prefix = "…"` in `.hyalo.toml`.
    Config,
    /// Inferred from the last component of the resolved vault directory.
    Derived,
    /// Explicitly disabled with an empty string (flag or config).
    Disabled,
}

impl SitePrefixSource {
    /// Short label used in `hyalo config` output and the JSON envelope.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Flag => "flag",
            Self::Config => "config",
            Self::Derived => "derived",
            Self::Disabled => "disabled",
        }
    }
}

/// Resolve the effective site prefix and record where it came from.
///
/// Tri-state precedence, highest first:
///
/// 1. `--site-prefix` — present wins; an empty string explicitly disables.
/// 2. `site_prefix` in `.hyalo.toml` — same empty-string rule.
/// 3. Auto-derived from the last component of the canonicalized vault dir.
///
/// This is the single owner of that chain: `run.rs` uses it to build the
/// pipeline context and `hyalo config` uses it to *report* the same answer,
/// so the two can never disagree.
pub(crate) fn resolve_site_prefix(
    cli_site_prefix: Option<&str>,
    config_site_prefix: Option<&str>,
    dir: &Path,
) -> (Option<String>, SitePrefixSource) {
    if let Some(flag) = cli_site_prefix {
        return if flag.is_empty() {
            (None, SitePrefixSource::Disabled)
        } else {
            (Some(flag.to_owned()), SitePrefixSource::Flag)
        };
    }
    if let Some(from_config) = config_site_prefix {
        return if from_config.is_empty() {
            (None, SitePrefixSource::Disabled)
        } else {
            (Some(from_config.to_owned()), SitePrefixSource::Config)
        };
    }
    // Auto-derive from the last component of the resolved dir.
    let derived = match std::fs::canonicalize(dir) {
        Ok(canonical) => canonical
            .file_name()
            .and_then(|n| n.to_str())
            .map(std::borrow::ToOwned::to_owned),
        Err(_) => {
            // canonicalize can still fail on valid directories (e.g. broken
            // symlink chains on some platforms). Fall back to the raw path
            // component rather than losing the prefix entirely.
            dir.file_name()
                .and_then(|n| n.to_str())
                .filter(|s| *s != ".")
                .map(std::borrow::ToOwned::to_owned)
        }
    };
    (derived, SitePrefixSource::Derived)
}

/// Render an index-vs-run mismatch as the field(s) that actually differ.
///
/// UX-3 (iter-213): the old wording printed the vault path twice — once for the
/// snapshot, once for the run — even when the paths were identical and only the
/// site prefix differed, and rendered the prefixes with `{:?}` so the reader
/// got `Some("en-us")` instead of a value. The one differing field was the last
/// thing you could see. This names only what differs, in `field: index X vs run
/// Y` form.
pub(crate) fn index_mismatch_summary(
    index_vault: &str,
    run_vault: &str,
    index_prefix: Option<&str>,
    run_prefix: Option<&str>,
) -> String {
    fn show(value: Option<&str>) -> String {
        value.map_or_else(|| "(none)".to_owned(), |v| format!("'{v}'"))
    }
    let mut parts = Vec::new();
    if index_vault != run_vault {
        parts.push(format!("vault: index '{index_vault}' vs run '{run_vault}'"));
    }
    if index_prefix != run_prefix {
        parts.push(format!(
            "site prefix: index {} vs run {}",
            show(index_prefix),
            show(run_prefix)
        ));
    }
    if parts.is_empty() {
        // Unreachable while `SnapshotIndex::validate` compares exactly these
        // two fields, but a future header field must not produce an empty
        // parenthetical.
        return "header differs".to_owned();
    }
    parts.join("; ")
}

/// Print the config-integrity diagnostic for the configuration that actually
/// governs this run, if any.
///
/// Called once, *after* `--dir` resolution. Before iter-213 the diagnostic was
/// printed the moment the CWD config failed to parse, which meant
/// `hyalo lint --dir other-vault` led with a warning about a file it had just
/// established does not apply — the stale warning even printed *before* the
/// "does not apply" note that contradicted it (dogfood UX-5).
pub(crate) fn emit_config_diagnostics(effective: &EffectiveConfig) {
    if let Some(diagnostic) = effective.config.malformed.as_deref() {
        crate::warn::warn_always(diagnostic);
    }
}

pub(crate) fn dir_override_note(effective: &EffectiveConfig) -> Option<String> {
    if !effective.cwd_config_shadowed {
        return None;
    }
    let dir = effective.dir.display();
    Some(match &effective.config_path {
        Some(path) => format!(
            "--dir {dir} selects a different vault: ./.hyalo.toml does not apply, \
             {} is in effect",
            path.display()
        ),
        None => format!(
            "--dir {dir} selects a different vault: ./.hyalo.toml does not apply and \
             {dir} has no .hyalo.toml — running on built-in defaults"
        ),
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

    // ---------------------------------------------------------------------------
    // resolve_effective / dir_override_note (iter-201, H-4)
    // ---------------------------------------------------------------------------

    /// A repo-root config pointing at a `kb/` subdirectory — the layout the
    /// `--dir` bug was worst on.
    fn make_project() -> TempDir {
        let dir = make_temp();
        fs::create_dir_all(dir.path().join("kb")).unwrap();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "dir = \"kb\"\nhints = false\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn resolve_without_dir_keeps_the_cwd_config() {
        let project = make_project();
        let effective = resolve_effective(load_config_from(project.path()), None);
        assert_eq!(effective.dir, PathBuf::from("kb"));
        assert!(!effective.config.hints, "the config must apply");
        assert!(effective.config_path.is_some());
        assert!(!effective.dir_redundant);
        assert!(!effective.cwd_config_shadowed);
    }

    #[test]
    fn resolve_with_dir_naming_the_configured_vault_keeps_the_config() {
        let project = make_project();
        let cli_dir = project.path().join("kb");
        let effective = resolve_effective(load_config_from(project.path()), Some(&cli_dir));
        assert!(
            !effective.config.hints,
            "the CWD config must survive a redundant --dir"
        );
        assert_eq!(
            effective.config_path,
            Some(project.path().join(".hyalo.toml"))
        );
        assert!(effective.dir_redundant);
        assert!(!effective.cwd_config_shadowed);
        assert_eq!(dir_override_note(&effective), None);
    }

    #[test]
    fn resolve_with_dir_naming_another_tree_drops_the_cwd_config() {
        let project = make_project();
        let other = project.path().join("other");
        fs::create_dir_all(&other).unwrap();
        let effective = resolve_effective(load_config_from(project.path()), Some(&other));
        assert!(
            effective.config.hints,
            "a different vault must not inherit hints = false"
        );
        assert_eq!(effective.config_path, None);
        assert!(!effective.dir_redundant);
        assert!(effective.cwd_config_shadowed);
        let note = dir_override_note(&effective).expect("the switch must be announced");
        assert!(note.contains("built-in defaults"), "note was: {note}");
    }

    #[test]
    fn resolve_with_dir_naming_a_tree_with_its_own_config_uses_that_file() {
        let project = make_project();
        let other = project.path().join("other");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join(".hyalo.toml"), "site_prefix = \"other\"\n").unwrap();
        let effective = resolve_effective(load_config_from(project.path()), Some(&other));
        assert_eq!(
            effective.config.site_prefix.as_deref(),
            Some("other"),
            "the target's own config must apply"
        );
        assert_eq!(effective.config_path, Some(other.join(".hyalo.toml")));
        let note = dir_override_note(&effective).expect("the switch must be announced");
        assert!(
            note.contains(".hyalo.toml is in effect"),
            "note was: {note}"
        );
    }

    #[test]
    fn resolve_without_a_cwd_config_announces_nothing() {
        // Nothing is being shadowed, so a `--dir` elsewhere is not news.
        let dir = make_temp();
        let other = dir.path().join("other");
        fs::create_dir_all(&other).unwrap();
        let effective = resolve_effective(load_config_from(dir.path()), Some(&other));
        assert!(!effective.cwd_config_shadowed);
        assert_eq!(dir_override_note(&effective), None);
    }

    #[test]
    fn resolve_with_a_malformed_cwd_config_does_not_claim_redundancy() {
        // A config that did not parse resolves no vault, so `--dir` cannot be
        // "the same as" anything and must not be called redundant.
        let dir = make_temp();
        fs::create_dir_all(dir.path().join("kb")).unwrap();
        fs::write(dir.path().join(".hyalo.toml"), "dir = \"kb\"\nnope = 1\n").unwrap();
        let _guard = crate::warn::WARN_TEST_LOCK.lock().unwrap();
        crate::warn::reset_for_test();
        crate::warn::init(false);
        let cli_dir = dir.path().join("kb");
        let effective = resolve_effective(load_config_from(dir.path()), Some(&cli_dir));
        assert!(!effective.dir_redundant);
    }

    #[test]
    fn salvage_dir_recovers_dir_from_an_otherwise_unusable_file() {
        assert_eq!(
            salvage_dir("dir = \"kb\"\nnope = 1\n"),
            Some(PathBuf::from("kb"))
        );
        assert_eq!(salvage_dir("this is not { toml"), None);
        assert_eq!(salvage_dir("nope = 1\n"), None);
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
    // [links.auto] config (iter-195a)
    // ---------------------------------------------------------------------------

    #[test]
    fn links_auto_all_keys_loaded() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[links.auto]\nexclude_titles = [\"permissions\", \"README\"]\n\
             exclude_target_globs = [\"templates/*\"]\nfirst_only = true\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(
            resolved.auto_link_exclude_titles,
            vec!["permissions".to_owned(), "README".to_owned()]
        );
        assert_eq!(
            resolved.auto_link_exclude_target_globs,
            vec!["templates/*".to_owned()]
        );
        assert!(resolved.auto_link_first_only);
        assert!(
            resolved.auto_link_warn_common_titles,
            "an unset warn_common_titles must stay on"
        );
    }

    // ---------------------------------------------------------------------------
    // [links.auto] warn_common_titles (iter-197)
    // ---------------------------------------------------------------------------

    #[test]
    fn links_auto_warn_common_titles_defaults_to_true() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "dir = \"vault\"\n").unwrap();

        assert!(load_config_from(dir.path()).auto_link_warn_common_titles);
    }

    #[test]
    fn links_auto_warn_common_titles_false_is_loaded() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[links.auto]\nwarn_common_titles = false\n",
        )
        .unwrap();

        assert!(!load_config_from(dir.path()).auto_link_warn_common_titles);
    }

    #[test]
    fn links_auto_warn_common_titles_true_is_loaded() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[links.auto]\nwarn_common_titles = true\n",
        )
        .unwrap();

        assert!(load_config_from(dir.path()).auto_link_warn_common_titles);
    }

    #[test]
    fn links_auto_partial_table_keeps_other_defaults() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[links.auto]\nfirst_only = true\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert!(resolved.auto_link_exclude_titles.is_empty());
        assert!(resolved.auto_link_exclude_target_globs.is_empty());
        assert!(resolved.auto_link_first_only);
    }

    #[test]
    fn links_auto_coexists_with_other_links_keys() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "[links]\ncase_insensitive = \"true\"\nfrontmatter_properties = [\"related\"]\n\n\
             [links.auto]\nexclude_titles = [\"index\"]\n",
        )
        .unwrap();

        let resolved = load_config_from(dir.path());
        assert_eq!(resolved.case_insensitive_mode, CaseInsensitiveMode::On);
        assert_eq!(
            resolved.frontmatter_link_props,
            Some(vec!["related".to_owned()])
        );
        assert_eq!(resolved.auto_link_exclude_titles, vec!["index".to_owned()]);
    }

    #[test]
    fn links_auto_defaults_when_absent() {
        let dir = make_temp();
        fs::write(dir.path().join(".hyalo.toml"), "[links]\n").unwrap();

        let resolved = load_config_from(dir.path());
        assert!(resolved.auto_link_exclude_titles.is_empty());
        assert!(resolved.auto_link_exclude_target_globs.is_empty());
        assert!(!resolved.auto_link_first_only);
    }

    #[test]
    fn links_auto_unknown_key_is_warned_and_config_ignored() {
        let dir = make_temp();
        fs::write(
            dir.path().join(".hyalo.toml"),
            "dir = \"vault\"\n[links.auto]\nexclude_title = [\"typo\"]\n",
        )
        .unwrap();

        // Same behaviour as every other unknown key: warn, then fall back to
        // hardcoded defaults for the whole file (deny_unknown_fields) — except
        // `dir`, which is salvaged so read-only commands stay pointed at the
        // configured vault instead of silently re-rooting (iter-201).
        let _guard = crate::warn::WARN_TEST_LOCK.lock().unwrap();
        crate::warn::reset_for_test();
        crate::warn::init(false);
        let resolved = load_config_from(dir.path());
        assert!(resolved.auto_link_exclude_titles.is_empty());
        assert_eq!(resolved.dir, PathBuf::from("vault"));
        assert!(
            resolved.malformed.is_some(),
            "an unusable config must be flagged so writers can refuse"
        );
        assert!(crate::warn::any_tracked_starts_with(
            "malformed .hyalo.toml"
        ));
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
