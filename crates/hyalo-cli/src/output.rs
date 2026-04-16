use std::collections::HashMap;
use std::fmt::Write as _;

use jaq_core::load::{self, Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Native, Vars, data};
use jaq_json::Val;
use serde::Serialize;
use serde_json::json;

// ---------------------------------------------------------------------------
// Filter cache
// ---------------------------------------------------------------------------

/// The `DataT` implementation used for jaq filter compilation and execution.
///
/// `JustLut<Val>` is a minimal wrapper that only provides the compiled lookup
/// table — sufficient because we don't use lifetime-dependent filters like
/// `inputs`.
type D = data::JustLut<Val>;

/// Cache of compiled jaq filters, keyed by filter source string.
///
/// The compiled `Filter` is fully owned (no lifetime parameters) and `Clone`,
/// so it can be stored directly in a `HashMap`. The `Arena` used during
/// `Loader::load` is a temporary scratch pad — once `compile` returns, the
/// `Filter` no longer borrows from it.
type JaqFilterCache = HashMap<String, jaq_core::compile::Filter<Native<D>>>;

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Json,
    Text,
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Format::Json => f.write_str("json"),
            Format::Text => f.write_str("text"),
        }
    }
}

/// Result of a command execution: either success (exit 0) or a user-facing error (exit 1).
/// Internal/unexpected errors are represented by `anyhow::Error` at the call site.
///
/// **Invariant**: `Success.output` must always be a valid JSON string — the pipeline handles
/// format conversion. Commands must never store pre-formatted text here.
///
/// For commands like `read` whose text output is raw file content (not structured data),
/// use `RawOutput` to bypass the JSON pipeline entirely.
#[derive(Debug)]
pub enum CommandOutcome {
    /// Successful operation — JSON output goes to stdout via the pipeline.
    Success {
        /// Always-valid JSON string (bare array, object, etc.). Never pre-formatted text.
        output: String,
        /// Optional total item count for pagination display.
        total: Option<u64>,
    },
    /// Raw text output, bypasses the JSON pipeline — printed directly to stdout as-is.
    /// Used by `read` command for text-format content output.
    RawOutput(String),
    /// User error (file not found, property missing, etc.) — output goes to stderr.
    UserError(String),
}

impl CommandOutcome {
    /// Construct a successful outcome carrying a JSON string with no total count.
    #[must_use]
    pub fn success(output: String) -> Self {
        Self::Success {
            output,
            total: None,
        }
    }

    /// Construct a successful outcome carrying a JSON string with a total item count.
    #[must_use]
    pub fn success_with_total(output: String, total: u64) -> Self {
        Self::Success {
            output,
            total: Some(total),
        }
    }

    /// Extract the output string from `Success` or `RawOutput`, or panic.
    ///
    /// Intended for use in unit tests where the command is expected to succeed.
    #[cfg(test)]
    #[must_use]
    pub fn unwrap_output(self) -> String {
        match self {
            Self::Success { output, .. } | Self::RawOutput(output) => output,
            Self::UserError(msg) => panic!("expected success, got UserError: {msg}"),
        }
    }
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

/// Strip control characters that could inject terminal escape sequences.
///
/// Removes bytes 0x00-0x08, 0x0B-0x0C, 0x0E-0x1F, 0x7F, and 0x9B-0x9F
/// (C0/C1 control codes minus `\n` (0x0A) and `\t` (0x09)).
fn sanitize_control_chars(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            // Keep printable chars, newline, and tab
            !c.is_control() || c == '\n' || c == '\t'
        })
        .collect()
}

