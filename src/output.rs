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
const PROPERTY_INFO_FILTER: &str = r#""\(.name) (\(.type)): \(if (.value | type) == "array" then (.value | join(", ")) else .value end)""#;

/// `FileProperties`: `{path, properties}`
/// List-type values are joined with ", " for readability.
/// Each property line is indented with two spaces for nesting under the file path.
const FILE_PROPERTIES_FILTER: &str = r#""\(.path)\n\(.properties | map("  \(.name) (\(.type)): \(if (.value | type) == "array" then (.value | join(", ")) else .value end)") | join("\n"))""#;

/// `PropertySummaryEntry`: `{count, name, type}`
const PROPERTY_SUMMARY_ENTRY_FILTER: &str =
    r#""\(.name)\t\(.type)\t\(.count) \(if .count == 1 then "file" else "files" end)""#;

/// `PropertyRemoved`: `{path, removed}`
const PROPERTY_REMOVED_FILTER: &str = r#""Removed \"\(.removed)\" from \(.path)""#;

/// `PropertyFindResult` without value: `{files, property, total}`
const PROPERTY_FIND_RESULT_FILTER: &str = r#""\(.property): \(.total) \(if .total == 1 then "file" else "files" end)\(if (.files | length) > 0 then "\n\(.files | map("  \(.)") | join("\n"))" else "" end)""#;

/// `PropertyFindResult` with value: `{files, property, total, value}`
const PROPERTY_FIND_RESULT_WITH_VALUE_FILTER: &str = r#""\(.property)=\(.value): \(.total) \(if .total == 1 then "file" else "files" end)\(if (.files | length) > 0 then "\n\(.files | map("  \(.)") | join("\n"))" else "" end)""#;

/// `PropertyMutationResult`: `{modified, property, skipped, total, values}`
const PROPERTY_MUTATION_RESULT_FILTER: &str = r#""\(.property): \(.modified | length) modified, \(.skipped | length) skipped\(if (.modified | length) > 0 then "\n  modified: \(.modified | join(", "))" else "" end)\(if (.skipped | length) > 0 then "\n  skipped: \(.skipped | join(", "))" else "" end)""#;

/// `FileTags`: `{path, tags}`
const FILE_TAGS_FILTER: &str = r#""\(.path): \(.tags | join(", "))""#;

/// `TagSummary`: `{tags, total}`
const TAG_SUMMARY_FILTER: &str = r#""\(.total) unique \(if .total == 1 then "tag" else "tags" end)\n\(.tags | map("  \(.name)\t\(.count) \(if .count == 1 then "file" else "files" end)") | join("\n"))""#;

/// `TagSummaryEntry`: `{count, name}`
const TAG_SUMMARY_ENTRY_FILTER: &str =
    r#""\(.name)\t\(.count) \(if .count == 1 then "file" else "files" end)""#;

/// `TagFindResult` without value: `{files, tag, total}`
const TAG_FIND_RESULT_FILTER: &str = r#""\(.tag): \(.total) \(if .total == 1 then "file" else "files" end)\(if (.files | length) > 0 then "\n\(.files | map("  \(.)") | join("\n"))" else "" end)""#;

/// `TagMutationResult`: `{modified, skipped, tag, total}`
const TAG_MUTATION_RESULT_FILTER: &str = r#""\(.tag): \(.modified | length) modified, \(.skipped | length) skipped\(if (.modified | length) > 0 then "\n  modified: \(.modified | join(", "))" else "" end)\(if (.skipped | length) > 0 then "\n  skipped: \(.skipped | join(", "))" else "" end)""#;

/// `LinkInfo` — just target: `{target}`
const LINK_INFO_TARGET_FILTER: &str = r#""  \(.target) (unresolved)""#;

/// `LinkInfo` with path: `{path, target}`
const LINK_INFO_PATH_FILTER: &str = r#""  \(.target) → \(.path)""#;

/// `LinkInfo` with label: `{label, target}`
const LINK_INFO_LABEL_FILTER: &str = r#""  \(.target) (unresolved) [\(.label)]""#;

