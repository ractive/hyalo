//! Zero-result hints for `find` (iteration 251).
//!
//! A query that matches nothing used to be the one place hyalo said the least:
//! `--format text` printed a bare `No results`, JSON carried `hints: []`. That
//! is the moment an agent most needs a next step, so an empty result set now
//! answers three questions instead of none:
//!
//! 1. *Did I misspell the value?* — a did-you-mean over the values property `K`
//!    actually has in the vault, fired only when the edit distance is small.
//! 2. *What values are there?* — the observed values, named inline when the
//!    scan collected them, otherwise a `properties` / `tags` listing command.
//! 3. *Which filter killed the query?* — the same query with its most
//!    selective filter dropped, ready to paste.
//!
//! The observed values come from the `find` scan itself
//! ([`crate::dispatch::CommandContext::zero_result_values`]): the index was
//! already walked to evaluate the filter, so collecting the distinct values of
//! the filtered keys on the empty path costs no extra I/O.

use std::collections::BTreeMap;

use super::{Hint, HintBuilder, HintContext};

/// Maximum hints emitted for a zero-result query (plan: 1–3).
pub(super) const MAX_ZERO_RESULT_HINTS: usize = 3;

/// Maximum edit distance between a filter value and an observed value for the
/// did-you-mean hint to fire.
const DID_YOU_MEAN_MAX_DISTANCE: usize = 2;

/// How many observed values to name inline before eliding the rest.
const MAX_NAMED_VALUES: usize = 6;

/// One active filter on the query that returned nothing.
///
/// `rank` orders filters most-selective-first; the drop hint removes the
/// lowest-ranked (i.e. most selective) one.
struct ActiveFilter {
    /// Rendered for the `No results for …` echo, e.g. `--property status=x`.
    echo: String,
    /// Selectivity rank; lower is more selective.
    rank: u8,
    /// The argv tokens this filter contributes to a rebuilt `find` command.
    argv: Vec<String>,
}

/// Collect the filters active on this `find` query, most selective first.
///
/// Selectivity is a fixed heuristic, not a measurement: a body search or an
/// exact property value almost always narrows harder than a tag or a glob, and
/// the hint only has to name a plausible culprit for the user to try dropping.
fn active_filters(ctx: &HintContext) -> Vec<ActiveFilter> {
    let mut out: Vec<ActiveFilter> = Vec::new();

    if let Some(pattern) = &ctx.body_pattern {
        out.push(ActiveFilter {
            echo: format!("'{pattern}'"),
            rank: 0,
            argv: vec![pattern.clone()],
        });
    }
    for pf in &ctx.property_filters {
        // An equality filter pins one value; `!K` / `K` / `K~=re` are broader.
        let rank = if pf.contains('=') && !pf.contains("~=") && !pf.contains("=~") {
            1
        } else {
            4
        };
        out.push(ActiveFilter {
            echo: format!("--property {pf}"),
            rank,
            argv: vec!["--property".to_owned(), pf.clone()],
        });
    }
    for sf in &ctx.section_filters {
        out.push(ActiveFilter {
            echo: format!("--section {sf}"),
            rank: 2,
            argv: vec!["--section".to_owned(), sf.clone()],
        });
    }
    if let Some(task) = &ctx.task_filter {
        out.push(ActiveFilter {
            echo: format!("--task {task}"),
            rank: 3,
            argv: vec!["--task".to_owned(), task.clone()],
        });
    }
    if let Some(title) = &ctx.title_filter {
        out.push(ActiveFilter {
            echo: format!("--title {title}"),
            rank: 3,
            argv: vec!["--title".to_owned(), title.clone()],
        });
    }
    for tf in &ctx.tag_filters {
        out.push(ActiveFilter {
            echo: format!("--tag {tf}"),
            rank: 5,
            argv: vec!["--tag".to_owned(), tf.clone()],
        });
    }
    if ctx.broken_links_filter {
        out.push(ActiveFilter {
            echo: "--broken-links".to_owned(),
            rank: 6,
            argv: vec!["--broken-links".to_owned()],
        });
    }
    if ctx.orphan_filter {
        out.push(ActiveFilter {
            echo: "--orphan".to_owned(),
            rank: 6,
            argv: vec!["--orphan".to_owned()],
        });
    }
    if ctx.dead_end_filter {
        out.push(ActiveFilter {
            echo: "--dead-end".to_owned(),
            rank: 6,
            argv: vec!["--dead-end".to_owned()],
        });
    }
    for g in &ctx.glob {
        out.push(ActiveFilter {
            echo: format!("--glob {g}"),
            rank: 7,
            argv: vec!["--glob".to_owned(), g.clone()],
        });
    }
    out.sort_by_key(|f| f.rank);
    out
}

