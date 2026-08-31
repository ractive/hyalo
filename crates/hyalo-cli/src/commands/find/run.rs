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

use super::{find, needs_body, project_filenames_only, project_filenames0};

/// Handler for `Commands::Find`.
///
/// `pattern` and `filters` are moved straight out of the clap enum; `view`
/// and `index_flags` were consumed earlier in `run.rs`/dispatch (view merge,
/// `--index` snapshot loading), so they never reach here.
#[allow(clippy::needless_pass_by_value)] // args moved verbatim from the clap variant
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
        files_from: _, // resolved in run.rs before dispatch
    } = filters_raw;
    if orphan && dead_end {
        crate::warn::warn(
            "--orphan and --dead-end are mutually exclusive (no file can be both); results will always be empty",
        );
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
                &e,
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

    let sort_needs_backlinks = matches!(
        sort_field.as_ref(),
        Some(hyalo_core::filter::SortField::BacklinksCount)
    );
    let sort_needs_links = matches!(
        sort_field.as_ref(),
        Some(hyalo_core::filter::SortField::LinksCount)
    );
    let sort_needs_title = matches!(
        sort_field.as_ref(),
        Some(hyalo_core::filter::SortField::Title)
    );
    let has_task_filter = task_filter.is_some();
    let has_section_filter = !section_filters.is_empty();
    let has_title_filter = title.is_some();
    // BM25 pattern search requires reading file bodies for each candidate.
    let has_bm25_search = pattern.is_some();
    let needs_body = needs_body(&parsed_fields, has_task_filter, has_section_filter)
        || sort_needs_links
        || sort_needs_title
        || broken_links
        || orphan
        || dead_end
        || has_title_filter
        || has_bm25_search;
    let needs_full_vault = parsed_fields.backlinks || sort_needs_backlinks || orphan || dead_end;
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
            // iter-251: an empty result set is where an agent most needs a
            // next step. Collect the distinct values each filtered property
            // key actually carries, reusing the index this query already
            // walked, so the hint layer can offer a did-you-mean and name the
            // real values instead of printing a bare `No results`.
            if matches!(outcome, CommandOutcome::Success { total: Some(0), .. }) {
                ctx.zero_result_values =
                    observed_property_values(resolved.as_index(), &prop_filters);
                // iter-258: `--property 'title~=/DEC-25/'` against a vault whose
                // `DEC-NNN` ids are `##` body headings is correct *and* useless:
                // the caller wanted `find -e 'DEC-25'`. Probe (bounded, and only
                // here) whether body text actually matches before saying so.
                ctx.zero_result_body_search = zero_result_body_search(
                    resolved.as_index(),
                    dir,
                    &prop_filters,
                    pattern.is_some() || regexp.is_some(),
                );
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

/// Files the zero-result body probe will open before giving up (iter-258).
///
/// The probe runs only when a `find` matched nothing *and* filtered on a
/// property regex, so the ceiling bounds a path that is already the cheapest
/// one in the command: a vault larger than this simply does not get the hint,
/// which is the right trade against turning every empty query into a full body
/// scan.
const BODY_PROBE_MAX_FILES: usize = 512;

/// Bytes the zero-result body probe will read before giving up (iter-258).
///
/// Accounted from [`hyalo_core::index::IndexEntry::size`], which a snapshot
/// written before iteration 252 reports as `0`; against such an index only
/// [`BODY_PROBE_MAX_FILES`] bounds the probe.
const BODY_PROBE_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Body-line visitor that answers one question — "does this regex match
/// anywhere in this file?" — and stops the scan the moment it knows.
///
/// Deliberately not [`hyalo_core::content_search::ContentSearchVisitor`]: that
/// one collects every match with its section context, which is exactly the
/// work a yes/no probe must not do.
struct BodyProbeVisitor<'a> {
    re: &'a regex::Regex,
    hit: bool,
}

impl hyalo_core::scanner::FileVisitor for BodyProbeVisitor<'_> {
    fn on_body_line(
        &mut self,
        raw: &str,
        _cleaned: &str,
        _line_num: usize,
    ) -> hyalo_core::scanner::ScanAction {
        self.probe(raw)
    }

    fn on_code_block_line(
        &mut self,
        raw: &str,
        _line_num: usize,
    ) -> hyalo_core::scanner::ScanAction {
        // `find -e` searches fenced code too, so the probe must agree.
        self.probe(raw)
    }

    fn needs_frontmatter(&self) -> bool {
        false
    }
}

