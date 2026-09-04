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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub prop_type: String,
    pub value: serde_json::Value,
}

/// Aggregate property summary entry.
/// Used by `properties` command and `summary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySummaryEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub prop_type: String,
    pub count: usize,
    /// Present only when the property has inconsistent types across files.
    /// Each entry is `(type_name, file_count)` for that type variant.
    /// When `None`, all occurrences share the same type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mixed_types: Option<Vec<MixedTypeEntry>>,
}

/// One type variant in a mixed-type property summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixedTypeEntry {
    #[serde(rename = "type")]
    pub prop_type: String,
    pub count: usize,
}

// ---------------------------------------------------------------------------
// Tag types
// ---------------------------------------------------------------------------

/// Aggregate tag summary.
/// Used by `tags` command and `summary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagSummary {
    pub tags: Vec<TagSummaryEntry>,
    pub total: usize,
}

/// A single tag with its file count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagSummaryEntry {
    pub name: String,
    pub count: usize,
}

// ---------------------------------------------------------------------------
// Link types
// ---------------------------------------------------------------------------

/// What a link in the `--fields links` inventory *is* — the reported `kind`
/// (iter-261, dogfood UX-6).
///
/// Distinct from [`crate::links::LinkKind`], which is the two-valued *syntax*
/// the resolver branches on. This is the user-facing bucket, and it mixes
/// syntax (`embed`, `markdown`) with verdict (`external`, `attachment`)
/// because that is what a reader triaging a link report needs: without it,
/// telling `![[img.png]]` from `[[note]]` from `<obsidian://…>` meant going
/// back to the file.
///
/// Precedence when several could apply — `external` beats `attachment` beats
/// `embed` beats the syntax kinds — so exactly one label is reported per link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LinkKindLabel {
    /// `[[note]]` — a plain wikilink to a vault note.
    #[default]
    Wikilink,
    /// `![[note]]` / `![[img.png]]` — an embed.
    Embed,
    /// `[text](note.md)` — a markdown link to a vault file.
    Markdown,
    /// `[text](https://…)`, `[[obsidian://…]]` — a URI, resolved by nothing.
    External,
    /// A link that resolved to a vault file which is **not** markdown — an
    /// image, a PDF, an Obsidian `.base`. Never broken, never a graph edge.
    Attachment,
    /// A `[[wikilink]]` written inside a YAML frontmatter value — `related:`,
    /// `categories: ["[[Books]]"]`, `type: "[[Author]]"` (iter-262, BUG-1).
    ///
    /// A real graph edge like a body wikilink; the separate label exists
    /// because the two are found in different places and a reader triaging a
    /// link report needs to know which. The originating key is reported
    /// alongside as `property`.
    Frontmatter,
}

impl LinkKindLabel {
    /// The label for a parsed link and its resolution result.
    #[must_use]
    pub fn classify(link: &crate::links::Link, resolved_path: Option<&str>) -> Self {
        if link.external {
            return Self::External;
        }
        if resolved_path.is_some_and(|p| !crate::discovery::has_md_extension(p)) {
            return Self::Attachment;
        }
        // iter-262: a frontmatter wikilink is reported as `frontmatter` rather
        // than `wikilink`, but a frontmatter value naming an image or a URI is
        // still an attachment / external first — those two verdicts say the
        // link resolves to nothing markdown, which outranks where it was
        // written.
        if link.is_frontmatter() {
            return Self::Frontmatter;
        }
        if link.embed {
            return Self::Embed;
        }
        match link.kind {
            crate::links::LinkKind::Wikilink => Self::Wikilink,
            crate::links::LinkKind::Markdown => Self::Markdown,
        }
    }

    /// Whether a link of this kind can ever be reported broken.
    ///
    /// An external URI names nothing in the vault and an attachment already
    /// resolved, so neither counts toward `links.broken`, HYALO006 or
    /// `find --broken-links`.
    #[must_use]
    pub fn is_resolvable_vault_link(self) -> bool {
        !matches!(self, Self::External | Self::Attachment)
    }
}

/// A single link with its resolution status.
/// Used by `find` (links field).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkInfo {
    pub target: String,
    pub path: Option<String>,
    pub label: Option<String>,
    /// What this link is: `wikilink` | `embed` | `markdown` | `external` |
    /// `attachment` (iter-261, dogfood UX-6). Always serialized — a link
    /// always has a kind — and defaulted to `wikilink` when reading JSON
    /// written by an older hyalo.
    #[serde(default)]
    pub kind: LinkKindLabel,
    /// The frontmatter key this link was written under, for a link with
    /// `kind: "frontmatter"` — the dotted key path for a nested map
    /// (`meta.source`), the plain key otherwise (iter-262). Absent for body
    /// links, so the shape of an existing report is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property: Option<String>,
    /// 1-based source line the link was written on (iter-215, dogfood UX-6).
    ///
    /// `find --broken-links` used to list every link of a matching file with no
    /// location, so finding the reported broken link meant grepping the file.
    /// The line is the same one `hyalo lint` (HYALO006) and `backlinks` report
    /// for the same link, and comes straight from the index
    /// (`IndexEntry::links` / `IndexEntry::self_anchors` already store it), so
    /// no extra file read is involved.
    ///
    /// Named `line` to match every other line-bearing shape in `.results`
    /// (`BacklinkInfo`, `OutlineSection`, `ContentMatch`, `TaskInfo`) — always
    /// a 1-based source line, never an index or an offset. Always serialized:
    /// unlike `fragment` / `broken_anchor` / `out_of_vault` this is not a
    /// verdict that may be absent, it is a location every link has.
    /// `#[serde(default)]` only covers deserializing JSON written by an older
    /// hyalo, where it reads back as `0`.
    #[serde(default)]
    pub line: usize,
    /// The `#fragment` (heading anchor) the link carried, without the leading
    /// `#`. `None` for links with no fragment. Skipped from JSON when absent so
    /// non-anchored links keep today's shape (L-21, iter-190).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
    /// `true` when the link's target file resolved (`path` is `Some`) but the
    /// `#fragment` does not name any heading in that file — a *broken anchor*.
    /// Distinct from a broken target (`path: None`); the two are never both set
    /// on one link. Skipped from JSON when `false` so non-anchored / valid
    /// links keep today's shape.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub broken_anchor: bool,
    /// The full heading text to write instead, when this link's dead fragment
    /// is the prefix of exactly one heading in the target file (iter-261 /
    /// DEC-268): `[[decision-log#DEC-068]]` → `DEC-068: Snapshot index format`.
    ///
    /// Only ever set alongside `broken_anchor`, and only when the prefix is
    /// unambiguous — two matching headings yield no suggestion. It is a
    /// suggestion, never an automatic rewrite: a silent prefix match would hide
    /// the typos this rule exists to surface. Skipped from JSON when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_fragment: Option<String>,
    /// `true` when the link's target normalizes to a path *above* the vault
    /// root, so it can never resolve to a scanned file. Implies `path: None`,
    /// but is deliberately distinguished from a broken target: the file is out
    /// of scope, not missing (iter-193). Skipped from JSON when `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub out_of_vault: bool,
}

