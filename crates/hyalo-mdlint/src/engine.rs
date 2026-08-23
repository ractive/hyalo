//! Engine factory and rule catalog.
//!
//! Builds a `LintEngine` that combines `StandardRuleProvider` from
//! `mdbook-lint-rulesets` with the two HYALO native rules.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use mdbook_lint_core::{Document, LintEngine, PluginRegistry};
use mdbook_lint_rulesets::StandardRuleProvider;

use crate::config::LintConfig;
use crate::{DiagFix, DiagSeverity, Diagnostic};

// ---------------------------------------------------------------------------
// Static tables
// ---------------------------------------------------------------------------

/// Hyalo-controlled severity table. Keys that are absent fall through to
/// `Warn` (a safe default). The upstream severity is ignored; we own it.
static SEVERITY_TABLE: &[(&str, DiagSeverity)] = &[
    // Bugs that break rendering
    ("MD001", DiagSeverity::Warn), // heading-increment — structural
    ("MD009", DiagSeverity::Warn), // trailing-spaces
    ("MD010", DiagSeverity::Warn), // no-hard-tabs
    // no-reversed-links — a genuinely reversed link `(text)[url]` breaks
    // rendering, hence error. Known false-positive class: literal regex or
    // math prose that writes `)[` (e.g. a character class `[)]` after a group)
    // can trip the detector. It stays error because the autofix only rewrites a
    // real reversed-link shape; when it misfires on regex text, disable it for
    // that file via `[lint.rules] MD011 = false` or fence the sample as code.
    ("MD011", DiagSeverity::Error),
    ("MD012", DiagSeverity::Warn),  // no-multiple-blanks
    ("MD018", DiagSeverity::Warn),  // no-missing-space-atx
    ("MD019", DiagSeverity::Warn),  // no-multiple-space-atx
    ("MD022", DiagSeverity::Warn),  // blanks-around-headings
    ("MD023", DiagSeverity::Warn),  // headings-start-left
    ("MD031", DiagSeverity::Warn),  // blanks-around-fences
    ("MD034", DiagSeverity::Warn),  // no-bare-urls
    ("MD040", DiagSeverity::Warn),  // fenced-code-language
    ("MD042", DiagSeverity::Error), // no-empty-links — breaks rendering
    ("MD047", DiagSeverity::Warn),  // single-trailing-newline
    // HYALO native
    ("HYALO001", DiagSeverity::Error),
    ("HYALO002", DiagSeverity::Error),
    ("HYALO003", DiagSeverity::Warn),
    ("HYALO004", DiagSeverity::Warn),
    ("HYALO005", DiagSeverity::Error),
    ("HYALO006", DiagSeverity::Warn),
];

/// Rules that are **default-on** (cheap, structural, low false-positive).
/// All others default to off.
static DEFAULT_ON: &[&str] = &[
    "MD001", "MD009", "MD010", "MD011", "MD012", "MD018", "MD019", "MD022", "MD023", "MD031",
    "MD034", "MD040", "MD042", "MD047", // HYALO rules are always default-on
    "HYALO001", "HYALO002", "HYALO003", "HYALO004", "HYALO005", "HYALO006",
];

// ---------------------------------------------------------------------------
// HYALO rule provider
// ---------------------------------------------------------------------------

/// Information about one rule in the catalog.
#[derive(Debug, Clone)]
pub struct RuleCatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_severity: DiagSeverity,
    pub default_enabled: bool,
    pub autofixable: bool,
    pub source: String,
}

/// A thin wrapper around the upstream `LintEngine` that:
/// 1. Owns the severity-override table and enabled-set logic.
/// 2. Post-processes violations: applies severity overrides, filters disabled rules.
/// 3. Exposes `available_rules()` over the combined stock + HYALO catalog.
pub struct HyaloLintEngine {
    inner: LintEngine,
    catalog: Vec<RuleCatalogEntry>,
}

