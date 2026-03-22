//! Generates drill-down command hints for CLI output.
//!
//! When `--hints` is enabled, each command's output includes suggested next
//! commands. All hints are concrete, executable strings — no templates or
//! placeholders.

/// Maximum number of hints to return from any generator.
const MAX_HINTS: usize = 5;

/// Identifies which command produced the output.
pub enum HintSource {
    Summary,
    PropertiesSummary,
    PropertiesList,
    TagsSummary,
    TagsList,
    PropertyFind {
        name: String,
        value: Option<String>,
    },
    TagFind {
        name: String,
    },
    /// Filter is "all", "done", "todo", or the status char.
    Tasks {
        filter: String,
    },
    Outline,
    Links {
        file: String,
    },
}

/// Global flags to propagate into generated hint commands.
pub struct HintContext {
    pub source: HintSource,
    /// `None` means "." (default) — omit from hints to keep them short.
    pub dir: Option<String>,
    pub glob: Option<String>,
}

/// Generate concrete drill-down hints from a command's JSON output.
///
/// Returns at most [`MAX_HINTS`] executable `hyalo` command strings.
#[must_use]
pub fn generate_hints(ctx: &HintContext, data: &serde_json::Value) -> Vec<String> {
    let hints = match &ctx.source {
        HintSource::Summary => hints_for_summary(ctx, data),
        HintSource::PropertiesSummary => hints_for_properties_summary(ctx, data),
        HintSource::PropertiesList => hints_for_properties_list(ctx, data),
        HintSource::TagsSummary => hints_for_tags_summary(ctx, data),
        HintSource::TagsList => hints_for_tags_list(ctx, data),
        HintSource::PropertyFind { name, value } => {
            hints_for_property_find(ctx, data, name, value.as_deref())
        }
        HintSource::TagFind { name } => hints_for_tag_find(ctx, data, name),
        HintSource::Tasks { filter } => hints_for_tasks(ctx, data, filter),
        HintSource::Outline => hints_for_outline(ctx, data),
        HintSource::Links { file } => hints_for_links(ctx, data, file),
    };
    hints.into_iter().take(MAX_HINTS).collect()
}

// ---------------------------------------------------------------------------
// Command builder
// ---------------------------------------------------------------------------

/// Build a command that intentionally omits `--glob` (for file-specific hints).
fn build_command_no_glob(ctx: &HintContext, args: &[&str]) -> String {
    // We need a stripped context — easiest is to inline the logic without glob.
    let mut parts: Vec<String> = vec!["hyalo".to_owned()];
    if let Some(dir) = &ctx.dir {
        parts.push("--dir".to_owned());
        parts.push(shell_quote(dir));
    }
    for arg in args {
        parts.push(shell_quote(arg));
    }
    parts.join(" ")
}

/// Build a command that propagates `--glob` when present.
fn build_command_with_glob(ctx: &HintContext, args: &[&str]) -> String {
    let mut parts: Vec<String> = vec!["hyalo".to_owned()];

    if let Some(dir) = &ctx.dir {
        parts.push("--dir".to_owned());
        parts.push(shell_quote(dir));
    }
    if let Some(glob) = &ctx.glob {
        parts.push("--glob".to_owned());
        parts.push(shell_quote(glob));
    }

    for arg in args {
        parts.push(shell_quote(arg));
    }

    parts.join(" ")
}

/// Wrap a string in single-quotes if it contains any shell-special characters.
///
/// Uses an allowlist of safe characters — anything not in the list triggers quoting.
/// Single-quoting avoids variable expansion and is safer than double-quoting.
fn shell_quote(s: &str) -> String {
    if s.is_empty()
        || s.chars().any(|c| {
            !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '/' | ':' | '@' | '=' | ',' | '+')
        })
    {
        // In single-quoted strings, the only character that needs escaping is '
        // which is done by ending the quote, adding an escaped quote, and reopening.
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_owned()
    }
}

// ---------------------------------------------------------------------------
// Status priority helpers
// ---------------------------------------------------------------------------

/// Priority rank for a status value: lower = more interesting.
fn status_priority(value: &str) -> u8 {
    let lower = value.to_lowercase();
    if lower == "in-progress" || lower == "in progress" || lower == "active" {
        0
    } else if lower == "planned" || lower == "todo" {
        1
    } else if lower == "draft" || lower == "idea" {
        2
    } else if lower == "completed" || lower == "done" || lower == "archived" {
        4
    } else {
        3
    }
}

