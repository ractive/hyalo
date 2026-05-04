//! `hyalo lint-rules` — manage markdown lint rule configuration in `.hyalo.toml`.
//!
//! Mirrors the `hyalo types` / `hyalo views` shape. All TOML mutations use
//! `toml_edit` to preserve comments and formatting.

use std::path::Path;

use anyhow::{Context, Result};

use crate::output::{CommandOutcome, Format, format_error, format_success};

const TOML_FILENAME: &str = ".hyalo.toml";

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

/// `hyalo lint-rules list` — list all rules with their current effective settings.
pub(crate) fn list_rules(
    config_dir: &Path,
    engine: &hyalo_mdlint::HyaloLintEngine,
    md_lint: &hyalo_mdlint::LintConfig,
    enabled_only: bool,
    disabled_only: bool,
    rule_prefix: Option<&str>,
    _format: Format,
) -> CommandOutcome {
    let rules = engine.available_rules();

    let results: Vec<serde_json::Value> = rules
        .iter()
        .filter(|e| {
            // Apply prefix filter
            if let Some(prefix) = rule_prefix
                && !e.id.starts_with(prefix)
            {
                return false;
            }
            // Determine effective enabled state
            let eff_enabled = effective_enabled(e, md_lint);
            if enabled_only && !eff_enabled {
                return false;
            }
            if disabled_only && eff_enabled {
                return false;
            }
            true
        })
        .map(|e| {
            let eff_sev = effective_severity(e, md_lint);
            let eff_enabled = effective_enabled(e, md_lint);
            let has_override = md_lint.rules.contains_key(&e.id);
            serde_json::json!({
                "id": e.id,
                "name": e.name,
                "description": e.description,
                "default_enabled": e.default_enabled,
                "default_severity": format!("{}", e.default_severity),
                "effective_enabled": eff_enabled,
                "effective_severity": format!("{}", eff_sev),
                "autofixable": e.autofixable,
                "source": e.source,
                "has_override": has_override,
            })
        })
        .collect();

    let _ = config_dir; // not used currently, kept for potential future hints
    let total = results.len() as u64;
    let val = serde_json::json!(results);
    CommandOutcome::success_with_total(format_success(Format::Json, &val), total)
}

// ---------------------------------------------------------------------------
// show
// ---------------------------------------------------------------------------

/// `hyalo lint-rules show <RULE_ID>` — full details for a single rule.
pub(crate) fn show_rule(
    rule_id: &str,
    engine: &hyalo_mdlint::HyaloLintEngine,
    md_lint: &hyalo_mdlint::LintConfig,
    format: Format,
) -> CommandOutcome {
    let Some(entry) = engine.rule_entry(rule_id) else {
        return CommandOutcome::UserError(format_error(
            format,
            &format!("no such rule: {rule_id}"),
            None,
            Some("run `hyalo lint-rules list` to see available rules"),
            None,
        ));
    };

    let eff_sev = effective_severity(entry, md_lint);
    let eff_enabled = effective_enabled(entry, md_lint);
    let override_entry = md_lint.rules.get(rule_id);

    let val = serde_json::json!({
        "id": entry.id,
        "name": entry.name,
        "description": entry.description,
        "default_enabled": entry.default_enabled,
        "default_severity": format!("{}", entry.default_severity),
        "effective_enabled": eff_enabled,
        "effective_severity": format!("{}", eff_sev),
        "autofixable": entry.autofixable,
        "source": entry.source,
        "override": override_entry.map(|ov| {
            serde_json::json!({
                "enabled": ov.enabled(),
                "severity": ov.severity(),
                "mode": ov.mode(),
            })
        }),
    });

    CommandOutcome::success(format_success(Format::Json, &val))
}

// ---------------------------------------------------------------------------
// set
// ---------------------------------------------------------------------------

