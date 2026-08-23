#![allow(clippy::missing_errors_doc)]
use std::path::Path;

use anyhow::{Context, Result};

use crate::output::{CommandOutcome, Format};
use hyalo_core::case_index::CaseInsensitiveIndex;
use hyalo_core::discovery;
use hyalo_core::index::VaultIndex;
use hyalo_core::link_fix::{LinkMatcher, apply_fixes, detect_broken_links_from_index, plan_fixes};

// ---------------------------------------------------------------------------
// Command entry points
// ---------------------------------------------------------------------------

/// Run `hyalo links fix` using a pre-built index.
///
/// `dry_run = true`  → preview only (default)
/// `dry_run = false` → write fixes to disk (`--apply`)
///
/// Case-mismatch fixes (rule `link-case-mismatch`) are included alongside
/// ordinary broken-link fixes when `case_index` is provided.
///
/// When `expand_short_form` is `true`, short-form wikilinks (no `/`) that
/// resolve only via stem matching are expanded to their full vault path on
/// `--apply`.  This is opt-in and documented as Obsidian-incompatible.
///
/// Returns `(CommandOutcome, modified_files)` where `modified_files` contains
/// vault-relative paths of files that were rewritten on disk.  The caller is
/// responsible for patching the snapshot index with these paths.
/// Opt-in policy for applying low-confidence fuzzy-match fixes.
///
/// Fuzzy (Jaro-Winkler) fixes are guesses: a broken `[[foo]]` can "match" an
/// unrelated `bar.md`. They are always *reported* in their own bucket but are
/// only written to disk under `--apply` when the user opts in here.
#[derive(Debug, Clone, Copy, Default)]
pub struct FuzzyApply {
    /// `--apply-fuzzy`: include fuzzy-match fixes in `--apply`.
    pub apply_fuzzy: bool,
    /// `--min-confidence <f>`: only apply fuzzy fixes at or above this
    /// confidence. Setting it implies `apply_fuzzy`.
    pub min_confidence: Option<f64>,
}

impl FuzzyApply {
    /// Whether fuzzy fixes should be applied at all (either flag opts in).
    fn enabled(&self) -> bool {
        self.apply_fuzzy || self.min_confidence.is_some()
    }

