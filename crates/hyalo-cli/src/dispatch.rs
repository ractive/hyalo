use std::path::Path;

use anyhow::{Context, Result};

use crate::cli::args::Commands;
use crate::commands::{
    append as append_commands, backlinks as backlinks_commands, changelog as changelog_commands,
    create_index as create_index_commands, drop_index as drop_index_commands,
    find as find_commands, links as links_commands, lint as lint_commands,
    lint_rules as lint_rules_commands, madr as madr_commands, mv as mv_commands,
    okf as okf_commands, properties, read as read_commands, remove as remove_commands,
    set as set_commands, summary as summary_commands, tags as tag_commands, tasks as task_commands,
    types as types_commands, views as views_commands,
};
use crate::output::{CommandOutcome, Format};
use hyalo_core::filter;
use hyalo_core::index::{SnapshotIndex, VaultIndex as _};
use hyalo_core::schema::SchemaConfig;
use hyalo_core::{CaseInsensitiveIndex, CaseInsensitiveMode};

/// Default output limit for list commands when no `--limit` is passed and no
/// `default_limit` is set in `.hyalo.toml`.
pub(crate) const DEFAULT_OUTPUT_LIMIT: usize = 50;

/// Build a [`CaseInsensitiveIndex`] from a full vault directory scan.
///
/// The scan is always vault-wide — not scoped to any `--file` or `--glob`
/// argument — because case-insensitive link resolution must find *any* file
/// in the vault, even files not included in the current query scope. A scoped
/// `VaultIndex` (built by `collect_files` when `--file` is used) would omit
/// the very link targets we need to resolve, so we re-walk from disk rather
/// than reusing the command's `VaultIndex`.
///
/// Errors during discovery are silently ignored (the index will just be less
/// complete, which degrades gracefully to no case-insensitive fallback).
///
/// On large vaults this disk walk is expensive (~2.7 s for ~2600 files), so
/// callers should only invoke it when wikilink/stem-map resolution is
/// actually needed — see [`maybe_case_index`]'s `needs_stem_map` parameter.
pub(crate) fn build_case_index_from_dir(dir: &std::path::Path) -> CaseInsensitiveIndex {
    use hyalo_core::discovery;
    let mut idx = CaseInsensitiveIndex::new();
    if let Ok(files) = discovery::discover_files(dir) {
        for file in &files {
            let rel = discovery::relative_path(dir, file);
            idx.insert(&rel);
        }
    }
    // iter-261 / BUG-5, BUG-6: attachments (`.png`, `.base`, `.pdf`) are vault
    // files Obsidian resolves links against, so they go into the same index —
    // keyed by full path and by basename. Their basename key carries the
    // extension (`img.png`), so it can never collide with a note stem.
    if let Ok(attachments) = discovery::discover_attachments(dir) {
        for rel in &attachments {
            idx.insert(rel);
        }
    }
    idx
}

/// Build a [`CaseInsensitiveIndex`] from an in-memory snapshot index.
///
/// Equivalent to [`build_case_index_from_dir`] but seeds the stem map from
/// the snapshot's entries (`rel_path` list) instead of walking the disk.
/// Cost is a linear scan over `snap.entries()` — about 4 ms for MDN's 14 399
/// files (iter-256), against seconds for the disk-walk variant. It was 62 ms
/// until iter-256 removed a quadratic dedupe scan in
/// [`CaseInsensitiveIndex::insert`]; see FIND-8.
pub(crate) fn build_case_index_from_snapshot(snap: &SnapshotIndex) -> CaseInsensitiveIndex {
    let mut idx = CaseInsensitiveIndex::with_capacity(snap.entries().len());
    for entry in snap.entries() {
        idx.insert(&entry.rel_path);
    }
    // iter-261: attachments recorded by `create-index` (empty for a snapshot
    // written by an older hyalo, which simply degrades to the old behaviour).
    for rel in snap.attachments() {
        idx.insert(rel);
    }
    idx
}

