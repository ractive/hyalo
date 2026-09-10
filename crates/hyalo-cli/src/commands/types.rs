/// `hyalo types` — manage document-type schemas in `.hyalo.toml`.
///
/// All TOML mutations use `toml_edit::DocumentMut` so that comments and
/// formatting in the user's config file are preserved.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use hyalo_core::discovery;
use hyalo_core::frontmatter::{read_frontmatter, write_frontmatter_within};
use hyalo_core::schema::{SchemaConfig, expand_default};

use crate::output::{CommandOutcome, Format, output_value, user_diagnostic};

const TOML_FILENAME: &str = ".hyalo.toml";

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

/// `hyalo types list` — list all defined types and their required fields.
pub(crate) fn list_types(schema: &SchemaConfig) -> CommandOutcome {
    let mut sorted_types: Vec<&str> = schema.types.keys().map(String::as_str).collect();
    sorted_types.sort_unstable();

    let results: Vec<TypeListResult> = sorted_types
        .iter()
        .map(|name| {
            let ts = &schema.types[*name];
            TypeListResult {
                r#type: name,
                required: &ts.required,
                has_filename_template: ts.filename_template.is_some(),
                property_count: ts.properties.len(),
            }
        })
        .collect();

    let total = results.len() as u64;
    CommandOutcome::success_with_total(output_value(&results), total)
}

// ---------------------------------------------------------------------------
// show
// ---------------------------------------------------------------------------

/// `hyalo types show <type>` — full merged schema for a type.
pub(crate) fn show_type(type_name: &str, schema: &SchemaConfig, format: Format) -> CommandOutcome {
    if !schema.types.contains_key(type_name) {
        return CommandOutcome::UserError(user_diagnostic(
            format,
            &format!("type '{type_name}' not found"),
            None,
            Some("run 'hyalo types list' to see available types"),
            None,
        ));
    }

    let merged = schema.merged_schema_for_type(type_name);

    let val = crate::output::output_value(
        &(TypeShowResult {
            r#type: type_name,
            required: &merged.required,
            filename_template: merged.filename_template.as_deref(),
            defaults: &merged.defaults,
            properties: merged
                .properties
                .iter()
                .map(|(k, c)| (k.as_str(), ConstraintResult::from(c)))
                .collect(),
            required_sections: &merged.required_sections,
        }),
    );

    CommandOutcome::success(val)
}

// ---------------------------------------------------------------------------
// remove
// ---------------------------------------------------------------------------

/// `hyalo types remove <type>` — remove a type entry.
pub(crate) fn remove_type(dir: &Path, type_name: &str, format: Format) -> Result<CommandOutcome> {
    let toml_path = resolve_toml_path(dir);
    let mut doc = read_toml_doc(&toml_path)?;

    if !toml_type_exists(&doc, type_name) {
        // iter-267 (UX-18): the bare "not found" read as a contradiction —
        // `hyalo lint` was busy reporting schema errors for files whose
        // frontmatter says `type: note`, so the type plainly existed as far as
        // the user could see. It exists in the FILES, not in `.hyalo.toml`:
        // there is no declaration to remove, and lint's complaints come from
        // `[schema.default]` plus the undeclared-type warning. Name both, and
        // the two things that actually change the outcome.
        return Ok(CommandOutcome::UserError(user_diagnostic(
            format,
            &format!("no [schema.types.{type_name}] block in .hyalo.toml — nothing to remove"),
            None,
            Some(&format!(
                "run 'hyalo types list' to see declared types. If `hyalo lint` reports errors for \
                 files whose frontmatter says `type: {type_name}`, those come from \
                 [schema.default] and the undeclared-type warning, not from a type entry — \
                 declare it with 'hyalo types set {type_name} --required …', or exclude those \
                 files with [schema] exempt in .hyalo.toml"
            )),
            None,
        )));
    }

    {
        let schema = doc["schema"]
            .as_table_mut()
            .context("malformed .hyalo.toml: schema is not a table")?;
        let types = schema["types"]
            .as_table_mut()
            .context("malformed .hyalo.toml: schema.types is not a table")?;
        types.remove(type_name);
    }

    let effects = write_toml_doc(&toml_path, &mut doc)?;

    let val = crate::output::output_value(
        &(TypeRemoveResult {
            action: "removed",
            r#type: type_name,
            dry_run: false,
        }),
    );
    Ok(CommandOutcome::success(val).with_apply_report(effects))
}

// ---------------------------------------------------------------------------
// set
// ---------------------------------------------------------------------------