    /// Whether a fuzzy fix with the given confidence should be applied.
    fn accepts(&self, confidence: f64) -> bool {
        self.enabled() && self.min_confidence.is_none_or(|min| confidence >= min)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn links_fix(
    index: &dyn VaultIndex,
    dir: &Path,
    site_prefix: Option<&str>,
    globs: &[String],
    dry_run: bool,
    threshold: f64,
    ignore_target: &[String],
    format: Format,
    case_index: Option<&CaseInsensitiveIndex>,
    expand_short_form: bool,
    fuzzy: FuzzyApply,
) -> Result<(CommandOutcome, Vec<String>, bool)> {
    let report =
        detect_broken_links_from_index(dir, index, site_prefix, case_index, expand_short_form);

    // Compute the set of in-scope source files when --glob is provided.
    // The same scope applies to broken, case_mismatches, and ambiguous so
    // that --apply never rewrites files outside the requested scope.
    let matched_owned: Option<Vec<String>> = if globs.is_empty() {
        None
    } else {
        let all_files: Vec<std::path::PathBuf> = index
            .entries()
            .iter()
            .map(|e| dir.join(&e.rel_path))
            .collect();
        let matched = discovery::match_globs(dir, &all_files, globs)?;
        crate::warn::warn_glob_dir_overlap(dir, globs, matched.len());
        Some(matched.into_iter().map(|(_, rel)| rel).collect())
    };
    let matched_set: Option<std::collections::HashSet<&str>> = matched_owned
        .as_ref()
        .map(|v| v.iter().map(String::as_str).collect());
    let in_scope = |source: &str| match &matched_set {
        Some(set) => set.contains(source),
        None => true,
    };

    let broken: Vec<_> = report
        .broken
        .into_iter()
        .filter(|b| in_scope(b.source.as_str()))
        .collect();

    // Out-of-vault targets (`../..` above the vault root) are reported in
    // their own bucket — see `BrokenLinkReport::out_of_vault`.
    let out_of_vault_links: Vec<_> = report
        .out_of_vault
        .into_iter()
        .filter(|b| in_scope(b.source.as_str()))
        .collect();
    let out_of_vault_count = out_of_vault_links.len();

    // Filter out ignored targets (--ignore-target substrings).
    let (broken, ignored_count) = if ignore_target.is_empty() {
        (broken, 0usize)
    } else {
        let before = broken.len();
        let filtered: Vec<_> = broken
            .into_iter()
            .filter(|b| {
                !ignore_target
                    .iter()
                    .any(|pat| b.target.contains(pat.as_str()))
            })
            .collect();
        let ignored = before - filtered.len();
        (filtered, ignored)
    };

    let matcher = LinkMatcher::from_index(index, threshold, site_prefix);
    let fix_report = plan_fixes(&broken, &matcher);

    // Split low-confidence guesses into their own bucket. Fuzzy matches are
    // guesses (a broken `[[foo]]` can "match" an unrelated `bar.md`), and so
    // is a basename fallback, which throws away the directory path the author
    // actually wrote (iter-200 / M-1). Both are reported separately and
    // excluded from `--apply` unless the user opts in via `--apply-fuzzy` /
    // `--min-confidence`. The remaining `certain_fixes` are the ones plain
    // `--apply` writes.
    let (fuzzy_fixes, certain_fixes): (Vec<_>, Vec<_>) =
        fix_report.fixes.iter().cloned().partition(|f| {
            matches!(
                f.strategy,
                hyalo_core::link_fix::FixStrategy::FuzzyMatch
                    | hyalo_core::link_fix::FixStrategy::BasenameFallback
            )
        });
    // Fuzzy fixes the policy accepts (opted-in and above --min-confidence).
    let applicable_fuzzy: Vec<_> = fuzzy_fixes
        .iter()
        .filter(|f| fuzzy.accepts(f.confidence))
        .cloned()
        .collect();

    // Collect all fixes: broken-link fixes + case-mismatch fixes.
    // Case-mismatch fixes come from the detection phase (not from plan_fixes).
    let case_mismatches: Vec<_> = report
        .case_mismatches
        .into_iter()
        .filter(|f| in_scope(f.source.as_str()))
        .collect();
    let case_mismatch_count = case_mismatches.len();
    // Ambiguous short-form links — reported but never auto-fixed.
    let ambiguous: Vec<_> = report
        .ambiguous
        .into_iter()
        .filter(|b| in_scope(b.source.as_str()))
        .collect();
    let ambiguous_count = ambiguous.len();

    let mut modified_files = Vec::new();
    // Fixes that were part of the plan but produced no on-disk change (e.g. a
    // frontmatter occurrence whose text no longer matched what detection saw).
    // L-25: dry-run also populates this by running the identical plan-building
    // phase against on-disk text, so it reports exactly the fixes `--apply`
    // would refuse — one code path, parity guaranteed.
    let mut unapplied_fixes: Vec<hyalo_core::link_fix::FixPlan> = Vec::new();
    // Fixes whose file produced a valid plan but the durable write failed
    // mid-batch (L-11). Non-empty ⇒ partial failure ⇒ non-zero exit code.
    let mut failed_fixes: Vec<hyalo_core::link_fix::FailedFix> = Vec::new();
    // Fixes the H-1 round-trip guard refused: the target hyalo would have
    // written does not resolve, so nothing was written and they are reported
    // as unfixable rather than fixed (iter-200). A non-empty list here means a
    // writer/resolver asymmetry was caught before it could corrupt a link.
    let mut rejected_fixes: Vec<hyalo_core::link_fix::FixPlan> = Vec::new();

    // Merge broken-link fixes and case-mismatch fixes into a single batch so the
    // apply/dry-run planner reads and rewrites each source file once — two
    // separate passes over the same file would see the first pass's rewrites
    // and could misbehave on overlapping edits.
    let mut all_fixes = certain_fixes.clone();
    all_fixes.extend(applicable_fuzzy.iter().cloned());
    all_fixes.extend(case_mismatches.iter().cloned());

    if dry_run {
        if !all_fixes.is_empty() {
            // L-25: validate plans against on-disk text without writing, so the
            // dry-run `unapplied` set matches what `--apply` would report.
            // `modified_files` stays empty here — dry-run must NOT patch the
            // index; `_would_modify` is informational only.
            let (_would_modify, unapplied, rejected) =
                hyalo_core::link_fix::plan_fixes_dry_run(dir, &all_fixes, site_prefix)?;
            unapplied_fixes = unapplied;
            rejected_fixes = rejected;
        }
    } else if !all_fixes.is_empty() {
        let (plans, unapplied, failed, rejected) = apply_fixes(dir, &all_fixes, site_prefix)?;
        unapplied_fixes = unapplied;
        failed_fixes = failed;
        rejected_fixes = rejected;

        // Only files that actually received a durable rewrite are "modified" —
        // do not patch the index for files whose fixes were all unapplied or
        // whose write failed.
        modified_files = plans.into_iter().map(|p| p.rel_path).collect();
    }
    let unapplied_count = unapplied_fixes.len();
    let failed_count = failed_fixes.len();

    // Fixes actually written to disk this run (or, in dry-run, the full
    // plan — nothing has been attempted yet so "applied" is meaningless).
    // Reporting only the successfully-applied subset here is what makes
    // "Applied: yes" honest: a fix that never landed on disk must not appear
    // as if it did, or a fix-loop driven by this count will never converge.
    let applied_fixes: Vec<_> = if dry_run {
        Vec::new()
    } else {
        // A fix is "applied" only if it was neither unapplied (stale text) nor
        // failed (write error). Both buckets exclude it from the applied set so
        // "Applied: yes" never over-reports a fix that did not land on disk.
        let mut excluded_keys: std::collections::HashSet<(&str, usize, &str, &str)> =
            unapplied_fixes
                .iter()
                .map(|f| {
                    (
                        f.source.as_str(),
                        f.line,
                        f.old_target.as_str(),
                        f.new_target.as_str(),
                    )
                })
                .collect();
        for ff in &failed_fixes {
            excluded_keys.insert((
                ff.fix.source.as_str(),
                ff.fix.line,
                ff.fix.old_target.as_str(),
                ff.fix.new_target.as_str(),
            ));
        }
        // Guard-rejected fixes were never written either.
        for rf in &rejected_fixes {
            excluded_keys.insert((
                rf.source.as_str(),
                rf.line,
                rf.old_target.as_str(),
                rf.new_target.as_str(),
            ));
        }
        certain_fixes
            .iter()
            .chain(applicable_fuzzy.iter())
            .chain(case_mismatches.iter())
            .filter(|f| {
                !excluded_keys.contains(&(
                    f.source.as_str(),
                    f.line,
                    f.old_target.as_str(),
                    f.new_target.as_str(),
                ))
            })
            .cloned()
            .collect()
    };

    // Guard-rejected fixes join the unfixable bucket: there *is* no
    // representable target for them, which is exactly what "unfixable" means.
    let mut unfixable_links = fix_report.unfixable.clone();
    unfixable_links.extend(
        rejected_fixes
            .iter()
            .map(|f| hyalo_core::link_fix::BrokenLinkInfo {
                source: f.source.clone(),
                line: f.line,
                target: f.old_target.clone(),
            }),
    );
    unfixable_links.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));

    // iter-207 BUG-4: template-expression destinations (`{% … %}`, `{{ … }}`,
    // `${…}`) are dynamic, not broken paths. They get their own bucket and are
    // never offered as fixes — a fuzzy rewrite would silently drop the
    // conditional and the round-trip guard cannot see the damage.
    let templated_links = fix_report.templated.clone();
    let templated_count = templated_links.len();

    let output = serde_json::json!({
        "broken": broken.len(),
        // `fixable`/`fixes` cover only the non-fuzzy (certain) fixes that
        // plain `--apply` writes. Fuzzy matches are reported exclusively in
        // the `fuzzy`/`fuzzy_fixes` bucket below — counting them here too
        // would make "Fixable: N" (and the "Apply N fixes" hint) overpromise
        // what a plain `--apply` actually writes.
        "fixable": certain_fixes.len(),
        "unfixable": unfixable_links.len(),
        "ignored": ignored_count,
        "fixes": certain_fixes,
        "unfixable_links": unfixable_links,
        "applied": !dry_run,
        "applied_fixes": applied_fixes,
        "unapplied": unapplied_count,
        "unapplied_fixes": unapplied_fixes,
        // L-11: fixes whose durable write failed mid-batch. Non-empty ⇒
        // partial failure ⇒ non-zero exit code.
        "failed": failed_count,
        "failed_fixes": failed_fixes,
        "case_mismatches": case_mismatch_count,
        "case_mismatch_fixes": case_mismatches,
        "ambiguous": ambiguous_count,
        "ambiguous_links": ambiguous,
        // iter-193: targets resolving above the vault root are out of scope,
        // not broken. Reported so they stay visible, but excluded from
        // `broken`/`unfixable` — there is nothing in the vault to fix them to.
        "out_of_vault": out_of_vault_count,
        "out_of_vault_links": out_of_vault_links,
        // iter-207: templated destinations. Reported so they stay visible,
        // but never rewritten — see `link_fix::is_templated_target`.
        "templated": templated_count,
        "templated_links": templated_links,
        // Fuzzy-match fixes are reported in their own bucket. They are excluded
        // from --apply unless --apply-fuzzy / --min-confidence opts in; the
        // `fuzzy_applied` flag tells the caller whether they were written.
        "fuzzy": fuzzy_fixes.len(),
        "fuzzy_fixes": fuzzy_fixes,
        "fuzzy_applied": fuzzy.enabled(),
        "fuzzy_min_confidence": fuzzy.min_confidence,
    });

