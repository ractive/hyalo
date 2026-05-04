//! Markdown linting engine for hyalo.
//!
//! Wraps `mdbook-lint-core` + `mdbook-lint-rulesets` and adds two
//! HYALO-native cross-cutting rules:
//!
//! - **HYALO001** — bare `[]` should be `- [ ]` (autofix).
//! - **HYALO002** — `status: completed` requires all task checkboxes ticked
//!   (only fires when the schema declares `status` as an enum containing
//!   `completed`).

pub mod config;
pub mod engine;
pub mod rules;

pub use config::{LintConfig, RuleOverride};
pub use engine::{HyaloLintEngine, RuleCatalogEntry};

/// A diagnostic produced by the markdown linter, adapted from upstream's `Violation`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Diagnostic {
    /// Rule identifier (e.g. `"MD013"` or `"HYALO001"`).
    pub rule_id: String,
    /// Human-readable rule name.
    pub rule_name: String,
    /// Violation message.
    pub message: String,
    /// Line number (1-based).
    pub line: usize,
    /// Column number (1-based).
    pub column: usize,
    /// Severity after hyalo overrides applied.
    pub severity: DiagSeverity,
    /// Optional autofix, if the rule supports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<DiagFix>,
}

/// Severity level (hyalo-controlled, not upstream-controlled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagSeverity {
    Error,
    Warn,
}

impl std::fmt::Display for DiagSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => f.write_str("error"),
            Self::Warn => f.write_str("warn"),
        }
    }
}

/// A byte-range autofix for a single violation in the body portion of a file.
/// `start`/`end` are byte offsets from the beginning of the **body** (post-frontmatter) content.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagFix {
    /// Human-readable description of the fix.
    pub description: String,
    /// Start byte offset (body-relative).
    pub start: usize,
    /// End byte offset (body-relative, exclusive).
    pub end: usize,
    /// Replacement text (empty string = delete).
    pub replacement: String,
}