/// Parse a property-type string to a known type string.
fn parse_property_type_str(s: &str) -> Result<&'static str, String> {
    match s {
        "string" => Ok("string"),
        "date" => Ok("date"),
        "datetime" => Ok("datetime"),
        "datetime-tz" => Ok("datetime-tz"),
        "number" => Ok("number"),
        "boolean" => Ok("boolean"),
        "list" => Ok("list"),
        "enum" => Ok("enum"),
        other => Err(format!(
            "invalid property type '{other}': must be one of string, date, datetime, datetime-tz, number, boolean, list, enum"
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
    format: Format,
    case_insensitive_mode: hyalo_core::CaseInsensitiveMode,
) -> Result<CommandOutcome> {
    // Validate type name before anything else.
    if let Err(msg) = validate_type_name(type_name) {
        return Ok(CommandOutcome::UserError(user_diagnostic(
            format, &msg, None, None, None,
        )));
    }

    // Require at least one mutation flag.
    if required_args.is_empty()
        && default_args.is_empty()
        && property_type_args.is_empty()
        && property_values_args.is_empty()
        && filename_template.is_none()
    {
        return Ok(CommandOutcome::UserError(user_diagnostic(
            format,
            "no mutation flags provided — specify at least one of: --required, --default, --property-type, --property-values, --filename-template",
            None,
            None,
            None,
        )));
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
            Err(e) => {
                return Ok(CommandOutcome::UserError(user_diagnostic(
                    format, &e, None, None, None,
                )));
            }
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
                Err(e) => {
                    return Ok(CommandOutcome::UserError(user_diagnostic(
                        format, &e, None, None, None,
                    )));
                }
            },
            Err(e) => {
                return Ok(CommandOutcome::UserError(user_diagnostic(
                    format, &e, None, None, None,
                )));
            }
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
                    return Ok(CommandOutcome::UserError(user_diagnostic(
                        format,
                        &format!(
                            "invalid --property-values argument '{arg}': enum values cannot be empty"
                        ),
                        None,
                        None,
                        None,
                    )));
                }
                prop_values_map.insert(k.to_owned(), vals);
            }
            Err(e) => {
                return Ok(CommandOutcome::UserError(user_diagnostic(
                    format, &e, None, None, None,
                )));
            }
        }
    }

    // Load TOML doc.
    let toml_path = resolve_toml_path(dir);
    let mut doc = read_toml_doc(&toml_path)?;

    // Collect what will change (used for dry-run preview and result).
    let mut toml_changes: Vec<String> = Vec::new();

    // Upsert: create the type if it doesn't exist (in-memory; disk write guarded below).
    let is_new = !toml_type_exists(&doc, type_name);
    if is_new {
        match ensure_schema_types_table(&mut doc) {
            Ok(validate_enabled) => {
                if validate_enabled {
                    toml_changes.push("enable validate_on_write (new schema)".to_owned());
                    // iter-274 (UX-19): creating the first type flips a switch
                    // the caller did not ask for and will feel later — from
                    // here on `set`/`append` REFUSE a value that violates the
                    // schema instead of writing it. Buried in `toml_changes`
                    // that reads as a detail; on stderr it reads as the
                    // behaviour change it is.
                    crate::warn::note(
                        "created the [schema] section and set validate_on_write = true — `set`/`append` now refuse values that violate the schema; set it to false in .hyalo.toml to keep writes unchecked",
                    );
                }
            }
            Err(msg) => {
                return Ok(CommandOutcome::UserError(user_diagnostic(
                    format, &msg, None, None, None,
                )));
            }
        }
        let schema = doc["schema"]
            .as_table_mut()
            .context("malformed .hyalo.toml: schema is not a table")?;
        let types = schema["types"]
            .as_table_mut()
            .context("malformed .hyalo.toml: schema.types is not a table")?;
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
            set_required_array(&mut doc, type_name, &new_required)?;
        }
    }

    // Apply --filename-template.
    if let Some(tmpl) = filename_template {
        toml_changes.push(format!("set filename-template: {tmpl}"));
        if !dry_run {
            set_string_field(&mut doc, type_name, "filename-template", tmpl)?;
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
            set_default_field(&mut doc, type_name, k, v)?;
        }
    }

    // Apply --property-values (enum constraint; wins over --property-type for same key).
    for (k, vals) in &prop_values_map {
        toml_changes.push(format!(
            "set property {k}: type=enum, values=[{}]",
            vals.join(", ")
        ));
        if !dry_run {
            set_property_enum(&mut doc, type_name, k, vals)?;
        }
        // Remove from prop_type_map so it isn't also applied below.
        prop_type_map.remove(k.as_str());
    }

    // Apply --property-type for remaining (non-enum) entries.
    for (k, pt) in &prop_type_map {
        toml_changes.push(format!("set property {k}: type={pt}"));
        if !dry_run {
            set_property_type_field(&mut doc, type_name, k, pt)?;
        }
    }

    // Auto-create string property entries for required fields without constraints.
    for f in &required_fields {
        // Skip if this field already has a property-type or property-values in this invocation.
        if prop_type_map.contains_key(f.as_str()) || prop_values_map.contains_key(f.as_str()) {
            continue;
        }
        // Skip if the property already exists in the type-local or default TOML table.
        let schema_item = doc.get("schema").and_then(|s| s.as_table());
        let in_type = schema_item
            .and_then(|t| t.get("types"))
            .and_then(|t| t.as_table())
            .and_then(|t| t.get(type_name))
            .and_then(|t| t.as_table())
            .and_then(|t| t.get("properties"))
            .and_then(|t| t.as_table())
            .and_then(|t| t.get(f.as_str()))
            .is_some();
        let in_default = schema_item
            .and_then(|t| t.get("default"))
            .and_then(|t| t.as_table())
            .and_then(|t| t.get("properties"))
            .and_then(|t| t.as_table())
            .and_then(|t| t.get(f.as_str()))
            .is_some();
        let already_has = in_type || in_default;
        if !already_has {
            // iter-266 DEC-281: infer the property type from what the vault
            // already holds for this key on files of this type, instead of
            // hardcoding `string`. On a vault where `categories` is a list
            // everywhere, the hardcoded `string` declared a constraint every
            // file violated the moment it was written.
            let inferred = infer_property_type_from_vault(dir, type_name, f);
            toml_changes.push(format!("auto-add property {f}: type={inferred}"));
            if !dry_run {
                set_property_type_field(&mut doc, type_name, f, inferred)?;
            }
        }
    }

    // Write TOML to disk (unless dry-run).
    let mut effects = if dry_run {
        None
    } else {
        Some(write_toml_doc(&toml_path, &mut doc)?)
    };
    if effects
        .as_ref()
        .is_some_and(super::apply::ApplyReport::failed)
    {
        return Ok(CommandOutcome::success(serde_json::Value::Null)
            .with_apply_report(effects.take().expect("checked report")));
    }
    let result = (|| -> Result<CommandOutcome> {
        // --- Side effects: --default auto-apply ---
        let mut defaults_applied: Vec<DefaultAppliedOwned> = Vec::new();

        if !defaults_map.is_empty() {
            let all_vault_files = discovery::discover_files(dir)?;
            let mut per_default_files: HashMap<String, Vec<String>> = HashMap::new();

            for full_path in &all_vault_files {
                let Ok(props) = read_frontmatter(full_path) else {
                    continue;
                };
                let file_type = props
                    .get("type")
                    .and_then(hyalo_core::schema::normalize_type_value)
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
                    write_frontmatter_within(dir, full_path, &new_props)
                        .with_context(|| format!("writing defaults to {rel}"))?;
                    if let Some(report) = &mut effects {
                        report.paths.push(super::apply::PathEffect {
                            file: rel,
                            state: super::apply::EffectState::Committed,
                            error: None,
                            category: None,
                        });
                    }
                }
            }

            for key in defaults_map.keys() {
                let expanded = expanded_defaults.get(key).cloned().unwrap_or_default();
                let applied_files = per_default_files.get(key).cloned().unwrap_or_default();
                let count = applied_files.len();
                defaults_applied.push(DefaultAppliedOwned {
                    property: key.to_owned(),
                    value: expanded,
                    files: applied_files,
                    count,
                });
            }
        }

        // --- Side effects: constraint violation reporting ---
        let needs_violation_check =
            !required_fields.is_empty() || !prop_type_map.is_empty() || !prop_values_map.is_empty();

        let mut constraint_violations: Vec<ConstraintViolationsOwned> = Vec::new();

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
                                .and_then(hyalo_core::schema::normalize_type_value)
                                .map(|t| t == type_name)
                        })
                        .unwrap_or(false)
                })
                .map(|p| {
                    let rel = discovery::relative_path(dir, p);
                    (p.clone(), rel)
                })
                .collect();

            // Resolved once here (only when a violation check is actually
            // needed) rather than probing the filesystem unconditionally on
            // every `hyalo types set` invocation.
            let case_insensitive = hyalo_core::mode_enabled(case_insensitive_mode, dir);
            let counts = crate::commands::lint::lint_counts_only(
                &file_pairs,
                &updated_schema,
                case_insensitive,
            )?;

            if counts.errors > 0 || counts.warnings > 0 {
                constraint_violations.push(ConstraintViolationsOwned {
                    file_count: counts.files_with_issues,
                    error_count: counts.errors,
                    warning_count: counts.warnings,
                    message: "Run `hyalo lint` for details.",
                });
            }
        }

        let val = crate::output::output_value(
            &(TypeSetResult {
                action: if is_new {
                    "created_and_updated"
                } else {
                    "updated"
                },
                r#type: type_name,
                dry_run,
                toml_changes: &toml_changes,
                defaults_applied: &defaults_applied,
                constraint_violations: &constraint_violations,
            }),
        );

        Ok(CommandOutcome::success(val))
    })();
    match (result, effects) {
        (Ok(outcome), Some(report)) => Ok(outcome.with_apply_report(report)),
        (result, None) => result,
        (Err(error), Some(report)) => {
            let mut diagnostic = crate::output::UserDiagnostic::new(error.to_string());
            diagnostic.effects = Some(report);
            diagnostic.category = Some("mutation_failure");
            Ok(CommandOutcome::UserError(diagnostic))
        }
    }
}