/// A single backlink: another file that links to this one.
/// Used by `find` (backlinks field).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintSummary {
    pub errors: usize,
    pub warnings: usize,
    /// Number of files with at least one schema violation.
    ///
    /// iter-216 D-5: named `files_with_violations` to match the key `hyalo
    /// lint` emits for the same quantity. `summary` used to call it
    /// `files_with_issues`, so a script comparing the digest against a full
    /// lint run had to know both spellings.
    pub files_with_violations: usize,
}

/// High-level vault summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSummary {
    /// Resolved vault directory (display string).
    pub dir: String,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkHealthSummary {
    pub total: usize,
    pub broken: usize,
    /// Links pointing above the scanned vault root (`../..` escapes). Kept out
    /// of `broken` because the target is out of scope rather than missing
    /// (iter-193). Omitted from JSON when zero so vaults with no such links
    /// keep the previous output shape.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub out_of_vault: usize,
    /// Links whose target resolves but whose `#fragment` names no heading
    /// there — a dead anchor, distinct from `broken` (a missing target).
    ///
    /// NEW-15 (dogfood pre3): `summary` used to say "0 broken" on a vault
    /// `find --broken-links` reported 3 files for, because this count never
    /// looked at anchors. Kept as its own field rather than folded into
    /// `broken` since the two are different failure modes with different
    /// fixes. Omitted from JSON when zero, same convention as `out_of_vault`.
    ///
    /// PR #251 review M3: only computed when `broken == 0` — checking it
    /// unconditionally would mean a second full link-resolution pass
    /// (re-hitting the filesystem for every fragment-bearing link) right
    /// after the first one `summary` already runs, doubling summary's own
    /// cost on a fragment-heavy corpus. A vault with both broken targets and
    /// broken anchors reports `Some(0)` here until the targets are fixed;
    /// `find --broken-links` is the always-accurate source of truth.
    ///
    /// PR #251 review L6: `None` when the vault directory itself could not
    /// be canonicalized. Deliberately serialized as JSON `null` — NOT
    /// omitted like a computed/gated `Some(0)` — so a script reading this
    /// field cannot mistake "could not check" for "checked and it's clean";
    /// omitting both would make them indistinguishable, which is exactly the
    /// false-clean-bill this finding exists to prevent.
    #[serde(skip_serializing_if = "is_zero_broken_anchors")]
    pub broken_anchors: Option<usize>,
}

/// Serde helper: skip a `usize` field when it is zero.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Serde helper for [`LinkHealthSummary::broken_anchors`]: skip only a
/// computed/gated zero, matching the sibling zero-omission convention —
/// `None` is deliberately kept (serializes as `null`) so it stays
/// distinguishable from a zero (PR #251 review L6).
///
/// `&Option<usize>` (not `Option<&usize>`) because serde's generated code
/// calls this with `&self.broken_anchors` — the signature is fixed by the
/// caller, not a style choice `Option<&T>` could improve.
#[allow(clippy::trivially_copy_pass_by_ref, clippy::ref_option)]
fn is_zero_broken_anchors(value: &Option<usize>) -> bool {
    matches!(value, Some(0))
}

/// File counts by directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCounts {
    pub total: usize,
    pub directories: Vec<DirectoryCount>,
}

/// Count of files in a directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryCount {
    pub directory: String,
    pub count: usize,
}

/// Files grouped by status property value (count only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusGroup {
    pub value: String,
    pub count: usize,
}

/// A recently modified file.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentMatch {
    pub line: usize,
    pub section: String,
    pub text: String,
}

/// The unified file object returned by the `find` command.
/// Always returned in an array. Optional fields are controlled by `--fields`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileObject {
    /// The only unconditional key (iteration 254, DEC-254): it names the
    /// result, so no projection may drop it.
    pub file: String,
    /// Last-modified timestamp. In the *default* field set — an agent picks
    /// its next call by recency — but an explicit `--fields` that does not
    /// name `modified` drops it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    /// File size in bytes (iteration 252), so an agent can budget a `read`
    /// before issuing it. Default field set; droppable via `--fields`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Line count (see [`crate::scanner::ScanStats`]) — the unit
    /// `read --lines A:B` takes. Default field set; droppable via `--fields`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<usize>,
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