/// Human-readable echo of the filters that produced an empty result set, e.g.
/// `--property status=x --tag y`. `None` when the query carried no filters at
/// all (an empty vault, where echoing nothing is the honest answer).
///
/// Rendered by `--format text` next to `No results` so the line names what was
/// actually asked instead of leaving the reader to reconstruct it from history.
#[must_use]
pub(crate) fn filter_echo(ctx: &HintContext) -> Option<String> {
    let filters = active_filters(ctx);
    if filters.is_empty() {
        return None;
    }
    Some(
        filters
            .iter()
            .map(|f| f.echo.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// The `K` and `V` of every `--property K=V` equality filter on the query.
fn equality_property_filters(ctx: &HintContext) -> Vec<(&str, &str)> {
    ctx.property_filters
        .iter()
        .filter_map(|pf| {
            if pf.contains("~=") || pf.contains("=~") || pf.starts_with('!') {
                return None;
            }
            let (key, value) = pf.split_once('=')?;
            // Exclude the ordering / inequality operators, whose trailing
            // character lands in `key` (`status!`, `date>`, …).
            if key.ends_with(['!', '>', '<']) || key.is_empty() || value.is_empty() {
                return None;
            }
            Some((key, value))
        })
        .collect()
}

/// Closest observed value to `value`, when it is close enough to be a typo.
///
/// Two guards keep an unrelated value from being offered as a correction: an
/// absolute ceiling of [`DID_YOU_MEAN_MAX_DISTANCE`] edits, and a relative one
/// requiring the edits to be a small fraction of the longer string — so
/// `draf` → `draft` fires while `done` → `todo` (3 edits over 4 characters)
/// does not.
fn did_you_mean<'a>(value: &str, observed: &'a [String]) -> Option<&'a str> {
    let lowered = value.to_lowercase();
    observed
        .iter()
        .filter(|candidate| !candidate.eq_ignore_ascii_case(value))
        .map(|candidate| {
            let dist = strsim::damerau_levenshtein(&lowered, &candidate.to_lowercase());
            (dist, candidate)
        })
        .filter(|(dist, candidate)| {
            let longer = lowered.chars().count().max(candidate.chars().count());
            *dist <= DID_YOU_MEAN_MAX_DISTANCE && dist * 3 <= longer
        })
        .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)))
        .map(|(_, candidate)| candidate.as_str())
}

/// Rebuild the `find` command from `ctx`, skipping the filter at `skip_index`
/// in the [`active_filters`] ordering. `None` skips nothing.
fn rebuild_find(ctx: &HintContext, filters: &[ActiveFilter], skip_index: Option<usize>) -> String {
    let mut b = HintBuilder::cmd("find");
    for (i, f) in filters.iter().enumerate() {
        if Some(i) == skip_index {
            continue;
        }
        for (n, token) in f.argv.iter().enumerate() {
            // The flag name is a literal; only its value needs quoting.
            if n == 0 && token.starts_with("--") {
                b.push_raw(token);
            } else {
                b.push_quoted(token);
            }
        }
    }
    b.finish(ctx)
}

