use std::path::Path;

use anyhow::{Context, Result};

use crate::cli::args::{
    ChangelogAction, Commands, IndexFlags, LinksAction, LintRulesAction, MadrAction,
    OkfAction, PropertiesAction, TagsAction, TypesAction,
};
use crate::commands::inputs::{ResolutionPolicy, ResolvedInputsOrOutcome, resolve_inputs};
use crate::commands::{
    IndexResolution, ResolvedIndex, append as append_commands, backlinks as backlinks_commands,
    create_index as create_index_commands, drop_index as drop_index_commands,
    find as find_commands, links as links_commands, lint as lint_commands,
    lint_rules as lint_rules_commands, mv as mv_commands, properties, read as read_commands,
    remove as remove_commands, resolve_index, set as set_commands, summary as summary_commands,
    tags as tag_commands, tasks as task_commands, views as views_commands,
};
use crate::output::{CommandOutcome, Format};
use hyalo_core::case_index::{CaseInsensitiveIndex, CaseInsensitiveMode, mode_enabled};
use hyalo_core::filter;
use hyalo_core::index::{ScanOptions, SnapshotIndex, VaultIndex as _};
use hyalo_core::schema::SchemaConfig;

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
    idx
}

/// Build a [`CaseInsensitiveIndex`] from an in-memory snapshot index.
///
/// Equivalent to [`build_case_index_from_dir`] but seeds the stem map from
/// the snapshot's entries (`rel_path` list) instead of walking the disk.
/// Cost is a linear scan over `snap.entries()`, expected to be microseconds
/// vs seconds for the disk-walk variant on large vaults.
pub(crate) fn build_case_index_from_snapshot(snap: &SnapshotIndex) -> CaseInsensitiveIndex {
    let mut idx = CaseInsensitiveIndex::new();
    for entry in snap.entries() {
        idx.insert(&entry.rel_path);
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
    idx.set_case_insensitive_paths(mode_enabled(mode, dir));
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

/// Public wrapper for [`patch_index_for_modified_files`] used by the body-lint pass.
pub(crate) fn patch_index_for_modified_files_pub(
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
    dir: &Path,
    modified_files: &[String],
) -> Result<()> {
    patch_index_for_modified_files(snapshot_index, index_path, dir, modified_files)
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

/// Patch the snapshot index for a list of vault-relative paths that were
/// modified on disk.  Uses `refresh_entry_and_links` to re-scan each file
/// once, refreshing the full entry (properties, tags, links, sections,
/// tasks, modified timestamp) AND the persisted `LinkGraph`'s outbound
/// edges — callers whose modification rewrites body wikilinks (`links fix
/// --apply`, `links auto --apply`) need both or `backlinks`/`find --fields
/// links` would keep returning pre-mutation results until a full
/// `create-index` rebuild. Flushes to disk once at the end.
pub(crate) fn patch_index_for_modified_files(
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
    dir: &Path,
    modified_files: &[String],
) -> Result<()> {
    if modified_files.is_empty() {
        return Ok(());
    }
    let Some(idx) = snapshot_index.as_mut() else {
        return Ok(());
    };
    let mut dirty = false;
    for rel in modified_files {
        match idx.refresh_entry_and_links(dir, rel) {
            Ok(true) => dirty = true,
            Ok(false) => {} // not in index, nothing to update
            Err(e) => {
                eprintln!("warning: could not refresh index entry for {rel}: {e:#}");
            }
        }
    }
    crate::commands::mutation::save_index_if_dirty(snapshot_index, index_path, dirty)
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
    // Capture the active conformance profile before borrowing `ctx.snapshot_index`
    // mutably below (the lint arm needs it while that mutable borrow is live).
    let snapshot_index = &mut *ctx.snapshot_index;
    let index_path = ctx.index_path;

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
            // iter-238: `--iteration <ID>` on single-file commands resolves to
            // exactly one file before the generic input resolution runs.
            let selection = match crate::commands::iteration::selection_with_iteration_resolved(
                &selection,
                dir,
                ctx.schema,
                effective_format,
            ) {
                Ok(s) => s,
                Err(outcome) => return Ok(outcome),
            };
            match resolve_inputs(
                &selection,
                dir,
                ctx.configured_dir_str,
                snapshot_index.as_ref(),
                &ResolutionPolicy::Single { allow_glob: false },
                effective_format,
                false,
            )? {
                ResolvedInputsOrOutcome::Outcome(o) => Ok(o),
                ResolvedInputsOrOutcome::Resolved(r) => {
                    ctx.files_from_counters = r.counters;
                    let (_full, file) = r
                        .files
                        .into_iter()
                        .next()
                        .context("Single resolution returned no files")?;
                    read_commands::run(
                        dir,
                        &file,
                        section.as_deref(),
                        lines.as_deref(),
                        frontmatter,
                        effective_format,
                        ctx.user_format,
                    )
                }
            }
        }
        Commands::Properties {
            glob: bare_glob,
            limit: bare_limit,
            action,
        } => {
            // M-8: bare `hyalo properties` IS `properties summary`, so it takes
            // the summary flags COMMAND REFERENCE documents for it rather than
            // rejecting them at parse time.
            let action = action.unwrap_or(PropertiesAction::Summary {
                glob: bare_glob,
                limit: bare_limit,
                index_flags: IndexFlags::default(),
            });
            match action {
                PropertiesAction::Summary {
                    ref glob,
                    limit: cli_limit,
                    index_flags: _, // consumed in run.rs before dispatch
                } => match resolve_index(
                    snapshot_index.as_ref(),
                    dir,
                    &[],
                    glob,
                    effective_format,
                    site_prefix,
                    false,
                    &ScanOptions {
                        scan_body: false,
                        bm25_tokenize: false,
                        default_language: None,
                        frontmatter_link_props: ctx.frontmatter_link_props,
                    },
                )? {
                    IndexResolution::Resolved(ResolvedIndex::Snapshot(idx)) => {
                        let filtered =
                            find_commands::filter_index_entries(idx.entries(), &[], glob);
                        match filtered {
                            Err(e) => Err(e),
                            Ok(filtered) => {
                                let paths: Vec<String> =
                                    filtered.iter().map(|e| e.rel_path.clone()).collect();
                                let file_filter = if glob.is_empty() {
                                    None
                                } else {
                                    Some(paths.as_slice())
                                };
                                properties::properties_summary(
                                    idx,
                                    file_filter,
                                    effective_format,
                                    resolve_limit(
                                        cli_limit,
                                        ctx.config_default_limit,
                                        ctx.programmatic_output,
                                    ),
                                )
                            }
                        }
                    }
                    IndexResolution::Resolved(ResolvedIndex::Scanned(build)) => {
                        properties::properties_summary(
                            &build.index,
                            None,
                            effective_format,
                            resolve_limit(
                                cli_limit,
                                ctx.config_default_limit,
                                ctx.programmatic_output,
                            ),
                        )
                    }
                    IndexResolution::Outcome(outcome) => Ok(outcome),
                },
                PropertiesAction::Rename {
                    from,
                    to,
                    glob,
                    dry_run,
                    index_flags: _, // consumed in run.rs before dispatch
                } => properties::properties_rename(
                    dir,
                    &from,
                    &to,
                    &glob,
                    dry_run,
                    effective_format,
                    snapshot_index,
                    index_path,
                ),
            }
        }
        Commands::Tags {
            glob: bare_glob,
            limit: bare_limit,
            action,
        } => {
            // M-8: see the `properties` arm — bare `hyalo tags` is `tags summary`.
            let action = action.unwrap_or(TagsAction::Summary {
                glob: bare_glob,
                limit: bare_limit,
                index_flags: IndexFlags::default(),
            });
            match action {
                TagsAction::Summary {
                    ref glob,
                    limit: cli_limit,
                    index_flags: _, // consumed in run.rs before dispatch
                } => match resolve_index(
                    snapshot_index.as_ref(),
                    dir,
                    &[],
                    glob,
                    effective_format,
                    site_prefix,
                    false,
                    &ScanOptions {
                        scan_body: false,
                        bm25_tokenize: false,
                        default_language: None,
                        frontmatter_link_props: ctx.frontmatter_link_props,
                    },
                )? {
                    IndexResolution::Resolved(ResolvedIndex::Snapshot(idx)) => {
                        let filtered =
                            find_commands::filter_index_entries(idx.entries(), &[], glob);
                        match filtered {
                            Err(e) => Err(e),
                            Ok(filtered) => {
                                let paths: Vec<String> =
                                    filtered.iter().map(|e| e.rel_path.clone()).collect();
                                let file_filter = if glob.is_empty() {
                                    None
                                } else {
                                    Some(paths.as_slice())
                                };
                                tag_commands::tags_summary(
                                    idx,
                                    file_filter,
                                    effective_format,
                                    resolve_limit(
                                        cli_limit,
                                        ctx.config_default_limit,
                                        ctx.programmatic_output,
                                    ),
                                )
                            }
                        }
                    }
                    IndexResolution::Resolved(ResolvedIndex::Scanned(build)) => {
                        tag_commands::tags_summary(
                            &build.index,
                            None,
                            effective_format,
                            resolve_limit(
                                cli_limit,
                                ctx.config_default_limit,
                                ctx.programmatic_output,
                            ),
                        )
                    }
                    IndexResolution::Outcome(outcome) => Ok(outcome),
                },
                TagsAction::Rename {
                    from,
                    to,
                    glob,
                    dry_run,
                    index_flags: _, // consumed in run.rs before dispatch
                } => tag_commands::tags_rename(
                    dir,
                    &from,
                    &to,
                    &glob,
                    dry_run,
                    effective_format,
                    snapshot_index,
                    index_path,
                ),
            }
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
        } => match resolve_index(
            snapshot_index.as_ref(),
            dir,
            &[],
            &glob,
            effective_format,
            site_prefix,
            true,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: ctx.frontmatter_link_props,
            },
        )? {
            IndexResolution::Resolved(resolved) => {
                // Summary always reports orphan/dead-end counts which rely on
                // wikilink resolution, so the stem map is always needed.
                let ci =
                    maybe_case_index(ctx.case_insensitive_mode, dir, true, resolved.as_snapshot());
                summary_commands::summary(
                    dir,
                    resolved.as_index(),
                    &glob,
                    recent,
                    depth,
                    site_prefix,
                    effective_format,
                    ctx.schema,
                    ctx.lint_ignore,
                    ci.as_ref(),
                )
            }
            IndexResolution::Outcome(outcome) => Ok(outcome),
        },
        Commands::Set {
            file_positional,
            properties,
            tag,
            file,
            glob,
            files_from: _, // resolved in run.rs before dispatch
            iteration,
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
                iteration,
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
            mut file,
            glob,
            files_from: _, // resolved in run.rs before dispatch
            where_properties,
            where_tags,
            dry_run,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            if !file_positional.is_empty() {
                file = file_positional;
            }
            let where_prop_filters = match parse_where_filters(&where_properties, &where_tags) {
                Ok(f) => f,
                Err(e) => {
                    return Ok(CommandOutcome::UserError(crate::output::format_error(
                        effective_format,
                        &e,
                        None,
                        None,
                        None,
                    )));
                }
            };
            remove_commands::remove(
                dir,
                &properties,
                &tag,
                &file,
                &glob,
                &where_prop_filters,
                &where_tags,
                effective_format,
                snapshot_index,
                index_path,
                dry_run,
            )
        }
        Commands::Append {
            file_positional,
            properties,
            mut file,
            glob,
            files_from: _, // resolved in run.rs before dispatch
            where_properties,
            where_tags,
            dry_run,
            validate,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            if !file_positional.is_empty() {
                file = file_positional;
            }
            let where_prop_filters = match parse_where_filters(&where_properties, &where_tags) {
                Ok(f) => f,
                Err(e) => {
                    return Ok(CommandOutcome::UserError(crate::output::format_error(
                        effective_format,
                        &e,
                        None,
                        None,
                        None,
                    )));
                }
            };
            let do_validate = validate || ctx.validate_on_write;
            append_commands::append(
                dir,
                &properties,
                &file,
                &glob,
                &where_prop_filters,
                &where_tags,
                effective_format,
                snapshot_index,
                index_path,
                dry_run,
                do_validate,
                if do_validate { Some(ctx.schema) } else { None },
            )
        }
        Commands::Backlinks {
            selection,
            limit: cli_limit,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // iter-238: `--iteration <ID>` support (single-file command).
            let selection = match crate::commands::iteration::selection_with_iteration_resolved(
                &selection,
                dir,
                ctx.schema,
                effective_format,
            ) {
                Ok(s) => s,
                Err(outcome) => return Ok(outcome),
            };
            match resolve_inputs(
                &selection,
                dir,
                ctx.configured_dir_str,
                snapshot_index.as_ref(),
                &ResolutionPolicy::Single { allow_glob: false },
                effective_format,
                mode_enabled(ctx.case_insensitive_mode, dir),
            )? {
                ResolvedInputsOrOutcome::Outcome(o) => Ok(o),
                ResolvedInputsOrOutcome::Resolved(r) => {
                    ctx.files_from_counters = r.counters;
                    let (_full, file) = r
                        .files
                        .into_iter()
                        .next()
                        .context("Single resolution returned no files")?;
                    match resolve_index(
                        snapshot_index.as_ref(),
                        dir,
                        &[],
                        &[],
                        effective_format,
                        site_prefix,
                        true,
                        &ScanOptions {
                            scan_body: true,
                            bm25_tokenize: false,
                            default_language: None,
                            frontmatter_link_props: ctx.frontmatter_link_props,
                        },
                    )? {
                        IndexResolution::Resolved(resolved) => backlinks_commands::backlinks(
                            resolved.as_index(),
                            &file,
                            dir,
                            effective_format,
                            resolve_limit(
                                cli_limit,
                                ctx.config_default_limit,
                                ctx.programmatic_output,
                            ),
                            mode_enabled(ctx.case_insensitive_mode, dir),
                        ),
                        IndexResolution::Outcome(outcome) => Ok(outcome),
                    }
                }
            }
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
        Commands::Links { action } => match action.unwrap_or(LinksAction::Fix {
            dry_run: true,
            apply: false,
            threshold: 0.8,
            apply_fuzzy: false,
            min_confidence: None,
            glob: vec![],
            ignore_target: vec![],
            expand_short_form: false,
            index_flags: IndexFlags::default(),
        }) {
            LinksAction::Fix {
                dry_run: _,
                apply,
                threshold,
                apply_fuzzy,
                min_confidence,
                glob,
                ignore_target,
                expand_short_form,
                index_flags: _, // consumed in run.rs before dispatch
            } => {
                // Scope the immutable borrow of snapshot_index (via resolve_index)
                // so we can borrow it mutably for index updates afterwards.
                let (outcome, modified_files, had_failures) = match resolve_index(
                    snapshot_index.as_ref(),
                    dir,
                    &[],
                    &[],
                    effective_format,
                    site_prefix,
                    true,
                    &ScanOptions {
                        scan_body: true,
                        bm25_tokenize: false,
                        default_language: None,
                        frontmatter_link_props: ctx.frontmatter_link_props,
                    },
                )? {
                    IndexResolution::Resolved(resolved) => {
                        // `links fix` is entirely about link resolution.
                        let ci = maybe_case_index(
                            ctx.case_insensitive_mode,
                            dir,
                            true,
                            resolved.as_snapshot(),
                        );
                        links_commands::links_fix(
                            resolved.as_index(),
                            dir,
                            site_prefix,
                            &glob,
                            !apply,
                            threshold,
                            &ignore_target,
                            effective_format,
                            ci.as_ref(),
                            expand_short_form,
                            links_commands::FuzzyApply {
                                apply_fuzzy,
                                min_confidence,
                                config_min_confidence: ctx.config_fuzzy_min_confidence,
                            },
                        )?
                    }
                    IndexResolution::Outcome(outcome) => (outcome, Vec::new(), false),
                };
                // L-11: a mid-batch write failure yields a non-zero exit code
                // even though the envelope is emitted in full.
                if had_failures {
                    ctx.exit_code_override = Some(1);
                }
                // resolved is dropped — safe to borrow snapshot_index mutably.
                patch_index_for_modified_files(snapshot_index, index_path, dir, &modified_files)?;
                Ok(outcome)
            }
            LinksAction::Auto {
                dry_run: _,
                apply,
                min_length,
                exclude_title,
                first_only,
                no_first_only,
                exclude_target_glob,
                no_warn_common_titles,
                file,
                glob,
                index_flags: _, // consumed in run.rs before dispatch
            } => {
                let (outcome, modified_files, had_failures) = match resolve_index(
                    snapshot_index.as_ref(),
                    dir,
                    &[],
                    &[],
                    effective_format,
                    site_prefix,
                    true,
                    &ScanOptions {
                        scan_body: false,
                        bm25_tokenize: false,
                        default_language: None,
                        frontmatter_link_props: ctx.frontmatter_link_props,
                    },
                )? {
                    IndexResolution::Resolved(resolved) => links_commands::links_auto(
                        resolved.as_index(),
                        dir,
                        apply,
                        &links_commands::AutoFilters {
                            min_length,
                            cli_exclude_titles: &exclude_title,
                            cli_exclude_target_globs: &exclude_target_glob,
                            cli_first_only: first_only,
                            cli_no_first_only: no_first_only,
                            config_exclude_titles: ctx.auto_link_exclude_titles,
                            config_exclude_target_globs: ctx.auto_link_exclude_target_globs,
                            config_first_only: ctx.auto_link_first_only,
                            cli_no_warn_common_titles: no_warn_common_titles,
                            config_warn_common_titles: ctx.auto_link_warn_common_titles,
                        },
                        file.as_deref(),
                        &glob,
                        effective_format,
                    )?,
                    IndexResolution::Outcome(outcome) => (outcome, Vec::new(), false),
                };
                // L-11: partial write failure ⇒ non-zero exit code.
                if had_failures {
                    ctx.exit_code_override = Some(1);
                }
                patch_index_for_modified_files(snapshot_index, index_path, dir, &modified_files)?;
                Ok(outcome)
            }
        },
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
            let action = action.unwrap_or(LintRulesAction::List {
                enabled_only: false,
                disabled_only: false,
                rule_prefix: None,
            });
            let md_engine = hyalo_mdlint::HyaloLintEngine::create()
                .map_err(|e| anyhow::anyhow!("failed to create lint engine: {e}"))?;
            match action {
                LintRulesAction::List {
                    enabled_only,
                    disabled_only,
                    rule_prefix,
                } => Ok(lint_rules_commands::list_rules(
                    ctx.config_dir,
                    &md_engine,
                    ctx.md_lint,
                    ctx.schema,
                    enabled_only,
                    disabled_only,
                    rule_prefix.as_deref(),
                    effective_format,
                )),
                LintRulesAction::Show { rule_id } => Ok(lint_rules_commands::show_rule(
                    &rule_id,
                    &md_engine,
                    ctx.md_lint,
                    ctx.schema,
                    ctx.user_format,
                )),
                LintRulesAction::Set {
                    rule_id,
                    enabled,
                    severity,
                    dry_run,
                } => lint_rules_commands::set_rule(
                    ctx.config_dir,
                    &rule_id,
                    enabled,
                    severity.as_deref(),
                    dry_run,
                    &md_engine,
                    ctx.md_lint,
                    ctx.user_format,
                ),
                LintRulesAction::Remove { rule_id, dry_run } => lint_rules_commands::remove_rule(
                    ctx.config_dir,
                    &rule_id,
                    dry_run,
                    &md_engine,
                    ctx.md_lint,
                    ctx.user_format,
                ),
            }
        }
        // `Init`, `Deinit`, and `Completion` are handled as early returns before dispatch is called.
        Commands::Init { .. } => unreachable!("Init is dispatched before this match reached"),
        Commands::Deinit => unreachable!("Deinit is dispatched before this match reached"),
        Commands::Completion { .. } => {
            unreachable!("Completion is dispatched before this match reached")
        }
        Commands::Views { action } => {
            // ARCH-1 (iter-225): the arm body now lives in `commands::views::run`.
            views_commands::run(ctx, action)
        }
        Commands::Types { action } => {
            let action = action.unwrap_or(TypesAction::List);
            match action {
                TypesAction::List => Ok(crate::commands::types::list_types(ctx.schema)),
                TypesAction::Show { type_name } => Ok(crate::commands::types::show_type(
                    &type_name,
                    ctx.schema,
                    effective_format,
                )),
                TypesAction::Remove { type_name } => crate::commands::types::remove_type(
                    ctx.config_dir,
                    &type_name,
                    effective_format,
                ),
                TypesAction::Set {
                    type_name,
                    required,
                    default,
                    property_type,
                    property_values,
                    filename_template,
                    dry_run,
                } => crate::commands::types::set_type(
                    ctx.config_dir,
                    &type_name,
                    &required,
                    &default,
                    &property_type,
                    &property_values,
                    filename_template.as_deref(),
                    dry_run,
                    effective_format,
                    ctx.case_insensitive_mode,
                ),
            }
        }
        Commands::New {
            r#type,
            file,
            index_flags: _,
        } => crate::commands::new::create_new(
            ctx.dir,
            &r#type,
            &file,
            ctx.schema,
            snapshot_index,
            index_path,
            effective_format,
        ),
        Commands::Okf { action } => match action {
            OkfAction::Index {
                scope,
                apply,
                dry_run: _,
                replace,
            } => {
                let case_insensitive = mode_enabled(ctx.case_insensitive_mode, ctx.dir);
                let (outcome, exit_override) = crate::commands::okf::run_index(
                    ctx.dir,
                    scope.as_deref(),
                    apply,
                    replace,
                    ctx.okf_ignore,
                    case_insensitive,
                    effective_format,
                )?;
                if let Some(code) = exit_override {
                    ctx.exit_code_override = Some(code);
                }
                Ok(outcome)
            }
            OkfAction::Log {
                target,
                message,
                action: log_action,
                apply,
                dry_run: _,
            } => crate::commands::okf::run_log(
                ctx.dir,
                target.as_deref(),
                &message,
                log_action.as_deref(),
                apply,
                effective_format,
            ),
        },
        Commands::Madr { action } => match action {
            MadrAction::Toc {
                adr_dir,
                apply,
                dry_run: _,
                replace,
            } => {
                let (outcome, exit_override) = crate::commands::madr::run_toc(
                    ctx.dir,
                    adr_dir.as_deref(),
                    apply,
                    replace,
                    ctx.schema,
                    &ctx.lint_profiles,
                    effective_format,
                )?;
                if let Some(code) = exit_override {
                    ctx.exit_code_override = Some(code);
                }
                Ok(outcome)
            }
        },
        Commands::Changelog { action } => match action {
            ChangelogAction::Release {
                version,
                date,
                apply,
                dry_run: _,
            } => {
                let changelog_file = match crate::commands::changelog::resolve_changelog_target(
                    ctx.dir,
                    ctx.config_dir,
                    ctx.changelog_path,
                    effective_format,
                ) {
                    crate::commands::changelog::ChangelogTarget::Path(p) => p,
                    crate::commands::changelog::ChangelogTarget::Refused(o) => return Ok(o),
                };
                let boundary_root = crate::commands::changelog::changelog_boundary_root(
                    ctx.dir,
                    ctx.config_dir,
                    ctx.changelog_path,
                );
                let (outcome, exit_override) = crate::commands::changelog::run_release(
                    &changelog_file,
                    &boundary_root,
                    &version,
                    date.as_deref(),
                    apply,
                    &ctx.lint_profiles,
                    effective_format,
                )?;
                if let Some(code) = exit_override {
                    ctx.exit_code_override = Some(code);
                }
                Ok(outcome)
            }
            ChangelogAction::Add {
                category,
                message,
                wrap,
                apply,
                dry_run: _,
            } => {
                let changelog_file = match crate::commands::changelog::resolve_changelog_target(
                    ctx.dir,
                    ctx.config_dir,
                    ctx.changelog_path,
                    effective_format,
                ) {
                    crate::commands::changelog::ChangelogTarget::Path(p) => p,
                    crate::commands::changelog::ChangelogTarget::Refused(o) => return Ok(o),
                };
                let boundary_root = crate::commands::changelog::changelog_boundary_root(
                    ctx.dir,
                    ctx.config_dir,
                    ctx.changelog_path,
                );
                let (outcome, exit_override) = crate::commands::changelog::run_add(
                    &changelog_file,
                    &boundary_root,
                    &category,
                    &message,
                    wrap,
                    apply,
                    &ctx.lint_profiles,
                    effective_format,
                )?;
                if let Some(code) = exit_override {
                    ctx.exit_code_override = Some(code);
                }
                Ok(outcome)
            }
        },
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
