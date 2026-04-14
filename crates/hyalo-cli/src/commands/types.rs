/// `hyalo types` — manage document-type schemas in `.hyalo.toml`.
///
/// All TOML mutations use `toml_edit::DocumentMut` so that comments and
/// formatting in the user's config file are preserved.
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use hyalo_core::discovery;
use hyalo_core::frontmatter::{read_frontmatter, write_frontmatter};
use hyalo_core::schema::{SchemaConfig, expand_default};

use crate::output::{CommandOutcome, Format, format_success};

const TOML_PATH: &str = ".hyalo.toml";

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

/// `hyalo types list` — list all defined types and their required fields.
pub(crate) fn list_types(schema: &SchemaConfig) -> CommandOutcome {
    let mut sorted_types: Vec<&str> = schema.types.keys().map(String::as_str).collect();
    sorted_types.sort_unstable();

    let results: Vec<Value> = sorted_types
        .iter()
        .map(|name| {
            let ts = &schema.types[*name];
            serde_json::json!({
                "type": name,
                "required": ts.required,
                "has_filename_template": ts.filename_template.is_some(),
                "property_count": ts.properties.len(),
            })
        })
        .collect();

    let total = results.len() as u64;
    let val = serde_json::json!(results);
    CommandOutcome::success_with_total(format_success(Format::Json, &val), total)
}

// ---------------------------------------------------------------------------
// show
// ---------------------------------------------------------------------------

/// `hyalo types show <type>` — full merged schema for a type.
pub(crate) fn show_type(type_name: &str, schema: &SchemaConfig) -> CommandOutcome {
    if !schema.types.contains_key(type_name) {
        return CommandOutcome::UserError(format!(
            "Error: type '{type_name}' not found\n\n  tip: run 'hyalo types list' to see available types"
        ));
    }

    let merged = schema.merged_schema_for_type(type_name);

    // Serialize property constraints.
    let props: serde_json::Map<String, Value> = merged
        .properties
        .iter()
        .map(|(k, constraint)| {
            let c_val = constraint_to_json(constraint);
            (k.clone(), c_val)
        })
        .collect();

    let val = serde_json::json!({
        "type": type_name,
        "required": merged.required,
        "filename_template": merged.filename_template,
        "defaults": merged.defaults,
        "properties": props,
    });

    CommandOutcome::success(format_success(Format::Json, &val))
}

fn constraint_to_json(c: &hyalo_core::schema::PropertyConstraint) -> Value {
    use hyalo_core::schema::PropertyConstraint;
    match c {
        PropertyConstraint::String { pattern } => {
            if let Some(pat) = pattern {
                serde_json::json!({"type": "string", "pattern": pat})
            } else {
                serde_json::json!({"type": "string"})
            }
        }
        PropertyConstraint::Date => serde_json::json!({"type": "date"}),
        PropertyConstraint::Number => serde_json::json!({"type": "number"}),
        PropertyConstraint::Boolean => serde_json::json!({"type": "boolean"}),
        PropertyConstraint::List => serde_json::json!({"type": "list"}),
        PropertyConstraint::Enum { values } => {
            serde_json::json!({"type": "enum", "values": values})
        }
    }
}

// ---------------------------------------------------------------------------
// remove
// ---------------------------------------------------------------------------

/// `hyalo types remove <type>` — remove a type entry.
pub(crate) fn remove_type(type_name: &str) -> Result<CommandOutcome> {
    let mut doc = read_toml_doc()?;

    if !toml_type_exists(&doc, type_name) {
        return Ok(CommandOutcome::UserError(format!(
            "Error: type '{type_name}' not found\n\n  tip: run 'hyalo types list' to see available types"
        )));
    }

    {
        let schema = doc["schema"].as_table_mut().expect("schema is a table");
        let types = schema["types"].as_table_mut().expect("types is a table");
        types.remove(type_name);
    }

    write_toml_doc(&doc)?;

    let val = serde_json::json!({
        "action": "removed",
        "type": type_name,
    });
    Ok(CommandOutcome::success(format_success(Format::Json, &val)))
}

