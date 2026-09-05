//! Generates drill-down command hints for CLI output.
//!
//! When `--hints` is enabled, each command's output includes suggested next
//! commands. All hints are concrete, executable strings — no templates or
//! placeholders.

/// Maximum number of hints to return from any generator.
const MAX_HINTS: usize = 5;

/// Wall-clock threshold for the slow-query hint (milliseconds).
///
/// Rationale: shorter than the human "this is slow" threshold (~1 s) with
/// margin; longer than typical disk scans on small vaults (~100 ms).
pub(crate) const SLOW_QUERY_THRESHOLD_MS: u64 = 500;

/// File count threshold for the large-vault summary hint.
///
/// Rationale: vaults above this size see measurable benefit from a snapshot
/// index; below it the disk scan is fast enough not to warrant the hint.
pub(crate) const LARGE_VAULT_FILE_COUNT: u64 = 500;

/// Prefix used by lint for frontmatter parse errors. Shared between
/// `commands::lint` and the hint generator to avoid brittle string coupling.
pub(crate) const PARSE_ERROR_PREFIX: &str = "could not parse frontmatter";

/// Minimum broken-link count before the "these look like site URLs" diagnostic
/// can fire. Below this the vault is small enough that broken links are more
/// likely genuine typos worth a `links fix` suggestion.
const SITE_URL_MIN_BROKEN: u64 = 500;

/// Percentage of links that must be broken for the site-URL diagnostic to fire.
/// At ~100% broken on a link-heavy vault the links are almost certainly
/// unresolved absolute site URLs, not fixable file references.
const SITE_URL_BROKEN_PERCENT: u64 = 95;

/// A zero-result `--property K~=REGEX` query whose regex nevertheless matches
/// body prose somewhere in the vault (iteration 258).
///
/// The canonical case is `--property 'title~=/DEC-25/'` run against a decision
/// log whose `DEC-NNN` identifiers are `##` body headings rather than
/// frontmatter titles: the filter is correct, the empty answer is correct, and
/// the thing the caller was actually after is one flag away. `find` sets this
/// only after a bounded probe *confirmed* a body match, so the hint reports a
/// fact rather than speculating.
#[derive(Debug, Clone)]
pub struct BodySearchSuggestion {
    /// Property key whose regex filter matched nothing (`title`, …).
    pub key: String,
    /// Regex source to hand to `find -e`, exactly as the user wrote it.
    pub pattern: String,
}

/// A single drill-down hint: a concrete command plus a short human-readable description.
#[derive(Debug, Clone)]
pub struct Hint {
    pub(crate) description: String,
    pub(crate) cmd: String,
    /// `true` when running `cmd` would modify the vault or `.hyalo.toml`.
    ///
    /// Derived from the command text by
    /// [`crate::mutation::command_line_writes`] rather than set by each hint
    /// builder, so a new hint cannot be added *without* being classified. The
    /// `views set …` suggestion sat unmarked among read-only drill-downs until
    /// iter-201; renderers now separate the two (see
    /// [`crate::output::format_envelope`]).
    pub(crate) writes: bool,
}

impl Hint {
    pub(crate) fn new(description: impl Into<String>, cmd: String) -> Self {
        let writes = crate::mutation::command_line_writes(&cmd);
        Self {
            description: description.into(),
            cmd,
            writes,
        }
    }

    /// Advice-only hint with no follow-up command. JSON consumers see
    /// `cmd: ""`; text renderers special-case the empty-cmd shape so the
    /// `  -> <cmd>  # <desc>` layout collapses to `  -> <desc>`.
    fn without_cmd(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            cmd: String::new(),
            writes: false,
        }
    }
}

