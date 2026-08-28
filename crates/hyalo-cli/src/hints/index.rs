//! Hints for `create-index` / `drop-index` and the OKF generators.
//!
//! Split out of the single 5,059-line `hints.rs` in iteration 247 (deep-review
//! hotspot). This is a file split only: the items keep the visibility they had
//! inside the one module, so `hints::...` paths and behaviour are unchanged.

use super::{Hint, HintContext, build_command_no_glob};

pub(super) fn hints_for_create_index(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    // Use bare `--index` (defaults to .hyalo-index in vault dir) for the default path.
    // Only include the explicit path when the index was created at a non-default location.
    let index_path = data
        .get("path")
        .and_then(|p| p.as_str())
        .or(ctx.index_path.as_deref());

    // Only treat as default when no path was reported or it's the bare default name.
    // Custom paths like `sub/.hyalo-index` must emit the explicit path in the hint.
    let is_default = index_path.is_none_or(|p| p == ".hyalo-index");

    let hint_cmd = if is_default {
        build_command_no_glob(ctx, &["find", "--index"])
    } else {
        build_command_no_glob(
            ctx,
            &["find", "--index-file", index_path.unwrap_or(".hyalo-index")],
        )
    };

    hints.push(Hint::new("Query using the index", hint_cmd));
    // L-9: the drop hint has to name the same file the query hint does.
    // A bare `drop-index` after `create-index -o <custom>` targets the default
    // `<vault>/.hyalo-index` — a different file, which is either absent or
    // someone else's index.
    let drop_cmd = if is_default {
        build_command_no_glob(ctx, &["drop-index"])
    } else {
        build_command_no_glob(
            ctx,
            &["drop-index", "--path", index_path.unwrap_or(".hyalo-index")],
        )
    };
    hints.push(Hint::new("Delete the index when done", drop_cmd));

    hints
}

pub(super) fn hints_for_drop_index(ctx: &HintContext, _data: &serde_json::Value) -> Vec<Hint> {
    vec![Hint::new(
        "Rebuild the index",
        build_command_no_glob(ctx, &["create-index"]),
    )]
}

/// Build the profile-aware `hyalo lint` validate hint. When the `okf` profile is
/// already active via `[lint] profiles`, the `--profile okf` flag is redundant
/// (plain `hyalo lint` runs it), so it is dropped to avoid noisy advice.
pub(super) fn okf_validate_hint(ctx: &HintContext) -> Hint {
    let cmd = if ctx.okf_profile_active {
        build_command_no_glob(ctx, &["lint"])
    } else {
        build_command_no_glob(ctx, &["lint", "--profile", "okf"])
    };
    Hint::new("Validate bundle conformance", cmd)
}

/// Drill-down hints for `hyalo okf index`.
///
/// Always suggests validating conformance. In a dry run that detected drift, it
/// additionally suggests applying the regenerated index. The `apply` /
/// `changed` signals are read from the command's own JSON result so the hint
/// stays in sync with what actually happened.
pub(super) fn hints_for_okf_index(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();
    let applied = data
        .get("apply")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let changed = data
        .get("changed")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    // On dry-run drift, applying is the natural next step — surface it first.
    if !applied && changed > 0 {
        hints.push(Hint::new(
            "Apply the regenerated index files",
            build_command_no_glob(ctx, &["okf", "index", "--apply"]),
        ));
    }
    hints.push(okf_validate_hint(ctx));
    hints
}

/// Drill-down hints for `hyalo okf log`: suggest validating conformance.
pub(super) fn hints_for_okf_log(ctx: &HintContext) -> Vec<Hint> {
    vec![okf_validate_hint(ctx)]
}
