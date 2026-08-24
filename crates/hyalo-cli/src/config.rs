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
    /// `true` when [`Self::dir`] was recovered from an otherwise-unusable
    /// `.hyalo.toml` by [`salvage_dir`] — only meaningful alongside
    /// [`Self::malformed`].
    ///
    /// NEW-17 (dogfood pre3): the malformed-config note says "every value
    /// below is a built-in default", which is false for `dir` whenever this
    /// is `true` — the reported vault came from the broken file, not a
    /// hardcoded fallback. Lets the reporter say so instead of contradicting
    /// itself.
    pub(crate) dir_salvaged: bool,
    /// Active conformance profiles from `[lint] profiles` (or the deprecated
    /// `[lint] profile` alias) in `.hyalo.toml` (e.g. `["okf", "madr"]`).
    /// Enables every listed profile's advisory lint rules for plain
    /// `hyalo lint`, matching the ephemeral `--profile` overlay. Multiple
    /// profiles compose.
    pub(crate) lint_profiles: Vec<String>,
    /// Diagnostic when this config's `dir` resolves outside `config_dir`
    /// itself — absolute, or a net `..` escape (H-1, iter-221). `None` for
    /// every other config, including one where `dir` was never set.
    ///
    /// Distinct from [`Self::malformed`]: the TOML parsed fine here, but a
    /// project-local `dir` this wide is a scope expansion the config is not
    /// entitled to make silently. `dir` above is left at the hardcoded `"."`
    /// default in this case, never the offending value, so nothing
    /// downstream can act on it even if a caller forgets to check this field
    /// first. Every caller that can touch the filesystem must refuse the run
    /// while this is `Some`; `hyalo config` is the one exception — it exists
    /// to surface exactly this and must keep working to do so.
    pub(crate) dir_out_of_bounds: Option<String>,
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
            dir_salvaged: false,
            lint_profiles: Vec::new(),
            dir_out_of_bounds: None,
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
        let dir_salvaged = salvaged_dir.is_some();
        Self {
            dir: salvaged_dir.unwrap_or_else(|| PathBuf::from(".")),
            malformed: Some(diagnostic),
            dir_salvaged,
            ..Self::defaults_for(dir)
        }
    }

    /// Defaults for a directory whose `.hyalo.toml` set a `dir` outside its
    /// own boundary (H-1, iter-221). See [`Self::dir_out_of_bounds`].
    fn dir_out_of_bounds_for(dir: &Path, diagnostic: String) -> Self {
        Self {
            dir_out_of_bounds: Some(diagnostic),
            ..Self::defaults_for(dir)
        }
    }
}

/// Why a project-local `dir` was refused by [`validate_project_local_dir`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirBoundaryReason {
    /// `dir` is an absolute path.
    Absolute,
    /// `dir`'s `..` components net-escape the config directory.
    Escapes,
}

/// A project-local `dir` that resolves outside its own config directory
/// (H-1, iter-221).
#[derive(Debug, Clone)]
struct DirBoundaryError {
    /// The `.hyalo.toml` that set the offending `dir`.
    config_path: PathBuf,
    /// The raw `dir = "…"` value, exactly as written in the file.
    raw_dir: String,
    reason: DirBoundaryReason,
}

