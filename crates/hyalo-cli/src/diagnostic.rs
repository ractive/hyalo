//! Typed command failures, kept separate from their rendered representation.
use crate::commands::apply::ApplyReport;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct UserDiagnostic {
    pub error: String,
    #[serde(flatten)]
    pub details: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Runnable follow-ups (DEC-367): a `hint` that points at a section of a
    /// command's long help carries that `hyalo <cmd> --help` here, so the next
    /// command is one copy away. Added through [`Self::with_help_pointer`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<crate::hints::Hint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<ApplyReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<&'static str>,
}

impl UserDiagnostic {
    #[must_use]
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            details: std::collections::BTreeMap::new(),
            path: None,
            hint: None,
            hints: Vec::new(),
            cause: None,
            effects: None,
            category: None,
        }
    }

    /// Attach the runnable twin of a prose hint that points at `section` of
    /// the command's long help: `-> hyalo <command> --help  # <section>`
    /// (DEC-367).
    /// check-help-drift (3g) fails on a prose pointer whose source file has
    /// no matching `with_help_pointer("<command>", "<section>")` call.
    #[must_use]
    pub fn with_help_pointer(mut self, command: &str, section: &str) -> Self {
        self.hints.push(crate::hints::Hint::from_builder(
            section,
            crate::hints::HintBuilder::cmd(command).flag("--help"),
        ));
        self
    }
    #[must_use]
    pub fn render(&self, format: crate::output::Format) -> String {
        if format == crate::output::Format::Json {
            serde_json::to_string_pretty(self)
                .unwrap_or_else(|_| "{\"error\":\"diagnostic serialization failed\"}".into())
        } else {
            let mut text = crate::output::format_error(
                format,
                &self.error,
                self.path.as_deref(),
                self.hint.as_deref(),
                self.cause.as_deref(),
            );
            for hint in &self.hints {
                text.push_str("\n  -> ");
                text.push_str(&hint.cmd);
                text.push_str("  # ");
                text.push_str(&hint.description);
            }
            if let Some(effects) = &self.effects {
                text.push_str("\nEffects: ");
                text.push_str(&effects.committed_paths().join(", "));
                text.push_str(" (committed; inspect before retrying)");
            }
            crate::output::sanitize_control_chars(&text)
        }
    }
    #[cfg(test)]
    pub fn contains(&self, value: &str) -> bool {
        self.render(crate::output::Format::Json).contains(value)
    }
}
impl std::fmt::Display for UserDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.error)
    }
}
impl std::error::Error for UserDiagnostic {}
impl From<String> for UserDiagnostic {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Compatibility-shaped constructor; format is consumed only by the renderer.
#[must_use]
pub fn user_diagnostic(
    _format: crate::output::Format,
    error: &str,
    path: Option<&str>,
    hint: Option<&str>,
    cause: Option<&str>,
) -> UserDiagnostic {
    UserDiagnostic {
        error: error.to_owned(),
        details: std::collections::BTreeMap::new(),
        path: path.map(str::to_owned),
        hint: hint.map(str::to_owned),
        hints: Vec::new(),
        cause: cause.map(str::to_owned),
        effects: None,
        category: None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn text_effect_paths_are_sanitized_while_json_preserves_exact_names() {
        use crate::commands::apply::{ApplyReport, EffectState, IndexDisposition, PathEffect};
        let hostile = "hostile\u{1b}[31m.md";
        let mut diagnostic = super::UserDiagnostic::new("output failed");
        diagnostic.effects = Some(ApplyReport {
            paths: vec![PathEffect {
                file: hostile.into(),
                state: EffectState::Committed,
                error: None,
                category: None,
            }],
            index: IndexDisposition::NotUsed,
            index_error: None,
        });
        let text = diagnostic.render(crate::output::Format::Text);
        assert!(!text.contains('\u{1b}'));
        assert!(text.contains("Effects: hostile"));
        let json: serde_json::Value =
            serde_json::from_str(&diagnostic.render(crate::output::Format::Json)).unwrap();
        assert_eq!(json["effects"]["paths"][0]["file"], hostile);
    }
}