impl BodyProbeVisitor<'_> {
    fn probe(&mut self, raw: &str) -> hyalo_core::scanner::ScanAction {
        if self.re.is_match(raw) {
            self.hit = true;
            hyalo_core::scanner::ScanAction::Stop
        } else {
            hyalo_core::scanner::ScanAction::Continue
        }
    }
}

/// Whether a zero-result `--property K~=RE` query's regex matches body prose,
/// and so deserves a "search bodies instead" hint (iter-258).
///
/// Returns `None` — no hint — unless every one of these holds:
///
/// * the query did not already search bodies (`PATTERN` / `-e`), because a
///   caller who did needs no lesson about body search;
/// * some `--property K~=RE` filter is active, with a non-empty pattern (an
///   empty regex matches the first line of the first file and would suggest
///   `find -e ''`);
/// * the regex, compiled the way `find -e` compiles it, matches a body line
///   within [`BODY_PROBE_MAX_FILES`] / [`BODY_PROBE_MAX_BYTES`].
///
/// The last point is why the hint can be stated as a fact: it is only emitted
/// after the suggested command has been shown to return something. The probe
/// deliberately ignores `--file` / `--glob` scoping and every other filter, and
/// the suggested command drops them to match — a hint that promised results
/// inside a narrower scope than it checked would be a lie.
fn zero_result_body_search(
    index: &dyn hyalo_core::index::VaultIndex,
    dir: &std::path::Path,
    filters: &[hyalo_core::filter::PropertyFilter],
    already_searched_body: bool,
) -> Option<crate::hints::BodySearchSuggestion> {
    use hyalo_core::filter::PropertyFilter;

    if already_searched_body {
        return None;
    }
    let (key, pattern) = filters.iter().find_map(|f| match f {
        PropertyFilter::RegexMatch { key, pattern } => Some((key.as_str(), pattern.as_str())),
        _ => None,
    })?;
    if pattern.is_empty() {
        return None;
    }
    // Compile exactly what `hyalo find -e <pattern>` compiles — case-insensitive
    // by default — so the hint and the command it suggests cannot disagree.
    let probe = regex::RegexBuilder::new(&format!("(?i){pattern}"))
        .size_limit(1 << 20)
        .build()
        .ok()?;

    let mut files_left = BODY_PROBE_MAX_FILES;
    let mut bytes_left = BODY_PROBE_MAX_BYTES;
    for entry in index.entries() {
        // Both ceilings are checked *before* the read, so the budget is a real
        // bound rather than one the last file is allowed to overrun.
        if files_left == 0 || entry.size > bytes_left {
            return None;
        }
        files_left -= 1;
        bytes_left -= entry.size;
        let mut visitor = BodyProbeVisitor {
            re: &probe,
            hit: false,
        };
        // A file the index knows but the filesystem does not (a snapshot built
        // elsewhere) is skipped, not escalated: this is a hint, not a query.
        if hyalo_core::scanner::scan_file_multi(&dir.join(&entry.rel_path), &mut [&mut visitor])
            .is_err()
        {
            continue;
        }
        if visitor.hit {
            return Some(crate::hints::BodySearchSuggestion {
                key: key.to_owned(),
                pattern: pattern.to_owned(),
            });
        }
    }
    None
}

/// Maximum distinct values collected per property key for the zero-result
/// did-you-mean. A key with more values than this is not a controlled
/// vocabulary, so naming them all would be noise rather than a correction.
const MAX_OBSERVED_VALUES: usize = 50;

