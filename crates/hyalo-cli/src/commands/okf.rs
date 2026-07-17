#![allow(clippy::missing_errors_doc)]
//! `hyalo okf` — Open Knowledge Format artifact generators.
//!
//! Two deterministic, LLM-free generators that maintain the *derived* reserved
//! files an OKF bundle otherwise hand-maintains:
//!
//! - [`run_index`] regenerates every directory's `index.md` from the frontmatter
//!   of its child concepts (`* [title](relative-link) - description`), grouped by
//!   the concept `type`, and lists child subdirectories. It writes into a stable
//!   *managed region* delimited by HTML comment markers so hand-written prose
//!   above or below the list is preserved, and it never clobbers the bundle-root
//!   `index.md`'s `okf_version` frontmatter line.
//! - [`run_log`] prepends a dated entry under today's `YYYY-MM-DD` heading to a
//!   scope-selectable `log.md` (directory-local per SPEC §7), newest first.
//!
//! Both default to `--dry-run` and mutate only with `--apply`, matching the
//! `links fix` / `links auto` house convention. In dry-run, [`run_index`] exits
//! non-zero when the on-disk `index.md` files differ from the generated output,
//! so it doubles as a CI drift check (`hyalo okf index --dry-run`).

use anyhow::{Context, Result};
use hyalo_core::discovery;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use crate::output::{CommandOutcome, Format, format_error};

/// Markers delimiting the region of `index.md` that `okf index` owns. Prose
/// outside these markers is preserved verbatim across regenerations.
const INDEX_BEGIN: &str = "<!-- okf:index:begin -->";
const INDEX_END: &str = "<!-- okf:index:end -->";

/// The canonical OKF frontmatter key order (`reference_agent`'s `bundle`
/// package emits keys in this order). Used by [`normalize_key_order`].
pub const OKF_KEY_ORDER: &[&str] = &[
    "type",
    "resource",
    "title",
    "description",
    "tags",
    "timestamp",
];

// ---------------------------------------------------------------------------
// index
// ---------------------------------------------------------------------------

/// One entry (a concept file) rendered into an `index.md` list.
struct ConceptEntry {
    /// The `type` frontmatter value, or `None` when absent (grouped last).
    concept_type: Option<String>,
    /// Display title: frontmatter `title` else the filename stem.
    title: String,
    /// Link target, relative to the directory the `index.md` lives in
    /// (forward slashes, e.g. `blocks.md` or `tables/blocks.md`).
    link: String,
    /// Optional `description` frontmatter value.
    description: Option<String>,
}

/// Result of planning a single `index.md` regeneration.
struct IndexPlan {
    /// Vault-relative path of the `index.md` (forward slashes).
    rel_path: String,
    /// The full new file content.
    new_content: String,
    /// The current on-disk content (empty string when the file is absent).
    old_content: String,
}

impl IndexPlan {
    fn changed(&self) -> bool {
        self.new_content != self.old_content
    }
    fn is_new(&self) -> bool {
        self.old_content.is_empty()
    }
}