// ---------------------------------------------------------------------------
// TOML helpers (toml_edit)
// ---------------------------------------------------------------------------

/// Returns the path to `.hyalo.toml` within the given directory.
fn resolve_toml_path(dir: &Path) -> PathBuf {
    dir.join(TOML_FILENAME)
}

/// Read `.hyalo.toml` as a `DocumentMut`, or return an empty doc if not found.
struct CapturedToml {
    doc: toml_edit::DocumentMut,
    source: Option<hyalo_core::rooted::CapturedInput>,
}
impl std::ops::Deref for CapturedToml {
    type Target = toml_edit::DocumentMut;
    fn deref(&self) -> &Self::Target {
        &self.doc
    }
}
impl std::ops::DerefMut for CapturedToml {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.doc
    }
}
fn read_toml_doc(toml_path: &Path) -> Result<CapturedToml> {
    let root =
        hyalo_core::rooted::ConfigRoot::new(toml_path.parent().context("config has no parent")?)?;
    let name = hyalo_core::rooted::RelativeName::new(
        toml_path.file_name().context("config has no name")?,
    )?;
    match root.capture(&name) {
        Ok(source) => {
            let contents = String::from_utf8(source.bytes()?).context("config is not UTF-8")?;
            let doc = contents.parse().context("failed to parse .hyalo.toml")?;
            Ok(CapturedToml {
                doc,
                source: Some(source),
            })
        }
        Err(e)
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(CapturedToml {
                doc: toml_edit::DocumentMut::new(),
                source: None,
            })
        }
        Err(e) => Err(e).context("failed to read .hyalo.toml"),
    }
}

/// Publish only the config whose bytes supplied this transformed document.
fn write_toml_doc(toml_path: &Path, doc: &mut CapturedToml) -> Result<super::apply::ApplyReport> {
    write_toml_doc_with_session(
        toml_path,
        doc,
        hyalo_core::rooted::WriteSession::new(hyalo_core::rooted::Durability::PerFile),
    )
}