/// `hyalo lint-rules set <RULE_ID> [--enabled BOOL] [--severity SEV] [--dry-run]`
pub(crate) fn set_rule(
    config_dir: &Path,
    rule_id: &str,
    enabled: Option<bool>,
    severity: Option<&str>,
    dry_run: bool,
    engine: &hyalo_mdlint::HyaloLintEngine,
    format: Format,
) -> Result<CommandOutcome> {
    // Validate rule ID
    if engine.rule_entry(rule_id).is_none() {
        return Ok(CommandOutcome::UserError(format_error(
            format,
            &format!("no such rule: {rule_id}"),
            None,
            Some("run `hyalo lint-rules list` to see available rules"),
            None,
        )));
    }

    // Validate severity value
    if let Some(sev) = severity
        && sev != "warn"
        && sev != "error"
    {
        return Ok(CommandOutcome::UserError(format_error(
            format,
            &format!("invalid severity {sev:?}"),
            None,
            Some("severity must be 'warn' or 'error'"),
            None,
        )));
    }

    // Require at least one mutation
    if enabled.is_none() && severity.is_none() {
        return Ok(CommandOutcome::UserError(format_error(
            format,
            "nothing to set",
            None,
            Some("pass --enabled BOOL and/or --severity warn|error"),
            None,
        )));
    }

    let toml_path = config_dir.join(TOML_FILENAME);
    let contents = std::fs::read_to_string(&toml_path).unwrap_or_default();

    let mut doc: toml_edit::DocumentMut = contents
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("parsing {}", toml_path.display()))?;

    // Determine form: scalar (only enabled, no severity) or table.
    let use_table = severity.is_some();

    // Ensure [lint] table exists.
    if doc.get("lint").is_none() {
        doc["lint"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    if use_table {
        // Promote to table form: [lint.rules.RULE_ID]
        let lint_rules =
            doc["lint"]["rules"].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        let rule_entry =
            lint_rules[rule_id].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));

        if let Some(b) = enabled {
            rule_entry["enabled"] = toml_edit::value(b);
        }
        if let Some(sev) = severity {
            rule_entry["severity"] = toml_edit::value(sev);
        }
    } else {
        // Scalar form: lint.rules.RULE_ID = bool
        let lint_rules =
            doc["lint"]["rules"].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        if let Some(b) = enabled {
            lint_rules[rule_id] = toml_edit::value(b);
        }
    }

    let new_contents = doc.to_string();

    if dry_run {
        let val = serde_json::json!({
            "action": "set",
            "rule_id": rule_id,
            "dry_run": true,
            "enabled": enabled,
            "severity": severity,
            "preview": new_contents,
        });
        return Ok(CommandOutcome::success(format_success(Format::Json, &val)));
    }

    std::fs::write(&toml_path, &new_contents)
        .with_context(|| format!("writing {}", toml_path.display()))?;

    let val = serde_json::json!({
        "action": "set",
        "rule_id": rule_id,
        "dry_run": false,
        "enabled": enabled,
        "severity": severity,
    });
    Ok(CommandOutcome::success(format_success(Format::Json, &val)))
}

// ---------------------------------------------------------------------------
// remove
// ---------------------------------------------------------------------------

/// `hyalo lint-rules remove <RULE_ID> [--dry-run]`
pub(crate) fn remove_rule(
    config_dir: &Path,
    rule_id: &str,
    dry_run: bool,
    engine: &hyalo_mdlint::HyaloLintEngine,
    format: Format,
) -> Result<CommandOutcome> {
    // Validate rule ID
    if engine.rule_entry(rule_id).is_none() {
        return Ok(CommandOutcome::UserError(format_error(
            format,
            &format!("no such rule: {rule_id}"),
            None,
            Some("run `hyalo lint-rules list` to see available rules"),
            None,
        )));
    }

    let toml_path = config_dir.join(TOML_FILENAME);
    let contents = match std::fs::read_to_string(&toml_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // No config file — nothing to remove
            let val = serde_json::json!({
                "action": "remove",
                "rule_id": rule_id,
                "dry_run": dry_run,
                "removed": false,
                "reason": "no .hyalo.toml found",
            });
            return Ok(CommandOutcome::success(format_success(Format::Json, &val)));
        }
        Err(e) => return Err(e).with_context(|| format!("reading {}", toml_path.display())),
    };

    let mut doc: toml_edit::DocumentMut = contents
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("parsing {}", toml_path.display()))?;

    let removed = remove_rule_from_doc(&mut doc, rule_id);

    if !removed {
        let val = serde_json::json!({
            "action": "remove",
            "rule_id": rule_id,
            "dry_run": dry_run,
            "removed": false,
            "reason": "no override found",
        });
        return Ok(CommandOutcome::success(format_success(Format::Json, &val)));
    }

    let new_contents = doc.to_string();

    if dry_run {
        let val = serde_json::json!({
            "action": "remove",
            "rule_id": rule_id,
            "dry_run": true,
            "removed": true,
        });
        return Ok(CommandOutcome::success(format_success(Format::Json, &val)));
    }

    std::fs::write(&toml_path, &new_contents)
        .with_context(|| format!("writing {}", toml_path.display()))?;

    let val = serde_json::json!({
        "action": "remove",
        "rule_id": rule_id,
        "dry_run": false,
        "removed": true,
    });
    Ok(CommandOutcome::success(format_success(Format::Json, &val)))
}

/// Remove the `[lint.rules.<rule_id>]` entry (or scalar `rule_id = bool` entry)
/// from the TOML document. Returns `true` when something was removed.
fn remove_rule_from_doc(doc: &mut toml_edit::DocumentMut, rule_id: &str) -> bool {
    // Try scalar form first (lint.rules.<rule_id> as a key)
    if let Some(rules) = doc
        .get_mut("lint")
        .and_then(|l| l.as_table_mut())
        .and_then(|lt| lt.get_mut("rules"))
        .and_then(|r| r.as_table_mut())
        && rules.contains_key(rule_id)
    {
        rules.remove(rule_id);
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn effective_enabled(
    entry: &hyalo_mdlint::RuleCatalogEntry,
    config: &hyalo_mdlint::LintConfig,
) -> bool {
    if let Some(ov) = config.rules.get(&entry.id)
        && let Some(b) = ov.enabled()
    {
        return b;
    }
    entry.default_enabled
}

fn effective_severity(
    entry: &hyalo_mdlint::RuleCatalogEntry,
    config: &hyalo_mdlint::LintConfig,
) -> hyalo_mdlint::DiagSeverity {
    if let Some(ov) = config.rules.get(&entry.id)
        && let Some(sev_str) = ov.severity()
    {
        return match sev_str {
            "error" => hyalo_mdlint::DiagSeverity::Error,
            _ => hyalo_mdlint::DiagSeverity::Warn,
        };
    }
    entry.default_severity
}