// ---------------------------------------------------------------------------
// Per-source hint generators
// ---------------------------------------------------------------------------

fn hints_for_summary(ctx: &HintContext, data: &serde_json::Value) -> Vec<String> {
    let mut hints = Vec::new();

    // Always suggest aggregate views.
    hints.push(build_command_with_glob(ctx, &["properties", "summary"]));
    hints.push(build_command_with_glob(ctx, &["tags", "summary"]));

    // Suggest tasks --todo if there are open tasks.
    let tasks_total = data
        .get("tasks")
        .and_then(|t| t.get("total"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let tasks_done = data
        .get("tasks")
        .and_then(|t| t.get("done"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if tasks_total > tasks_done {
        hints.push(build_command_with_glob(ctx, &["tasks", "--todo"]));
    }

    // Pick 1-2 most interesting status values.
    if let Some(status_arr) = data.get("status").and_then(|s| s.as_array()) {
        let mut groups: Vec<(&str, u8)> = status_arr
            .iter()
            .filter_map(|g| {
                let value = g.get("value").and_then(|v| v.as_str())?;
                Some((value, status_priority(value)))
            })
            .collect();
        groups.sort_by_key(|&(_, p)| p);

        let remaining = MAX_HINTS.saturating_sub(hints.len());
        for (value, _) in groups.into_iter().take(remaining.min(2)) {
            hints.push(build_command_no_glob(
                ctx,
                &["property", "find", "--name", "status", "--value", value],
            ));
        }
    }

    hints
}

fn hints_for_properties_summary(ctx: &HintContext, data: &serde_json::Value) -> Vec<String> {
    let arr = match data.as_array() {
        Some(a) => a,
        None => return vec![],
    };

    // Sort by count descending, take top 3.
    let mut entries: Vec<(&str, u64)> = arr
        .iter()
        .filter_map(|e| {
            let name = e.get("name").and_then(|n| n.as_str())?;
            let count = e.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
            Some((name, count))
        })
        .collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    entries
        .into_iter()
        .take(3)
        .map(|(name, _)| build_command_with_glob(ctx, &["property", "find", "--name", name]))
        .collect()
}

fn hints_for_properties_list(ctx: &HintContext, data: &serde_json::Value) -> Vec<String> {
    let paths: Vec<&str> = match data {
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|e| e.get("path").and_then(|p| p.as_str()))
            .collect(),
        serde_json::Value::Object(_) => {
            // Single file result.
            data.get("path")
                .and_then(|p| p.as_str())
                .into_iter()
                .collect()
        }
        _ => vec![],
    };

    paths
        .into_iter()
        .take(2)
        .map(|path| build_command_no_glob(ctx, &["outline", "--file", path]))
        .collect()
}

fn hints_for_tags_summary(ctx: &HintContext, data: &serde_json::Value) -> Vec<String> {
    let tags_arr = match data.get("tags").and_then(|t| t.as_array()) {
        Some(a) => a,
        None => return vec![],
    };

    // Sort by count descending, take top 3.
    let mut entries: Vec<(&str, u64)> = tags_arr
        .iter()
        .filter_map(|entry| {
            let name = entry.get("name").and_then(|n| n.as_str())?;
            let count = entry.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
            Some((name, count))
        })
        .collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    entries
        .into_iter()
        .take(3)
        .map(|(name, _)| build_command_with_glob(ctx, &["tag", "find", "--name", name]))
        .collect()
}

fn hints_for_tags_list(ctx: &HintContext, data: &serde_json::Value) -> Vec<String> {
    let paths: Vec<&str> = match data {
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|e| e.get("path").and_then(|p| p.as_str()))
            .collect(),
        serde_json::Value::Object(_) => data
            .get("path")
            .and_then(|p| p.as_str())
            .into_iter()
            .collect(),
        _ => vec![],
    };

    paths
        .into_iter()
        .take(2)
        .map(|path| build_command_no_glob(ctx, &["outline", "--file", path]))
        .collect()
}

fn hints_for_property_find(
    ctx: &HintContext,
    data: &serde_json::Value,
    name: &str,
    value: Option<&str>,
) -> Vec<String> {
    let files = match data.get("files").and_then(|f| f.as_array()) {
        Some(a) => a,
        None => return vec![],
    };

    let file_paths: Vec<&str> = files.iter().filter_map(|f| f.as_str()).collect();

    let mut hints = Vec::new();

    // Suggest outline for first 2-3 files.
    let outline_count = if value.is_none() { 2 } else { 3 };
    for path in file_paths.iter().take(outline_count) {
        hints.push(build_command_no_glob(ctx, &["outline", "--file", path]));
    }

    // If no --value was used, suggest reading the property from the first file.
    if value.is_none()
        && let Some(first_file) = file_paths.first()
    {
        hints.push(build_command_no_glob(
            ctx,
            &["property", "read", "--name", name, "--file", first_file],
        ));
    }

    hints
}

fn hints_for_tag_find(ctx: &HintContext, data: &serde_json::Value, _name: &str) -> Vec<String> {
    let files = match data.get("files").and_then(|f| f.as_array()) {
        Some(a) => a,
        None => return vec![],
    };

    files
        .iter()
        .filter_map(|f| f.as_str())
        .take(3)
        .map(|path| build_command_no_glob(ctx, &["outline", "--file", path]))
        .collect()
}

fn hints_for_tasks(ctx: &HintContext, data: &serde_json::Value, filter: &str) -> Vec<String> {
    let arr = match data.as_array() {
        Some(a) => a,
        None => return vec![],
    };

    let mut hints = Vec::new();

    // Find first file with non-empty tasks and suggest reading its first task.
    'outer: for entry in arr {
        let file = match entry.get("file").and_then(|f| f.as_str()) {
            Some(f) => f,
            None => continue,
        };
        let tasks = match entry.get("tasks").and_then(|t| t.as_array()) {
            Some(t) => t,
            None => continue,
        };
        for task in tasks {
            let line = match task.get("line").and_then(|l| l.as_u64()) {
                Some(l) => l,
                None => continue,
            };
            let line_str = line.to_string();
            hints.push(build_command_no_glob(
                ctx,
                &["task", "read", "--file", file, "--line", &line_str],
            ));
            break 'outer;
        }
    }

    // If showing all tasks, also suggest the todo view.
    if filter == "all" {
        hints.push(build_command_with_glob(ctx, &["tasks", "--todo"]));
    }

    hints
}

fn hints_for_outline(ctx: &HintContext, data: &serde_json::Value) -> Vec<String> {
    // Works for both a single FileOutline and an array of them.
    let outlines: Vec<&serde_json::Value> = match data {
        serde_json::Value::Array(arr) => arr.iter().collect(),
        obj @ serde_json::Value::Object(_) => vec![obj],
        _ => return vec![],
    };

    let mut hints = Vec::new();

    if let Some(first) = outlines.first() {
        // Suggest property find for first property in the first outline.
        if let Some(props) = first.get("properties").and_then(|p| p.as_array())
            && let Some(first_prop) = props.first()
            && let Some(name) = first_prop.get("name").and_then(|n| n.as_str())
        {
            hints.push(build_command_with_glob(
                ctx,
                &["property", "find", "--name", name],
            ));
        }

        // Suggest tag find for first tag in the first outline.
        if let Some(tags) = first.get("tags").and_then(|t| t.as_array())
            && let Some(first_tag) = tags.first()
            && let Some(tag) = first_tag.as_str()
        {
            hints.push(build_command_with_glob(
                ctx,
                &["tag", "find", "--name", tag],
            ));
        }
    }

    hints
}

fn hints_for_links(ctx: &HintContext, data: &serde_json::Value, _file: &str) -> Vec<String> {
    let links = match data.get("links").and_then(|l| l.as_array()) {
        Some(a) => a,
        None => return vec![],
    };

    // Only suggest outlines for resolved links (those with a `path` field).
    links
        .iter()
        .filter_map(|link| link.get("path").and_then(|p| p.as_str()))
        .take(2)
        .map(|path| build_command_no_glob(ctx, &["outline", "--file", path]))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx(source: HintSource) -> HintContext {
        HintContext {
            source,
            dir: None,
            glob: None,
        }
    }

    fn ctx_with_dir(source: HintSource, dir: &str) -> HintContext {
        HintContext {
            source,
            dir: Some(dir.to_owned()),
            glob: None,
        }
    }

    fn ctx_with_glob(source: HintSource, glob: &str) -> HintContext {
        HintContext {
            source,
            dir: None,
            glob: Some(glob.to_owned()),
        }
    }

    // --- shell_quote ---

    #[test]
    fn shell_quote_plain_string() {
        assert_eq!(shell_quote("status"), "status");
    }

    #[test]
    fn shell_quote_string_with_space() {
        assert_eq!(shell_quote("in progress"), "'in progress'");
    }

    #[test]
    fn shell_quote_string_with_special_chars() {
        assert_eq!(shell_quote("foo$bar"), "'foo$bar'");
    }

    #[test]
    fn shell_quote_string_with_single_quote() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn shell_quote_glob_chars() {
        assert_eq!(shell_quote("**/*.md"), "'**/*.md'");
    }

    #[test]
    fn shell_quote_empty_string() {
        assert_eq!(shell_quote(""), "''");
    }

    // --- build_command ---

    #[test]
    fn build_command_no_flags() {
        let c = ctx(HintSource::Summary);
        assert_eq!(
            build_command_no_glob(&c, &["properties", "summary"]),
            "hyalo properties summary"
        );
    }

    #[test]
    fn build_command_with_dir() {
        let c = ctx_with_dir(HintSource::Summary, "/my/vault");
        assert_eq!(
            build_command_no_glob(&c, &["tags", "summary"]),
            "hyalo --dir /my/vault tags summary"
        );
    }

    #[test]
    fn build_command_with_glob_propagated() {
        let c = ctx_with_glob(HintSource::PropertiesSummary, "**/*.md");
        assert_eq!(
            build_command_with_glob(&c, &["properties", "summary"]),
            "hyalo --glob '**/*.md' properties summary"
        );
    }

    #[test]
    fn build_command_no_glob_omits_glob() {
        let c = ctx_with_glob(HintSource::PropertiesList, "notes/*.md");
        assert_eq!(
            build_command_no_glob(&c, &["outline", "--file", "note.md"]),
            "hyalo outline --file note.md"
        );
    }

    // --- status_priority ---

    #[test]
    fn status_priority_ordering() {
        assert!(status_priority("in-progress") < status_priority("planned"));
        assert!(status_priority("planned") < status_priority("draft"));
        assert!(status_priority("draft") < status_priority("custom"));
        assert!(status_priority("custom") < status_priority("completed"));
    }

    // --- hints_for_summary ---

    #[test]
    fn summary_always_includes_properties_and_tags() {
        let c = ctx(HintSource::Summary);
        let data = json!({
            "files": {"total": 10, "by_directory": []},
            "properties": [],
            "tags": {"tags": [], "total": 0},
            "status": [],
            "tasks": {"total": 0, "done": 0},
            "recent_files": []
        });
        let hints = generate_hints(&c, &data);
        assert!(hints.iter().any(|h| h.contains("properties summary")));
        assert!(hints.iter().any(|h| h.contains("tags summary")));
    }

    #[test]
    fn summary_suggests_tasks_todo_when_open_tasks() {
        let c = ctx(HintSource::Summary);
        let data = json!({
            "files": {"total": 5, "by_directory": []},
            "properties": [],
            "tags": {"tags": [], "total": 0},
            "status": [],
            "tasks": {"total": 10, "done": 3},
            "recent_files": []
        });
        let hints = generate_hints(&c, &data);
        assert!(
            hints
                .iter()
                .any(|h| h.contains("tasks") && h.contains("--todo"))
        );
    }

    #[test]
    fn summary_omits_tasks_todo_when_all_done() {
        let c = ctx(HintSource::Summary);
        let data = json!({
            "files": {"total": 5, "by_directory": []},
            "properties": [],
            "tags": {"tags": [], "total": 0},
            "status": [],
            "tasks": {"total": 10, "done": 10},
            "recent_files": []
        });
        let hints = generate_hints(&c, &data);
        assert!(!hints.iter().any(|h| h.contains("--todo")));
    }

    #[test]
    fn summary_picks_interesting_status_values() {
        let c = ctx(HintSource::Summary);
        let data = json!({
            "files": {"total": 5, "by_directory": []},
            "properties": [],
            "tags": {"tags": [], "total": 0},
            "status": [
                {"value": "completed", "files": ["a.md"]},
                {"value": "in-progress", "files": ["b.md"]},
                {"value": "planned", "files": ["c.md"]}
            ],
            "tasks": {"total": 0, "done": 0},
            "recent_files": []
        });
        let hints = generate_hints(&c, &data);
        // in-progress should appear before completed
        let in_progress_pos = hints.iter().position(|h| h.contains("in-progress"));
        let completed_pos = hints.iter().position(|h| h.contains("completed"));
        assert!(in_progress_pos.is_some(), "should suggest in-progress");
        // completed may appear (only if limit not reached) or not — but in-progress must come first
        if let Some(cp) = completed_pos {
            assert!(in_progress_pos.unwrap() < cp);
        }
    }

    #[test]
    fn summary_max_hints_not_exceeded() {
        let c = ctx(HintSource::Summary);
        let data = json!({
            "files": {"total": 5, "by_directory": []},
            "properties": [],
            "tags": {"tags": [], "total": 0},
            "status": [
                {"value": "in-progress", "files": ["a.md"]},
                {"value": "planned", "files": ["b.md"]},
                {"value": "draft", "files": ["c.md"]},
                {"value": "idea", "files": ["d.md"]}
            ],
            "tasks": {"total": 5, "done": 1},
            "recent_files": []
        });
        let hints = generate_hints(&c, &data);
        assert!(hints.len() <= MAX_HINTS);
    }

    // --- hints_for_properties_summary ---

    #[test]
    fn properties_summary_top3_by_count() {
        let c = ctx(HintSource::PropertiesSummary);
        let data = json!([
            {"name": "title", "type": "text", "count": 100},
            {"name": "status", "type": "text", "count": 50},
            {"name": "tags", "type": "list", "count": 30},
            {"name": "author", "type": "text", "count": 5}
        ]);
        let hints = generate_hints(&c, &data);
        assert_eq!(hints.len(), 3);
        assert!(hints[0].contains("title"));
        assert!(hints[1].contains("status"));
        assert!(hints[2].contains("tags"));
        // author should not appear (rank 4)
        assert!(!hints.iter().any(|h| h.contains("author")));
    }

    #[test]
    fn properties_summary_empty_data() {
        let c = ctx(HintSource::PropertiesSummary);
        let hints = generate_hints(&c, &json!([]));
        assert!(hints.is_empty());
    }

    #[test]
    fn properties_summary_propagates_glob() {
        let c = ctx_with_glob(HintSource::PropertiesSummary, "notes/*.md");
        let data = json!([{"name": "status", "type": "text", "count": 5}]);
        let hints = generate_hints(&c, &data);
        assert!(hints[0].contains("--glob"));
        assert!(hints[0].contains("notes/*.md"));
    }

    // --- hints_for_properties_list ---

    #[test]
    fn properties_list_suggests_outline_for_first_2_files() {
        let c = ctx(HintSource::PropertiesList);
        let data = json!([
            {"path": "a.md", "properties": []},
            {"path": "b.md", "properties": []},
            {"path": "c.md", "properties": []}
        ]);
        let hints = generate_hints(&c, &data);
        assert_eq!(hints.len(), 2);
        assert!(hints[0].contains("outline") && hints[0].contains("a.md"));
        assert!(hints[1].contains("outline") && hints[1].contains("b.md"));
        // c.md should not appear
        assert!(!hints.iter().any(|h| h.contains("c.md")));
    }

    #[test]
    fn properties_list_single_file_object() {
        let c = ctx(HintSource::PropertiesList);
        let data = json!({"path": "note.md", "properties": []});
        let hints = generate_hints(&c, &data);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].contains("outline") && hints[0].contains("note.md"));
    }

    #[test]
    fn properties_list_glob_not_in_file_specific_hints() {
        let c = ctx_with_glob(HintSource::PropertiesList, "**/*.md");
        let data = json!([{"path": "note.md", "properties": []}]);
        let hints = generate_hints(&c, &data);
        assert!(!hints[0].contains("--glob"));
    }

    // --- hints_for_tags_summary ---

    #[test]
    fn tags_summary_top3_by_count() {
        let c = ctx(HintSource::TagsSummary);
        let data = json!({
            "tags": [
                {"name": "rust", "count": 20},
                {"name": "cli", "count": 10},
                {"name": "api", "count": 5},
                {"name": "draft", "count": 2}
            ],
            "total": 4
        });
        let hints = generate_hints(&c, &data);
        assert_eq!(hints.len(), 3);
        assert!(hints[0].contains("rust"));
        assert!(hints[1].contains("cli"));
        assert!(hints[2].contains("api"));
        assert!(!hints.iter().any(|h| h.contains("draft")));
    }

    #[test]
    fn tags_summary_empty_tags() {
        let c = ctx(HintSource::TagsSummary);
        let data = json!({"tags": [], "total": 0});
        let hints = generate_hints(&c, &data);
        assert!(hints.is_empty());
    }

    // --- hints_for_tags_list ---

    #[test]
    fn tags_list_suggests_outline_for_first_2_files() {
        let c = ctx(HintSource::TagsList);
        let data = json!([
            {"path": "a.md", "tags": ["rust"]},
            {"path": "b.md", "tags": ["cli"]},
            {"path": "c.md", "tags": ["api"]}
        ]);
        let hints = generate_hints(&c, &data);
        assert_eq!(hints.len(), 2);
        assert!(hints[0].contains("a.md"));
        assert!(hints[1].contains("b.md"));
    }

    // --- hints_for_property_find ---

    #[test]
    fn property_find_with_value_suggests_outlines() {
        let c = ctx(HintSource::PropertyFind {
            name: "status".to_owned(),
            value: Some("draft".to_owned()),
        });
        let data = json!({
            "property": "status",
            "value": "draft",
            "files": ["a.md", "b.md", "c.md"],
            "total": 3
        });
        let hints = generate_hints(&c, &data);
        // With a value, we get up to 3 outlines, no property read
        assert!(hints.iter().all(|h| h.contains("outline")));
        assert_eq!(hints.len(), 3);
        assert!(!hints.iter().any(|h| h.contains("property read")));
    }

    #[test]
    fn property_find_without_value_suggests_outline_and_read() {
        let c = ctx(HintSource::PropertyFind {
            name: "status".to_owned(),
            value: None,
        });
        let data = json!({
            "property": "status",
            "files": ["a.md", "b.md"],
            "total": 2
        });
        let hints = generate_hints(&c, &data);
        // 2 outlines + 1 property read
        assert!(hints.iter().any(|h| h.contains("outline")));
        assert!(
            hints
                .iter()
                .any(|h| h.contains("property read") && h.contains("a.md"))
        );
    }

    #[test]
    fn property_find_empty_files() {
        let c = ctx(HintSource::PropertyFind {
            name: "status".to_owned(),
            value: None,
        });
        let data = json!({"property": "status", "files": [], "total": 0});
        let hints = generate_hints(&c, &data);
        // No outlines possible, no read possible
        assert!(hints.is_empty());
    }

    // --- hints_for_tag_find ---

    #[test]
    fn tag_find_suggests_outlines() {
        let c = ctx(HintSource::TagFind {
            name: "rust".to_owned(),
        });
        let data = json!({
            "tag": "rust",
            "files": ["a.md", "b.md", "c.md", "d.md"],
            "total": 4
        });
        let hints = generate_hints(&c, &data);
        // Up to 3 outlines
        assert_eq!(hints.len(), 3);
        assert!(hints.iter().all(|h| h.contains("outline")));
        assert!(!hints.iter().any(|h| h.contains("d.md")));
    }

    // --- hints_for_tasks ---

    #[test]
    fn tasks_suggests_task_read_for_first_task() {
        let c = ctx(HintSource::Tasks {
            filter: "todo".to_owned(),
        });
        let data = json!([
            {
                "file": "todo.md",
                "tasks": [{"line": 5, "status": " ", "text": "Fix bug", "done": false}],
                "total": 1
            }
        ]);
        let hints = generate_hints(&c, &data);
        assert!(
            hints
                .iter()
                .any(|h| h.contains("task read") && h.contains("todo.md") && h.contains("5"))
        );
    }

    #[test]
    fn tasks_all_filter_also_suggests_todo_view() {
        let c = ctx(HintSource::Tasks {
            filter: "all".to_owned(),
        });
        let data = json!([
            {
                "file": "todo.md",
                "tasks": [{"line": 3, "status": " ", "text": "Work", "done": false}],
                "total": 1
            }
        ]);
        let hints = generate_hints(&c, &data);
        assert!(
            hints
                .iter()
                .any(|h| h.contains("tasks") && h.contains("--todo"))
        );
    }

    #[test]
    fn tasks_skips_files_with_empty_tasks() {
        let c = ctx(HintSource::Tasks {
            filter: "todo".to_owned(),
        });
        let data = json!([
            {"file": "empty.md", "tasks": [], "total": 0},
            {
                "file": "has-tasks.md",
                "tasks": [{"line": 7, "status": " ", "text": "Do it", "done": false}],
                "total": 1
            }
        ]);
        let hints = generate_hints(&c, &data);
        assert!(hints.iter().any(|h| h.contains("has-tasks.md")));
        assert!(!hints.iter().any(|h| h.contains("empty.md")));
    }

    #[test]
    fn tasks_empty_array() {
        let c = ctx(HintSource::Tasks {
            filter: "todo".to_owned(),
        });
        let hints = generate_hints(&c, &json!([]));
        assert!(hints.is_empty());
    }

    // --- hints_for_outline ---

    #[test]
    fn outline_suggests_property_find_and_tag_find() {
        let c = ctx(HintSource::Outline);
        let data = json!({
            "file": "note.md",
            "properties": [{"name": "status", "type": "text", "value": "draft"}],
            "tags": ["rust", "cli"],
            "sections": []
        });
        let hints = generate_hints(&c, &data);
        assert!(
            hints
                .iter()
                .any(|h| h.contains("property find") && h.contains("status"))
        );
        assert!(
            hints
                .iter()
                .any(|h| h.contains("tag find") && h.contains("rust"))
        );
    }

    #[test]
    fn outline_array_uses_first_item() {
        let c = ctx(HintSource::Outline);
        let data = json!([
            {
                "file": "a.md",
                "properties": [{"name": "title", "type": "text", "value": "A"}],
                "tags": ["first"],
                "sections": []
            },
            {
                "file": "b.md",
                "properties": [{"name": "other", "type": "text", "value": "B"}],
                "tags": ["second"],
                "sections": []
            }
        ]);
        let hints = generate_hints(&c, &data);
        // Properties and tags come from first item
        assert!(hints.iter().any(|h| h.contains("title")));
        assert!(hints.iter().any(|h| h.contains("first")));
        assert!(!hints.iter().any(|h| h.contains("other")));
        assert!(!hints.iter().any(|h| h.contains("second")));
    }

    #[test]
    fn outline_no_properties_or_tags() {
        let c = ctx(HintSource::Outline);
        let data = json!({"file": "empty.md", "properties": [], "tags": [], "sections": []});
        let hints = generate_hints(&c, &data);
        assert!(hints.is_empty());
    }

    // --- hints_for_links ---

    #[test]
    fn links_suggests_outline_for_resolved_links() {
        let c = ctx(HintSource::Links {
            file: "index.md".to_owned(),
        });
        let data = json!({
            "path": "index.md",
            "links": [
                {"target": "note-a", "path": "note-a.md"},
                {"target": "note-b", "path": "note-b.md"},
                {"target": "broken", "path": null}
            ]
        });
        let hints = generate_hints(&c, &data);
        assert_eq!(hints.len(), 2);
        assert!(hints[0].contains("note-a.md"));
        assert!(hints[1].contains("note-b.md"));
        // Broken link should not appear
        assert!(!hints.iter().any(|h| h.contains("broken")));
    }

    #[test]
    fn links_only_unresolved_returns_empty() {
        let c = ctx(HintSource::Links {
            file: "index.md".to_owned(),
        });
        let data = json!({
            "path": "index.md",
            "links": [
                {"target": "broken1"},
                {"target": "broken2"}
            ]
        });
        let hints = generate_hints(&c, &data);
        assert!(hints.is_empty());
    }

    // --- flag propagation ---

    #[test]
    fn dir_flag_propagated_to_all_hints() {
        let c = ctx_with_dir(HintSource::TagsSummary, "/vault");
        let data = json!({
            "tags": [{"name": "rust", "count": 5}],
            "total": 1
        });
        let hints = generate_hints(&c, &data);
        assert!(hints[0].contains("--dir"));
        assert!(hints[0].contains("/vault"));
    }

    #[test]
    fn glob_not_included_in_file_specific_hints() {
        let c = ctx_with_glob(
            HintSource::TagFind {
                name: "rust".to_owned(),
            },
            "notes/*.md",
        );
        let data = json!({"tag": "rust", "files": ["a.md"], "total": 1});
        let hints = generate_hints(&c, &data);
        // outline hints are file-specific, should not have --glob
        assert!(!hints.iter().any(|h| h.contains("--glob")));
    }
}