fn write_toml_doc_with_session(
    toml_path: &Path,
    doc: &mut CapturedToml,
    mut session: hyalo_core::rooted::WriteSession,
) -> Result<super::apply::ApplyReport> {
    use super::apply::{ApplyReport, EffectFailure, EffectState, IndexDisposition, PathEffect};
    use hyalo_core::rooted::{ConfigRoot, RelativeName};
    let root = ConfigRoot::new(toml_path.parent().context("config has no parent")?)?;
    let name = RelativeName::new(toml_path.file_name().context("config has no name")?)?;
    let bytes = doc.to_string();
    let effect = if let Some(source) = doc.source.take() {
        source
            .prepare(bytes.as_bytes(), &session)?
            .commit(&mut session)?
    } else {
        root.destination(name)?
            .create(bytes.as_bytes(), &mut session)?
    };
    let finish_error = session.finish().err().map(|error| error.to_string());
    let error = effect
        .finalization_error()
        .map(str::to_owned)
        .or(finish_error);
    Ok(ApplyReport {
        paths: vec![PathEffect {
            file: toml_path.display().to_string(),
            state: if error.is_some() {
                EffectState::CommittedWithFinalizationError
            } else {
                EffectState::Committed
            },
            category: error.as_ref().map(|_| EffectFailure::Finalization),
            error,
        }],
        index: IndexDisposition::NotUsed,
        index_error: None,
    })
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
/// When the `[schema]` section is created for the first time, also sets
/// `validate_on_write = true` so that future `set`/`append` operations
/// enforce schema constraints by default. Existing `[schema]` sections
/// (e.g. hand-edited or from a prior `types set`) are left untouched.
///
/// Returns a user-facing error if `schema` or `schema.types` exist but are not
/// TOML tables (e.g. the user hand-edited `.hyalo.toml` into something like
/// `schema = "foo"`). We prefer a clear error over a panic on user input.
/// Returns `Ok(true)` when `validate_on_write` was auto-enabled (schema was new).
fn ensure_schema_types_table(doc: &mut toml_edit::DocumentMut) -> Result<bool, String> {
    let schema_is_new = !doc.contains_key("schema");
    if schema_is_new {
        doc["schema"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let schema = doc["schema"].as_table_mut().ok_or_else(|| {
        "malformed .hyalo.toml: `schema` is not a table — expected `[schema]` section".to_owned()
    })?;
    // When creating the [schema] section for the first time, enable
    // validate_on_write so that `set`/`append` enforce schema constraints
    // by default once any type has been defined.
    if schema_is_new {
        schema.insert(
            "validate_on_write",
            toml_edit::Item::Value(toml_edit::Value::Boolean(toml_edit::Formatted::new(true))),
        );
    }
    if !schema.contains_key("types") {
        schema.insert("types", toml_edit::Item::Table(toml_edit::Table::new()));
    }
    if !schema["types"].is_table() {
        return Err(
            "malformed .hyalo.toml: `schema.types` is not a table — expected `[schema.types]` section".to_owned(),
        );
    }
    Ok(schema_is_new)
}

/// Ensure `[schema.types.<name>.defaults]` table exists.
fn ensure_defaults_table(doc: &mut toml_edit::DocumentMut, type_name: &str) -> Result<()> {
    let schema = doc["schema"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema is not a table")?;
    let types = schema["types"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema.types is not a table")?;
    let type_table = types[type_name]
        .as_table_mut()
        .context("malformed .hyalo.toml: type entry is not a table")?;
    if !type_table.contains_key("defaults") {
        type_table.insert("defaults", toml_edit::Item::Table(toml_edit::Table::new()));
    }
    Ok(())
}

/// Ensure `[schema.types.<name>.properties.<prop>]` table exists.
fn ensure_property_table(
    doc: &mut toml_edit::DocumentMut,
    type_name: &str,
    prop: &str,
) -> Result<()> {
    let schema = doc["schema"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema is not a table")?;
    let types = schema["types"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema.types is not a table")?;
    let type_table = types[type_name]
        .as_table_mut()
        .context("malformed .hyalo.toml: type entry is not a table")?;
    if !type_table.contains_key("properties") {
        type_table.insert(
            "properties",
            toml_edit::Item::Table(toml_edit::Table::new()),
        );
    }
    let props = type_table["properties"]
        .as_table_mut()
        .context("malformed .hyalo.toml: properties section is not a table")?;
    if !props.contains_key(prop) {
        props.insert(prop, toml_edit::Item::Table(toml_edit::Table::new()));
    }
    Ok(())
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
fn set_required_array(
    doc: &mut toml_edit::DocumentMut,
    type_name: &str,
    fields: &[String],
) -> Result<()> {
    let schema = doc["schema"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema is not a table")?;
    let types = schema["types"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema.types is not a table")?;
    let type_table = types[type_name]
        .as_table_mut()
        .context("malformed .hyalo.toml: type entry is not a table")?;
    let mut arr = toml_edit::Array::new();
    for f in fields {
        arr.push(f.as_str());
    }
    type_table["required"] = toml_edit::Item::Value(toml_edit::Value::Array(arr));
    Ok(())
}

/// Set a string field at `[schema.types.<name>.<key>]`.
fn set_string_field(
    doc: &mut toml_edit::DocumentMut,
    type_name: &str,
    key: &str,
    value: &str,
) -> Result<()> {
    let schema = doc["schema"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema is not a table")?;
    let types = schema["types"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema.types is not a table")?;
    let type_table = types[type_name]
        .as_table_mut()
        .context("malformed .hyalo.toml: type entry is not a table")?;
    type_table[key] = toml_edit::value(value);
    Ok(())
}

/// Set `[schema.types.<name>.defaults.<key>] = value`.
fn set_default_field(
    doc: &mut toml_edit::DocumentMut,
    type_name: &str,
    key: &str,
    value: &str,
) -> Result<()> {
    ensure_defaults_table(doc, type_name)?;
    let schema = doc["schema"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema is not a table")?;
    let types = schema["types"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema.types is not a table")?;
    let type_table = types[type_name]
        .as_table_mut()
        .context("malformed .hyalo.toml: type entry is not a table")?;
    let defaults = type_table["defaults"]
        .as_table_mut()
        .context("malformed .hyalo.toml: defaults section is not a table")?;
    defaults[key] = toml_edit::value(value);
    Ok(())
}

/// Infer the schema property type to auto-declare for a newly required field
/// (iter-266 SCHEMA-1, DEC-281).
///
/// Scans the vault for files whose `type:` normalises to `type_name` and takes
/// the most common inferred type of their `key` values. Falls back to `string`
/// when nothing in the vault carries the key — the historical default — so a
/// brand-new type behaves exactly as before.
///
/// Best-effort by design: a file whose frontmatter will not parse is simply
/// not consulted, and a tie is broken by the type name so the result is
/// deterministic.
fn infer_property_type_from_vault(
    dir: &std::path::Path,
    type_name: &str,
    key: &str,
) -> &'static str {
    const DEFAULT: &str = "string";
    let Ok(files) = discovery::discover_files(dir) else {
        return DEFAULT;
    };
    let mut counts: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    for full_path in &files {
        let Ok(props) = read_frontmatter(full_path) else {
            continue;
        };
        let matches_type = props
            .get("type")
            .and_then(hyalo_core::schema::normalize_type_value)
            .is_some_and(|t| t == type_name);
        if !matches_type {
            continue;
        }
        let Some(value) = props.get(key) else {
            continue;
        };
        // `null` says nothing about the intended type.
        if value.is_null() {
            continue;
        }
        // Map hyalo's inferred value type onto the schema's property-type
        // vocabulary; anything without a counterpart stays `string`.
        let pt = match hyalo_core::frontmatter::infer_type(value) {
            "number" => "number",
            "checkbox" => "boolean",
            "list" => "list",
            "date" => "date",
            "datetime" => "datetime",
            "datetime-tz" => "datetime-tz",
            _ => DEFAULT,
        };
        *counts.entry(pt).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(name, count)| (*count, std::cmp::Reverse(*name)))
        .map_or(DEFAULT, |(name, _)| name)
}

/// Set a simple (non-enum) property constraint: `type = "<pt>"`.
fn set_property_type_field(
    doc: &mut toml_edit::DocumentMut,
    type_name: &str,
    prop: &str,
    pt: &str,
) -> Result<()> {
    ensure_property_table(doc, type_name, prop)?;
    let schema = doc["schema"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema is not a table")?;
    let types = schema["types"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema.types is not a table")?;
    let type_table = types[type_name]
        .as_table_mut()
        .context("malformed .hyalo.toml: type entry is not a table")?;
    let props = type_table["properties"]
        .as_table_mut()
        .context("malformed .hyalo.toml: properties section is not a table")?;
    let prop_table = props[prop]
        .as_table_mut()
        .context("malformed .hyalo.toml: property entry is not a table")?;
    prop_table["type"] = toml_edit::value(pt);
    // Remove values key if switching away from enum.
    prop_table.remove("values");
    Ok(())
}

/// Set an enum property constraint.
fn set_property_enum(
    doc: &mut toml_edit::DocumentMut,
    type_name: &str,
    prop: &str,
    values: &[String],
) -> Result<()> {
    ensure_property_table(doc, type_name, prop)?;
    let schema = doc["schema"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema is not a table")?;
    let types = schema["types"]
        .as_table_mut()
        .context("malformed .hyalo.toml: schema.types is not a table")?;
    let type_table = types[type_name]
        .as_table_mut()
        .context("malformed .hyalo.toml: type entry is not a table")?;
    let props = type_table["properties"]
        .as_table_mut()
        .context("malformed .hyalo.toml: properties section is not a table")?;
    let prop_table = props[prop]
        .as_table_mut()
        .context("malformed .hyalo.toml: property entry is not a table")?;
    prop_table["type"] = toml_edit::value("enum");
    let mut arr = toml_edit::Array::new();
    for v in values {
        arr.push(v.as_str());
    }
    prop_table["values"] = toml_edit::Item::Value(toml_edit::Value::Array(arr));
    Ok(())
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
    // `default` is reserved: it names the fallback `[schema.default]` table, not
    // a concrete type. `hyalo types set default …` would silently create a
    // phantom `[schema.types.default]` type that never binds to anything
    // (user-service BUG-A3). Reject it and point at the real table.
    if name.eq_ignore_ascii_case("default") {
        return Err(
            "'default' is a reserved name for the fallback schema — edit \
             `[schema.default]` in .hyalo.toml directly (it applies to every \
             untyped file); `types set` only manages concrete `[schema.types.*]` types"
                .to_owned(),
        );
    }
    // A dot would make toml_edit create *nested* tables
    // (`[schema.types.type.v2]`), which no longer round-trips to the original
    // type name, so it stays rejected. Spaces are allowed: a quoted TOML key
    // (`[schema.types."Data Table"]`) round-trips cleanly and is a safe
    // frontmatter `type:` value, and hand-declared spaced types already work
    // end to end — the CLI restriction was the odd one out (BUG-4).
    if name.contains('.') {
        return Err(format!(
            "invalid type name '{name}': dots are not allowed (they create nested TOML tables that don't round-trip to the original name)"
        ));
    }
    // Leading/trailing whitespace and interior control characters are rejected:
    // they either don't round-trip predictably or make confusing frontmatter.
    if name.trim() != name {
        return Err(format!(
            "invalid type name '{name}': leading or trailing whitespace is not allowed"
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ')
    {
        return Err(format!(
            "invalid type name '{name}': must contain only alphanumeric characters, hyphens, underscores, or spaces"
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
            exempt: Vec::new(),
            bind: Vec::new(),
            validate_on_write: None,
        });
    Ok(SchemaConfig::from_raw_lossy(raw_schema))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::items_after_test_module)] // dispatch handler appended below (ARCH-1, iter-225)
mod tests {
    #[test]
    fn config_finalization_failure_retains_persisted_bytes_and_typed_effect() {
        use hyalo_core::rooted::{Durability, FaultPoint, WriteSession};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".hyalo.toml");
        std::fs::write(&path, "dir = \".\"\n").unwrap();
        let mut doc = super::read_toml_doc(&path).unwrap();
        doc.doc["format"] = toml_edit::value("json");
        let report = super::write_toml_doc_with_session(
            &path,
            &mut doc,
            WriteSession::with_fault(Durability::PerFile, FaultPoint::Finalize),
        )
        .unwrap();
        let outcome = CommandOutcome::success(serde_json::Value::Null).with_apply_report(report);
        let CommandOutcome::UserError(diagnostic) = outcome else {
            panic!("finalization is not a clean success");
        };
        let value: serde_json::Value =
            serde_json::from_str(&diagnostic.render(Format::Json)).unwrap();
        assert_eq!(
            value["effects"]["paths"][0]["state"],
            "committed_with_finalization_error"
        );
        assert_eq!(value["effects"]["paths"][0]["category"], "finalization");
        assert_eq!(value["effects"]["index"], "not_used");
        assert_eq!(
            value["effects"]["paths"][0]["file"],
            path.display().to_string()
        );
        assert!(
            std::fs::read_to_string(path)
                .unwrap()
                .contains("format = \"json\"")
        );
    }

    #[test]
    fn config_publication_checks_the_capture_used_for_transformation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".hyalo.toml");
        std::fs::write(&path, "dir = \".\"\n").unwrap();
        let mut document = super::read_toml_doc(&path).unwrap();
        document.doc["format"] = toml_edit::value("json");
        std::fs::write(&path, "dir = \"other\"\n").unwrap();
        let error = super::write_toml_doc(&path, &mut document).unwrap_err();
        assert!(
            error
                .downcast_ref::<hyalo_core::rooted::SourceConflict>()
                .is_some()
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), "dir = \"other\"\n");
    }

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
            ..Default::default()
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
            ..Default::default()
        }
    }

    // --- list_types ---

    #[test]
    fn list_types_empty_schema() {
        let schema = SchemaConfig::default();
        let outcome = list_types(&schema);
        match outcome {
            CommandOutcome::Success { output, total, .. } => {
                let v: serde_json::Value = serde_json::from_value(output).unwrap();
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
            CommandOutcome::Success { output, total, .. } => {
                let v: serde_json::Value = serde_json::from_value(output).unwrap();
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
        let outcome = show_type("nonexistent", &schema, Format::Json);
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn show_type_found() {
        let schema = make_schema_with_type("note", &["title"]);
        let outcome = show_type("note", &schema, Format::Json);
        match outcome {
            CommandOutcome::Success { output, .. } => {
                let v: serde_json::Value = serde_json::from_value(output).unwrap();
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
        let outcome = show_type("note", &schema, Format::Json);
        match outcome {
            CommandOutcome::Success { output, .. } => {
                let v: serde_json::Value = serde_json::from_value(output).unwrap();
                assert_eq!(v["properties"]["status"]["type"], "enum");
                let vals = v["properties"]["status"]["values"].as_array().unwrap();
                assert!(vals.contains(&serde_json::json!("draft")));
            }
            other => panic!("expected Success, got {other:?}"),
        }
    }

    #[test]
    fn show_type_with_object_list_constraint() {
        let mut key_patterns = indexmap::IndexMap::new();
        key_patterns.insert("commit".to_owned(), "^[0-9a-f]{7,40}$".to_owned());
        let schema = make_schema_with_constraint(
            "memory",
            "sources",
            PropertyConstraint::ObjectList {
                required_keys: vec!["ref".to_owned()],
                allowed_keys: Some(vec!["ref".to_owned(), "commit".to_owned()]),
                key_patterns,
            },
        );
        let outcome = show_type("memory", &schema, Format::Json);
        match outcome {
            CommandOutcome::Success { output, .. } => {
                let v: serde_json::Value = serde_json::from_value(output).unwrap();
                let sources = &v["properties"]["sources"];
                assert_eq!(sources["type"], "object-list");
                assert_eq!(sources["required-keys"], serde_json::json!(["ref"]));
                assert_eq!(
                    sources["allowed-keys"],
                    serde_json::json!(["ref", "commit"])
                );
                assert_eq!(sources["key-patterns"]["commit"], "^[0-9a-f]{7,40}$");
            }
            other => panic!("expected Success, got {other:?}"),
        }
    }

    #[test]
    fn show_type_object_list_omits_absent_allowed_keys_and_empty_patterns() {
        let schema = make_schema_with_constraint(
            "memory",
            "sources",
            PropertyConstraint::ObjectList {
                required_keys: Vec::new(),
                allowed_keys: None,
                key_patterns: indexmap::IndexMap::new(),
            },
        );
        let outcome = show_type("memory", &schema, Format::Json);
        match outcome {
            CommandOutcome::Success { output, .. } => {
                let v: serde_json::Value = serde_json::from_value(output).unwrap();
                let sources = v["properties"]["sources"].as_object().unwrap();
                assert!(!sources.contains_key("allowed-keys"));
                assert!(!sources.contains_key("key-patterns"));
                assert_eq!(sources["required-keys"], serde_json::json!([]));
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
        // Spaced type names are valid (BUG-4): they round-trip as quoted TOML
        // keys and are safe frontmatter `type:` values.
        assert!(validate_type_name("Data Table").is_ok());
        assert!(validate_type_name("BigQuery Table").is_ok());
    }

    #[test]
    fn validate_type_name_rejects_reserved_default() {
        let err = validate_type_name("default").unwrap_err();
        assert!(
            err.contains("[schema.default]"),
            "points at the table: {err}"
        );
        // Case-insensitive.
        assert!(validate_type_name("Default").is_err());
    }

    #[test]
    fn set_type_default_is_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = set_type(
            tmp.path(),
            "default",
            &["title".to_owned()],
            &[],
            &[],
            &[],
            None,
            false,
            Format::Json,
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        assert!(
            matches!(outcome, CommandOutcome::UserError(_)),
            "types set default must be a user error"
        );
        // No phantom type table was written.
        let toml_path = tmp.path().join(".hyalo.toml");
        if toml_path.exists() {
            let contents = std::fs::read_to_string(&toml_path).unwrap();
            assert!(
                !contents.contains("[schema.types.default]"),
                "must not create a phantom default type"
            );
        }
    }

    #[test]
    fn validate_type_name_invalid() {
        assert!(validate_type_name("").is_err());
        // Interior spaces are now allowed, but leading/trailing whitespace and
        // other punctuation are not.
        assert!(validate_type_name(" leading").is_err());
        assert!(validate_type_name("trailing ").is_err());
        assert!(validate_type_name("type/slash").is_err());
        assert!(validate_type_name("type$bad").is_err());
        // Dots are rejected: would create nested TOML tables that don't
        // round-trip to the original type name.
        assert!(validate_type_name("type.v2").is_err());
    }

    // --- parse_property_type_str ---

    #[test]
    fn parse_property_type_valid() {
        assert_eq!(parse_property_type_str("string"), Ok("string"));
        assert_eq!(parse_property_type_str("date"), Ok("date"));
        assert_eq!(parse_property_type_str("datetime"), Ok("datetime"));
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

    // --- set_type / remove_type respect --dir ---

    #[test]
    fn set_type_writes_to_custom_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        let outcome = set_type(
            dir,
            "note",
            &["title".to_owned()],
            &[],
            &[],
            &[],
            None,
            false,
            Format::Json,
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::Success { .. }));

        // Config must be written inside the temp dir, not in CWD.
        let toml_path = dir.join(".hyalo.toml");
        assert!(toml_path.exists(), ".hyalo.toml not found in custom dir");
        let contents = std::fs::read_to_string(&toml_path).unwrap();
        assert!(contents.contains("note"), "type 'note' not in written TOML");
    }

    #[test]
    fn remove_type_reads_from_custom_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        // First create the type in the custom dir.
        set_type(
            dir,
            "note",
            &["title".to_owned()],
            &[],
            &[],
            &[],
            None,
            false,
            Format::Json,
            hyalo_core::CaseInsensitiveMode::Off,
        )
        .unwrap();

        // Now remove it — should succeed and update the same file.
        let outcome = remove_type(dir, "note", Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::Success { .. }));

        let toml_path = dir.join(".hyalo.toml");
        let contents = std::fs::read_to_string(&toml_path).unwrap();
        assert!(
            !contents.contains("[schema.types.note]"),
            "type 'note' should have been removed"
        );
    }

    // --- malformed .hyalo.toml error handling (no panics) ---

    #[test]
    fn remove_type_malformed_toml_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        // schema is a string instead of a table — should error, not panic
        std::fs::write(tmp.path().join(".hyalo.toml"), "schema = \"not-a-table\"\n").unwrap();
        let result = remove_type(tmp.path(), "iteration", Format::Json);
        // Should succeed with UserError or Err, but NOT panic
        match result {
            Ok(CommandOutcome::UserError(msg)) => {
                assert!(
                    msg.contains("nothing to remove") || msg.contains("malformed"),
                    "unexpected error: {msg}"
                );
            }
            Err(e) => {
                let msg = format!("{e:#}");
                assert!(
                    msg.contains("malformed") || msg.contains("not a table"),
                    "unexpected error: {msg}"
                );
            }
            other => panic!("expected error for malformed TOML, got: {other:?}"),
        }
    }

    #[test]
    fn set_type_malformed_schema_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".hyalo.toml"),
            "[schema]\ntypes = \"not-a-table\"\n",
        )
        .unwrap();
        let result = set_type(
            tmp.path(),
            "iteration",
            &["title".to_owned()],
            &[],
            &[],
            &[],
            None,
            false,
            Format::Json,
            hyalo_core::CaseInsensitiveMode::Off,
        );
        // Should return an error about malformed TOML, not panic
        assert!(
            result.is_err() || matches!(result, Ok(CommandOutcome::UserError(_))),
            "expected error for malformed TOML"
        );
    }
}

// ---------------------------------------------------------------------------
// Dispatch handler (ARCH-1, iter-225)
// ---------------------------------------------------------------------------

/// The `hyalo types` dispatch arm, extracted verbatim from `dispatch.rs`.
#[allow(clippy::items_after_statements)] // extracted handler keeps its mid-fn imports (ARCH-1, iter-225)
pub(crate) fn run(
    ctx: &mut crate::dispatch::CommandContext<'_>,
    action: Option<crate::cli::args::TypesAction>,
) -> Result<CommandOutcome> {
    let effective_format = ctx.effective_format;
    use crate::cli::args::TypesAction;

    let action = action.unwrap_or(TypesAction::List);
    match action {
        TypesAction::List => Ok(crate::commands::types::list_types(ctx.schema)),
        TypesAction::Show { type_name } => Ok(crate::commands::types::show_type(
            &type_name,
            ctx.schema,
            effective_format,
        )),
        TypesAction::Remove { type_name } => {
            crate::commands::types::remove_type(ctx.config_dir, &type_name, effective_format)
        }
        TypesAction::Set {
            type_name,
            required,
            default,
            property_type,
            property_values,
            filename_template,
            dry_run,
        } => crate::commands::types::set_type(
            ctx.config_dir,
            &type_name,
            &required,
            &default,
            &property_type,
            &property_values,
            filename_template.as_deref(),
            dry_run,
            effective_format,
            ctx.case_insensitive_mode,
        ),
    }
}

/// Serialized TypeListResult command contract.
#[derive(serde::Serialize)]
struct TypeListResult<'a> {
    /// Schema type name.
    #[serde(rename = "type")]
    r#type: &'a str,
    /// Required property names.
    required: &'a [String],
    /// Whether a filename template is configured.
    has_filename_template: bool,
    /// Number of local property constraints.
    property_count: usize,
}

/// Serialized TypeShowResult command contract.
#[derive(serde::Serialize)]
struct TypeShowResult<'a> {
    /// Schema type name.
    #[serde(rename = "type")]
    r#type: &'a str,
    /// Merged required property names.
    required: &'a [String],
    /// Filename template, or null when unset.
    filename_template: Option<&'a str>,
    /// User-named property default templates.
    defaults: &'a HashMap<String, String>,
    /// Typed constraints keyed by user-defined property name.
    properties: std::collections::BTreeMap<&'a str, ConstraintResult<'a>>,
    /// Required body sections in order.
    required_sections: &'a [String],
}

/// Serialized TypeRemoveResult command contract.
#[derive(serde::Serialize)]
struct TypeRemoveResult<'a> {
    /// Configuration mutation action.
    action: &'a str,
    /// Removed schema type.
    #[serde(rename = "type")]
    r#type: &'a str,
    /// Whether this is a preview; always false for remove.
    dry_run: bool,
}

/// Serialized TypeSetResult command contract.
#[derive(serde::Serialize)]
struct TypeSetResult<'a> {
    /// Configuration mutation action.
    action: &'a str,
    /// Updated schema type.
    #[serde(rename = "type")]
    r#type: &'a str,
    /// Whether this is a preview.
    dry_run: bool,
    /// Descriptions of planned configuration changes.
    toml_changes: &'a [String],
    /// Defaults applied to existing documents.
    defaults_applied: &'a [DefaultAppliedOwned],
    /// Aggregated violations after updating the schema.
    constraint_violations: &'a [ConstraintViolationsOwned],
}

/// A default applied to existing documents.
#[derive(serde::Serialize)]
struct DefaultAppliedOwned {
    /// User-defined property name.
    property: String,
    /// Expanded default text.
    value: String,
    /// Affected vault-relative files.
    files: Vec<String>,
    /// Number of affected files.
    count: usize,
}
/// Aggregated schema violations following a type mutation.
#[derive(serde::Serialize)]
struct ConstraintViolationsOwned {
    /// Number of files with violations.
    file_count: usize,
    /// Number of errors.
    error_count: usize,
    /// Number of warnings.
    warning_count: usize,
    /// Follow-up guidance.
    message: &'static str,
}

/// Serialized ConstraintResult command contract.
#[derive(Default, serde::Serialize)]
struct ConstraintResult<'a> {
    /// Constraint kind.
    #[serde(rename = "type")]
    r#type: &'a str,
    /// String pattern when configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pattern: Option<&'a str>,
    /// Minimum string length.
    #[serde(rename = "min-length", skip_serializing_if = "Option::is_none")]
    min_length: Option<usize>,
    /// Maximum string length.
    #[serde(rename = "max-length", skip_serializing_if = "Option::is_none")]
    max_length: Option<usize>,
    /// Minimum numeric value.
    #[serde(skip_serializing_if = "Option::is_none")]
    minimum: Option<f64>,
    /// Maximum numeric value.
    #[serde(skip_serializing_if = "Option::is_none")]
    maximum: Option<f64>,
    /// Allowed enum values.
    #[serde(skip_serializing_if = "Option::is_none")]
    values: Option<&'a [String]>,
    /// Pattern applied to each string-list item.
    #[serde(skip_serializing_if = "Option::is_none")]
    item_pattern: Option<&'a str>,
    /// Required keys for object-list entries, including an empty list.
    #[serde(rename = "required-keys", skip_serializing_if = "Option::is_none")]
    required_keys: Option<&'a [String]>,
    /// Allowed keys for object-list entries.
    #[serde(rename = "allowed-keys", skip_serializing_if = "Option::is_none")]
    allowed_keys: Option<&'a [String]>,
    /// Patterns keyed by user-defined object field name.
    #[serde(rename = "key-patterns", skip_serializing_if = "Option::is_none")]
    key_patterns: Option<&'a indexmap::IndexMap<String, String>>,
}

impl<'a> From<&'a hyalo_core::schema::PropertyConstraint> for ConstraintResult<'a> {
    fn from(c: &'a hyalo_core::schema::PropertyConstraint) -> Self {
        use hyalo_core::schema::PropertyConstraint;
        let mut out = Self::default();
        out.r#type = match c {
            PropertyConstraint::String {
                pattern,
                min_length,
                max_length,
            } => {
                out.pattern = pattern.as_deref();
                out.min_length = *min_length;
                out.max_length = *max_length;
                "string"
            }
            PropertyConstraint::Date => "date",
            PropertyConstraint::DateTime => "datetime",
            PropertyConstraint::DateTimeTz => "datetime-tz",
            PropertyConstraint::Number { minimum, maximum } => {
                out.minimum = *minimum;
                out.maximum = *maximum;
                "number"
            }
            PropertyConstraint::Boolean => "boolean",
            PropertyConstraint::List => "list",
            PropertyConstraint::Enum { values } => {
                out.values = Some(values);
                "enum"
            }
            PropertyConstraint::StringList { item_pattern } => {
                out.item_pattern = item_pattern.as_deref();
                "string-list"
            }
            PropertyConstraint::ObjectList {
                required_keys,
                allowed_keys,
                key_patterns,
            } => {
                out.required_keys = Some(required_keys);
                out.allowed_keys = allowed_keys.as_deref();
                out.key_patterns = (!key_patterns.is_empty()).then_some(key_patterns);
                "object-list"
            }
        };
        out
    }
}
