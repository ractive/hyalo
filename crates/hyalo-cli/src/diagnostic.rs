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
            cause: None,
            effects: None,
            category: None,
        }
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