/// Predicate: does a `Commands::Find` invocation with these flags need the
/// wikilink stem map?
///
/// Outbound-link resolution (the `links` field, `--broken-links`, `--orphan`,
/// `--dead-end`, sort-by-link-count) goes through `discovery::resolve_target`,
/// which consults the case/stem index. Backlinks (field + sort) come from the
/// pre-built link graph and do NOT use the case index, so backlinks-only
/// queries can skip the vault-wide disk walk.
///
/// This is the single source of truth for the predicate — both the dispatch
/// `Commands::Find` branch and the test matrix call into this function so
/// they cannot drift.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) fn find_needs_stem_map(
    broken_links: bool,
    orphan: bool,
    dead_end: bool,
    fields_links: bool,
    sort_links: bool,
) -> bool {
    broken_links || orphan || dead_end || fields_links || sort_links
}

/// Resolve a [`CaseInsensitiveIndex`] for the current command.
///
/// Behaviour by parameter:
/// - `needs_stem_map = false` → returns an empty index without touching disk.
///   Callers that never resolve wikilinks pass `false` here to skip the
///   vault-wide walk. An empty `CaseInsensitiveIndex` behaves identically
///   to the `EMPTY_CASE_INDEX` fallback used in `link_rewrite` — lookups
///   return `None` and callers degrade gracefully.
/// - `needs_stem_map = true` AND a `snapshot` is provided → seeds the stem
///   map from the snapshot's `rel_path` list (microseconds).
/// - `needs_stem_map = true` AND no snapshot → falls back to the full
///   disk walk via [`build_case_index_from_dir`].
///
/// In all cases case-insensitive path lookups are configured per `mode`.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn maybe_case_index(
    mode: CaseInsensitiveMode,
    dir: &std::path::Path,
    needs_stem_map: bool,
    snapshot: Option<&SnapshotIndex>,
) -> Option<CaseInsensitiveIndex> {
    let mut idx = if !needs_stem_map {
        CaseInsensitiveIndex::new()
    } else if let Some(snap) = snapshot {
        build_case_index_from_snapshot(snap)
    } else {
        build_case_index_from_dir(dir)
    };
    // DEC-267 (iter-261): link resolution folds case on every platform unless
    // the vault opts out — not "whatever this filesystem does", so the
    // filesystem probe plays no part here.
    idx.set_case_insensitive_paths(hyalo_core::links_case_insensitive(mode));
    Some(idx)
}

