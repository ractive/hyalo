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

/// The kind of change a planned `index.md` regeneration represents. Reported in
/// the JSON payload's `action` field and the dry-run text notice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum IndexAction {
    /// The file did not exist and will be created.
    Create,
    /// The file exists with a managed region that will be updated in place.
    Update,
    /// The file exists WITHOUT markers; its body is preserved and the managed
    /// region is appended (non-destructive adoption).
    Adopt,
    /// The file exists without markers and `--replace` discards its body.
    Replace,
    /// The file exists with malformed markers (dangling/reversed/duplicate); it
    /// is left byte-identical and reported so the operator can fix it by hand.
    /// Rewriting across a malformed marker would delete hand-written prose
    /// (BUG-3), so the safe action is to skip.
    Skip,
}

impl IndexAction {
    fn as_str(self) -> &'static str {
        match self {
            IndexAction::Create => "create",
            IndexAction::Update => "update",
            IndexAction::Adopt => "adopt",
            IndexAction::Replace => "replace",
            IndexAction::Skip => "skip",
        }
    }
}

/// The state of the `okf:index` managed-region markers in an existing file.
///
/// Only [`MarkerState::Healthy`] and [`MarkerState::None`] are safe to splice;
/// everything else is malformed and must be skipped (never rewritten) so a
/// dangling/reversed marker can't cause the second apply to delete the prose
/// after it (BUG-3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MarkerState {
    /// No markers at all — a marker-less file (adopt/replace/create applies).
    None,
    /// Exactly one begin followed by exactly one end, in order — the region
    /// spans `(begin_offset, end_offset)` (end_offset is the start of END).
    Healthy(usize, usize),
    /// A begin with no matching end after it.
    DanglingBegin,
    /// An end with no begin before it.
    DanglingEnd,
    /// More than one begin and/or more than one end marker.
    Duplicate,
}

impl MarkerState {
    /// A human-readable description of the marker problem for warnings.
    fn problem(self) -> &'static str {
        match self {
            MarkerState::None | MarkerState::Healthy(..) => "",
            MarkerState::DanglingBegin => {
                "dangling `<!-- okf:index:begin -->` with no matching `<!-- okf:index:end -->`"
            }
            MarkerState::DanglingEnd => {
                "dangling `<!-- okf:index:end -->` with no preceding `<!-- okf:index:begin -->`"
            }
            MarkerState::Duplicate => "duplicate `okf:index` managed-region markers",
        }
    }
}

/// Classify the `okf:index` markers in `content`.
///
/// The check is anchored on the first *ordered* BEGIN→END pair, so a stray
/// mention of a marker in prose or a code fence above the real region does not
/// make an otherwise-healthy file look malformed (that tolerance is the whole
/// reason the splice is position-anchored). Given the first ordered pair:
///
/// - a genuine *second* region — a BEGIN that starts after the first END — is a
///   [`MarkerState::Duplicate`];
/// - otherwise the pair is [`MarkerState::Healthy`].
///
/// With no ordered pair, a lone BEGIN is [`MarkerState::DanglingBegin`], a lone
/// END (including the reversed `end … begin` case) is
/// [`MarkerState::DanglingEnd`], and nothing at all is [`MarkerState::None`].
fn classify_markers(content: &str) -> MarkerState {
    match find_ordered_pair(content) {
        Some((begin, end)) => {
            // A BEGIN starting after the first END is a second managed region —
            // a real duplicate, not a stray prose mention before the region.
            let after_region = end + INDEX_END.len();
            if content[after_region..].contains(INDEX_BEGIN) {
                MarkerState::Duplicate
            } else {
                MarkerState::Healthy(begin, end)
            }
        }
        None => {
            if content.contains(INDEX_BEGIN) {
                MarkerState::DanglingBegin
            } else if content.contains(INDEX_END) {
                MarkerState::DanglingEnd
            } else {
                MarkerState::None
            }
        }
    }
}

/// Find the first BEGIN and the first END that appears strictly after it,
/// returning `(begin_offset, end_offset)` where `end_offset` is the start of
/// the end marker. `None` when there is no such ordered pair.
fn find_ordered_pair(content: &str) -> Option<(usize, usize)> {
    content.find(INDEX_BEGIN).and_then(|begin| {
        content[begin + INDEX_BEGIN.len()..]
            .find(INDEX_END)
            .map(|rel_end| (begin, begin + INDEX_BEGIN.len() + rel_end))
    })
}

/// Result of planning a single `index.md` regeneration.
struct IndexPlan {
    /// Vault-relative path of the `index.md` (forward slashes).
    rel_path: String,
    /// The full new file content.
    new_content: String,
    /// The current on-disk content (empty string when the file is absent).
    old_content: String,
    /// The kind of change this plan represents.
    action: IndexAction,
    /// When [`IndexAction::Skip`], the marker problem that caused the skip
    /// (for the run summary + warning). Empty otherwise.
    skip_reason: &'static str,
}

