/// `hyalo new` — create a new markdown file scaffolded from a schema type.
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use anyhow::Context;
use indexmap::IndexMap;

use hyalo_core::index::SnapshotIndex;
use hyalo_core::schema::{PropertyConstraint, SchemaConfig, expand_default, today_iso8601};

use anyhow::Result;

use crate::commands::mutation;
use crate::output::{CommandOutcome, Format, format_error, format_success};

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// `hyalo new --type <name> --file <path>` — scaffold a new file from schema.
pub(crate) fn create_new(
    dir: &Path,
    type_name: &str,
    file_arg: &str,
    schema: &SchemaConfig,
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
    format: Format,
) -> Result<CommandOutcome> {
    // ------------------------------------------------------------------
    // Step 1: validate --type
    // ------------------------------------------------------------------
    if !schema.types.contains_key(type_name) {
        let mut sorted_types: Vec<&str> = schema.types.keys().map(String::as_str).collect();
        sorted_types.sort_unstable();
        let list = if sorted_types.is_empty() {
            "no types defined in schema".to_owned()
        } else {
            format!("available types: {}", sorted_types.join(", "))
        };
        return Ok(CommandOutcome::UserError(format_error(
            format,
            &format!("type '{type_name}' not found"),
            None,
            Some(&list),
            None,
        )));
    }

    // ------------------------------------------------------------------
    // Step 2: validate --file (must be vault-relative, no absolute, no ..)
    // ------------------------------------------------------------------
    let file_path = Path::new(file_arg);
    // Cross-platform validation: walk components rather than splitting on '/'
    // so backslash separators on Windows are also caught.
    for component in file_path.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => {
                return Ok(CommandOutcome::UserError(format_error(
                    format,
                    "invalid file path: must be vault-relative, not absolute",
                    Some(file_arg),
                    Some("omit the leading '/' and provide the path relative to the vault root"),
                    None,
                )));
            }
            Component::ParentDir => {
                return Ok(CommandOutcome::UserError(format_error(
                    format,
                    "invalid file path: '..' traversal is not allowed",
                    Some(file_arg),
                    Some("provide a path relative to the vault root without '..' components"),
                    None,
                )));
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    let full_path: PathBuf = dir.join(file_arg);

    // ------------------------------------------------------------------
    // Step 3: ensure parent directory exists (create if needed)
    // ------------------------------------------------------------------
    if let Some(parent) = full_path.parent()
        && !parent.is_dir()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directories for {}", full_path.display()))?;
    }

    // ------------------------------------------------------------------
    // Step 4 & 6: synthesise and atomically create the file
    // ------------------------------------------------------------------
    let merged = schema.merged_schema_for_type(type_name);
    let content = synthesise_content(type_name, &merged, dir);

    // Pre-flight budget check: reject before touching the filesystem.
    // Extract the YAML content between the --- delimiters.
    if let Some(yaml_part) = content
        .strip_prefix("---\n")
        .and_then(|s| s.find("\n---\n").map(|pos| &s[..=pos]))
        && let Err(budget_err) =
            hyalo_core::frontmatter::check_frontmatter_size_budget(yaml_part, &full_path)
    {
        return Ok(CommandOutcome::UserError(
            crate::output::format_budget_error(format, &budget_err),
        ));
    }

    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&full_path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Ok(CommandOutcome::UserError(format_error(
                format,
                "file already exists; remove it first if you mean to re-create",
                Some(file_arg),
                None,
                None,
            )));
        }
        Err(e) => {
            return Err(e).with_context(|| format!("creating new file at {}", full_path.display()));
        }
    };
    file.write_all(content.as_bytes())
        .with_context(|| format!("writing new file to {}", full_path.display()))?;
    // Drop the file handle so the subsequent index scan sees the final state
    // on platforms (notably Windows) where open writers can interfere with
    // readers, and so mtime reflects the completed write.
    drop(file);

    // ------------------------------------------------------------------
    // Step 6.5: keep the snapshot index in sync (no-op when no index loaded)
    // ------------------------------------------------------------------
    // Normalize to forward slashes so the snapshot's rel_path invariant holds
    // when callers pass Windows-style backslashes via `--file`.
    let rel_path_owned;
    let rel_path: &str = if file_arg.contains('\\') {
        rel_path_owned = file_arg.replace('\\', "/");
        &rel_path_owned
    } else {
        file_arg
    };
    let mut index_dirty = false;
    mutation::add_index_entry(snapshot_index, rel_path, &full_path, &mut index_dirty)?;
    mutation::save_index_if_dirty(snapshot_index, index_path, index_dirty)?;

    // ------------------------------------------------------------------
    // Step 7: output
    // ------------------------------------------------------------------
    let out = match format {
        Format::Text => format!("created {rel_path}\n"),
        Format::Json => {
            let val = serde_json::json!({
                "type": type_name,
                "file": rel_path,
                "created": true,
            });
            format_success(Format::Json, &val)
        }
    };
    Ok(CommandOutcome::success(out))
}

