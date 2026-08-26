#![allow(clippy::missing_errors_doc)]
//! ARCH-1 (iter-225): the `hyalo find` dispatch arm, extracted verbatim from
//! `dispatch.rs` so its filter-merge / warning-policy / stem-map policy is
//! unit-testable in-process instead of only via an e2e spawn.

use anyhow::Result;

use crate::cli::args::FindFilters;
use crate::commands::{IndexResolution, resolve_index};
use crate::dispatch::{
    CommandContext, find_needs_stem_map, maybe_case_index, property_filter_error_outcome,
    resolve_limit,
};
use crate::output::CommandOutcome;

use super::{find, needs_body, project_filenames0, project_filenames_only};

/// Handler for `Commands::Find`.
///
/// `pattern` and `filters` are moved straight out of the clap enum; `view`
/// and `index_flags` were consumed earlier in `run.rs`/dispatch (view merge,
/// `--index` snapshot loading), so they never reach here.
pub(crate) fn run(
    ctx: &mut CommandContext<'_>,
    pattern: Option<String>,
    file_positional: Vec<String>,
    mut filters_raw: FindFilters,
) -> Result<CommandOutcome> {
    let dir = ctx.dir;
    let site_prefix = ctx.site_prefix;
    let effective_format = ctx.effective_format;
    let snapshot_index = &mut *ctx.snapshot_index;

    // Merge positional files into filters (clap prevents positional+--file
    // and positional+--glob at parse time; a view may have set glob though).
    if !file_positional.is_empty() {
        if !filters_raw.glob.is_empty() {
            crate::warn::warn(
                "positional file arguments override the view's --glob; \
                 glob filter has been ignored",
            );
        }
        filters_raw.file = file_positional;
        filters_raw.glob.clear(); // file overrides view's glob
    }
    let FindFilters {
        pattern: _, // pattern is handled in run.rs before dispatch
        regexp,
        properties,
        tag,
        task,
        sections,
        file,
        glob,
        fields,
        sort,
        reverse,
        limit,
        broken_links,
        strict,
        orphan,
        dead_end,
        title,
        language,
        filenames_only,
        filenames0,
        iteration,
        files_from: _, // resolved in run.rs before dispatch
    } = filters_raw;
    if orphan && dead_end {
        crate::warn::warn(
            "--orphan and --dead-end are mutually exclusive (no file can be both); results will always be empty",
        );
    }
    // Resolve --iteration <ID> into glob patterns from the schema's
    // type filename_templates (iter-235). The globs join the --glob
    // set with OR semantics (positive globs are unioned), so
    // `--iteration 206` is just another way to scope the same find.
    let mut glob = glob;
    if let Some(id_str) = iteration {
        match hyalo_core::iteration_id::parse_iteration_id(&id_str) {
            Ok(id) => {
                match crate::commands::iteration::resolve_iteration_globs(
                    ctx.schema,
                    &id,
                    effective_format,
                ) {
                    crate::commands::iteration::IterationGlobs::Globs(g) => {
                        glob.extend(g);
                    }
                    crate::commands::iteration::IterationGlobs::Outcome(o) => {
                        return Ok(o);
                    }
                }
            }
            Err(e) => {
                return Ok(CommandOutcome::UserError(crate::output::format_error(
                    effective_format,
                    &e.to_string(),
                    Some(&id_str),
                    Some(
                        "pass a bare integer (206), zero-padded integer (01), or integer + letter suffix (16b)",
                    ),
                    None,
                )));
            }
        }
    }
    // Parse property filters
    let prop_filters: Vec<hyalo_core::filter::PropertyFilter> = match properties
        .iter()
        .map(|s| hyalo_core::filter::parse_property_filter(s))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(f) => f,
        Err(e) => {
            return Ok(property_filter_error_outcome(&e, effective_format));
        }
    };
    // Parse task filter
    let task_filter = match task.as_deref().map(hyalo_core::filter::parse_task_filter) {
        Some(Ok(f)) => Some(f),
        Some(Err(e)) => {
            return Ok(CommandOutcome::UserError(crate::output::format_error(
                effective_format,
                &e.to_string(),
                None,
                None,
                None,
            )));
        }
        None => None,
    };
    // Parse fields
    let parsed_fields = match hyalo_core::filter::Fields::parse(&fields) {
        Ok(f) => f,
        Err(e) => {
            return Ok(CommandOutcome::UserError(crate::output::format_error(
                effective_format,
                &e.to_string(),
                None,
                None,
                None,
            )));
        }
    };
    // Parse sort
    let sort_field = match sort.as_deref().map(hyalo_core::filter::parse_sort) {
        Some(Ok(f)) => Some(f),
        Some(Err(e)) => {
            return Ok(CommandOutcome::UserError(crate::output::format_error(
                effective_format,
                &e.to_string(),
                None,
                None,
                None,
            )));
        }
        None => None,
    };
    // Parse section filters
    let section_filters: Vec<hyalo_core::heading::SectionFilter> = match sections
        .iter()
        .map(|s| hyalo_core::heading::SectionFilter::parse(s))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(f) => f,
        Err(e) => {
            return Ok(CommandOutcome::UserError(crate::output::format_error(
                effective_format,
                &e.to_string(),
                None,
                None,
                None,
            )));
        }
    };

    for t in &tag {
        if let Err(msg) = crate::commands::tags::validate_tag(t) {
            return Ok(CommandOutcome::UserError(crate::output::format_error(
                effective_format,
                &msg,
                None,
                None,
                None,
            )));
        }
    }

    // Validate --language flag and config language against supported languages.
    if let Some(ref lang) = language
        && let Err(e) = hyalo_core::bm25::parse_language(lang)
    {
        return Ok(CommandOutcome::UserError(crate::output::format_error(
            effective_format,
            &format!("invalid --language value {lang:?}: {e}"),
            None,
            None,
            None,
        )));
    }
    if let Some(cfg_lang) = ctx.config_language
        && let Err(e) = hyalo_core::bm25::parse_language(cfg_lang)
    {
        return Ok(CommandOutcome::UserError(crate::output::format_error(
            effective_format,
            &format!("invalid [search].language config value {cfg_lang:?}: {e}"),
            None,
            None,
            None,
        )));
    }

    // Strip the dir prefix from --file args so that
    // filter_index_entries matches vault-relative paths.
    let file: Vec<String> = file
        .into_iter()
        .map(|f| hyalo_core::discovery::strip_dir_prefix(dir, &f).unwrap_or(f))
        .collect();

    let sort_needs_backlinks =
        matches!(sort_field.as_ref(), Some(hyalo_core::filter::SortField::BacklinksCount));
    let sort_needs_links =
        matches!(sort_field.as_ref(), Some(hyalo_core::filter::SortField::LinksCount));
    let sort_needs_title =
        matches!(sort_field.as_ref(), Some(hyalo_core::filter::SortField::Title));
    let has_task_filter = task_filter.is_some();
    let has_section_filter = !section_filters.is_empty();
    let has_title_filter = title.is_some();
    // BM25 pattern search requires reading file bodies for each candidate.
    let has_bm25_search = pattern.is_some();
    let needs_body =
        needs_body(&parsed_fields, has_task_filter, has_section_filter)
            || sort_needs_links
            || sort_needs_title
            || broken_links
            || orphan
            || dead_end
            || has_title_filter
            || has_bm25_search;
    let needs_full_vault =
        parsed_fields.backlinks || sort_needs_backlinks || orphan || dead_end;
    // The link graph is only built when scan_body is true, so
    // backlinks / backlink-sort always require body scanning.
    let scan_body = needs_body || needs_full_vault;
    let needs_stem_map = find_needs_stem_map(
        broken_links,
        orphan,
        dead_end,
        parsed_fields.links,
        sort_needs_links,
    );
    match resolve_index(
        snapshot_index.as_ref(),
        dir,
        &file,
        &glob,
        effective_format,
        site_prefix,
        needs_full_vault,
        &hyalo_core::index::ScanOptions {
            scan_body,
            bm25_tokenize: false,
            default_language: None,
            frontmatter_link_props: ctx.frontmatter_link_props,
        },
    )? {
        IndexResolution::Resolved(resolved) => {
            let ci = maybe_case_index(
                ctx.case_insensitive_mode,
                dir,
                needs_stem_map,
                resolved.as_snapshot(),
            );
            let mut outcome = find(
                resolved.as_index(),
                dir,
                site_prefix,
                pattern.as_deref(),
                regexp.as_deref(),
                &prop_filters,
                &tag,
                task_filter.as_ref(),
                &section_filters,
                &file,
                &glob,
                &parsed_fields,
                sort_field.as_ref(),
                reverse,
                resolve_limit(limit, ctx.config_default_limit, ctx.programmatic_output),
                broken_links,
                orphan,
                dead_end,
                title.as_deref(),
                effective_format,
                language.as_deref(),
                ctx.config_language,
                ci.as_ref(),
            )?;
            // UX-2 (dogfood pre3): `--strict` gives any `find` query
            // (most commonly `--broken-links`) a CI-gateable exit code.
            // Pure policy function, unit-tested in-process (ARCH-1 proof).
            if let Some(code) = strict_exit_code(&outcome, strict) {
                ctx.exit_code_override = Some(code);
            }
            // iter-235: `--filenames-only` projects the find result
            // set onto raw file paths (one per line, no envelope,
            // no count, no hints). It runs *after* the `--strict`
            // check above, so `find --filenames-only --strict`
            // still flips the exit code (1 when results exist) —
            // the CI-gate + filename-list use case. RawOutput bypasses
            // the JSON pipeline entirely (no jq, no count, no hints,
            // no envelope), which is exactly the point: an agent or
            // shell pipeline wants bare paths, nothing else.
            //
            // iter-238: `--filenames0` is the NUL-delimited sibling
            // (`find -print0` precedent) for `xargs -0` / newline-safe
            // consumption; identical semantics otherwise.
            if filenames_only {
                outcome = project_filenames_only(outcome);
            } else if filenames0 {
                outcome = project_filenames0(outcome);
            }
            Ok(outcome)
        }
        IndexResolution::Outcome(outcome) => Ok(outcome),
    }
}

