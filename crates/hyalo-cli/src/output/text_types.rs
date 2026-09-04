//! Hand-written text renderings for `types show` / `types list`.
//!
//! Split out of the single 3,744-line `output.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `output::...` paths and behaviour are unchanged.

/// Format a `types show` result as human-readable text.
///
/// Expected JSON shape: `{type, required, filename_template, defaults, properties}`.
/// Output example:
/// ```text
/// Type: iteration
///
/// Required: title, type, date
///
/// Properties:
///   branch:
///     type: string
///     pattern: ^iter-\d+/
///
///   date:
///     type: date
///
/// Filename template: iteration-{N}-{slug}.md
/// ```
pub(super) fn format_type_show_text(map: &serde_json::Map<String, serde_json::Value>) -> String {
    use std::fmt::Write as _;

    let mut s = String::new();

    let type_name = map
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?");
    let _ = write!(s, "Type: {type_name}");

    // Required fields.
    if let Some(serde_json::Value::Array(req)) = map.get("required")
        && !req.is_empty()
    {
        let list: Vec<&str> = req.iter().filter_map(serde_json::Value::as_str).collect();
        let _ = write!(s, "\n\nRequired: {}", list.join(", "));
    }

    // Defaults block.
    if let Some(serde_json::Value::Object(defaults)) = map.get("defaults")
        && !defaults.is_empty()
    {
        let _ = write!(s, "\n\nDefaults:");
        let mut keys: Vec<&str> = defaults.keys().map(String::as_str).collect();
        keys.sort_unstable();
        for key in keys {
            if let Some(value) = defaults.get(key) {
                let display = match value {
                    serde_json::Value::String(sv) => sv.clone(),
                    other => other.to_string(),
                };
                let _ = write!(s, "\n  {key}: {display}");
            }
        }
    }

    // Properties block.
    if let Some(serde_json::Value::Object(props)) = map.get("properties")
        && !props.is_empty()
    {
        let _ = write!(s, "\n\nProperties:");
        let mut prop_names: Vec<&str> = props.keys().map(String::as_str).collect();
        prop_names.sort_unstable();
        for name in prop_names {
            let Some(prop_val) = props.get(name) else {
                continue;
            };
            let _ = write!(s, "\n  {name}:");
            if let Some(obj) = prop_val.as_object() {
                // Print each constraint key on its own indented line.
                // Always show "type" first, then remaining keys sorted.
                let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
                keys.sort_unstable_by(|a, b| {
                    if *a == "type" {
                        std::cmp::Ordering::Less
                    } else if *b == "type" {
                        std::cmp::Ordering::Greater
                    } else {
                        a.cmp(b)
                    }
                });
                for key in keys {
                    if let Some(v) = obj.get(key) {
                        // A nested object (object-list's `key-patterns`) prints
                        // as an indented block; everything else keeps the
                        // generic one-line key/value dump.
                        if let serde_json::Value::Object(nested) = v {
                            let _ = write!(s, "\n    {key}:");
                            let mut nested_keys: Vec<&str> =
                                nested.keys().map(String::as_str).collect();
                            nested_keys.sort_unstable();
                            for nk in nested_keys {
                                if let Some(nv) = nested.get(nk) {
                                    let display = match nv {
                                        serde_json::Value::String(sv) => sv.clone(),
                                        other => other.to_string(),
                                    };
                                    let _ = write!(s, "\n      {nk}: {display}");
                                }
                            }
                            continue;
                        }
                        let display = match v {
                            serde_json::Value::Array(arr) => arr
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .collect::<Vec<_>>()
                                .join(", "),
                            serde_json::Value::String(sv) => sv.clone(),
                            other => other.to_string(),
                        };
                        let _ = write!(s, "\n    {key}: {display}");
                    }
                }
            }
            s.push('\n'); // blank line between property blocks
        }
    }

    // Required sections.
    if let Some(serde_json::Value::Array(sections)) = map.get("required_sections")
        && !sections.is_empty()
    {
        let _ = write!(s, "\n\nRequired sections:");
        for sec in sections {
            if let Some(sv) = sec.as_str() {
                let _ = write!(s, "\n  {sv}");
            }
        }
    }

    // Optional filename template.
    if let Some(serde_json::Value::String(tmpl)) = map.get("filename_template") {
        let _ = write!(s, "\nFilename template: {tmpl}");
    }

    s
}

/// Format a single `types list` entry as human-readable text.
///
/// Expected JSON shape: `{type, required, property_count, has_filename_template}`.
/// Output example:
/// ```text
/// iteration (4 required, 6 properties)
///   required: title, type, date, tags
/// ```
///
/// Note: `has_filename_template` is a boolean; the actual template is only in `types show`.
/// When present, a hint to run `types show` is appended.
pub(super) fn format_type_list_entry_text(
    map: &serde_json::Map<String, serde_json::Value>,
) -> String {
    use std::fmt::Write as _;

    let mut s = String::new();

    let type_name = map
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?");

    let req_arr: &[serde_json::Value] = map
        .get("required")
        .and_then(serde_json::Value::as_array)
        .map_or(&[], Vec::as_slice);
    let req_count = req_arr.len();

    let prop_count = map
        .get("property_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let has_filename = map
        .get("has_filename_template")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let prop_label = if prop_count == 1 {
        "property"
    } else {
        "properties"
    };
    let _ = write!(
        s,
        "{type_name} ({prop_count} {prop_label}, {req_count} required)"
    );

    if !req_arr.is_empty() {
        let list: Vec<&str> = req_arr
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        let _ = write!(s, "\n  required: {}", list.join(", "));
    }

    if has_filename {
        let _ = write!(s, "\n  filename: (see type details)");
    }

    s
}
