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
    ("MD001", DiagSeverity::Warn),  // heading-increment — structural
    ("MD009", DiagSeverity::Warn),  // trailing-spaces
    ("MD010", DiagSeverity::Warn),  // no-hard-tabs
    ("MD011", DiagSeverity::Error), // no-reversed-links — breaks rendering
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
];

/// Rules that are **default-on** (cheap, structural, low false-positive).
/// All others default to off.
static DEFAULT_ON: &[&str] = &[
    "MD001", "MD009", "MD010", "MD011", "MD012", "MD018", "MD019", "MD022", "MD023", "MD031",
    "MD034", "MD040", "MD042", "MD047", // HYALO rules are always default-on
    "HYALO001", "HYALO002", "HYALO003", "HYALO004",
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
                    // MD047's own fix positions don't survive translation to
                    // byte offsets for the common "N trailing newlines" case
                    // (see `md047_fix`) — compute it directly instead.
                    let fix = if *rule_id == "MD047" {
                        md047_fix(body_content)
                    } else {
                        convert_fix(&v, body_content, rule_id)
                    };
                    diagnostics.push(Diagnostic {
                        rule_id: rule_id.to_string(),
                        rule_name: v.rule_name.clone(),
                        message: v.message.clone(),
                        line: v.line,
                        column: v.column,
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
                let fix = convert_fix(&v, body_content, "HYALO001");
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

/// Upstream rules disagree on what a `Fix` column means: some compute
/// columns from byte lengths (`line.len() + 1` style — MD009, and our own
/// HYALO001), others index into a `Vec<char>` and emit char positions (at
/// least MD034 and MD011). Using the wrong unit on a line with multibyte
/// UTF-8 lands the fix on the wrong byte offset and corrupts the file, so
/// the walk must be chosen per rule. Only rules whose column math has been
/// verified byte-based belong here; everything else gets the char-based
/// walk, whose worst case is a dropped fix rather than corruption.
fn rule_uses_byte_columns(rule_id: &str) -> bool {
    matches!(rule_id, "MD009" | "HYALO001")
}

/// Convert an upstream `Fix` (line/column positions) to a byte-offset `DiagFix`.
///
/// The column unit is selected per rule (see [`rule_uses_byte_columns`]).
/// The conversion from line+column to byte offsets is best-effort; if
/// conversion fails the fix is silently dropped (the violation is still
/// reported without a fix).
fn convert_fix(v: &mdbook_lint_core::Violation, content: &str, rule_id: &str) -> Option<DiagFix> {
    let byte_columns = rule_uses_byte_columns(rule_id);
    let fix = v.fix.as_ref()?;
    let start = line_col_to_byte(content, fix.start.line, fix.start.column, byte_columns)?;
    let mut end = line_col_to_byte(content, fix.end.line, fix.end.column, byte_columns)?;
    let mut replacement = fix.replacement.clone().unwrap_or_default();

    // Upstream MD011 emits its end column as the 1-based position OF the
    // closing ']' (its own source comments "+1 because end_pos is 0-based
    // position of ']'"), i.e. inclusive — one short as an exclusive end, so
    // applying the fix as-is leaves a stray ']' behind on every line, ASCII
    // included. Extend the range by one, but only when the byte at `end`
    // really is the ']' the range claims to cover, so a future upstream
    // correction cannot make this overshoot.
    if rule_id == "MD011" && content[end..].starts_with(']') {
        end += 1;
    }

    // Some upstream rules (MD009, MD023, ...) express "replace this whole
    // line" with an end column of `line_len + 1` and a replacement that
    // re-adds its own trailing '\n', expecting to consume the line's
    // original terminator too. Translated through `line_col_to_byte`, that
    // end column lands *on* the first byte of the terminator (CR on CRLF
    // input, LF otherwise) rather than past it — so consuming only
    // `[start, end)` leaves the original terminator in place and the
    // replacement's own '\n' creates a duplicate blank line.
    //
    // Other rules (MD022, ...) use the *same* end-column shape to express a
    // deliberate insertion: the replacement is the untouched original line
    // plus an extra '\n', meant to add a new line while leaving the
    // existing terminator alone. Tell the two apart by comparing the
    // replacement (minus its trailing '\n') against the original
    // `[start, end)` slice: identical means "insert before the terminator",
    // different means "replace the line, including its terminator".
    if let Some(without_nl) = replacement.strip_suffix('\n')
        && content
            .get(start..end)
            .is_some_and(|orig| orig != without_nl)
    {
        let bytes = content.as_bytes();
        if bytes.get(end) == Some(&b'\r') && bytes.get(end + 1) == Some(&b'\n') {
            // CRLF terminator: consume both bytes and keep the replacement's
            // ending CRLF-style so the fix doesn't flip the file to LF-only.
            end += 2;
            replacement.truncate(replacement.len() - 1);
            replacement.push_str("\r\n");
        } else if bytes.get(end) == Some(&b'\n') {
            end += 1;
        }
    }

    Some(DiagFix {
        description: fix.description.clone(),
        start,
        end,
        replacement,
    })
}

/// Compute a corrected single-pass fix for MD047 (single-trailing-newline),
/// bypassing upstream's own `Fix` positions entirely.
///
/// Upstream computes its "remove extra trailing newlines" fix range in
/// line/column terms that, once translated to byte offsets via
/// [`line_col_to_byte`], is a byte-for-byte no-op when the body has exactly
/// two trailing newlines (so `--fix` would report "fixed" forever without
/// the file ever changing) and only removes one newline per application
/// otherwise. Compute the correct range directly against the body's byte
/// length instead, so any number of trailing newlines converges to exactly
/// one in a single application.
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

/// Convert a 1-based (line, column) pair to a 0-based byte offset.
///
/// `byte_columns` selects the column unit (see [`rule_uses_byte_columns`]):
/// when `true`, columns are 1-based **byte** positions within the line
/// (`line.len() - trailing + 1` style) and the walk advances by each
/// character's UTF-8 byte length; when `false`, columns are 1-based **char**
/// positions (`Vec<char>` indices) and the walk advances by one per
/// character. Using the wrong unit on a multibyte line either drops the fix
/// (byte target unreachable by char walk) or lands on the wrong byte
/// (char target overshot by byte walk) — the latter corrupts content, which
/// is why unverified rules default to the char walk.
fn line_col_to_byte(
    text: &str,
    target_line: usize,
    target_col: usize,
    byte_columns: bool,
) -> Option<usize> {
    let mut cur_line = 1;
    let mut cur_col = 1;
    for (offset, ch) in text.char_indices() {
        if cur_line == target_line && cur_col == target_col {
            return Some(offset);
        }
        if ch == '\n' {
            cur_line += 1;
            cur_col = 1;
        } else {
            cur_col += if byte_columns { ch.len_utf8() } else { 1 };
        }
    }
    if cur_line == target_line && cur_col == target_col {
        Some(text.len())
    } else {
        None
    }
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

    // --- H-1a: line_col_to_byte must use byte columns, not char columns ---

    #[test]
    fn line_col_to_byte_handles_multibyte_utf8() {
        // "café" — 'é' is 2 bytes in UTF-8, so byte and char columns diverge
        // partway through the line.
        let text = "café\n";
        // Byte column 6 is the position right after 'é' (byte offset 5,
        // since c=1,a=1,f=1,é=2 bytes -> line is 5 bytes long).
        assert_eq!(line_col_to_byte(text, 1, 6, true), Some(5));
        // Char column 5 is the same position under the char convention.
        assert_eq!(line_col_to_byte(text, 1, 5, false), Some(5));
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
}