/// Format a successful JSON value for output.
#[must_use]
pub fn format_success(format: Format, value: &serde_json::Value) -> String {
    match format {
        Format::Json => serde_json::to_string_pretty(value)
            .expect("serializing serde_json::Value is infallible"),
        Format::Text => {
            let mut cache = JaqFilterCache::new();
            sanitize_control_chars(&format_value_as_text(value, &mut cache))
        }
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

/// Build the JSON envelope value: `{"results": ..., "total": <optional>, "hints": [...]}`.
///
/// The envelope is always present even when hints is empty (hints becomes `[]`).
/// `total` is included only when `Some`.
#[must_use]
pub fn build_envelope_value(
    value: &serde_json::Value,
    total: Option<u64>,
    hints: &[crate::hints::Hint],
) -> serde_json::Value {
    let hints_json: Vec<serde_json::Value> = hints
        .iter()
        .map(|h| serde_json::json!({"description": &h.description, "cmd": &h.cmd}))
        .collect();
    let mut envelope = serde_json::json!({
        "results": value,
        "hints": hints_json,
    });
    if let Some(t) = total {
        envelope["total"] = serde_json::json!(t);
    }
    envelope
}

/// Format the output envelope for the user.
///
/// - **JSON**: serializes `{"results": ..., "total": <optional>, "hints": [...]}`
/// - **Text**: formats `results` as text, appends hint lines if any, adds pagination notice if needed
#[must_use]
pub fn format_envelope(
    format: Format,
    value: &serde_json::Value,
    total: Option<u64>,
    hints: &[crate::hints::Hint],
) -> String {
    match format {
        Format::Json => {
            let envelope = build_envelope_value(value, total, hints);
            serde_json::to_string_pretty(&envelope)
                .expect("serializing serde_json::Value is infallible")
        }
        Format::Text => {
            let mut cache = JaqFilterCache::new();
            let mut text = format_results_as_text(value, total, &mut cache);
            if !hints.is_empty() {
                text.push('\n');
                for hint in hints {
                    text.push_str("\n  -> ");
                    text.push_str(&hint.cmd);
                    text.push_str("  # ");
                    text.push_str(&hint.description);
                }
            }
            sanitize_control_chars(&text)
        }
    }
}

/// Format results for text output, applying pagination notice and tag-summary header.
///
/// Called by [`format_envelope`] when producing text output. The `total` is the
/// count stored in the envelope (may exceed the number of items in `results`).
fn format_results_as_text(
    results: &serde_json::Value,
    total: Option<u64>,
    cache: &mut JaqFilterCache,
) -> String {
    // Special case: array of tag summary entries ({count, name}) — reconstruct
    // the "N unique tags" header that was previously part of the TAG_SUMMARY_FILTER.
    if let (Some(total), serde_json::Value::Array(arr)) = (total, results) {
        let is_tag_array = !arr.is_empty()
            && arr.iter().all(|v| {
                v.as_object().is_some_and(|m| {
                    m.contains_key("count") && m.contains_key("name") && m.len() == 2
                })
            });
        if is_tag_array {
            let tag_label = if total == 1 { "tag" } else { "tags" };
            let header = format!("{total} unique {tag_label}");
            let entries = format_value_as_text(results, cache);
            return if entries.is_empty() {
                header
            } else {
                format!("{header}\n{entries}")
            };
        }
    }

    let text = format_value_as_text(results, cache);
    if let Some(total) = total {
        let shown = match results {
            serde_json::Value::Array(arr) => arr.len() as u64,
            _ => return text,
        };
        if shown < total {
            return format!("{text}\nshowing {shown} of {total} matches");
        }
    }
    text
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
            serde_json::to_string_pretty(&obj).expect("serializing serde_json::Value is infallible")
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

/// `PropertyInfo` (used by `--fields properties-typed`): `{name, type, value}`
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
/// Format: `  "target" (unresolved)`
const LINK_INFO_TARGET_FILTER: &str = r#""  \"\(.target)\" (unresolved)""#;

/// `LinkInfo` with path: `{path, target}`
/// Format: `  "target" → "path"`
const LINK_INFO_PATH_FILTER: &str = r#""  \"\(.target)\" → \"\(.path)\"""#;

/// `LinkInfo` with label: `{label, target}`
/// Format: `  "target" (unresolved) [label]`
const LINK_INFO_LABEL_FILTER: &str = r#""  \"\(.target)\" (unresolved) [\(.label)]""#;

/// `LinkInfo` with path and label: `{label, path, target}`
/// Format: `  "target" → "path" [label]`
const LINK_INFO_FULL_FILTER: &str = r#""  \"\(.target)\" → \"\(.path)\" [\(.label)]""#;

/// `TaskCount`: `{done, total}`
const TASK_COUNT_FILTER: &str = r#""[\(.done)/\(.total)]""#;

/// `OutlineSection` without tasks: `{code_blocks, heading, level, line, links}`
const OUTLINE_SECTION_FILTER: &str = r##""\("#" * .level) \(.heading // "(pre-heading)")\(if (.links | length) > 0 then "\n\(.links | map("  → \"\(.)\"") | join("\n"))" else "" end)""##;

/// `OutlineSection` with tasks: `{code_blocks, heading, level, line, links, tasks}`
const OUTLINE_SECTION_WITH_TASKS_FILTER: &str = r##""\("#" * .level) \(.heading // "(pre-heading)") [\(.tasks.done)/\(.tasks.total)]\(if (.links | length) > 0 then "\n\(.links | map("  → \"\(.)\"") | join("\n"))" else "" end)""##;

/// `TaskInfo`: `{done, line, status, text}`
const TASK_INFO_FILTER: &str =
    r#""line \(.line): [\(.status)] \(.text)\(if .done then " (done)" else "" end)""#;

/// `TaskReadResult`: `{done, file, line, status, text}`
const TASK_READ_RESULT_FILTER: &str =
    r#""\"\(.file)\":\(.line) [\(.status)] \(.text)\(if .done then " (done)" else "" end)""#;

/// `TaskDryRunResult`: `{done, file, line, old_status, status, text}`
/// Format: `"file":line [old] -> [new] text` — makes the direction of change
/// explicit for `task toggle --dry-run`.
const TASK_DRY_RUN_RESULT_FILTER: &str =
    r#""\"\(.file)\":\(.line) [\(.old_status)] -> [\(.status)] \(.text)""#;

/// `VaultSummary`: `{dead_ends, files, links, orphans, properties, recent_files, status, tags, tasks}`
/// Compact single-line-per-section format (~20-30 lines regardless of vault size).
const VAULT_SUMMARY_FILTER: &str = r#""Files: \(.files.total)\nDirectories: \(if (.files.directories | length) > 0 then (.files.directories | .[:7] | map("\(.directory)/ (\(.count))") | join(", ")) + (if (.files.directories | length) > 7 then ", ..." else "" end) else "(none)" end)\nProperties: \(.properties | length) — \(if (.properties | length) > 0 then (.properties | sort_by(-.count) | .[:7] | map("\(.name) (\(.count))") | join(", ")) + (if (.properties | length) > 7 then ", ..." else "" end) else "(none)" end)\nTags: \(.tags.total) — \(if (.tags.tags | length) > 0 then (.tags.tags | .[:7] | map("\(.name) (\(.count))") | join(", ")) + (if (.tags.tags | length) > 7 then ", ..." else "" end) else "(none)" end)\nTasks: \(.tasks.done)/\(.tasks.total)\nLinks: \(.links.total) total, \(.links.broken) broken\nOrphans: \(.orphans)\nDead-ends: \(.dead_ends)\nStatus: \(if (.status | length) > 0 then (.status | sort_by(-.count) | map("\(.value) (\(.count))") | join(", ")) else "(none)" end)\nRecent: \(if (.recent_files | length) > 0 then (.recent_files | map(.path) | join(", ")) else "(none)" end)""#;

/// `FindTaskInfo`: `{done, line, section, status, text}`
/// Format: `  [x] text (line N, section)` or `  [ ] text (line N, section)`
const FIND_TASK_INFO_FILTER: &str =
    r#""  [\(if .done then "x" else " " end)] \(.text) (line \(.line), \(.section))""#;

/// `ContentMatch`: `{line, section, text}`
/// Format: `  line N (section): text`
const CONTENT_MATCH_FILTER: &str = r#""  line \(.line) (\(.section)): \(.text)""#;

/// Mutation result with `property` + `value` fields:
/// covers `SetPropertyResult`, `AppendPropertyResult`, and `RemovePropertyResult` (with value).
/// Key signature: `dry_run,modified,property,scanned,skipped,total,value`
/// Format: `[dry-run] property=value: N/T modified (S scanned)` when dry-run; omits prefix otherwise.
/// Appends `(S scanned)` when not all scanned files were processed (e.g. where-filters).
const PROPERTY_VALUE_MUTATION_FILTER: &str = r#""\(if .dry_run then "[dry-run] " else "" end)\(.property)=\(.value): \(.modified | length)/\(.total) modified\(if .scanned != .total then " (\(.scanned) scanned)" else "" end)\(if (.modified | length) > 0 then "\n\(.modified | map("  \"\(.)\"") | join("\n"))" else "" end)""#;

/// Mutation result with `property` only (no value field):
/// covers `RemovePropertyResult` (without value).
/// Key signature: `dry_run,modified,property,scanned,skipped,total`
/// Format: `[dry-run] property: N/T modified (S scanned)` when dry-run; omits prefix otherwise.
/// Appends `(S scanned)` when not all scanned files were processed (e.g. where-filters).
const PROPERTY_MUTATION_FILTER: &str = r#""\(if .dry_run then "[dry-run] " else "" end)\(.property): \(.modified | length)/\(.total) modified\(if .scanned != .total then " (\(.scanned) scanned)" else "" end)\(if (.modified | length) > 0 then "\n\(.modified | map("  \"\(.)\"") | join("\n"))" else "" end)""#;

/// Mutation result with `tag` field:
/// covers `SetTagResult` and `RemoveTagResult`.
/// Key signature: `dry_run,modified,scanned,skipped,tag,total`
/// Format: `[dry-run] tag: N/T modified (S scanned)` when dry-run; omits prefix otherwise.
/// Appends `(S scanned)` when not all scanned files were processed (e.g. where-filters).
const TAG_MUTATION_FILTER: &str = r#""\(if .dry_run then "[dry-run] " else "" end)\(.tag): \(.modified | length)/\(.total) modified\(if .scanned != .total then " (\(.scanned) scanned)" else "" end)\(if (.modified | length) > 0 then "\n\(.modified | map("  \"\(.)\"") | join("\n"))" else "" end)""#;

/// `BacklinksResult`: `{file, backlinks: [...]}`
/// Format: `N backlink(s) for "file"` with each backlink listed as `  source.md: line N`.
/// Empty case: `No backlinks found for "file"`.
const BACKLINKS_RESULT_FILTER: &str = r#"if (.backlinks | length) == 0 then "No backlinks found for \"\(.file)\"" else "\(.backlinks | length) \(if (.backlinks | length) == 1 then "backlink" else "backlinks" end) for \"\(.file)\"\n\(.backlinks | map("  \(.source): line \(.line)") | join("\n"))" end"#;

/// `LinksFix result`: `{applied, broken, case_mismatch_fixes, case_mismatches, fixable, fixes, ignored, unfixable, unfixable_links}`
/// Format: summary line with fix status. Includes case-mismatch count when non-zero.
const LINKS_FIX_FILTER: &str = r#""Broken links: \(.broken)\nFixable: \(.fixable)\nUnfixable: \(.unfixable)\nIgnored: \(.ignored)\(if .case_mismatches > 0 then "\nCase mismatches: \(.case_mismatches)" else "" end)\nApplied: \(if .applied then "yes" else "no" end)\(if (.fixes | length) > 0 then "\n\(.fixes | map("  \(.source) line \(.line): \"\(.old_target)\" → \"\(.new_target)\"") | join("\n"))" else "" end)\(if (.case_mismatch_fixes | length) > 0 then "\nCase-mismatch fixes:\n\(.case_mismatch_fixes | map("  \(.source) line \(.line): \"\(.old_target)\" → \"\(.new_target)\" [link-case-mismatch]") | join("\n"))" else "" end)""#;

/// `MvResult`: `{dry_run, from, to, total_files_updated, total_links_updated, updated_files}`
/// Format: `[dry-run] Moved <from> → <to>` with list of updated files and replacements.
const MV_RESULT_FILTER: &str = r#""\(if .dry_run then "[dry-run] " else "" end)Moved \(.from) → \(.to)\(.updated_files | if length > 0 then "\n" + (map("  \(.file): " + (.replacements | map(.old_text + " → " + .new_text) | join(", "))) | join("\n")) else "" end)""#;

/// `ViewsListEntry`: `{filters, name}`
/// Format: `name  key=value key=value ...` — compact one-line summary of the view and its filters.
const VIEWS_LIST_ENTRY_FILTER: &str = r#""\(.name)\t\(.filters | to_entries | map("\(.key)=\(.value | if type == "array" then join(",") else tostring end)") | join(" "))""#;

/// `ViewsMutationResult`: `{action, name}`
/// Format: `action: name`
const VIEWS_MUTATION_RESULT_FILTER: &str = r#""\(.action): \(.name)""#;

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
        // TaskDryRunResult
        "done,file,line,old_status,status,text" => Some(TASK_DRY_RUN_RESULT_FILTER),
        // VaultSummary
        "dead_ends,files,links,orphans,properties,recent_files,status,tags,tasks"
        | "dead_ends,files,links,orphans,properties,recent_files,schema,status,tags,tasks" => {
            Some(VAULT_SUMMARY_FILTER)
        }
        // Mutation results with property + value (SetPropertyResult, AppendPropertyResult,
        // RemovePropertyResult with value)
        "dry_run,modified,property,scanned,skipped,total,value" => {
            Some(PROPERTY_VALUE_MUTATION_FILTER)
        }
        // Mutation results with property only (RemovePropertyResult without value)
        "dry_run,modified,property,scanned,skipped,total" => Some(PROPERTY_MUTATION_FILTER),
        // Mutation results with tag (SetTagResult, RemoveTagResult)
        "dry_run,modified,scanned,skipped,tag,total" => Some(TAG_MUTATION_FILTER),
        // BacklinksResult
        "backlinks,file" => Some(BACKLINKS_RESULT_FILTER),
        // LinksFix result
        "applied,broken,case_mismatch_fixes,case_mismatches,fixable,fixes,ignored,unfixable,unfixable_links" => {
            Some(LINKS_FIX_FILTER)
        }
        // MvResult
        "dry_run,from,to,total_files_updated,total_links_updated,updated_files" => {
            Some(MV_RESULT_FILTER)
        }
        // ViewsListEntry
        "filters,name" => Some(VIEWS_LIST_ENTRY_FILTER),
        // ViewsMutationResult
        "action,name" => Some(VIEWS_MUTATION_RESULT_FILTER),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// jq filter execution engine
// ---------------------------------------------------------------------------

/// Apply a jq filter string to a `serde_json::Value` and return the text output.
///
/// Looks up or compiles the filter in `cache`. Multiple outputs are joined with
/// newlines. On any error (parse or runtime), returns `None` (used internally
/// by the text formatter, which has its own fallbacks).
fn apply_jq_filter(
    filter_code: &str,
    value: &serde_json::Value,
    cache: &mut JaqFilterCache,
) -> Option<String> {
    run_jq_filter_cached(filter_code, value, cache).ok()
}

/// Apply a user-supplied jq filter to a `serde_json::Value`.
///
/// Compiles the filter on every call. For repeated use across many values,
/// prefer the cached path via [`format_success`] / [`format_value_as_text`].
///
/// Returns `Ok(String)` with newline-joined output values on success, or
/// `Err(String)` with a human-readable description of the parse or runtime error.
pub fn apply_jq_filter_result(
    filter_code: &str,
    value: &serde_json::Value,
) -> Result<String, String> {
    let filter = compile_jq_filter(filter_code)?;
    execute_jq_filter(&filter, value)
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

/// Compile a jq filter string into a reusable `Filter`.
///
/// The `Arena` used during loading is a temporary scratch pad and is dropped
/// after this function returns — the compiled `Filter` owns all its data.
fn compile_jq_filter(filter_code: &str) -> Result<jaq_core::compile::Filter<Native<D>>, String> {
    let program = File {
        code: filter_code,
        path: (),
    };
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let loader = Loader::new(defs);
    let arena = Arena::default();

    let modules = loader
        .load(&arena, program)
        .map_err(|errs| format_load_errors(&errs))?;

    let funs = jaq_core::funs::<D>()
        .chain(jaq_std::funs::<D>())
        .chain(jaq_json::funs::<D>());
    Compiler::default()
        .with_funs(funs)
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
        })
}

/// Maximum total output size for a jq filter to prevent pathological filters
/// from causing unbounded memory growth (e.g. exponential-expansion patterns).
const JQ_OUTPUT_CAP: usize = 10 * 1024 * 1024; // 10 MiB

/// Execute a pre-compiled jq filter against a JSON value and return the text output.
fn execute_jq_filter(
    filter: &jaq_core::compile::Filter<Native<D>>,
    value: &serde_json::Value,
) -> Result<String, String> {
    let input: Val = serde_json::from_value(value.clone())
        .map_err(|e| format!("jq input conversion error: {e}"))?;
    let ctx = Ctx::<D>::new(&filter.lut, Vars::new([]));

    let mut parts = Vec::new();
    let mut total_len: usize = 0;
    for result in filter.id.run((ctx, input)).map(jaq_core::unwrap_valr) {
        match result {
            Ok(val) => {
                let s = match val {
                    Val::TStr(ref s) | Val::BStr(ref s) => match std::str::from_utf8(s) {
                        Ok(valid) => valid.to_owned(),
                        Err(_) => String::from_utf8_lossy(s).into_owned(),
                    },
                    // For non-string values, `Display` produces valid JSON
                    // (numbers, booleans, null, arrays, objects).
                    other => other.to_string(),
                };
                total_len += s.len();
                if total_len > JQ_OUTPUT_CAP {
                    return Err(format!(
                        "jq filter output exceeds {} MiB limit",
                        JQ_OUTPUT_CAP / (1024 * 1024)
                    ));
                }
                parts.push(s);
            }
            Err(e) => return Err(format!("jq runtime error: {e}")),
        }
    }

    Ok(parts.join("\n"))
}

/// Look up or compile a jq filter from `cache`, then execute it against `value`.
fn run_jq_filter_cached(
    filter_code: &str,
    value: &serde_json::Value,
    cache: &mut JaqFilterCache,
) -> Result<String, String> {
    if let Some(filter) = cache.get(filter_code) {
        return execute_jq_filter(filter, value);
    }
    let compiled = compile_jq_filter(filter_code)?;
    let filter = cache.entry(filter_code.to_owned()).or_insert(compiled);
    execute_jq_filter(filter, value)
}

// ---------------------------------------------------------------------------
// FileObject dynamic filter builder
// ---------------------------------------------------------------------------

/// Build a jaq filter string for a `FileObject` by inspecting which optional
/// fields are present in the JSON object.
///
/// The file header is always emitted. Each optional section (properties, tags,
/// sections, tasks, matches, links) is included only when the key is present.
///
/// **How it works:** Each part is a jaq expression that either emits a string or
/// `empty` (when the field is absent/empty). Parts are joined with `, ` — jaq's
/// alternation operator — so the filter produces one output per present section.
/// `run_jq_filter` then joins those outputs with `"\n"`, producing the final
/// multi-line text block. This coupling is intentional: changing the separator
/// in `run_jq_filter` would affect `FileObject` rendering.
fn build_file_object_filter(map: &serde_json::Map<String, serde_json::Value>) -> String {
    // Header: file path and modified timestamp — always present.
    let mut parts = vec![r#""\"\(.file)\"  (\(.modified))""#.to_owned()];

    // Title: "  title: <value>" or "  title: (none)"
    if map.contains_key("title") {
        parts.push(r#""  title: \(if .title != null then .title else "(none)" end)""#.to_owned());
    }

    // Properties: header then each as "    key: value"
    if map.contains_key("properties") {
        parts.push(
            r#"if (.properties | length) > 0 then "  properties:\n\(.properties | to_entries | map("    \(.key): \(if (.value | type) == "array" then "[" + (.value | map(tostring) | join(", ")) + "]" else .value end)") | join("\n"))" else empty end"#.to_owned(),
        );
    }

    // Properties (typed): header then each as "    name (type): value"
    if map.contains_key("properties_typed") {
        parts.push(
            r#"if (.properties_typed | length) > 0 then "  properties_typed:\n\(.properties_typed | map("    \(.name) (\(.type)): \(if (.value | type) == "array" then "[" + (.value | map(tostring) | join(", ")) + "]" else .value end)") | join("\n"))" else empty end"#.to_owned(),
        );
    }

    // Tags: "  tags: [tag1, tag2, ...]"
    if map.contains_key("tags") {
        parts.push(
            r#"if (.tags | length) > 0 then "  tags: [\(.tags | join(", "))]" else empty end"#
                .to_owned(),
        );
    }

    // Sections: header then each as "    ## Heading [done/total]" or "    ## Heading"
    // Note: uses r##"..."## because the jq filter contains the sequence "#" (hash-quoted).
    if map.contains_key("sections") {
        parts.push(
            r##"if (.sections | length) > 0 then "  sections:\n\(.sections | map("    \("#" * .level) \(.heading // "(pre-heading)")\(if .tasks then " [\(.tasks.done)/\(.tasks.total)]" else "" end)") | join("\n"))" else empty end"##.to_owned(),
        );
    }

    // Tasks: header then each as "    [x] text (line N)"
    if map.contains_key("tasks") {
        parts.push(
            r#"if (.tasks | length) > 0 then "  tasks:\n\(.tasks | map("    [\(if .done then "x" else " " end)] \(.text) (line \(.line))") | join("\n"))" else empty end"#.to_owned(),
        );
    }

    // Matches: header then each as "    line N (section): text"
    if map.contains_key("matches") {
        parts.push(
            r#"if (.matches | length) > 0 then "  matches:\n\(.matches | map("    line \(.line) (\(.section)): \(.text)") | join("\n"))" else empty end"#.to_owned(),
        );
    }

    // Score: "  score: <value>" — BM25 relevance score when pattern search was used
    if map.contains_key("score") {
        parts.push(r#""  score: \(.score)""#.to_owned());
    }

    // Links: header then each as "    \"target\" → \"path\"" or "    \"target\" (unresolved)"
    if map.contains_key("links") {
        parts.push(
            r#"if (.links | length) > 0 then "  links:\n\(.links | map("    \"\(.target)\"\(if .path then " → \"\(.path)\"" else " (unresolved)" end)") | join("\n"))" else empty end"#.to_owned(),
        );
    }

    // Backlinks: header then each as "    \"source\" line N" or "    \"source\" line N: label"
    if map.contains_key("backlinks") {
        parts.push(
            r#"if (.backlinks | length) > 0 then "  backlinks:\n\(.backlinks | map("    \"\(.source)\" line \(.line)\(if .label then ": \(.label)" else "" end)") | join("\n"))" else empty end"#.to_owned(),
        );
    }

    parts.join(", ")
}

// ---------------------------------------------------------------------------
// Text formatting
// ---------------------------------------------------------------------------

/// Format a JSON value as human-readable text using jq filters where available.
fn format_value_as_text(value: &serde_json::Value, cache: &mut JaqFilterCache) -> String {
    match value {
        serde_json::Value::Array(arr) => {
            // TypeList: array of type list entries — use custom formatter with blank-line separation.
            let is_type_list = arr.first().and_then(|v| v.as_object()).is_some_and(|m| {
                key_signature(m) == "has_filename_template,property_count,required,type"
            });
            if is_type_list {
                return arr
                    .iter()
                    .filter_map(|v| v.as_object())
                    .map(format_type_list_entry_text)
                    .collect::<Vec<_>>()
                    .join("\n\n");
            }
            // Use blank-line separator between FileObjects for readability.
            let is_file_objects = arr
                .first()
                .and_then(|v| v.as_object())
                .is_some_and(|m| m.contains_key("file") && m.contains_key("modified"));
            let sep = if is_file_objects { "\n\n" } else { "\n" };
            arr.iter()
                .map(|v| format_value_as_text(v, cache))
                .collect::<Vec<_>>()
                .join(sep)
        }
        serde_json::Value::Object(map) => {
            let sig = key_signature(map);
            if let Some(filter) = lookup_filter(&sig)
                && let Some(output) = apply_jq_filter(filter, value, cache)
            {
                return output;
            }
            // TypeShow: detected by presence of "properties" object + "required" array + "type" string.
            if sig == "defaults,filename_template,properties,required,type" {
                return format_type_show_text(map);
            }
            // LintOutput: detected by "files" array of {file, violations} + "total".
            if map.contains_key("total")
                && map.contains_key("files")
                && let Some(serde_json::Value::Array(arr)) = map.get("files")
            {
                let is_lint = arr
                    .first()
                    .and_then(|v| v.as_object())
                    .is_some_and(|m| m.contains_key("file") && m.contains_key("violations"))
                    || arr.is_empty();
                if is_lint {
                    return format_lint_output_text(map);
                }
            }
            // FileObject: dynamically compose filter from present fields.
            if map.contains_key("file") && map.contains_key("modified") {
                let filter = build_file_object_filter(map);
                if let Some(output) = apply_jq_filter(&filter, value, cache) {
                    return output;
                }
            }
            // Fallback: generic key: value lines
            format_object_generic(map, cache)
        }
        other => format_scalar(other, cache),
    }
}

/// Format `LintOutput` JSON as human-readable text.
///
/// Reproduces the format previously generated by `commands::lint::format_text_output`.
fn format_lint_output_text(map: &serde_json::Map<String, serde_json::Value>) -> String {
    use std::fmt::Write as _;

    let mut s = String::new();
    let dry_run = map
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // Fix actions (shown first).
    if let Some(fixes_arr) = map.get("fixes").and_then(|f| f.as_array()) {
        let verb = if dry_run { "Would fix" } else { "Fixed" };
        for file_fix in fixes_arr {
            let file = file_fix
                .get("file")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let actions = file_fix.get("actions").and_then(|a| a.as_array());
            let Some(actions) = actions else { continue };
            if actions.is_empty() {
                continue;
            }
            let _ = writeln!(s, "{verb} {file}:");
            for a in actions {
                let kind = a
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let property = a
                    .get("property")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("?");
                let new = a
                    .get("new")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let old = a.get("old").and_then(serde_json::Value::as_str);
                match (kind, old) {
                    ("insert-default", _) => {
                        let _ = writeln!(s, "  insert  {property} = {new:?}");
                    }
                    ("infer-type", _) => {
                        let _ = writeln!(s, "  infer   type = {new:?}");
                    }
                    ("fix-enum-typo", Some(old_v)) => {
                        let _ = writeln!(s, "  enum    {property}: {old_v:?} -> {new:?}");
                    }
                    ("normalize-date", Some(old_v)) => {
                        let _ = writeln!(s, "  date    {property}: {old_v:?} -> {new:?}");
                    }
                    _ => {
                        let _ = writeln!(s, "  {kind}  {property} = {new:?}");
                    }
                }
            }
        }
    }

    // File violations.
    let files = map.get("files").and_then(|f| f.as_array());
    if let Some(files) = files {
        for file_entry in files {
            let file = file_entry
                .get("file")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let violations = file_entry.get("violations").and_then(|v| v.as_array());
            let Some(violations) = violations else {
                continue;
            };
            if violations.is_empty() {
                continue;
            }
            let _ = writeln!(s, "{file}:");
            for v in violations {
                let severity = v
                    .get("severity")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("warn");
                let message = v
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let pad = if severity == "error" {
                    "error"
                } else {
                    "warn "
                };
                let _ = writeln!(s, "  {pad}  {message}");
            }
        }
    }

    let error_count: u64 = map
        .get("errors")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let warn_count: u64 = map
        .get("warnings")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let files_with_issues: u64 = map
        .get("files_with_issues")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let limited = map
        .get("limited")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let shown_files = map
        .get("files")
        .and_then(|f| f.as_array())
        .map_or(0, |arr| {
            arr.iter()
                .filter(|e| {
                    e.get("violations")
                        .and_then(|v| v.as_array())
                        .is_some_and(|v| !v.is_empty())
                })
                .count()
        });

    if limited {
        let _ = writeln!(
            s,
            "… (showing {shown_files} of {files_with_issues} files with issues)"
        );
    }

    // Summary line.
    let files_checked: u64 = map
        .get("files_checked")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let files_label = if files_checked == 1 { "file" } else { "files" };
    if error_count == 0 && warn_count == 0 {
        let _ = write!(s, "{files_checked} {files_label} checked, no issues");
    } else {
        let _ = write!(
            s,
            "{files_checked} {files_label} checked, {files_with_issues} with issues ({error_count} errors, {warn_count} warnings)",
        );
    }

    let fix_count: usize = map
        .get("fixes")
        .and_then(|f| f.as_array())
        .map_or(0, |arr| {
            arr.iter()
                .filter_map(|f| f.get("actions").and_then(|a| a.as_array()).map(Vec::len))
                .sum()
        });
    if fix_count > 0 {
        let fixed_label = if dry_run { "would fix" } else { "fixed" };
        let _ = write!(s, " — {fixed_label} {fix_count}");
    }

    s
}

/// Format a `types show` result as human-readable text.
///
/// Expected JSON shape: `{type, required, filename_template, defaults, properties}`.
/// Output example:
/// ```text
/// Type: iteration
///
/// Required: title, type, date
///
/// Properties:
///   branch:
///     type: string
///     pattern: ^iter-\d+/
///
///   date:
///     type: date
///
/// Filename template: iteration-{N}-{slug}.md
/// ```
fn format_type_show_text(map: &serde_json::Map<String, serde_json::Value>) -> String {
    use std::fmt::Write as _;

    let mut s = String::new();

    let type_name = map
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?");
    let _ = write!(s, "Type: {type_name}");

    // Required fields.
    if let Some(serde_json::Value::Array(req)) = map.get("required")
        && !req.is_empty()
    {
        let list: Vec<&str> = req.iter().filter_map(serde_json::Value::as_str).collect();
        let _ = write!(s, "\n\nRequired: {}", list.join(", "));
    }

    // Defaults block.
    if let Some(serde_json::Value::Object(defaults)) = map.get("defaults")
        && !defaults.is_empty()
    {
        let _ = write!(s, "\n\nDefaults:");
        let mut keys: Vec<&str> = defaults.keys().map(String::as_str).collect();
        keys.sort_unstable();
        for key in keys {
            if let Some(value) = defaults.get(key) {
                let display = match value {
                    serde_json::Value::String(sv) => sv.clone(),
                    other => other.to_string(),
                };
                let _ = write!(s, "\n  {key}: {display}");
            }
        }
    }

    // Properties block.
    if let Some(serde_json::Value::Object(props)) = map.get("properties")
        && !props.is_empty()
    {
        let _ = write!(s, "\n\nProperties:");
        let mut prop_names: Vec<&str> = props.keys().map(String::as_str).collect();
        prop_names.sort_unstable();
        for name in prop_names {
            let Some(prop_val) = props.get(name) else {
                continue;
            };
            let _ = write!(s, "\n  {name}:");
            if let Some(obj) = prop_val.as_object() {
                // Print each constraint key on its own indented line.
                // Always show "type" first, then remaining keys sorted.
                let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
                keys.sort_unstable_by(|a, b| {
                    if *a == "type" {
                        std::cmp::Ordering::Less
                    } else if *b == "type" {
                        std::cmp::Ordering::Greater
                    } else {
                        a.cmp(b)
                    }
                });
                for key in keys {
                    if let Some(v) = obj.get(key) {
                        let display = match v {
                            serde_json::Value::Array(arr) => arr
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .collect::<Vec<_>>()
                                .join(", "),
                            serde_json::Value::String(sv) => sv.clone(),
                            other => other.to_string(),
                        };
                        let _ = write!(s, "\n    {key}: {display}");
                    }
                }
            }
            s.push('\n'); // blank line between property blocks
        }
    }

    // Optional filename template.
    if let Some(serde_json::Value::String(tmpl)) = map.get("filename_template") {
        let _ = write!(s, "\nFilename template: {tmpl}");
    }

    s
}

/// Format a single `types list` entry as human-readable text.
///
/// Expected JSON shape: `{type, required, property_count, has_filename_template}`.
/// Output example:
/// ```text
/// iteration (4 required, 6 properties)
///   required: title, type, date, tags
/// ```
///
/// Note: `has_filename_template` is a boolean; the actual template is only in `types show`.
/// When present, a hint to run `types show` is appended.
fn format_type_list_entry_text(map: &serde_json::Map<String, serde_json::Value>) -> String {
    use std::fmt::Write as _;

    let mut s = String::new();

    let type_name = map
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?");

    let req_arr: &[serde_json::Value] = map
        .get("required")
        .and_then(serde_json::Value::as_array)
        .map_or(&[], Vec::as_slice);
    let req_count = req_arr.len();

    let prop_count = map
        .get("property_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let has_filename = map
        .get("has_filename_template")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let prop_label = if prop_count == 1 {
        "property"
    } else {
        "properties"
    };
    let _ = write!(
        s,
        "{type_name} ({prop_count} {prop_label}, {req_count} required)"
    );

    if !req_arr.is_empty() {
        let list: Vec<&str> = req_arr
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        let _ = write!(s, "\n  required: {}", list.join(", "));
    }

    if has_filename {
        let _ = write!(s, "\n  filename: (see type details)");
    }

    s
}

/// Generic key: value rendering for unknown object shapes.
fn format_object_generic(
    map: &serde_json::Map<String, serde_json::Value>,
    cache: &mut JaqFilterCache,
) -> String {
    map.iter()
        .map(|(k, v)| format!("{k}: {}", format_value_as_text(v, cache)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format a scalar JSON value as text.
fn format_scalar(value: &serde_json::Value, cache: &mut JaqFilterCache) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_owned(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(|v| format_scalar(v, cache)).collect();
            items.join(", ")
        }
        serde_json::Value::Object(_) => format_value_as_text(value, cache),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Convenience wrappers so individual tests don't have to construct a cache.
    fn jq(filter: &str, val: &serde_json::Value) -> Option<String> {
        apply_jq_filter(filter, val, &mut JaqFilterCache::new())
    }

    fn fmt(val: &serde_json::Value) -> String {
        format_value_as_text(val, &mut JaqFilterCache::new())
    }

    fn scalar(val: &serde_json::Value) -> String {
        format_scalar(val, &mut JaqFilterCache::new())
    }

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
        let result = jq(r#""\(.name): \(.count)""#, &val);
        assert_eq!(result.as_deref(), Some("hello: 3"));
    }

    #[test]
    fn apply_jq_filter_array_map() {
        let val = json!(["a", "b", "c"]);
        let result = jq(".[]", &val);
        assert_eq!(result.as_deref(), Some("a\nb\nc"));
    }

    #[test]
    fn apply_jq_filter_invalid_returns_none() {
        let val = json!({"x": 1});
        let result = jq("this is not valid jq %%%", &val);
        assert!(result.is_none());
    }

    // --- jq output size cap ---

    #[test]
    fn jq_output_cap_constant_is_10_mib() {
        assert_eq!(JQ_OUTPUT_CAP, 10 * 1024 * 1024);
    }

    #[test]
    fn jq_output_within_cap_succeeds() {
        // A small output must pass through without hitting the cap.
        let val = json!({"msg": "hello"});
        let result = apply_jq_filter_result(".msg", &val);
        assert_eq!(result.as_deref(), Ok("hello"));
    }

    #[test]
    fn jq_output_cap_triggers_on_large_output() {
        // Build a JSON array large enough to exceed JQ_OUTPUT_CAP when expanded.
        // Each element is "aaaa...a" (1000 chars). 11_000 elements = 11 MB > 10 MB cap.
        let big_string = "a".repeat(1000);
        let val = serde_json::Value::Array(
            std::iter::repeat_n(serde_json::Value::String(big_string), 11_000).collect(),
        );
        // ".[]" emits each element as a separate output value.
        let result = apply_jq_filter_result(".[]", &val);
        assert!(result.is_err(), "expected cap error but got Ok output");
        let err = result.unwrap_err();
        assert!(
            err.contains("exceeds") && err.contains("MiB"),
            "unexpected error message: {err}"
        );
    }

    // --- property type filters ---

    #[test]
    fn property_info_filter() {
        let val = json!({"name": "title", "type": "text", "value": "My Note"});
        let out = jq(PROPERTY_INFO_FILTER, &val).unwrap();
        assert!(out.contains("title"));
        assert!(out.contains("text"));
        assert!(out.contains("My Note"));
    }

    #[test]
    fn property_info_filter_list_value() {
        let val = json!({"name": "tags", "type": "list", "value": ["rust", "cli"]});
        let out = jq(PROPERTY_INFO_FILTER, &val).unwrap();
        assert!(out.contains("tags"));
        assert!(out.contains("list"));
        // Array values should be wrapped in brackets and joined with ", "
        assert!(out.contains("[rust, cli]"), "expected [rust, cli]: {out}");
        assert!(!out.contains("[\"rust\""));
    }

    #[test]
    fn property_summary_entry_filter() {
        let val = json!({"count": 7, "name": "title", "type": "text"});
        let out = jq(PROPERTY_SUMMARY_ENTRY_FILTER, &val).unwrap();
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
        let out = jq(TAG_SUMMARY_FILTER, &val).unwrap();
        assert!(out.contains("2 unique tags"));
        assert!(out.contains("rust"));
        assert!(out.contains("3 files"));
    }

    // --- link type filters ---

    #[test]
    fn link_info_target_only_filter() {
        let val = json!({"target": "broken-link"});
        let out = jq(LINK_INFO_TARGET_FILTER, &val).unwrap();
        assert!(out.contains("broken-link"));
        assert!(out.contains("unresolved"));
    }

    #[test]
    fn link_info_with_path_filter() {
        let val = json!({"path": "note-b.md", "target": "note-b"});
        let out = jq(LINK_INFO_PATH_FILTER, &val).unwrap();
        assert!(out.contains("note-b"));
        assert!(out.contains("note-b.md"));
    }

    // --- outline type filters ---

    #[test]
    fn task_count_filter() {
        let val = json!({"done": 3, "total": 5});
        let out = jq(TASK_COUNT_FILTER, &val).unwrap();
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
        let out = jq(OUTLINE_SECTION_FILTER, &val).unwrap();
        assert!(out.contains('#'));
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
        let out = jq(OUTLINE_SECTION_WITH_TASKS_FILTER, &val).unwrap();
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
        let out = jq(FIND_TASK_INFO_FILTER, &val).unwrap();
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
        let out = jq(FIND_TASK_INFO_FILTER, &val).unwrap();
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
        let out = fmt(&val);
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
        let out = jq(CONTENT_MATCH_FILTER, &val).unwrap();
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
        let out = fmt(&val);
        assert!(out.contains("line 3"));
        assert!(out.contains("hello world"));
        assert!(!out.contains("line: 3"), "should not use generic fallback");
    }

    // --- Mutation result filters ---

    #[test]
    fn property_value_mutation_filter_with_modified() {
        // SetPropertyResult / AppendPropertyResult / RemovePropertyResult (with value)
        // scanned == total: no "(N scanned)" suffix
        let val = json!({
            "modified": ["note-a.md", "note-b.md"],
            "property": "status",
            "scanned": 2,
            "skipped": [],
            "total": 2,
            "value": "done"
        });
        let out = jq(PROPERTY_VALUE_MUTATION_FILTER, &val).unwrap();
        assert!(out.contains("status=done"));
        assert!(out.contains("2/2 modified"));
        assert!(
            !out.contains("scanned"),
            "no scanned suffix when scanned == total"
        );
        assert!(out.contains("note-a.md"));
        assert!(out.contains("note-b.md"));
    }

    #[test]
    fn property_value_mutation_filter_all_skipped() {
        let val = json!({
            "modified": [],
            "property": "priority",
            "scanned": 1,
            "skipped": ["note-a.md"],
            "total": 1,
            "value": "high"
        });
        let out = jq(PROPERTY_VALUE_MUTATION_FILTER, &val).unwrap();
        assert!(out.contains("priority=high"));
        assert!(out.contains("0/1 modified"));
        // No file paths should appear when nothing was modified
        assert!(!out.contains("note-a.md"));
    }

    #[test]
    fn property_value_mutation_filter_with_where_filter() {
        // scanned > total: "(N scanned)" suffix should appear
        let val = json!({
            "modified": ["note-a.md"],
            "property": "status",
            "scanned": 5,
            "skipped": [],
            "total": 1,
            "value": "done"
        });
        let out = jq(PROPERTY_VALUE_MUTATION_FILTER, &val).unwrap();
        assert!(out.contains("status=done"));
        assert!(out.contains("1/1 modified"));
        assert!(out.contains("(5 scanned)"));
    }

    #[test]
    fn property_value_mutation_via_format_value_as_text() {
        let val = json!({
            "dry_run": false,
            "modified": ["notes/a.md"],
            "property": "status",
            "scanned": 1,
            "skipped": [],
            "total": 1,
            "value": "done"
        });
        let out = fmt(&val);
        assert!(out.contains("status=done"));
        assert!(
            !out.contains("modified: "),
            "should not use generic fallback"
        );
    }

    #[test]
    fn property_mutation_filter_no_value() {
        // RemovePropertyResult without value; scanned == total
        let val = json!({
            "dry_run": false,
            "modified": ["note.md"],
            "property": "draft",
            "scanned": 1,
            "skipped": [],
            "total": 1
        });
        let out = jq(PROPERTY_MUTATION_FILTER, &val).unwrap();
        assert!(out.contains("draft"));
        assert!(out.contains("1/1 modified"));
        assert!(
            !out.contains("scanned"),
            "no scanned suffix when scanned == total"
        );
        assert!(out.contains("note.md"));
    }

    #[test]
    fn property_mutation_filter_no_value_with_where_filter() {
        // RemovePropertyResult without value; scanned > total
        let val = json!({
            "dry_run": false,
            "modified": ["note.md"],
            "property": "draft",
            "scanned": 7,
            "skipped": [],
            "total": 1
        });
        let out = jq(PROPERTY_MUTATION_FILTER, &val).unwrap();
        assert!(out.contains("draft"));
        assert!(out.contains("1/1 modified"));
        assert!(out.contains("(7 scanned)"));
    }

    #[test]
    fn tag_mutation_filter_with_modified() {
        // SetTagResult / RemoveTagResult; scanned == total
        let val = json!({
            "dry_run": false,
            "modified": ["a.md", "b.md"],
            "scanned": 3,
            "skipped": ["c.md"],
            "tag": "rust",
            "total": 3
        });
        let out = jq(TAG_MUTATION_FILTER, &val).unwrap();
        assert!(out.contains("rust"));
        assert!(out.contains("2/3 modified"));
        assert!(
            !out.contains("scanned"),
            "no scanned suffix when scanned == total"
        );
        assert!(out.contains("a.md"));
        assert!(out.contains("b.md"));
        assert!(!out.contains("c.md"));
    }

    #[test]
    fn tag_mutation_filter_with_where_filter() {
        // scanned > total: "(N scanned)" suffix
        let val = json!({
            "dry_run": false,
            "modified": ["a.md"],
            "scanned": 10,
            "skipped": [],
            "tag": "rust",
            "total": 1
        });
        let out = jq(TAG_MUTATION_FILTER, &val).unwrap();
        assert!(out.contains("rust"));
        assert!(out.contains("1/1 modified"));
        assert!(out.contains("(10 scanned)"));
    }

    #[test]
    fn tag_mutation_via_format_value_as_text() {
        let val = json!({
            "dry_run": false,
            "modified": [],
            "scanned": 1,
            "skipped": ["note.md"],
            "tag": "cli",
            "total": 1
        });
        let out = fmt(&val);
        assert!(out.contains("cli"));
        assert!(!out.contains("tag: cli"), "should not use generic fallback");
    }

    // --- dry-run prefix in text output ---

    #[test]
    fn property_value_mutation_dry_run_prefix() {
        let val = json!({
            "dry_run": true,
            "modified": ["note.md"],
            "property": "status",
            "scanned": 1,
            "skipped": [],
            "total": 1,
            "value": "done"
        });
        let out = fmt(&val);
        assert!(
            out.contains("[dry-run] status=done"),
            "dry-run prefix missing: {out}"
        );
    }

    #[test]
    fn tag_mutation_dry_run_prefix() {
        let val = json!({
            "dry_run": true,
            "modified": ["note.md"],
            "scanned": 1,
            "skipped": [],
            "tag": "rust",
            "total": 1
        });
        let out = fmt(&val);
        assert!(
            out.contains("[dry-run] rust"),
            "dry-run prefix missing: {out}"
        );
    }

    #[test]
    fn property_value_mutation_no_dry_run_prefix() {
        let val = json!({
            "dry_run": false,
            "modified": ["note.md"],
            "property": "status",
            "scanned": 1,
            "skipped": [],
            "total": 1,
            "value": "done"
        });
        let out = fmt(&val);
        assert!(
            !out.contains("[dry-run]"),
            "should not have dry-run prefix: {out}"
        );
    }

    // --- build_file_object_filter ---

    #[test]
    fn build_file_object_filter_minimal() {
        // Only the required `file` and `modified` fields.
        let map: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"file": "notes/foo.md", "modified": "2024-01-01"}"#).unwrap();
        let filter = build_file_object_filter(&map);
        let val = json!({"file": "notes/foo.md", "modified": "2024-01-01"});
        let out = jq(&filter, &val).unwrap();
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
        let out = jq(&filter, &val).unwrap();
        assert!(out.contains("foo.md"));
        assert!(out.contains("tags: [rust, cli]"));
    }

    #[test]
    fn build_file_object_filter_with_properties() {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"file": "foo.md", "modified": "2024-01-01", "properties": {"status": "done"}}"#,
        )
        .unwrap();
        let filter = build_file_object_filter(&map);
        let val = json!({
            "file": "foo.md",
            "modified": "2024-01-01",
            "properties": {"status": "done"}
        });
        let out = jq(&filter, &val).unwrap();
        assert!(out.contains("foo.md"));
        assert!(out.contains("properties:"));
        assert!(out.contains("status: done"));
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
        let out = jq(&filter, &val).unwrap();
        assert!(out.contains("foo.md"));
        assert!(out.contains("tasks:"));
        assert!(out.contains("[x] Ship it"));
        assert!(out.contains("line 5"));
    }

    #[test]
    fn build_file_object_filter_with_sections() {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"file": "foo.md", "modified": "2024-01-01", "sections": [{"code_blocks": 0, "heading": "Intro", "level": 1, "line": 1, "links": []}]}"#,
        )
        .unwrap();
        let filter = build_file_object_filter(&map);
        let val = json!({
            "file": "foo.md",
            "modified": "2024-01-01",
            "sections": [{"code_blocks": 0, "heading": "Intro", "level": 1, "line": 1, "links": []}]
        });
        let out = jq(&filter, &val).unwrap();
        assert!(out.contains("foo.md"));
        assert!(out.contains("sections:"));
        assert!(out.contains("# Intro"));
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
        let out = jq(&filter, &val).unwrap();
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
        let out = jq(&filter, &val).unwrap();
        assert!(out.contains("foo.md"));
        assert!(out.contains("links:"));
        assert!(out.contains(r#""bar" → "bar.md""#));
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
        let out = jq(&filter, &val).unwrap();
        assert!(out.contains(r#""missing" (unresolved)"#));
    }

    // --- FileObject text rendering through format_value_as_text ---

    #[test]
    fn file_object_text_rendering_minimal() {
        let val = json!({"file": "notes/foo.md", "modified": "2024-01-15"});
        let out = fmt(&val);
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
            "properties": {"status": "active"},
            "tasks": [
                {"done": false, "line": 10, "section": "Todo", "status": " ", "text": "Fix bug"},
                {"done": true, "line": 20, "section": "Done", "status": "x", "text": "Write docs"}
            ]
        });
        let out = fmt(&val);
        assert!(out.contains("notes/project.md"));
        assert!(out.contains("properties:"));
        assert!(out.contains("status: active"));
        assert!(out.contains("tags: [rust, work]"));
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
        let out = fmt(&val);
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
        let out = fmt(&val);
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
        let out = scalar(&inner);
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
        let out = fmt(&val);
        assert!(out.contains("status"));
        assert!(out.contains("3 files"));
        // Should NOT look like "count: 3" (that's the generic fallback)
        assert!(!out.contains("count: 3"));
    }

    #[test]
    fn format_value_as_text_falls_back_for_unknown_shape() {
        let val = json!({"foo": "bar", "baz": 42});
        let out = fmt(&val);
        // Generic fallback: key: value
        assert!(out.contains("foo: bar") || out.contains("baz: 42"));
    }

    #[test]
    fn mv_result_filter_applied() {
        let val = json!({
            "dry_run": false,
            "from": "sub/b.md",
            "to": "archive/b.md",
            "total_files_updated": 1,
            "total_links_updated": 1,
            "updated_files": [
                {
                    "file": "a.md",
                    "replacements": [
                        {"old_text": "[[sub/b]]", "new_text": "[[archive/b]]", "line": 1}
                    ]
                }
            ]
        });
        // Verify key signature matches expected
        let sig = {
            let map = val.as_object().unwrap();
            let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
            keys.sort_unstable();
            keys.join(",")
        };
        assert_eq!(
            sig,
            "dry_run,from,to,total_files_updated,total_links_updated,updated_files"
        );
        // Verify the jq filter itself works
        let filter_result = apply_jq_filter_result(MV_RESULT_FILTER, &val);
        assert!(filter_result.is_ok(), "filter error: {filter_result:?}");
        let out = filter_result.unwrap();
        assert!(out.contains("Moved sub/b.md"), "out: {out}");
        assert!(out.contains("archive/b.md"), "out: {out}");
        assert!(out.contains("[[sub/b]]"), "out: {out}");
        assert!(out.contains("[[archive/b]]"), "out: {out}");
        // Verify lookup_filter finds the filter for this shape
        let found_filter =
            lookup_filter("dry_run,from,to,total_files_updated,total_links_updated,updated_files");
        assert!(
            found_filter.is_some(),
            "lookup_filter returned None for MvResult shape"
        );
        // format_value_as_text should pick up the filter
        let formatted = fmt(&val);
        assert!(
            formatted.contains("Moved sub/b.md"),
            "formatted: {formatted}"
        );
    }

    #[test]
    fn format_value_as_text_array_of_typed_objects() {
        let val = json!([
            {"path": "a.md", "tags": ["rust"]},
            {"path": "b.md", "tags": ["cli"]}
        ]);
        let out = fmt(&val);
        assert!(out.contains("a.md"));
        assert!(out.contains("b.md"));
        assert!(out.contains("rust"));
        assert!(out.contains("cli"));
    }

    // --- sanitize_control_chars ---

    #[test]
    fn sanitize_control_chars_strips_escape_sequences() {
        let input = "Hello\x1b[31mRED\x1b[0m World";
        let output = sanitize_control_chars(input);
        assert!(
            !output.contains('\x1b'),
            "escape sequences should be stripped"
        );
        assert!(output.contains("Hello"));
        assert!(output.contains("RED"));
        assert!(output.contains("World"));
    }

    #[test]
    fn sanitize_control_chars_preserves_newline_and_tab() {
        let input = "line1\nline2\ttabbed";
        let output = sanitize_control_chars(input);
        assert_eq!(output, input);
    }

    #[test]
    fn text_output_sanitizes_escape_sequences() {
        let value = serde_json::json!({
            "results": {
                "title": "Hello\x1b[31mRED\x1b[0m World",
                "file": "test\x1b[2J.md"
            }
        });
        let output = format_success(Format::Text, &value);
        assert!(
            !output.contains('\x1b'),
            "escape sequences should be stripped"
        );
        assert!(output.contains("Hello") && output.contains("World"));
    }
}