impl HyaloLintEngine {
    /// Build the full catalog (stock rules only; HYALO rules are added separately).
    fn build_catalog(inner: &LintEngine) -> Vec<RuleCatalogEntry> {
        let default_on: HashSet<&str> = DEFAULT_ON.iter().copied().collect();
        let severity_map: HashMap<&str, DiagSeverity> = SEVERITY_TABLE.iter().copied().collect();

        let mut catalog: Vec<RuleCatalogEntry> = inner
            .available_rules()
            .iter()
            .map(|id| {
                let sev = severity_map.get(id).copied().unwrap_or(DiagSeverity::Warn);
                let enabled = default_on.contains(*id);
                // Retrieve the rule's description from the registry.
                let (name, description) = inner
                    .registry()
                    .get_rule(id)
                    .map_or(("unknown", ""), |r| (r.name(), r.description()));
                let autofixable = inner
                    .registry()
                    .get_rule(id)
                    .is_some_and(mdbook_lint_core::rule::Rule::can_fix);
                RuleCatalogEntry {
                    id: id.to_string(),
                    name: name.to_owned(),
                    description: description.to_owned(),
                    default_severity: sev,
                    default_enabled: enabled,
                    autofixable,
                    source: "mdbook-lint-rulesets".to_owned(),
                }
            })
            .collect();

        // Add HYALO entries (they are linted separately, not through mdbook-lint-core).
        let hyalo_entries = [
            RuleCatalogEntry {
                id: "HYALO001".to_owned(),
                name: "bare-checkbox".to_owned(),
                description: "Bare `[]` should be written as `- [ ]`".to_owned(),
                default_severity: DiagSeverity::Error,
                default_enabled: true,
                autofixable: true,
                source: "hyalo-mdlint".to_owned(),
            },
            RuleCatalogEntry {
                id: "HYALO002".to_owned(),
                name: "completed-tasks".to_owned(),
                description: "`status: completed` requires all task checkboxes to be ticked"
                    .to_owned(),
                default_severity: DiagSeverity::Error,
                default_enabled: true,
                autofixable: false,
                source: "hyalo-mdlint".to_owned(),
            },
            RuleCatalogEntry {
                id: "HYALO003".to_owned(),
                name: "date-format".to_owned(),
                description: "Date-typed frontmatter key has a value that is not a valid ISO 8601 date (YYYY-MM-DD)".to_owned(),
                default_severity: DiagSeverity::Warn,
                default_enabled: true,
                autofixable: false,
                source: "hyalo-mdlint".to_owned(),
            },
            RuleCatalogEntry {
                id: "HYALO004".to_owned(),
                name: "datetime-format".to_owned(),
                description: "Schema-declared datetime property has a value that is not a valid ISO 8601 datetime (YYYY-MM-DDThh:mm:ss)".to_owned(),
                default_severity: DiagSeverity::Warn,
                default_enabled: true,
                autofixable: false,
                source: "hyalo-mdlint".to_owned(),
            },
            RuleCatalogEntry {
                id: "HYALO005".to_owned(),
                name: "frontmatter-parse-error".to_owned(),
                description: "Frontmatter could not be parsed (invalid YAML, duplicate keys, oversized scalar). The file is otherwise invisible to lint, so this is an error by default and cannot be silently downgraded by a profile.".to_owned(),
                default_severity: DiagSeverity::Error,
                default_enabled: true,
                autofixable: false,
                source: "hyalo-mdlint".to_owned(),
            },
            RuleCatalogEntry {
                id: "HYALO006".to_owned(),
                name: "broken-link".to_owned(),
                description: "A wikilink or markdown link points at a vault file that does not exist. Checks the link TARGET only — a broken `#heading` anchor is not flagged here; use `find --broken-links` for anchors. Warns by default; promoted to error under --strict so CI can gate broken links.".to_owned(),
                default_severity: DiagSeverity::Warn,
                default_enabled: true,
                autofixable: false,
                source: "hyalo-mdlint".to_owned(),
            },
        ];
        catalog.extend_from_slice(&hyalo_entries);

        // OKF conformance-profile rules. These only *execute* under
        // `hyalo lint --profile okf` (or a vault whose `.hyalo.toml` sets
        // `[lint] profile = "okf"`); the CLI gates them at runtime. They are
        // listed here so `lint-rules list` / `--rule-prefix OKF` see them and
        // `[lint.rules.OKF-*]` overrides round-trip. `default_enabled = true`
        // means "on when the profile is active" — so `lint-rules set
        // OKF-* --enabled false` writes a real override that suppresses the rule
        // under the profile (a `false == default` set would be a silent no-op).
        // They are advisory (warn) per the OKF permissive-consumption model:
        // SPEC §9 says a consumer MUST NOT reject on broken links or
        // reserved-file structure.
        let okf_entries = [
            (
                "OKF-INDEX-STRUCTURE",
                "okf-index-structure",
                "Reserved `index.md` should be a Markdown link list (OKF §6)",
            ),
            (
                "OKF-INDEX-MARKERS",
                "okf-index-markers",
                "Reserved `index.md` managed-region markers should be a single well-formed begin/end pair (not dangling/reversed/duplicate)",
            ),
            (
                "OKF-LOG-STRUCTURE",
                "okf-log-structure",
                "Reserved `log.md` should be date-grouped, newest first (OKF §7)",
            ),
            (
                "OKF-CITATIONS-PRESENT",
                "okf-citations-present",
                "Claim-bearing concept doc should have a `# Citations` section (OKF §8)",
            ),
            (
                "OKF-CITATIONS-WELL-FORMED",
                "okf-citations-well-formed",
                "`# Citations` entries should be a list of links, not free prose (OKF §8)",
            ),
            (
                "OKF-CITATIONS-RESOLVE",
                "okf-citations-resolve",
                "Bundle-relative / `references/` citation links should resolve to a file",
            ),
            (
                "OKF-AUGMENTATION-GUARD",
                "okf-augmentation-guard",
                "`# Schema` / `# Citations` sections should not be present-but-empty",
            ),
        ];
        for (id, name, description) in okf_entries {
            catalog.push(RuleCatalogEntry {
                id: id.to_owned(),
                name: name.to_owned(),
                description: description.to_owned(),
                default_severity: DiagSeverity::Warn,
                default_enabled: true,
                autofixable: false,
                source: "hyalo-mdlint (okf profile)".to_owned(),
            });
        }

        // MADR conformance-profile rules. Same gating model as the OKF rules
        // above: listed here so `lint-rules list` / `--rule-prefix MADR` see them
        // and `[lint.rules.MADR-*]` overrides round-trip, but they only execute
        // under `hyalo lint --profile madr` (or `[lint] profile = "madr"`). Both
        // are advisory (warn): a dangling supersede or duplicate number is a
        // smell, not a hard error.
        let madr_entries = [
            (
                "MADR-SUPERSEDE-RESOLVE",
                "madr-supersede-resolve",
                "`status: superseded by ADR-NNNN` should point at an existing ADR file",
            ),
            (
                "MADR-DUPLICATE-NUMBER",
                "madr-duplicate-number",
                "Two ADR files in a directory should not share the same `NNNN` number",
            ),
        ];
        for (id, name, description) in madr_entries {
            catalog.push(RuleCatalogEntry {
                id: id.to_owned(),
                name: name.to_owned(),
                description: description.to_owned(),
                default_severity: DiagSeverity::Warn,
                default_enabled: true,
                autofixable: false,
                source: "hyalo-mdlint (madr profile)".to_owned(),
            });
        }

        // Agent Skills conformance-profile rules. Same gating model as the OKF
        // and MADR rules above: listed here so `lint-rules list` /
        // `--rule-prefix SKILL` see them and `[lint.rules.SKILL-*]` overrides
        // round-trip, but they only execute under `hyalo lint --profile skills`
        // (or `[lint] profile = "skills"`). A reserved `name` is a hard spec
        // violation (error); the dirname mismatch and over-budget body are
        // smells, not hard errors, so they default to warn — the hard `name`
        // regex/length and `description` length constraints are the schema's
        // job (see `hyalo-cli/templates/profile-skills.toml`).
        let skill_entries = [
            (
                "SKILL-RESERVED-NAME",
                "skill-reserved-name",
                "A skill's `name` must not be a reserved word (`anthropic` / `claude`)",
                DiagSeverity::Error,
            ),
            (
                "SKILL-NAME-DIRNAME",
                "skill-name-dirname",
                "A skill's `name` should equal its parent directory (`<name>/SKILL.md`)",
                DiagSeverity::Warn,
            ),
            (
                "SKILL-LINE-BUDGET",
                "skill-line-budget",
                "A SKILL.md body should stay under 500 lines (move detail into `references/`)",
                DiagSeverity::Warn,
            ),
        ];
        for (id, name, description, default_severity) in skill_entries {
            catalog.push(RuleCatalogEntry {
                id: id.to_owned(),
                name: name.to_owned(),
                description: description.to_owned(),
                default_severity,
                default_enabled: true,
                autofixable: false,
                source: "hyalo-mdlint (skills profile)".to_owned(),
            });
        }

        // Keep a Changelog 1.1.0 conformance-profile rules. Same gating model as
        // the OKF / MADR / skills rules above: listed here so `lint-rules list` /
        // `--rule-prefix CHANGELOG` see them and `[lint.rules.CHANGELOG-*]`
        // overrides round-trip, but they only execute under
        // `hyalo lint --profile changelog` (or `[lint] profile = "changelog"`).
        // The changelog grammar is stricter than the other profiles: a malformed
        // changelog is a real defect, so the grammar/ordering rules default to
        // *error*; the two soft rules (empty section, footer link-ref
        // cross-check) are advisory (warn). Per-rule severity is the source of
        // truth — see each entry's `default_severity` below.
        let changelog_entries = [
            (
                "CHANGELOG-TITLE",
                "changelog-title",
                "A changelog must start with a `# Changelog` H1",
                DiagSeverity::Error,
            ),
            (
                "CHANGELOG-VERSION-HEADING",
                "changelog-version-heading",
                "H2 version headings must be `[Unreleased]` or `[X.Y.Z] - YYYY-MM-DD`",
                DiagSeverity::Error,
            ),
            (
                "CHANGELOG-CATEGORY",
                "changelog-category",
                "H3 headings must be one of Added/Changed/Deprecated/Removed/Fixed/Security",
                DiagSeverity::Error,
            ),
            (
                "CHANGELOG-VERSION-ORDER",
                "changelog-version-order",
                "Versions must be strictly descending (newest first)",
                DiagSeverity::Error,
            ),
            (
                "CHANGELOG-DATE-ORDER",
                "changelog-date-order",
                "Release dates must be non-increasing (newest first)",
                DiagSeverity::Error,
            ),
            (
                "CHANGELOG-UNRELEASED-POSITION",
                "changelog-unreleased-position",
                "`## [Unreleased]` must be the first version section",
                DiagSeverity::Error,
            ),
            (
                "CHANGELOG-EMPTY-SECTION",
                "changelog-empty-section",
                "A released or category section should not be empty",
                DiagSeverity::Warn,
            ),
            (
                "CHANGELOG-LINK-REF",
                "changelog-link-ref",
                "Every version heading needs a matching footer link reference and vice versa",
                DiagSeverity::Warn,
            ),
        ];
        for (id, name, description, default_severity) in changelog_entries {
            catalog.push(RuleCatalogEntry {
                id: id.to_owned(),
                name: name.to_owned(),
                description: description.to_owned(),
                default_severity,
                default_enabled: true,
                autofixable: false,
                source: "hyalo-mdlint (changelog profile)".to_owned(),
            });
        }
        catalog
    }

