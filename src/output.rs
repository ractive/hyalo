use std::fmt::Write as _;

use jaq_core::load::{self, Arena, File, Loader};
use jaq_core::{Compiler, Ctx, RcIter};
use jaq_json::Val;
use serde::Serialize;
use serde_json::json;

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Text,
}

/// Result of a command execution: either success (exit 0) or a user-facing error (exit 1).
/// Internal/unexpected errors are represented by `anyhow::Error` at the call site.
#[derive(Debug)]
pub enum CommandOutcome {
    /// Successful operation — output goes to stdout.
    Success(String),
    /// User error (file not found, property missing, etc.) — output goes to stderr.
    UserError(String),
}

impl Format {
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "json" => Some(Self::Json),
            "text" => Some(Self::Text),
            _ => None,
        }
    }
}

/// Format a successful JSON value for output.
#[must_use]
pub fn format_success(format: Format, value: &serde_json::Value) -> String {
    match format {
        Format::Json => serde_json::to_string_pretty(value).unwrap_or_default(),
        Format::Text => format_value_as_text(value),
    }
}

/// Format any `Serialize` type for output.
///
/// Converts the value to `serde_json::Value` first so that the text formatter
/// can operate on a uniform representation.
#[must_use]
pub fn format_output<T: Serialize>(format: Format, value: &T) -> String {
    let json = serde_json::to_value(value).expect("derived Serialize impl should not fail");
    format_success(format, &json)
}

/// Format output with drill-down hints appended.
///
/// - **JSON**: wraps the original value in `{"data": ..., "hints": [...]}`
/// - **Text**: appends `  -> <command>` lines after the formatted output
///
/// If `hints` is empty, produces the same output as [`format_success`].
#[must_use]
pub fn format_with_hints(format: Format, value: &serde_json::Value, hints: &[String]) -> String {
    if hints.is_empty() {
        return format_success(format, value);
    }
    match format {
        Format::Json => {
            let envelope = serde_json::json!({
                "data": value,
                "hints": hints,
            });
            serde_json::to_string_pretty(&envelope).unwrap_or_default()
        }
        Format::Text => {
            let mut text = format_value_as_text(value);
            text.push('\n');
            for hint in hints {
                text.push_str("\n  -> ");
                text.push_str(hint);
            }
            text
        }
    }
}

/// Format an error for output to stderr.
#[must_use]
pub fn format_error(
    format: Format,
    error: &str,
    path: Option<&str>,
    hint: Option<&str>,
    cause: Option<&str>,
) -> String {
    match format {
        Format::Json => {
            let mut obj = json!({"error": error});
            if let Some(p) = path {
                obj["path"] = json!(p);
            }
            if let Some(h) = hint {
                obj["hint"] = json!(h);
            }
            if let Some(c) = cause {
                obj["cause"] = json!(c);
            }
            serde_json::to_string_pretty(&obj).unwrap_or_default()
        }
        Format::Text => {
            let mut msg = format!("Error: {error}");
            if let Some(p) = path {
                let _ = write!(msg, "\n  path: {p}");
            }
            if let Some(h) = hint {
                let _ = write!(msg, "\n  hint: {h}");
            }
            if let Some(c) = cause {
                let _ = write!(msg, "\n  cause: {c}");
            }
            msg
        }
    }
}

// ---------------------------------------------------------------------------
// jq filter constants — one per output type
// ---------------------------------------------------------------------------

/// `PropertyInfo`: `{name, type, value}`
/// When value is an array (list type), join elements with ", " for readability.
const PROPERTY_INFO_FILTER: &str = r#""\(.name) (\(.type)): \(if (.value | type) == "array" then "[" + (.value | join(", ")) + "]" else .value end)""#;

/// `PropertySummaryEntry`: `{count, name, type}`
const PROPERTY_SUMMARY_ENTRY_FILTER: &str =
    r#""\(.name)\t\(.type)\t\(.count) \(if .count == 1 then "file" else "files" end)""#;

/// `TagSummary`: `{tags, total}`
const TAG_SUMMARY_FILTER: &str = r#""\(.total) unique \(if .total == 1 then "tag" else "tags" end)\n\(.tags | map("  \(.name)\t\(.count) \(if .count == 1 then "file" else "files" end)") | join("\n"))""#;

/// `TagSummaryEntry`: `{count, name}`
const TAG_SUMMARY_ENTRY_FILTER: &str =
    r#""\(.name)\t\(.count) \(if .count == 1 then "file" else "files" end)""#;

/// `LinkInfo` — just target: `{target}`
const LINK_INFO_TARGET_FILTER: &str = r#""  \(.target) (unresolved)""#;

/// `LinkInfo` with path: `{path, target}`
const LINK_INFO_PATH_FILTER: &str = r#""  \(.target) → \(.path)""#;