/// `LinkInfo` with path and label: `{label, path, target}`
const LINK_INFO_FULL_FILTER: &str = r#""  \(.target) → \(.path) [\(.label)]""#;

/// `FileLinks`: `{links, path}`
const FILE_LINKS_FILTER: &str = r#""\(.path): \(.links | length) \(if (.links | length) == 1 then "link" else "links" end)\(if (.links | length) > 0 then "\n\(.links | map("  \(.target)\(if .path then " → \(.path)" else " (unresolved)" end)\(if .label then " [\(.label)]" else "" end)") | join("\n"))" else "" end)""#;

/// `TaskCount`: `{done, total}`
const TASK_COUNT_FILTER: &str = r#""[\(.done)/\(.total)]""#;

/// `OutlineSection` without tasks: `{code_blocks, heading, level, line, links}`
const OUTLINE_SECTION_FILTER: &str = r##""\("#" * .level) \(.heading // "(pre-heading)")\(if (.links | length) > 0 then "\n  → \(.links | join(", "))" else "" end)""##;

/// `OutlineSection` with tasks: `{code_blocks, heading, level, line, links, tasks}`
const OUTLINE_SECTION_WITH_TASKS_FILTER: &str = r##""\("#" * .level) \(.heading // "(pre-heading)") [\(.tasks.done)/\(.tasks.total)]\(if (.links | length) > 0 then "\n  → \(.links | join(", "))" else "" end)""##;

/// `FileOutline`: `{file, properties, sections, tags}`
const FILE_OUTLINE_FILTER: &str = r##""\(.file)\(if (.tags | length) > 0 then "\n  tags: \(.tags | join(", "))" else "" end)\(if (.properties | length) > 0 then "\n  props: \(.properties | map("\(.name)=\(if (.value | type) == "array" then "[\(.value | join(", "))]" else .value end)") | join(", "))" else "" end)\n\(.sections | map("\("#" * .level) \(.heading // "(pre-heading)")\(if .tasks then " [\(.tasks.done)/\(.tasks.total)]" else "" end)\(if (.links | length) > 0 then "\n  → \(.links | join(", "))" else "" end)") | join("\n"))""##;

/// `TaskInfo`: `{done, line, status, text}`
const TASK_INFO_FILTER: &str =
    r#""line \(.line): [\(.status)] \(.text)\(if .done then " (done)" else "" end)""#;

/// `FileTasks`: `{file, tasks, total}`
const FILE_TASKS_FILTER: &str = r#""\(.file) (\(.total) \(if .total == 1 then "task" else "tasks" end))\(if (.tasks | length) > 0 then "\n\(.tasks | map("  line \(.line): [\(.status)] \(.text)") | join("\n"))" else "" end)""#;

/// `TaskReadResult`: `{done, file, line, status, text}`
const TASK_READ_RESULT_FILTER: &str =
    r#""\(.file):\(.line) [\(.status)] \(.text)\(if .done then " (done)" else "" end)""#;