/// Identifies which command produced the output.
pub enum HintSource {
    Summary,
    PropertiesSummary,
    TagsSummary,
    Find,
    Set,
    Remove,
    Append,
    Read,
    Backlinks,
    Mv,
    TaskRead,
    TaskToggle,
    TaskSetStatus,
    LinksFix,
    LinksAuto,
    CreateIndex,
    DropIndex,
    Lint,
    Types {
        subcommand: Option<String>,
    },
    New {
        file: String,
    },
    /// `hyalo okf index` — suggest validating conformance (and applying on drift).
    OkfIndex,
    /// `hyalo okf log` — suggest validating conformance.
    OkfLog,
    /// `hyalo views list` (iter-210). Listing saved views used to be a
    /// navigation dead end: the whole point of a view is to run it, and the
    /// listing never said how.
    ViewsList,
    /// `hyalo lint-rules list` (iter-210). Same dead end — the catalog told
    /// you a rule exists but not how to inspect, disable or lint with it.
    LintRulesList,
    /// `hyalo lint-rules show <ID>` (NEW-18, dogfood pre3). Was also a dead
    /// end despite inspecting one specific, actionable rule.
    LintRulesShow,
}

/// One distinct frontmatter value a zero-result `find` scan saw for a filtered
/// property key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedValue {
    /// How the value reads in a hint — a scalar as written, a list as
    /// `[[Published]]`, a YAML null as `null`.
    pub rendered: String,
    /// How many files carry this value.
    pub count: u64,
    /// Whether the value can be typed back into `--property K=V`. Only scalars
    /// can, so only they are offered as did-you-mean corrections.
    pub typeable: bool,
}

/// What a zero-result `find` scan saw for one filtered property key.
///
/// iter-274 (BUG-17): the zero-result hint used to say "No file has a `status`
/// property" whenever it collected no *typeable* value — which is exactly what
/// happens on a vault whose `status:` values are YAML nulls or wikilink lists
/// (`status: [[Published]]` is a nested flow sequence, not a scalar). Key-absent
/// and value-absent are different diagnoses and now read differently.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObservedProperty {
    /// Files carrying the key at all, whatever its value — a YAML null and a
    /// list both count.
    pub files: u64,
    /// Distinct values, most frequent first, capped by the scan.
    pub values: Vec<ObservedValue>,
}

impl ObservedProperty {
    /// The values that can be typed back into `--property K=V`.
    #[must_use]
    pub fn typeable_values(&self) -> Vec<String> {
        self.values
            .iter()
            .filter(|v| v.typeable)
            .map(|v| v.rendered.clone())
            .collect()
    }
}