impl DirBoundaryError {
    /// One-line diagnostic naming both the offending file and value, with a
    /// pointer to the escape hatch — used for the loud stderr warning, the
    /// hard-refusal error body, and `hyalo config`'s report alike so the
    /// wording never has to be kept in sync across three call sites.
    ///
    /// Sanitized before being returned: `raw_dir` is attacker-controlled
    /// TOML content from a project-local `.hyalo.toml`, and this diagnostic
    /// is printed via `warn::warn_always` and `AppError::User`'s `eprintln!`
    /// — neither of which sanitizes on its own — so an embedded terminal
    /// escape sequence in `dir` would otherwise reach the user's terminal
    /// raw, even under `-q` (PR #253 review, Copilot finding 2). The config
    /// directory's own path is included too, even though it is less likely
    /// to be attacker-chosen — a directory name can still contain arbitrary
    /// bytes on a hostile repo.
    fn diagnostic(&self) -> String {
        let what = match self.reason {
            DirBoundaryReason::Absolute => {
                "is an absolute path, which a project-local .hyalo.toml is not allowed to set"
            }
            DirBoundaryReason::Escapes => "resolves above the config directory",
        };
        crate::output::sanitize_control_chars(&format!(
            "{}: dir = {:?} {what} — pass --dir explicitly if that scope is genuinely intended",
            self.config_path.display(),
            self.raw_dir
        ))
    }
}

/// Refuse a project-local `dir` that is absolute or that nets outside
/// `config_dir` after resolving `..` components (H-1, iter-221).
///
/// A project-local `.hyalo.toml` travels with the repository it lives in —
/// hyalo is agent-driven, and agents run its hints verbatim, so a hostile
/// clone that redefines `dir` this way could redirect every read and write
/// hyalo does at a location the user never chose. `dir` is trusted to name
/// anything *at or below* `config_dir` (bounded `sub/../other` round-trips
/// included, mirroring [`resolve_changelog_file`]); an absolute path or a net
/// `..` escape is refused outright rather than silently clamped, matching the
/// "no silent config discard" stance from iter-201 (DEC-069/070/071) — a
/// scope expansion this large is exactly the kind of thing that must be loud.
///
/// Symlinks are checked too: when the resolved path already exists on disk,
/// both sides are canonicalized and compared for real containment, so a
/// `dir` that is lexically bounded but physically escapes via a symlink is
/// still caught. The canonicalize check runs against the *lexically
/// normalized* join (`config_dir` + the `..`-collapsed form), not the raw
/// join with `..` segments still in it — a raw join for an allowed
/// round-trip like `"sub/../kb"` would still contain the phantom `sub/`
/// component, and `canonicalize` fails outright (ENOENT) walking through a
/// directory that doesn't exist, silently no-opping the symlink check for
/// exactly the inputs that need it (PR #253 review, Copilot finding 1). A
/// `dir` whose *normalized* target does not exist yet has nothing to
/// canonicalize — the lexical check above is authoritative for it, and the
/// filesystem cannot yet be walked through it anyway.
fn validate_project_local_dir(
    config_dir: &Path,
    raw_dir: &str,
) -> Result<PathBuf, DirBoundaryError> {
    let config_path = config_dir.join(".hyalo.toml");
    let err = |reason| DirBoundaryError {
        config_path: config_path.clone(),
        raw_dir: raw_dir.to_owned(),
        reason,
    };

    let raw_norm = raw_dir.replace('\\', "/");
    if Path::new(&raw_norm).is_absolute() || raw_norm.starts_with('/') {
        return Err(err(DirBoundaryReason::Absolute));
    }

    let mut depth: i32 = 0;
    for comp in Path::new(&raw_norm).components() {
        match comp {
            std::path::Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return Err(err(DirBoundaryReason::Escapes));
                }
            }
            std::path::Component::Normal(_) => depth += 1,
            std::path::Component::CurDir => {}
            // `RootDir`/`Prefix` — the `is_absolute`/`starts_with('/')` check
            // above only ever catches a rooted or POSIX-absolute path; a
            // Windows drive-*relative* value like `C:foo` (no `\` after the
            // colon) is NOT `is_absolute()` per `std::path::Path` and reaches
            // this loop as a `Prefix` component instead. This arm is the
            // actual, load-bearing refusal for that case — not a defensive
            // backstop — so do not remove it under the assumption the checks
            // above already cover every platform.
            _ => return Err(err(DirBoundaryReason::Escapes)),
        }
    }

    // The join actually resolved (`..` collapsed away): a bounded round-trip
    // like `"sub/../kb"` lands on `config_dir/kb`, never touching a phantom
    // `sub/` that may not exist on disk.
    let joined = config_dir.join(lexically_normalize_relative(&raw_norm));

    // Defense in depth against a symlink escape: when the target exists,
    // compare canonicalized paths instead of trusting the lexical walk above.
    if let (Ok(canon_dir), Ok(canon_config)) = (
        dunce::canonicalize(&joined),
        dunce::canonicalize(config_dir),
    ) && !canon_dir.starts_with(&canon_config)
    {
        return Err(err(DirBoundaryReason::Escapes));
    }

    Ok(joined)
}