/// `VaultSummary`: `{files, properties, recent_files, status, tags, tasks}`
const VAULT_SUMMARY_FILTER: &str = r#""Files: \(.files.total) total\(if (.files.by_directory | length) > 0 then " (\(.files.by_directory | map("\(.directory): \(.count)") | join(", ")))" else "" end)\nProperties: \(.properties | length) unique\nTags: \(.tags.total) unique\nStatus: \(if (.status | length) > 0 then (.status | map("\(.value) (\(.files | length))") | join(", ")) else "(none)" end)\nTasks: \(.tasks.done)/\(.tasks.total)\nRecent: \(if (.recent_files | length) > 0 then (.recent_files | map(.path) | join(", ")) else "(none)" end)""#;

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
        // FileProperties
        "path,properties" => Some(FILE_PROPERTIES_FILTER),
        // PropertySummaryEntry
        "count,name,type" => Some(PROPERTY_SUMMARY_ENTRY_FILTER),
        // PropertyRemoved
        "path,removed" => Some(PROPERTY_REMOVED_FILTER),
        // PropertyFindResult (with and without value)
        "files,property,total" => Some(PROPERTY_FIND_RESULT_FILTER),
        "files,property,total,value" => Some(PROPERTY_FIND_RESULT_WITH_VALUE_FILTER),
        // PropertyMutationResult
        "modified,property,skipped,total,values" => Some(PROPERTY_MUTATION_RESULT_FILTER),
        // FileTags
        "path,tags" => Some(FILE_TAGS_FILTER),
        // TagSummary
        "tags,total" => Some(TAG_SUMMARY_FILTER),
        // TagSummaryEntry
        "count,name" => Some(TAG_SUMMARY_ENTRY_FILTER),
        // TagFindResult
        "files,tag,total" => Some(TAG_FIND_RESULT_FILTER),
        // TagMutationResult
        "modified,skipped,tag,total" => Some(TAG_MUTATION_RESULT_FILTER),
        // LinkInfo variants (optional path and label → 4 combos)
        "target" => Some(LINK_INFO_TARGET_FILTER),
        "path,target" => Some(LINK_INFO_PATH_FILTER),
        "label,target" => Some(LINK_INFO_LABEL_FILTER),
        "label,path,target" => Some(LINK_INFO_FULL_FILTER),
        // FileLinks
        "links,path" => Some(FILE_LINKS_FILTER),
        // TaskCount
        "done,total" => Some(TASK_COUNT_FILTER),
        // OutlineSection (with and without tasks)
        "code_blocks,heading,level,line,links" => Some(OUTLINE_SECTION_FILTER),
        "code_blocks,heading,level,line,links,tasks" => Some(OUTLINE_SECTION_WITH_TASKS_FILTER),
        // FileOutline
        "file,properties,sections,tags" => Some(FILE_OUTLINE_FILTER),
        // TaskInfo
        "done,line,status,text" => Some(TASK_INFO_FILTER),
        // FileTasks
        "file,tasks,total" => Some(FILE_TASKS_FILTER),
        // TaskReadResult
        "done,file,line,status,text" => Some(TASK_READ_RESULT_FILTER),
        // VaultSummary
        "files,properties,recent_files,status,tags,tasks" => Some(VAULT_SUMMARY_FILTER),
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
// Text formatting
// ---------------------------------------------------------------------------