/// `LinkInfo` with label: `{label, target}`
const LINK_INFO_LABEL_FILTER: &str = r#""  \(.target) (unresolved) [\(.label)]""#;

/// `LinkInfo` with path and label: `{label, path, target}`
const LINK_INFO_FULL_FILTER: &str = r#""  \(.target) → \(.path) [\(.label)]""#;

/// `TaskCount`: `{done, total}`
const TASK_COUNT_FILTER: &str = r#""[\(.done)/\(.total)]""#;

/// `OutlineSection` without tasks: `{code_blocks, heading, level, line, links}`
const OUTLINE_SECTION_FILTER: &str = r##""\("#" * .level) \(.heading // "(pre-heading)")\(if (.links | length) > 0 then "\n  → \(.links | join(", "))" else "" end)""##;

/// `OutlineSection` with tasks: `{code_blocks, heading, level, line, links, tasks}`
const OUTLINE_SECTION_WITH_TASKS_FILTER: &str = r##""\("#" * .level) \(.heading // "(pre-heading)") [\(.tasks.done)/\(.tasks.total)]\(if (.links | length) > 0 then "\n  → \(.links | join(", "))" else "" end)""##;

/// `TaskInfo`: `{done, line, status, text}`
const TASK_INFO_FILTER: &str =
    r#""line \(.line): [\(.status)] \(.text)\(if .done then " (done)" else "" end)""#;

/// `TaskReadResult`: `{done, file, line, status, text}`
const TASK_READ_RESULT_FILTER: &str =
    r#""\(.file):\(.line) [\(.status)] \(.text)\(if .done then " (done)" else "" end)""#;

/// `VaultSummary`: `{files, properties, recent_files, status, tags, tasks}`
const VAULT_SUMMARY_FILTER: &str = r#""Files: \(.files.total) total\(if (.files.by_directory | length) > 0 then " (\(.files.by_directory | map("\(.directory): \(.count)") | join(", ")))" else "" end)\nProperties: \(.properties | length) unique\nTags: \(.tags.total) unique\nStatus: \(if (.status | length) > 0 then (.status | map("\(.value) (\(.files | length))") | join(", ")) else "(none)" end)\nTasks: \(.tasks.done)/\(.tasks.total)\nRecent: \(if (.recent_files | length) > 0 then (.recent_files | map(.path) | join(", ")) else "(none)" end)""#;

/// `FindTaskInfo`: `{done, line, section, status, text}`
/// Format: `  [x] text (line N, section)` or `  [ ] text (line N, section)`
const FIND_TASK_INFO_FILTER: &str =
    r#""  [\(if .done then "x" else " " end)] \(.text) (line \(.line), \(.section))""#;

/// `ContentMatch`: `{line, section, text}`
/// Format: `  line N (section): text`
const CONTENT_MATCH_FILTER: &str = r#""  line \(.line) (\(.section)): \(.text)""#;

/// Mutation result with `property` + `value` fields:
/// covers `SetPropertyResult`, `AppendPropertyResult`, and `RemovePropertyResult` (with value).
/// Key signature: `modified,property,skipped,total,value`
/// Format: `property=value: N/T modified` followed by modified file paths.
const PROPERTY_VALUE_MUTATION_FILTER: &str = r#""\(.property)=\(.value): \(.modified | length)/\(.total) modified\(if (.modified | length) > 0 then "\n\(.modified | map("  \(.)") | join("\n"))" else "" end)""#;

/// Mutation result with `property` only (no value field):
/// covers `RemovePropertyResult` (without value).
/// Key signature: `modified,property,skipped,total`
/// Format: `property: N/T modified` followed by modified file paths.
const PROPERTY_MUTATION_FILTER: &str = r#""\(.property): \(.modified | length)/\(.total) modified\(if (.modified | length) > 0 then "\n\(.modified | map("  \(.)") | join("\n"))" else "" end)""#;

/// Mutation result with `tag` field:
/// covers `SetTagResult` and `RemoveTagResult`.
/// Key signature: `modified,skipped,tag,total`
/// Format: `tag: N/T modified` followed by modified file paths.
const TAG_MUTATION_FILTER: &str = r#""\(.tag): \(.modified | length)/\(.total) modified\(if (.modified | length) > 0 then "\n\(.modified | map("  \(.)") | join("\n"))" else "" end)""#;

// ---------------------------------------------------------------------------
// Shape-based filter lookup
// ---------------------------------------------------------------------------

/// Compute a sorted comma-joined key signature from a JSON object's top-level keys.
fn key_signature(map: &serde_json::Map<String, serde_json::Value>) -> String {
    let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys.join(",")
}