/// Build the 1–3 hints shown when a `find` query matched nothing.
///
/// `observed` maps a property key to the distinct values that key carries in
/// the vault, as collected by the scan that returned nothing. An empty map
/// (no equality filters, or an index that could not be walked) simply drops
/// the value-aware hints.
#[must_use]
pub(super) fn zero_result_hints(ctx: &HintContext) -> Vec<Hint> {
    let observed: &BTreeMap<String, Vec<String>> = &ctx.observed_property_values;
    let mut hints: Vec<Hint> = Vec::new();
    let filters = active_filters(ctx);

    // 1. Did-you-mean over the observed values of each filtered key.
    for (key, value) in equality_property_filters(ctx) {
        if hints.len() >= MAX_ZERO_RESULT_HINTS {
            break;
        }
        let Some(values) = observed.get(key) else {
            continue;
        };
        if let Some(suggestion) = did_you_mean(value, values) {
            let corrected = format!("{key}={suggestion}");
            let mut b = HintBuilder::cmd("find");
            for f in &filters {
                for (n, token) in f.argv.iter().enumerate() {
                    if n == 0 && token.starts_with("--") {
                        b.push_raw(token);
                    } else if token == &format!("{key}={value}") {
                        b.push_quoted(&corrected);
                    } else {
                        b.push_quoted(token);
                    }
                }
            }
            hints.push(Hint::new(
                format!("Did you mean {key}={suggestion}?"),
                b.finish(ctx),
            ));
        }
    }

    // 2. Name the values the key actually has, so the next query is informed.
    for (key, _) in equality_property_filters(ctx) {
        if hints.len() >= MAX_ZERO_RESULT_HINTS {
            break;
        }
        match observed.get(key) {
            Some(values) if !values.is_empty() => {
                let shown: Vec<&str> = values
                    .iter()
                    .take(MAX_NAMED_VALUES)
                    .map(String::as_str)
                    .collect();
                let more = values.len().saturating_sub(shown.len());
                let suffix = if more > 0 {
                    format!(", … (+{more})")
                } else {
                    String::new()
                };
                hints.push(Hint::new(
                    format!(
                        "Values of `{key}` in this vault: {}{suffix}",
                        shown.join(", ")
                    ),
                    HintBuilder::cmd("find")
                        .flag_value("--property", key)
                        .flag_value("--fields", "properties")
                        .finish(ctx),
                ));
            }
            _ => {
                hints.push(Hint::new(
                    format!("No file has a `{key}` property — list the ones that exist"),
                    HintBuilder::cmd("properties summary").finish(ctx),
                ));
            }
        }
    }

    // 3. Re-run without the most selective filter.
    if filters.len() >= 2 && hints.len() < MAX_ZERO_RESULT_HINTS {
        hints.push(Hint::new(
            format!("Drop the most selective filter ({})", filters[0].echo),
            rebuild_find(ctx, &filters, Some(0)),
        ));
    }

    // Fallbacks when nothing above applied: point at the observed-value
    // listings, which is what `--tag`-only and filter-less empty queries need.
    if hints.len() < MAX_ZERO_RESULT_HINTS && !ctx.tag_filters.is_empty() {
        hints.push(Hint::new(
            "List the tags this vault actually uses",
            HintBuilder::cmd("tags summary").finish(ctx),
        ));
    }
    if hints.is_empty() {
        hints.push(Hint::new(
            "List the properties this vault actually uses",
            HintBuilder::cmd("properties summary").finish(ctx),
        ));
    }

    hints.truncate(MAX_ZERO_RESULT_HINTS);
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hints::{HintSource, generate_hints};

    fn ctx_with(properties: &[&str], tags: &[&str]) -> HintContext {
        let mut ctx = HintContext::new(HintSource::Find);
        ctx.property_filters = properties.iter().map(|s| (*s).to_owned()).collect();
        ctx.tag_filters = tags.iter().map(|s| (*s).to_owned()).collect();
        ctx
    }

    fn observed(ctx: &mut HintContext, key: &str, values: &[&str]) {
        ctx.observed_property_values.insert(
            key.to_owned(),
            values.iter().map(|s| (*s).to_owned()).collect(),
        );
    }

    #[test]
    fn did_you_mean_fires_for_one_character_typo() {
        let values = vec!["draft".to_owned(), "completed".to_owned()];
        assert_eq!(did_you_mean("draf", &values), Some("draft"));
        assert_eq!(did_you_mean("Draft", &values), None, "exact match, no typo");
    }

    #[test]
    fn did_you_mean_ignores_unrelated_values() {
        let values = vec!["todo".to_owned(), "completed".to_owned()];
        assert_eq!(
            did_you_mean("done", &values),
            None,
            "3 edits over 4 chars is a different word, not a typo"
        );
        assert_eq!(did_you_mean("nonexistent", &values), None);
    }

    #[test]
    fn zero_result_hints_are_non_empty_for_a_bad_property_value() {
        let mut ctx = ctx_with(&["status=nonexistent"], &[]);
        observed(&mut ctx, "status", &["draft", "completed"]);
        let hints = zero_result_hints(&ctx);
        assert!(!hints.is_empty(), "empty result must still hint");
        assert!(
            hints.iter().any(|h| h.description.contains("Values of")),
            "should name the observed values: {hints:?}"
        );
    }

    #[test]
    fn typo_produces_a_runnable_corrected_query() {
        let mut ctx = ctx_with(&["status=draf"], &["iteration"]);
        observed(&mut ctx, "status", &["draft", "completed"]);
        let hints = zero_result_hints(&ctx);
        let first = &hints[0];
        assert_eq!(first.description, "Did you mean status=draft?");
        assert!(
            first.cmd.contains("status=draft") && first.cmd.contains("--tag iteration"),
            "corrected query keeps the other filters: {}",
            first.cmd
        );
    }

    #[test]
    fn drop_hint_names_the_most_selective_filter() {
        let ctx = ctx_with(&["status=draft"], &["iteration"]);
        let hints = zero_result_hints(&ctx);
        let drop = hints
            .iter()
            .find(|h| h.description.starts_with("Drop the most selective"))
            .expect("two filters ⇒ a drop hint");
        assert!(drop.description.contains("--property status=draft"));
        assert!(
            !drop.cmd.contains("status=draft") && drop.cmd.contains("--tag iteration"),
            "the dropped filter is gone, the rest survives: {}",
            drop.cmd
        );
    }

    #[test]
    fn filter_echo_lists_active_filters() {
        let ctx = ctx_with(&["status=x"], &["y"]);
        assert_eq!(
            filter_echo(&ctx).as_deref(),
            Some("--property status=x --tag y")
        );
        assert_eq!(filter_echo(&HintContext::new(HintSource::Find)), None);
    }

    #[test]
    fn generate_hints_emits_them_for_an_empty_find() {
        let mut ctx = ctx_with(&["status=nonexistent"], &[]);
        observed(&mut ctx, "status", &["draft"]);
        let hints = generate_hints(&ctx, &serde_json::json!([]), Some(0));
        assert!(
            !hints.is_empty(),
            "the empty-array find path must route through zero_result_hints"
        );
        assert!(hints.len() <= MAX_ZERO_RESULT_HINTS);
    }

    #[test]
    fn hints_are_capped_at_three() {
        let mut ctx = ctx_with(&["status=draf", "type=iteratio"], &["iteration"]);
        observed(&mut ctx, "status", &["draft", "completed"]);
        observed(&mut ctx, "type", &["iteration", "decision"]);
        assert!(zero_result_hints(&ctx).len() <= MAX_ZERO_RESULT_HINTS);
    }

    #[test]
    fn unknown_key_points_at_the_properties_listing() {
        let ctx = ctx_with(&["nosuchkey=1"], &[]);
        let hints = zero_result_hints(&ctx);
        assert!(
            hints[0].description.contains("No file has a `nosuchkey`"),
            "{hints:?}"
        );
        assert!(hints[0].cmd.contains("properties summary"));
    }
}