/// Which snapshot index a `find` query used, for re-emission in derived hints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum FindIndexHint {
    /// No index was active — hints scan the vault (the common case).
    #[default]
    None,
    /// Bare `--index`: the default vault `.hyalo-index`.
    Default,
    /// Explicit `--index-file <path>` at a non-default location.
    File(String),
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
    pub glob: Vec<String>,
    /// Explicit `--format` from CLI (not from config).
    pub format: Option<String>,
    /// Explicit `--hints` from CLI (not from config).
    pub hints: bool,
    /// Wall-clock elapsed time for the command body (set after dispatch).
    /// Used by the slow-query hint; `None` means not yet measured.
    pub elapsed_ms: Option<u64>,
    /// Whether `--quiet` / `-q` was passed.  Suppresses the slow-query hint.
    pub quiet: bool,
    /// Whether an `--index` / `--index-file` snapshot was active for this run.
    /// Suppresses index-suggestion hints when already using an index.
    pub has_index: bool,
    /// `true` when a `.hyalo-index` snapshot already exists in the vault dir
    /// but this run did not opt into it.
    ///
    /// iter-267 (UX-18): the slow-query hint always said `create-index`, even
    /// on a vault that had been indexed minutes earlier — telling the reader
    /// to rebuild what they already had instead of to pass `--index`.
    pub snapshot_on_disk: bool,
    /// Vault-relative path the `find` PATTERN itself names, when the pattern
    /// is a `.md` path that exists in the vault.
    ///
    /// iter-267 (UX-3, reverse direction): `hyalo find notes/todo.md` is a
    /// body search for the literal text `notes/todo.md`, which is almost never
    /// what the caller meant. The results are not wrong, so this is a hint,
    /// not an error — `--file` is one line away.
    pub pattern_names_a_file: Option<String>,
    // Find context
    pub fields: Vec<String>,
    pub sort: Option<String>,
    pub has_limit: bool,
    pub has_body_search: bool,
    /// The actual body-search pattern string, when a body search was issued.
    pub body_pattern: Option<String>,
    pub has_regex_search: bool,
    pub property_filters: Vec<String>,
    pub tag_filters: Vec<String>,
    pub task_filter: Option<String>,
    pub file_targets: Vec<String>,
    pub section_filters: Vec<String>,
    /// `--broken-links` graph filter was active.
    pub broken_links_filter: bool,
    /// `--orphan` graph filter was active.
    pub orphan_filter: bool,
    /// `--dead-end` graph filter was active.
    pub dead_end_filter: bool,
    /// `--reverse` / `--desc` was active (paired with `sort` when preserved).
    pub reverse: bool,
    /// `--title` substring/regex filter value, when active.
    pub title_filter: Option<String>,
    /// Active snapshot index for a `find` query, preserved into derived `find`
    /// hints so they query the same index rather than silently rescanning the
    /// vault (BUG-7 audit: `--index-file` was a dropped flag). `Default` means
    /// bare `--index` (vault `.hyalo-index`); `File(path)` means an explicit
    /// `--index-file <path>`; `None` means no index was active.
    pub find_index: FindIndexHint,
    /// Set when the query was produced by `--view <name>`; suppresses the
    /// "save as view" hint to avoid suggesting the user save a view they
    /// already have.
    pub view_name: Option<String>,
    /// Task selector used: "all", "section:<name>", or "lines" (for multi-line).
    /// `None` means single-line or no task context.
    pub task_selector: Option<String>,
    /// `read` already narrowed its output with `--section` or `--lines`
    /// (iter-252). Suppresses the large-file "read less" hint, which would
    /// otherwise tell a caller to do what they just did.
    pub read_narrowed: bool,
    // Mutation context
    pub dry_run: bool,
    // Index context
    pub index_path: Option<String>,
    // Links-auto context (for replaying the exact preview scope in hints)
    pub auto_link_file: Option<String>,
    pub auto_link_min_length: Option<usize>,
    pub auto_link_exclude_titles: Vec<String>,
    // Lint-specific context (for smarter hint generation)
    /// Whether `--fix` was passed (not just `--fix --dry-run`).
    pub lint_is_fix: bool,
    /// Single rule filter (`--rule`).
    pub lint_rule: Option<String>,
    /// Rule prefix filter (`--rule-prefix`).
    pub lint_rule_prefix: Option<String>,
    /// Rules to fix (`--fix-rule`, repeatable).
    pub lint_fix_rules: Vec<String>,
    /// Whether the `okf` conformance profile is already active via
    /// `[lint] profiles` in `.hyalo.toml`. When true the `okf` validate hint
    /// drops the redundant `--profile okf` flag (plain `hyalo lint` runs it).
    pub okf_profile_active: bool,
    /// Frontmatter values observed for each property key named by a
    /// `--property K=V` filter, collected by the `find` scan when it matched
    /// nothing (iter-251). Feeds the zero-result did-you-mean and the
    /// "values of `K` in this vault" hint; empty on every non-empty result.
    pub observed_property_values: std::collections::BTreeMap<String, ObservedProperty>,
    /// Set when a zero-result `find` carried a `--property K~=RE` filter whose
    /// regex matches body text somewhere in the vault (iteration 258).
    /// Confirmed by a bounded body probe on the zero-result path, never
    /// guessed; `None` on every non-empty result and whenever the probe found
    /// nothing within its budget.
    pub body_search_suggestion: Option<BodySearchSuggestion>,
}

/// Common global flags captured once per command dispatch and threaded into
/// every `HintContext`. Avoids repeating the same three field assignments in
/// every `match` arm of `run.rs`.
pub struct CommonHintFlags {
    /// `--dir` value when explicitly passed on the CLI; `None` when inherited
    /// from `.hyalo.toml` (the hint can omit it and rely on config).
    pub dir: Option<String>,
    /// `--format` value when explicitly passed on the CLI.
    pub format: Option<String>,
    /// Whether `--hints` was explicitly passed on the CLI.
    pub hints: bool,
}

