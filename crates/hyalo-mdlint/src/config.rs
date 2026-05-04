//! Configuration types for `[lint]` and `[lint.rules]` in `.hyalo.toml`.

use std::collections::HashMap;

/// Full lint configuration parsed from `.hyalo.toml`.
#[derive(Debug, Default, Clone)]
pub struct LintConfig {
    /// Per-rule output cap (default 3).
    pub max_violations_per_rule: Option<usize>,
    /// Worst-offender file cap (default 50).
    pub max_files: Option<usize>,
    /// Per-rule overrides from `[lint.rules]`.
    pub rules: HashMap<String, RuleOverride>,
}

impl LintConfig {
    pub fn max_violations_per_rule(&self) -> usize {
        self.max_violations_per_rule.unwrap_or(3)
    }

    pub fn max_files(&self) -> usize {
        self.max_files.unwrap_or(50)
    }
}

/// A single rule override entry from `[lint.rules]`.
///
/// Scalar form: `MD013 = false` → `RuleOverride::Enabled(false)`.
/// Table form: `[lint.rules.MD013]` with `enabled`, `severity`.
#[derive(Debug, Clone)]
pub enum RuleOverride {
    /// Scalar bool: just toggle enabled/disabled.
    Enabled(bool),
    /// Table form with full options.
    Table {
        enabled: Option<bool>,
        severity: Option<String>,
    },
}

impl RuleOverride {
    pub fn enabled(&self) -> Option<bool> {
        match self {
            Self::Enabled(b) => Some(*b),
            Self::Table { enabled, .. } => *enabled,
        }
    }

    pub fn severity(&self) -> Option<&str> {
        match self {
            Self::Enabled(_) => None,
            Self::Table { severity, .. } => severity.as_deref(),
        }
    }
}