/// Shared context for command dispatch.
pub(crate) struct CommandContext<'a> {
    pub dir: &'a Path,
    /// The directory where `.hyalo.toml` was loaded from.  This is the
    /// project root when `dir` comes from `dir = "subdir"` in the config,
    /// or the `--dir` target when the user passes `--dir` explicitly.
    /// Views and types are stored in `config_dir/.hyalo.toml`.
    pub config_dir: &'a Path,
    /// The vault dir as configured (`config.dir.to_string_lossy()`).
    /// Used for `--files-from` prefix stripping in the unified resolver.
    pub configured_dir_str: &'a str,
    pub site_prefix: Option<&'a str>,
    /// Internal format — always Json; commands build JSON, pipeline handles conversion.
    pub effective_format: Format,
    /// The user-requested format (Text or Json). Used by `read` to decide between
    /// `RawOutput` (text mode) and `Success` (JSON mode).
    pub user_format: Format,
    pub snapshot_index: &'a mut Option<SnapshotIndex>,
    pub index_path: Option<&'a Path>,
    /// Default stemming language from `[search] language` in `.hyalo.toml`.
    pub config_language: Option<&'a str>,
    /// Frontmatter property names to scan for `[[wikilink]]` values in the link graph.
    /// Comes from `[links] frontmatter_properties` in `.hyalo.toml`. `None` = use defaults.
    pub frontmatter_link_props: Option<&'a [String]>,
    /// Parsed schema configuration from `[schema.*]` sections in `.hyalo.toml`.
    pub schema: &'a SchemaConfig,
    /// The diagnostic when `[schema]` existed but could not be loaded, so
    /// [`Self::schema`] above is the empty fallback rather than the vault's
    /// (DEC-289). `set`/`append` refuse when validation was asked for and this
    /// is `Some`; everything else keeps running on the fallback.
    pub schema_invalid: Option<&'a str>,
    /// When `true`, schema validation runs on every `set`/`append` operation even
    /// without `--validate`. Comes from `validate_on_write = true` in `.hyalo.toml`.
    pub validate_on_write: bool,
    /// Vault-relative paths excluded from `hyalo lint`. From `[lint] ignore` in `.hyalo.toml`.
    pub lint_ignore: &'a [String],
    /// Vault-relative globs the OKF generators skip. From `[okf] ignore` in `.hyalo.toml`.
    pub okf_ignore: &'a [String],
    /// Raw `[changelog] path` value (config-dir-relative), if set. Resolved
    /// against `config_dir` by the changelog commands. `None` = default
    /// `CHANGELOG.md` in the vault dir.
    pub changelog_path: Option<&'a str>,
    /// Markdown lint configuration from `[lint]` in `.hyalo.toml`.
    pub md_lint: &'a hyalo_mdlint::LintConfig,
    /// Case-insensitive link resolution mode from `[links] case_insensitive`.
    pub case_insensitive_mode: CaseInsensitiveMode,
    /// `[links.case_insensitive] resolve` OR the `links fix
    /// --case-insensitive` flag (UX-6, iter-244): treat case-fold-resolving
    /// link targets as resolved rather than fixable, so `links fix` offers
    /// no `link-case-mismatch` rewrites for MDN-style case-folded vaults.
    pub case_insensitive_resolve: bool,
    /// Persisted `hyalo links auto` exclusions and preference from
    /// `[links.auto]` in `.hyalo.toml` (iter-195a). Unioned with the CLI flags
    /// in the `links auto` dispatch arm.
    pub auto_link_exclude_titles: &'a [String],
    /// See [`CommandContext::auto_link_exclude_titles`].
    pub auto_link_exclude_target_globs: &'a [String],
    /// `[links.auto] first_only` — `--first-only` for every run.
    pub auto_link_first_only: bool,
    /// `[links] fuzzy_min_confidence` — the confidence floor `links fix
    /// --apply-fuzzy` uses when `--min-confidence` is absent (iter-212).
    /// `None` means the built-in
    /// [`hyalo_core::link_score::DEFAULT_FUZZY_MIN_CONFIDENCE`].
    pub config_fuzzy_min_confidence: Option<f64>,
    /// `[links.auto] warn_common_titles` (default `true`) — whether `links auto`
    /// may emit the advisory note naming noisy candidate titles (common English
    /// words, or titles that dominate the run).
    pub auto_link_warn_common_titles: bool,
    /// Optional exit code override set by commands that need a non-0/2 exit code
    /// (e.g. `lint` returns 1 when errors are found). The output pipeline uses this
    /// to override its own exit code calculation.
    pub exit_code_override: Option<i32>,
    /// Default output limit from `.hyalo.toml` (`default_limit`).
    /// `None` = use `DEFAULT_OUTPUT_LIMIT`.
    /// `Some(0)` = unlimited.
    /// `Some(n)` = limit to n.
    pub config_default_limit: Option<usize>,
    /// When true, the output is consumed programmatically (`--jq` or `--count`),
    /// so the default limit should not apply — only an explicit `--limit` is honoured.
    pub programmatic_output: bool,
    /// Strict schema validation mode from `[lint] strict = true` in `.hyalo.toml`.
    /// When `true`, "no 'type' property" and "undeclared property" warnings are
    /// promoted to errors, and lint exits non-zero on them.
    pub lint_strict: bool,
    /// Active conformance profiles (e.g. `["okf", "madr"]`) from
    /// `[lint] profiles` in `.hyalo.toml` or an explicit `hyalo lint --profile`.
    /// Enables every listed profile's advisory lint rules. Multiple profiles
    /// compose — all their rules fire in one lint pass.
    pub lint_profiles: Vec<String>,
    /// `--files-from` counters captured during dispatch for commands that resolve
    /// `--files-from` inside `resolve_inputs` (read/backlinks/task). Surfaced by
    /// the output pipeline as `files_from_counters` in the envelope.
    pub files_from_counters: Option<crate::commands::files_from::FilesFromCounters>,
    /// Distinct frontmatter values observed for each property key named by a
    /// `--property K=V` filter, collected by `find` when the query matched
    /// nothing (iter-251). The index has already been walked at that point, so
    /// this costs no extra I/O; `run.rs` copies it into the hint context so the
    /// zero-result did-you-mean can be computed without a second scan. Empty
    /// for every command other than an empty `find`.
    pub zero_result_values: std::collections::BTreeMap<String, Vec<String>>,
    /// A `--property K~=RE` filter whose regex matched no frontmatter value but
    /// *does* match body prose, confirmed by the bounded probe `find` runs on
    /// the zero-result path (iter-258). `run.rs` moves it into the hint context
    /// so the empty result can point at `find -e` instead of leaving the caller
    /// to guess that the text lives in bodies. `None` for every other command
    /// and for any empty `find` without a property regex filter.
    pub zero_result_body_search: Option<crate::hints::BodySearchSuggestion>,
}