/// Collapse `.` and `..` components of a relative `dir` string without
/// touching the filesystem or joining it onto any base directory.
///
/// [`validate_project_local_dir`] already refused any `dir` that nets above
/// the config directory, so by the time this runs a bounded round-trip like
/// `"sub/../kb"` is known-legal — but left as literal text it would still
/// require the phantom `sub/` to exist on disk purely so the OS can resolve
/// the `..` through it. Collapsing it here makes an allowed round-trip behave
/// exactly like writing `"kb"` directly, matching how every existing `dir`
/// value (which never contained `..`) already behaved.
fn lexically_normalize_relative(raw: &str) -> PathBuf {
    let raw_norm = raw.replace('\\', "/");
    let mut out = PathBuf::new();
    for comp in Path::new(&raw_norm).components() {
        match comp {
            // Already refused before this runs whenever it would net below
            // empty; kept defensive (push the literal `..`) rather than
            // panicking if that invariant is ever violated.
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
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
fn announce_ancestor_config(cwd: &Path, config_dir: &Path, vault: &Path) {
    let same = match (dunce::canonicalize(vault), dunce::canonicalize(cwd)) {
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

/// Load configuration governing `start_dir`, with the ancestor-discovery
/// fallback [`discover_ancestor_config`] describes.
///
/// Resolution order:
/// 1. `.hyalo.toml` in `start_dir` itself.
/// 2. Otherwise the nearest ancestor `.hyalo.toml` whose configured vault
///    contains `start_dir` (see [`discover_ancestor_config`]).
/// 3. Otherwise built-in defaults.
///
/// Returns the resolved config plus the ancestor directory it was adopted
/// from, when resolution went through step 2 (`None` for steps 1 and 3) —
/// callers decide whether and how to announce that on their own terms; this
/// function never prints anything itself.
///
/// NEW-17 (dogfood pre3): originally inlined into [`load_config`] for the
/// real process working directory only. `resolve_effective`'s `--dir
/// <foreign-tree>` branch used to call [`load_config_from`] directly instead
/// — which checks only `<foreign-tree>` itself — so `--dir sub/deep` from the
/// repo root reported "no .hyalo.toml — built-in defaults" for a tree where
/// `cd sub/deep && hyalo …` would have silently adopted the repo-root config.
/// Which `.hyalo.toml` governs a directory must not depend on how the caller
/// named it.
///
/// PR #251 review H1: this used to call [`announce_ancestor_config`] itself,
/// unconditionally, for *every* caller. On the `--dir <foreign-tree>` path
/// that produced a wrong, actively misleading note: `announce_ancestor_config`
/// names the *ancestor's own configured vault* as "the vault" (correct for
/// [`load_config`], where the caller stands *inside* that vault), but
/// `resolve_effective` sets `EffectiveConfig::dir` to the literal `--dir`
/// value, which is typically a narrower subdirectory of that vault, not equal
/// to it — so `hyalo config --dir other/deep/sub` announced "the vault is
/// .../other, not the current directory — pass --dir ." while the run only
/// ever scanned `other/deep/sub`. Worse, when the CWD *also* had its own
/// shadowed config, this fired *alongside* `dir_override_note`'s own
/// (correct) announcement — two notes, the first one wrong. Moving the
/// announcement out lets each caller use the wording that actually matches
/// what it does with the result.
fn load_config_for_dir(start_dir: &Path) -> (ResolvedDefaults, Option<PathBuf>) {
    if start_dir.join(".hyalo.toml").is_file() {
        return (load_config_from(start_dir), None);
    }
    match discover_ancestor_config(start_dir) {
        Some(ancestor) => {
            let mut config = load_config_from(&ancestor);
            // `dir` is stored relative to the config file, and the caller's
            // directory is not necessarily where the file lives — everything
            // downstream resolves the vault against `start_dir`. Absolutize
            // it so the two cannot disagree (`config_dir` still points at the
            // adopted file, so `views set` and friends keep writing to it).
            let vault = ancestor.join(&config.dir);
            config.dir = dunce::canonicalize(&vault).unwrap_or(vault);
            (config, Some(ancestor))
        }
        None => (load_config_from(start_dir), None),
    }
}

/// Load configuration for the current working directory. See
/// [`load_config_for_dir`] for the resolution order.
///
/// Missing file → silent, returns hardcoded defaults.
/// I/O error (not NotFound) → records a diagnostic, returns defaults.
/// Malformed TOML or unknown fields → records a diagnostic, returns defaults.
/// Valid config → merges with defaults (config values take precedence).
pub(crate) fn load_config() -> ResolvedDefaults {
    match std::env::current_dir() {
        Ok(cwd) => {
            let (config, ancestor) = load_config_for_dir(&cwd);
            // The real process CWD stands *inside* whatever vault applies —
            // `announce_ancestor_config`'s "the vault is X, not the current
            // directory" wording is accurate here, unlike the `--dir` path
            // (see `load_config_for_dir`'s doc comment, PR #251 review H1).
            if let Some(ancestor) = ancestor {
                announce_ancestor_config(&cwd, &ancestor, &config.dir);
            }
            config
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

    // H-1 (iter-221): refuse before any other processing — a `dir` outside
    // this file's own boundary makes every other setting in it moot, since
    // the run must not touch the filesystem at all until this is fixed.
    if let Some(raw) = cfg.dir.as_deref()
        && let Err(boundary_err) = validate_project_local_dir(dir, raw)
    {
        return ResolvedDefaults::dir_out_of_bounds_for(dir, boundary_err.diagnostic());
    }

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
        dir: cfg
            .dir
            .as_deref()
            .map(lexically_normalize_relative)
            .unwrap_or(defaults.dir),
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
        dir_salvaged: false,
        lint_profiles,
        dir_out_of_bounds: None,
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
    /// The CWD `.hyalo.toml` that [`Self::cwd_config_shadowed`] refers to,
    /// `None` unless it is `true`.
    ///
    /// NEW-17 (dogfood pre3): [`dir_override_note`] used to hardcode
    /// `./.hyalo.toml` for "the file that no longer applies" instead of
    /// naming it from data. At a config root, `--dir .` reloads that very
    /// file for the new vault (`--dir .` targets the config's own directory,
    /// which does have a `.hyalo.toml` — itself), so the hardcoded half and
    /// the freshly computed [`Self::config_path`] printed the identical path
    /// twice while claiming the first "does not apply" and the second "is in
    /// effect".
    pub(crate) shadowed_config_path: Option<PathBuf>,
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
            shadowed_config_path: None,
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
            shadowed_config_path: None,
        };
    }

    // NEW-17 (dogfood pre3): `load_config_for_dir` (not `load_config_from`)
    // so a foreign `--dir` gets the same ancestor-discovery fallback `cd
    // <dir> && hyalo …` would — otherwise this branch could wrongly report
    // "no .hyalo.toml — built-in defaults" for a directory whose *ancestor*
    // config actually governs it.
    //
    // PR #251 review H1: deliberately silent about *how* the config was
    // found (the discarded `Option<PathBuf>` ancestor). `dir_override_note`
    // below already announces the adopted file correctly, naming the real
    // effective vault (`cli_dir`) — a second, ancestor-discovery-specific
    // announcement here would either duplicate it or (worse) describe the
    // ancestor's own configured vault as "the vault", which is wrong once
    // `--dir` narrows the run to a subdirectory of it.
    let (target, _adopted_from) = load_config_for_dir(cli_dir);
    // The config actually governing `target` may live in an ancestor of
    // `cli_dir`, not `cli_dir` itself — read it from `target.config_dir`
    // (which `load_config_for_dir` sets to wherever the file was actually
    // found) rather than re-deriving it from `cli_dir`, and canonicalize so
    // this note prints absolute like the ancestor-adoption note does.
    let target_config_dir =
        dunce::canonicalize(&target.config_dir).unwrap_or_else(|_| target.config_dir.clone());
    let target_config_path = config_file_in(&target_config_dir);
    // Canonicalize the shadowed CWD config path too so a self-reference (see
    // `shadowed_config_path`'s doc comment) is caught by equality, not just
    // by coincidentally matching relative-path text.
    let shadowed_config_path = cwd_config_path
        .as_deref()
        .map(|p| dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()));
    EffectiveConfig {
        config: target,
        dir: cli_dir.to_path_buf(),
        config_path: target_config_path,
        dir_redundant: false,
        cwd_config_shadowed: shadowed_config_path.is_some(),
        shadowed_config_path,
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
    if let Some(diagnostic) = effective.config.dir_out_of_bounds.as_deref() {
        crate::warn::warn_always(diagnostic);
    }
}

pub(crate) fn dir_override_note(effective: &EffectiveConfig) -> Option<String> {
    if !effective.cwd_config_shadowed {
        return None;
    }
    let dir = effective.dir.display();
    // NEW-17 (dogfood pre3): the shadowed file used to be hardcoded as the
    // literal text `./.hyalo.toml`, so `--dir .` at a config root — which
    // reloads that very file as `config_path` — printed the identical path
    // twice while claiming the first half "does not apply" and the second
    // "is in effect". Name the shadowed file from data, and when it turns
    // out to be the very file still in effect, say that instead of
    // contradicting the line right after it.
    let shadowed = effective
        .shadowed_config_path
        .as_deref()
        .map_or_else(|| "./.hyalo.toml".to_owned(), |p| p.display().to_string());
    if effective.config_path.as_deref() == effective.shadowed_config_path.as_deref() {
        return Some(format!(
            "--dir {dir} selects a different vault: {shadowed} is still in effect (it lives in \
             {dir}) but its own `dir` setting no longer applies to this run"
        ));
    }
    Some(match &effective.config_path {
        Some(path) => format!(
            "--dir {dir} selects a different vault: {shadowed} does not apply, \
             {} is in effect",
            path.display()
        ),
        None => format!(
            "--dir {dir} selects a different vault: {shadowed} does not apply and \
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
        // NEW-17 (dogfood pre3): `config_path` is now canonicalized so this
        // note prints absolute like the ancestor-adoption note does — compare
        // against the canonicalized form too, since a tempdir path can itself
        // cross a symlink (e.g. macOS `/var` -> `/private/var`).
        let expected = dunce::canonicalize(other.join(".hyalo.toml")).unwrap();
        assert_eq!(effective.config_path, Some(expected));
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

    /// NEW-17 (dogfood pre3): `--dir .` at the config's own root used to
    /// print `./.hyalo.toml does not apply, ./.hyalo.toml is in effect` — the
    /// identical literal path on both halves of one contradictory sentence.
    /// The repro: a config root (`dir = "kb"`) is asked to treat *itself*
    /// (not `kb/`) as the vault. That is a different vault than the
    /// configured one, but the `.hyalo.toml` governing it is the very same
    /// file — the note must say so, not contradict itself.
    #[test]
    fn dir_naming_the_config_root_itself_does_not_contradict_itself() {
        let project = make_project();
        // `--dir <config-root>` names the directory the `.hyalo.toml` itself
        // lives in — not the `kb` vault it configures — so `same_vault` is
        // false and this hits the "different vault" branch, but the file
        // `load_config_for_dir` finds there is the identical physical file
        // `cwd_config` was already loaded from.
        let effective = resolve_effective(load_config_from(project.path()), Some(project.path()));
        assert!(effective.cwd_config_shadowed);

        let note = dir_override_note(&effective).expect("the switch must be announced");
        assert!(
            !(note.contains("does not apply") && note.contains("is in effect")),
            "note must not claim the same file both does not apply and is in effect: {note}"
        );
        assert!(
            note.contains("is still in effect"),
            "note should say the file is still the one governing this run: {note}"
        );
    }

    /// NEW-17 (dogfood pre3): `--dir <foreign-tree>` used to call
    /// `load_config_from` directly, which only checks `<foreign-tree>` itself
    /// — so a subdirectory of an unrelated tree with its *own* ancestor
    /// config reported "no .hyalo.toml — built-in defaults", even though `cd
    /// <foreign-tree> && hyalo …` would have adopted that ancestor config.
    #[test]
    fn dir_naming_a_foreign_subdir_adopts_its_own_ancestor_config() {
        let dir = make_temp();
        // An unrelated tree, `other/`, with its own root config and a deep
        // subdirectory that carries no `.hyalo.toml` of its own.
        fs::create_dir_all(dir.path().join("other/deep/sub")).unwrap();
        fs::write(
            dir.path().join("other/.hyalo.toml"),
            "dir = \".\"\nsite_prefix = \"adopted\"\n",
        )
        .unwrap();

        let cwd_config = load_config_from(dir.path()); // no config at `dir` itself
        let effective = resolve_effective(cwd_config, Some(&dir.path().join("other/deep/sub")));

        assert_eq!(
            effective.config.site_prefix.as_deref(),
            Some("adopted"),
            "the foreign subdir's own ancestor config must be adopted, not built-in defaults; \
             malformed: {:?}",
            effective.config.malformed
        );
        assert!(
            effective.config_path.is_some(),
            "config_path must name the adopted ancestor file, not report no config"
        );
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

    // -----------------------------------------------------------------------
    // iter-213 — ancestor discovery and the index-mismatch summary
    // -----------------------------------------------------------------------

    #[test]
    fn ancestor_discovery_adopts_a_config_whose_vault_contains_cwd() {
        let project = make_project(); // dir = "kb"
        let vault = project.path().join("kb");
        fs::create_dir_all(&vault).unwrap();
        assert_eq!(
            discover_ancestor_config(&vault),
            Some(dunce::canonicalize(project.path()).unwrap()),
            "a config whose vault is CWD governs the run"
        );
    }

    #[test]
    fn ancestor_discovery_reaches_a_deeper_subdirectory() {
        let project = make_project();
        let nested = project.path().join("kb/iterations");
        fs::create_dir_all(&nested).unwrap();
        assert_eq!(
            discover_ancestor_config(&nested),
            Some(dunce::canonicalize(project.path()).unwrap())
        );
    }

    #[test]
    fn ancestor_discovery_skips_a_config_whose_vault_excludes_cwd() {
        let project = make_project();
        let sibling = project.path().join("elsewhere");
        fs::create_dir_all(&sibling).unwrap();
        fs::create_dir_all(project.path().join("kb")).unwrap();
        assert_eq!(
            discover_ancestor_config(&sibling),
            None,
            "a config pointing at another tree does not govern this one"
        );
    }

    #[test]
    fn ancestor_discovery_stops_at_the_nearest_config() {
        // outer/.hyalo.toml (dir = ".") would contain everything, but the
        // nearer inner/.hyalo.toml points elsewhere — nearest wins, so nothing
        // is adopted rather than the walk continuing to the outer file.
        let outer = make_temp();
        fs::write(outer.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
        let inner = outer.path().join("inner");
        fs::create_dir_all(inner.join("vault")).unwrap();
        fs::write(inner.join(".hyalo.toml"), "dir = \"vault\"\n").unwrap();
        let cwd = inner.join("other");
        fs::create_dir_all(&cwd).unwrap();
        assert_eq!(discover_ancestor_config(&cwd), None);
    }

    #[test]
    fn ancestor_discovery_adopts_an_unparseable_config_so_it_is_surfaced() {
        // No usable `dir`, so the config directory itself is the vault — which
        // contains CWD, so the file is adopted and its diagnostic reported.
        let project = make_temp();
        fs::write(project.path().join(".hyalo.toml"), "not = = toml\n").unwrap();
        let cwd = project.path().join("sub");
        fs::create_dir_all(&cwd).unwrap();
        assert_eq!(
            discover_ancestor_config(&cwd),
            Some(dunce::canonicalize(project.path()).unwrap())
        );
    }

    #[test]
    fn index_mismatch_summary_names_only_the_differing_field() {
        assert_eq!(
            index_mismatch_summary("/v", "/v", Some("en-us"), None),
            "site prefix: index 'en-us' vs run (none)",
            "an identical vault path must not be printed twice"
        );
        assert_eq!(
            index_mismatch_summary("/a", "/b", Some("p"), Some("p")),
            "vault: index '/a' vs run '/b'"
        );
        assert_eq!(
            index_mismatch_summary("/a", "/b", None, Some("p")),
            "vault: index '/a' vs run '/b'; site prefix: index (none) vs run 'p'"
        );
    }

    #[test]
    fn index_mismatch_summary_never_renders_a_rust_option() {
        let summary = index_mismatch_summary("/v", "/v", Some("en-us"), None);
        assert!(!summary.contains("Some("), "{summary}");
        assert!(!summary.contains("None"), "{summary}");
    }

    #[test]
    fn missing_config_returns_defaults() {
        let dir = make_temp();
        let resolved = load_config_from(dir.path());
        assert_eq!(resolved, ResolvedDefaults::defaults_for(dir.path()));
    }

    // -------------------------------------------------------------------
    // PR #253 review — finding 1: the symlink defense-in-depth check must
    // canonicalize the *normalized* join, not the raw one still carrying a
    // phantom `..`-preceding segment.
    // -------------------------------------------------------------------

    /// `docs/.hyalo.toml` sets `dir = "phantom/../link"` — an allowed
    /// bounded round-trip lexically (`phantom/../link` nets to `link`, which
    /// exists) — but `docs/link` is a symlink escaping the config directory.
    /// Before the fix, canonicalizing the *raw* join (`docs/phantom/../link`)
    /// failed with ENOENT because `docs/phantom/` was never created, so the
    /// symlink check silently no-opped and the escape passed. After the fix,
    /// canonicalizing the *normalized* join (`docs/link`) resolves through
    /// the symlink and is refused.
    #[test]
    #[cfg(unix)]
    fn symlink_escape_through_a_bounded_round_trip_dir_is_refused() {
        let tmp = make_temp();
        let docs = tmp.path().join("docs");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&docs).unwrap();
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, docs.join("link")).unwrap();

        let err = validate_project_local_dir(&docs, "phantom/../link")
            .expect_err("a round-trip dir landing on a symlink that escapes docs/ must refuse");
        assert_eq!(err.reason, DirBoundaryReason::Escapes);
    }

    /// The same round-trip form pointed at a real, in-bounds subdirectory
    /// (no symlink involved) must still be allowed — the fix must not turn
    /// every round-trip into a refusal, only ones that physically escape.
    #[test]
    fn symlink_free_bounded_round_trip_dir_is_still_allowed() {
        let tmp = make_temp();
        fs::create_dir_all(tmp.path().join("kb")).unwrap();
        let resolved = validate_project_local_dir(tmp.path(), "phantom/../kb")
            .expect("a round-trip dir with no symlink involved must be allowed");
        assert_eq!(resolved, tmp.path().join("kb"));
    }

    // -------------------------------------------------------------------
    // PR #253 review — finding 2: the diagnostic must not carry raw control
    // bytes from an attacker-controlled `dir` value onto the terminal.
    // -------------------------------------------------------------------

    #[test]
    fn diagnostic_strips_embedded_escape_sequences() {
        let tmp = make_temp();
        // An absolute path (guaranteed refused) carrying a raw ESC (0x1B) CSI
        // sequence, as a hostile `.hyalo.toml` might embed to manipulate the
        // victim's terminal. `raw_dir` is interpolated via `{:?}` (Debug),
        // which already escapes control bytes to `\u{...}` text on its own —
        // this test mainly guards against that formatting choice changing.
        let hostile = "/\u{1b}[31mFAKE ERROR\u{1b}[0m";
        let err = validate_project_local_dir(tmp.path(), hostile).unwrap_err();
        assert_eq!(err.reason, DirBoundaryReason::Absolute);
        let diagnostic = err.diagnostic();
        assert!(
            !diagnostic.contains('\u{1b}'),
            "the diagnostic must not carry a raw ESC byte: {diagnostic:?}"
        );
    }

    /// The vector `raw_dir`'s `{:?}` (Debug) formatting does not cover:
    /// `config_path` is interpolated via `{}` (Display), which does *not*
    /// escape control bytes. A repo can name a directory with an embedded
    /// ESC sequence (arbitrary bytes are legal in a Unix filename), so
    /// `config_path` is exactly as attacker-controlled as `dir` itself once
    /// the victim clones and `cd`s into it. Constructs the error directly
    /// rather than via a real symlinked/oddly-named directory, since only
    /// the string content — not actual filesystem behavior — is under test
    /// here (PR #253 review, Copilot finding 2).
    #[test]
    fn diagnostic_strips_escape_sequences_from_the_config_path_too() {
        let err = DirBoundaryError {
            config_path: PathBuf::from("/tmp/\u{1b}[31mFAKE\u{1b}[0m/.hyalo.toml"),
            raw_dir: "..".to_owned(),
            reason: DirBoundaryReason::Escapes,
        };
        let diagnostic = err.diagnostic();
        assert!(
            !diagnostic.contains('\u{1b}'),
            "the diagnostic must not carry a raw ESC byte from the config path either: {diagnostic:?}"
        );
    }

    // -------------------------------------------------------------------
    // PR #253 review — finding 3: the component-loop catch-all is the real
    // refusal for a Windows drive-relative `dir`, not a defensive backstop.
    // -------------------------------------------------------------------

    /// `C:foo` (no `\` after the colon) is drive-*relative*, not absolute —
    /// `Path::is_absolute()` returns `false` for it on Windows, unlike the
    /// already-rejected `C:\foo`. Only the component loop's `Prefix` catch-all
    /// arm refuses it. Gated to Windows because `std::path::Path` only parses
    /// a leading `C:` as a `Prefix` component under `#[cfg(windows)]`; on
    /// Unix `"C:foo"` is just an ordinary `Normal("C:foo")` path segment, so
    /// this input is not meaningful to test off Windows. CI runs
    /// `windows-latest`, so this executes for real rather than being
    /// permanently skipped.
    #[test]
    #[cfg(windows)]
    fn windows_drive_relative_dir_is_refused() {
        let tmp = make_temp();
        let err = validate_project_local_dir(tmp.path(), "C:foo")
            .expect_err("a drive-relative dir must be refused even though it is not is_absolute()");
        assert_eq!(err.reason, DirBoundaryReason::Escapes);
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
        // The diagnostic is recorded here and printed by
        // `emit_config_diagnostics` once `--dir` resolution has settled which
        // config governs the run (iter-213, UX-5).
        crate::warn::reset_for_test();
        crate::warn::init(false);
        emit_config_diagnostics(&resolve_effective(resolved, None));
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
