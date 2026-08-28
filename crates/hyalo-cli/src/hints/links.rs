//! Hints for `links fix` and `links auto`.
//!
//! Split out of the single 5,059-line `hints.rs` in iteration 247 (deep-review
//! hotspot). This is a file split only: the items keep the visibility they had
//! inside the one module, so `hints::...` paths and behaviour are unchanged.

use super::{
    Hint, HintContext, MAX_HINTS, build_command_no_glob, build_command_with_glob,
    format_confidence, shell_quote,
};

pub(super) fn hints_for_links_fix(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let is_dry_run = !data
        .get("applied")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let fixable = data
        .get("fixable")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let unfixable = data
        .get("unfixable")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    // L-25: dry-run validates plans against on-disk text, so some `fixable`
    // fixes may be stale (their text no longer matches). Discount them so the
    // "Apply N fixes" hint count matches what `--apply` actually writes.
    let unapplied = data
        .get("unapplied")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let applicable = fixable.saturating_sub(unapplied);

    if is_dry_run && applicable > 0 {
        hints.push(Hint::new(
            format!("Apply {applicable} fixes"),
            build_command_with_glob(ctx, &["links", "fix", "--apply"]),
        ));
    }

    // Fuzzy-match fixes are excluded from --apply by default. Surface the
    // opt-in when a dry-run turned up fuzzy candidates that were not applied
    // — but only count candidates that actually clear the confidence floor
    // (NEW-14, iter-218). `fuzzy` is every candidate found, including ones
    // below the floor that `--apply-fuzzy` would never write; promising to
    // apply all of `fuzzy` produced a hint like "apply 3253 fixes" that
    // applied 0 files when every one of them was below-floor.
    let fuzzy = data
        .get("fuzzy")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let fuzzy_below_floor = data
        .get("fuzzy_below_floor")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let applicable_fuzzy = fuzzy.saturating_sub(fuzzy_below_floor);
    let fuzzy_applied = data
        .get("fuzzy_applied")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if applicable_fuzzy > 0 && !fuzzy_applied {
        // `applicable_fuzzy` is counted against THIS run's effective floor
        // (`fuzzy_min_confidence`), which may differ from
        // `DEFAULT_FUZZY_MIN_CONFIDENCE` — either because this run passed
        // `--min-confidence` or because `.hyalo.toml` sets
        // `[links] fuzzy_min_confidence`. The hinted command must carry the
        // same floor, or a dry run at a lower floor promises a count the
        // hinted apply (which would silently fall back to the default 0.8)
        // does not deliver (review finding #1). Appending it whenever the
        // floor is non-default is simpler than tracking whether it came
        // from the flag or the config file, and is harmless when it came
        // from the config (the flag just repeats what the config would
        // have applied anyway).
        let floor = data
            .get("fuzzy_min_confidence")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(hyalo_core::link_score::DEFAULT_FUZZY_MIN_CONFIDENCE);
        let mut cmd_parts = vec!["links", "fix", "--apply", "--apply-fuzzy"];
        let floor_arg;
        if (floor - hyalo_core::link_score::DEFAULT_FUZZY_MIN_CONFIDENCE).abs() > f64::EPSILON {
            floor_arg = format_confidence(floor);
            cmd_parts.push("--min-confidence");
            cmd_parts.push(&floor_arg);
        }
        hints.push(Hint::new(
            format!("Review then apply {applicable_fuzzy} lower-confidence fuzzy fixes"),
            build_command_with_glob(ctx, &cmd_parts),
        ));
    } else if fuzzy > 0 && !fuzzy_applied {
        // Every candidate is below the floor — `--apply-fuzzy` would apply 0
        // of them. Point at reviewing with a lower floor instead of a
        // command that reads as an apply but writes nothing.
        let floor = data
            .get("fuzzy_min_confidence")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        hints.push(Hint::new(
            format!(
                "{fuzzy} fuzzy candidates found, all below the confidence floor \
                 {floor} — review with a lower --min-confidence before applying"
            ),
            build_command_with_glob(ctx, &["links", "fix", "--min-confidence", "0"]),
        ));
    }

    if unfixable > 0 {
        hints.push(Hint::new(
            "List files with remaining broken links",
            build_command_with_glob(ctx, &["find", "--broken-links"]),
        ));
    }

    // L-11: a non-zero `failed` count means some fixes produced a valid plan
    // but the durable write itself failed mid-batch (e.g. read-only file).
    // Point at the per-fix detail and suggest the most common cause.
    let failed = data
        .get("failed")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if failed > 0 {
        hints.push(Hint::without_cmd(format!(
            "{failed} fix(es) failed to write — see `failed_fixes` for the per-file \
             error, check file permissions"
        )));
    }

    // Case-mismatch and relocation repairs are written by plain `--apply` but
    // are *not* part of `fixable`, so a vault whose only problem was one of
    // these produced no "Apply" hint at all — the fix was available and
    // unadvertised (iter-210). NEW-13 (dogfood pre3) split relocations out of
    // `case_mismatches` into their own bucket; both still land here since
    // both are written by plain `--apply`.
    let case_mismatches = data
        .get("case_mismatches")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let relocations = data
        .get("relocations")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let case_and_relocation_fixes = case_mismatches + relocations;
    if is_dry_run && case_and_relocation_fixes > 0 && applicable == 0 && hints.len() < MAX_HINTS {
        let label = if case_mismatches > 0 && relocations > 0 {
            format!("Apply {case_mismatches} case-mismatch and {relocations} relocation fixes")
        } else if relocations > 0 {
            format!("Apply {relocations} relocation fixes")
        } else {
            format!("Apply {case_mismatches} case-mismatch fixes")
        };
        hints.push(Hint::new(
            label,
            build_command_with_glob(ctx, &["links", "fix", "--apply"]),
        ));
    }

    // A vault with nothing broken used to emit no hints whatsoever, making
    // `links` a navigation dead end (dogfood UX-4). Point at the two link
    // questions a clean fix report does *not* answer.
    if hints.is_empty() {
        hints.push(Hint::new(
            "Preview title mentions that could become links",
            build_command_with_glob(ctx, &["links", "auto"]),
        ));
        hints.push(Hint::new(
            "List notes nothing links to",
            build_command_no_glob(ctx, &["find", "--orphan"]),
        ));
    }

    hints
}

