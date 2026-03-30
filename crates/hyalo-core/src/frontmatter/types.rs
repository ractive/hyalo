use anyhow::{Context, Result};
use serde_json::Value;

/// Infer the Obsidian property type from a YAML value.
#[must_use]
pub fn infer_type(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "checkbox",
        Value::Number(_) => "number",
        Value::Array(_) => "list",
        Value::String(s) => infer_string_type(s),
        Value::Null => "null",
        Value::Object(_) => "text",
    }
}

/// Infer the type of a string value (date, datetime, or text).
fn infer_string_type(s: &str) -> &'static str {
    if is_date(s) {
        "date"
    } else if is_datetime(s) {
        "datetime"
    } else {
        "text"
    }
}

/// Check if a string matches `YYYY-MM-DD`.
fn is_date(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }
    let b = s.as_bytes();
    b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit)
}

/// Check if a string matches `YYYY-MM-DDThh:mm:ss`.
fn is_datetime(s: &str) -> bool {
    if s.len() != 19 {
        return false;
    }
    let b = s.as_bytes();
    b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit)
        && b[11..13].iter().all(u8::is_ascii_digit)
        && b[14..16].iter().all(u8::is_ascii_digit)
        && b[17..19].iter().all(u8::is_ascii_digit)
}

/// Parse a string value into an appropriate YAML Value, optionally forced to a specific type.
pub fn parse_value(raw: &str, forced_type: Option<&str>) -> Result<Value> {
    match forced_type {
        Some("text") => Ok(Value::String(raw.to_owned())),
        Some("number") => {
            if let Ok(i) = raw.parse::<i64>() {
                Ok(Value::Number(i.into()))
            } else {
                let f: f64 = raw.parse().context("value is not a valid number")?;
                anyhow::ensure!(f.is_finite(), "value is not a finite number");
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .ok_or_else(|| anyhow::anyhow!("value is not a finite number"))
            }
        }
        Some("checkbox") => {
            let b = match raw {
                "true" | "yes" | "1" => true,
                "false" | "no" | "0" => false,
                _ => anyhow::bail!("value is not a valid checkbox (use true/false)"),
            };
            Ok(Value::Bool(b))
        }
        Some("date") => {
            anyhow::ensure!(is_date(raw), "value is not a valid date (YYYY-MM-DD)");
            Ok(Value::String(raw.to_owned()))
        }
        Some("datetime") => {
            anyhow::ensure!(
                is_datetime(raw),
                "value is not a valid datetime (YYYY-MM-DDThh:mm:ss)"
            );
            Ok(Value::String(raw.to_owned()))
        }
        Some("list") => {
            // Parse comma-separated values
            let items: Vec<Value> = raw
                .split(',')
                .map(|s| Value::String(s.trim().to_owned()))
                .collect();
            Ok(Value::Array(items))
        }
        Some(other) => anyhow::bail!("unknown type: {other}"),
        None => Ok(infer_value(raw)),
    }
}

/// Infer a YAML value from a raw string (try number, bool, date, then text).
fn infer_value(raw: &str) -> Value {
    // Try integer
    if let Ok(i) = raw.parse::<i64>() {
        return Value::Number(i.into());
    }
    // Try float (reject NaN/inf which parse successfully but aren't useful property values)
    if let Ok(f) = raw.parse::<f64>()
        && f.is_finite()
    {
        return serde_json::Number::from_f64(f)
            .map_or_else(|| Value::String(raw.to_owned()), Value::Number);
    }
    // Try bool
    match raw {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }
    // Try list: [a, b, c] syntax
    if raw.starts_with('[') && raw.ends_with(']') {
        let inner = &raw[1..raw.len() - 1];
        // Empty brackets = empty list
        if inner.trim().is_empty() {
            return Value::Array(Vec::new());
        }
        // Split by comma, trim each item, keep as strings
        let items: Vec<Value> = inner
            .split(',')
            .map(|s| Value::String(s.trim().to_owned()))
            .collect();
        return Value::Array(items);
    }
    Value::String(raw.to_owned())
}
