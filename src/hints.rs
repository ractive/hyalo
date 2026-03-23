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
    TagsSummary,
}

/// Global flags to propagate into generated hint commands.
///
/// Each `Option` field is `Some` only when the user passed the flag explicitly
/// on the CLI. Values that came from `.hyalo.toml` config are omitted so that
/// the copy-pasted hint command inherits the same config automatically.
pub struct HintContext {
    pub source: HintSource,
    /// `None` means "." (default) or came from config — omit from hints.
    pub dir: Option<String>,
    pub glob: Option<String>,
    /// Explicit `--format` from CLI (not from config).
    pub format: Option<String>,
    /// Explicit `--hints` from CLI (not from config).
    pub hints: bool,
}

/// Generate concrete drill-down hints from a command's JSON output.
///
/// Returns at most [`MAX_HINTS`] executable `hyalo` command strings.
#[must_use]
pub fn generate_hints(ctx: &HintContext, data: &serde_json::Value) -> Vec<String> {
    let hints = match &ctx.source {
        HintSource::Summary => hints_for_summary(ctx, data),
        HintSource::PropertiesSummary => hints_for_properties_summary(ctx, data),
        HintSource::TagsSummary => hints_for_tags_summary(ctx, data),
    };
    hints.into_iter().take(MAX_HINTS).collect()
}

// ---------------------------------------------------------------------------
// Command builder
// ---------------------------------------------------------------------------

/// Push the global flags that were explicitly passed on the CLI.
fn push_global_flags(parts: &mut Vec<String>, ctx: &HintContext) {
    if let Some(dir) = &ctx.dir {
        parts.push("--dir".to_owned());
        parts.push(shell_quote(dir));
    }
    if let Some(fmt) = &ctx.format {
        parts.push("--format".to_owned());
        parts.push(shell_quote(fmt));
    }
    if ctx.hints {
        parts.push("--hints".to_owned());
    }
}

/// Build a command that intentionally omits `--glob` (for file-specific hints).
fn build_command_no_glob(ctx: &HintContext, args: &[&str]) -> String {
    let mut parts: Vec<String> = vec!["hyalo".to_owned()];
    push_global_flags(&mut parts, ctx);
    for arg in args {
        parts.push(shell_quote(arg));
    }
    parts.join(" ")
}

/// Build a command that propagates `--glob` when present.
fn build_command_with_glob(ctx: &HintContext, args: &[&str]) -> String {
    let mut parts: Vec<String> = vec!["hyalo".to_owned()];
    push_global_flags(&mut parts, ctx);
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
    if value.eq_ignore_ascii_case("in-progress")
        || value.eq_ignore_ascii_case("in progress")
        || value.eq_ignore_ascii_case("active")
    {
        0
    } else if value.eq_ignore_ascii_case("planned") || value.eq_ignore_ascii_case("todo") {
        1
    } else if value.eq_ignore_ascii_case("draft") || value.eq_ignore_ascii_case("idea") {
        2
    } else if value.eq_ignore_ascii_case("completed")
        || value.eq_ignore_ascii_case("done")
        || value.eq_ignore_ascii_case("archived")
    {
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
    hints.push(build_command_with_glob(ctx, &["properties"]));
    hints.push(build_command_with_glob(ctx, &["tags"]));

    // Suggest find --task todo if there are open tasks.
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
        hints.push(build_command_with_glob(ctx, &["find", "--task", "todo"]));
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
            let filter = format!("status={value}");
            hints.push(build_command_no_glob(ctx, &["find", "--property", &filter]));
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
        .map(|(name, _)| build_command_with_glob(ctx, &["find", "--property", name]))
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
        .map(|(name, _)| build_command_with_glob(ctx, &["find", "--tag", name]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx(source: HintSource) -> HintContext {
        HintContext {
            source,
            dir: None,
            glob: None,
            format: None,
            hints: false,
        }
    }

    fn ctx_with_dir(source: HintSource, dir: &str) -> HintContext {
        HintContext {
            source,
            dir: Some(dir.to_owned()),
            glob: None,
            format: None,
            hints: false,
        }
    }

    fn ctx_with_glob(source: HintSource, glob: &str) -> HintContext {
        HintContext {
            source,
            dir: None,
            glob: Some(glob.to_owned()),
            format: None,
            hints: false,
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
            build_command_no_glob(&c, &["properties"]),
            "hyalo properties"
        );
    }

    #[test]
    fn build_command_with_dir() {
        let c = ctx_with_dir(HintSource::Summary, "/my/vault");
        assert_eq!(
            build_command_no_glob(&c, &["tags"]),
            "hyalo --dir /my/vault tags"
        );
    }

    #[test]
    fn build_command_with_glob_propagated() {
        let c = ctx_with_glob(HintSource::PropertiesSummary, "**/*.md");
        assert_eq!(
            build_command_with_glob(&c, &["properties"]),
            "hyalo --glob '**/*.md' properties"
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
        assert!(hints.iter().any(|h| {
            h == "hyalo properties"
                || (h.starts_with("hyalo --dir ") && h.ends_with(" properties"))
                || (h.starts_with("hyalo --glob ") && h.ends_with(" properties"))
        }));
        assert!(hints.iter().any(|h| {
            h == "hyalo tags"
                || (h.starts_with("hyalo --dir ") && h.ends_with(" tags"))
                || (h.starts_with("hyalo --glob ") && h.ends_with(" tags"))
        }));
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
                .any(|h| h.contains("find") && h.contains("--task") && h.contains("todo"))
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
}
