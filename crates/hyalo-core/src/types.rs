//! Shared typed structs for JSON output shapes.
//!
//! Commands use these types for overlapping data (e.g. `PropertyInfo`,
//! `FileObject`). Some commands also define result structs in their own
//! modules (e.g. `SetPropertyResult`, `RemoveTagResult`).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Property types
// ---------------------------------------------------------------------------

/// A single frontmatter property with its inferred type and value.
/// Used by `properties` (aggregate summary).
#[derive(Debug, Clone, Serialize)]
pub struct PropertyInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub prop_type: String,
    pub value: serde_json::Value,
}

/// Aggregate property summary entry.
/// Used by `properties` command and `summary`.
#[derive(Debug, Clone, Serialize)]
pub struct PropertySummaryEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub prop_type: String,
    pub count: usize,
}

// ---------------------------------------------------------------------------
// Tag types
// ---------------------------------------------------------------------------

/// Aggregate tag summary.
/// Used by `tags` command and `summary`.
#[derive(Debug, Clone, Serialize)]
pub struct TagSummary {
    pub tags: Vec<TagSummaryEntry>,
    pub total: usize,
}

/// A single tag with its file count.
#[derive(Debug, Clone, Serialize)]
pub struct TagSummaryEntry {
    pub name: String,
    pub count: usize,
}

// ---------------------------------------------------------------------------
// Link types
// ---------------------------------------------------------------------------

/// A single link with its resolution status.
/// Used by `find` (links field).
#[derive(Debug, Clone, Serialize)]
pub struct LinkInfo {
    pub target: String,
    pub path: Option<String>,
    pub label: Option<String>,
}

/// A single backlink: another file that links to this one.
/// Used by `find` (backlinks field).
#[derive(Debug, Clone, Serialize)]
pub struct BacklinkInfo {
    pub source: String,
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

// ---------------------------------------------------------------------------
// Outline types
// ---------------------------------------------------------------------------

/// Task checkbox counts within a section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCount {
    pub total: usize,
    pub done: usize,
}

/// A single section in the document outline.
/// Used by `find` (sections field).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineSection {
    pub level: u8,
    pub heading: Option<String>,
    pub line: usize,
    pub links: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<TaskCount>,
    pub code_blocks: Vec<String>,
}

// ---------------------------------------------------------------------------
// Task types
// ---------------------------------------------------------------------------

/// A single task (checkbox) with its location and state.
/// Used by `task read`, `task toggle`, `task set`.
#[derive(Debug, Clone, Serialize)]
pub struct TaskInfo {
    pub line: usize,
    pub status: char,
    pub text: String,
    pub done: bool,
}

/// Result of reading or mutating a single task.
/// Used by `task read`, `task toggle`, `task set`.
#[derive(Debug, Clone, Serialize)]
pub struct TaskReadResult {
    pub file: String,
    pub line: usize,
    pub status: char,
    pub text: String,
    pub done: bool,
}

/// Result of a `task toggle --dry-run` simulation.
/// Carries both the original and the would-be status so the text formatter
/// can render `"file":line [old] -> [new] text` and make the direction of
/// change explicit.
#[derive(Debug, Clone, Serialize)]
pub struct TaskDryRunResult {
    pub file: String,
    pub line: usize,
    pub old_status: char,
    pub status: char,
    pub text: String,
    pub done: bool,
}

// ---------------------------------------------------------------------------
// Summary types
// ---------------------------------------------------------------------------

/// Lint violation counts for the vault summary.
#[derive(Debug, Clone, Serialize)]
pub struct LintSummary {
    pub errors: usize,
    pub warnings: usize,
    pub files_with_issues: usize,
}

/// High-level vault summary.
#[derive(Debug, Clone, Serialize)]
pub struct VaultSummary {
    pub files: FileCounts,
    pub orphans: usize,
    pub dead_ends: usize,
    pub links: LinkHealthSummary,
    pub properties: Vec<PropertySummaryEntry>,
    pub tags: TagSummary,
    pub status: Vec<StatusGroup>,
    pub tasks: TaskCount,
    pub recent_files: Vec<RecentFile>,
    /// Schema lint counts — `None` when no `[schema]` block is configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<LintSummary>,
}

/// Vault-wide link health: total links and broken count.
#[derive(Debug, Clone, Serialize)]
pub struct LinkHealthSummary {
    pub total: usize,
    pub broken: usize,
}

/// File counts by directory.
#[derive(Debug, Clone, Serialize)]
pub struct FileCounts {
    pub total: usize,
    pub directories: Vec<DirectoryCount>,
}

/// Count of files in a directory.
#[derive(Debug, Clone, Serialize)]
pub struct DirectoryCount {
    pub directory: String,
    pub count: usize,
}

/// Files grouped by status property value (count only).
#[derive(Debug, Clone, Serialize)]
pub struct StatusGroup {
    pub value: String,
    pub count: usize,
}

/// A recently modified file.
#[derive(Debug, Clone, Serialize)]
pub struct RecentFile {
    pub path: String,
    pub modified: String,
}

// ---------------------------------------------------------------------------
// Find command types
// ---------------------------------------------------------------------------

/// A single task with section context, used by the `find` command.
/// Extends `TaskInfo` with section heading information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindTaskInfo {
    pub line: usize,
    pub section: String,
    pub status: char,
    pub text: String,
    pub done: bool,
}

/// A content search match within a file body.
#[derive(Debug, Clone, Serialize)]
pub struct ContentMatch {
    pub line: usize,
    pub section: String,
    pub text: String,
}

/// The unified file object returned by the `find` command.
/// Always returned in an array. Optional fields are controlled by `--fields`.
#[derive(Debug, Clone, Serialize)]
pub struct FileObject {
    pub file: String,
    pub modified: String,
    /// Title extracted from frontmatter `title` property or first H1 heading.
    /// - `None`: field not requested (omitted from JSON output)
    /// - `Some(Value::String(...))`: title found
    /// - `Some(Value::Null)`: title requested but not found
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties_typed: Option<Vec<PropertyInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sections: Option<Vec<OutlineSection>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<Vec<FindTaskInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<LinkInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backlinks: Option<Vec<BacklinkInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<Vec<ContentMatch>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}