    /// Create the engine by registering `StandardRuleProvider` with the plugin registry.
    ///
    /// Deviation from plan: we do NOT use HYALO rules through mdbook-lint-core's `Rule`
    /// trait here — they are executed separately in `lint_body()` to avoid the overhead
    /// of constructing per-file `Rule` instances through the registry. The catalog still
    /// includes them so `available_rules()` / `lint-rules list` shows them.
    pub fn create() -> Result<Self> {
        let mut registry = PluginRegistry::new();
        registry
            .register_provider(Box::new(StandardRuleProvider))
            .context("registering StandardRuleProvider")?;
        let inner = registry.create_engine().context("creating LintEngine")?;
        let catalog = Self::build_catalog(&inner);
        Ok(Self { inner, catalog })
    }

    /// All available rule IDs (stock + HYALO).
    pub fn available_rules(&self) -> &[RuleCatalogEntry] {
        &self.catalog
    }

    /// Look up a single rule entry by ID.
    pub fn rule_entry(&self, id: &str) -> Option<&RuleCatalogEntry> {
        self.catalog.iter().find(|e| e.id == id)
    }

    /// Look up a single rule entry by ID, falling back to a case-insensitive
    /// match when the exact spelling is unknown.
    ///
    /// Rule ids are conventionally upper-case (`MD013`, `HYALO006`), but
    /// hand-typed filters routinely arrive lower-cased. Callers that only need
    /// to *select* a rule (`hyalo lint --rule`) should use this and then
    /// canonicalize to [`RuleCatalogEntry::id`]; callers that *write* the id
    /// into config keep using the exact [`Self::rule_entry`].
    pub fn rule_entry_ci(&self, id: &str) -> Option<&RuleCatalogEntry> {
        self.rule_entry(id)
            .or_else(|| self.catalog.iter().find(|e| e.id.eq_ignore_ascii_case(id)))
    }

    /// Every rule id whose spelling starts with `prefix`, case-insensitively.
    ///
    /// Backs `hyalo lint --rule-prefix`, which selects a rule family rather
    /// than a single rule; an empty result means the filter matches nothing.
    pub fn rules_matching_prefix_ci(&self, prefix: &str) -> Vec<&RuleCatalogEntry> {
        let upper = prefix.to_ascii_uppercase();
        self.catalog
            .iter()
            .filter(|e| e.id.to_ascii_uppercase().starts_with(&upper))
            .collect()
    }

    /// Check HYALO003 (date-format) against the parsed frontmatter properties.
    ///
    /// Returns a `Vec<Diagnostic>` (zero or more) that can be merged with
    /// the caller's violations.  Respects the user's rule config (enabled/severity).
    pub fn lint_frontmatter_hyalo003(
        &self,
        _rel_path: &str,
        properties: &indexmap::IndexMap<String, serde_json::Value>,
        config: &LintConfig,
        rule_filter: &[String],
        strict: bool,
    ) -> Vec<Diagnostic> {
        use crate::rules::hyalo003::check_date_keys;

        // Is the rule enabled?
        let enabled = if let Some(ov) = config.rules.get("HYALO003")
            && let Some(b) = ov.enabled()
        {
            b
        } else {
            DEFAULT_ON.contains(&"HYALO003")
        };
        if !enabled {
            return vec![];
        }

        // Is it included in the rule filter?
        if !rule_filter.is_empty() && !rule_filter.iter().any(|r| r == "HYALO003") {
            return vec![];
        }

        // Effective severity: user override → strict promotion → SEVERITY_TABLE default.
        let sev = if let Some(ov) = config.rules.get("HYALO003")
            && let Some(sev_str) = ov.severity()
        {
            match sev_str {
                "error" => DiagSeverity::Error,
                _ => DiagSeverity::Warn,
            }
        } else if strict {
            // --strict promotes HYALO003 warnings to errors.
            DiagSeverity::Error
        } else {
            DiagSeverity::Warn
        };

        check_date_keys(properties)
            .into_iter()
            .map(|(key, bad_val)| Diagnostic {
                rule_id: "HYALO003".to_owned(),
                rule_name: "date-format".to_owned(),
                message: format!(
                    "property `{key}` has value {bad_val:?} which is not a valid ISO 8601 date (YYYY-MM-DD)"
                ),
                line: 1,
                column: 1,
                severity: sev,
                fix: None,
            })
            .collect()
    }