// ---------------------------------------------------------------------------
// set
// ---------------------------------------------------------------------------

/// Parse a property-type string to a known type string.
fn parse_property_type_str(s: &str) -> Result<&'static str, String> {
    match s {
        "string" => Ok("string"),
        "date" => Ok("date"),
        "number" => Ok("number"),
        "boolean" => Ok("boolean"),
        "list" => Ok("list"),
        "enum" => Ok("enum"),
        other => Err(format!(
            "invalid property type '{other}': must be one of string, date, number, boolean, list, enum"
        )),
    }
}

/// Parse a `KEY=VALUE` pair, returning an error string if malformed.
fn parse_kv<'a>(s: &'a str, flag: &str) -> Result<(&'a str, &'a str), String> {
    match s.find('=') {
        Some(pos) => {
            let key = s[..pos].trim();
            if key.is_empty() {
                return Err(format!(
                    "invalid {flag} argument '{s}': key cannot be empty"
                ));
            }
            Ok((key, &s[pos + 1..]))
        }
        None => Err(format!(
            "invalid {flag} argument '{s}': expected KEY=VALUE format"
        )),
    }
}

/// `hyalo types set <type> [flags...]` — update a type schema.
#[allow(clippy::too_many_arguments)]
pub(crate) fn set_type(
    dir: &Path,
    type_name: &str,
    required_args: &[String],
    default_args: &[String],
    property_type_args: &[String],
    property_values_args: &[String],
    filename_template: Option<&str>,
    dry_run: bool,
) -> Result<CommandOutcome> {
    // Validate type name before anything else.
    if let Err(msg) = validate_type_name(type_name) {
        return Ok(CommandOutcome::UserError(format!("Error: {msg}")));
    }

    // Require at least one mutation flag.
    if required_args.is_empty()
        && default_args.is_empty()
        && property_type_args.is_empty()
        && property_values_args.is_empty()
        && filename_template.is_none()
    {
        return Ok(CommandOutcome::UserError(
            "Error: no mutation flags provided — specify at least one of: \
             --required, --default, --property-type, --property-values, --filename-template"
                .to_owned(),
        ));
    }

    // Parse --required: each arg may be comma-separated.
    let required_fields: Vec<String> = required_args
        .iter()
        .flat_map(|s| s.split(','))
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    // Parse --default key=value pairs.
    let mut defaults_map: HashMap<String, String> = HashMap::new();
    for arg in default_args {
        match parse_kv(arg, "--default") {
            Ok((k, v)) => {
                defaults_map.insert(k.to_owned(), v.to_owned());
            }
            Err(e) => return Ok(CommandOutcome::UserError(format!("Error: {e}"))),
        }
    }

    // Parse --property-type key=type pairs.
    let mut prop_type_map: HashMap<String, &'static str> = HashMap::new();
    for arg in property_type_args {
        match parse_kv(arg, "--property-type") {
            Ok((k, v)) => match parse_property_type_str(v) {
                Ok(pt) => {
                    prop_type_map.insert(k.to_owned(), pt);
                }
                Err(e) => return Ok(CommandOutcome::UserError(format!("Error: {e}"))),
            },
            Err(e) => return Ok(CommandOutcome::UserError(format!("Error: {e}"))),
        }
    }

    // Parse --property-values key=val1,val2,... pairs.
    let mut prop_values_map: HashMap<String, Vec<String>> = HashMap::new();
    for arg in property_values_args {
        match parse_kv(arg, "--property-values") {
            Ok((k, v)) => {
                // Trim each value; reject empty entries (e.g. `status=` or
                // `status=a,`) so we don't silently persist an enum with a
                // `""` value, which would make lint/fix output confusing.
                let vals: Vec<String> = v.split(',').map(|s| s.trim().to_owned()).collect();
                if vals.iter().any(String::is_empty) {
                    return Ok(CommandOutcome::UserError(format!(
                        "Error: invalid --property-values argument '{arg}': enum values cannot be empty"
                    )));
                }
                prop_values_map.insert(k.to_owned(), vals);
            }
            Err(e) => return Ok(CommandOutcome::UserError(format!("Error: {e}"))),
        }
    }

    // Load TOML doc.
    let mut doc = read_toml_doc()?;

    // Collect what will change (used for dry-run preview and result).
    let mut toml_changes: Vec<String> = Vec::new();

    // Upsert: create the type if it doesn't exist (in-memory; disk write guarded below).
    let is_new = !toml_type_exists(&doc, type_name);
    if is_new {
        if let Err(msg) = ensure_schema_types_table(&mut doc) {
            return Ok(CommandOutcome::UserError(format!("Error: {msg}")));
        }
        let schema = doc["schema"].as_table_mut().expect("schema is a table");
        let types = schema["types"].as_table_mut().expect("types is a table");
        let mut type_table = toml_edit::Table::new();
        type_table.insert(
            "required",
            toml_edit::Item::Value(toml_edit::Value::Array(toml_edit::Array::new())),
        );
        types.insert(type_name, toml_edit::Item::Table(type_table));
        toml_changes.push(format!("create type: {type_name}"));
    }

    // Apply --required additions.
    if !required_fields.is_empty() {
        let cur_required = get_required_array(&doc, type_name);
        let mut new_required = cur_required.clone();
        for f in &required_fields {
            if !new_required.contains(f) {
                new_required.push(f.clone());
                toml_changes.push(format!("add required field: {f}"));
            }
        }
        if !dry_run {
            set_required_array(&mut doc, type_name, &new_required);
        }
    }

    // Apply --filename-template.
    if let Some(tmpl) = filename_template {
        toml_changes.push(format!("set filename-template: {tmpl}"));
        if !dry_run {
            set_string_field(&mut doc, type_name, "filename-template", tmpl);
        }
    }

    // Pre-expand each default once per invocation so time-sensitive tokens
    // (e.g. `$today`) produce the same value for TOML storage, for vault
    // writes, and for the reported `defaults_applied` — even if the run
    // crosses midnight.
    let expanded_defaults: HashMap<String, String> = defaults_map
        .iter()
        .map(|(k, v)| (k.clone(), expand_default(v)))
        .collect();

    // Apply --default key=value.
    for (k, v) in &defaults_map {
        toml_changes.push(format!("set default: {k} = {v}"));
        if !dry_run {
            set_default_field(&mut doc, type_name, k, v);
        }
    }

    // Apply --property-values (enum constraint; wins over --property-type for same key).
    for (k, vals) in &prop_values_map {
        toml_changes.push(format!(
            "set property {k}: type=enum, values=[{}]",
            vals.join(", ")
        ));
        if !dry_run {
            set_property_enum(&mut doc, type_name, k, vals);
        }
        // Remove from prop_type_map so it isn't also applied below.
        prop_type_map.remove(k.as_str());
    }

    // Apply --property-type for remaining (non-enum) entries.
    for (k, pt) in &prop_type_map {
        toml_changes.push(format!("set property {k}: type={pt}"));
        if !dry_run {
            set_property_type_field(&mut doc, type_name, k, pt);
        }
    }

    // Auto-create string property entries for required fields without constraints.
    if !dry_run {
        for f in &required_fields {
            // Skip if this field already has a property-type or property-values in this invocation.
            if prop_type_map.contains_key(f.as_str()) || prop_values_map.contains_key(f.as_str()) {
                continue;
            }
            // Skip if the property already exists in TOML.
            let already_has = doc
                .get("schema")
                .and_then(|s| s.as_table())
                .and_then(|t| t.get("types"))
                .and_then(|t| t.as_table())
                .and_then(|t| t.get(type_name))
                .and_then(|t| t.as_table())
                .and_then(|t| t.get("properties"))
                .and_then(|t| t.as_table())
                .and_then(|t| t.get(f.as_str()))
                .is_some();
            if !already_has {
                set_property_type_field(&mut doc, type_name, f, "string");
                toml_changes.push(format!("auto-add property {f}: type=string"));
            }
        }
    }

    // Write TOML to disk (unless dry-run).
    if !dry_run {
        write_toml_doc(&doc)?;
    }

    // --- Side effects: --default auto-apply ---
    let mut defaults_applied: Vec<Value> = Vec::new();

    if !defaults_map.is_empty() {
        let all_vault_files = discovery::discover_files(dir)?;
        let mut per_default_files: HashMap<String, Vec<String>> = HashMap::new();

        for full_path in &all_vault_files {
            let Ok(props) = read_frontmatter(full_path) else {
                continue;
            };
            let file_type = props
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if file_type != type_name {
                continue;
            }
            let rel = discovery::relative_path(dir, full_path);

            // Find which defaults this file is missing.
            let mut file_needs: HashMap<String, String> = HashMap::new();
            for key in defaults_map.keys() {
                if !props.contains_key(key.as_str()) {
                    let expanded = expanded_defaults.get(key).cloned().unwrap_or_default();
                    file_needs.insert(key.clone(), expanded);
                    per_default_files
                        .entry(key.clone())
                        .or_default()
                        .push(rel.clone());
                }
            }

            if !dry_run && !file_needs.is_empty() {
                let mut new_props = props.clone();
                for (key, expanded) in &file_needs {
                    // If the user declared a non-string property-type in this
                    // same invocation, coerce the default to the matching
                    // JSON type so we don't write `archived: "true"` when the
                    // user intended `archived: true`.
                    let typed = coerce_default_for_prop(
                        expanded,
                        prop_type_map.get(key.as_str()).copied(),
                        prop_values_map.contains_key(key.as_str()),
                    );
                    new_props.insert(key.clone(), typed);
                }
                write_frontmatter(full_path, &new_props)
                    .with_context(|| format!("writing defaults to {rel}"))?;
            }
        }

        for key in defaults_map.keys() {
            let expanded = expanded_defaults.get(key).cloned().unwrap_or_default();
            let applied_files = per_default_files.get(key).cloned().unwrap_or_default();
            let count = applied_files.len();
            defaults_applied.push(serde_json::json!({
                "property": key,
                "value": expanded,
                "files": applied_files,
                "count": count,
            }));
        }
    }

    // --- Side effects: constraint violation reporting ---
    let needs_violation_check =
        !required_fields.is_empty() || !prop_type_map.is_empty() || !prop_values_map.is_empty();

    let mut constraint_violations: Vec<Value> = Vec::new();

    if needs_violation_check && !dry_run {
        let updated_schema = load_schema_from_doc(&doc)?;
        let all_vault_files = discovery::discover_files(dir)?;

        let file_pairs: Vec<(std::path::PathBuf, String)> = all_vault_files
            .iter()
            .filter(|p| {
                read_frontmatter(p)
                    .ok()
                    .and_then(|props| {
                        props
                            .get("type")
                            .and_then(|v| v.as_str())
                            .map(|t| t == type_name)
                    })
                    .unwrap_or(false)
            })
            .map(|p| {
                let rel = discovery::relative_path(dir, p);
                (p.clone(), rel)
            })
            .collect();

        let counts = crate::commands::lint::lint_counts_only(&file_pairs, &updated_schema)?;

        if counts.errors > 0 || counts.warnings > 0 {
            constraint_violations.push(serde_json::json!({
                "file_count": counts.files_with_issues,
                "error_count": counts.errors,
                "warning_count": counts.warnings,
                "message": "Run `hyalo lint` for details.",
            }));
        }
    }

    let val = serde_json::json!({
        "action": if is_new { "created_and_updated" } else { "updated" },
        "type": type_name,
        "dry_run": dry_run,
        "toml_changes": toml_changes,
        "defaults_applied": defaults_applied,
        "constraint_violations": constraint_violations,
    });

    Ok(CommandOutcome::success(format_success(Format::Json, &val)))
}

