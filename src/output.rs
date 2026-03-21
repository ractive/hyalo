use std::fmt::Write as _;

use serde_json::json;

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Text,
}

/// Result of a command execution: either success (exit 0) or a user-facing error (exit 1).
/// Internal/unexpected errors are represented by `anyhow::Error` at the call site.
#[derive(Debug)]
pub enum CommandOutcome {
    /// Successful operation — output goes to stdout.
    Success(String),
    /// User error (file not found, property missing, etc.) — output goes to stderr.
    UserError(String),
}

impl Format {
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "json" => Some(Self::Json),
            "text" => Some(Self::Text),
            _ => None,
        }
    }
}

/// Format a successful JSON value for output.
#[must_use]
pub fn format_success(format: Format, value: &serde_json::Value) -> String {
    match format {
        Format::Json => serde_json::to_string_pretty(value).unwrap_or_default(),
        Format::Text => format_value_as_text(value),
    }
}

/// Format an error for output to stderr.
#[must_use]
pub fn format_error(
    format: Format,
    error: &str,
    path: Option<&str>,
    hint: Option<&str>,
    cause: Option<&str>,
) -> String {
    match format {
        Format::Json => {
            let mut obj = json!({"error": error});
            if let Some(p) = path {
                obj["path"] = json!(p);
            }
            if let Some(h) = hint {
                obj["hint"] = json!(h);
            }
            if let Some(c) = cause {
                obj["cause"] = json!(c);
            }
            serde_json::to_string_pretty(&obj).unwrap_or_default()
        }
        Format::Text => {
            let mut msg = format!("Error: {error}");
            if let Some(p) = path {
                let _ = write!(msg, "\n  path: {p}");
            }
            if let Some(h) = hint {
                let _ = write!(msg, "\n  hint: {h}");
            }
            if let Some(c) = cause {
                let _ = write!(msg, "\n  cause: {c}");
            }
            msg
        }
    }
}

/// Format a JSON value as human-readable text.
fn format_value_as_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(format_value_as_text)
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| format!("{k}: {}", format_scalar(v)))
            .collect::<Vec<_>>()
            .join("\n"),
        other => format_scalar(other),
    }
}

/// Format a scalar JSON value as text.
fn format_scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_owned(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_scalar).collect();
            items.join(", ")
        }
        serde_json::Value::Object(map) => {
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k}={}", format_scalar(v)))
                .collect();
            items.join(", ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_json_error() {
        let out = format_error(
            Format::Json,
            "file not found",
            Some("foo/bar"),
            Some("did you mean foo/bar.md?"),
            None,
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["error"], "file not found");
        assert_eq!(parsed["hint"], "did you mean foo/bar.md?");
        assert!(parsed.get("cause").is_none());
    }

    #[test]
    fn format_text_error() {
        let out = format_error(Format::Text, "file not found", Some("foo"), None, None);
        assert!(out.contains("Error: file not found"));
        assert!(out.contains("path: foo"));
    }

    #[test]
    fn format_json_success() {
        let val = serde_json::json!({"name": "test", "value": 42});
        let out = format_success(Format::Json, &val);
        assert!(out.contains("\"name\": \"test\""));
    }

    #[test]
    fn format_text_success_object() {
        let val = serde_json::json!({"name": "test", "value": "hello"});
        let out = format_success(Format::Text, &val);
        assert!(out.contains("name: test"));
    }
}