    /// Check HYALO004 (datetime-format) against schema-declared datetime
    /// properties present in frontmatter.
    ///
    /// The caller is responsible for filtering `properties` against the
    /// effective schema so that only schema-declared `datetime`/`datetime-tz`
    /// fields with string values are passed in. Each triple is
    /// `(name, value, is_tz)` where `is_tz` marks a `datetime-tz`-typed
    /// property (validated against the tz-aware grammar). Triples whose value
    /// is not a string are ignored (a separate SCHEMA-level violation covers
    /// type mismatches).
    pub fn lint_frontmatter_hyalo004(
        &self,
        _rel_path: &str,
        datetime_pairs: &[(&str, &str, bool)],
        config: &LintConfig,
        rule_filter: &[String],
        strict: bool,
    ) -> Vec<Diagnostic> {
        use crate::rules::hyalo004::check_datetime_properties;

        let enabled = if let Some(ov) = config.rules.get("HYALO004")
            && let Some(b) = ov.enabled()
        {
            b
        } else {
            DEFAULT_ON.contains(&"HYALO004")
        };
        if !enabled {
            return vec![];
        }

        if !rule_filter.is_empty() && !rule_filter.iter().any(|r| r == "HYALO004") {
            return vec![];
        }

        let sev = if let Some(ov) = config.rules.get("HYALO004")
            && let Some(sev_str) = ov.severity()
        {
            match sev_str {
                "error" => DiagSeverity::Error,
                _ => DiagSeverity::Warn,
            }
        } else if strict {
            DiagSeverity::Error
        } else {
            DiagSeverity::Warn
        };

        check_datetime_properties(datetime_pairs)
            .into_iter()
            .map(|(key, bad_val)| Diagnostic {
                rule_id: "HYALO004".to_owned(),
                rule_name: "datetime-format".to_owned(),
                message: format!(
                    "property `{key}` has value {bad_val:?} which is not a valid ISO 8601 datetime (YYYY-MM-DDThh:mm:ss)"
                ),
                line: 1,
                column: 1,
                severity: sev,
                fix: None,
            })
            .collect()
    }

    /// Lint the **body** portion of a file (content after frontmatter).
    ///
    /// # Arguments
    /// - `body_content` — the body text (after `---` frontmatter).
    /// - `rel_path` — vault-relative path (for error messages).
    /// - `frontmatter_status` — extracted from frontmatter (for HYALO002).
    /// - `schema_has_completed` — whether schema declares `status: completed`.
    /// - `config` — user lint configuration.
    /// - `rule_filter` — if non-empty, only run these rule IDs.
    #[allow(clippy::too_many_arguments)]
    pub fn lint_body(
        &self,
        body_content: &str,
        rel_path: &str,
        frontmatter_status: Option<&str>,
        schema_has_completed: bool,
        config: &LintConfig,
        rule_filter: &[String],
    ) -> Result<Vec<Diagnostic>> {
        use crate::rules::hyalo001::Hyalo001;
        use crate::rules::hyalo002::Hyalo002;
        use mdbook_lint_core::rule::Rule;

        let severity_map: HashMap<&str, DiagSeverity> = SEVERITY_TABLE.iter().copied().collect();
        let default_on: HashSet<&str> = DEFAULT_ON.iter().copied().collect();

        let filter_set: HashSet<&str> = rule_filter.iter().map(String::as_str).collect();

        // Helper: is a rule enabled?
        let is_enabled = |rule_id: &str| -> bool {
            if let Some(ov) = config.rules.get(rule_id)
                && let Some(b) = ov.enabled()
            {
                return b;
            }
            default_on.contains(rule_id)
        };

        // Helper: should we run this rule (considering filter + enabled)?
        let should_run = |rule_id: &str| -> bool {
            if !filter_set.is_empty() && !filter_set.contains(rule_id) {
                return false;
            }
            is_enabled(rule_id)
        };

        // Post-process: apply hyalo severity override + user config override.
        let effective_severity = |rule_id: &str| -> DiagSeverity {
            // User config wins.
            if let Some(ov) = config.rules.get(rule_id)
                && let Some(sev_str) = ov.severity()
            {
                return match sev_str {
                    "error" => DiagSeverity::Error,
                    _ => DiagSeverity::Warn,
                };
            }
            severity_map
                .get(rule_id)
                .copied()
                .unwrap_or(DiagSeverity::Warn)
        };

        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        // --- Stock MD rules (through mdbook-lint-core) ---
        let enabled_stock_ids: Vec<&str> = self
            .catalog
            .iter()
            .filter(|e| e.source != "hyalo-mdlint")
            .filter(|e| should_run(&e.id))
            .map(|e| e.id.as_str())
            .collect();

        if !enabled_stock_ids.is_empty() {
            // Create a Document from the body content. We use rel_path for error messages.
            let doc = Document::new(body_content.to_string(), PathBuf::from(rel_path))
                .with_context(|| format!("creating Document for {rel_path}"))?;

            for rule_id in &enabled_stock_ids {
                let Some(rule) = self.inner.registry().get_rule(rule_id) else {
                    continue;
                };
                let violations = rule
                    .check(&doc)
                    .with_context(|| format!("running {rule_id} on {rel_path}"))?;

                let sev = effective_severity(rule_id);
                for v in violations {
                    // Upstream MD047 hard-codes "\n" for the missing-EOF-newline
                    // insertion and never fires on CRLF files with extra
                    // trailing blank lines (see `md047_fix`), so CRLF bodies
                    // still need the local computation. LF bodies take the
                    // upstream fix, which is exact since 0.16.0.
                    let fix = if *rule_id == "MD047" && body_content.contains("\r\n") {
                        md047_fix(body_content)
                    } else {
                        convert_fix(&v, body_content)
                    };
                    // A handful of upstream rules report `column` as a byte
                    // offset, not a Unicode scalar one — see
                    // `BYTE_COLUMN_RULE_IDS` (DEC-073, iter-218 NEW-11).
                    //
                    // `checked_sub` (not `saturating_sub`, review finding
                    // #7): `v.line` is 1-based, so a well-formed diagnostic
                    // never reports 0. If one somehow does, `saturating_sub`
                    // would silently convert the column against line 1's
                    // bytes — the wrong line — instead of the nonexistent
                    // "line 0". `checked_sub` makes that case look up
                    // nothing, so `map_or` falls back to the raw column.
                    let column = if BYTE_COLUMN_RULE_IDS.contains(rule_id) {
                        v.line
                            .checked_sub(1)
                            .and_then(|i| doc.lines.get(i))
                            .map_or(v.column, |line| byte_col_to_scalar_col(line, v.column))
                    } else {
                        v.column
                    };
                    diagnostics.push(Diagnostic {
                        rule_id: rule_id.to_string(),
                        rule_name: v.rule_name.clone(),
                        message: v.message.clone(),
                        line: v.line,
                        column,
                        severity: sev,
                        fix,
                    });
                }
            }
        }

        // --- HYALO001 ---
        if should_run("HYALO001") {
            let doc = Document::new(body_content.to_string(), PathBuf::from(rel_path))
                .with_context(|| format!("creating Document for HYALO001 on {rel_path}"))?;
            let sev = effective_severity("HYALO001");
            let violations = Hyalo001
                .check(&doc)
                .with_context(|| format!("running HYALO001 on {rel_path}"))?;
            for v in violations {
                let fix = convert_fix(&v, body_content);
                diagnostics.push(Diagnostic {
                    rule_id: "HYALO001".to_owned(),
                    rule_name: v.rule_name.clone(),
                    message: v.message.clone(),
                    line: v.line,
                    column: v.column,
                    severity: sev,
                    fix,
                });
            }
        }

        // --- HYALO002 (completed-tasks; renamed from HYALO003 in iter-127) ---
        if should_run("HYALO002") {
            let doc = Document::new(body_content.to_string(), PathBuf::from(rel_path))
                .with_context(|| format!("creating Document for HYALO002 on {rel_path}"))?;
            let rule = Hyalo002::new(schema_has_completed, frontmatter_status.map(str::to_owned));
            let sev = effective_severity("HYALO002");
            let violations = rule
                .check(&doc)
                .with_context(|| format!("running HYALO002 on {rel_path}"))?;
            for v in violations {
                diagnostics.push(Diagnostic {
                    rule_id: "HYALO002".to_owned(),
                    rule_name: v.rule_name.clone(),
                    message: v.message.clone(),
                    line: v.line,
                    column: v.column,
                    severity: sev,
                    fix: None,
                });
            }
        }

        Ok(diagnostics)
    }
}