/// Resolve the effective limit for a list command.
///
/// Precedence (highest first):
/// 1. `cli_limit = Some(n)` — user passed `--limit n` (0 = unlimited → returns `None`)
/// 2. If `programmatic` is true (`--jq` or `--count`), skip the default limit — the
///    output is consumed by a pipeline that needs complete results.
/// 3. `config_default` = `Some(n)` from `.hyalo.toml` (0 = unlimited → returns `None`)
/// 4. `DEFAULT_OUTPUT_LIMIT` — hard-coded fallback
///
/// Returns `None` for unlimited, `Some(n)` for an effective cap.
pub(crate) fn resolve_limit(
    cli_limit: Option<usize>,
    config_default: Option<usize>,
    programmatic: bool,
) -> Option<usize> {
    match cli_limit {
        Some(0) => None, // explicit --limit 0 = unlimited
        Some(n) => Some(n),
        None => {
            if programmatic {
                return None;
            }
            match config_default {
                Some(0) => None, // config default_limit = 0 = unlimited
                Some(n) => Some(n),
                None => Some(DEFAULT_OUTPUT_LIMIT),
            }
        }
    }
}

/// Convert a legacy [`lint_commands::FileLintResult`] (frontmatter/view violations, old shape)
/// into an [`lint_commands::ExtFileLintResult`] (new rule_groups shape).
///
/// View violations are grouped under the synthetic rule id `SCHEMA`.
pub(crate) fn adapt_view_result_to_ext(
    result: &lint_commands::FileLintResult,
) -> lint_commands::ExtFileLintResult {
    let violations: Vec<lint_commands::BodyViolation> = result
        .violations
        .iter()
        .map(|v| lint_commands::BodyViolation {
            line: 0,
            column: 0,
            severity: match v.severity {
                lint_commands::Severity::Error => "error".to_owned(),
                lint_commands::Severity::Warn => "warn".to_owned(),
            },
            message: v.message.clone(),
            fix: None,
        })
        .collect();

    let total = violations.len();
    // Group severity is the max across members so a folded SCHEMA group that
    // contains any error reads as `error` (BUG-17 parity with the main path).
    let group_severity = if violations.iter().any(|v| v.severity == "error") {
        "error".to_string()
    } else {
        "warn".to_string()
    };
    let rule_groups = if total == 0 {
        vec![]
    } else {
        vec![lint_commands::RuleGroup {
            rule: "SCHEMA".to_string(),
            count: total,
            shown: total,
            truncated: false,
            severity: group_severity,
            autofixable: false,
            violations,
        }]
    };

    lint_commands::ExtFileLintResult {
        file: result.file.clone(),
        doc_type: None,
        rule_groups,
    }
}