    let _ = format;
    Ok((
        CommandOutcome::success(
            serde_json::to_string_pretty(&output).context("failed to serialize")?,
        ),
        modified_files,
        failed_count > 0,
    ))
}

/// `hyalo links auto` filtering inputs from both sources: the flags typed on
/// this invocation and the persisted `[links.auto]` section of `.hyalo.toml`
/// (iter-195a).
///
/// Merge semantics:
/// - the two list keys are **unioned** — flags extend the config, never
///   replace it, so a vault-wide exclusion cannot be lost by adding one flag
/// - `first_only` is on when either source asks for it: config turns it on for
///   every run, `--first-only` turns it on for a single run — unless
///   `--no-first-only` (iter-198) forces it off for this run, which wins over
///   both
#[derive(Debug, Clone, Copy)]
pub struct AutoFilters<'a> {
    /// `--min-length`: shortest title considered a candidate. Source-agnostic —
    /// it has no `[links.auto]` key (see the iteration's non-goals).
    pub min_length: usize,
    /// `--exclude-title` values (repeatable).
    pub cli_exclude_titles: &'a [String],
    /// `--exclude-target-glob` values (repeatable).
    pub cli_exclude_target_globs: &'a [String],
    /// `--first-only` was passed.
    pub cli_first_only: bool,
    /// `--no-first-only` was passed: force first-mention-only OFF for this run
    /// whatever `[links.auto] first_only` says. Clap rejects it alongside
    /// `--first-only`; if both ever arrive anyway, off wins (see
    /// [`AutoFilters::effective_first_only`]).
    pub cli_no_first_only: bool,
    /// `[links.auto] exclude_titles`.
    pub config_exclude_titles: &'a [String],
    /// `[links.auto] exclude_target_globs`.
    pub config_exclude_target_globs: &'a [String],
    /// `[links.auto] first_only`.
    pub config_first_only: bool,
    /// `--no-warn-common-titles` was passed: silence the common-title note for
    /// this run whatever the config says.
    pub cli_no_warn_common_titles: bool,
    /// `[links.auto] warn_common_titles` (default `true`).
    pub config_warn_common_titles: bool,
}

impl Default for AutoFilters<'_> {
    /// Mirrors the CLI defaults, not `bool::default()`: the common-title note is
    /// on unless a vault opts out, so `config_warn_common_titles` starts `true`.
    fn default() -> Self {
        Self {
            min_length: 0,
            cli_exclude_titles: &[],
            cli_exclude_target_globs: &[],
            cli_first_only: false,
            cli_no_first_only: false,
            config_exclude_titles: &[],
            config_exclude_target_globs: &[],
            config_first_only: false,
            cli_no_warn_common_titles: false,
            config_warn_common_titles: true,
        }
    }
}

impl AutoFilters<'_> {
    /// Union of config and CLI titles, config first, duplicates dropped
    /// case-insensitively (matching `--exclude-title`'s own comparison).
    pub fn effective_exclude_titles(&self) -> Vec<String> {
        let mut out: Vec<String> =
            Vec::with_capacity(self.config_exclude_titles.len() + self.cli_exclude_titles.len());
        for title in self
            .config_exclude_titles
            .iter()
            .chain(self.cli_exclude_titles)
        {
            if !out.iter().any(|t: &String| t.eq_ignore_ascii_case(title)) {
                out.push(title.clone());
            }
        }
        out
    }

    /// Union of config and CLI target globs, config first, exact duplicates
    /// dropped. Globs are kept verbatim otherwise — near-identical patterns
    /// are the user's business, and a duplicated pattern in a globset is inert.
    pub fn effective_exclude_target_globs(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::with_capacity(
            self.config_exclude_target_globs.len() + self.cli_exclude_target_globs.len(),
        );
        for glob in self
            .config_exclude_target_globs
            .iter()
            .chain(self.cli_exclude_target_globs)
        {
            if !out.iter().any(|g: &String| g == glob) {
                out.push(glob.clone());
            }
        }
        out
    }

    /// `true` when first-mention-only linking applies to this run.
    ///
    /// `--no-first-only` is the one input that can force it off: it wins over
    /// both `[links.auto] first_only = true` and (defensively — clap declares
    /// the two flags as conflicting) an accompanying `--first-only`. Otherwise
    /// either source asking for it is enough.
    pub fn effective_first_only(&self) -> bool {
        if self.cli_no_first_only {
            return false;
        }
        self.cli_first_only || self.config_first_only
    }

    /// `true` when the advisory common-title note should be considered for this
    /// run: on by default, off when the vault sets
    /// `[links.auto] warn_common_titles = false` or the run passes
    /// `--no-warn-common-titles` (the flag can only turn it off — there is
    /// nothing to turn on that the default does not already do).
    pub fn effective_warn_common_titles(&self) -> bool {
        self.config_warn_common_titles && !self.cli_no_warn_common_titles
    }

    /// `true` when `[links.auto]` contributes at least one exclusion, i.e. when
    /// the `config_excluded` attribution pass is worth running.
    pub fn has_config_exclusions(&self) -> bool {
        !self.config_exclude_titles.is_empty() || !self.config_exclude_target_globs.is_empty()
    }

    /// The exclusions typed on this invocation, without the config's.
    fn cli_exclusions(&self) -> hyalo_core::auto_link::ExclusionSets<'_> {
        hyalo_core::auto_link::ExclusionSets {
            exclude_titles: self.cli_exclude_titles,
            exclude_target_globs: self.cli_exclude_target_globs,
        }
    }
}

/// How many offending titles the common-title note names in its prose list
/// before it admits it is truncating ("showing the 5 noisiest of 7"). Five
/// keeps the sentence inside a terminal line or two.
///
/// The suggested `--exclude-title` flags are deliberately **not** capped
/// (dogfood L-12): a truncated flag list needs two paste-backs to extinguish
/// the note, and flags are cheap.
const COMMON_TITLE_NOTE_MAX_LISTED: usize = 5;

/// Absolute floor for the frequency trigger: below this many proposed links a
/// title is never called out on frequency alone, however dominant its share
/// (DEC-205). It keeps small vaults — where the user simply reads every
/// proposed link — from being nagged.
const FREQUENT_TITLE_MIN_MATCHES: usize = 25;

