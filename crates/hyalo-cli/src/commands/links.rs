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

    let matcher = LinkMatcher::from_index(index, threshold);
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

/// How many offending titles the common-title note names before it summarises
/// the rest as `+N more`. Five keeps the note (and the ready-to-paste flag list
/// it suggests) inside a terminal line or two.
const COMMON_TITLE_NOTE_MAX_LISTED: usize = 5;

/// Build the advisory common-title note for a `links auto` run, or `None` when
/// no proposed link came from a common English word (iter-197).
///
/// The heuristic runs on the *emitted* matches, not on the title inventory, so
/// it is self-extinguishing and never speculative:
///
/// - a common-word title that produced no match is not mentioned — nothing to
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

    let mut counts: HashMap<String, usize> = HashMap::new();
    for m in matches {
        // Key on the lowercased matched text: `--exclude-title` compares
        // case-insensitively, so "Permissions" and "permissions" are one title
        // and the note must not split them into two entries.
        let key = m.matched_text.trim().to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        *counts.entry(key).or_insert(0) += 1;
    }

    let mut offenders: Vec<(String, usize)> = counts
        .into_iter()
        .filter(|(title, _)| hyalo_core::common_words::is_common_word(title))
        .collect();
    if offenders.is_empty() {
        return None;
    }
    // Most-noisy first; ties broken alphabetically so the note is deterministic.
    offenders.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let affected: usize = offenders.iter().map(|(_, count)| *count).sum();
    let listed = offenders.len().min(COMMON_TITLE_NOTE_MAX_LISTED);
    let mut names: Vec<String> = offenders[..listed]
        .iter()
        .map(|(title, count)| format!("\"{title}\" ({count}×)"))
        .collect();
    if offenders.len() > listed {
        names.push(format!("+{} more", offenders.len() - listed));
    }

    let subject = if offenders.len() == 1 {
        "1 auto-link candidate title is a common English word and accounts for".to_owned()
    } else {
        format!(
            "{} auto-link candidate titles are common English words and account for",
            offenders.len()
        )
    };
    let mut flags = String::new();
    for (title, _) in &offenders[..listed] {
        use std::fmt::Write as _;
        // Writing into a String is infallible; the Result only exists to satisfy
        // the `fmt::Write` signature. Reuse the same shell-quoting the other
        // suggested-flag hints use (`hints::shell_quote`) rather than a
        // bespoke whitespace-only check — titles can contain apostrophes,
        // `$`, backticks, or double quotes, none of which a naive
        // whitespace check would escape.
        let _ = write!(
            flags,
            " --exclude-title {}",
            crate::hints::shell_quote(title)
        );
    }

    Some(format!(
        "{subject} {affected} of {total} proposed links: {names}. \
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
    use super::{AutoFilters, COMMON_TITLE_NOTE_MAX_LISTED, common_title_note};
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
            col: 0,
            matched_text: matched_text.to_owned(),
            link_target: matched_text.to_ascii_lowercase(),
        }
    }

    fn matches_for(surface_forms: &[&str]) -> Vec<AutoLinkMatch> {
        surface_forms.iter().map(|t| match_for(t)).collect()
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
    // iter-197: note text
    // -----------------------------------------------------------------------

    #[test]
    fn no_matches_produces_no_note() {
        assert!(common_title_note(&[]).is_none());
    }

    #[test]
    fn domain_specific_titles_produce_no_note() {
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
            pos("\"note\" (3×)") < pos("\"index\" (1×)"),
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
        // "Note" and "note" into two separate offenders.
        let matches = matches_for(&["Note", "note"]);
        let note = common_title_note(&matches).expect("common word should be flagged");
        assert!(
            note.contains("1 auto-link candidate title is"),
            "case variants are one title: {note}"
        );
        assert!(
            note.contains("\"note\" (2×)"),
            "counts should be merged under the lowercased title: {note}"
        );
    }

    #[test]
    fn long_offender_lists_are_capped_with_a_remainder_count() {
        let matches = matches_for(&[
            "access", "account", "action", "active", "address", "agree", "answer",
        ]);
        let note = common_title_note(&matches).expect("common words should be flagged");
        assert!(
            note.contains(&format!("+{} more", 7 - COMMON_TITLE_NOTE_MAX_LISTED)),
            "the tail should be summarised, not listed: {note}"
        );
        assert_eq!(
            note.matches("--exclude-title ").count(),
            COMMON_TITLE_NOTE_MAX_LISTED,
            "only the named offenders get a suggested flag: {note}"
        );
    }

    #[test]
    fn multiword_titles_are_shell_quoted_in_the_suggestion() {
        // "state of the art" is not a title we would flag, but a common word
        // *is* reachable as a multi-word title via frontmatter aliases, and the
        // suggested flag has to survive a copy-paste into a shell. Uses the
        // same `hints::shell_quote` every other suggested-flag hint uses, so
        // it also escapes apostrophes/`$`/backticks/double quotes, not just
        // whitespace.
        let quoted = crate::hints::shell_quote("data model");
        assert_eq!(quoted, "'data model'");
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
