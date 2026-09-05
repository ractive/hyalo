//! Hints for the configuration-surface listings: `views`, `lint-rules`, `types`, `new`.
//!
//! Split out of the single 5,059-line `hints.rs` in iteration 247 (deep-review
//! hotspot). This is a file split only: the items keep the visibility they had
//! inside the one module, so `hints::...` paths and behaviour are unchanged.

use super::{Hint, HintContext, HintSource, MAX_HINTS, build_command_no_glob};

/// Drill-downs for `hyalo views list` (iter-210, dogfood UX-4).
///
/// The listing was a dead end: it named saved queries without saying how to
/// run one, and on an empty vault it printed nothing at all with no way
/// forward. The hints now always lead somewhere — into the first view when one
/// exists, into creating one when none does.
pub(super) fn hints_for_views_list(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let names: Vec<&str> = data
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("name").and_then(serde_json::Value::as_str))
                .collect()
        })
        .unwrap_or_default();

    if let Some(first) = names.first() {
        hints.push(Hint::new(
            format!("Run the '{first}' view"),
            build_command_no_glob(ctx, &["find", "--view", first]),
        ));
        if hints.len() < MAX_HINTS {
            hints.push(Hint::new(
                format!("Delete the '{first}' view"),
                build_command_no_glob(ctx, &["views", "remove", first]),
            ));
        }
    } else {
        // No views yet. `views set` needs at least one filter to be a valid
        // command, so the suggestion carries a concrete (and runnable) one.
        hints.push(Hint::new(
            "Save a query as a view",
            build_command_no_glob(
                ctx,
                &["views", "set", "drafts", "--property", "status=draft"],
            ),
        ));
        hints.push(Hint::new(
            "Survey the vault to decide which query is worth saving",
            build_command_no_glob(ctx, &["summary"]),
        ));
    }

    hints
}