/// Inject an [`lint_commands::ExtFileLintResult`] into the serialized
/// [`lint_commands::ExtLintOutput`] stored inside a `CommandOutcome`.
///
/// Deserializes the JSON, prepends the new file result, updates `files_with_violations`
/// and `total`, then re-serializes.
pub(crate) fn inject_ext_file_result(
    outcome: CommandOutcome,
    extra: &lint_commands::ExtFileLintResult,
) -> Result<CommandOutcome> {
    let (payload, total_count) = match outcome {
        CommandOutcome::Success { output, total } => (output, total),
        other => return Ok(other),
    };

    let mut value: serde_json::Value =
        serde_json::from_str(&payload).context("failed to re-parse extended lint output JSON")?;

    if let Some(obj) = value.as_object_mut() {
        let extra_violations: usize = extra.rule_groups.iter().map(|g| g.count).sum();
        let is_fix_mode = obj.contains_key("total_remaining");

        // In fix-mode, the per-file shape is `ExtFileLintFixResult` (with
        // `fixed_groups`/`remaining_groups`/`conflicts`), not the read-only
        // `ExtFileLintResult` shape. Adapt before injecting so the renderer
        // and JSON consumers see consistent structure.
        let extra_value = if is_fix_mode {
            let remaining_groups = serde_json::to_value(&extra.rule_groups)
                .context("failed to serialize view lint groups")?;
            serde_json::json!({
                "file": extra.file,
                "fixed_groups": serde_json::Value::Array(Vec::new()),
                "remaining_groups": remaining_groups,
                "conflicts": serde_json::Value::Array(Vec::new()),
            })
        } else {
            serde_json::to_value(extra).context("failed to serialize view lint result")?
        };

        if let Some(files) = obj.get_mut("files").and_then(|f| f.as_array_mut()) {
            files.insert(0, extra_value);
        }
        // Read-only shape has `violations` (named `total` before iter-216 D-2).
        if let Some(n) = obj.get_mut("violations").and_then(|v| v.as_u64()) {
            obj.insert(
                "violations".to_string(),
                serde_json::Value::from(n + extra_violations as u64),
            );
        }
        // Fix-mode shape has `total_remaining`.
        if let Some(n) = obj.get_mut("total_remaining").and_then(|v| v.as_u64()) {
            obj.insert(
                "total_remaining".to_string(),
                serde_json::Value::from(n + extra_violations as u64),
            );
        }
        if extra_violations > 0
            && let Some(n) = obj
                .get_mut("files_with_violations")
                .and_then(|v| v.as_u64())
        {
            obj.insert(
                "files_with_violations".to_string(),
                serde_json::Value::from(n + 1),
            );
        }
        // Bump severity totals so the summary stays consistent with the
        // injected groups. View violations are categorised by `severity`
        // ("error" or "warn") on each rule group.
        let mut extra_errors: u64 = 0;
        let mut extra_warnings: u64 = 0;
        for g in &extra.rule_groups {
            let n = g.count as u64;
            match g.severity.as_str() {
                "error" => extra_errors += n,
                _ => extra_warnings += n,
            }
        }
        // Fix-mode renamed these to `remaining_errors`/`remaining_warnings`
        // (iter-218 NEW-6b) since they mean something different there than
        // on the read-only shape's `errors`/`warnings` (remaining-after-fix
        // vs whole-run severity counts) — bump whichever key this payload
        // actually carries.
        let (errors_key, warnings_key) = if is_fix_mode {
            ("remaining_errors", "remaining_warnings")
        } else {
            ("errors", "warnings")
        };
        if extra_errors > 0
            && let Some(n) = obj.get_mut(errors_key).and_then(|v| v.as_u64())
        {
            obj.insert(
                errors_key.to_string(),
                serde_json::Value::from(n + extra_errors),
            );
        }
        if extra_warnings > 0
            && let Some(n) = obj.get_mut(warnings_key).and_then(|v| v.as_u64())
        {
            obj.insert(
                warnings_key.to_string(),
                serde_json::Value::from(n + extra_warnings),
            );
        }
    }

    let extra_violations: usize = extra.rule_groups.iter().map(|g| g.count).sum();
    let bump_total = extra_violations > 0;
    let new_payload = crate::output::format_success(crate::output::Format::Json, &value);
    Ok(match total_count {
        Some(t) => {
            CommandOutcome::success_with_total(new_payload, if bump_total { t + 1 } else { t })
        }
        None => CommandOutcome::success(new_payload),
    })
}