/// Regenerate `index.md` files across the vault (or a scoped subtree).
///
/// `scope` optionally restricts regeneration to a single directory subtree
/// (vault-relative). `apply` writes changes; otherwise this is a dry run and
/// returns exit code 1 (via `exit_code_override`) when any `index.md` would
/// change — the CI drift signal.
pub fn run_index(
    dir: &Path,
    scope: Option<&str>,
    apply: bool,
    format: Format,
) -> Result<(CommandOutcome, Option<i32>)> {
    // Resolve the optional scope directory to a vault-relative prefix.
    let scope_prefix = match resolve_scope(dir, scope) {
        Ok(p) => p,
        Err(msg) => {
            return Ok((
                CommandOutcome::UserError(format_error(format, &msg, scope, None, None)),
                None,
            ));
        }
    };

    let files = discovery::discover_files(dir).context("failed to scan vault for concepts")?;

    // Group concept files by their containing directory (vault-relative, forward
    // slashes; "" == bundle root). Reserved files (index.md/log.md) are skipped
    // as *entries* but their directories still get an index.
    let mut by_dir: BTreeMap<String, Vec<ConceptEntry>> = BTreeMap::new();
    // Ensure every directory that contains concepts (or is the root) gets an
    // index. Also record child subdirectories per directory.
    let mut child_dirs: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for full in &files {
        let rel = discovery::relative_path(dir, full);
        let file_name = Path::new(&rel)
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let parent = parent_dir(&rel);

        // Register the directory (even if it only holds reserved files) so the
        // bundle root and every subdir gets an index entry key.
        by_dir.entry(parent.clone()).or_default();
        register_ancestor_dirs(&parent, &mut child_dirs);

        // Reserved files are outputs, never entries.
        if file_name == "index.md" || file_name == "log.md" {
            continue;
        }

        let entry = read_concept_entry(full, &rel, &parent)?;
        by_dir.entry(parent).or_default().push(entry);
    }

    // Always ensure the bundle root has an index, even in an empty vault.
    by_dir.entry(String::new()).or_default();

    // Build a plan per directory in scope.
    let mut plans: Vec<IndexPlan> = Vec::new();
    for (parent, entries) in &by_dir {
        if !dir_in_scope(parent, scope_prefix.as_deref()) {
            continue;
        }
        let subdirs = child_dirs.get(parent).cloned().unwrap_or_default();
        let plan = plan_index(dir, parent, entries, &subdirs)?;
        plans.push(plan);
    }
    plans.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    let changed: Vec<&IndexPlan> = plans.iter().filter(|p| p.changed()).collect();

    if apply {
        for plan in &changed {
            let full = dir.join(&plan.rel_path);
            hyalo_core::fs_util::atomic_write(&full, plan.new_content.as_bytes())
                .with_context(|| format!("failed to write {}", plan.rel_path))?;
        }
    }

    let results: Vec<serde_json::Value> = changed
        .iter()
        .map(|p| {
            serde_json::json!({
                "file": p.rel_path,
                "action": if p.is_new() { "create" } else { "update" },
            })
        })
        .collect();

    let payload = serde_json::json!({
        "command": "okf index",
        "apply": apply,
        "scanned": plans.len(),
        "changed": changed.len(),
        "files": results,
    });

    // In dry-run mode, drift (any changed file) is a non-zero exit for CI.
    let exit_override = if !apply && !changed.is_empty() {
        Some(1)
    } else {
        None
    };

    Ok((
        CommandOutcome::success_with_total(payload.to_string(), changed.len() as u64),
        exit_override,
    ))
}

/// Read a concept file's frontmatter into a [`ConceptEntry`].
fn read_concept_entry(full: &Path, rel: &str, parent: &str) -> Result<ConceptEntry> {
    let props = hyalo_core::frontmatter::read_frontmatter(full)
        .with_context(|| format!("failed to parse frontmatter of {rel}"))?;

    let concept_type = props
        .get("type")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    let stem = Path::new(rel)
        .file_stem()
        .map_or_else(|| rel.to_owned(), |s| s.to_string_lossy().into_owned());

    let title = props
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or(stem, ToOwned::to_owned);

    let description = props
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    // Link is relative to the directory the index.md lives in.
    let file_name = Path::new(rel)
        .file_name()
        .map_or_else(|| rel.to_owned(), |n| n.to_string_lossy().into_owned());
    let link = if parent.is_empty() {
        file_name
    } else {
        // rel starts with `parent/`; strip the parent prefix to get the
        // directory-relative link. parent has forward slashes already.
        rel.strip_prefix(&format!("{parent}/"))
            .unwrap_or(&file_name)
            .to_owned()
    };

    Ok(ConceptEntry {
        concept_type,
        title,
        link,
        description,
    })
}

/// Plan the regeneration of one directory's `index.md`.
fn plan_index(
    dir: &Path,
    parent: &str,
    entries: &[ConceptEntry],
    subdirs: &[String],
) -> Result<IndexPlan> {
    let rel_path = if parent.is_empty() {
        "index.md".to_owned()
    } else {
        format!("{parent}/index.md")
    };
    let full = dir.join(&rel_path);

    let old_content = if full.is_file() {
        std::fs::read_to_string(&full).with_context(|| format!("failed to read {rel_path}"))?
    } else {
        String::new()
    };

    let generated = render_index_body(entries, subdirs, parent);
    let new_content = splice_managed_region(&old_content, &generated, parent.is_empty());

    Ok(IndexPlan {
        rel_path,
        new_content,
        old_content,
    })
}

