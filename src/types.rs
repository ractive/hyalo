//! Typed structs for all JSON output shapes.
//!
//! Every command serializes one of these types (or a `Vec` of them) as its
//! JSON response. Using concrete types ensures that `find`, set/remove/append,
//! properties, tags, task, and summary all share the same shapes for
//! overlapping data (e.g. `PropertyInfo`).

use serde::Serialize;

// ---------------------------------------------------------------------------
// Property types
// ---------------------------------------------------------------------------

/// A single frontmatter property with its inferred type and value.
/// Used by `find` (properties field) and `properties` (aggregate summary).
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

// ---------------------------------------------------------------------------
// Outline types
// ---------------------------------------------------------------------------

/// Task checkbox counts within a section.
#[derive(Debug, Clone, Serialize)]
pub struct TaskCount {
    pub total: usize,
    pub done: usize,
}

/// A single section in the document outline.
/// Used by `find` (sections field).
#[derive(Debug, Clone, Serialize)]
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
/// Used by `task read`, `task toggle`, `task set-status`.
#[derive(Debug, Clone, Serialize)]
pub struct TaskInfo {
    pub line: usize,
    pub status: String,
    pub text: String,
    pub done: bool,
}

/// Result of reading or mutating a single task.
/// Used by `task read`, `task toggle`, `task set-status`.
#[derive(Debug, Clone, Serialize)]
pub struct TaskReadResult {
    pub file: String,
    pub line: usize,
    pub status: String,
    pub text: String,
    pub done: bool,
}

// ---------------------------------------------------------------------------
// Summary types
// ---------------------------------------------------------------------------

/// High-level vault summary.
#[derive(Debug, Clone, Serialize)]
pub struct VaultSummary {
    pub files: FileCounts,
    pub properties: Vec<PropertySummaryEntry>,
    pub tags: TagSummary,
    pub status: Vec<StatusGroup>,
    pub tasks: TaskCount,
    pub recent_files: Vec<RecentFile>,
}

/// File counts by directory.
#[derive(Debug, Clone, Serialize)]
pub struct FileCounts {
    pub total: usize,
    pub by_directory: Vec<DirectoryCount>,
}

/// Count of files in a directory.
#[derive(Debug, Clone, Serialize)]
pub struct DirectoryCount {
    pub directory: String,
    pub count: usize,
}

/// Files grouped by status property value.
#[derive(Debug, Clone, Serialize)]
pub struct StatusGroup {
    pub value: String,
    pub files: Vec<String>,
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
#[derive(Debug, Clone, Serialize)]
pub struct FindTaskInfo {
    pub line: usize,
    pub section: String,
    pub status: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Vec<PropertyInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sections: Option<Vec<OutlineSection>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<Vec<FindTaskInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<LinkInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<Vec<ContentMatch>>,
}