/// Rule IDs whose upstream `Violation.column` (the *reported* diagnostic
/// position, distinct from the `Fix` range `convert_fix` handles above) is
/// computed from a byte offset rather than a Unicode-scalar one.
///
/// Confirmed by reading each rule's source in `mdbook-lint-rulesets` 0.16.0
/// and reproducing on a multibyte fixture (iter-218, dogfood NEW-11):
/// MD010 (`line.find('\t')`, a byte offset) and MD042 (comrak AST
/// `sourcepos.start.column`, byte-based) are both default-on; MD052
/// (`self.pos - self.line_start`, a byte cursor into `self.input: &[u8]`) is
/// opt-in but wrong the same way once enabled. Rules that already index a
/// `Vec<char>` (MD009, MD011, MD034) or only ever report column 1 or an
/// ASCII-whitespace-prefix length (MD001, MD012, MD018, MD019, MD022, MD023,
/// MD031, MD040, MD047) are unaffected and must not be added here — passing
/// an already-scalar column through [`byte_col_to_scalar_col`] would corrupt
/// it on any line with multibyte content before the flagged position.
///
/// Not filed upstream (unlike the CRLF gap `md047_fix` compensates for) — the
/// three rules here each compute the byte offset a different way, so there is
/// no single upstream fix to track. Re-check this table on every
/// `mdbook-lint-rulesets` version bump: if a rule listed here switches to a
/// `Vec<char>`/scalar computation upstream (as MD011/MD034 already do) and
/// stays in this list, its column gets converted twice and silently comes
/// out wrong again — the opposite failure mode from the one this table
/// fixes, and just as invisible without a test. The
/// `md010_reports_byte_column_before_conversion` test below pins the
/// pre-conversion assumption for MD010 so a silent upstream fix breaks the
/// test instead of double-converting unnoticed.
const BYTE_COLUMN_RULE_IDS: &[&str] = &["MD010", "MD042", "MD052"];

/// Convert a rule's 1-based **byte** column to a 1-based Unicode-scalar
/// column, matching `lint`'s DEC-073 convention.
///
/// `line` must be the exact line text the upstream rule measured against
/// (no terminator) — `Document::lines` splits the same way upstream rules
/// do, via `str::lines()`. A column that does not land on a char boundary
/// (out of range, or inside a multibyte sequence — which should not happen
/// for a byte offset upstream computed honestly, but a corrupted/mismatched
/// input must not panic) falls back to the original byte column rather than
/// silently reporting a wrong one.
fn byte_col_to_scalar_col(line: &str, byte_col_1based: usize) -> usize {
    let byte_offset = byte_col_1based.saturating_sub(1);
    line.get(..byte_offset)
        .map_or(byte_col_1based, |prefix| prefix.chars().count() + 1)
}

/// Convert an upstream `Fix` (line/column [`Position`]s) to a byte-offset
/// [`DiagFix`].
///
/// Since mdbook-lint 0.16.0 (upstream PR #493, "Define exact autofix
/// coordinates") `Fix` ranges are **exact and half-open** and `Position`
/// columns are 1-based **Unicode-scalar** offsets within the line's content,
/// with CRLF treated as atomic and line terminators never included
/// implicitly. `Position::to_byte_offset` is the canonical checked
/// conversion, so this function is a straight translation with no per-rule
/// compensation: the pre-0.16 byte-column allowlist, the MD011 inclusive-end
/// guard, the MD034 Liquid pull-back and the `line_len + 1`
/// replace-vs-insert heuristic all became unnecessary and were deleted in
/// iteration 196.
///
/// A position that cannot be resolved (out-of-range line/column, or an
/// offset inside a CRLF pair) yields `None`; the violation is still reported,
/// just without a fix.
fn convert_fix(v: &mdbook_lint_core::Violation, content: &str) -> Option<DiagFix> {
    let fix = v.fix.as_ref()?;
    let start = fix.start.to_byte_offset(content)?;
    let end = fix.end.to_byte_offset(content)?;
    // Half-open means `start <= end`; anything else is a malformed range and
    // applying it would panic or corrupt the file, so drop the fix instead.
    if end < start {
        return None;
    }
    Some(DiagFix {
        description: fix.description.clone(),
        start,
        end,
        replacement: fix.replacement.clone().unwrap_or_default(),
    })
}

