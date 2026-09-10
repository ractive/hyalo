use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::cli::args::{FindFilters, ViewsAction};
use crate::output::{CommandOutcome, Format, user_diagnostic};

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
pub(crate) fn list_views(dir: &Path, _format: Format) -> Result<CommandOutcome> {
    let views = load_views(dir);
    let mut items: Vec<ViewResult> = Vec::new();
    let mut sorted_keys: Vec<&String> = views.keys().collect();
    sorted_keys.sort();
    for name in sorted_keys {
        let filters = &views[name];
        items.push(ViewResult { name, filters });
    }
    let total = items.len() as u64;
    let output = serde_json::to_value(&items).context("failed to serialize views list")?;
    Ok(CommandOutcome::success_with_total(output, total))
}

/// Save a view to `.hyalo.toml` within `dir`.
pub(crate) fn set_view(
    dir: &Path,
    name: &str,
    filters: &FindFilters,
    format: Format,
) -> Result<CommandOutcome> {
    // Validate name: alphanumeric, hyphens, and underscores only (TOML bare-key safe)
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Ok(CommandOutcome::UserError(user_diagnostic(
            format,
            &format!(
                "invalid view name '{name}': must be non-empty and contain only alphanumeric characters, hyphens, or underscores"
            ),
            None,
            None,
            None,
        )));
    }

    // Check that at least one filter is set
    let filters_value =
        toml::Value::try_from(filters).context("failed to serialize filters to TOML")?;
    let default_value = toml::Value::try_from(FindFilters::default())
        .context("failed to serialize default filters")?;
    if filters_value == default_value {
        return Ok(CommandOutcome::UserError(user_diagnostic(
            format,
            "no filters specified — a view must contain at least one filter",
            None,
            None,
            None,
        )));
    }

    let toml_path = resolve_toml_path(dir);
    let mut doc = super::config_write::capture(&toml_path)?;

    // Get or create the [views] table
    if !doc.contains_key("views") {
        doc["views"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let Some(views_item) = doc.get_mut("views") else {
        unreachable!()
    };
    let Some(views_table) = views_item.as_table_mut() else {
        return Ok(CommandOutcome::UserError(user_diagnostic(
            format,
            "'views' in .hyalo.toml is not a table — check your config file",
            None,
            None,
            None,
        )));
    };

    // Convert filters to a toml_edit table via text round-trip
    let edit_item = toml_value_to_edit_item(&filters_value)?;
    views_table.insert(name, edit_item);

    let effects = super::config_write::publish(&toml_path, &mut doc)?;

    let output = serde_json::to_value(&ViewMutationResult {
        action: "set",
        name,
    })
    .context("failed to serialize result")?;
    Ok(CommandOutcome::success(output).with_apply_report(effects))
}

/// Remove a view from `.hyalo.toml` within `dir`.
pub(crate) fn remove_view(dir: &Path, name: &str, format: Format) -> Result<CommandOutcome> {
    let toml_path = resolve_toml_path(dir);
    let mut doc = super::config_write::capture(&toml_path)?;

    let Some(views_table) = doc.get_mut("views").and_then(|v| v.as_table_mut()) else {
        return Ok(CommandOutcome::UserError(user_diagnostic(
            format,
            &format!("view '{name}' not found"),
            None,
            Some("run 'hyalo views list' to see available views"),
            None,
        )));
    };

    if views_table.remove(name).is_none() {
        return Ok(CommandOutcome::UserError(user_diagnostic(
            format,
            &format!("view '{name}' not found"),
            None,
            Some("run 'hyalo views list' to see available views"),
            None,
        )));
    }

    // Clean up: remove empty views table
    if views_table.is_empty() {
        doc.remove("views");
    }

    let effects = super::config_write::publish(&toml_path, &mut doc)?;

    let output = serde_json::to_value(&ViewMutationResult {
        action: "removed",
        name,
    })
    .context("failed to serialize result")?;
    Ok(CommandOutcome::success(output).with_apply_report(effects))
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
#[allow(clippy::items_after_test_module)] // dispatch handler appended below (ARCH-1, iter-225)
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

        let outcome = set_view(dir, "my-view", &filters, Format::Json).unwrap();
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

        set_view(dir, "iter-view", &filters, Format::Json).unwrap();

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

        set_view(dir, "done-view", &filters, Format::Json).unwrap();
        let outcome = remove_view(dir, "done-view", Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::Success { .. }));

        let views = load_views(dir);
        assert!(
            !views.contains_key("done-view"),
            "view should be gone after remove"
        );
    }

    #[test]
    fn load_views_supports_orphan_and_dead_end_flags() {
        // Regression: views must be able to filter by `orphan` / `dead_end`
        // the same way CLI flags do.
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        std::fs::write(
            dir.join(".hyalo.toml"),
            "[views.orphans]\norphan = true\n\n[views.dead-ends]\ndead_end = true\n",
        )
        .unwrap();

        let views = load_views(dir);
        let orphan_view = views.get("orphans").expect("orphans view missing");
        assert!(orphan_view.orphan, "view should have orphan = true");
        assert!(
            !orphan_view.dead_end,
            "view should not have dead_end = true"
        );

        let dead_view = views.get("dead-ends").expect("dead-ends view missing");
        assert!(dead_view.dead_end, "view should have dead_end = true");
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
        set_view(dir, "iter", &filters, Format::Json).unwrap();

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

// ---------------------------------------------------------------------------
// Dispatch handler (ARCH-1, iter-225)
// ---------------------------------------------------------------------------

/// The `hyalo views` dispatch arm, extracted verbatim from `dispatch.rs`.
#[allow(clippy::items_after_statements)] // extracted handler keeps its mid-fn imports (ARCH-1, iter-225)
pub(crate) fn run(
    ctx: &mut crate::dispatch::CommandContext<'_>,
    action: Option<ViewsAction>,
) -> Result<CommandOutcome> {
    let effective_format = ctx.effective_format;

    {
        let action = action.unwrap_or(ViewsAction::List);
        match action {
            ViewsAction::List => {
                crate::commands::views::list_views(ctx.config_dir, effective_format)
            }
            ViewsAction::Set {
                name,
                pattern,
                mut filters,
            } => {
                if pattern.is_some() && filters.regexp.is_some() {
                    return Ok(CommandOutcome::UserError(crate::output::user_diagnostic(
                        effective_format,
                        "PATTERN and --regexp are mutually exclusive",
                        None,
                        None,
                        None,
                    )));
                }
                filters.pattern = pattern;
                crate::commands::views::set_view(ctx.config_dir, &name, &filters, effective_format)
            }
            ViewsAction::Remove { name } => {
                crate::commands::views::remove_view(ctx.config_dir, &name, effective_format)
            }
            ViewsAction::Run { .. } => anyhow::bail!("view queries require prepared dispatch"),
        }
    }
}

/// Serialized ViewResult command contract.
#[derive(serde::Serialize)]
struct ViewResult<'a> {
    /// Saved view name.
    name: &'a str,
    /// Typed saved find filters.
    filters: &'a FindFilters,
}

/// Serialized ViewMutationResult command contract.
#[derive(serde::Serialize)]
struct ViewMutationResult<'a> {
    /// Configuration mutation action.
    action: &'a str,
    /// Saved view name.
    name: &'a str,
}