impl HintContext {
    pub fn new(source: HintSource) -> Self {
        Self {
            source,
            dir: None,
            glob: vec![],
            format: None,
            hints: false,
            elapsed_ms: None,
            quiet: false,
            has_index: false,
            snapshot_on_disk: false,
            pattern_names_a_file: None,
            fields: vec![],
            sort: None,
            has_limit: false,
            has_body_search: false,
            body_pattern: None,
            has_regex_search: false,
            property_filters: vec![],
            tag_filters: vec![],
            task_filter: None,
            file_targets: vec![],
            section_filters: vec![],
            broken_links_filter: false,
            orphan_filter: false,
            dead_end_filter: false,
            reverse: false,
            title_filter: None,
            find_index: FindIndexHint::None,
            view_name: None,
            task_selector: None,
            read_narrowed: false,
            dry_run: false,
            index_path: None,
            auto_link_file: None,
            auto_link_min_length: None,
            auto_link_exclude_titles: vec![],
            lint_is_fix: false,
            lint_rule: None,
            lint_rule_prefix: None,
            lint_fix_rules: vec![],
            okf_profile_active: false,
            observed_property_values: std::collections::BTreeMap::new(),
            body_search_suggestion: None,
        }
    }

    /// Construct a `HintContext` with the common global flags pre-populated.
    ///
    /// Equivalent to calling `new(source)` followed by assigning `dir`,
    /// `format`, and `hints` — extracted here so every `match` arm in
    /// `run.rs` does not repeat those three lines.
    pub fn from_common(source: HintSource, common: &CommonHintFlags) -> Self {
        let mut ctx = Self::new(source);
        ctx.dir.clone_from(&common.dir);
        ctx.format.clone_from(&common.format);
        ctx.hints = common.hints;
        ctx
    }

    /// The `find_index` value for a query that ran against the default vault
    /// `.hyalo-index` (re-emitted as bare `--index`).
    #[must_use]
    pub fn default_find_index() -> FindIndexHint {
        FindIndexHint::Default
    }

    /// The `find_index` value for a query that ran against an explicit
    /// `--index-file <path>` (re-emitted verbatim).
    #[must_use]
    pub fn file_find_index(path: String) -> FindIndexHint {
        FindIndexHint::File(path)
    }
}

/// Generate concrete drill-down hints from a command's JSON output.
///
/// `total` is the real count of items (may exceed the number of items in `data`
/// when output was truncated by a limit). `None` means the command doesn't
/// produce a list with a total.
///
/// Returns at most [`MAX_HINTS`] [`Hint`]s, each with a human-readable description
/// and an executable `hyalo` command (`cmd`).
/// Counts of paths the `--files-from` resolver dropped during input
/// processing. Mirrors the fields injected into the JSON envelope by
/// `output_pipeline::inject_files_from_counters`. Passed into
/// [`generate_hints`] alongside `data` so counter-aware hints can fire even
/// though the counters haven't been merged into the data value yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct FilesFromCounterSummary {
    pub files_missing: u64,
    pub files_skipped_outside_vault: u64,
}

#[must_use]
pub fn generate_hints(
    ctx: &HintContext,
    data: &serde_json::Value,
    total: Option<u64>,
) -> Vec<Hint> {
    generate_hints_with_counters(ctx, data, total, None)
}