// ---------------------------------------------------------------------------
// Content synthesis
// ---------------------------------------------------------------------------

fn synthesise_content(
    type_name: &str,
    merged: &hyalo_core::schema::TypeSchema,
    _dir: &Path,
) -> String {
    // Build an ordered map of frontmatter properties.
    // `type` always comes first.
    let mut props: IndexMap<String, PropValue> = IndexMap::new();
    props.insert("type".to_owned(), PropValue::Str(type_name.to_owned()));

    for prop_name in &merged.required {
        if prop_name == "type" {
            continue; // already emitted
        }

        // Check if the schema has a default value.
        let default_val = merged.defaults.get(prop_name).map(|d| expand_default(d));

        let value = match merged.properties.get(prop_name.as_str()) {
            Some(PropertyConstraint::Date) => {
                PropValue::Str(default_val.unwrap_or_else(today_iso8601))
            }
            Some(PropertyConstraint::DateTime) => PropValue::Str(
                default_val.unwrap_or_else(|| format!("{}T00:00:00", today_iso8601())),
            ),
            Some(PropertyConstraint::DateTimeTz) => PropValue::Str(
                default_val.unwrap_or_else(|| format!("{}T00:00:00Z", today_iso8601())),
            ),
            Some(PropertyConstraint::Number) => match default_val {
                Some(s) => s
                    .parse::<i64>()
                    .map_or_else(|_| PropValue::Str(s), PropValue::Int),
                None => PropValue::Int(0),
            },
            Some(PropertyConstraint::Boolean) => match default_val {
                Some(s) => match s.as_str() {
                    "true" => PropValue::Bool(true),
                    "false" => PropValue::Bool(false),
                    _ => PropValue::Str(s),
                },
                None => PropValue::Bool(false),
            },
            Some(PropertyConstraint::List | PropertyConstraint::StringList { .. }) => {
                // A default for list properties is uncommon; treat unparseable
                // values as a scalar string fallback rather than fabricating items.
                default_val.map_or(PropValue::EmptyList, PropValue::Str)
            }
            Some(PropertyConstraint::Enum { values }) => {
                if let Some(d) = default_val {
                    PropValue::Str(d)
                } else {
                    let first = values.first().map_or("TBD", |v| v.as_str());
                    PropValue::Str(first.to_owned())
                }
            }
            Some(PropertyConstraint::String { .. }) | None => {
                PropValue::Str(default_val.unwrap_or_else(|| "TBD".to_owned()))
            }
        };
        props.insert(prop_name.clone(), value);
    }

    // Build YAML lines manually for precise control over output format.
    let mut yaml_lines = String::new();
    for (k, v) in &props {
        match v {
            PropValue::Str(s) => {
                let _ = writeln!(yaml_lines, "{k}: {}", yaml_scalar(s));
            }
            PropValue::Bool(b) => {
                let _ = writeln!(yaml_lines, "{k}: {b}");
            }
            PropValue::Int(n) => {
                let _ = writeln!(yaml_lines, "{k}: {n}");
            }
            PropValue::EmptyList => {
                let _ = writeln!(yaml_lines, "{k}: []");
            }
        }
    }

    // Build frontmatter block.
    let mut content = String::from("---\n");
    content.push_str(&yaml_lines);
    content.push_str("---\n");

    // Append required sections.
    if !merged.required_sections.is_empty() {
        content.push('\n');
        for entry in &merged.required_sections {
            content.push_str(entry);
            content.push('\n');
            content.push('\n');
            content.push_str("TBD\n");
            content.push('\n');
        }
    }

    // Ensure exactly one trailing newline.
    let trimmed = content.trim_end_matches('\n');
    let mut content = trimmed.to_owned();
    content.push('\n');

    content
}

/// Typed property value for YAML emission in synthesised files.
enum PropValue {
    Str(String),
    Bool(bool),
    Int(i64),
    EmptyList,
}

/// Produce a YAML scalar string value. Quotes the string if needed.
fn yaml_scalar(s: &str) -> String {
    // Strings that need quoting: empty, contain leading/trailing whitespace,
    // look like YAML keywords, or contain characters that change YAML meaning.
    let needs_quoting = s.is_empty()
        || s.trim() != s
        || matches!(s, "true" | "false" | "yes" | "no" | "null" | "~")
        || s.starts_with(['#', '&', '*', '?', '|', '-', '<', '>', '!', '%', '@', '`'])
        || s.starts_with('"')
        || s.starts_with('\'')
        || s.contains(": ")
        || s.contains(" #")
        || s.contains('\n')
        || s.parse::<f64>().is_ok(); // looks like a number

    if needs_quoting {
        // Simple double-quote with minimal escaping.
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_owned()
    }
}
