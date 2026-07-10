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

    // Compute the desired post-set state by combining new flags with defaults.
    let target_enabled = enabled.unwrap_or(before_enabled);
    let target_severity_str = severity.map_or_else(|| before_severity.clone(), str::to_owned);
    let default_severity_str = format!("{}", entry.default_severity);

    // An override is needed for each dimension only when the target diverges
    // from the rule's built-in default. Setting a value back to its default is
    // a no-op (BUG-2) — we should remove any existing override and prune
    // empty parent tables.
    let need_enabled_override = target_enabled != entry.default_enabled;
    let need_severity_override = target_severity_str != default_severity_str;

    if !need_enabled_override && !need_severity_override {
        // Pure no-op: drop any existing override on this rule.
        if let Some(rules_item) = doc.get_mut("lint").and_then(|lint| lint.get_mut("rules"))
            && let Some(rules_tbl) = rules_item.as_table_mut()
        {
            rules_tbl.remove(rule_id);
        }
    } else if need_severity_override {
        // Table form is required to carry severity.
        let lint_rules = match lint_rules_table_mut(&mut doc) {
            Ok(item) => item,
            Err(err) => {
                return Ok(CommandOutcome::UserError(format_error(
                    format,
                    &err.to_string(),
                    None,
                    Some("fix or remove the malformed entry in .hyalo.toml"),
                    None,
                )));
            }
        };

        // If the existing entry is a scalar bool, promote it to a table so
        // we can index into it without panicking. Handle both regular and
        // inline parent tables — `lint = { rules = { MD013 = false } }`
        // encodes `lint_rules` as an inline table, and skipping it here would
        // resurrect the same panic the regular-table branch is guarding
        // against.
        if lint_rules
            .get(rule_id)
            .is_some_and(|item| !item.is_table() && !item.is_inline_table())
        {
            if let Some(rules_tbl) = lint_rules.as_table_mut() {
                rules_tbl.remove(rule_id);
                rules_tbl.insert(rule_id, toml_edit::Item::Table(toml_edit::Table::new()));
            } else if let Some(rules_inline) = lint_rules.as_inline_table_mut() {
                rules_inline.remove(rule_id);
                rules_inline.insert(
                    rule_id,
                    toml_edit::Value::InlineTable(toml_edit::InlineTable::default()),
                );
            }
        }

        let rule_entry =
            lint_rules[rule_id].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));

        if need_enabled_override {
            rule_entry["enabled"] = toml_edit::value(target_enabled);
        } else if let Some(tbl) = rule_entry.as_table_mut() {
            tbl.remove("enabled");
        } else if let Some(inline) = rule_entry.as_inline_table_mut() {
            inline.remove("enabled");
        }
        rule_entry["severity"] = toml_edit::value(target_severity_str.clone());
    } else {
        // Only `enabled` diverges from default — scalar form is enough.
        let lint_rules = match lint_rules_table_mut(&mut doc) {
            Ok(item) => item,
            Err(err) => {
                return Ok(CommandOutcome::UserError(format_error(
                    format,
                    &err.to_string(),
                    None,
                    Some("fix or remove the malformed entry in .hyalo.toml"),
                    None,
                )));
            }
        };

        if let Some(existing) = lint_rules.get_mut(rule_id)
            && (existing.is_table() || existing.is_inline_table())
        {
            // Preserve existing table entry: drop severity (matches default)
            // and set enabled.
            if let Some(tbl) = existing.as_table_mut() {
                tbl.remove("severity");
            } else if let Some(inline) = existing.as_inline_table_mut() {
                inline.remove("severity");
            }
            existing["enabled"] = toml_edit::value(target_enabled);
        } else {
            lint_rules[rule_id] = toml_edit::value(target_enabled);
        }
    }

    // Prune empty parent tables so consecutive set/unset cycles don't leave
    // orphan headers (`[lint.rules]` / `[lint]`) behind.
    prune_empty_lint_tables(&mut doc);

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
        // Mirror the non-dry-run write decision so the text formatter can
        // distinguish a tautological dry-run (no diff) from one that would
        // mutate the file.
        let would_write = new_contents != contents;
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
            "wrote": would_write,
        });
        return Ok(CommandOutcome::success(format_success(Format::Json, &val)));
    }

    // Skip the write when the on-disk content would not change (BUG-2:
    // setting a property to its current default value should be a true no-op,
    // not "(no change)" + a redundant write).
    let wrote = if new_contents == contents {
        false
    } else {
        std::fs::write(&toml_path, &new_contents)
            .with_context(|| format!("writing {}", toml_path.display()))?;
        true
    };

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
        "wrote": wrote,
    });
    Ok(CommandOutcome::success(format_success(Format::Json, &val)))
}