/// Drill-downs for `hyalo lint-rules list` (iter-210, dogfood UX-4).
///
/// Picks the first *overridden* rule when there is one — that is the row a
/// reader is most likely to be checking — and otherwise the first listed rule.
pub(super) fn hints_for_lint_rules_list(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let rules = data.as_array().map(Vec::as_slice).unwrap_or_default();
    let rule_id = |v: &serde_json::Value| {
        v.get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let overridden = rules
        .iter()
        .find(|r| {
            r.get("has_override")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .and_then(rule_id);
    let focus = overridden
        .clone()
        .or_else(|| rules.first().and_then(rule_id));

    if let Some(ref id) = focus {
        hints.push(Hint::new(
            format!("Show what {id} checks and how to configure it"),
            build_command_no_glob(ctx, &["lint-rules", "show", id]),
        ));
    }

    if let Some(ref id) = overridden {
        if hints.len() < MAX_HINTS {
            hints.push(Hint::new(
                format!("Drop the {id} override and go back to the default"),
                build_command_no_glob(ctx, &["lint-rules", "remove", id]),
            ));
        }
    } else if let Some(ref id) = focus
        && hints.len() < MAX_HINTS
    {
        hints.push(Hint::new(
            format!("Turn {id} off for this vault"),
            build_command_no_glob(ctx, &["lint-rules", "set", id, "--enabled", "false"]),
        ));
    }

    if hints.len() < MAX_HINTS {
        match focus {
            // Narrowing lint to the rule in question is the fastest way to see
            // whether it actually fires here.
            Some(ref id) => hints.push(Hint::new(
                format!("Run just {id} against the vault"),
                build_command_no_glob(ctx, &["lint", "--rule", id]),
            )),
            None => hints.push(Hint::new(
                "Run the markdown lint rules against the vault",
                build_command_no_glob(ctx, &["lint"]),
            )),
        }
    }

    hints
}

/// Drill-downs for `hyalo lint-rules show <ID>` (NEW-18, dogfood pre3).
///
/// Was a hint dead end despite inspecting one specific, actionable rule — the
/// natural next steps are running lint scoped to just that rule, and either
/// toggling it or dropping an existing override.
pub(super) fn hints_for_lint_rules_show(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let id = data
        .get("id")
        .and_then(serde_json::Value::as_str)
        .or(ctx.lint_rule.as_deref());
    let Some(id) = id else {
        return hints;
    };

    hints.push(Hint::new(
        format!("Run just {id} against the vault"),
        build_command_no_glob(ctx, &["lint", "--rule", id]),
    ));

    // BUG-27 (iter-276): a rule marked `configurable: false` — SCHEMA, whose
    // severity comes from the schema — has no `lint-rules set`/`remove` form
    // at all, and the hint hyalo printed (`=> hyalo lint-rules set SCHEMA
    // --enabled false`) failed with `no such rule` when run. The rule's own
    // `note` already says to use `hyalo types set`; don't contradict it.
    let configurable = data
        .get("configurable")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    if !configurable {
        return hints;
    }

    let has_override = data.get("override").is_some_and(|o| !o.is_null());
    if has_override {
        if hints.len() < MAX_HINTS {
            hints.push(Hint::new(
                format!("Drop the {id} override and go back to the default"),
                build_command_no_glob(ctx, &["lint-rules", "remove", id]),
            ));
        }
    } else if hints.len() < MAX_HINTS {
        let effective_enabled = data
            .get("effective_enabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        if effective_enabled {
            hints.push(Hint::new(
                format!("Turn {id} off for this vault"),
                build_command_no_glob(ctx, &["lint-rules", "set", id, "--enabled", "false"]),
            ));
        } else {
            hints.push(Hint::new(
                format!("Turn {id} on for this vault"),
                build_command_no_glob(ctx, &["lint-rules", "set", id, "--enabled", "true"]),
            ));
        }
    }

    hints
}

pub(super) fn hints_for_types(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let subcommand = match &ctx.source {
        HintSource::Types { subcommand } => subcommand.as_deref().unwrap_or("list"),
        _ => "list",
    };

    let mut hints = Vec::new();

    match subcommand {
        "list" => {
            // Suggest showing the first listed type.
            if let Some(first_type) = data
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|entry| entry.get("type"))
                .and_then(serde_json::Value::as_str)
            {
                hints.push(Hint::new(
                    format!("Show schema for type: {first_type}"),
                    build_command_no_glob(ctx, &["types", "show", first_type]),
                ));
            }
            if hints.len() < MAX_HINTS {
                hints.push(Hint::new(
                    "Validate all files against schema",
                    build_command_no_glob(ctx, &["lint"]),
                ));
            }
        }
        "show" => {
            let type_name = data.get("type").and_then(serde_json::Value::as_str);
            // Suggest scaffolding a new file of this type when the type
            // declares any `required` properties. Without required fields,
            // `hyalo new` would only emit a `type:` stub — low value.
            let has_required = data
                .get("required")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|arr| !arr.is_empty());
            if let Some(name) = type_name
                && has_required
                && hints.len() < MAX_HINTS
            {
                let placeholder = format!("path/to/new-{name}.md");
                hints.push(Hint::new(
                    format!("Scaffold a new file of type: {name}"),
                    build_command_no_glob(ctx, &["new", "--type", name, "--file", &placeholder]),
                ));
            }
            if hints.len() < MAX_HINTS {
                hints.push(Hint::new(
                    "Validate files against schema",
                    build_command_no_glob(ctx, &["lint"]),
                ));
            }
            if hints.len() < MAX_HINTS {
                hints.push(Hint::new(
                    "List all type schemas",
                    build_command_no_glob(ctx, &["types", "list"]),
                ));
            }
            if let Some(name) = type_name
                && hints.len() < MAX_HINTS
            {
                let filter = format!("type={name}");
                hints.push(Hint::new(
                    format!("Find files of type: {name}"),
                    build_command_no_glob(ctx, &["find", "--property", &filter]),
                ));
            }
        }
        "set" => {
            let type_name = data.get("type").and_then(serde_json::Value::as_str);
            if let Some(name) = type_name
                && hints.len() < MAX_HINTS
            {
                hints.push(Hint::new(
                    format!("Review updated schema: {name}"),
                    build_command_no_glob(ctx, &["types", "show", name]),
                ));
            }
            if hints.len() < MAX_HINTS {
                hints.push(Hint::new(
                    "Validate files against schema",
                    build_command_no_glob(ctx, &["lint"]),
                ));
            }
        }
        _ => {}
    }

    hints
}

pub(super) fn hints_for_new(ctx: &HintContext, file: &str) -> Vec<Hint> {
    vec![Hint::new(
        "Validate the new file and see placeholder violations",
        build_command_no_glob(ctx, &["lint", "--file", file]),
    )]
}