// ---------------------------------------------------------------------------
// TOML helpers (toml_edit)
// ---------------------------------------------------------------------------

/// Read `.hyalo.toml` as a `DocumentMut`, or return an empty doc if not found.
fn read_toml_doc() -> Result<toml_edit::DocumentMut> {
    match fs::read_to_string(TOML_PATH) {
        Ok(contents) => contents
            .parse::<toml_edit::DocumentMut>()
            .context("failed to parse .hyalo.toml"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(toml_edit::DocumentMut::new()),
        Err(e) => Err(e).context("failed to read .hyalo.toml"),
    }
}

/// Write a `DocumentMut` back to `.hyalo.toml`.
fn write_toml_doc(doc: &toml_edit::DocumentMut) -> Result<()> {
    fs::write(TOML_PATH, doc.to_string()).context("failed to write .hyalo.toml")
}

/// Returns `true` when `[schema.types.<name>]` exists in the doc.
fn toml_type_exists(doc: &toml_edit::DocumentMut, type_name: &str) -> bool {
    doc.get("schema")
        .and_then(|s| s.as_table())
        .and_then(|t| t.get("types"))
        .and_then(|t| t.as_table())
        .and_then(|t| t.get(type_name))
        .is_some()
}

/// Ensure `[schema]` and `[schema.types]` tables exist in the doc.
///
/// Returns a user-facing error if `schema` or `schema.types` exist but are not
/// TOML tables (e.g. the user hand-edited `.hyalo.toml` into something like
/// `schema = "foo"`). We prefer a clear error over a panic on user input.
fn ensure_schema_types_table(doc: &mut toml_edit::DocumentMut) -> Result<(), String> {
    if !doc.contains_key("schema") {
        doc["schema"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let schema = doc["schema"].as_table_mut().ok_or_else(|| {
        "malformed .hyalo.toml: `schema` is not a table — expected `[schema]` section".to_owned()
    })?;
    if !schema.contains_key("types") {
        schema.insert("types", toml_edit::Item::Table(toml_edit::Table::new()));
    }
    if !schema["types"].is_table() {
        return Err(
            "malformed .hyalo.toml: `schema.types` is not a table — expected `[schema.types]` section".to_owned(),
        );
    }
    Ok(())
}

/// Ensure `[schema.types.<name>.defaults]` table exists.
fn ensure_defaults_table(doc: &mut toml_edit::DocumentMut, type_name: &str) {
    let schema = doc["schema"].as_table_mut().expect("schema is a table");
    let types = schema["types"].as_table_mut().expect("types is a table");
    let type_table = types[type_name]
        .as_table_mut()
        .expect("type entry is a table");
    if !type_table.contains_key("defaults") {
        type_table.insert("defaults", toml_edit::Item::Table(toml_edit::Table::new()));
    }
}

/// Ensure `[schema.types.<name>.properties.<prop>]` table exists.
fn ensure_property_table(doc: &mut toml_edit::DocumentMut, type_name: &str, prop: &str) {
    let schema = doc["schema"].as_table_mut().expect("schema is a table");
    let types = schema["types"].as_table_mut().expect("types is a table");
    let type_table = types[type_name]
        .as_table_mut()
        .expect("type entry is a table");
    if !type_table.contains_key("properties") {
        type_table.insert(
            "properties",
            toml_edit::Item::Table(toml_edit::Table::new()),
        );
    }
    let props = type_table["properties"]
        .as_table_mut()
        .expect("properties is a table");
    if !props.contains_key(prop) {
        props.insert(prop, toml_edit::Item::Table(toml_edit::Table::new()));
    }
}

/// Get the current `required` array for a type.
fn get_required_array(doc: &toml_edit::DocumentMut, type_name: &str) -> Vec<String> {
    doc.get("schema")
        .and_then(|s| s.as_table())
        .and_then(|t| t.get("types"))
        .and_then(|t| t.as_table())
        .and_then(|t| t.get(type_name))
        .and_then(|t| t.as_table())
        .and_then(|t| t.get("required"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Set the `required` array for a type.
fn set_required_array(doc: &mut toml_edit::DocumentMut, type_name: &str, fields: &[String]) {
    let schema = doc["schema"].as_table_mut().expect("schema is a table");
    let types = schema["types"].as_table_mut().expect("types is a table");
    let type_table = types[type_name]
        .as_table_mut()
        .expect("type entry is a table");
    let mut arr = toml_edit::Array::new();
    for f in fields {
        arr.push(f.as_str());
    }
    type_table["required"] = toml_edit::Item::Value(toml_edit::Value::Array(arr));
}

/// Set a string field at `[schema.types.<name>.<key>]`.
fn set_string_field(doc: &mut toml_edit::DocumentMut, type_name: &str, key: &str, value: &str) {
    let schema = doc["schema"].as_table_mut().expect("schema is a table");
    let types = schema["types"].as_table_mut().expect("types is a table");
    let type_table = types[type_name]
        .as_table_mut()
        .expect("type entry is a table");
    type_table[key] = toml_edit::value(value);
}

/// Set `[schema.types.<name>.defaults.<key>] = value`.
fn set_default_field(doc: &mut toml_edit::DocumentMut, type_name: &str, key: &str, value: &str) {
    ensure_defaults_table(doc, type_name);
    let schema = doc["schema"].as_table_mut().expect("schema is a table");
    let types = schema["types"].as_table_mut().expect("types is a table");
    let type_table = types[type_name]
        .as_table_mut()
        .expect("type entry is a table");
    let defaults = type_table["defaults"]
        .as_table_mut()
        .expect("defaults is a table");
    defaults[key] = toml_edit::value(value);
}

/// Set a simple (non-enum) property constraint: `type = "<pt>"`.
fn set_property_type_field(
    doc: &mut toml_edit::DocumentMut,
    type_name: &str,
    prop: &str,
    pt: &str,
) {
    ensure_property_table(doc, type_name, prop);
    let schema = doc["schema"].as_table_mut().expect("schema is a table");
    let types = schema["types"].as_table_mut().expect("types is a table");
    let type_table = types[type_name]
        .as_table_mut()
        .expect("type entry is a table");
    let props = type_table["properties"]
        .as_table_mut()
        .expect("properties is a table");
    let prop_table = props[prop].as_table_mut().expect("property is a table");
    prop_table["type"] = toml_edit::value(pt);
    // Remove values key if switching away from enum.
    prop_table.remove("values");
}

/// Set an enum property constraint.
fn set_property_enum(
    doc: &mut toml_edit::DocumentMut,
    type_name: &str,
    prop: &str,
    values: &[String],
) {
    ensure_property_table(doc, type_name, prop);
    let schema = doc["schema"].as_table_mut().expect("schema is a table");
    let types = schema["types"].as_table_mut().expect("types is a table");
    let type_table = types[type_name]
        .as_table_mut()
        .expect("type entry is a table");
    let props = type_table["properties"]
        .as_table_mut()
        .expect("properties is a table");
    let prop_table = props[prop].as_table_mut().expect("property is a table");
    prop_table["type"] = toml_edit::value("enum");
    let mut arr = toml_edit::Array::new();
    for v in values {
        arr.push(v.as_str());
    }
    prop_table["values"] = toml_edit::Item::Value(toml_edit::Value::Array(arr));
}

/// Coerce a raw default string to the JSON type implied by the property's
/// constraint in this invocation. Returns `Value::String(raw)` when no typed
/// coercion is possible (the caller declared no matching `--property-type`,
/// or the string could not be parsed as the declared type).
///
/// `pt` is the property type declared with `--property-type` in this
/// invocation (e.g. `"boolean"`); `is_enum` is `true` when
/// `--property-values` was supplied for this key.
fn coerce_default_for_prop(raw: &str, pt: Option<&'static str>, is_enum: bool) -> Value {
    // Enums and explicit "string" declarations always keep string defaults.
    if is_enum {
        return Value::String(raw.to_owned());
    }
    match pt {
        Some("boolean") => match raw {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::String(raw.to_owned()),
        },
        Some("number") => {
            if let Ok(n) = raw.parse::<i64>() {
                Value::Number(n.into())
            } else if let Ok(f) = raw.parse::<f64>()
                && let Some(num) = serde_json::Number::from_f64(f)
            {
                Value::Number(num)
            } else {
                Value::String(raw.to_owned())
            }
        }
        Some("list") => {
            // Comma-separated list.
            let items: Vec<Value> = raw
                .split(',')
                .map(|s| Value::String(s.trim().to_owned()))
                .collect();
            Value::Array(items)
        }
        _ => Value::String(raw.to_owned()),
    }
}

/// Validate a type name: alphanumeric, hyphens, underscores.
///
/// Dots are disallowed because TOML interprets `[schema.types.a.b]` as nested
/// tables, so a type named `a.b` would not round-trip back to itself under
/// `list`/`show`/`set`/`remove`.
fn validate_type_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("type name cannot be empty".to_owned());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "invalid type name '{name}': must contain only alphanumeric characters, hyphens, or underscores"
        ));
    }
    Ok(())
}

/// Re-parse the schema from the current TOML document string.
fn load_schema_from_doc(doc: &toml_edit::DocumentMut) -> Result<SchemaConfig> {
    let toml_str = doc.to_string();
    let table: toml::Value = toml::from_str(&toml_str).context("failed to re-parse TOML")?;
    let raw_schema: hyalo_core::schema::RawSchemaConfig = table
        .get("schema")
        .and_then(|v| v.clone().try_into().ok())
        .unwrap_or(hyalo_core::schema::RawSchemaConfig {
            default: None,
            types: HashMap::new(),
        });
    Ok(SchemaConfig::from(raw_schema))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hyalo_core::schema::{PropertyConstraint, TypeSchema};
    use std::collections::HashMap;

    fn make_schema_with_type(type_name: &str, required: &[&str]) -> SchemaConfig {
        let type_schema = TypeSchema {
            required: required.iter().map(ToString::to_string).collect(),
            ..Default::default()
        };
        let mut types = HashMap::new();
        types.insert(type_name.to_owned(), type_schema);
        SchemaConfig {
            default: TypeSchema::default(),
            types,
        }
    }

    fn make_schema_with_constraint(
        type_name: &str,
        prop: &str,
        constraint: PropertyConstraint,
    ) -> SchemaConfig {
        let mut properties = HashMap::new();
        properties.insert(prop.to_owned(), constraint);
        let type_schema = TypeSchema {
            properties,
            ..Default::default()
        };
        let mut types = HashMap::new();
        types.insert(type_name.to_owned(), type_schema);
        SchemaConfig {
            default: TypeSchema::default(),
            types,
        }
    }

    // --- list_types ---

    #[test]
    fn list_types_empty_schema() {
        let schema = SchemaConfig::default();
        let outcome = list_types(&schema);
        match outcome {
            CommandOutcome::Success { output, total } => {
                let v: serde_json::Value = serde_json::from_str(&output).unwrap();
                assert!(v.as_array().unwrap().is_empty());
                assert_eq!(total, Some(0));
            }
            other => panic!("expected Success, got {other:?}"),
        }
    }

    #[test]
    fn list_types_with_entries() {
        let schema = make_schema_with_type("iteration", &["title", "date"]);
        let outcome = list_types(&schema);
        match outcome {
            CommandOutcome::Success { output, total } => {
                let v: serde_json::Value = serde_json::from_str(&output).unwrap();
                let arr = v.as_array().unwrap();
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0]["type"], "iteration");
                assert_eq!(total, Some(1));
            }
            other => panic!("expected Success, got {other:?}"),
        }
    }

    // --- show_type ---

    #[test]
    fn show_type_not_found() {
        let schema = SchemaConfig::default();
        let outcome = show_type("nonexistent", &schema);
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn show_type_found() {
        let schema = make_schema_with_type("note", &["title"]);
        let outcome = show_type("note", &schema);
        match outcome {
            CommandOutcome::Success { output, .. } => {
                let v: serde_json::Value = serde_json::from_str(&output).unwrap();
                assert_eq!(v["type"], "note");
                assert!(
                    v["required"]
                        .as_array()
                        .unwrap()
                        .contains(&serde_json::json!("title"))
                );
            }
            other => panic!("expected Success, got {other:?}"),
        }
    }

    #[test]
    fn show_type_with_enum_constraint() {
        let schema = make_schema_with_constraint(
            "note",
            "status",
            PropertyConstraint::Enum {
                values: vec!["draft".to_owned(), "published".to_owned()],
            },
        );
        let outcome = show_type("note", &schema);
        match outcome {
            CommandOutcome::Success { output, .. } => {
                let v: serde_json::Value = serde_json::from_str(&output).unwrap();
                assert_eq!(v["properties"]["status"]["type"], "enum");
                let vals = v["properties"]["status"]["values"].as_array().unwrap();
                assert!(vals.contains(&serde_json::json!("draft")));
            }
            other => panic!("expected Success, got {other:?}"),
        }
    }

    // --- validate_type_name ---

    #[test]
    fn validate_type_name_valid() {
        assert!(validate_type_name("iteration").is_ok());
        assert!(validate_type_name("my-type").is_ok());
        assert!(validate_type_name("my_type").is_ok());
    }

    #[test]
    fn validate_type_name_invalid() {
        assert!(validate_type_name("").is_err());
        assert!(validate_type_name("type with spaces").is_err());
        assert!(validate_type_name("type/slash").is_err());
        // Dots are rejected: would create nested TOML tables that don't
        // round-trip to the original type name.
        assert!(validate_type_name("type.v2").is_err());
    }

    // --- parse_property_type_str ---

    #[test]
    fn parse_property_type_valid() {
        assert_eq!(parse_property_type_str("string"), Ok("string"));
        assert_eq!(parse_property_type_str("date"), Ok("date"));
        assert_eq!(parse_property_type_str("number"), Ok("number"));
        assert_eq!(parse_property_type_str("boolean"), Ok("boolean"));
        assert_eq!(parse_property_type_str("list"), Ok("list"));
        assert_eq!(parse_property_type_str("enum"), Ok("enum"));
    }

    #[test]
    fn parse_property_type_invalid() {
        assert!(parse_property_type_str("text").is_err());
        assert!(parse_property_type_str("integer").is_err());
    }

    // --- parse_kv ---

    #[test]
    fn parse_kv_valid() {
        let (k, v) = parse_kv("status=planned", "--default").unwrap();
        assert_eq!(k, "status");
        assert_eq!(v, "planned");
    }

    #[test]
    fn parse_kv_value_with_equals() {
        let (k, v) = parse_kv("url=http://example.com/path=value", "--default").unwrap();
        assert_eq!(k, "url");
        assert_eq!(v, "http://example.com/path=value");
    }

    #[test]
    fn parse_kv_no_equals() {
        assert!(parse_kv("noequalssign", "--default").is_err());
    }

    #[test]
    fn parse_kv_empty_key() {
        assert!(parse_kv("=value", "--default").is_err());
    }
}