/// Look up the jq filter for a given key signature.
///
/// Returns `None` for unknown shapes, which will fall back to generic formatting.
fn lookup_filter(key_sig: &str) -> Option<&'static str> {
    match key_sig {
        // PropertyInfo
        "name,type,value" => Some(PROPERTY_INFO_FILTER),
        // PropertySummaryEntry
        "count,name,type" => Some(PROPERTY_SUMMARY_ENTRY_FILTER),
        // TagSummary
        "tags,total" => Some(TAG_SUMMARY_FILTER),
        // TagSummaryEntry
        "count,name" => Some(TAG_SUMMARY_ENTRY_FILTER),
        // LinkInfo variants (optional path and label → 4 combos)
        "target" => Some(LINK_INFO_TARGET_FILTER),
        "path,target" => Some(LINK_INFO_PATH_FILTER),
        "label,target" => Some(LINK_INFO_LABEL_FILTER),
        "label,path,target" => Some(LINK_INFO_FULL_FILTER),
        // TaskCount
        "done,total" => Some(TASK_COUNT_FILTER),
        // OutlineSection (with and without tasks)
        "code_blocks,heading,level,line,links" => Some(OUTLINE_SECTION_FILTER),
        "code_blocks,heading,level,line,links,tasks" => Some(OUTLINE_SECTION_WITH_TASKS_FILTER),
        // TaskInfo
        "done,line,status,text" => Some(TASK_INFO_FILTER),
        // FindTaskInfo
        "done,line,section,status,text" => Some(FIND_TASK_INFO_FILTER),
        // ContentMatch
        "line,section,text" => Some(CONTENT_MATCH_FILTER),
        // TaskReadResult
        "done,file,line,status,text" => Some(TASK_READ_RESULT_FILTER),
        // VaultSummary
        "files,properties,recent_files,status,tags,tasks" => Some(VAULT_SUMMARY_FILTER),
        // Mutation results with property + value (SetPropertyResult, AppendPropertyResult,
        // RemovePropertyResult with value)
        "modified,property,skipped,total,value" => Some(PROPERTY_VALUE_MUTATION_FILTER),
        // Mutation results with property only (RemovePropertyResult without value)
        "modified,property,skipped,total" => Some(PROPERTY_MUTATION_FILTER),
        // Mutation results with tag (SetTagResult, RemoveTagResult)
        "modified,skipped,tag,total" => Some(TAG_MUTATION_FILTER),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// jq filter execution engine
// ---------------------------------------------------------------------------

/// Apply a jq filter string to a `serde_json::Value` and return the text output.
///
/// Multiple outputs are joined with newlines. On any error (parse or runtime),
/// returns `None` (used internally by the text formatter, which has its own fallbacks).
fn apply_jq_filter(filter_code: &str, value: &serde_json::Value) -> Option<String> {
    run_jq_filter(filter_code, value).ok()
}

/// Apply a user-supplied jq filter to a `serde_json::Value`.
///
/// Returns `Ok(String)` with newline-joined output values on success, or
/// `Err(String)` with a human-readable description of the parse or runtime error.
pub fn apply_jq_filter_result(
    filter_code: &str,
    value: &serde_json::Value,
) -> Result<String, String> {
    run_jq_filter(filter_code, value)
}

/// Format a jaq load error (lex/parse/IO) into a human-readable string.
///
/// `load::Error<&str>` does not implement `Display`, so we extract the first
/// error's kind and the offending source snippet manually.
fn format_load_errors(errs: &load::Errors<&str, ()>) -> String {
    // errs is Vec<(File<&str, ()>, load::Error<&str>)>
    // We take the first entry and describe its error kind.
    for (_file, err) in errs {
        match err {
            load::Error::Io(ios) => {
                if let Some((_path, msg)) = ios.first() {
                    return format!("jq filter error (IO): {msg}");
                }
            }
            load::Error::Lex(lex_errs) => {
                if let Some((expect, span)) = lex_errs.first() {
                    return format!(
                        "jq filter syntax error: expected {} near {:?}",
                        expect.as_str(),
                        span
                    );
                }
            }
            load::Error::Parse(parse_errs) => {
                if let Some((expect, _token)) = parse_errs.first() {
                    return format!("jq filter parse error: expected {}", expect.as_str());
                }
            }
        }
    }
    "jq filter error: invalid filter syntax".to_owned()
}

/// Core jq execution logic shared by `apply_jq_filter` and `apply_jq_filter_result`.
fn run_jq_filter(filter_code: &str, value: &serde_json::Value) -> Result<String, String> {
    let program = File {
        code: filter_code,
        path: (),
    };
    let loader = Loader::new(jaq_std::defs().chain(jaq_json::defs()));
    let arena = Arena::default();

    let modules = loader
        .load(&arena, program)
        .map_err(|errs| format_load_errors(&errs))?;
    let filter = Compiler::default()
        .with_funs(jaq_std::funs().chain(jaq_json::funs()))
        .compile(modules)
        .map_err(|errs| {
            // compile::Errors = Vec<(File<S,P>, Vec<(S, Undefined)>)>
            // Extract the first undefined symbol name for a useful message.
            let first = errs.iter().flat_map(|(_file, undefs)| undefs.iter()).next();
            if let Some((name, undef)) = first {
                format!("jq filter error: undefined {} {:?}", undef.as_str(), name)
            } else {
                "jq filter error: compilation failed".to_owned()
            }
        })?;

    let input = Val::from(value.clone());
    let inputs = RcIter::new(core::iter::empty());
    let ctx = Ctx::new([], &inputs);

    let mut parts = Vec::new();
    for result in filter.run((ctx, input)) {
        match result {
            Ok(val) => {
                let s = match val {
                    Val::Str(s) => (*s).clone(),
                    other => serde_json::Value::from(other).to_string(),
                };
                parts.push(s);
            }
            Err(e) => return Err(format!("jq runtime error: {e}")),
        }
    }

    Ok(parts.join("\n"))
}

// ---------------------------------------------------------------------------
// FileObject dynamic filter builder
// ---------------------------------------------------------------------------

/// Build a jaq filter string for a `FileObject` by inspecting which optional
/// fields are present in the JSON object.
///
/// The file header is always emitted. Each optional section (properties, tags,
/// sections, tasks, matches, links) is included only when the key is present.
fn build_file_object_filter(map: &serde_json::Map<String, serde_json::Value>) -> String {
    // Header: file path and modified timestamp — always present.
    let mut parts = vec![r#""\(.file)  (\(.modified))""#.to_owned()];

    // Properties: each rendered as "  name (type): value"
    if map.contains_key("properties") {
        parts.push(
            r##"if .properties then (.properties | map("  \(.name) (\(.type)): \(if (.value | type) == "array" then "[" + (.value | join(", ")) + "]" else .value end)") | join("\n")) else empty end"##.to_owned(),
        );
    }

    // Tags: "  tags: [tag1, tag2, ...]"
    if map.contains_key("tags") {
        parts.push(
            r#"if (.tags | length) > 0 then "  tags: [\(.tags | join(", "))]" else empty end"#
                .to_owned(),
        );
    }

    // Sections: each as "  ## Heading [done/total]" or "  ## Heading"
    // Note: uses r##"..."## because the jq filter contains the sequence "#" (hash-quoted).
    if map.contains_key("sections") {
        parts.push(
            r##"if .sections then (.sections | map("  \("#" * .level) \(.heading // "(pre-heading)")\(if .tasks then " [\(.tasks.done)/\(.tasks.total)]" else "" end)") | join("\n")) else empty end"##.to_owned(),
        );
    }

    // Tasks: header then each as "    [x] text (line N)"
    if map.contains_key("tasks") {
        parts.push(
            r##"if (.tasks | length) > 0 then "  tasks:\n\(.tasks | map("    [\(if .done then "x" else " " end)] \(.text) (line \(.line))") | join("\n"))" else empty end"##.to_owned(),
        );
    }

    // Matches: header then each as "    line N (section): text"
    if map.contains_key("matches") {
        parts.push(
            r##"if (.matches | length) > 0 then "  matches:\n\(.matches | map("    line \(.line) (\(.section)): \(.text)") | join("\n"))" else empty end"##.to_owned(),
        );
    }

    // Links: header then each as "    target → path" or "    target (unresolved)"
    if map.contains_key("links") {
        parts.push(
            r##"if (.links | length) > 0 then "  links:\n\(.links | map("    \(.target)\(if .path then " → \(.path)" else " (unresolved)" end)") | join("\n"))" else empty end"##.to_owned(),
        );
    }

    parts.join(", ")
}

// ---------------------------------------------------------------------------
// Text formatting
// ---------------------------------------------------------------------------

/// Format a JSON value as human-readable text using jq filters where available.
fn format_value_as_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Array(arr) => {
            // Use blank-line separator between FileObjects for readability.
            let is_file_objects = arr
                .first()
                .and_then(|v| v.as_object())
                .map(|m| m.contains_key("file") && m.contains_key("modified"))
                .unwrap_or(false);
            let sep = if is_file_objects { "\n\n" } else { "\n" };
            arr.iter()
                .map(format_value_as_text)
                .collect::<Vec<_>>()
                .join(sep)
        }
        serde_json::Value::Object(map) => {
            let sig = key_signature(map);
            if let Some(filter) = lookup_filter(&sig)
                && let Some(output) = apply_jq_filter(filter, value)
            {
                return output;
            }
            // FileObject: dynamically compose filter from present fields.
            if map.contains_key("file") && map.contains_key("modified") {
                let filter = build_file_object_filter(map);
                if let Some(output) = apply_jq_filter(&filter, value) {
                    return output;
                }
            }
            // Fallback: generic key: value lines
            format_object_generic(map)
        }
        other => format_scalar(other),
    }
}