/// Render the managed list body (between the markers, exclusive).
fn render_index_body(entries: &[ConceptEntry], subdirs: &[String], parent: &str) -> String {
    let mut out = String::new();

    // Group concept entries by type, preserving a stable order: named types
    // sorted alphabetically, then untyped entries last.
    let mut grouped: BTreeMap<String, Vec<&ConceptEntry>> = BTreeMap::new();
    let mut untyped: Vec<&ConceptEntry> = Vec::new();
    for e in entries {
        match &e.concept_type {
            Some(t) => grouped.entry(t.clone()).or_default().push(e),
            None => untyped.push(e),
        }
    }

    // Subdirectories first (as a navigational group), if any.
    if !subdirs.is_empty() {
        out.push_str("## Subdirectories\n\n");
        let mut sorted = subdirs.to_vec();
        sorted.sort();
        sorted.dedup();
        for sub in sorted {
            let name = sub.rsplit('/').next().unwrap_or(&sub);
            let _ = writeln!(out, "* [{name}]({name}/index.md)");
        }
        out.push('\n');
    }

    for (type_name, mut items) in grouped {
        items.sort_by_key(|a| a.title.to_lowercase());
        let _ = write!(out, "## {type_name}\n\n");
        for e in items {
            out.push_str(&render_entry_line(e));
        }
        out.push('\n');
    }

    if !untyped.is_empty() {
        untyped.sort_by_key(|a| a.title.to_lowercase());
        out.push_str("## Other\n\n");
        for e in untyped {
            out.push_str(&render_entry_line(e));
        }
        out.push('\n');
    }

    if entries.is_empty() && subdirs.is_empty() {
        let where_ = if parent.is_empty() {
            "this bundle".to_owned()
        } else {
            format!("`{parent}`")
        };
        let _ = write!(out, "_No concepts in {where_} yet._\n\n");
    }

    // Trim the trailing blank line we appended after the last group.
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// Render one `* [title](link) - description` line.
fn render_entry_line(e: &ConceptEntry) -> String {
    match &e.description {
        Some(d) => format!("* [{}]({}) - {}\n", e.title, e.link, d),
        None => format!("* [{}]({})\n", e.title, e.link),
    }
}

/// Splice the generated body into `old_content`'s managed region, preserving
/// prose outside the markers. When no markers exist, produce a fresh file:
/// - the bundle-root `index.md` keeps a leading `okf_version` frontmatter block
///   if the old file had one;
/// - otherwise a minimal `# Index` heading precedes the managed block.
fn splice_managed_region(old_content: &str, generated: &str, is_root: bool) -> String {
    let managed = format!("{INDEX_BEGIN}\n{generated}\n{INDEX_END}");

    if let (Some(begin), Some(end)) = (old_content.find(INDEX_BEGIN), old_content.find(INDEX_END))
        && begin < end
    {
        let before = &old_content[..begin];
        let after = &old_content[end + INDEX_END.len()..];
        let mut result = String::with_capacity(before.len() + managed.len() + after.len());
        result.push_str(before);
        result.push_str(&managed);
        result.push_str(after);
        return ensure_trailing_newline(&result);
    }

    // No (valid) markers → fresh file.
    let mut result = String::new();
    if is_root && let Some(fm) = extract_okf_version_frontmatter(old_content) {
        result.push_str(&fm);
        result.push('\n');
    }
    result.push_str("# Index\n\n");
    result.push_str(&managed);
    ensure_trailing_newline(&result)
}

/// Extract a leading `okf_version` frontmatter block (`---\nokf_version: ...\n---`)
/// from `content`, returning the reserialized block, or `None` when absent.
///
/// Reserved `index.md` files are frontmatter-free except for this single lone
/// key on the bundle root, so a minimal line scan of the leading `---` block is
/// sufficient (and avoids a re-read of the file).
fn extract_okf_version_frontmatter(content: &str) -> Option<String> {
    let rest = content.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    let block = &rest[..end];
    for line in block.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("okf_version:") {
            let v = value.trim().trim_matches(['"', '\'']);
            if !v.is_empty() {
                return Some(format!("---\nokf_version: \"{v}\"\n---"));
            }
        }
    }
    None
}

