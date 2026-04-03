use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result};

use crate::cli::args::FindFilters;
use crate::output::CommandOutcome;

const TOML_PATH: &str = ".hyalo.toml";

/// Load all views from `.hyalo.toml`.
/// Returns an empty map if the file doesn't exist or has no views.
pub(crate) fn load_views() -> HashMap<String, FindFilters> {
    let contents = match fs::read_to_string(TOML_PATH) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return HashMap::new(),
        Err(e) => {
            crate::warn::warn(format!("could not read .hyalo.toml for views: {e}"));
            return HashMap::new();
        }
    };
    let table: toml::Table = match toml::from_str(&contents) {
        Ok(t) => t,
        Err(e) => {
            crate::warn::warn(format!("malformed .hyalo.toml: {e}"));
            return HashMap::new();
        }
    };
    let Some(toml::Value::Table(views_table)) = table.get("views") else {
        return HashMap::new();
    };
    let mut views = HashMap::new();
    for (name, value) in views_table {
        match value.clone().try_into::<FindFilters>() {
            Ok(filters) => {
                views.insert(name.clone(), filters);
            }
            Err(e) => {
                crate::warn::warn(format!("skipping malformed view '{name}': {e}"));
            }
        }
    }
    views
}

/// List all saved views.
pub(crate) fn list_views() -> Result<CommandOutcome> {
    let views = load_views();
    let mut items: Vec<serde_json::Value> = Vec::new();
    let mut sorted_keys: Vec<&String> = views.keys().collect();
    sorted_keys.sort();
    for name in sorted_keys {
        let filters = &views[name];
        let filters_json =
            serde_json::to_value(filters).context("failed to serialize view filters")?;
        items.push(serde_json::json!({
            "name": name,
            "filters": filters_json,
        }));
    }
    let total = items.len() as u64;
    let output = serde_json::to_string_pretty(&items).context("failed to serialize views list")?;
    Ok(CommandOutcome::success_with_total(output, total))
}

/// Save a view to `.hyalo.toml`.
pub(crate) fn set_view(name: &str, filters: &FindFilters) -> Result<CommandOutcome> {
    // Validate name: alphanumeric, hyphens, and underscores only (TOML bare-key safe)
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Ok(CommandOutcome::UserError(format!(
            "Error: invalid view name '{name}': must be non-empty and contain only \
             alphanumeric characters, hyphens, or underscores"
        )));
    }

    // Check that at least one filter is set
    let filters_value =
        toml::Value::try_from(filters).context("failed to serialize filters to TOML")?;
    let default_value = toml::Value::try_from(FindFilters::default())
        .context("failed to serialize default filters")?;
    if filters_value == default_value {
        return Ok(CommandOutcome::UserError(
            "Error: no filters specified — a view must contain at least one filter".to_owned(),
        ));
    }

    let mut table = read_toml_table()?;

    // Get or create the views table
    let views = table
        .entry("views")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));

    let toml::Value::Table(views_table) = views else {
        return Ok(CommandOutcome::UserError(
            "Error: 'views' in .hyalo.toml is not a table — check your config file".to_owned(),
        ));
    };

    views_table.insert(name.to_owned(), filters_value);

    write_toml_table(&table)?;

    let output = serde_json::to_string_pretty(&serde_json::json!({
        "action": "set",
        "name": name,
    }))
    .context("failed to serialize result")?;
    Ok(CommandOutcome::success(output))
}

/// Remove a view from `.hyalo.toml`.
pub(crate) fn remove_view(name: &str) -> Result<CommandOutcome> {
    let mut table = read_toml_table()?;

    let Some(toml::Value::Table(views_table)) = table.get_mut("views") else {
        return Ok(CommandOutcome::UserError(format!(
            "Error: view '{name}' not found\n\n  tip: run 'hyalo views list' to see available views"
        )));
    };

    if views_table.remove(name).is_none() {
        return Ok(CommandOutcome::UserError(format!(
            "Error: view '{name}' not found\n\n  tip: run 'hyalo views list' to see available views"
        )));
    }

    // Clean up: remove empty views table
    if views_table.is_empty() {
        table.remove("views");
    }

    write_toml_table(&table)?;

    let output = serde_json::to_string_pretty(&serde_json::json!({
        "action": "removed",
        "name": name,
    }))
    .context("failed to serialize result")?;
    Ok(CommandOutcome::success(output))
}

/// Read `.hyalo.toml` as a TOML table, creating an empty table if the file doesn't exist.
fn read_toml_table() -> Result<toml::Table> {
    match fs::read_to_string(TOML_PATH) {
        Ok(contents) => toml::from_str(&contents).context("failed to parse .hyalo.toml"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(toml::Table::new()),
        Err(e) => Err(e).context("failed to read .hyalo.toml"),
    }
}

/// Write a TOML table back to `.hyalo.toml`.
fn write_toml_table(table: &toml::Table) -> Result<()> {
    let content = toml::to_string(table).context("failed to serialize .hyalo.toml")?;
    fs::write(TOML_PATH, content).context("failed to write .hyalo.toml")?;
    Ok(())
}