/// ARCH-1 proof: the `--strict` exit-code policy of `find` as a pure
/// function, previously observable only through an e2e process spawn (the
/// `find --broken-links --strict` exit code). Unit-tested in `tests` below.
#[must_use]
pub(crate) fn strict_exit_code(outcome: &CommandOutcome, strict: bool) -> Option<i32> {
    if strict
        && let CommandOutcome::Success {
            total: Some(total), ..
        } = outcome
        && *total > 0
    {
        Some(1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Format;

    fn success_with_total(total: Option<u64>) -> CommandOutcome {
        match total {
            Some(t) => CommandOutcome::success_with_total("{}".to_owned(), t),
            None => CommandOutcome::success("{}".to_owned()),
        }
    }

    /// Previously e2e-only: `--strict` with findings ⇒ exit 1 (UX-2).
    #[test]
    fn strict_exit_code_policy() {
        assert_eq!(
            strict_exit_code(&success_with_total(Some(3)), true),
            Some(1),
            "--strict with findings must exit 1"
        );
        assert_eq!(
            strict_exit_code(&success_with_total(Some(3)), false),
            None,
            "without --strict the exit code stays 0"
        );
        assert_eq!(
            strict_exit_code(&success_with_total(Some(0)), true),
            None,
            "--strict with zero findings stays 0"
        );
        assert_eq!(
            strict_exit_code(&success_with_total(None), true),
            None,
            "no total ⇒ nothing to gate on"
        );
        let err = CommandOutcome::UserError(crate::output::format_error(
            Format::Json,
            "boom",
            None,
            None,
            None,
        ));
        assert_eq!(
            strict_exit_code(&err, true),
            None,
            "non-success outcomes keep their own exit code"
        );
    }
}