fn ensure_trailing_newline(s: &str) -> String {
    if s.ends_with('\n') {
        s.to_owned()
    } else {
        format!("{s}\n")
    }
}

// ---------------------------------------------------------------------------
// log
// ---------------------------------------------------------------------------

/// Prepend a dated entry to a scope-selectable `log.md`.
///
/// `target` selects which `log.md`:
/// - a directory → `<target>/log.md`
/// - a `log.md` file path → that file
/// - `None` → the bundle-root `log.md`
///
/// The entry is inserted under today's `YYYY-MM-DD` heading (newest first);
/// `action` optionally prefixes a bold action word (`**Update:** ...`).
pub fn run_log(
    dir: &Path,
    target: Option<&str>,
    message: &str,
    action: Option<&str>,
    apply: bool,
    format: Format,
) -> Result<CommandOutcome> {
    if message.trim().is_empty() {
        return Ok(CommandOutcome::UserError(format_error(
            format,
            "log message must not be empty",
            None,
            Some("pass --message \"...\""),
            None,
        )));
    }

    let rel_path = match resolve_log_target(dir, target) {
        Ok(p) => p,
        Err(msg) => {
            return Ok(CommandOutcome::UserError(format_error(
                format, &msg, target, None, None,
            )));
        }
    };
    let full = dir.join(&rel_path);

    let old_content = if full.is_file() {
        std::fs::read_to_string(&full).with_context(|| format!("failed to read {rel_path}"))?
    } else {
        String::new()
    };

    let today = hyalo_core::schema::today_iso8601();
    let entry_line = match action.map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) => format!("- **{a}:** {}", message.trim()),
        None => format!("- {}", message.trim()),
    };

    let new_content = prepend_log_entry(&old_content, &today, &entry_line);

    if apply {
        hyalo_core::fs_util::atomic_write(&full, new_content.as_bytes())
            .with_context(|| format!("failed to write {rel_path}"))?;
    }

    let payload = serde_json::json!({
        "command": "okf log",
        "apply": apply,
        "file": rel_path,
        "date": today,
        "entry": entry_line,
        "created": old_content.is_empty(),
    });
    Ok(CommandOutcome::success(payload.to_string()))
}

/// Insert `entry_line` under a `## <date>` heading in `content`, newest first.
///
/// - If the date heading already exists, the entry is prepended as the first
///   bullet under it (most recent within the day first).
/// - Otherwise a new date heading is inserted above any existing (older) date
///   sections, below an optional `# Log` title.
fn prepend_log_entry(content: &str, date: &str, entry_line: &str) -> String {
    let heading = format!("## {date}");

    if content.trim().is_empty() {
        return format!("# Log\n\n{heading}\n\n{entry_line}\n");
    }

    // If today's heading already exists, insert the entry right after it.
    if let Some(pos) = find_heading(content, &heading) {
        let insert_at = line_end_after(content, pos);
        let mut result = String::with_capacity(content.len() + entry_line.len() + 2);
        result.push_str(&content[..insert_at]);
        // Skip a single blank line following the heading, then insert.
        let rest = &content[insert_at..];
        let rest_trimmed = rest.strip_prefix('\n').unwrap_or(rest);
        result.push('\n');
        result.push_str(entry_line);
        result.push('\n');
        result.push_str(rest_trimmed);
        return ensure_trailing_newline(&result);
    }

    // No heading for today: insert a fresh section above the first existing
    // `## ` date section, or after the `# Log` title, or at the top.
    let block = format!("{heading}\n\n{entry_line}\n\n");
    if let Some(first_section) = content.find("\n## ") {
        let at = first_section + 1; // keep the preceding newline
        let mut result = String::with_capacity(content.len() + block.len());
        result.push_str(&content[..at]);
        result.push_str(&block);
        result.push_str(&content[at..]);
        return ensure_trailing_newline(&result);
    }
    if content.starts_with("## ") {
        let mut result = String::with_capacity(content.len() + block.len());
        result.push_str(&block);
        result.push_str(content);
        return ensure_trailing_newline(&result);
    }
    // Has a `# Log` (or other) title but no date sections yet: append after it.
    let sep = if content.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    ensure_trailing_newline(&format!("{content}{sep}{heading}\n\n{entry_line}\n"))
}

