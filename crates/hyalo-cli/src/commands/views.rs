use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result, bail};

use crate::cli::args::FindFilters;
use crate::output::CommandOutcome;

const TOML_PATH: &str = ".hyalo.toml";

/// Load all views from `.hyalo.toml`.
/// Returns an empty map if the file doesn't exist or has no views.
pub(crate) fn load_views() -> HashMap<String, FindFilters> {
    let Ok(contents) = fs::read_to_string(TOML_PATH) else {
        return HashMap::new();
    };
    let Ok(table) = toml::from_str::<toml::Table>(&contents) else {
        return HashMap::new();
    };
    let Some(toml::Value::Table(views_table)) = table.get("views") else {
        return HashMap::new();
    };
    let mut views = HashMap::new();
    for (name, value) in views_table {
        if let Ok(filters) = value.clone().try_into::<FindFilters>() {
            views.insert(name.clone(), filters);
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
    let total = items.len();
    let output = serde_json::to_string_pretty(&serde_json::json!({
        "results": items,
        "total": total,
    }))
    .context("failed to serialize views list")?;
    Ok(CommandOutcome::Success {
        output,
        total: Some(total as u64),
    })
}

/// Save a view to `.hyalo.toml`.
pub(crate) fn set_view(name: &str, filters: &FindFilters) -> Result<CommandOutcome> {
    // Validate name: must be non-empty, no whitespace, no dots (TOML key safety)
    if name.is_empty() || name.contains(char::is_whitespace) || name.contains('.') {
        return Ok(CommandOutcome::UserError(format!(
            "Error: invalid view name '{name}': must be non-empty, with no whitespace or dots"
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
        bail!("'views' in .hyalo.toml is not a table");
    };

    views_table.insert(name.to_owned(), filters_value);

    write_toml_table(&table)?;

    let output = serde_json::to_string_pretty(&serde_json::json!({
        "results": {
            "action": "set",
            "name": name,
        }
    }))
    .context("failed to serialize result")?;
    Ok(CommandOutcome::Success {
        output,
        total: None,
    })
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
        "results": {
            "action": "removed",
            "name": name,
        }
    }))
    .context("failed to serialize result")?;
    Ok(CommandOutcome::Success {
        output,
        total: None,
    })
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