/// Denominator of the frequency trigger's share: `1/40` = 2.5% of a run's
/// proposed links (DEC-205). Kept as an integer divisor so the threshold is
/// computed without floating point.
const FREQUENT_TITLE_SHARE_DIVISOR: usize = 40;

/// Minimum match count at which a title counts as frequency-dominant in a run
/// of `total` proposed links: `max(25, ceil(total / 40))`.
///
/// The two terms cross at exactly 1,000 proposed links: below that the
/// absolute floor rules, above it the 2.5% share does. Measured against three
/// corpora in DEC-205 — it flags `workflows` (502/1,179) through `concurrency`
/// (39/1,179) on a GitHub Docs slice while leaving that slice's 28-match
/// runner-up alone.
fn frequent_title_threshold(total: usize) -> usize {
    total
        .div_ceil(FREQUENT_TITLE_SHARE_DIVISOR)
        .max(FREQUENT_TITLE_MIN_MATCHES)
}

/// `count` as a whole-percent share of `total`, rounded to nearest.
fn percent_of(count: usize, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    (count * 200 + total) / (total * 2)
}

/// Running tally for one candidate title: how many links it produced, and how
/// each occurrence was spelled in the prose.
#[derive(Default)]
struct TitleTally {
    count: usize,
    /// Surface form → occurrences, so the note can display the spelling the
    /// vault actually uses (dogfood L-13) instead of the lowercased key.
    surfaces: std::collections::HashMap<String, usize>,
}

/// One title the note will complain about, with the reason(s) it was flagged.
struct Offender {
    /// The `to_ascii_lowercase` key — what `--exclude-title` compares against.
    key: String,
    /// The most frequent original spelling; ties broken lexicographically so
    /// the note is deterministic.
    display: String,
    count: usize,
    /// Flagged because the title is an ordinary English word (wordlist path).
    common_word: bool,
    /// Flagged because the title dominates this run (frequency path).
    frequent: bool,
}

/// Build the advisory common-title note for a `links auto` run, or `None` when
/// no proposed link came from a suspicious title (iter-197, widened in
/// iter-205).
///
/// A title is suspicious when it is **either**
///
/// - a common English word (`is_common_word`; ASCII-only by construction —
///   the list is an English word list), **or**
/// - frequency-dominant for this run (`frequent_title_threshold`), which is
///   language-independent and therefore the only trigger a non-English vault
///   ever sees.
///
/// The heuristic runs on the *emitted* matches, not on the title inventory, so
/// it is self-extinguishing and never speculative:
///
/// - a suspicious title that produced no match is not mentioned — nothing to
///   act on
/// - a title already excluded (by `--exclude-title` or `[links.auto]
///   exclude_titles`) cannot produce a match, so acting on the note makes it
///   disappear
/// - the counts quoted are exactly the links the user is being offered
///
/// The result is written to stderr as a `note:` by the caller. It deliberately
/// never reaches the stdout envelope: a vault that has not opted into anything
/// must see a byte-identical report (see the iteration's non-goals).
fn common_title_note(matches: &[hyalo_core::auto_link::AutoLinkMatch]) -> Option<String> {
    use std::collections::HashMap;

    let total = matches.len();
    if total == 0 {
        return None;
    }

    let mut tallies: HashMap<String, TitleTally> = HashMap::new();
    for m in matches {
        // Key on the ASCII-lowercased matched text: `auto_link` compares
        // `exclude_titles` with `to_ascii_lowercase` too, so "Permissions" and
        // "permissions" are one title here exactly as they are one exclusion
        // there. A Unicode-aware lowercase would merge titles the exclusion
        // would then fail to cover, and the note would suggest a flag that
        // does not work.
        let surface = m.matched_text.trim();
        if surface.is_empty() {
            continue;
        }
        let tally = tallies.entry(surface.to_ascii_lowercase()).or_default();
        tally.count += 1;
        *tally.surfaces.entry(surface.to_owned()).or_insert(0) += 1;
    }

    let frequent_at = frequent_title_threshold(total);
    let mut offenders: Vec<Offender> = tallies
        .into_iter()
        .filter_map(|(key, tally)| {
            let common_word = hyalo_core::common_words::is_common_word(&key);
            let frequent = tally.count >= frequent_at;
            if !common_word && !frequent {
                return None;
            }
            // Most frequent spelling wins; equal counts resolve alphabetically
            // so the same vault always produces the same note.
            let display = tally
                .surfaces
                .into_iter()
                .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
                .map_or_else(|| key.clone(), |(surface, _)| surface);
            Some(Offender {
                key,
                display,
                count: tally.count,
                common_word,
                frequent,
            })
        })
        .collect();
    if offenders.is_empty() {
        return None;
    }
    // Most-noisy first; ties broken alphabetically so the note is deterministic.
    offenders.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));

    let affected: usize = offenders.iter().map(|o| o.count).sum();
    let listed = offenders.len().min(COMMON_TITLE_NOTE_MAX_LISTED);

    // When every offender was flagged for the same reason the subject line can
    // state it once; a mixed set has to label each entry instead, because the
    // user's judgment differs per cause.
    let mixed = offenders
        .iter()
        .any(|o| o.common_word != offenders[0].common_word || o.frequent != offenders[0].frequent);
    let plural = offenders.len() != 1;
    let reason_clause = if mixed {
        "are common English words or unusually frequent".to_owned()
    } else {
        let first = &offenders[0];
        match (first.common_word, first.frequent, plural) {
            (true, true, false) => "is a common English word and unusually frequent".to_owned(),
            (true, true, true) => "are common English words and unusually frequent".to_owned(),
            (true, false, false) => "is a common English word".to_owned(),
            (true, false, true) => "are common English words".to_owned(),
            (false, _, false) => "is unusually frequent".to_owned(),
            (false, _, true) => "are unusually frequent".to_owned(),
        }
    };
    let subject = format!(
        "{count} auto-link candidate title{s} {reason_clause} and account{verb} for",
        count = offenders.len(),
        s = if plural { "s" } else { "" },
        verb = if plural { "" } else { "s" },
    );

    // L-12: the prose list is capped, and says so; the flag list below is not.
    let truncation = if offenders.len() > listed {
        format!(" (showing the {listed} noisiest of {})", offenders.len())
    } else {
        String::new()
    };

    let names: Vec<String> = offenders[..listed]
        .iter()
        .map(|o| {
            let mut detail = format!("{}×", o.count);
            if o.frequent {
                use std::fmt::Write as _;
                let _ = write!(detail, ", {}%", percent_of(o.count, total));
            }
            if mixed {
                detail.push_str(match (o.common_word, o.frequent) {
                    (true, true) => ", common English word + unusually frequent",
                    (true, false) => ", common English word",
                    (false, _) => ", unusually frequent",
                });
            }
            format!("\"{}\" ({detail})", o.display)
        })
        .collect();

    let mut flags = String::new();
    for o in &offenders {
        use std::fmt::Write as _;
        // Writing into a String is infallible; the Result only exists to satisfy
        // the `fmt::Write` signature. Reuse the same shell-quoting the other
        // suggested-flag hints use (`hints::shell_quote`) rather than a
        // bespoke whitespace-only check — titles can contain apostrophes,
        // `$`, backticks, or double quotes, none of which a naive
        // whitespace check would escape. The displayed spelling is used
        // verbatim: `--exclude-title` matches case-insensitively, so it works
        // either way and stays greppable in the user's own files.
        let _ = write!(
            flags,
            " --exclude-title {}",
            crate::hints::shell_quote(&o.display)
        );
    }

    Some(format!(
        "{subject} {affected} of {total} proposed links{truncation}: {names}. \
         If those are prose mentions rather than deliberate references, skip them with{flags} \
         — or persist them once under [links.auto] exclude_titles in .hyalo.toml. \
         Silence this note with --no-warn-common-titles.",
        names = names.join(", "),
    ))
}