/// Render a `parse_property_filter` error as a `UserError`, surfacing the
/// underlying engine detail (regex compile error with caret/position) the same
/// way `find -e` does. `parse_property_filter` wraps the regex error as a
/// `.source()` cause under a top-level `"invalid regex in property filter: ..."`
/// context; passing that cause into `format_error` puts the caret detail in the
/// `cause` field (iter-181 task 5) instead of dropping it.
pub(crate) fn property_filter_error_outcome(e: &anyhow::Error, format: Format) -> CommandOutcome {
    // Use the *root* cause (last link in the chain) so the regex engine's own
    // message — the one carrying the caret/position — reaches the user, not the
    // intermediate `"invalid regex pattern: ..."` wrapper. The chain's first link
    // is the top-level error itself; a chain of length 1 has no wrapped cause.
    let cause = if e.chain().count() > 1 {
        e.chain().last().map(std::string::ToString::to_string)
    } else {
        None
    };
    CommandOutcome::UserError(crate::output::format_error(
        format,
        &e.to_string(),
        None,
        None,
        cause.as_deref(),
    ))
}

/// Parse `--where-property` filters and validate `--where-tag` names.
/// Returns an error string on invalid input.
pub(crate) fn parse_where_filters(
    where_properties: &[String],
    where_tags: &[String],
) -> Result<Vec<filter::PropertyFilter>, String> {
    let filters = where_properties
        .iter()
        .map(|s| filter::parse_property_filter(s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for tag in where_tags {
        crate::commands::tags::validate_tag(tag)?;
    }
    Ok(filters)
}

pub(crate) fn dispatch(command: Commands, ctx: &mut CommandContext<'_>) -> Result<CommandOutcome> {
    let dir = ctx.dir;
    let site_prefix = ctx.site_prefix;
    let effective_format = ctx.effective_format;

    match command {
        Commands::Find {
            pattern,
            file_positional,
            view: _, // resolved before dispatch
            filters,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // ARCH-1 (iter-225): the ~310-line arm body now lives in
            // `commands::find::run` — dispatch only forwards the parsed args.
            find_commands::run::run(ctx, pattern, file_positional, filters)
        }
        Commands::Read {
            selection,
            section,
            lines,
            frontmatter,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // ARCH-1 (iter-225): the arm body now lives in
            // `commands::read::run_command`.
            read_commands::run_command(ctx, selection, section, lines, frontmatter)
        }
        Commands::Properties {
            glob: bare_glob,
            limit: bare_limit,
            index_flags: _, // consumed in run.rs before dispatch
            action,
        } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::properties::run`.
            properties::run(ctx, bare_glob, bare_limit, action)
        }
        Commands::Tags {
            glob: bare_glob,
            limit: bare_limit,
            index_flags: _, // consumed in run.rs before dispatch
            action,
        } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::tags::run`.
            tag_commands::run(ctx, bare_glob, bare_limit, action)
        }
        Commands::Task { action } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::tasks::run`.
            task_commands::run(ctx, action)
        }
        Commands::Summary {
            glob,
            recent,
            depth,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::summary::run`.
            summary_commands::run(ctx, glob, recent, depth)
        }
        Commands::Set {
            file_positional,
            properties,
            tag,
            file,
            glob,
            files_from: _, // resolved in run.rs before dispatch
            where_properties,
            where_tags,
            dry_run,
            validate,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::set::run`.
            set_commands::run(
                ctx,
                file_positional,
                properties,
                tag,
                file,
                glob,
                where_properties,
                where_tags,
                dry_run,
                validate,
            )
        }
        Commands::Remove {
            file_positional,
            properties,
            tag,
            file,
            glob,
            files_from: _, // resolved in run.rs before dispatch
            where_properties,
            where_tags,
            dry_run,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::remove::run`.
            remove_commands::run(
                ctx,
                file_positional,
                properties,
                tag,
                file,
                glob,
                where_properties,
                where_tags,
                dry_run,
            )
        }
        Commands::Append {
            file_positional,
            properties,
            file,
            glob,
            files_from: _, // resolved in run.rs before dispatch
            where_properties,
            where_tags,
            dry_run,
            validate,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::append::run`.
            append_commands::run(
                ctx,
                file_positional,
                properties,
                file,
                glob,
                where_properties,
                where_tags,
                dry_run,
                validate,
            )
        }
        Commands::Backlinks {
            selection,
            limit: cli_limit,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::backlinks::run`.
            backlinks_commands::run(ctx, selection, cli_limit)
        }
        Commands::Mv {
            file_positional,
            file,
            to_positional,
            to,
            glob,
            files_from: _, // resolved in run.rs before dispatch
            properties,
            tag,
            r#type,
            dry_run,
            apply,
            on_conflict,
            allow_ambiguous,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::mv::run`.
            mv_commands::run(
                ctx,
                file_positional,
                file,
                to_positional,
                to,
                glob,
                properties,
                tag,
                r#type,
                dry_run,
                apply,
                on_conflict,
                allow_ambiguous,
            )
        }
        Commands::CreateIndex {
            output,
            allow_outside_vault,
        } => create_index_commands::create_index(
            dir,
            site_prefix,
            output.as_deref(),
            effective_format,
            allow_outside_vault,
            ctx.config_language,
        ),
        Commands::DropIndex {
            path,
            allow_outside_vault,
        } => drop_index_commands::drop_index(
            dir,
            path.as_deref(),
            effective_format,
            allow_outside_vault,
        ),
        Commands::Links { action } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::links::run`.
            links_commands::run(ctx, action)
        }
        Commands::Lint {
            file_positional,
            file,
            glob,
            r#type: lint_type,
            files_from: _, // resolved in run.rs before dispatch
            fix,
            dry_run,
            limit: cli_limit,
            detailed,
            rule,
            rule_prefix,
            max_per_rule,
            fix_rule,
            strict: lint_strict_flag,
            // Profile activation is resolved in run.rs into `ctx.lint_profiles`
            // (it needs the raw config to overlay); the flag itself is consumed
            // there. Ignore it here.
            profile: _,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // ARCH-1 (iter-225): the ~390-line arm body now lives in
            // `commands::lint::run`.
            lint_commands::run(
                ctx,
                file_positional,
                file,
                glob,
                lint_type,
                fix,
                dry_run,
                cli_limit,
                detailed,
                rule,
                rule_prefix,
                max_per_rule,
                fix_rule,
                lint_strict_flag,
            )
        }
        Commands::LintRules { action } => {
            // ARCH-1 (iter-225): the arm body now lives in
            // `commands::lint_rules::run`.
            lint_rules_commands::run(ctx, action)
        }
        // `Init`, `Deinit`, and `Completion` are handled as early returns before dispatch is called.
        Commands::Init { .. } => unreachable!("Init is dispatched before this match reached"),
        Commands::Deinit => unreachable!("Deinit is dispatched before this match reached"),
        Commands::Completion { .. } => {
            unreachable!("Completion is dispatched before this match reached")
        }
        // iter-256 HELP-5: `help <cmd>` is rewritten to `<cmd> -h` in argv
        // before clap parses, so the variant only exists to reserve the name
        // (and to put `help` in shell completions and the COMMAND REFERENCE).
        Commands::Help { .. } => unreachable!("Help is rewritten to `<cmd> -h` before parsing"),
        Commands::Views { action } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::views::run`.
            views_commands::run(ctx, action)
        }
        Commands::Types { action } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::types::run`.
            types_commands::run(ctx, action)
        }
        Commands::New {
            r#type,
            file,
            dry_run,
            index_flags: _,
        } => crate::commands::new::create_new(
            ctx.dir,
            &r#type,
            &file,
            ctx.schema,
            &mut crate::commands::journal::MutationJournal::new(
                &mut *ctx.snapshot_index,
                ctx.index_path,
            ),
            dry_run,
            effective_format,
        ),
        Commands::Okf { action } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::okf::run`.
            okf_commands::run(ctx, action)
        }
        Commands::Madr { action } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::madr::run`.
            madr_commands::run(ctx, action)
        }
        Commands::Changelog { action } => {
            // ARCH-1 (iter-225): the arm body now lives in
            // `commands::changelog::run`.
            changelog_commands::run(ctx, action)
        }
        // Config is dispatched as an early-return in run.rs before dispatch() is called.
        Commands::Config { .. } => unreachable!("Config command is handled before dispatch"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyalo_core::index::{ScanOptions, ScannedIndex};

    #[test]
    fn find_needs_stem_map_matrix() {
        // No link-related flag → no stem map needed.
        assert!(!find_needs_stem_map(false, false, false, false, false));
        // Each flag independently turns it on.
        assert!(find_needs_stem_map(true, false, false, false, false));
        assert!(find_needs_stem_map(false, true, false, false, false));
        assert!(find_needs_stem_map(false, false, true, false, false));
        assert!(find_needs_stem_map(false, false, false, true, false));
        assert!(find_needs_stem_map(false, false, false, false, true));
    }

    fn write(dir: &std::path::Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn snapshot_seeded_case_index_matches_disk_walk() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "alpha.md", "# alpha\n");
        write(dir, "beta.md", "# beta\n");
        write(dir, "sub/gamma.md", "# gamma\n");

        let files = hyalo_core::discovery::discover_files(dir).unwrap();
        let pairs: Vec<(std::path::PathBuf, String)> = files
            .iter()
            .map(|p| (p.clone(), hyalo_core::discovery::relative_path(dir, p)))
            .collect();
        let build = ScannedIndex::build(
            &pairs,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();

        let snap_dir = tempfile::tempdir().unwrap();
        let snap_path = snap_dir.path().join(".hyalo-index");
        SnapshotIndex::save(&build.index, &snap_path, &dir.to_string_lossy(), None, None).unwrap();
        let snap = SnapshotIndex::load(&snap_path).unwrap().unwrap();

        let from_disk = build_case_index_from_dir(dir);
        let from_snap = build_case_index_from_snapshot(&snap);

        assert_eq!(from_disk.lookup_stem("alpha"), Some("alpha.md"));
        assert_eq!(from_snap.lookup_stem("alpha"), Some("alpha.md"));
        assert_eq!(
            from_snap.lookup_stem("gamma"),
            from_disk.lookup_stem("gamma")
        );
        assert_eq!(from_snap.lookup_stem("beta"), from_disk.lookup_stem("beta"));
        assert!(from_snap.lookup_stem("does-not-exist").is_none());
    }

    #[test]
    fn maybe_case_index_skips_walk_when_not_needed() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "alpha.md", "# alpha\n");

        let idx = maybe_case_index(CaseInsensitiveMode::Auto, dir, false, None).unwrap();
        // Empty index — no stem map, no path lookups.
        assert!(idx.lookup_stem("alpha").is_none());
    }

    #[test]
    fn maybe_case_index_walks_disk_without_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "alpha.md", "# alpha\n");

        let idx = maybe_case_index(CaseInsensitiveMode::Auto, dir, true, None).unwrap();
        assert_eq!(idx.lookup_stem("alpha"), Some("alpha.md"));
    }
}