pub(super) fn hints_for_links_auto(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    // iter-216 D-4: prefer the explicit `dry_run` key. `applied` is false both
    // for a preview and for an `--apply` run that found nothing, so inverting
    // it alone mislabels the latter.
    let is_dry_run = data
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_else(|| {
            !data
                .get("applied")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        });
    // iter-216 D-3: `matched` (was `total`).
    let total = data
        .get("matched")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    if is_dry_run && total > 0 {
        // Rebuild the exact command from the preview, preserving all
        // scope-narrowing flags so the apply doesn't widen the mutation set.
        let mut args: Vec<&str> = vec!["links", "auto", "--apply"];
        let min_str;
        if let Some(ml) = ctx.auto_link_min_length
            && ml != 3
        {
            args.push("--min-length");
            min_str = ml.to_string();
            args.push(&min_str);
        }
        let cmd = build_command_with_glob(ctx, &args);
        // Append --file and --exclude-title after the builder (they are not
        // glob-related and aren't handled by build_command_with_glob).
        let mut parts = vec![cmd];
        if let Some(ref f) = ctx.auto_link_file {
            parts.push(format!("--file {}", shell_quote(f)));
        }
        for et in &ctx.auto_link_exclude_titles {
            parts.push(format!("--exclude-title {}", shell_quote(et)));
        }
        hints.push(Hint::new(
            format!("Apply {total} auto-links"),
            parts.join(" "),
        ));
    }

    // L-11: a non-zero `files_failed` count means some files produced a
    // valid auto-link plan but the durable write itself failed mid-batch.
    // Point at the per-file detail and suggest the most common cause.
    let files_failed = data
        .get("files_failed")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if files_failed > 0 {
        hints.push(Hint::without_cmd(format!(
            "{files_failed} file(s) failed to write — see `apply_outcomes` for the \
             per-file error, check file permissions"
        )));
    }

    hints
}