/// Run `hyalo links auto` using a pre-built index.
///
/// `apply = false` → preview only (default)
/// `apply = true`  → write `[[wikilinks]]` to disk
///
/// Returns `(CommandOutcome, modified_files)` where `modified_files` contains
/// vault-relative paths of files that were rewritten on disk.  The caller is
/// responsible for patching the snapshot index with these paths.
pub fn links_auto(
    index: &dyn VaultIndex,
    dir: &Path,
    apply: bool,
    filters: &AutoFilters<'_>,
    file_filter: Option<&str>,
    glob_filter: &[String],
    format: Format,
) -> Result<(CommandOutcome, Vec<String>, bool)> {
    let exclude_titles = filters.effective_exclude_titles();
    let exclude_target_globs = filters.effective_exclude_target_globs();
    let opts = hyalo_core::auto_link::AutoLinkOptions {
        apply,
        min_length: filters.min_length,
        exclude_titles: &exclude_titles,
        first_only: filters.effective_first_only(),
        exclude_target_globs: &exclude_target_globs,
        file_filter,
        glob_filter,
    };

    // How many candidate titles the persisted config took away — reported as
    // `config_excluded` so a bare `links auto` run stays explainable.
    let config_excluded = if filters.has_config_exclusions() {
        hyalo_core::auto_link::count_config_excluded_titles(
            index,
            filters.min_length,
            filters.cli_exclusions(),
            hyalo_core::auto_link::ExclusionSets {
                exclude_titles: &exclude_titles,
                exclude_target_globs: &exclude_target_globs,
            },
        )?
    } else {
        0
    };

    let report = hyalo_core::auto_link::auto_link(index, dir, &opts)?;

    // iter-197: advisory note when common English words drive the candidates.
    // stderr only (deduped and suppressed by `-q` like every other note), so the
    // stdout envelope stays byte-identical for vaults that never opt into
    // anything.
    if filters.effective_warn_common_titles()
        && let Some(note) = common_title_note(&report.matches)
    {
        crate::warn::note(note);
    }

    // Collect unique modified files for the caller to patch the index. Only
    // files that were actually applied (not skipped/failed) count as modified,
    // so the index is never patched for a file whose write was skipped.
    let applied_files: std::collections::HashSet<&str> = report
        .apply_outcomes
        .iter()
        .filter(|o| o.status == hyalo_core::auto_link::AutoApplyStatus::Applied)
        .map(|o| o.file.as_str())
        .collect();
    let modified_files: Vec<String> = if report.applied {
        applied_files.iter().map(|s| (*s).to_owned()).collect()
    } else {
        Vec::new()
    };

    // Per-file apply outcome counts for the envelope.
    let (applied_count, skipped_count, failed_count) =
        report
            .apply_outcomes
            .iter()
            .fold((0usize, 0usize, 0usize), |(a, s, f), o| match o.status {
                hyalo_core::auto_link::AutoApplyStatus::Applied => (a + 1, s, f),
                hyalo_core::auto_link::AutoApplyStatus::Skipped => (a, s + 1, f),
                hyalo_core::auto_link::AutoApplyStatus::Failed => (a, s, f + 1),
            });

    let mut output = serde_json::json!({
        "scanned": report.scanned,
        "total": report.total,
        "matches": report.matches,
        "ambiguous_titles": report.ambiguous_titles,
        "applied": report.applied,
        // L-11: per-file apply outcomes (applied/skipped/failed with reason).
        // Empty in preview mode. `files_applied`/`files_skipped`/`files_failed`
        // are the counts; `apply_outcomes` carries the per-file detail.
        "files_applied": applied_count,
        "files_skipped": skipped_count,
        "files_failed": failed_count,
        "apply_outcomes": report.apply_outcomes,
    });
    // Omitted when zero, matching the `links.out_of_vault` precedent: the key
    // only appears when `[links.auto]` config actually removed candidates.
    if config_excluded > 0
        && let Some(obj) = output.as_object_mut()
    {
        obj.insert(
            "config_excluded".to_owned(),
            serde_json::json!(config_excluded),
        );
    }

    let _ = format;
    Ok((
        CommandOutcome::success(
            serde_json::to_string_pretty(&output).context("failed to serialize")?,
        ),
        modified_files,
        failed_count > 0,
    ))
}

// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        AutoFilters, COMMON_TITLE_NOTE_MAX_LISTED, common_title_note, frequent_title_threshold,
        percent_of,
    };
    use hyalo_core::auto_link::AutoLinkMatch;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }

    /// One proposed match with `matched_text` as the surface form; the other
    /// fields are irrelevant to the common-title heuristic.
    fn match_for(matched_text: &str) -> AutoLinkMatch {
        AutoLinkMatch {
            file: "guide.md".to_owned(),
            line: 1,
            byte_col: 0,
            col: 0,
            matched_text: matched_text.to_owned(),
            link_target: matched_text.to_ascii_lowercase(),
        }
    }

    fn matches_for(surface_forms: &[&str]) -> Vec<AutoLinkMatch> {
        surface_forms.iter().map(|t| match_for(t)).collect()
    }

    /// `n` proposed links that all share the same surface form.
    fn repeated(surface: &str, n: usize) -> Vec<AutoLinkMatch> {
        (0..n).map(|_| match_for(surface)).collect()
    }

    // -----------------------------------------------------------------------
    // first_only: all four (config, flag) combinations
    // -----------------------------------------------------------------------

    #[test]
    fn first_only_off_in_both_sources_stays_off() {
        let filters = AutoFilters::default();
        assert!(!filters.effective_first_only());
    }

    #[test]
    fn first_only_from_config_alone_applies() {
        let filters = AutoFilters {
            config_first_only: true,
            ..AutoFilters::default()
        };
        assert!(filters.effective_first_only());
    }

    #[test]
    fn first_only_from_flag_alone_applies() {
        let filters = AutoFilters {
            cli_first_only: true,
            ..AutoFilters::default()
        };
        assert!(filters.effective_first_only());
    }

    #[test]
    fn first_only_from_both_sources_applies_once() {
        let filters = AutoFilters {
            cli_first_only: true,
            config_first_only: true,
            ..AutoFilters::default()
        };
        assert!(filters.effective_first_only());
    }

    // -----------------------------------------------------------------------
    // --no-first-only (iter-198): the one input that forces first_only OFF
    // -----------------------------------------------------------------------

    #[test]
    fn no_first_only_flag_overrides_config_first_only() {
        let filters = AutoFilters {
            cli_no_first_only: true,
            config_first_only: true,
            ..AutoFilters::default()
        };
        assert!(
            !filters.effective_first_only(),
            "--no-first-only must win over [links.auto] first_only = true"
        );
    }

    #[test]
    fn no_first_only_flag_alone_is_a_no_op() {
        let filters = AutoFilters {
            cli_no_first_only: true,
            ..AutoFilters::default()
        };
        assert!(
            !filters.effective_first_only(),
            "first-only is already off without the config key"
        );
    }

    #[test]
    fn no_first_only_wins_over_first_only_if_both_arrive() {
        // Clap declares the two flags as conflicting, so this pair cannot come
        // from the CLI. The tie-break is asserted anyway: any other caller of
        // AutoFilters gets "off wins", never an accidental first-only run.
        let filters = AutoFilters {
            cli_first_only: true,
            cli_no_first_only: true,
            config_first_only: true,
            ..AutoFilters::default()
        };
        assert!(!filters.effective_first_only());
    }

    // -----------------------------------------------------------------------
    // list unions: flags extend the config, never replace it
    // -----------------------------------------------------------------------

    #[test]
    fn exclude_titles_union_keeps_config_entries_and_appends_flags() {
        let config = strings(&["permissions", "README"]);
        let cli = strings(&["index"]);
        let filters = AutoFilters {
            cli_exclude_titles: &cli,
            config_exclude_titles: &config,
            ..AutoFilters::default()
        };
        assert_eq!(
            filters.effective_exclude_titles(),
            strings(&["permissions", "README", "index"]),
            "config entries come first, flags extend them"
        );
    }

    #[test]
    fn exclude_titles_union_dedups_case_insensitively() {
        // `--exclude-title` compares case-insensitively, so "readme" and
        // "README" are the same exclusion — keep the config spelling.
        let config = strings(&["README"]);
        let cli = strings(&["readme", "index"]);
        let filters = AutoFilters {
            cli_exclude_titles: &cli,
            config_exclude_titles: &config,
            ..AutoFilters::default()
        };
        assert_eq!(
            filters.effective_exclude_titles(),
            strings(&["README", "index"])
        );
    }

    #[test]
    fn exclude_titles_config_only_is_used_verbatim() {
        let config = strings(&["permissions"]);
        let filters = AutoFilters {
            config_exclude_titles: &config,
            ..AutoFilters::default()
        };
        assert_eq!(
            filters.effective_exclude_titles(),
            strings(&["permissions"])
        );
    }

    #[test]
    fn exclude_target_globs_union_keeps_both_sources() {
        let config = strings(&["templates/*"]);
        let cli = strings(&["archive/**"]);
        let filters = AutoFilters {
            cli_exclude_target_globs: &cli,
            config_exclude_target_globs: &config,
            ..AutoFilters::default()
        };
        assert_eq!(
            filters.effective_exclude_target_globs(),
            strings(&["templates/*", "archive/**"])
        );
    }

    #[test]
    fn exclude_target_globs_union_dedups_exact_repeats() {
        let config = strings(&["templates/*"]);
        let cli = strings(&["templates/*"]);
        let filters = AutoFilters {
            cli_exclude_target_globs: &cli,
            config_exclude_target_globs: &config,
            ..AutoFilters::default()
        };
        assert_eq!(
            filters.effective_exclude_target_globs(),
            strings(&["templates/*"])
        );
    }

    // -----------------------------------------------------------------------
    // config-exclusion attribution gate
    // -----------------------------------------------------------------------

    #[test]
    fn has_config_exclusions_is_false_for_cli_only_flags() {
        let cli = strings(&["permissions"]);
        let filters = AutoFilters {
            cli_exclude_titles: &cli,
            ..AutoFilters::default()
        };
        assert!(!filters.has_config_exclusions());
    }

    #[test]
    fn has_config_exclusions_is_true_for_either_config_list() {
        let titles = strings(&["permissions"]);
        assert!(
            AutoFilters {
                config_exclude_titles: &titles,
                ..AutoFilters::default()
            }
            .has_config_exclusions()
        );
        let globs = strings(&["templates/*"]);
        assert!(
            AutoFilters {
                config_exclude_target_globs: &globs,
                ..AutoFilters::default()
            }
            .has_config_exclusions()
        );
    }

    // -----------------------------------------------------------------------
    // iter-197: warn_common_titles resolution
    // -----------------------------------------------------------------------

    #[test]
    fn common_title_note_is_on_by_default() {
        assert!(AutoFilters::default().effective_warn_common_titles());
    }

    #[test]
    fn config_false_turns_the_common_title_note_off() {
        let filters = AutoFilters {
            config_warn_common_titles: false,
            ..AutoFilters::default()
        };
        assert!(!filters.effective_warn_common_titles());
    }

    #[test]
    fn flag_turns_the_common_title_note_off_for_one_run() {
        let filters = AutoFilters {
            cli_no_warn_common_titles: true,
            ..AutoFilters::default()
        };
        assert!(!filters.effective_warn_common_titles());
    }

    #[test]
    fn flag_and_config_both_off_stays_off() {
        let filters = AutoFilters {
            cli_no_warn_common_titles: true,
            config_warn_common_titles: false,
            ..AutoFilters::default()
        };
        assert!(!filters.effective_warn_common_titles());
    }

    // -----------------------------------------------------------------------
    // iter-205: the frequency threshold itself (DEC-205)
    // -----------------------------------------------------------------------

    #[test]
    fn frequency_threshold_is_the_absolute_floor_for_small_runs() {
        assert_eq!(frequent_title_threshold(1), 25);
        // The own knowledgebase, measured in DEC-205.
        assert_eq!(frequent_title_threshold(195), 25);
        assert_eq!(frequent_title_threshold(999), 25);
    }

    #[test]
    fn frequency_threshold_switches_to_the_share_at_a_thousand_links() {
        // 2.5% overtakes the 25-match floor exactly at 1,000 proposed links.
        assert_eq!(frequent_title_threshold(1_000), 25);
        assert_eq!(frequent_title_threshold(1_040), 26);
        // The two corpora DEC-205 was tuned against.
        assert_eq!(frequent_title_threshold(1_179), 30);
        assert_eq!(frequent_title_threshold(33_859), 847);
    }

    #[test]
    fn frequency_threshold_rounds_the_share_up() {
        // ceil, not floor: a title has to clear the share outright.
        assert_eq!(frequent_title_threshold(1_001), 26);
    }

    #[test]
    fn percent_rounds_to_nearest_and_survives_an_empty_run() {
        assert_eq!(percent_of(502, 1_179), 43);
        assert_eq!(percent_of(1, 3), 33);
        assert_eq!(percent_of(2, 3), 67);
        assert_eq!(percent_of(1, 1), 100);
        assert_eq!(percent_of(0, 0), 0);
    }

    // -----------------------------------------------------------------------
    // iter-197: note text (wordlist trigger)
    // -----------------------------------------------------------------------

    #[test]
    fn no_matches_produces_no_note() {
        assert!(common_title_note(&[]).is_none());
    }

    #[test]
    fn domain_specific_titles_produce_no_note() {
        // Neither an English word nor anywhere near the frequency floor.
        let matches = matches_for(&["Kubernetes", "hyalo", "frontmatter"]);
        assert!(common_title_note(&matches).is_none());
    }

    #[test]
    fn single_offender_uses_singular_phrasing_and_exact_counts() {
        let matches = matches_for(&["permissions", "permissions", "Kubernetes"]);
        let note = common_title_note(&matches).expect("common word should be flagged");
        assert!(
            note.starts_with("1 auto-link candidate title is a common English word"),
            "singular phrasing expected: {note}"
        );
        assert!(
            note.contains("accounts for 2 of 3 proposed links"),
            "counts should be offender-vs-total: {note}"
        );
        assert!(
            note.contains("\"permissions\" (2×)"),
            "the offender should be named with its count: {note}"
        );
        assert!(
            !note.contains('%'),
            "a wordlist-only offender has no share to report: {note}"
        );
        assert!(
            note.contains("--exclude-title permissions"),
            "the note should suggest the flag: {note}"
        );
    }

    #[test]
    fn offenders_are_ordered_by_count_then_alphabetically() {
        // "index" 1×, "note" 3×, "report" 1×  →  note, index, report
        let matches = matches_for(&["note", "Note", "NOTE", "report", "index"]);
        let note = common_title_note(&matches).expect("common words should be flagged");
        let pos = |needle: &str| note.find(needle).expect("offender should be listed");
        assert!(
            pos("(3×)") < pos("\"index\" (1×)"),
            "highest count first: {note}"
        );
        assert!(
            pos("\"index\" (1×)") < pos("\"report\" (1×)"),
            "ties broken alphabetically: {note}"
        );
        assert!(
            note.starts_with("3 auto-link candidate titles are common English words"),
            "plural phrasing expected: {note}"
        );
    }

    #[test]
    fn case_variants_of_one_title_are_counted_once() {
        // `--exclude-title` is case-insensitive, so the note must not split
        // "Note" and "note" into two separate offenders. With the two
        // spellings equally frequent the display form is the alphabetically
        // first one, so the note is deterministic (L-13).
        let matches = matches_for(&["Note", "note"]);
        let note = common_title_note(&matches).expect("common word should be flagged");
        assert!(
            note.contains("1 auto-link candidate title is"),
            "case variants are one title: {note}"
        );
        assert!(
            note.contains("\"Note\" (2×)"),
            "counts should be merged under one spelling: {note}"
        );
    }

    // -----------------------------------------------------------------------
    // iter-205 / L-13: the displayed spelling is the vault's, not the key's
    // -----------------------------------------------------------------------

    #[test]
    fn the_note_displays_the_most_frequent_original_spelling() {
        // A page titled README, mentioned mostly as "README": the note has to
        // say README, not the lowercased lookup key (dogfood L-13).
        let mut matches = repeated("README", 3);
        matches.extend(repeated("readme", 1));
        let note = common_title_note(&matches).expect("common word should be flagged");
        assert!(
            note.contains("\"README\" (4×)"),
            "the dominant spelling should be displayed: {note}"
        );
        assert!(
            note.contains("--exclude-title README"),
            "the suggested flag uses the same spelling: {note}"
        );
    }

    // -----------------------------------------------------------------------
    // iter-205: the frequency trigger (dogfood UX-1)
    // -----------------------------------------------------------------------

    #[test]
    fn a_dominant_title_is_flagged_even_when_it_is_not_an_english_word() {
        // The UX-1 repro in miniature: "Workflows" drove 43% of a GitHub Docs
        // run and the wordlist trigger never mentioned it.
        let mut matches = repeated("Workflows", 30);
        matches.extend(repeated("Kubernetes", 10));
        let note = common_title_note(&matches).expect("a dominant title should be flagged");
        assert!(
            note.starts_with("1 auto-link candidate title is unusually frequent"),
            "the reason should be frequency, not the wordlist: {note}"
        );
        assert!(
            note.contains("\"Workflows\" (30×, 75%)"),
            "a frequency offender reports its share of the run: {note}"
        );
        assert!(
            note.contains("--exclude-title Workflows"),
            "the note should suggest the flag: {note}"
        );
        assert!(
            !note.contains("Kubernetes"),
            "the quiet title is nobody's business: {note}"
        );
    }

    #[test]
    fn a_title_under_the_absolute_floor_is_never_flagged_on_frequency() {
        // 24 of 25 links is a 96% share, but on a run this small the user
        // simply reads them — the floor keeps the note quiet.
        let mut matches = repeated("Workflows", 24);
        matches.extend(repeated("Kubernetes", 1));
        assert!(
            common_title_note(&matches).is_none(),
            "the 25-match floor should hold on tiny runs"
        );
    }

    #[test]
    fn a_title_under_the_share_is_not_flagged_in_a_large_run() {
        // 2,000 proposed links → the threshold is 50, not the floor's 25.
        let filler: Vec<AutoLinkMatch> = (0..49)
            .flat_map(|i| repeated(&format!("filler-{i}"), 40))
            .collect();

        let mut quiet = repeated("Workflows", 40);
        quiet.extend(filler.iter().cloned());
        assert_eq!(quiet.len(), 2_000);
        assert!(
            common_title_note(&quiet).is_none(),
            "40 of 2,000 links is under 2.5% — the floor must not carry it"
        );

        let mut loud = repeated("Workflows", 60);
        loud.extend(filler);
        let note = common_title_note(&loud).expect("60 of 2,020 links clears the share");
        assert!(
            note.contains("\"Workflows\" (60×"),
            "the dominant title should be named: {note}"
        );
    }

    #[test]
    fn non_ascii_titles_reach_the_frequency_trigger() {
        // The wordlist is English and ASCII-gated, which left non-English
        // vaults with no note at all; the frequency path has no such gate.
        let mut matches = repeated("Übersicht", 30);
        matches.extend(repeated("Kubernetes", 10));
        let note = common_title_note(&matches).expect("a German title should still be flagged");
        assert!(
            note.contains("\"Übersicht\" (30×, 75%)"),
            "the non-ASCII title should be named with its share: {note}"
        );
        assert!(
            note.contains("--exclude-title 'Übersicht'"),
            "the suggested flag should be shell-safe: {note}"
        );
    }

    #[test]
    fn a_mixed_offender_set_labels_every_entry_with_its_reason() {
        let mut matches = repeated("Workflows", 30); // frequency only
        matches.extend(repeated("README", 26)); // both triggers
        matches.extend(repeated("permissions", 2)); // wordlist only
        matches.extend(repeated("Kubernetes", 10)); // neither
        let note = common_title_note(&matches).expect("three offenders should be flagged");
        assert!(
            note.starts_with(
                "3 auto-link candidate titles are common English words or unusually frequent \
                 and account for 58 of 68 proposed links"
            ),
            "a mixed set names both reasons in the subject: {note}"
        );
        assert!(
            note.contains("\"Workflows\" (30×, 44%, unusually frequent)"),
            "frequency-only offender should be labelled: {note}"
        );
        assert!(
            note.contains("\"README\" (26×, 38%, common English word + unusually frequent)"),
            "a doubly-flagged offender should say so: {note}"
        );
        assert!(
            note.contains("\"permissions\" (2×, common English word)"),
            "wordlist-only offender should be labelled and carry no share: {note}"
        );
    }

    #[test]
    fn a_homogeneous_offender_set_states_the_reason_once() {
        let mut matches = repeated("Workflows", 30);
        matches.extend(repeated("Übersicht", 30));
        let note = common_title_note(&matches).expect("two dominant titles should be flagged");
        assert!(
            note.starts_with("2 auto-link candidate titles are unusually frequent"),
            "plural frequency phrasing expected: {note}"
        );
        assert!(
            !note.contains(", unusually frequent)"),
            "no per-entry label is needed when every offender shares a reason: {note}"
        );
    }

    #[test]
    fn excluding_the_dominant_title_can_reveal_the_next_tier() {
        // A share-relative trigger re-scales when the run shrinks: excluding a
        // 74% title leaves a much smaller run in which the runner-up clears
        // the threshold it previously missed. That is the trigger working as
        // designed, not a nag loop — the 25-match floor bounds it, because
        // every round removes at least 25 links from the run.
        let filler: Vec<AutoLinkMatch> = (0..19)
            .flat_map(|i| repeated(&format!("filler-{i}"), 20))
            .collect();

        // Round 1: 1,620 links, threshold 41. Only the dominant title clears it.
        let mut round1 = repeated("Workflows", 1_200);
        round1.extend(repeated("runner groups", 40));
        round1.extend(filler.iter().cloned());
        assert_eq!(round1.len(), 1_620);
        let note = common_title_note(&round1).expect("the dominant title should be flagged");
        assert!(
            note.starts_with("1 auto-link candidate title is unusually frequent"),
            "the 40-match runner-up is under 2.5% of 1,620: {note}"
        );

        // Round 2: the same vault with "Workflows" excluded — 420 links, so
        // the threshold falls back to the floor and the runner-up surfaces.
        let mut round2 = repeated("runner groups", 40);
        round2.extend(filler.iter().cloned());
        let note = common_title_note(&round2).expect("the next tier should surface");
        assert!(
            note.contains("\"runner groups\" (40×, 10%)"),
            "the runner-up is now the dominant title: {note}"
        );

        // Round 3: nothing left clears the 25-match floor. The process ends.
        assert!(
            common_title_note(&filler).is_none(),
            "the floor terminates the cascade"
        );
    }

    // -----------------------------------------------------------------------
    // iter-205 / L-12: honest truncation, complete flag list
    // -----------------------------------------------------------------------

    #[test]
    fn long_offender_lists_are_truncated_in_prose_but_not_in_flags() {
        let matches = matches_for(&[
            "access", "account", "action", "active", "address", "agree", "answer",
        ]);
        let note = common_title_note(&matches).expect("common words should be flagged");
        assert!(
            note.contains(&format!(
                "(showing the {COMMON_TITLE_NOTE_MAX_LISTED} noisiest of 7)"
            )),
            "the note should admit that it is truncating: {note}"
        );
        assert!(
            !note.contains("more"),
            "the old '+N more' phrasing is gone: {note}"
        );
        assert_eq!(
            note.matches("--exclude-title ").count(),
            7,
            "every offender gets a flag so one paste-back extinguishes the note: {note}"
        );
        assert_eq!(
            note.matches('"').count(),
            COMMON_TITLE_NOTE_MAX_LISTED * 2,
            "only the named offenders appear in the prose list: {note}"
        );
    }

    #[test]
    fn multiword_frequent_titles_are_shell_quoted_in_the_suggestion() {
        // "runner groups" (45 links on the GitHub Docs slice) is the real-data
        // case: a frequency offender whose title contains a space, so the
        // suggested flag has to survive a copy-paste into a shell.
        let mut matches = repeated("runner groups", 30);
        matches.extend(repeated("Kubernetes", 10));
        let note = common_title_note(&matches).expect("a dominant title should be flagged");
        assert!(
            note.contains("\"runner groups\" (30×, 75%)"),
            "the multi-word title should be named: {note}"
        );
        assert!(
            note.contains("--exclude-title 'runner groups'"),
            "the suggested flag should be quoted: {note}"
        );
    }

    #[test]
    fn suggested_flags_use_the_shared_shell_quoting_helper() {
        // Uses the same `hints::shell_quote` every other suggested-flag hint
        // uses, so it also escapes apostrophes/`$`/backticks/double quotes,
        // not just whitespace.
        assert_eq!(crate::hints::shell_quote("data model"), "'data model'");
        assert_eq!(crate::hints::shell_quote("permissions"), "permissions");
        assert_eq!(
            crate::hints::shell_quote("it's a trap"),
            "'it'\\''s a trap'"
        );
    }

    #[test]
    fn whitespace_only_matched_text_is_ignored() {
        // Defensive: an empty surface form must not become an offender key.
        let matches = matches_for(&["   ", "permissions"]);
        let note = common_title_note(&matches).expect("common word should be flagged");
        assert!(
            note.contains("\"permissions\" (1×)"),
            "blank surface forms should be skipped, not counted: {note}"
        );
    }
}