/// Format a JSON value as human-readable text using jq filters where available.
fn format_value_as_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(format_value_as_text)
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Object(map) => {
            let sig = key_signature(map);
            if let Some(filter) = lookup_filter(&sig)
                && let Some(output) = apply_jq_filter(filter, value)
            {
                return output;
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
        .map(|(k, v)| format!("{k}: {}", format_scalar(v)))
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
        serde_json::Value::Object(map) => {
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k}={}", format_scalar(v)))
                .collect();
            items.join(", ")
        }
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
        // Array values should be joined with ", " not rendered as JSON
        assert!(out.contains("rust, cli") || (out.contains("rust") && out.contains("cli")));
        assert!(!out.contains("[\"rust\""));
    }

    #[test]
    fn file_properties_filter() {
        let val = json!({
            "path": "notes/a.md",
            "properties": [
                {"name": "title", "type": "text", "value": "A"},
                {"name": "status", "type": "text", "value": "draft"}
            ]
        });
        let out = apply_jq_filter(FILE_PROPERTIES_FILTER, &val).unwrap();
        assert!(out.contains("notes/a.md"));
        assert!(out.contains("title"));
        assert!(out.contains("status"));
        assert!(out.contains("draft"));
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
    fn property_find_result_filter() {
        let val = json!({"files": ["a.md", "b.md"], "property": "status", "total": 2});
        let out = apply_jq_filter(PROPERTY_FIND_RESULT_FILTER, &val).unwrap();
        assert!(out.contains("status"));
        assert!(out.contains("2 files"));
        assert!(out.contains("a.md"));
        assert!(out.contains("b.md"));
    }

    #[test]
    fn property_find_result_with_value_filter() {
        let val = json!({"files": ["a.md"], "property": "status", "total": 1, "value": "draft"});
        let out = apply_jq_filter(PROPERTY_FIND_RESULT_WITH_VALUE_FILTER, &val).unwrap();
        assert!(out.contains("status=draft"));
        assert!(out.contains("1 file"));
        assert!(!out.contains("1 files"));
    }

    #[test]
    fn property_find_result_with_value_filter_plural() {
        let val =
            json!({"files": ["a.md", "b.md"], "property": "status", "total": 2, "value": "draft"});
        let out = apply_jq_filter(PROPERTY_FIND_RESULT_WITH_VALUE_FILTER, &val).unwrap();
        assert!(out.contains("2 files"));
    }

    #[test]
    fn property_mutation_result_filter() {
        let val = json!({
            "modified": ["a.md"],
            "property": "status",
            "skipped": [],
            "total": 1,
            "values": ["draft"]
        });
        let out = apply_jq_filter(PROPERTY_MUTATION_RESULT_FILTER, &val).unwrap();
        assert!(out.contains("status"));
        assert!(out.contains("1 modified"));
        assert!(out.contains("0 skipped"));
    }

    // --- tag type filters ---

    #[test]
    fn file_tags_filter() {
        let val = json!({"path": "note.md", "tags": ["rust", "cli"]});
        let out = apply_jq_filter(FILE_TAGS_FILTER, &val).unwrap();
        assert!(out.contains("note.md"));
        assert!(out.contains("rust"));
        assert!(out.contains("cli"));
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

    #[test]
    fn tag_find_result_filter() {
        let val = json!({"files": ["a.md", "b.md"], "tag": "rust", "total": 2});
        let out = apply_jq_filter(TAG_FIND_RESULT_FILTER, &val).unwrap();
        assert!(out.contains("rust"));
        assert!(out.contains("2 files"));
        assert!(out.contains("a.md"));
    }

    #[test]
    fn tag_mutation_result_filter() {
        let val = json!({
            "modified": ["a.md", "b.md"],
            "skipped": [],
            "tag": "rust",
            "total": 2
        });
        let out = apply_jq_filter(TAG_MUTATION_RESULT_FILTER, &val).unwrap();
        // Format is now "rust: N modified, N skipped" — no Tag prefix, no quotes
        assert!(out.contains("rust"));
        assert!(!out.contains("Tag "));
        assert!(!out.contains("\"rust\""));
        assert!(out.contains("2 modified"));
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

    #[test]
    fn file_links_filter() {
        let val = json!({
            "links": [
                {"target": "note-b", "path": "note-b.md"},
                {"target": "broken"}
            ],
            "path": "note-a.md"
        });
        let out = apply_jq_filter(FILE_LINKS_FILTER, &val).unwrap();
        assert!(out.contains("note-a.md"));
        assert!(out.contains("2 links"));
        assert!(out.contains("note-b.md"));
        assert!(out.contains("unresolved"));
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

    #[test]
    fn file_outline_filter() {
        let val = json!({
            "file": "note.md",
            "properties": [{"name": "title", "type": "text", "value": "My Note"}],
            "sections": [
                {
                    "code_blocks": [],
                    "heading": "Intro",
                    "level": 1,
                    "line": 5,
                    "links": []
                }
            ],
            "tags": ["rust", "cli"]
        });
        let out = apply_jq_filter(FILE_OUTLINE_FILTER, &val).unwrap();
        assert!(out.contains("note.md"));
        assert!(out.contains("rust"));
        assert!(out.contains("cli"));
        assert!(out.contains("# Intro"));
    }

    // --- format_value_as_text integration ---

    #[test]
    fn format_value_as_text_uses_filter_for_known_shape() {
        let val = json!({"path": "note.md", "tags": ["rust"]});
        let out = format_value_as_text(&val);
        assert!(out.contains("note.md"));
        assert!(out.contains("rust"));
        // Should NOT look like "path: note.md" (that's the generic fallback)
        assert!(!out.contains("path: note.md"));
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