/// Compute a corrected single-pass fix for MD047 (single-trailing-newline)
/// on **CRLF** bodies, bypassing upstream's own `Fix` positions.
///
/// mdbook-lint 0.16.0 fixed the LF range arithmetic (upstream #486/#493), so
/// LF bodies now go through [`convert_fix`] unchanged. Two CRLF gaps remain
/// in the shipped 0.16.0 crate, both in `mdbook-lint-rulesets`
/// `src/standard/md047.rs`:
///
/// 1. The missing-trailing-newline branch builds
///    `Fix::insertion("Add newline at end of file", "\n", …)` with a
///    hard-coded LF, which would flip the last line of a CRLF file to a bare
///    LF.
/// 2. `check_file_ending` counts trailing terminators with
///    `content.chars().rev().take_while(|&c| c == '\n')`, which stops at the
///    `\r` of the second-to-last CRLF — so a CRLF file with several trailing
///    blank lines counts one terminator and the rule never fires.
///
/// (2) is a detection gap upstream owns; (1) is a fix-output gap this
/// function compensates for. Both are filed upstream as
/// joshrotenberg/mdbook-lint#495; drop this function once a release carrying
/// that fix is picked up. Kept as the documented exception required by
/// iteration 196's acceptance criteria; re-check on the next mdbook-lint
/// bump.
fn md047_fix(body: &str) -> Option<DiagFix> {
    // Match the body's own line-ending style so the fix never flips a CRLF
    // file to LF (or vice versa).
    let nl = if body.contains("\r\n") { "\r\n" } else { "\n" };
    if body.is_empty() {
        return Some(DiagFix {
            description: "Add newline at end of file".to_owned(),
            start: 0,
            end: 0,
            replacement: "\n".to_owned(),
        });
    }
    if !body.ends_with('\n') {
        return Some(DiagFix {
            description: "Add newline at end of file".to_owned(),
            start: body.len(),
            end: body.len(),
            replacement: nl.to_owned(),
        });
    }
    // Count trailing line terminators, treating each CRLF pair as one.
    let bytes = body.as_bytes();
    let mut content_end = bytes.len();
    let mut terminators = 0usize;
    while content_end > 0 && bytes[content_end - 1] == b'\n' {
        content_end -= if content_end >= 2 && bytes[content_end - 2] == b'\r' {
            2
        } else {
            1
        };
        terminators += 1;
    }
    if terminators <= 1 {
        return None; // MD047 would not have fired.
    }
    // Keep the first terminator after the content, drop the rest.
    let first_terminator_len = if bytes.get(content_end) == Some(&b'\r') {
        2
    } else {
        1
    };
    Some(DiagFix {
        description: "Remove extra trailing newlines".to_owned(),
        start: content_end + first_terminator_len,
        end: body.len(),
        replacement: String::new(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_creates_successfully() {
        let engine = HyaloLintEngine::create().unwrap();
        let rules = engine.available_rules();
        assert!(!rules.is_empty());
        // Should include stock rules
        assert!(rules.iter().any(|r| r.id.starts_with("MD")));
        // Should include HYALO rules
        assert!(rules.iter().any(|r| r.id == "HYALO001"));
        assert!(rules.iter().any(|r| r.id == "HYALO002"));
        assert!(rules.iter().any(|r| r.id == "HYALO003"));
        assert!(rules.iter().any(|r| r.id == "HYALO004"));
        // HYALO005 (frontmatter-parse-error) is listed, default-on, and error-severity.
        let h5 = rules
            .iter()
            .find(|r| r.id == "HYALO005")
            .expect("HYALO005 must be in the catalog");
        assert!(h5.default_enabled, "HYALO005 is default-on");
        assert_eq!(
            h5.default_severity,
            DiagSeverity::Error,
            "HYALO005 defaults to error severity"
        );
    }

    #[test]
    fn default_on_rules_are_enabled() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let diagnostics = engine
            .lint_body("trailing spaces   \n", "test.md", None, false, &config, &[])
            .unwrap();
        // MD009 (trailing spaces) is default-on
        assert!(diagnostics.iter().any(|d| d.rule_id == "MD009"));
    }

    #[test]
    fn hyalo001_fires_for_bare_checkbox() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let diagnostics = engine
            .lint_body("[] Open task\n", "test.md", None, false, &config, &[])
            .unwrap();
        assert!(diagnostics.iter().any(|d| d.rule_id == "HYALO001"));
    }

    /// Apply a `DiagFix` to `body`, as `apply_body_fixes` in the CLI would.
    fn apply(body: &str, fix: &DiagFix) -> String {
        let mut out = body.to_owned();
        out.replace_range(fix.start..fix.end, &fix.replacement);
        out
    }

    // --- iter-196: the 0.16 coordinate contract replaces `line_col_to_byte` ---

    #[test]
    fn upstream_position_columns_are_unicode_scalars_not_bytes() {
        use mdbook_lint_core::violation::Position;

        // "café" — 'é' is 2 bytes in UTF-8, so byte and scalar columns diverge
        // partway through the line. Under the 0.16 contract the line ends at
        // scalar column 5, which resolves to byte offset 5.
        let text = "café\n";
        assert_eq!(
            Position { line: 1, column: 5 }.to_byte_offset(text),
            Some(5),
            "scalar column 5 is the end of `café`"
        );
        // The old byte-column convention (column 6) is now out of range.
        assert_eq!(Position { line: 1, column: 6 }.to_byte_offset(text), None);
    }

    #[test]
    fn upstream_position_treats_crlf_as_atomic() {
        use mdbook_lint_core::violation::Position;

        let text = "ab\r\ncd\r\n";
        // End of line 1 is before the terminator.
        assert_eq!(
            Position { line: 1, column: 3 }.to_byte_offset(text),
            Some(2)
        );
        // Column 1 of line 2 is after the whole CRLF pair — nothing addresses
        // the gap between '\r' and '\n'.
        assert_eq!(
            Position { line: 2, column: 1 }.to_byte_offset(text),
            Some(4)
        );
        assert_eq!(Position::from_byte_offset(text, 3), None);
    }

    #[test]
    fn hyalo001_fix_applies_on_non_ascii_line() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "[] café task\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "HYALO001")
            .expect("HYALO001 should fire");
        let fix = d.fix.as_ref().expect("HYALO001 fix should not be dropped");
        let fixed = apply(body, fix);
        assert!(
            fixed.starts_with("- [ ]"),
            "expected bare checkbox to be fixed, got: {fixed:?}"
        );
    }

    #[test]
    fn md009_fix_applies_on_trailing_space_cjk_line() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "日本語のテキスト   \n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD009")
            .expect("MD009 should fire");
        let fix = d.fix.as_ref().expect("MD009 fix should not be dropped");
        let fixed = apply(body, fix);
        assert_eq!(fixed, "日本語のテキスト\n");
    }

    // --- iter-218 NEW-11: MD010's upstream `Violation.column` is a byte
    // offset (`line.find('\t')` in mdbook-lint-rulesets), not the
    // Unicode-scalar column DEC-073 requires. `byte_col_to_scalar_col` in
    // this file corrects it before the diagnostic is emitted. ---

    #[test]
    fn md010_reports_unicode_scalar_column_not_byte_offset() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();

        // "àéî" is 3 Unicode scalars but 6 UTF-8 bytes (2 bytes each), so the
        // tab's byte offset (6) and scalar offset (3) diverge. Before
        // iter-218 the reported column was 7 (byte offset + 1); the correct
        // Unicode-scalar column is 4 (scalar offset + 1).
        let body = "àéî\tTAB\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD010")
            .expect("MD010 should fire on the hard tab");
        assert_eq!(
            d.column, 4,
            "expected scalar column 4 (3 chars + 1), not the byte column 7"
        );
    }

    #[test]
    fn md010_reports_unicode_scalar_column_on_emoji_line() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();

        // A single emoji is one Unicode scalar but 4 UTF-8 bytes, so the
        // byte-vs-scalar gap is even wider than the 2-byte-per-char case
        // above. Byte column would be 5 (4 bytes + 1); scalar column is 2.
        let body = "😀\tTAB\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD010")
            .expect("MD010 should fire on the hard tab");
        assert_eq!(
            d.column, 2,
            "expected scalar column 2 (1 char + 1), not the byte column 5"
        );
    }

    #[test]
    fn md042_reports_unicode_scalar_column_not_byte_offset() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();

        // MD042 (no-empty-links, default-on) computes its column from
        // comrak's byte-indexed AST `sourcepos`. Nested inside a blockquote
        // so the fixture also proves the conversion uses the *raw* line text
        // (block marker included) rather than content stripped of it —
        // comrak's sourcepos is relative to the raw line, and so is
        // `Document::lines`, so they must agree.
        let body = "> àéî []()\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD042")
            .expect("MD042 should fire on the empty link");
        // "> àéî " is 6 Unicode scalars (`>`, space, à, é, î, space) — 9 UTF-8
        // bytes. Scalar column is 7; byte column would be 10.
        assert_eq!(
            d.column, 7,
            "expected scalar column 7 (6 chars + 1), not the byte column 10"
        );
    }

    #[test]
    fn md052_reports_unicode_scalar_column_not_byte_offset_when_enabled() {
        // MD052 (undefined reference link) is opt-in — enable it via the
        // same `[lint.rules]` override path `.hyalo.toml` uses.
        use crate::config::RuleOverride;
        let mut config = LintConfig::default();
        config
            .rules
            .insert("MD052".to_owned(), RuleOverride::Enabled(true));
        let engine = HyaloLintEngine::create().unwrap();

        // MD052 computes its column from a byte cursor
        // (`self.pos - self.line_start`) into its own `&[u8]` input.
        let body = "àéî [ref][undefined]\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD052")
            .expect("MD052 should fire on the undefined reference label");
        // "àéî " is 4 Unicode scalars, 7 UTF-8 bytes. Scalar column is 5;
        // byte column would be 8.
        assert_eq!(
            d.column, 5,
            "expected scalar column 5 (4 chars + 1), not the byte column 8"
        );
    }

    /// Pins the *pre-conversion* assumption `BYTE_COLUMN_RULE_IDS` documents:
    /// MD010's raw upstream `Violation.column` is a byte offset. Calls the
    /// upstream `mdbook_lint_rulesets::standard::md010::MD010` rule directly
    /// — bypassing `lint_body`'s conversion entirely — so a future
    /// `mdbook-lint-rulesets` release that switches MD010 to a scalar
    /// computation (as MD011/MD034 already do) fails this test loudly
    /// instead of silently getting double-converted by
    /// `byte_col_to_scalar_col` and coming out wrong again (review finding
    /// #3).
    #[test]
    fn md010_reports_byte_column_before_conversion() {
        use mdbook_lint_core::rule::Rule as _;
        use mdbook_lint_rulesets::standard::md010::MD010;

        let body = "àéî\tTAB\n";
        let doc = Document::new(body.to_owned(), PathBuf::from("test.md")).unwrap();
        let violations = MD010::default().check(&doc).unwrap();
        let v = violations
            .iter()
            .find(|v| v.rule_id == "MD010")
            .expect("MD010 should fire on the hard tab");
        // "àéî" is 6 UTF-8 bytes (2 per char); the tab's 1-based byte column
        // is 7. If this ever becomes 4 (the scalar column), MD010 has moved
        // off byte offsets upstream and must come out of
        // `BYTE_COLUMN_RULE_IDS`.
        assert_eq!(
            v.column, 7,
            "MD010's raw upstream column is expected to still be byte-indexed; \
             if this fails because it's now 4, remove MD010 from \
             BYTE_COLUMN_RULE_IDS instead of updating this assertion"
        );
    }

    // --- Char-column rules (MD034, MD011) must not be corrupted by the
    // byte-column walk when multibyte UTF-8 precedes the flagged span ---

    #[test]
    fn md034_fix_correct_on_line_with_multibyte_prefix() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "café http://example.com is a site.\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &["MD034".to_owned()])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD034")
            .expect("MD034 should fire on a bare URL");
        let fix = d.fix.as_ref().expect("MD034 fix should convert");
        let fixed = apply(body, fix);
        // The byte-column walk used to eat the space before the URL and
        // leave a stray fragment: "café<http://example.com>m is a site."
        assert_eq!(fixed, "café <http://example.com> is a site.\n");
    }

    #[test]
    fn md034_fix_does_not_swallow_trailing_liquid_tag() {
        // GitHub Docs prose embeds Liquid template syntax right after a URL.
        // The MD034 autolink must stop at the real URL, leaving `{% ... %}`
        // outside the `<...>` so the template markup survives.
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "See https://example.com{% ifversion ghes %} for details.\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &["MD034".to_owned()])
            .unwrap();
        let Some(d) = diagnostics.iter().find(|d| d.rule_id == "MD034") else {
            // If upstream stopped flagging this shape at all, there is nothing
            // to corrupt — the invariant still holds.
            return;
        };
        if let Some(fix) = d.fix.as_ref() {
            let fixed = apply(body, fix);
            assert!(
                !fixed.contains("{% ifversion ghes %}>"),
                "Liquid tag must not be pulled inside the autolink: {fixed:?}"
            );
            assert!(
                fixed.contains("{% ifversion ghes %}"),
                "Liquid tag must survive verbatim: {fixed:?}"
            );
        }
    }

    #[test]
    fn md034_fix_wraps_a_clean_url_exactly() {
        // The former `trim_md034_liquid` no-op case: a URL with no Liquid tag
        // must round-trip through the upstream fix untouched apart from the
        // autolink brackets.
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "See https://example.com for details.\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &["MD034".to_owned()])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD034")
            .expect("MD034 should fire on a bare URL");
        let fix = d.fix.as_ref().expect("MD034 fix should convert");
        assert_eq!(apply(body, fix), "See <https://example.com> for details.\n");
    }

    #[test]
    fn md011_fix_correct_on_line_with_multibyte_prefix() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "café (some text)[http://example.com] end.\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &["MD011".to_owned()])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD011")
            .expect("MD011 should fire on a reversed link");
        let fix = d.fix.as_ref().expect("MD011 fix should convert");
        let fixed = apply(body, fix);
        assert_eq!(fixed, "café [some text](http://example.com) end.\n");
    }

    // --- md047_fix must handle CRLF terminators ---

    #[test]
    fn md047_fix_crlf_removes_extra_trailing_newlines_in_one_pass() {
        let body = "body\r\n\r\n\r\n";
        let fix = md047_fix(body).expect("multiple trailing CRLF should produce a fix");
        let fixed = apply(body, &fix);
        assert_eq!(fixed, "body\r\n");
    }

    #[test]
    fn md047_fix_crlf_adds_matching_terminator() {
        let body = "line one\r\nlast line";
        let fix = md047_fix(body).expect("missing trailing newline should produce a fix");
        let fixed = apply(body, &fix);
        assert_eq!(fixed, "line one\r\nlast line\r\n");
    }

    #[test]
    fn md047_fix_crlf_single_trailing_newline_is_clean() {
        assert!(md047_fix("body\r\n").is_none());
    }

    // --- H-1b: MD009 must not duplicate the line terminator ---

    #[test]
    fn md009_fix_does_not_inject_blank_line() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "x   \ny\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD009")
            .expect("MD009 should fire");
        let fix = d.fix.as_ref().expect("MD009 fix should not be dropped");
        let fixed = apply(body, fix);
        assert_eq!(fixed, "x\ny\n", "fix must not insert a blank line");
    }

    #[test]
    fn md009_fix_preserves_crlf_line_endings() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "x   \r\ny\r\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD009")
            .expect("MD009 should fire");
        let fix = d.fix.as_ref().expect("MD009 fix should not be dropped");
        let fixed = apply(body, fix);
        assert_eq!(
            fixed, "x\r\ny\r\n",
            "fix must keep CRLF endings, not flip to mixed/LF"
        );
    }

    // --- H-1c: MD047 must converge in a single application ---

    #[test]
    fn md047_fix_converges_two_trailing_newlines_in_one_pass() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "body\n\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD047")
            .expect("MD047 should fire");
        let fix = d.fix.as_ref().expect("MD047 fix should not be dropped");
        let fixed = apply(body, fix);
        assert_eq!(
            fixed, "body\n",
            "must converge to a single trailing newline"
        );

        // A second run against the fixed body must report no violation.
        let diagnostics2 = engine
            .lint_body(&fixed, "test.md", None, false, &config, &[])
            .unwrap();
        assert!(!diagnostics2.iter().any(|d| d.rule_id == "MD047"));
    }

    #[test]
    fn md047_fix_converges_many_trailing_newlines_in_one_pass() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "body\n\n\n\n\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD047")
            .expect("MD047 should fire");
        let fix = d.fix.as_ref().expect("MD047 fix should not be dropped");
        let fixed = apply(body, fix);
        assert_eq!(fixed, "body\n");
    }

    #[test]
    fn md047_fix_adds_missing_trailing_newline() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "body without newline";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD047")
            .expect("MD047 should fire");
        let fix = d.fix.as_ref().expect("MD047 fix should not be dropped");
        let fixed = apply(body, fix);
        assert_eq!(fixed, "body without newline\n");
    }
    // -----------------------------------------------------------------
    // iter-196: fixtures proving the mdbook-lint 0.16.0 upstream fixes are
    // present in the *published* crate, so the downstream workarounds they
    // replace stay deleted. Each of these fails under 0.15.2 semantics.
    // -----------------------------------------------------------------

    /// Upstream #486: before 0.16.0 MD011 emitted an *inclusive* end column
    /// (the position of the closing `]`), so applying the fix verbatim left a
    /// stray `]` behind on every line, ASCII included. That is what the
    /// deleted `end += 1` guard compensated for.
    #[test]
    fn md011_fix_leaves_no_stray_bracket() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "(some text)[http://example.com] tail\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &["MD011".to_owned()])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD011")
            .expect("MD011 should fire on a reversed link");
        let fix = d.fix.as_ref().expect("MD011 fix should convert");
        let fixed = apply(body, fix);
        assert_eq!(fixed, "[some text](http://example.com) tail\n");
        assert!(
            !fixed.contains("]] "),
            "no stray closing bracket may survive: {fixed:?}"
        );
    }

    /// Upstream #486: MD034's URL boundary scan now stops before Liquid /
    /// Handlebars openers, so the deleted `trim_md034_liquid` pull-back is no
    /// longer needed. Unlike the older tolerant test above, this one insists
    /// the shipped crate gets it right on its own.
    #[test]
    fn md034_upstream_stops_autolink_before_liquid_tag() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "See https://example.com{% ifversion ghes %} for details.\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &["MD034".to_owned()])
            .unwrap();
        if let Some(fix) = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD034")
            .and_then(|d| d.fix.as_ref())
        {
            let fixed = apply(body, fix);
            assert_eq!(
                fixed, "See <https://example.com>{% ifversion ghes %} for details.\n",
                "upstream must close the autolink before the Liquid opener"
            );
        }
    }

    /// Upstream #492 (our issue #491): a paragraph continuation line that
    /// starts with an issue reference such as `#472` is prose, not a
    /// malformed ATX heading, and must not be flagged. A genuinely standalone
    /// `#foo` still is, and a mid-line `PR #472` never was.
    #[test]
    fn md018_ignores_paragraph_continuation_lines() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "Upstream tracked this in\n#472 which is a continuation line.\n\nSee PR #472 for the fix.\n\n#standalone\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &["MD018".to_owned()])
            .unwrap();
        let lines: Vec<usize> = diagnostics
            .iter()
            .filter(|d| d.rule_id == "MD018")
            .map(|d| d.line)
            .collect();
        assert!(
            !lines.contains(&2),
            "continuation line `#472` must not be flagged: {lines:?}"
        );
        assert!(
            !lines.contains(&4),
            "mid-line `PR #472` must not be flagged: {lines:?}"
        );
        assert!(
            lines.contains(&6),
            "a standalone `#standalone` is still a malformed heading: {lines:?}"
        );
    }

    /// Upstream #493: insertion-shaped fixes (MD022 adds a blank line around
    /// a heading) and replacement-shaped fixes (MD009 rewrites a line) are
    /// now distinguished by the half-open range itself, so the deleted
    /// `line_len + 1` replace-vs-insert heuristic is unnecessary. Applying
    /// MD022's fix must add a blank line without eating the heading.
    #[test]
    fn md022_insertion_fix_adds_blank_line_without_duplicating_content() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "Some prose.\n# Heading\n\nMore prose.\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &["MD022".to_owned()])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD022")
            .expect("MD022 should fire on a heading with no blank line above");
        let fix = d.fix.as_ref().expect("MD022 fix should convert");
        let fixed = apply(body, fix);
        assert_eq!(fixed, "Some prose.\n\n# Heading\n\nMore prose.\n");
    }

    /// Upstream #493 promises CRLF preservation, but the shipped 0.16.0
    /// MD047 still hard-codes `"\n"` for the missing-EOF-newline insertion
    /// (see [`md047_fix`]). This asserts hyalo's remaining CRLF
    /// compensation, which is the one documented exception to the
    /// "no downstream workarounds" rule.
    #[test]
    fn md047_crlf_body_keeps_crlf_when_adding_the_final_terminator() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "line one\r\nlast line";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD047")
            .expect("MD047 should fire");
        let fix = d.fix.as_ref().expect("MD047 fix should not be dropped");
        assert_eq!(apply(body, fix), "line one\r\nlast line\r\n");
    }

    /// A multibyte line with CRLF terminators exercises both halves of the
    /// 0.16 contract at once: scalar columns and atomic CRLF.
    #[test]
    fn fixes_are_exact_on_multibyte_crlf_content() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let body = "café — naïve   \r\nsecond ünïcode line\r\n";
        let diagnostics = engine
            .lint_body(body, "test.md", None, false, &config, &[])
            .unwrap();
        let d = diagnostics
            .iter()
            .find(|d| d.rule_id == "MD009")
            .expect("MD009 should fire on the trailing spaces");
        let fix = d.fix.as_ref().expect("MD009 fix should not be dropped");
        assert_eq!(apply(body, fix), "café — naïve\r\nsecond ünïcode line\r\n");
    }
}