/// Find the byte offset of a heading line exactly matching `heading` (start of
/// line). Returns `None` when absent.
fn find_heading(content: &str, heading: &str) -> Option<usize> {
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        let trimmed = line.strip_suffix('\n').unwrap_or(line);
        if trimmed == heading {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

/// Byte offset just past the end of the line that starts at `line_start`.
fn line_end_after(content: &str, line_start: usize) -> usize {
    match content[line_start..].find('\n') {
        Some(rel) => line_start + rel + 1,
        None => content.len(),
    }
}

// ---------------------------------------------------------------------------
// path resolution / scoping
// ---------------------------------------------------------------------------

/// Resolve the optional `index` scope directory to a vault-relative prefix
/// (forward slashes, no trailing slash; `""` == whole vault). Rejects paths
/// that escape the vault.
fn resolve_scope(dir: &Path, scope: Option<&str>) -> std::result::Result<Option<String>, String> {
    let Some(raw) = scope else {
        return Ok(None);
    };
    let rel = normalize_vault_rel(dir, raw)?;
    if rel.is_empty() {
        return Ok(None);
    }
    let full = dir.join(&rel);
    if full.exists() && !full.is_dir() {
        return Err(format!("scope '{raw}' is not a directory"));
    }
    Ok(Some(rel))
}

/// Resolve a `log` target to a vault-relative `log.md` path (forward slashes).
fn resolve_log_target(dir: &Path, target: Option<&str>) -> std::result::Result<String, String> {
    let Some(raw) = target else {
        return Ok("log.md".to_owned());
    };
    let rel = normalize_vault_rel(dir, raw)?;
    if rel.is_empty() {
        return Ok("log.md".to_owned());
    }
    // A path ending in log.md → use directly. A directory → append log.md.
    let is_log_file = Path::new(&rel)
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("log.md"));
    if is_log_file {
        return Ok(rel);
    }
    let full = dir.join(&rel);
    if full.is_file() {
        return Err(format!(
            "target '{raw}' is a file but not a log.md; pass a directory or a log.md path"
        ));
    }
    Ok(format!("{rel}/log.md"))
}

/// Normalize a user-supplied path to a vault-relative form (forward slashes),
/// rejecting absolute paths and `..` traversal that would escape the vault.
fn normalize_vault_rel(dir: &Path, raw: &str) -> std::result::Result<String, String> {
    if raw.contains('\0') {
        return Err(format!("invalid path (contains null byte): {raw}"));
    }
    let mut normalized = raw.replace('\\', "/");
    // Strip the configured dir prefix if the user passed a CWD-relative path.
    if let Some(dir_name) = dir.file_name().and_then(|n| n.to_str())
        && let Some(stripped) = normalized.strip_prefix(&format!("{dir_name}/"))
    {
        normalized = stripped.to_owned();
    }
    let normalized = normalized.trim_start_matches("./").trim_end_matches('/');
    if normalized.is_empty() || normalized == "." {
        return Ok(String::new());
    }
    if normalized.starts_with('/')
        || Path::new(normalized).is_absolute()
        || discovery::has_parent_traversal(normalized)
    {
        return Err(format!("path resolves outside the vault boundary: {raw}"));
    }
    Ok(normalized.to_owned())
}

/// The vault-relative parent directory of `rel` (forward slashes; `""` == root).
fn parent_dir(rel: &str) -> String {
    match rel.rsplit_once('/') {
        Some((parent, _)) => parent.to_owned(),
        None => String::new(),
    }
}

/// Register `dir_rel` and every ancestor as a child of its own parent, so each
/// directory's index lists its immediate subdirectories.
fn register_ancestor_dirs(dir_rel: &str, child_dirs: &mut BTreeMap<String, Vec<String>>) {
    if dir_rel.is_empty() {
        return;
    }
    let mut current = dir_rel.to_owned();
    loop {
        let parent = parent_dir(&current);
        let children = child_dirs.entry(parent.clone()).or_default();
        if !children.contains(&current) {
            children.push(current.clone());
        }
        if parent.is_empty() {
            break;
        }
        current = parent;
    }
}

/// Whether directory `parent` is within the scope prefix.
fn dir_in_scope(parent: &str, scope_prefix: Option<&str>) -> bool {
    match scope_prefix {
        None => true,
        Some(prefix) => parent == prefix || parent.starts_with(&format!("{prefix}/")),
    }
}

// ---------------------------------------------------------------------------
// frontmatter key-order normalization
// ---------------------------------------------------------------------------

/// Reorder the keys of a YAML frontmatter mapping to the canonical OKF order
/// ([`OKF_KEY_ORDER`]), keeping any extra keys in their original relative order
/// after the known ones. Returns the reordered key list.
///
/// This operates on a list of `(key, position)` pairs so callers can reuse it
/// against whatever frontmatter representation they hold. Pure and total.
#[must_use]
pub fn normalize_key_order(keys: &[String]) -> Vec<String> {
    let mut ordered: Vec<String> = Vec::with_capacity(keys.len());
    // Known keys first, in canonical order.
    for known in OKF_KEY_ORDER {
        if let Some(k) = keys.iter().find(|k| k.as_str() == *known) {
            ordered.push(k.clone());
        }
    }
    // Then any remaining keys, in original order.
    for k in keys {
        if !OKF_KEY_ORDER.contains(&k.as_str()) {
            ordered.push(k.clone());
        }
    }
    ordered
}

/// Produce a tz-aware `timestamp` value for "now" in RFC 3339 form with a
/// `+00:00` UTC offset (the shape OKF sample bundles use). Reuses the crate's
/// epoch-seconds formatter and swaps the trailing `Z` for `+00:00`.
#[must_use]
pub fn now_timestamp_tz() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let iso = hyalo_core::index::format_iso8601(secs);
    // format_iso8601 ends with `Z`; OKF samples use `+00:00`.
    iso.strip_suffix('Z')
        .map_or(iso.clone(), |base| format!("{base}+00:00"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_order_canonical() {
        let keys: Vec<String> = ["timestamp", "title", "type", "extra", "tags"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let ordered = normalize_key_order(&keys);
        assert_eq!(ordered, vec!["type", "title", "tags", "timestamp", "extra"]);
    }

    #[test]
    fn key_order_preserves_unknown_order() {
        let keys: Vec<String> = ["zeta", "type", "alpha"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let ordered = normalize_key_order(&keys);
        assert_eq!(ordered, vec!["type", "zeta", "alpha"]);
    }

    #[test]
    fn timestamp_is_offset_form() {
        let ts = now_timestamp_tz();
        assert!(ts.ends_with("+00:00"), "got {ts}");
        assert!(hyalo_core::util::is_iso8601_datetime_tz(&ts), "got {ts}");
    }

    #[test]
    fn render_entry_with_and_without_description() {
        let e = ConceptEntry {
            concept_type: Some("Reference".into()),
            title: "Bitcoin".into(),
            link: "bitcoin.md".into(),
            description: Some("A currency".into()),
        };
        assert_eq!(
            render_entry_line(&e),
            "* [Bitcoin](bitcoin.md) - A currency\n"
        );
        let e2 = ConceptEntry {
            description: None,
            ..e
        };
        assert_eq!(render_entry_line(&e2), "* [Bitcoin](bitcoin.md)\n");
    }

    #[test]
    fn render_body_groups_by_type() {
        let entries = vec![
            ConceptEntry {
                concept_type: Some("Table".into()),
                title: "Blocks".into(),
                link: "blocks.md".into(),
                description: None,
            },
            ConceptEntry {
                concept_type: Some("Table".into()),
                title: "Accounts".into(),
                link: "accounts.md".into(),
                description: Some("acct".into()),
            },
            ConceptEntry {
                concept_type: None,
                title: "Loose".into(),
                link: "loose.md".into(),
                description: None,
            },
        ];
        let body = render_index_body(&entries, &[], "");
        // Type heading present; entries sorted by title within a type.
        assert!(body.contains("## Table"));
        let accounts = body.find("Accounts").unwrap();
        let blocks = body.find("Blocks").unwrap();
        assert!(accounts < blocks, "sorted by title");
        assert!(body.contains("## Other"));
        assert!(body.contains("* [Loose](loose.md)"));
    }

    #[test]
    fn render_body_lists_subdirs() {
        let body = render_index_body(&[], &["tables".into(), "references".into()], "");
        assert!(body.contains("## Subdirectories"));
        assert!(body.contains("* [tables](tables/index.md)"));
        assert!(body.contains("* [references](references/index.md)"));
    }

    #[test]
    fn splice_preserves_prose_outside_markers() {
        let old = format!(
            "# Index\n\nIntro prose.\n\n{INDEX_BEGIN}\nOLD LIST\n{INDEX_END}\n\nFooter note.\n"
        );
        let out = splice_managed_region(&old, "* [X](x.md)", false);
        assert!(out.contains("Intro prose."));
        assert!(out.contains("Footer note."));
        assert!(out.contains("* [X](x.md)"));
        assert!(!out.contains("OLD LIST"));
    }

    #[test]
    fn splice_fresh_root_keeps_okf_version() {
        let old = "---\nokf_version: \"0.1\"\n---\n\n# Old\n";
        let out = splice_managed_region(old, "* [X](x.md)", true);
        assert!(out.contains("okf_version: \"0.1\""));
        assert!(out.contains(INDEX_BEGIN));
        assert!(out.contains("* [X](x.md)"));
    }

    #[test]
    fn splice_is_idempotent() {
        let first = splice_managed_region("", "* [X](x.md)", false);
        let second = splice_managed_region(&first, "* [X](x.md)", false);
        assert_eq!(first, second);
    }

    #[test]
    fn log_creates_from_empty() {
        let out = prepend_log_entry("", "2026-07-17", "- Added blocks");
        assert!(out.starts_with("# Log\n"));
        assert!(out.contains("## 2026-07-17"));
        assert!(out.contains("- Added blocks"));
    }

    #[test]
    fn log_prepends_under_existing_date() {
        let old = "# Log\n\n## 2026-07-17\n\n- First entry\n";
        let out = prepend_log_entry(old, "2026-07-17", "- Second entry");
        let first = out.find("First entry").unwrap();
        let second = out.find("Second entry").unwrap();
        assert!(second < first, "newest first within a day: {out}");
        // Only one heading for the day.
        assert_eq!(out.matches("## 2026-07-17").count(), 1);
    }

    #[test]
    fn log_inserts_new_date_above_older() {
        let old = "# Log\n\n## 2026-07-10\n\n- Old entry\n";
        let out = prepend_log_entry(old, "2026-07-17", "- New entry");
        let new_h = out.find("## 2026-07-17").unwrap();
        let old_h = out.find("## 2026-07-10").unwrap();
        assert!(new_h < old_h, "newest date section first: {out}");
    }

    #[test]
    fn parent_dir_root_and_nested() {
        assert_eq!(parent_dir("a.md"), "");
        assert_eq!(parent_dir("tables/a.md"), "tables");
        assert_eq!(parent_dir("a/b/c.md"), "a/b");
    }

    #[test]
    fn scope_rejects_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(normalize_vault_rel(tmp.path(), "../escape").is_err());
        assert!(normalize_vault_rel(tmp.path(), "/etc/passwd").is_err());
    }

    #[test]
    fn log_target_directory_appends_log_md() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("tables")).unwrap();
        let p = resolve_log_target(tmp.path(), Some("tables")).unwrap();
        assert_eq!(p, "tables/log.md");
    }

    #[test]
    fn log_target_log_file_used_directly() {
        let tmp = tempfile::tempdir().unwrap();
        let p = resolve_log_target(tmp.path(), Some("tables/log.md")).unwrap();
        assert_eq!(p, "tables/log.md");
    }

    #[test]
    fn log_target_default_is_root() {
        let tmp = tempfile::tempdir().unwrap();
        let p = resolve_log_target(tmp.path(), None).unwrap();
        assert_eq!(p, "log.md");
    }

    #[test]
    fn dir_in_scope_matches_prefix() {
        assert!(dir_in_scope("tables", Some("tables")));
        assert!(dir_in_scope("tables/sub", Some("tables")));
        assert!(!dir_in_scope("references", Some("tables")));
        assert!(dir_in_scope("anything", None));
    }
}