/// Distinct scalar values of every key named by an equality `--property`
/// filter, in first-seen-sorted order.
///
/// Only equality (`K=V`) filters are probed: an existence, absence, or regex
/// filter has no misspelled value to correct. Non-scalar values (maps,
/// sequences) are skipped — a did-you-mean over `[a, b]` would suggest
/// something the user cannot type back into `--property`.
fn observed_property_values(
    index: &dyn hyalo_core::index::VaultIndex,
    filters: &[hyalo_core::filter::PropertyFilter],
) -> std::collections::BTreeMap<String, Vec<String>> {
    use hyalo_core::filter::{FilterOp, PropertyFilter};

    let keys: Vec<&str> = filters
        .iter()
        .filter_map(|f| match f {
            PropertyFilter::Scalar {
                name,
                op: FilterOp::Eq,
                value: Some(_),
            } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    if keys.is_empty() {
        return std::collections::BTreeMap::new();
    }

    let mut out: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> = keys
        .iter()
        .map(|k| ((*k).to_owned(), std::collections::BTreeSet::new()))
        .collect();
    for entry in index.entries() {
        for key in &keys {
            let Some(value) = entry.properties.get(*key) else {
                continue;
            };
            let Some(rendered) = scalar_to_string(value) else {
                continue;
            };
            let bucket = out.entry((*key).to_owned()).or_default();
            if bucket.len() < MAX_OBSERVED_VALUES {
                bucket.insert(rendered);
            }
        }
    }
    out.into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect()
}

/// Render a scalar JSON value the way `--property K=V` would accept it back.
/// Returns `None` for maps and sequences, which have no single typeable form.
fn scalar_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
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

    // --- iter-258: the zero-result body probe -------------------------------

    fn probe_vault(files: &[(&str, &str)]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for (rel, body) in files {
            std::fs::write(tmp.path().join(rel), body).unwrap();
        }
        tmp
    }

    fn probe_index(dir: &std::path::Path) -> hyalo_core::index::ScannedIndex {
        let files = hyalo_core::discovery::discover_files(dir).unwrap();
        let pairs: Vec<(std::path::PathBuf, String)> = files
            .iter()
            .map(|p| (p.clone(), hyalo_core::discovery::relative_path(dir, p)))
            .collect();
        hyalo_core::index::ScannedIndex::build(
            &pairs,
            None,
            &hyalo_core::index::ScanOptions {
                scan_body: false,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap()
        .index
    }

    fn probe(dir: &std::path::Path, filter: &str, already_searched_body: bool) -> Option<String> {
        let index = probe_index(dir);
        let filters = vec![hyalo_core::filter::parse_property_filter(filter).unwrap()];
        zero_result_body_search(&index, dir, &filters, already_searched_body)
            .map(|s| format!("{}:{}", s.key, s.pattern))
    }

    /// The motivating case: `DEC-NNN` lives in `##` headings, not in `title`.
    #[test]
    fn body_probe_fires_when_only_the_body_matches() {
        let tmp = probe_vault(&[(
            "decision-log.md",
            "---\ntitle: Decision log\n---\n\n## DEC-251 — something\n",
        )]);
        assert_eq!(
            probe(tmp.path(), "title~=/DEC-25/", false).as_deref(),
            Some("title:DEC-25")
        );
    }

    /// The absence half of the same rule: no body match ⇒ no promise.
    #[test]
    fn body_probe_stays_silent_when_the_body_does_not_match_either() {
        let tmp = probe_vault(&[(
            "decision-log.md",
            "---\ntitle: Decision log\n---\n\n## DEC-251 — something\n",
        )]);
        assert_eq!(probe(tmp.path(), "title~=/NOSUCHTHING/", false), None);
    }

    #[test]
    fn body_probe_skips_a_query_that_already_searched_bodies() {
        let tmp = probe_vault(&[("a.md", "---\ntitle: A\n---\n\n## DEC-251\n")]);
        assert_eq!(
            probe(tmp.path(), "title~=/DEC-25/", true),
            None,
            "a caller who already passed PATTERN / -e needs no body-search hint"
        );
    }

    #[test]
    fn body_probe_ignores_non_regex_property_filters() {
        let tmp = probe_vault(&[("a.md", "---\ntitle: A\n---\n\n## DEC-251\n")]);
        let index = probe_index(tmp.path());
        let filters = vec![
            hyalo_core::filter::parse_property_filter("status=draft").unwrap(),
            hyalo_core::filter::parse_property_filter("!archived").unwrap(),
        ];
        assert!(zero_result_body_search(&index, tmp.path(), &filters, false).is_none());
    }

    /// An empty regex matches the first body line of the first file, which
    /// would suggest the useless `hyalo find -e ''`.
    #[test]
    fn body_probe_refuses_an_empty_pattern() {
        let tmp = probe_vault(&[("a.md", "---\ntitle: A\n---\n\nbody\n")]);
        assert_eq!(probe(tmp.path(), "title~=", false), None);
    }

    /// The probe matches fenced code, because `find -e` does.
    #[test]
    fn body_probe_matches_inside_fenced_code() {
        let tmp = probe_vault(&[(
            "a.md",
            "---\ntitle: A\n---\n\n```sh\nhyalo find -e DEC-251\n```\n",
        )]);
        assert_eq!(
            probe(tmp.path(), "title~=/DEC-251/", false).as_deref(),
            Some("title:DEC-251")
        );
    }

    /// `find -e` is case-insensitive, so the probe must be too — otherwise a
    /// case-sensitive property regex could suppress a hint whose suggested
    /// command would in fact have found something.
    #[test]
    fn body_probe_is_case_insensitive_like_find_e() {
        let tmp = probe_vault(&[("a.md", "---\ntitle: A\n---\n\n## dec-251\n")]);
        assert_eq!(
            probe(tmp.path(), "title~=/DEC-251/", false).as_deref(),
            Some("title:DEC-251")
        );
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
