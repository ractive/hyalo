use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::cli::args::FindFilters;
use crate::output::CommandOutcome;

const TOML_FILENAME: &str = ".hyalo.toml";

/// Returns the path to `.hyalo.toml` within the given directory.
fn resolve_toml_path(dir: &Path) -> PathBuf {
    dir.join(TOML_FILENAME)
}

/// Load all views from `.hyalo.toml` within `dir`.
/// Returns an empty map if the file doesn't exist or has no views.
pub(crate) fn load_views(dir: &Path) -> HashMap<String, FindFilters> {
    let toml_path = resolve_toml_path(dir);
    let contents = match fs::read_to_string(&toml_path) {
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
pub(crate) fn list_views(dir: &Path) -> Result<CommandOutcome> {
    let views = load_views(dir);
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

/// Save a view to `.hyalo.toml` within `dir`.
pub(crate) fn set_view(dir: &Path, name: &str, filters: &FindFilters) -> Result<CommandOutcome> {
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

    let toml_path = resolve_toml_path(dir);
    let mut doc = read_toml_doc(&toml_path)?;

    // Get or create the [views] table
    if !doc.contains_key("views") {
        doc["views"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let Some(views_item) = doc.get_mut("views") else {
        unreachable!()
    };
    let Some(views_table) = views_item.as_table_mut() else {
        return Ok(CommandOutcome::UserError(
            "Error: 'views' in .hyalo.toml is not a table — check your config file".to_owned(),
        ));
    };

    // Convert filters to a toml_edit table via text round-trip
    let edit_item = toml_value_to_edit_item(&filters_value)?;
    views_table.insert(name, edit_item);

    write_toml_doc(&toml_path, &doc)?;

    let output = serde_json::to_string_pretty(&serde_json::json!({
        "action": "set",
        "name": name,
    }))
    .context("failed to serialize result")?;
    Ok(CommandOutcome::success(output))
}

/// Remove a view from `.hyalo.toml` within `dir`.
pub(crate) fn remove_view(dir: &Path, name: &str) -> Result<CommandOutcome> {
    let toml_path = resolve_toml_path(dir);
    let mut doc = read_toml_doc(&toml_path)?;

    let Some(views_table) = doc.get_mut("views").and_then(|v| v.as_table_mut()) else {
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
        doc.remove("views");
    }

    write_toml_doc(&toml_path, &doc)?;

    let output = serde_json::to_string_pretty(&serde_json::json!({
        "action": "removed",
        "name": name,
    }))
    .context("failed to serialize result")?;
    Ok(CommandOutcome::success(output))
}

/// Read `.hyalo.toml` as a `DocumentMut`, or return an empty doc if not found.
fn read_toml_doc(toml_path: &Path) -> Result<toml_edit::DocumentMut> {
    match fs::read_to_string(toml_path) {
        Ok(contents) => contents
            .parse::<toml_edit::DocumentMut>()
            .context("failed to parse .hyalo.toml"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(toml_edit::DocumentMut::new()),
        Err(e) => Err(e).context("failed to read .hyalo.toml"),
    }
}

/// Write a `DocumentMut` back to `.hyalo.toml`, preserving formatting.
fn write_toml_doc(toml_path: &Path, doc: &toml_edit::DocumentMut) -> Result<()> {
    fs::write(toml_path, doc.to_string()).context("failed to write .hyalo.toml")
}

/// Convert a `toml::Value` (table) to a `toml_edit::Item` via text round-trip.
fn toml_value_to_edit_item(value: &toml::Value) -> Result<toml_edit::Item> {
    let text = toml::to_string(value).context("failed to serialize TOML value")?;
    let doc: toml_edit::DocumentMut = text
        .parse()
        .context("failed to re-parse serialized TOML value")?;
    // The round-tripped doc contains only the keys from this value; wrap in a table item.
    Ok(toml_edit::Item::Table(doc.into_table()))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tag_filters(tag: &str) -> FindFilters {
        FindFilters {
            tag: vec![tag.to_owned()],
            ..Default::default()
        }
    }

    #[test]
    fn set_view_writes_to_custom_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        let filters = make_tag_filters("iteration");

        let outcome = set_view(dir, "my-view", &filters).unwrap();
        assert!(matches!(outcome, CommandOutcome::Success { .. }));

        // Config must be written inside the temp dir, not CWD.
        let toml_path = dir.join(".hyalo.toml");
        assert!(toml_path.exists(), ".hyalo.toml not found in custom dir");
        let contents = std::fs::read_to_string(&toml_path).unwrap();
        assert!(
            contents.contains("my-view"),
            "view 'my-view' not found in written TOML"
        );
    }

    #[test]
    fn load_views_reads_from_custom_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        let filters = make_tag_filters("iteration");

        set_view(dir, "iter-view", &filters).unwrap();

        let views = load_views(dir);
        assert!(
            views.contains_key("iter-view"),
            "expected view not found after load"
        );
    }

    #[test]
    fn remove_view_reads_from_custom_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        let filters = make_tag_filters("done");

        set_view(dir, "done-view", &filters).unwrap();
        let outcome = remove_view(dir, "done-view").unwrap();
        assert!(matches!(outcome, CommandOutcome::Success { .. }));

        let views = load_views(dir);
        assert!(
            !views.contains_key("done-view"),
            "view should be gone after remove"
        );
    }

    #[test]
    fn set_view_preserves_existing_sections_and_order() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        // Write a .hyalo.toml with specific section ordering and a comment
        let original = "# Main config\ndir = \"notes\"\nformat = \"text\"\n\n\
                         [search]\nlanguage = \"english\"\n\n\
                         [schema.types.iteration]\nrequired = [\"title\", \"date\"]\n";
        std::fs::write(dir.join(".hyalo.toml"), original).unwrap();

        let filters = make_tag_filters("iteration");
        set_view(dir, "iter", &filters).unwrap();

        let result = std::fs::read_to_string(dir.join(".hyalo.toml")).unwrap();
        // Existing sections should still appear in order
        let dir_pos = result.find("dir =").unwrap();
        let search_pos = result.find("[search]").unwrap();
        let schema_pos = result.find("[schema").unwrap();
        assert!(
            dir_pos < search_pos && search_pos < schema_pos,
            "existing section order should be preserved"
        );
        // The comment should be preserved
        assert!(result.contains("# Main config"));
        // The view should be present
        assert!(result.contains("[views.iter]"));
    }
}
