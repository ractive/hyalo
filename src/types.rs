//! Typed structs for all JSON output shapes.
//!
//! Every command serializes one of these types (or a `Vec` of them) as its
//! JSON response.  Using concrete types instead of ad-hoc `json!()` macros
//! ensures that the outline command, properties, tags, and links all share
//! the same shapes for overlapping data (e.g. `PropertyInfo`).

use serde::Serialize;

// ---------------------------------------------------------------------------
// Property types (shared by properties list/read/set and outline)
// ---------------------------------------------------------------------------

/// A single frontmatter property with its inferred type and value.
/// Used by `properties list`, `property read`, `property set`, and `outline`.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub prop_type: String,
    pub value: serde_json::Value,
}

/// A file with its frontmatter properties.
/// Used by `properties list` (per-file detail).
#[derive(Debug, Clone, Serialize)]
pub struct FileProperties {
    pub path: String,
    pub properties: Vec<PropertyInfo>,
}

/// Aggregate property summary entry.
/// Used by `properties summary`.
#[derive(Debug, Clone, Serialize)]
pub struct PropertySummaryEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub prop_type: String,
    pub count: usize,
}

/// Result of removing a property.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyRemoved {
    pub removed: String,
    pub path: String,
}

// ---------------------------------------------------------------------------
// Tag types (shared by tags list/summary and outline)
// ---------------------------------------------------------------------------

/// A file with its tags.
/// Used by `tags list` (per-file detail).
#[derive(Debug, Clone, Serialize)]
pub struct FileTags {
    pub path: String,
    pub tags: Vec<String>,
}

/// Aggregate tag summary.
/// Used by `tags summary`.
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
// Link types (shared by links command and outline)
// ---------------------------------------------------------------------------

/// A single link with its resolution status.
/// Used by `links` command.
#[derive(Debug, Clone, Serialize)]
pub struct LinkInfo {
    pub target: String,
    pub path: Option<String>,
    pub label: Option<String>,
}

/// A file with its outgoing links.
/// Used by `links` command.
#[derive(Debug, Clone, Serialize)]
pub struct FileLinks {
    pub path: String,
    pub links: Vec<LinkInfo>,
}

// ---------------------------------------------------------------------------
// Find / mutation result types
// ---------------------------------------------------------------------------

/// Result of a `property find` command.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyFindResult {
    pub property: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub files: Vec<String>,
    pub total: usize,
}

/// Result of a `tag find` command.
#[derive(Debug, Clone, Serialize)]
pub struct TagFindResult {
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub files: Vec<String>,
    pub total: usize,
}

/// Result of a `property add-to-list` or `property remove-from-list` command.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyMutationResult {
    pub property: String,
    pub values: Vec<String>,
    pub modified: Vec<String>,
    pub skipped: Vec<String>,
    pub total: usize,
}

/// Result of a `tag add` or `tag remove` command.
#[derive(Debug, Clone, Serialize)]
pub struct TagMutationResult {
    pub tag: String,
    pub modified: Vec<String>,
    pub skipped: Vec<String>,
    pub total: usize,
}

// ---------------------------------------------------------------------------
// Outline types (new in iteration 6)
// ---------------------------------------------------------------------------

/// Task checkbox counts within a section.
#[derive(Debug, Clone, Serialize)]
pub struct TaskCount {
    pub total: usize,
    pub done: usize,
}

/// A single section in the document outline.
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

/// Full outline of a single file.
#[derive(Debug, Clone, Serialize)]
pub struct FileOutline {
    pub file: String,
    pub properties: Vec<PropertyInfo>,
    pub tags: Vec<String>,
    pub sections: Vec<OutlineSection>,
}

// ---------------------------------------------------------------------------
// Task types (new in iteration 9)
// ---------------------------------------------------------------------------

/// A single task (checkbox) with its location and state.
/// Used by `tasks`, `task read`, `task toggle`, `task set-status`.
#[derive(Debug, Clone, Serialize)]
pub struct TaskInfo {
    pub line: usize,
    pub status: String,
    pub text: String,
    pub done: bool,
}

/// A file with its tasks.
/// Used by `tasks` command (per-file detail).
#[derive(Debug, Clone, Serialize)]
pub struct FileTasks {
    pub file: String,
    pub tasks: Vec<TaskInfo>,
    pub total: usize,
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