/// Same as [`generate_hints`] but also factors in `--files-from` counters
/// known to the caller (the output pipeline) but not yet injected into
/// `data`. Used by the dispatch layer; tests and other callers can use the
/// no-counters [`generate_hints`].
#[must_use]
pub fn generate_hints_with_counters(
    ctx: &HintContext,
    data: &serde_json::Value,
    total: Option<u64>,
    counters: Option<FilesFromCounterSummary>,
) -> Vec<Hint> {
    let mut hints = match &ctx.source {
        HintSource::Summary => hints_for_summary(ctx, data),
        HintSource::PropertiesSummary => hints_for_properties_summary(ctx, data, total),
        HintSource::TagsSummary => hints_for_tags_summary(ctx, data, total),
        HintSource::Find => hints_for_find(ctx, data, total),
        HintSource::Set | HintSource::Remove | HintSource::Append => hints_for_mutation(ctx, data),
        HintSource::Read => hints_for_read(ctx, data),
        HintSource::Backlinks => hints_for_backlinks(ctx, data, total),
        HintSource::Mv => hints_for_mv(ctx, data),
        HintSource::TaskRead => hints_for_task_read(ctx, data),
        HintSource::TaskToggle | HintSource::TaskSetStatus => hints_for_task_mutation(ctx, data),
        HintSource::LinksFix => hints_for_links_fix(ctx, data),
        HintSource::LinksAuto => hints_for_links_auto(ctx, data),
        HintSource::CreateIndex => hints_for_create_index(ctx, data),
        HintSource::DropIndex => hints_for_drop_index(ctx, data),
        HintSource::Lint => hints_for_lint(ctx, data, total),
        HintSource::Types { .. } => hints_for_types(ctx, data),
        HintSource::ViewsList => hints_for_views_list(ctx, data),
        HintSource::LintRulesList => hints_for_lint_rules_list(ctx, data),
        HintSource::LintRulesShow => hints_for_lint_rules_show(ctx, data),
        HintSource::New { file } => hints_for_new(ctx, file),
        HintSource::OkfIndex => hints_for_okf_index(ctx, data),
        HintSource::OkfLog => hints_for_okf_log(ctx),
    };
    // iter-144: slow-query index-suggestion hint. Appended after per-command
    // hints so domain-specific hints are not displaced; counts toward MAX_HINTS.
    // Dedupe against the large-vault hint emitted by `hints_for_summary`:
    // when a `summary` run is *both* slow and large, only one create-index
    // hint should occupy a slot.
    if hints.len() < MAX_HINTS
        && let Some(hint) = slow_query_hint(ctx)
        && !hints.iter().any(|h| h.cmd == hint.cmd)
    {
        hints.push(hint);
    }

    // iter-143: `--files-from`-aware hints. Counters are passed in from the
    // output pipeline (the envelope merge happens *after* hint generation,
    // so `data` doesn't carry them yet). Prepended so the `MAX_HINTS` cap
    // doesn't crowd them out — a skipped-input warning is more urgent than
    // a follow-up suggestion.
    let mut ff_hints = files_from_hints(counters);
    ff_hints.append(&mut hints);
    ff_hints.into_iter().take(MAX_HINTS).collect()
}

/// Return an index-suggestion hint when the command was slow and no index is active.
///
/// Eligible sources: `find`, `lint`, `backlinks`, `properties summary`,
/// `tags summary`, `summary`, and `read` — commands that scan the vault and
/// benefit from a snapshot index.
///
/// Suppressed when:
/// - `ctx.quiet` is true (`--quiet` flag).
/// - `ctx.has_index` is true (snapshot already active).
/// - `elapsed_ms` is below [`SLOW_QUERY_THRESHOLD_MS`].
fn slow_query_hint(ctx: &HintContext) -> Option<Hint> {
    // Only eligible commands produce vault scans that an index can speed up.
    let eligible = matches!(
        ctx.source,
        HintSource::Find
            | HintSource::Lint
            | HintSource::Backlinks
            | HintSource::PropertiesSummary
            | HintSource::TagsSummary
            | HintSource::Summary
            | HintSource::Read
    );
    if !eligible {
        return None;
    }
    if ctx.quiet || ctx.has_index {
        return None;
    }
    let elapsed = ctx.elapsed_ms?;
    if elapsed <= SLOW_QUERY_THRESHOLD_MS {
        return None;
    }
    // Route through the command builder so the hint carries the same `--dir`
    // (and other explicit global flags) the slow command ran with. A bare
    // `hyalo create-index` string would index the *default* vault, not the
    // `--dir` one the user is actually querying (BUG-7).
    // iter-267 (UX-18): when the vault already HAS a `.hyalo-index`, the
    // actionable advice is to USE it, not to rebuild what is already there.
    // `find`, `lint` and `summary` get a runnable command with their scope
    // preserved; the remaining eligible commands get advice only, because
    // they take no `--index` flag of their own.
    if ctx.snapshot_on_disk {
        let description = format!(
            "Command took {elapsed} ms — a `.hyalo-index` snapshot exists in the vault; \
             re-run with --index to use it"
        );
        let cmd = match ctx.source {
            HintSource::Find => build_find_command_preserving_filters(ctx, &["--index"]),
            HintSource::Lint => build_command_with_glob_and_files(ctx, &["lint", "--index"]),
            HintSource::Summary => build_command_with_glob(ctx, &["summary", "--index"]),
            _ => String::new(),
        };
        return Some(Hint::new(description, cmd));
    }
    Some(Hint::new(
        format!("Command took {elapsed} ms — create an index for faster queries"),
        build_command_no_glob(ctx, &["create-index"]),
    ))
}