/// Generic key: value rendering for unknown object shapes.
fn format_object_generic(map: &serde_json::Map<String, serde_json::Value>) -> String {
    map.iter()
        .map(|(k, v)| format!("{k}: {}", format_value_as_text(v)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format a scalar JSON value as text.
fn format_scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_owned(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_scalar).collect();
            items.join(", ")
        }
        serde_json::Value::Object(_) => format_value_as_text(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- error formatting ---

    #[test]
    fn format_json_error() {
        let out = format_error(
            Format::Json,
            "file not found",
            Some("foo/bar"),
            Some("did you mean foo/bar.md?"),
            None,
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["error"], "file not found");
        assert_eq!(parsed["hint"], "did you mean foo/bar.md?");
        assert!(parsed.get("cause").is_none());
    }

    #[test]
    fn format_text_error() {
        let out = format_error(Format::Text, "file not found", Some("foo"), None, None);
        assert!(out.contains("Error: file not found"));
        assert!(out.contains("path: foo"));
    }

    #[test]
    fn format_json_success() {
        let val = json!({"name": "test", "value": 42});
        let out = format_success(Format::Json, &val);
        assert!(out.contains("\"name\": \"test\""));
    }

    // --- apply_jq_filter ---

    #[test]
    fn apply_jq_filter_simple() {
        let val = json!({"name": "hello", "count": 3});
        let result = apply_jq_filter(r#""\(.name): \(.count)""#, &val);
        assert_eq!(result.as_deref(), Some("hello: 3"));
    }

    #[test]
    fn apply_jq_filter_array_map() {
        let val = json!(["a", "b", "c"]);
        let result = apply_jq_filter(".[]", &val);
        assert_eq!(result.as_deref(), Some("a\nb\nc"));
    }

    #[test]
    fn apply_jq_filter_invalid_returns_none() {
        let val = json!({"x": 1});
        let result = apply_jq_filter("this is not valid jq %%%", &val);
        assert!(result.is_none());
    }

    // --- property type filters ---

    #[test]
    fn property_info_filter() {
        let val = json!({"name": "title", "type": "text", "value": "My Note"});
        let out = apply_jq_filter(PROPERTY_INFO_FILTER, &val).unwrap();
        assert!(out.contains("title"));
        assert!(out.contains("text"));
        assert!(out.contains("My Note"));
    }

    #[test]
    fn property_info_filter_list_value() {
        let val = json!({"name": "tags", "type": "list", "value": ["rust", "cli"]});
        let out = apply_jq_filter(PROPERTY_INFO_FILTER, &val).unwrap();
        assert!(out.contains("tags"));
        assert!(out.contains("list"));
        // Array values should be wrapped in brackets and joined with ", "
        assert!(out.contains("[rust, cli]"), "expected [rust, cli]: {out}");
        assert!(!out.contains("[\"rust\""));
    }

    #[test]
    fn property_summary_entry_filter() {
        let val = json!({"count": 7, "name": "title", "type": "text"});
        let out = apply_jq_filter(PROPERTY_SUMMARY_ENTRY_FILTER, &val).unwrap();
        assert!(out.contains("title"));
        assert!(out.contains("text"));
        assert!(out.contains("7 files"));
    }

    #[test]
    fn tag_summary_filter() {
        let val = json!({
            "tags": [{"name": "rust", "count": 3}, {"name": "cli", "count": 1}],
            "total": 2
        });
        let out = apply_jq_filter(TAG_SUMMARY_FILTER, &val).unwrap();
        assert!(out.contains("2 unique tags"));
        assert!(out.contains("rust"));
        assert!(out.contains("3 files"));
    }

    // --- link type filters ---

    #[test]
    fn link_info_target_only_filter() {
        let val = json!({"target": "broken-link"});
        let out = apply_jq_filter(LINK_INFO_TARGET_FILTER, &val).unwrap();
        assert!(out.contains("broken-link"));
        assert!(out.contains("unresolved"));
    }

    #[test]
    fn link_info_with_path_filter() {
        let val = json!({"path": "note-b.md", "target": "note-b"});
        let out = apply_jq_filter(LINK_INFO_PATH_FILTER, &val).unwrap();
        assert!(out.contains("note-b"));
        assert!(out.contains("note-b.md"));
    }

    // --- outline type filters ---

    #[test]
    fn task_count_filter() {
        let val = json!({"done": 3, "total": 5});
        let out = apply_jq_filter(TASK_COUNT_FILTER, &val).unwrap();
        assert_eq!(out, "[3/5]");
    }

    #[test]
    fn outline_section_filter() {
        let val = json!({
            "code_blocks": [],
            "heading": "Introduction",
            "level": 1,
            "line": 5,
            "links": ["[[other]]"]
        });
        let out = apply_jq_filter(OUTLINE_SECTION_FILTER, &val).unwrap();
        assert!(out.contains("#"));
        assert!(out.contains("Introduction"));
        assert!(out.contains("[[other]]"));
    }

    #[test]
    fn outline_section_with_tasks_filter() {
        let val = json!({
            "code_blocks": [],
            "heading": "Tasks",
            "level": 2,
            "line": 10,
            "links": [],
            "tasks": {"done": 2, "total": 4}
        });
        let out = apply_jq_filter(OUTLINE_SECTION_WITH_TASKS_FILTER, &val).unwrap();
        assert!(out.contains("##"));
        assert!(out.contains("Tasks"));
        assert!(out.contains("[2/4]"));
    }

    // --- FindTaskInfo filter ---

    #[test]
    fn find_task_info_filter_done() {
        let val = json!({
            "done": true,
            "line": 42,
            "section": "Implementation",
            "status": "x",
            "text": "Write the tests"
        });
        let out = apply_jq_filter(FIND_TASK_INFO_FILTER, &val).unwrap();
        assert!(out.contains("[x]"));
        assert!(out.contains("Write the tests"));
        assert!(out.contains("line 42"));
        assert!(out.contains("Implementation"));
    }

    #[test]
    fn find_task_info_filter_not_done() {
        let val = json!({
            "done": false,
            "line": 7,
            "section": "Todo",
            "status": " ",
            "text": "Review PR"
        });
        let out = apply_jq_filter(FIND_TASK_INFO_FILTER, &val).unwrap();
        assert!(out.contains("[ ]"));
        assert!(out.contains("Review PR"));
        assert!(out.contains("line 7"));
        assert!(out.contains("Todo"));
    }

    #[test]
    fn find_task_info_via_format_value_as_text() {
        // Verify that format_value_as_text dispatches to the correct filter.
        let val = json!({
            "done": true,
            "line": 5,
            "section": "Goals",
            "status": "x",
            "text": "Ship it"
        });
        let out = format_value_as_text(&val);
        assert!(out.contains("[x]"));
        assert!(out.contains("Ship it"));
        assert!(
            !out.contains("done: true"),
            "should not use generic fallback"
        );
    }

    // --- ContentMatch filter ---

    #[test]
    fn content_match_filter() {
        let val = json!({
            "line": 15,
            "section": "Background",
            "text": "This is the matching line"
        });
        let out = apply_jq_filter(CONTENT_MATCH_FILTER, &val).unwrap();
        assert!(out.contains("line 15"));
        assert!(out.contains("Background"));
        assert!(out.contains("This is the matching line"));
    }

    #[test]
    fn content_match_via_format_value_as_text() {
        let val = json!({
            "line": 3,
            "section": "Intro",
            "text": "hello world"
        });
        let out = format_value_as_text(&val);
        assert!(out.contains("line 3"));
        assert!(out.contains("hello world"));
        assert!(!out.contains("line: 3"), "should not use generic fallback");
    }

    // --- Mutation result filters ---

    #[test]
    fn property_value_mutation_filter_with_modified() {
        // SetPropertyResult / AppendPropertyResult / RemovePropertyResult (with value)
        let val = json!({
            "modified": ["note-a.md", "note-b.md"],
            "property": "status",
            "skipped": [],
            "total": 2,
            "value": "done"
        });
        let out = apply_jq_filter(PROPERTY_VALUE_MUTATION_FILTER, &val).unwrap();
        assert!(out.contains("status=done"));
        assert!(out.contains("2/2 modified"));
        assert!(out.contains("note-a.md"));
        assert!(out.contains("note-b.md"));
    }

    #[test]
    fn property_value_mutation_filter_all_skipped() {
        let val = json!({
            "modified": [],
            "property": "priority",
            "skipped": ["note-a.md"],
            "total": 1,
            "value": "high"
        });
        let out = apply_jq_filter(PROPERTY_VALUE_MUTATION_FILTER, &val).unwrap();
        assert!(out.contains("priority=high"));
        assert!(out.contains("0/1 modified"));
        // No file paths should appear when nothing was modified
        assert!(!out.contains("note-a.md"));
    }

    #[test]
    fn property_value_mutation_via_format_value_as_text() {
        let val = json!({
            "modified": ["notes/a.md"],
            "property": "status",
            "skipped": [],
            "total": 1,
            "value": "done"
        });
        let out = format_value_as_text(&val);
        assert!(out.contains("status=done"));
        assert!(
            !out.contains("modified: "),
            "should not use generic fallback"
        );
    }

    #[test]
    fn property_mutation_filter_no_value() {
        // RemovePropertyResult without value
        let val = json!({
            "modified": ["note.md"],
            "property": "draft",
            "skipped": [],
            "total": 1
        });
        let out = apply_jq_filter(PROPERTY_MUTATION_FILTER, &val).unwrap();
        assert!(out.contains("draft"));
        assert!(out.contains("1/1 modified"));
        assert!(out.contains("note.md"));
    }

    #[test]
    fn tag_mutation_filter_with_modified() {
        // SetTagResult / RemoveTagResult
        let val = json!({
            "modified": ["a.md", "b.md"],
            "skipped": ["c.md"],
            "tag": "rust",
            "total": 3
        });
        let out = apply_jq_filter(TAG_MUTATION_FILTER, &val).unwrap();
        assert!(out.contains("rust"));
        assert!(out.contains("2/3 modified"));
        assert!(out.contains("a.md"));
        assert!(out.contains("b.md"));
        assert!(!out.contains("c.md"));
    }

    #[test]
    fn tag_mutation_via_format_value_as_text() {
        let val = json!({
            "modified": [],
            "skipped": ["note.md"],
            "tag": "cli",
            "total": 1
        });
        let out = format_value_as_text(&val);
        assert!(out.contains("cli"));
        assert!(!out.contains("tag: cli"), "should not use generic fallback");
    }

    // --- build_file_object_filter ---

    #[test]
    fn build_file_object_filter_minimal() {
        // Only the required `file` and `modified` fields.
        let map: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"file": "notes/foo.md", "modified": "2024-01-01"}"#).unwrap();
        let filter = build_file_object_filter(&map);
        let val = json!({"file": "notes/foo.md", "modified": "2024-01-01"});
        let out = apply_jq_filter(&filter, &val).unwrap();
        assert!(out.contains("notes/foo.md"));
        assert!(out.contains("2024-01-01"));
    }

    #[test]
    fn build_file_object_filter_with_tags() {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"file": "foo.md", "modified": "2024-01-01", "tags": ["rust", "cli"]}"#,
        )
        .unwrap();
        let filter = build_file_object_filter(&map);
        let val = json!({"file": "foo.md", "modified": "2024-01-01", "tags": ["rust", "cli"]});
        let out = apply_jq_filter(&filter, &val).unwrap();
        assert!(out.contains("foo.md"));
        assert!(out.contains("tags: [rust, cli]"));
    }

    #[test]
    fn build_file_object_filter_with_properties() {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"file": "foo.md", "modified": "2024-01-01", "properties": [{"name": "status", "type": "text", "value": "done"}]}"#,
        )
        .unwrap();
        let filter = build_file_object_filter(&map);
        let val = json!({
            "file": "foo.md",
            "modified": "2024-01-01",
            "properties": [{"name": "status", "type": "text", "value": "done"}]
        });
        let out = apply_jq_filter(&filter, &val).unwrap();
        assert!(out.contains("foo.md"));
        assert!(out.contains("status (text): done"));
    }

    #[test]
    fn build_file_object_filter_with_tasks() {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"file": "foo.md", "modified": "2024-01-01", "tasks": [{"done": true, "line": 5, "section": "Goals", "status": "x", "text": "Ship it"}]}"#,
        )
        .unwrap();
        let filter = build_file_object_filter(&map);
        let val = json!({
            "file": "foo.md",
            "modified": "2024-01-01",
            "tasks": [{"done": true, "line": 5, "section": "Goals", "status": "x", "text": "Ship it"}]
        });
        let out = apply_jq_filter(&filter, &val).unwrap();
        assert!(out.contains("foo.md"));
        assert!(out.contains("tasks:"));
        assert!(out.contains("[x] Ship it"));
        assert!(out.contains("line 5"));
    }

    #[test]
    fn build_file_object_filter_with_matches() {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"file": "foo.md", "modified": "2024-01-01", "matches": [{"line": 3, "section": "Intro", "text": "hello world"}]}"#,
        )
        .unwrap();
        let filter = build_file_object_filter(&map);
        let val = json!({
            "file": "foo.md",
            "modified": "2024-01-01",
            "matches": [{"line": 3, "section": "Intro", "text": "hello world"}]
        });
        let out = apply_jq_filter(&filter, &val).unwrap();
        assert!(out.contains("foo.md"));
        assert!(out.contains("matches:"));
        assert!(out.contains("line 3 (Intro): hello world"));
    }

    #[test]
    fn build_file_object_filter_with_links() {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"file": "foo.md", "modified": "2024-01-01", "links": [{"target": "bar", "path": "bar.md"}]}"#,
        )
        .unwrap();
        let filter = build_file_object_filter(&map);
        let val = json!({
            "file": "foo.md",
            "modified": "2024-01-01",
            "links": [{"target": "bar", "path": "bar.md"}]
        });
        let out = apply_jq_filter(&filter, &val).unwrap();
        assert!(out.contains("foo.md"));
        assert!(out.contains("links:"));
        assert!(out.contains("bar → bar.md"));
    }

    #[test]
    fn build_file_object_filter_unresolved_link() {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"file": "foo.md", "modified": "2024-01-01", "links": [{"target": "missing"}]}"#,
        )
        .unwrap();
        let filter = build_file_object_filter(&map);
        let val = json!({
            "file": "foo.md",
            "modified": "2024-01-01",
            "links": [{"target": "missing"}]
        });
        let out = apply_jq_filter(&filter, &val).unwrap();
        assert!(out.contains("missing (unresolved)"));
    }

    // --- FileObject text rendering through format_value_as_text ---

    #[test]
    fn file_object_text_rendering_minimal() {
        let val = json!({"file": "notes/foo.md", "modified": "2024-01-15"});
        let out = format_value_as_text(&val);
        assert!(out.contains("notes/foo.md"));
        assert!(out.contains("2024-01-15"));
        // Should not look like generic fallback
        assert!(!out.contains("file: notes/foo.md"));
    }

    #[test]
    fn file_object_text_rendering_full() {
        let val = json!({
            "file": "notes/project.md",
            "modified": "2024-03-01",
            "tags": ["rust", "work"],
            "properties": [{"name": "status", "type": "text", "value": "active"}],
            "tasks": [
                {"done": false, "line": 10, "section": "Todo", "status": " ", "text": "Fix bug"},
                {"done": true, "line": 20, "section": "Done", "status": "x", "text": "Write docs"}
            ]
        });
        let out = format_value_as_text(&val);
        assert!(out.contains("notes/project.md"));
        assert!(out.contains("tags: [rust, work]"));
        assert!(out.contains("status (text): active"));
        assert!(out.contains("tasks:"));
        assert!(out.contains("[ ] Fix bug"));
        assert!(out.contains("[x] Write docs"));
    }

    // --- Array of FileObjects with blank-line separator ---

    #[test]
    fn array_of_file_objects_uses_blank_line_separator() {
        let val = json!([
            {"file": "a.md", "modified": "2024-01-01"},
            {"file": "b.md", "modified": "2024-01-02"}
        ]);
        let out = format_value_as_text(&val);
        assert!(out.contains("a.md"));
        assert!(out.contains("b.md"));
        // Should have a blank line between entries
        assert!(
            out.contains("\n\n"),
            "expected blank-line separator between file objects"
        );
    }

    #[test]
    fn array_of_non_file_objects_uses_single_newline() {
        let val = json!([
            {"count": 1, "name": "status", "type": "text"},
            {"count": 3, "name": "title", "type": "text"}
        ]);
        let out = format_value_as_text(&val);
        assert!(out.contains("status"));
        assert!(out.contains("title"));
        // Should NOT have a blank line separator
        assert!(
            !out.contains("\n\n"),
            "non-file-objects should use single newline"
        );
    }

    // --- format_scalar nested object delegation ---

    #[test]
    fn format_scalar_delegates_nested_objects() {
        // A nested object with a known shape should get its filter applied,
        // not the k=v flat format.
        let inner = json!({"count": 2, "name": "status", "type": "text"});
        let out = format_scalar(&inner);
        // Should NOT look like the old "count=2, name=status, type=text" format.
        assert!(
            !out.contains("count=2"),
            "should delegate to format_value_as_text"
        );
        // Should look like the PropertySummaryEntry filter output.
        assert!(out.contains("status"));
        assert!(out.contains("2 files"));
    }

    // --- format_value_as_text integration ---

    #[test]
    fn format_value_as_text_uses_filter_for_known_shape() {
        // PropertySummaryEntry has a known shape: {count, name, type}
        let val = json!({"count": 3, "name": "status", "type": "text"});
        let out = format_value_as_text(&val);
        assert!(out.contains("status"));
        assert!(out.contains("3 files"));
        // Should NOT look like "count: 3" (that's the generic fallback)
        assert!(!out.contains("count: 3"));
    }

    #[test]
    fn format_value_as_text_falls_back_for_unknown_shape() {
        let val = json!({"foo": "bar", "baz": 42});
        let out = format_value_as_text(&val);
        // Generic fallback: key: value
        assert!(out.contains("foo: bar") || out.contains("baz: 42"));
    }

    #[test]
    fn format_value_as_text_array_of_typed_objects() {
        let val = json!([
            {"path": "a.md", "tags": ["rust"]},
            {"path": "b.md", "tags": ["cli"]}
        ]);
        let out = format_value_as_text(&val);
        assert!(out.contains("a.md"));
        assert!(out.contains("b.md"));
        assert!(out.contains("rust"));
        assert!(out.contains("cli"));
    }
}
