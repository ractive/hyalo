//! Engine factory and rule catalog.
//!
//! Builds a `LintEngine` that combines `StandardRuleProvider` from
//! `mdbook-lint-rulesets` with the three HYALO native rules.

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
    ("HYALO002", DiagSeverity::Warn),
    ("HYALO003", DiagSeverity::Error),
];

/// Rules that are **default-on** (cheap, structural, low false-positive).
/// All others default to off.
static DEFAULT_ON: &[&str] = &[
    "MD001", "MD009", "MD010", "MD011", "MD012", "MD018", "MD019", "MD022", "MD023", "MD031",
    "MD034", "MD040", "MD042", "MD047", // HYALO rules are always default-on
    "HYALO001", "HYALO002", "HYALO003",
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
                name: "title-h1-agreement".to_owned(),
                description: "Frontmatter `title` and first H1 heading should agree".to_owned(),
                default_severity: DiagSeverity::Warn,
                default_enabled: true,
                autofixable: false,
                source: "hyalo-mdlint".to_owned(),
            },
            RuleCatalogEntry {
                id: "HYALO003".to_owned(),
                name: "completed-tasks".to_owned(),
                description: "`status: completed` requires all task checkboxes to be ticked"
                    .to_owned(),
                default_severity: DiagSeverity::Error,
                default_enabled: true,
                autofixable: false,
                source: "hyalo-mdlint".to_owned(),
            },
        ];
        catalog.extend_from_slice(&hyalo_entries);
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

    /// Lint the **body** portion of a file (content after frontmatter).
    ///
    /// # Arguments
    /// - `body_content` — the body text (after `---` frontmatter).
    /// - `rel_path` — vault-relative path (for error messages).
    /// - `frontmatter_title` — extracted from frontmatter (for HYALO002).
    /// - `frontmatter_status` — extracted from frontmatter (for HYALO003).
    /// - `schema_has_completed` — whether schema declares `status: completed`.
    /// - `config` — user lint configuration.
    /// - `rule_filter` — if non-empty, only run these rule IDs.
    #[allow(clippy::too_many_arguments)]
    pub fn lint_body(
        &self,
        body_content: &str,
        rel_path: &str,
        frontmatter_title: Option<&str>,
        frontmatter_status: Option<&str>,
        schema_has_completed: bool,
        config: &LintConfig,
        rule_filter: &[String],
    ) -> Result<Vec<Diagnostic>> {
        use crate::rules::hyalo001::Hyalo001;
        use crate::rules::hyalo002::{Hyalo002, TitleMode};
        use crate::rules::hyalo003::Hyalo003;
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
                    let fix = convert_fix(&v, body_content);
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

        // --- HYALO002 ---
        if should_run("HYALO002") {
            let mode_str = config
                .rules
                .get("HYALO002")
                .and_then(|ov| ov.mode())
                .unwrap_or("either");
            let mode = TitleMode::from_config_str(mode_str);
            if mode != TitleMode::Off {
                let doc = Document::new(body_content.to_string(), PathBuf::from(rel_path))
                    .with_context(|| format!("creating Document for HYALO002 on {rel_path}"))?;
                let rule = Hyalo002::new(mode, frontmatter_title.map(str::to_owned));
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
        }

        // --- HYALO003 ---
        if should_run("HYALO003") {
            let doc = Document::new(body_content.to_string(), PathBuf::from(rel_path))
                .with_context(|| format!("creating Document for HYALO003 on {rel_path}"))?;
            let rule = Hyalo003::new(schema_has_completed, frontmatter_status.map(str::to_owned));
            let sev = effective_severity("HYALO003");
            let violations = rule
                .check(&doc)
                .with_context(|| format!("running HYALO003 on {rel_path}"))?;
            for v in violations {
                diagnostics.push(Diagnostic {
                    rule_id: "HYALO003".to_owned(),
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

/// Convert an upstream `Fix` (line/column positions) to a byte-offset `DiagFix`.
///
/// The conversion from line+column to byte offsets is best-effort; if
/// conversion fails the fix is silently dropped (the violation is still
/// reported without a fix).
fn convert_fix(v: &mdbook_lint_core::Violation, content: &str) -> Option<DiagFix> {
    let fix = v.fix.as_ref()?;
    let start = line_col_to_byte(content, fix.start.line, fix.start.column)?;
    let end = line_col_to_byte(content, fix.end.line, fix.end.column)?;
    let replacement = fix.replacement.clone().unwrap_or_default();
    Some(DiagFix {
        description: fix.description.clone(),
        start,
        end,
        replacement,
    })
}

/// Convert a 1-based (line, column) pair to a 0-based byte offset.
fn line_col_to_byte(text: &str, target_line: usize, target_col: usize) -> Option<usize> {
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
            cur_col += 1;
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
    }

    #[test]
    fn default_on_rules_are_enabled() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let diagnostics = engine
            .lint_body(
                "trailing spaces   \n",
                "test.md",
                None,
                None,
                false,
                &config,
                &[],
            )
            .unwrap();
        // MD009 (trailing spaces) is default-on
        assert!(diagnostics.iter().any(|d| d.rule_id == "MD009"));
    }

    #[test]
    fn hyalo001_fires_for_bare_checkbox() {
        let config = LintConfig::default();
        let engine = HyaloLintEngine::create().unwrap();
        let diagnostics = engine
            .lint_body("[] Open task\n", "test.md", None, None, false, &config, &[])
            .unwrap();
        assert!(diagnostics.iter().any(|d| d.rule_id == "HYALO001"));
    }
}