impl IndexPlan {
    fn changed(&self) -> bool {
        self.new_content != self.old_content
    }
    fn is_skip(&self) -> bool {
        self.action == IndexAction::Skip
    }
}

/// Regenerate `index.md` files across the vault (or a scoped subtree).
///
/// `scope` optionally restricts regeneration to a single directory subtree
/// (vault-relative). `apply` writes changes; otherwise this is a dry run and
/// returns exit code 1 (via `exit_code_override`) when any `index.md` would
/// change — the CI drift signal.
#[allow(clippy::too_many_arguments)]
pub fn run_index(
    dir: &Path,
    scope: Option<&str>,
    apply: bool,
    replace: bool,
    ignore: &[String],
    case_insensitive: bool,
    active_profiles: &[String],
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

    let mode = if replace {
        AdoptMode::Replace
    } else {
        AdoptMode::Adopt
    };

    // Compile the `[okf] ignore` globs once. A bad pattern emits a warning and
    // disables filtering (fail-open) so we never silently drop content.
    let ignore_set = build_ignore_globset(ignore);

    let files = discovery::discover_files(dir).context("failed to scan vault for concepts")?;

    // Group concept files by their containing directory (vault-relative, forward
    // slashes; "" == bundle root). Reserved files (index.md/log.md) are skipped
    // as *entries* but their directories still get an index.
    let mut by_dir: BTreeMap<String, Vec<ConceptEntry>> = BTreeMap::new();
    // Ensure every directory that contains concepts (or is the root) gets an
    // index. Also record child subdirectories per directory.
    let mut child_dirs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Count of files skipped for unparseable frontmatter (surfaced in payload).
    let mut skipped_malformed = 0usize;

    for full in &files {
        let rel = discovery::relative_path(dir, full);
        let parent = parent_dir(&rel);

        // Files matching an `[okf] ignore` glob are neither entries nor index
        // targets — skip them entirely (don't even register their directory).
        if let Some(set) = &ignore_set
            && set.is_match(rel.replace('\\', "/"))
        {
            continue;
        }

        // Out-of-scope files must not influence the run at all: a malformed file
        // in `iterations/` must never break `okf index rdp` (RB-3). Skip before
        // reading frontmatter so a bad file outside scope is invisible.
        if !dir_in_scope(&parent, scope_prefix.as_deref()) {
            continue;
        }

        let file_name = Path::new(&rel)
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        // Register the directory (even if it only holds reserved files) so the
        // bundle root and every subdir gets an index entry key.
        by_dir.entry(parent.clone()).or_default();
        register_ancestor_dirs(&parent, &mut child_dirs);

        // Reserved files are outputs, never entries.
        if file_name == "index.md" || file_name == "log.md" {
            continue;
        }

        // Skip-and-warn on unparseable frontmatter instead of aborting the whole
        // run: one malformed concept must not stop every other index from being
        // generated (RB-3).
        match read_concept_entry(full, &rel, &parent) {
            Ok(entry) => {
                by_dir.entry(parent).or_default().push(entry);
            }
            Err(err) => {
                skipped_malformed += 1;
                crate::warn::warn(format!(
                    "skipping {rel}: {}",
                    crate::commands::terse_root_cause(&err)
                ));
            }
        }
    }

    // Always ensure the bundle root has an index, even in an empty vault.
    by_dir.entry(String::new()).or_default();

    // Ensure every directory that appears as a *child* subdirectory also has a
    // by_dir entry, even when it holds no concept files directly (only nested
    // subdirectories). Without this, a directory like `a/` containing only
    // `a/b/concept.md` never gets an `index.md` planned, even though its
    // parent's index links to `a/index.md`.
    for children in child_dirs.values() {
        for child in children {
            by_dir.entry(child.clone()).or_default();
        }
    }

    // Build a plan per directory in scope.
    let mut plans: Vec<IndexPlan> = Vec::new();
    for (parent, entries) in &by_dir {
        if !dir_in_scope(parent, scope_prefix.as_deref()) {
            continue;
        }
        let subdirs = child_dirs.get(parent).cloned().unwrap_or_default();
        let plan = plan_index(dir, parent, entries, &subdirs, mode, case_insensitive)?;
        plans.push(plan);
    }
    plans.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    // Files whose managed-region markers are malformed (dangling/reversed/
    // duplicate): reported and warned, never rewritten (BUG-3).
    let skipped_markers: Vec<&IndexPlan> = plans.iter().filter(|p| p.is_skip()).collect();
    for plan in &skipped_markers {
        crate::warn::warn(format!(
            "skipping {}: {} (nothing was written)",
            plan.rel_path, plan.skip_reason
        ));
    }

    // A malformed-marker file is byte-identical to its plan, so `changed()`
    // already excludes it; filter defensively on the action too.
    let changed: Vec<&IndexPlan> = plans
        .iter()
        .filter(|p| p.changed() && !p.is_skip())
        .collect();

    // Per-file write failures must not abort the run mid-way (BUG-11): warn,
    // record the target, and keep going. The exit code reflects the partial
    // failure without leaving the remaining files unwritten.
    let mut write_failures: Vec<String> = Vec::new();
    if apply {
        for plan in &changed {
            let full = dir.join(&plan.rel_path);
            if let Err(err) = hyalo_core::fs_util::atomic_write(&full, plan.new_content.as_bytes())
            {
                crate::warn::warn(format!(
                    "failed to write {}: {} — skipping this file, continuing",
                    plan.rel_path,
                    crate::commands::terse_root_cause(&err)
                ));
                write_failures.push(plan.rel_path.clone());
            }
        }
    }

    let mut results: Vec<serde_json::Value> = changed
        .iter()
        .filter(|p| !write_failures.contains(&p.rel_path))
        .map(|p| {
            let mut obj = serde_json::json!({
                "file": p.rel_path,
                "action": p.action.as_str(),
            });
            // On an adopt, report how many existing lines are being preserved so
            // `--dry-run` can print an explicit "preserving N existing lines"
            // notice, distinct from a plain update.
            if p.action == IndexAction::Adopt {
                obj["preserved_lines"] = serde_json::Value::from(count_lines(&p.old_content));
            }
            obj
        })
        .collect();

    // Report the skipped malformed-marker files so `--dry-run` surfaces them and
    // apply records why they were left alone.
    for plan in &skipped_markers {
        results.push(serde_json::json!({
            "file": plan.rel_path,
            "action": IndexAction::Skip.as_str(),
            "reason": plan.skip_reason,
        }));
    }

    let payload = serde_json::json!({
        "command": "okf index",
        "apply": apply,
        "scanned": plans.len(),
        "changed": changed.len() - write_failures.len(),
        "skipped_malformed": skipped_malformed,
        "skipped_markers": skipped_markers.len(),
        "write_failures": write_failures,
        "files": results,
        "hint": crate::commands::profile_lint_hint("okf", active_profiles, "validate bundle conformance"),
    });

    // Exit code:
    // - apply with any write failure → non-zero (partial failure, BUG-11).
    // - dry-run with drift (any changed file) → non-zero for CI.
    // Malformed-marker skips do not by themselves fail an apply (they are a
    // pre-existing hand-edit problem, not a generator failure), but they DO
    // count as drift in dry-run so CI surfaces them.
    let exit_override = if apply {
        if write_failures.is_empty() {
            None
        } else {
            Some(1)
        }
    } else if changed.is_empty() && skipped_markers.is_empty() {
        None
    } else {
        Some(1)
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
///
/// `case_insensitive` is the effective FS case-fold setting (from
/// `[links] case_insensitive`, auto-detected). When on, an existing
/// `INDEX.md`/`Index.md` on disk is recognized as *the* reserved file and its
/// on-disk casing is preserved on adopt — so a marker-less hand-curated
/// `INDEX.md` is never orphaned or destroyed by writing a sibling `index.md`.
fn plan_index(
    dir: &Path,
    parent: &str,
    entries: &[ConceptEntry],
    subdirs: &[String],
    mode: AdoptMode,
    case_insensitive: bool,
) -> Result<IndexPlan> {
    // Resolve the on-disk filename, honoring an existing case variant when the
    // filesystem is case-insensitive.
    let file_name = existing_index_file_name(dir, parent, case_insensitive);
    let rel_path = if parent.is_empty() {
        file_name.clone()
    } else {
        format!("{parent}/{file_name}")
    };
    let full = dir.join(&rel_path);

    // Impossible target: the path exists but is not a regular file (e.g. a
    // directory literally named `index.md`). Writing would fail at apply time;
    // detect it during planning so dry-run reports it instead of claiming
    // `create`, and apply skips it cleanly instead of erroring after the fact
    // (BUG-11). Report the same way as a malformed-marker skip.
    if full.exists() && !full.is_file() {
        return Ok(IndexPlan {
            rel_path,
            new_content: String::new(),
            old_content: String::new(),
            action: IndexAction::Skip,
            skip_reason: "target exists but is not a regular file (a directory named index.md?)",
        });
    }

    let old_content = if full.is_file() {
        std::fs::read_to_string(&full).with_context(|| format!("failed to read {rel_path}"))?
    } else {
        String::new()
    };

    let marker_state = classify_markers(&old_content);

    // Malformed markers (dangling/reversed/duplicate) are never rewritten:
    // splicing across a dangling marker would delete the hand-written prose
    // that follows it on a later apply (BUG-3). Leave the file byte-identical
    // and report the problem so the operator can fix it.
    if matches!(
        marker_state,
        MarkerState::DanglingBegin | MarkerState::DanglingEnd | MarkerState::Duplicate
    ) {
        return Ok(IndexPlan {
            rel_path,
            new_content: old_content.clone(),
            old_content,
            action: IndexAction::Skip,
            skip_reason: marker_state.problem(),
        });
    }

    let action = if old_content.is_empty() {
        IndexAction::Create
    } else if matches!(marker_state, MarkerState::Healthy(..)) {
        IndexAction::Update
    } else if mode == AdoptMode::Replace {
        IndexAction::Replace
    } else {
        IndexAction::Adopt
    };

    let generated = render_index_body(entries, subdirs, parent);
    let new_content = splice_managed_region(
        &old_content,
        &generated,
        parent.is_empty(),
        mode,
        marker_state,
    );

    Ok(IndexPlan {
        rel_path,
        new_content,
        old_content,
        action,
        skip_reason: "",
    })
}

/// Determine the on-disk filename to use for a directory's index. Defaults to
/// `index.md`; when `case_insensitive` is set and a different-cased variant
/// (e.g. `INDEX.md`) physically exists in the directory, that exact casing is
/// returned so the generator targets the real file instead of creating a
/// case-only-different sibling the FS would silently collide with.
fn existing_index_file_name(dir: &Path, parent: &str, case_insensitive: bool) -> String {
    const DEFAULT: &str = "index.md";
    if !case_insensitive {
        return DEFAULT.to_owned();
    }
    let dir_full = if parent.is_empty() {
        dir.to_path_buf()
    } else {
        dir.join(parent)
    };
    let Ok(read) = std::fs::read_dir(&dir_full) else {
        return DEFAULT.to_owned();
    };
    for dirent in read.flatten() {
        let name = dirent.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.eq_ignore_ascii_case(DEFAULT) {
            return name.to_owned();
        }
    }
    DEFAULT.to_owned()
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
            let _ = writeln!(
                out,
                "* [{}]({})",
                escape_link_text(name),
                encode_link_destination(&format!("{name}/index.md"))
            );
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
///
/// The title is escaped for link-text (`[`/`]` backslash-escaped), the
/// destination is angle-bracket-wrapped or percent-encoded when it contains
/// spaces or other link-breaking characters, and any newlines in the
/// description are collapsed to single spaces — so the emitted bullet is always
/// a valid single-line CommonMark link list item (BUG-10).
fn render_entry_line(e: &ConceptEntry) -> String {
    let title = escape_link_text(&e.title);
    let link = encode_link_destination(&e.link);
    match &e.description {
        Some(d) => format!("* [{title}]({link}) - {}\n", collapse_whitespace(d)),
        None => format!("* [{title}]({link})\n"),
    }
}

/// Escape a string for use as CommonMark link text (`[text]`). Backslash-escapes
/// `[` and `]` (which would otherwise open/close a nested bracket and break the
/// link) and collapses any embedded newlines to spaces so the bullet stays on
/// one line.
fn escape_link_text(text: &str) -> String {
    let collapsed = collapse_whitespace(text);
    let mut out = String::with_capacity(collapsed.len());
    for ch in collapsed.chars() {
        if ch == '[' || ch == ']' || ch == '\\' {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Encode a link destination so it is a valid CommonMark inline-link target.
///
/// A destination with no spaces, control chars, or unbalanced/`<`/`>` parens is
/// emitted verbatim. Otherwise it is wrapped in `<...>` (CommonMark's explicit
/// destination form), with any literal `<`, `>`, or `\` inside it
/// backslash-escaped. Newlines — which cannot appear even inside `<...>` — are
/// percent-encoded so the destination is never split across lines.
fn encode_link_destination(dest: &str) -> String {
    let needs_wrap = dest.chars().any(|c| {
        c.is_whitespace() || c.is_control() || c == '<' || c == '>' || c == '(' || c == ')'
    });
    if !needs_wrap {
        return dest.to_owned();
    }
    let mut out = String::with_capacity(dest.len() + 2);
    out.push('<');
    for ch in dest.chars() {
        match ch {
            '\n' => out.push_str("%0A"),
            '\r' => out.push_str("%0D"),
            '<' | '>' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out.push('>');
    out
}

/// Collapse every run of whitespace (including newlines) in `s` to a single
/// space and trim the ends. Used to keep generated bullets on one line.
fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How a marker-less existing file is handled when regenerating its managed
/// region.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AdoptMode {
    /// Non-destructive default: preserve the whole existing body and append the
    /// managed region after it. Never drops hand-written content.
    Adopt,
    /// Destructive: overwrite a marker-less file with a fresh managed file,
    /// discarding the existing body (opt-in via `--replace`).
    Replace,
}

/// The managed region block, wrapped so its inner content passes MD022
/// (blank lines around the headings the generated body starts/ends with).
///
/// Shape: `<begin>\n\n<generated>\n\n<end>` — the blank line after `begin`
/// separates it from the first `## ` heading in `generated`, and the blank
/// line before `end` separates it from the last list item. This is what stops
/// the `lint --fix` ↔ `okf index` revert ping-pong (RB-3): `lint --fix` no
/// longer wants to insert the blank lines the generator now already emits.
fn managed_block(generated: &str) -> String {
    // Normalize the generated body's surrounding whitespace so the wrap always
    // produces exactly one blank line after BEGIN and before END — regardless
    // of whether `generated` already carries trailing newlines. A stray double
    // blank line here is what MD012 (`lint --fix`) would strip, re-introducing
    // the drift ping-pong this wrapping is meant to kill.
    let body = generated.trim_matches('\n');
    format!("{INDEX_BEGIN}\n\n{body}\n\n{INDEX_END}")
}

/// Splice the generated body into `old_content`'s managed region, preserving
/// prose outside the markers.
///
/// `marker_state` is the pre-classified marker arrangement (from
/// [`classify_markers`]). Only [`MarkerState::Healthy`] and [`MarkerState::None`]
/// reach here — malformed states are skipped before planning — so this function
/// splices a healthy region in place or, absent markers, adopts/replaces/creates.
///
/// When markers already exist, the region between them is replaced in place.
/// When no markers exist:
/// - [`AdoptMode::Adopt`] (default) preserves the entire existing body and
///   appends the managed region after it — never dropping content;
/// - [`AdoptMode::Replace`] discards the body and writes a fresh managed file.
///
/// A fresh/replaced file gets a leading `okf_version` frontmatter block on the
/// bundle root (if the old file had one) and a minimal `# Index` heading.
fn splice_managed_region(
    old_content: &str,
    generated: &str,
    is_root: bool,
    mode: AdoptMode,
    marker_state: MarkerState,
) -> String {
    let managed = managed_block(generated);

    if let MarkerState::Healthy(begin, end) = marker_state {
        let before = &old_content[..begin];
        let after = &old_content[end + INDEX_END.len()..];
        let mut result = String::with_capacity(before.len() + managed.len() + after.len());
        result.push_str(before);
        result.push_str(&managed);
        result.push_str(after);
        return ensure_trailing_newline(&result);
    }

    // No markers. Non-destructive adopt: keep the existing body verbatim and
    // append the managed region after it. Only --replace discards the body.
    if mode == AdoptMode::Adopt && !old_content.trim().is_empty() {
        let mut result = String::with_capacity(old_content.len() + managed.len() + 2);
        result.push_str(old_content);
        // Guarantee a blank line between the preserved body and the region so
        // the appended `<begin>` marker sits on its own paragraph (MD022 safe).
        if !result.ends_with("\n\n") {
            while result.ends_with('\n') {
                result.pop();
            }
            result.push_str("\n\n");
        }
        result.push_str(&managed);
        return ensure_trailing_newline(&result);
    }

    // Fresh file (empty on disk, or --replace on a marker-less file).
    let mut result = String::new();
    if is_root && let Some(fm) = extract_okf_version_frontmatter(old_content) {
        result.push_str(&fm);
        result.push('\n');
    }
    result.push_str("# Index\n\n");
    result.push_str(&managed);
    ensure_trailing_newline(&result)
}

/// Count of lines in `s` (used to describe how many existing lines an adopt
/// preserves). An empty string has zero lines.
fn count_lines(s: &str) -> usize {
    if s.is_empty() { 0 } else { s.lines().count() }
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

/// Build a [`globset::GlobSet`] from `[okf] ignore` patterns. Each pattern is a
/// vault-relative glob (`_template/**`, `test/fixture-vault/**`) matched against
/// forward-slash paths. Returns `None` when the list is empty or any pattern is
/// invalid (fail-open with a warning — never silently drop files).
fn build_ignore_globset(patterns: &[String]) -> Option<globset::GlobSet> {
    use globset::{GlobBuilder, GlobSetBuilder};
    if patterns.is_empty() {
        return None;
    }
    let mut builder = GlobSetBuilder::new();
    let mut build_failed = false;
    for pat in patterns {
        match GlobBuilder::new(pat)
            .literal_separator(true)
            .backslash_escape(true)
            .build()
        {
            Ok(g) => {
                builder.add(g);
            }
            Err(e) => {
                crate::warn::warn(format!("invalid [okf] ignore pattern {pat:?}: {e}"));
                build_failed = true;
            }
        }
    }
    if build_failed {
        return None;
    }
    builder.build().ok()
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
    active_profiles: &[String],
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

    // `--action ""` is a user error, mirroring `--message ""` — an empty action
    // word is never intended and would silently degrade to a plain bullet
    // (BUG-12/consistency). An omitted `--action` is fine (`None`).
    if let Some(a) = action
        && a.trim().is_empty()
    {
        return Ok(CommandOutcome::UserError(format_error(
            format,
            "log action must not be empty",
            None,
            Some("omit --action, or pass a non-empty word like --action Update"),
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

    // The target must be a regular file or absent — a directory named `log.md`
    // (or any non-file at that path) can't be written; reject it in both
    // dry-run and apply so they agree (BUG-11 parity for `okf log`).
    if full.exists() && !full.is_file() {
        return Ok(CommandOutcome::UserError(format_error(
            format,
            &format!("target '{rel_path}' exists but is not a regular file"),
            target,
            None,
            None,
        )));
    }

    let old_content = if full.is_file() {
        std::fs::read_to_string(&full).with_context(|| format!("failed to read {rel_path}"))?
    } else {
        String::new()
    };

    let today = hyalo_core::schema::today_iso8601();
    // Collapse the message onto a single logical bullet: continuation lines are
    // indented two spaces so they stay part of the list item instead of
    // producing an unindented paragraph or a literal `## fake heading` that
    // corrupts the log structure (BUG-14).
    let message_body = indent_continuation(message.trim());
    let entry_line = match action.map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) => format!("- **{a}:** {message_body}"),
        None => format!("- {message_body}"),
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
        "hint": crate::commands::profile_lint_hint("okf", active_profiles, "validate bundle conformance"),
    });
    Ok(CommandOutcome::success(payload.to_string()))
}

/// Indent the continuation lines of a (possibly multi-line) log message so the
/// whole thing renders as one Markdown list item.
///
/// The first line stays as-is (it follows the `- ` / `- **Action:** ` marker);
/// every subsequent non-empty line is prefixed with two spaces so it is a lazy
/// continuation of the bullet rather than a new block. Blank lines and lines
/// that would start a heading (`#`) or a new list marker are still indented, so
/// a `## fake heading` in the message can never break out of the list (BUG-14).
/// Trailing `\r` from CRLF input is stripped.
fn indent_continuation(message: &str) -> String {
    let mut lines = message.split('\n');
    let Some(first) = lines.next() else {
        return String::new();
    };
    let mut out = String::with_capacity(message.len());
    out.push_str(first.strip_suffix('\r').unwrap_or(first));
    for line in lines {
        let line = line.strip_suffix('\r').unwrap_or(line);
        out.push('\n');
        if line.trim().is_empty() {
            // Keep an empty continuation line empty (no trailing whitespace) so
            // it doesn't trip trailing-space lint, but still inside the item.
            continue;
        }
        out.push_str("  ");
        out.push_str(line);
    }
    out
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
        // Skip a single blank line following the heading, then insert. The
        // blank line is itself an empty line terminated by `\n` or `\r\n`.
        let rest = &content[insert_at..];
        let rest_trimmed = rest
            .strip_prefix('\n')
            .or_else(|| rest.strip_prefix("\r\n"))
            .unwrap_or(rest);
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
/// line). Returns `None` when absent. Tolerates CRLF line endings (the
/// trailing `\r` is trimmed before comparison) so existing headings are still
/// found in a `log.md` that was checked out or hand-edited with CRLF.
fn find_heading(content: &str, heading: &str) -> Option<usize> {
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        let trimmed = strip_line_ending(line);
        if trimmed == heading {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

/// Strip a trailing `\n` or `\r\n` line ending from `line`.
fn strip_line_ending(line: &str) -> &str {
    line.strip_suffix('\n')
        .map_or(line, |s| s.strip_suffix('\r').unwrap_or(s))
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
    if !full.exists() {
        // A typo'd scope must not vacuously pass a CI freshness check (BUG-13):
        // `okf index no-such-dir` used to report `0 scanned`, exit 0. Reject it.
        return Err(format!(
            "scope '{raw}' does not exist (no such directory in the vault)"
        ));
    }
    if !full.is_dir() {
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
    // A directory target must already exist. Otherwise dry-run and apply
    // disagree: dry-run happily reports `(created)` (exit 0) while apply fails
    // to create a temp file in the missing parent (exit 2) — BUG-15. Reject a
    // nonexistent directory with a hint to create it first.
    if !full.exists() {
        return Err(format!(
            "target directory '{raw}' does not exist; create it first (mkdir), then log into it"
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

    /// Test shim: classify `old` and splice, mirroring what `plan_index` does
    /// for the healthy/marker-less cases (malformed states never reach splice).
    fn splice_for_test(old: &str, generated: &str, is_root: bool, mode: AdoptMode) -> String {
        splice_managed_region(old, generated, is_root, mode, classify_markers(old))
    }

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
        let out = splice_for_test(&old, "* [X](x.md)", false, AdoptMode::Adopt);
        assert!(out.contains("Intro prose."));
        assert!(out.contains("Footer note."));
        assert!(out.contains("* [X](x.md)"));
        assert!(!out.contains("OLD LIST"));
    }

    #[test]
    fn splice_ignores_end_marker_mentioned_in_prose_before_begin() {
        // A stray mention of the end-marker text above the real managed
        // region must not be mistaken for the closing marker.
        let old = format!(
            "# Index\n\nSee marker `{INDEX_END}` in the docs.\n\n{INDEX_BEGIN}\nOLD LIST\n{INDEX_END}\n\nFooter note.\n"
        );
        let out = splice_for_test(&old, "* [X](x.md)", false, AdoptMode::Adopt);
        assert!(
            out.contains("See marker"),
            "prose before begin marker must survive: {out}"
        );
        assert!(out.contains("Footer note."), "footer must survive: {out}");
        assert!(out.contains("* [X](x.md)"));
        assert!(!out.contains("OLD LIST"));
    }

    #[test]
    fn splice_fresh_root_keeps_okf_version() {
        let old = "---\nokf_version: \"0.1\"\n---\n\n# Old\n";
        // --replace on the marker-less root: discards the `# Old` body but keeps
        // the okf_version frontmatter.
        let out = splice_for_test(old, "* [X](x.md)", true, AdoptMode::Replace);
        assert!(out.contains("okf_version: \"0.1\""));
        assert!(out.contains(INDEX_BEGIN));
        assert!(out.contains("* [X](x.md)"));
        assert!(!out.contains("# Old"), "replace discards body: {out}");
    }

    #[test]
    fn splice_is_idempotent() {
        let first = splice_for_test("", "* [X](x.md)", false, AdoptMode::Adopt);
        let second = splice_for_test(&first, "* [X](x.md)", false, AdoptMode::Adopt);
        assert_eq!(first, second);
    }

    #[test]
    fn splice_adopts_marker_less_body() {
        // A marker-less hand-written file: adopt must preserve every line and
        // append the managed region after it.
        let old = "# Curated\n\nHand-written intro.\n\n- manual item\n";
        let out = splice_for_test(old, "* [X](x.md)", false, AdoptMode::Adopt);
        assert!(out.contains("# Curated"), "heading preserved: {out}");
        assert!(
            out.contains("Hand-written intro."),
            "prose preserved: {out}"
        );
        assert!(
            out.contains("- manual item"),
            "manual list preserved: {out}"
        );
        assert!(out.contains(INDEX_BEGIN));
        assert!(out.contains("* [X](x.md)"));
        // A blank line must precede the appended begin marker (MD022).
        assert!(
            out.contains(&format!("\n\n{INDEX_BEGIN}")),
            "blank line before appended region: {out}"
        );
    }

    #[test]
    fn splice_adopt_is_idempotent_second_pass() {
        // First adopt appends the region; a second pass finds the markers and
        // updates in place (no duplicate region, no re-appended body).
        let old = "# Curated\n\nProse.\n";
        let first = splice_for_test(old, "* [X](x.md)", false, AdoptMode::Adopt);
        let second = splice_for_test(&first, "* [X](x.md)", false, AdoptMode::Adopt);
        assert_eq!(first, second, "adopt then update is idempotent");
        assert_eq!(
            second.matches(INDEX_BEGIN).count(),
            1,
            "only one managed region: {second}"
        );
    }

    #[test]
    fn splice_replace_discards_marker_less_body() {
        let old = "# Curated\n\nHand-written that --replace throws away.\n";
        let out = splice_for_test(old, "* [X](x.md)", false, AdoptMode::Replace);
        assert!(!out.contains("Hand-written"), "replace drops body: {out}");
        assert!(out.contains("# Index"));
        assert!(out.contains(INDEX_BEGIN));
    }

    #[test]
    fn managed_block_has_md022_blank_lines() {
        let block = managed_block("## Group\n\n* [X](x.md)");
        assert!(
            block.starts_with(&format!("{INDEX_BEGIN}\n\n")),
            "blank line after begin: {block}"
        );
        assert!(
            block.ends_with(&format!("\n\n{INDEX_END}")),
            "blank line before end: {block}"
        );
    }

    #[test]
    fn count_lines_counts() {
        assert_eq!(count_lines(""), 0);
        assert_eq!(count_lines("one line"), 1);
        assert_eq!(count_lines("a\nb\nc\n"), 3);
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
    fn log_prepends_under_existing_date_crlf() {
        // A log.md checked out or hand-edited with CRLF line endings must
        // still be recognized as having today's heading already, instead of
        // creating a duplicate `## <date>` section.
        let old = "# Log\r\n\r\n## 2026-07-17\r\n\r\n- First entry\r\n";
        let out = prepend_log_entry(old, "2026-07-17", "- Second entry");
        assert_eq!(
            out.matches("## 2026-07-17").count(),
            1,
            "must not duplicate the date heading on CRLF input: {out}"
        );
        let first = out.find("First entry").unwrap();
        let second = out.find("Second entry").unwrap();
        assert!(second < first, "newest first within a day: {out}");
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

    // -----------------------------------------------------------------------
    // Iteration 176 — marker classification (BUG-3)
    // -----------------------------------------------------------------------

    #[test]
    fn classify_none_and_healthy() {
        assert_eq!(
            classify_markers("# Index\n\njust prose\n"),
            MarkerState::None
        );
        let ok = format!("# Index\n\n{INDEX_BEGIN}\n* [x](x.md)\n{INDEX_END}\n");
        assert!(matches!(classify_markers(&ok), MarkerState::Healthy(..)));
    }

    #[test]
    fn classify_dangling_begin() {
        // Prose + a lone begin marker (the BUG-3 repro).
        let s = format!("# Index\n\nhand prose\n\n{INDEX_BEGIN}\nsome list\n");
        assert_eq!(classify_markers(&s), MarkerState::DanglingBegin);
    }

    #[test]
    fn classify_dangling_end() {
        let s = format!("# Index\n\n{INDEX_END}\nhand prose\n");
        assert_eq!(classify_markers(&s), MarkerState::DanglingEnd);
    }

    #[test]
    fn classify_reversed_is_malformed() {
        // END before BEGIN: one of each, but no ordered pair (the BEGIN has no
        // END after it). Classified as a dangling begin — malformed either way,
        // so the file is skipped, never rewritten.
        let s = format!("{INDEX_END}\n\n{INDEX_BEGIN}\n");
        assert_eq!(classify_markers(&s), MarkerState::DanglingBegin);
    }

    #[test]
    fn classify_duplicate() {
        let s = format!("{INDEX_BEGIN}\na\n{INDEX_END}\n\n{INDEX_BEGIN}\nb\n{INDEX_END}\n");
        assert_eq!(classify_markers(&s), MarkerState::Duplicate);
    }

    #[test]
    fn dangling_begin_plan_leaves_file_byte_identical() {
        // The BUG-3 data-loss repro: a file with prose after a dangling begin
        // marker must be planned as a Skip whose new content equals the old.
        let tmp = tempfile::tempdir().unwrap();
        let content = format!("# Index\n\nHAND PROSE\n\n{INDEX_BEGIN}\nstale list\n");
        std::fs::write(tmp.path().join("index.md"), &content).unwrap();
        let plan = plan_index(tmp.path(), "", &[], &[], AdoptMode::Adopt, false).unwrap();
        assert_eq!(plan.action, IndexAction::Skip);
        assert_eq!(plan.new_content, plan.old_content);
        assert!(plan.new_content.contains("HAND PROSE"));
        assert!(!plan.changed(), "malformed-marker file must not be changed");
    }

    #[test]
    fn impossible_target_directory_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        // A *directory* literally named index.md is an impossible write target.
        std::fs::create_dir_all(tmp.path().join("index.md")).unwrap();
        let plan = plan_index(tmp.path(), "", &[], &[], AdoptMode::Adopt, false).unwrap();
        assert_eq!(plan.action, IndexAction::Skip);
        assert!(!plan.changed());
        assert!(plan.skip_reason.contains("not a regular file"));
    }

    // -----------------------------------------------------------------------
    // Iteration 176 — CommonMark-valid links (BUG-10)
    // -----------------------------------------------------------------------

    #[test]
    fn encode_destination_wraps_spaces() {
        assert_eq!(encode_link_destination("blocks.md"), "blocks.md");
        assert_eq!(
            encode_link_destination("blocks table.md"),
            "<blocks table.md>"
        );
        assert_eq!(
            encode_link_destination("spaced dir/index.md"),
            "<spaced dir/index.md>"
        );
    }

    #[test]
    fn encode_destination_escapes_angles_and_newlines() {
        assert_eq!(encode_link_destination("a<b>.md"), "<a\\<b\\>.md>");
        assert_eq!(encode_link_destination("a\nb.md"), "<a%0Ab.md>");
    }

    #[test]
    fn escape_link_text_escapes_brackets() {
        assert_eq!(escape_link_text("plain"), "plain");
        assert_eq!(escape_link_text("a [test] b"), "a \\[test\\] b");
        assert_eq!(escape_link_text("line1\nline2"), "line1 line2");
    }

    #[test]
    fn render_entry_line_is_commonmark_valid() {
        let e = ConceptEntry {
            concept_type: Some("Table".into()),
            title: "Blöcke [Übersicht] 🎉".into(),
            link: "blocks table.md".into(),
            description: Some("multi\nline\ndesc".into()),
        };
        let line = render_entry_line(&e);
        assert_eq!(
            line,
            "* [Blöcke \\[Übersicht\\] 🎉](<blocks table.md>) - multi line desc\n"
        );
        // Single line, exactly one `](` link.
        assert_eq!(line.matches('\n').count(), 1);
    }

    // -----------------------------------------------------------------------
    // Iteration 176 — log multiline handling (BUG-14)
    // -----------------------------------------------------------------------

    #[test]
    fn indent_continuation_indents_and_neutralizes_headings() {
        let msg = "first line\n## fake heading\nmore";
        let out = indent_continuation(msg);
        assert_eq!(out, "first line\n  ## fake heading\n  more");
        // The would-be heading is now indented, so it can't break the list.
        assert!(!out.contains("\n## "));
    }

    #[test]
    fn indent_continuation_single_line_unchanged() {
        assert_eq!(indent_continuation("just one line"), "just one line");
    }

    #[test]
    fn indent_continuation_crlf() {
        assert_eq!(indent_continuation("a\r\nb"), "a\n  b");
    }
}