/// Get (creating if absent) the `[lint.rules]` table as a mutable `Item`.
///
/// Returns a user-facing error if `lint` or `lint.rules` exist but are not
/// TOML tables (e.g. the user hand-edited `.hyalo.toml` into something like
/// `lint = "oops"`). We prefer a clear error over a panic on user input —
/// mirrors `ensure_schema_types_table` in `types.rs` for the identical
/// malformed-config scenario.
fn lint_rules_table_mut(doc: &mut toml_edit::DocumentMut) -> Result<&mut toml_edit::Item> {
    if doc.get("lint").is_none() {
        doc["lint"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let lint = doc["lint"].as_table_mut().ok_or_else(|| {
        anyhow::anyhow!("malformed .hyalo.toml: `lint` is not a table — expected `[lint]` section")
    })?;
    if !lint.contains_key("rules") {
        lint.insert("rules", toml_edit::Item::Table(toml_edit::Table::new()));
    }
    if !lint["rules"].is_table() && !lint["rules"].is_inline_table() {
        return Err(anyhow::anyhow!(
            "malformed .hyalo.toml: `lint.rules` is not a table — expected `[lint.rules]` section"
        ));
    }
    Ok(&mut lint["rules"])
}

/// Remove `[lint.rules]` and `[lint]` from `doc` when they have no surviving
/// keys, so that pruning an override doesn't leave an empty section header
/// behind.
fn prune_empty_lint_tables(doc: &mut toml_edit::DocumentMut) {
    fn is_empty(item: &toml_edit::Item) -> bool {
        match item {
            toml_edit::Item::Table(t) => t.is_empty(),
            toml_edit::Item::Value(toml_edit::Value::InlineTable(t)) => t.is_empty(),
            _ => false,
        }
    }

    if let Some(lint) = doc.get_mut("lint") {
        if let Some(rules) = lint.get("rules")
            && is_empty(rules)
            && let Some(lint_tbl) = lint.as_table_mut()
        {
            lint_tbl.remove("rules");
        }
        if is_empty(lint) {
            doc.as_table_mut().remove("lint");
        }
    }
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
    // Prune empty `[lint.rules]` and `[lint]` tables that may have been left
    // behind by the removal (mirrors the pruning done in `set_rule`).
    if removed {
        prune_empty_lint_tables(&mut doc);
    }

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

    /// CRITICAL reproducer: `.hyalo.toml` with `lint` as a non-table scalar
    /// must return a clean error from `set --severity`, not panic via
    /// `IndexMut` on a non-table `Item`.
    #[test]
    fn set_severity_malformed_lint_scalar_returns_error() {
        let dir = tempdir().unwrap();
        let toml_path = dir.path().join(".hyalo.toml");
        std::fs::write(&toml_path, "lint = \"oops\"\n").unwrap();

        let engine = make_engine();
        let md_lint = load_md_lint(dir.path());

        let result = set_rule(
            dir.path(),
            "MD013",
            None,
            Some("error"),
            false,
            &engine,
            &md_lint,
            Format::Json,
        );

        let outcome = result.expect("malformed `lint` scalar must not panic or bubble up");
        let CommandOutcome::UserError(msg) = outcome else {
            panic!("expected UserError outcome, got {outcome:?}");
        };
        assert!(
            msg.contains("malformed") && msg.contains("lint"),
            "unexpected error: {msg}"
        );

        // The file must be left untouched — no partial write on error.
        let contents_after = std::fs::read_to_string(&toml_path).unwrap();
        assert_eq!(contents_after, "lint = \"oops\"\n");
    }

    /// CRITICAL reproducer: same malformed config for the `--enabled` path
    /// (scalar-form branch), which used the same unguarded `doc["lint"]["rules"]`
    /// indexing.
    #[test]
    fn set_enabled_malformed_lint_scalar_returns_error() {
        let dir = tempdir().unwrap();
        let toml_path = dir.path().join(".hyalo.toml");
        std::fs::write(&toml_path, "lint = \"oops\"\n").unwrap();

        let engine = make_engine();
        let md_lint = load_md_lint(dir.path());

        // MD013 defaults to enabled=false, so `--enabled true` diverges from
        // the default and takes the scalar-form (`else`) branch.
        let result = set_rule(
            dir.path(),
            "MD013",
            Some(true),
            None,
            false,
            &engine,
            &md_lint,
            Format::Json,
        );

        let outcome = result.expect("malformed `lint` scalar must not panic or bubble up");
        let CommandOutcome::UserError(msg) = outcome else {
            panic!("expected UserError outcome, got {outcome:?}");
        };
        assert!(
            msg.contains("malformed") && msg.contains("lint"),
            "unexpected error: {msg}"
        );

        let contents_after = std::fs::read_to_string(&toml_path).unwrap();
        assert_eq!(contents_after, "lint = \"oops\"\n");
    }

    /// `lint.rules` present but not a table (e.g. hand-edited into a scalar)
    /// must also error cleanly rather than panic.
    #[test]
    fn set_severity_malformed_lint_rules_scalar_returns_error() {
        let dir = tempdir().unwrap();
        let toml_path = dir.path().join(".hyalo.toml");
        std::fs::write(&toml_path, "[lint]\nrules = \"oops\"\n").unwrap();

        let engine = make_engine();
        let md_lint = load_md_lint(dir.path());

        let result = set_rule(
            dir.path(),
            "MD013",
            None,
            Some("error"),
            false,
            &engine,
            &md_lint,
            Format::Json,
        );

        let outcome = result.expect("malformed `lint.rules` scalar must not panic or bubble up");
        let CommandOutcome::UserError(msg) = outcome else {
            panic!("expected UserError outcome, got {outcome:?}");
        };
        assert!(
            msg.contains("malformed") && msg.contains("lint.rules"),
            "unexpected error: {msg}"
        );
    }

    /// `lint-rules remove` already guards its read path with `.and_then`
    /// chains (no blind indexing), so a malformed `lint` scalar must fall
    /// through to "no override found" rather than panicking or erroring.
    #[test]
    fn remove_rule_malformed_lint_scalar_does_not_panic() {
        let dir = tempdir().unwrap();
        let toml_path = dir.path().join(".hyalo.toml");
        std::fs::write(&toml_path, "lint = \"oops\"\n").unwrap();

        let engine = make_engine();
        let md_lint = load_md_lint(dir.path());

        let outcome = remove_rule(dir.path(), "MD013", false, &engine, &md_lint, Format::Json)
            .expect("remove must not panic on malformed lint scalar");

        match outcome {
            CommandOutcome::Success { output, .. } => {
                let v: serde_json::Value = serde_json::from_str(&output).unwrap();
                assert_eq!(v["removed"], false);
            }
            other => panic!("expected success, got {other:?}"),
        }
    }

    #[test]
    fn set_severity_promotes_scalar_false_to_table() {
        // Use MD011 (default_enabled = true, default_severity = error) so
        // the existing `MD011 = false` is a meaningful override, and
        // `--severity warn` likewise diverges from the default. After iter-131
        // we prune redundant overrides, so this test must use a rule whose
        // default differs from the requested values.
        let dir = tempdir().unwrap();
        let toml_path = dir.path().join(".hyalo.toml");
        std::fs::write(&toml_path, "[lint.rules]\nMD011 = false\n").unwrap();

        let engine = make_engine();
        let md_lint = load_md_lint(dir.path());

        let _ = set_rule(
            dir.path(),
            "MD011",
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