/// Return `--files-from`-counter hints when the resolver reported non-zero
/// `files_missing` or `files_skipped_outside_vault`. `files_skipped_non_md`
/// is intentionally not hinted — it's common when piping from `git diff` and
/// not actionable (the caller's diff included `.toml` / `.md.lock` / etc).
fn files_from_hints(counters: Option<FilesFromCounterSummary>) -> Vec<Hint> {
    let mut out = Vec::new();
    let Some(c) = counters else {
        return out;
    };

    if c.files_missing > 0 {
        let (noun, verb) = if c.files_missing == 1 {
            ("path", "did")
        } else {
            ("paths", "did")
        };
        out.push(Hint::without_cmd(format!(
            "{} input {noun} {verb} not exist on disk (likely deletions); \
             use `git diff --name-only --diff-filter=AMR` upstream to filter them out",
            c.files_missing
        )));
    }
    if c.files_skipped_outside_vault > 0 {
        let (noun, verb) = if c.files_skipped_outside_vault == 1 {
            ("path", "was")
        } else {
            ("paths", "were")
        };
        out.push(Hint::without_cmd(format!(
            "{} input {noun} {verb} outside the vault; \
             check your --dir or the upstream filter",
            c.files_skipped_outside_vault
        )));
    }
    out
}

// ---------------------------------------------------------------------------
// Submodules (iteration 247)
// ---------------------------------------------------------------------------
//
// `hints.rs` had grown to 5,059 lines -- a review hotspot flagged by the
// 2026-08-27 deep review. The generators are now split by the command family
// they serve; this file keeps only the vocabulary they share (the [`Hint`]
// type, [`HintContext`], the thresholds) and the dispatch above. Every item is
// re-exported at the visibility it already had, so no caller outside this
// module can tell the difference.

mod command;
mod config;
mod find;
mod index;
mod links;
mod lint;
mod mutation;
mod summary;
mod util;
mod zero_result;

pub use command::{HintBuilder, shell_quote, shorten_index_path_for_hint};
use command::{
    build_command_no_glob, build_command_with_file, build_command_with_glob,
    build_command_with_glob_and_files, build_find_command_composing,
    build_find_command_preserving_filters, build_find_command_with_pattern,
};
use config::{
    hints_for_lint_rules_list, hints_for_lint_rules_show, hints_for_new, hints_for_types,
    hints_for_views_list,
};
use find::{format_confidence, hints_for_find};
use index::{hints_for_create_index, hints_for_drop_index, hints_for_okf_index, hints_for_okf_log};
use links::{hints_for_links_auto, hints_for_links_fix};
use lint::hints_for_lint;
use mutation::{
    hints_for_backlinks, hints_for_mutation, hints_for_mv, hints_for_read, hints_for_task_mutation,
    hints_for_task_read,
};
use summary::{hints_for_properties_summary, hints_for_summary, hints_for_tags_summary};
use util::{first_modified_file, status_priority};
pub(crate) use zero_result::zero_result_notice;

#[cfg(test)]
mod tests;
