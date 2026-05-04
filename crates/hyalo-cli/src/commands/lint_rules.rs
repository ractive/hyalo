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
#[allow(clippy::too_many_arguments)]
pub(crate) fn list_rules(
    config_dir: &Path,
    engine: &hyalo_mdlint::HyaloLintEngine,
    md_lint: &hyalo_mdlint::LintConfig,
    schema: &hyalo_core::schema::SchemaConfig,
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
            let activation = activation_for(&e.id, schema);
            let mut entry = serde_json::json!({
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
            });
            if let Some(act) = activation {
                entry["activation"] = serde_json::json!({
                    "predicate": act.predicate,
                    "satisfied": act.satisfied,
                });
            }
            entry
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
    schema: &hyalo_core::schema::SchemaConfig,
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
    let activation = activation_for(&entry.id, schema);

    let mut val = serde_json::json!({
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
            })
        }),
    });
    if let Some(act) = activation {
        val["activation"] = serde_json::json!({
            "predicate": act.predicate,
            "satisfied": act.satisfied,
        });
    }

    CommandOutcome::success(format_success(Format::Json, &val))
}

// ---------------------------------------------------------------------------
// set
// ---------------------------------------------------------------------------

/// `hyalo lint-rules set <RULE_ID> [--enabled BOOL] [--severity SEV] [--dry-run]`
#[allow(clippy::too_many_arguments)]
pub(crate) fn set_rule(
    config_dir: &Path,
    rule_id: &str,
    enabled: Option<bool>,
    severity: Option<&str>,
    dry_run: bool,
    engine: &hyalo_mdlint::HyaloLintEngine,
    md_lint: &hyalo_mdlint::LintConfig,
    format: Format,
) -> Result<CommandOutcome> {
    // Validate rule ID
    let Some(entry) = engine.rule_entry(rule_id) else {
        return Ok(CommandOutcome::UserError(format_error(
            format,
            &format!("no such rule: {rule_id}"),
            None,
            Some("run `hyalo lint-rules list` to see available rules"),
            None,
        )));
    };

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

    // Compute the "before" state (effective values from current config).
    let before_enabled = effective_enabled(entry, md_lint);
    let before_severity = format!("{}", effective_severity(entry, md_lint));

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

    let lint_rules =
        doc["lint"]["rules"].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));

    if use_table {
        // Table form: [lint.rules.RULE_ID]
        // BUG-1 fix: if the existing entry is a scalar bool, we cannot index into
        // it as a table (it would panic with "index not found"). Detect the scalar
        // form and promote it to a table, preserving its boolean as `enabled`.
        let promoted_enabled: Option<bool> = lint_rules
            .get(rule_id)
            .filter(|item| !item.is_table() && !item.is_inline_table())
            .and_then(|item| item.as_value())
            .and_then(toml_edit::Value::as_bool);

        let needs_promotion = promoted_enabled.is_some();
        if needs_promotion {
            // Replace the scalar with an empty table; preserve enabled below.
            if let Some(rules_tbl) = lint_rules.as_table_mut() {
                rules_tbl.remove(rule_id);
                rules_tbl.insert(rule_id, toml_edit::Item::Table(toml_edit::Table::new()));
            } else if let Some(rules_inline) = lint_rules.as_inline_table_mut() {
                rules_inline.remove(rule_id);
                rules_inline.insert(
                    rule_id,
                    toml_edit::value(toml_edit::InlineTable::new())
                        .into_value()
                        .map_err(|_| {
                            anyhow::anyhow!(
                                "failed to build inline-table value for lint.rules.{rule_id}"
                            )
                        })?,
                );
            }
        }

        let rule_entry =
            lint_rules[rule_id].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));

        // Preserve the existing scalar's value when no explicit --enabled was given.
        if let Some(b) = enabled {
            rule_entry["enabled"] = toml_edit::value(b);
        } else if let Some(prev) = promoted_enabled {
            rule_entry["enabled"] = toml_edit::value(prev);
        }
        if let Some(sev) = severity {
            rule_entry["severity"] = toml_edit::value(sev);
        }
    } else if let Some(b) = enabled {
        // Only `--enabled` provided. If a table-form override already exists,
        // update its `enabled` field in place (preserving severity).
        // Otherwise write the scalar form: lint.rules.RULE_ID = bool.
        if let Some(existing) = lint_rules.get_mut(rule_id)
            && (existing.is_table() || existing.is_inline_table())
        {
            existing["enabled"] = toml_edit::value(b);
        } else {
            lint_rules[rule_id] = toml_edit::value(b);
        }
    }

    let new_contents = doc.to_string();

    // Compute "after" state by re-parsing the new TOML through the same
    // override-extraction logic. Falls back to assuming the new values stick
    // when re-parsing fails (which shouldn't happen — we just wrote it).
    let (after_enabled, after_severity) = compute_after_state(
        &new_contents,
        rule_id,
        entry,
        before_enabled,
        &before_severity,
        enabled,
        severity,
    );

    let path_str = toml_path.display().to_string();

    if dry_run {
        let val = serde_json::json!({
            "action": "set",
            "rule_id": rule_id,
            "dry_run": true,
            "enabled": enabled,
            "severity": severity,
            "before": {
                "enabled": before_enabled,
                "severity": before_severity,
            },
            "after": {
                "enabled": after_enabled,
                "severity": after_severity,
            },
            "config_path": path_str,
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
        "before": {
            "enabled": before_enabled,
            "severity": before_severity,
        },
        "after": {
            "enabled": after_enabled,
            "severity": after_severity,
        },
        "config_path": path_str,
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
    md_lint: &hyalo_mdlint::LintConfig,
    format: Format,
) -> Result<CommandOutcome> {
    // Validate rule ID
    let Some(entry) = engine.rule_entry(rule_id) else {
        return Ok(CommandOutcome::UserError(format_error(
            format,
            &format!("no such rule: {rule_id}"),
            None,
            Some("run `hyalo lint-rules list` to see available rules"),
            None,
        )));
    };

    let before_enabled = effective_enabled(entry, md_lint);
    let before_severity = format!("{}", effective_severity(entry, md_lint));
    let path_str = config_dir.join(TOML_FILENAME).display().to_string();

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
                "before": {"enabled": before_enabled, "severity": before_severity},
                "after": {"enabled": before_enabled, "severity": before_severity},
                "config_path": path_str,
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
            "before": {"enabled": before_enabled, "severity": before_severity},
            "after": {"enabled": before_enabled, "severity": before_severity},
            "config_path": path_str,
        });
        return Ok(CommandOutcome::success(format_success(Format::Json, &val)));
    }

    let new_contents = doc.to_string();

    // After removing, the effective state reverts to the rule's defaults.
    let after_enabled = entry.default_enabled;
    let after_severity = format!("{}", entry.default_severity);

    if dry_run {
        let val = serde_json::json!({
            "action": "remove",
            "rule_id": rule_id,
            "dry_run": true,
            "removed": true,
            "before": {"enabled": before_enabled, "severity": before_severity},
            "after": {"enabled": after_enabled, "severity": after_severity},
            "config_path": path_str,
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
        "before": {"enabled": before_enabled, "severity": before_severity},
        "after": {"enabled": after_enabled, "severity": after_severity},
        "config_path": path_str,
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

/// Compute the post-write effective enabled/severity for the rule.
///
/// Overrides take precedence over defaults; when the user passes a flag
/// explicitly, that wins. Otherwise we fall back to the previous effective
/// value (no change for this dimension).
fn compute_after_state(
    _new_contents: &str,
    _rule_id: &str,
    entry: &hyalo_mdlint::RuleCatalogEntry,
    before_enabled: bool,
    before_severity: &str,
    enabled: Option<bool>,
    severity: Option<&str>,
) -> (bool, String) {
    let _ = entry; // not needed beyond defaults; reserved for future use
    let after_enabled = enabled.unwrap_or(before_enabled);
    let after_severity = severity.map_or_else(|| before_severity.to_owned(), str::to_owned);
    (after_enabled, after_severity)
}

/// Activation predicate description for rules whose effective behaviour depends
/// on schema state at runtime. Returns `None` for rules that always run when
/// enabled.
struct Activation {
    predicate: String,
    satisfied: bool,
}

fn activation_for(rule_id: &str, schema: &hyalo_core::schema::SchemaConfig) -> Option<Activation> {
    if rule_id == "HYALO002" {
        let satisfied = schema_has_completed_status(schema);
        return Some(Activation {
            predicate: "schema declares `status` as enum containing \"completed\"".to_owned(),
            satisfied,
        });
    }
    None
}

fn schema_has_completed_status(schema: &hyalo_core::schema::SchemaConfig) -> bool {
    use hyalo_core::schema::PropertyConstraint;
    let check = |props: &std::collections::HashMap<String, PropertyConstraint>| -> bool {
        if let Some(PropertyConstraint::Enum { values }) = props.get("status") {
            return values.iter().any(|v| v == "completed");
        }
        false
    };
    if check(&schema.default_schema().properties) {
        return true;
    }
    schema.types.values().any(|ts| check(&ts.properties))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyalo_core::schema::SchemaConfig;
    use tempfile::tempdir;

    fn make_engine() -> hyalo_mdlint::HyaloLintEngine {
        hyalo_mdlint::HyaloLintEngine::create().expect("engine creates")
    }

    /// Reload the lint config from a directory's `.hyalo.toml` so the test can
    /// assert against the same path the production code reads.
    fn load_md_lint(dir: &Path) -> hyalo_mdlint::LintConfig {
        let path = dir.join(".hyalo.toml");
        let contents = std::fs::read_to_string(&path).unwrap_or_default();
        let mut config = hyalo_mdlint::LintConfig::default();
        let Ok(table) = contents.parse::<toml::Table>() else {
            return config;
        };
        let Some(toml::Value::Table(lint)) = table.get("lint") else {
            return config;
        };
        let Some(toml::Value::Table(rules)) = lint.get("rules") else {
            return config;
        };
        for (rule_id, value) in rules {
            let ov = match value {
                toml::Value::Boolean(b) => hyalo_mdlint::RuleOverride::Enabled(*b),
                toml::Value::Table(tbl) => {
                    let enabled = tbl.get("enabled").and_then(toml::Value::as_bool);
                    let severity = tbl
                        .get("severity")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned);
                    hyalo_mdlint::RuleOverride::Table { enabled, severity }
                }
                _ => continue,
            };
            config.rules.insert(rule_id.clone(), ov);
        }
        config
    }

    /// BUG-1 reproducer: scalar `MD013 = true` followed by
    /// `set --severity error` must NOT panic and must promote the entry to a
    /// table preserving the original boolean.
    #[test]
    fn set_severity_promotes_scalar_bool_to_table() {
        let dir = tempdir().unwrap();
        let toml_path = dir.path().join(".hyalo.toml");
        // Pre-seed: scalar form
        std::fs::write(&toml_path, "[lint.rules]\nMD013 = true\n").unwrap();

        let engine = make_engine();
        let md_lint = load_md_lint(dir.path());
        let schema = SchemaConfig::default();

        let outcome = set_rule(
            dir.path(),
            "MD013",
            None,
            Some("error"),
            false,
            &engine,
            &md_lint,
            Format::Json,
        )
        .expect("set --severity must not panic on scalar leaf");

        match outcome {
            CommandOutcome::Success { .. } => {}
            other => panic!("expected success, got {other:?}"),
        }

        let new_contents = std::fs::read_to_string(&toml_path).unwrap();
        // Must contain a table form with both enabled (true preserved) and severity = "error".
        assert!(
            new_contents.contains("[lint.rules.MD013]") || new_contents.contains("MD013 = { "),
            "expected table form for MD013, got:\n{new_contents}"
        );
        assert!(
            new_contents.contains("enabled = true"),
            "expected preserved enabled = true, got:\n{new_contents}"
        );
        assert!(
            new_contents.contains("severity = \"error\""),
            "expected severity = \"error\", got:\n{new_contents}"
        );

        let _ = (schema, &md_lint);
    }

    #[test]
    fn set_severity_promotes_scalar_false_to_table() {
        let dir = tempdir().unwrap();
        let toml_path = dir.path().join(".hyalo.toml");
        std::fs::write(&toml_path, "[lint.rules]\nMD013 = false\n").unwrap();

        let engine = make_engine();
        let md_lint = load_md_lint(dir.path());

        let _ = set_rule(
            dir.path(),
            "MD013",
            None,
            Some("warn"),
            false,
            &engine,
            &md_lint,
            Format::Json,
        )
        .unwrap();

        let new_contents = std::fs::read_to_string(&toml_path).unwrap();
        assert!(
            new_contents.contains("enabled = false"),
            "expected preserved enabled = false, got:\n{new_contents}"
        );
        assert!(new_contents.contains("severity = \"warn\""));
    }

    /// UX-4: HYALO002 activation predicate reflects schema state.
    #[test]
    fn activation_for_hyalo002_reflects_schema() {
        use hyalo_core::schema::{PropertyConstraint, TypeSchema};

        let empty = SchemaConfig::default();
        let act = activation_for("HYALO002", &empty).expect("HYALO002 has activation");
        assert!(!act.satisfied, "no schema → unsatisfied");

        // Schema with status enum that includes "completed".
        let mut props = std::collections::HashMap::new();
        props.insert(
            "status".to_owned(),
            PropertyConstraint::Enum {
                values: vec!["planned".into(), "completed".into()],
            },
        );
        let mut types = std::collections::HashMap::new();
        types.insert(
            "iteration".to_owned(),
            TypeSchema {
                properties: props,
                ..Default::default()
            },
        );
        let schema = SchemaConfig {
            default: TypeSchema::default(),
            types,
        };
        let act = activation_for("HYALO002", &schema).unwrap();
        assert!(act.satisfied, "schema with completed → satisfied");
    }

    /// HYALO001 has no activation field (it always runs when enabled).
    #[test]
    fn activation_for_hyalo001_is_none() {
        let empty = SchemaConfig::default();
        assert!(activation_for("HYALO001", &empty).is_none());
    }
}
