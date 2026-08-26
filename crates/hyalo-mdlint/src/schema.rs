//! Schema-driven frontmatter linting (ARCH-2, iter-226).
//!
//! Core of the old `hyalo-cli/src/commands/lint.rs`: the types, violation
//! constants, per-file schema validation (`lint_file` /
//! `lint_file_with_fix`), auto-fix computation, and the
//! `validate_constraint` family used by `set`/`append --validate`.
//! Moved into `hyalo-mdlint` so lint logic is reusable in-process by
//! library consumers (and unit-testable without spawning a CLI process).
//! The CLI keeps flag parsing and output formatting only; it re-exports
//! these items from `commands::lint` for call-site compatibility.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use hyalo_core::filename_template::FilenameTemplate;
use hyalo_core::frontmatter::{read_frontmatter, write_frontmatter, write_frontmatter_within};
use hyalo_core::is_iso8601_date;
use hyalo_core::scanner;
use hyalo_core::schema::{
    self, PropertyConstraint, SchemaConfig, TypeSchema, parse_required_section_entry,
};

use crate::profiles::section_scanner::SectionScanner;

/// Message prefix shared with the CLI's `hints` module so HYALO005
/// violations and parse-error hints agree on wording (kept in sync
/// deliberately — output must stay byte-identical, iter-226 non-goal).
pub const PARSE_ERROR_PREFIX: &str = "could not parse frontmatter";

/// Redundant leading prefix already re-stated by [`PARSE_ERROR_PREFIX`].
const REDUNDANT_ERROR_PREFIXES: &[&str] = &["failed to parse YAML frontmatter: "];

/// Deepest error message in an `anyhow` chain, condensed to its first line
/// and stripped of the redundant prefix (mirror of the CLI's
/// `commands::terse_root_cause`, kept local so this crate has no CLI
/// dependency; behaviour is identical).
pub fn terse_root_cause(err: &anyhow::Error) -> String {
    let root = err.root_cause().to_string();
    let first_line = root.lines().next().unwrap_or(&root).trim();
    let mut msg = first_line;
    loop {
        let mut stripped_any = false;
        for prefix in REDUNDANT_ERROR_PREFIXES {
            if let Some(stripped) = msg.strip_prefix(prefix) {
                msg = stripped;
                stripped_any = true;
            }
        }
        if !stripped_any {
            break;
        }
    }
    msg.trim_end().to_owned()
}

// ---------------------------------------------------------------------------
// Moved from hyalo-cli/src/commands/lint.rs (iter-226, ARCH-2)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Severity of a single lint violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warn,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => f.write_str("error"),
            Self::Warn => f.write_str("warn"),
        }
    }
}

/// Stable identifier for schema warnings that strict mode promotes to errors.
///
/// Strict-mode promotion matches on these constants rather than the
/// user-facing `message`, so reworded messages don't silently disable
/// the promotion logic.
pub const VIOLATION_KIND_MISSING_TYPE: &str = "schema/missing-type";
pub const VIOLATION_KIND_UNDECLARED_PROPERTY: &str = "schema/undeclared-property";
/// An explicit `type:` disagrees with the `[schema.bind]` path binding.
pub const VIOLATION_KIND_BIND_MISMATCH: &str = "schema/bind-mismatch";
/// The `[schema]` block itself failed to parse (unknown key, invalid field
/// combination, etc.) — review round finding 2. Distinct from the other
/// `schema/*` kinds above, which describe a *file* disagreeing with an
/// otherwise-valid schema; this one means schema validation is silently
/// disabled vault-wide until the config is fixed, which is a strictly
/// louder problem and always promoted under `--strict` (see
/// `validate_schema_config`).
pub const VIOLATION_KIND_SCHEMA_MALFORMED: &str = "schema/malformed";

/// A required property that is missing/empty AND has no declared `default`, so
/// `--fix` cannot synthesize a value (mapl BUG-3). Carried on the SCHEMA
/// violation so its group is reported `autofixable: false` instead of `true`.
pub const VIOLATION_KIND_MISSING_REQUIRED_NO_DEFAULT: &str = "schema/missing-required-no-default";

/// Stable rule id for a file whose frontmatter cannot be parsed (invalid YAML,
/// duplicate keys, oversized scalar). Emitted as an error-severity lint
/// violation so a corrupt file becomes a loud CI failure instead of silently
/// vanishing from the scan (RB-3 / df-own-kb B3). Listed in the rule catalog
/// (`lint-rules list`) so its severity is user-configurable via
/// `[lint.rules.HYALO005]`, but it is never silently downgraded by a profile.
pub const RULE_ID_FRONTMATTER_PARSE_ERROR: &str = "HYALO005";

/// Rule id for the broken-link check (HYALO006). Enabled + warn by default,
/// promoted to error under `--strict`. Vault-aware: implemented CLI-side in
/// [`hyalo_mdlint::profiles::link`] where the link graph / case index live.
pub const RULE_ID_BROKEN_LINK: &str = "HYALO006";

/// A single lint violation found in a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub severity: Severity,
    pub message: String,
    /// Stable kind identifier for programmatic dispatch (e.g. strict-mode
    /// promotion in `lint_file`). `None` for ad-hoc violations that don't
    /// need to be matched programmatically.
    #[serde(skip)]
    pub kind: Option<&'static str>,
}

impl Default for Violation {
    fn default() -> Self {
        Self {
            severity: Severity::Warn,
            message: String::new(),
            kind: None,
        }
    }
}

/// Lint results for a single file.
#[derive(Debug, Serialize, Deserialize)]
pub struct FileLintResult {
    pub file: String,
    pub violations: Vec<Violation>,
}

/// A single auto-fix that was (or would be) applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixAction {
    /// Kind of fix: "insert-default", "fix-enum-typo", "normalize-date", "infer-type".
    pub kind: String,
    /// Frontmatter property affected.
    pub property: String,
    /// Old value (if any) — omitted for inserted properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    /// New value applied (or previewed with --dry-run).
    pub new: String,
}

/// Fixes applied to a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFixResult {
    pub file: String,
    pub actions: Vec<FixAction>,
}

/// Summary counts returned to callers (e.g. `hyalo summary`).
#[derive(Debug, Clone, Default)]
pub struct LintCounts {
    pub errors: usize,
    pub warnings: usize,
    /// Number of files with at least one violation.
    pub files_with_issues: usize,
}

/// Whether — and how — the lint `--fix` path should apply auto-fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixMode {
    /// Read-only: do not attempt to fix anything.
    Off,
    /// Apply fixes in memory and write them back to disk.
    Apply,
    /// Compute the fixes that would be applied but don't write any files.
    DryRun,
}

/// Compute lint counts for `hyalo summary` without formatting output.
pub fn lint_counts_only(
    files: &[(std::path::PathBuf, String)],
    schema: &SchemaConfig,
    case_insensitive: bool,
) -> Result<LintCounts> {
    let mut counts = LintCounts::default();
    for (full_path, rel_path) in files {
        let file_result = lint_file(full_path, rel_path, schema, case_insensitive)?;
        for v in &file_result.violations {
            match v.severity {
                Severity::Error => counts.errors += 1,
                Severity::Warn => counts.warnings += 1,
            }
        }
        if !file_result.violations.is_empty() {
            counts.files_with_issues += 1;
        }
    }
    Ok(counts)
}

pub fn lint_counts_from_properties<'a>(
    entries: impl Iterator<Item = (&'a str, &'a IndexMap<String, Value>)>,
    schema: &SchemaConfig,
    case_insensitive: bool,
) -> LintCounts {
    let mut counts = LintCounts::default();
    for (rel_path, properties) in entries {
        let violations = validate_properties(rel_path, properties, schema, case_insensitive);
        for v in &violations {
            match v.severity {
                Severity::Error => counts.errors += 1,
                Severity::Warn => counts.warnings += 1,
            }
        }
        if !violations.is_empty() {
            counts.files_with_issues += 1;
        }
    }
    counts
}

// ---------------------------------------------------------------------------
// Per-file validation
// ---------------------------------------------------------------------------

pub fn lint_file(
    full_path: &Path,
    rel_path: &str,
    schema: &SchemaConfig,
    case_insensitive: bool,
) -> Result<FileLintResult> {
    let (result, _) = lint_file_with_fix(
        full_path,
        rel_path,
        schema,
        FixMode::Off,
        case_insensitive,
        None,
    )?;
    Ok(result)
}

/// Lint a single file, optionally applying auto-fixes.
///
/// `vault_root`, when `Some`, is used to re-check the vault boundary against
/// the *resolved* write destination via
/// [`hyalo_core::frontmatter::write_frontmatter_within`] — see that
/// function's doc comment. Only meaningful when `fix` is `FixMode::Apply`;
/// `FixMode::Off` and `FixMode::DryRun` never write, so `None` is always safe
/// there.
pub fn lint_file_with_fix(
    full_path: &Path,
    rel_path: &str,
    schema: &SchemaConfig,
    fix: FixMode,
    case_insensitive: bool,
    vault_root: Option<&Path>,
) -> Result<(FileLintResult, FileFixResult)> {
    let properties = match read_frontmatter(full_path) {
        Ok(props) => props,
        Err(e) if hyalo_core::frontmatter::is_parse_error(&e) => {
            // Malformed frontmatter — report as a single error violation.
            return Ok((
                FileLintResult {
                    file: rel_path.to_owned(),
                    violations: vec![Violation {
                        severity: Severity::Error,
                        kind: None,
                        // `terse_root_cause` strips the redundant
                        // "failed to parse YAML frontmatter: " prefix so the
                        // shared PARSE_ERROR_PREFIX is not doubled (HYALO005
                        // double-prefix).
                        message: format!("{}: {}", PARSE_ERROR_PREFIX, terse_root_cause(&e)),
                    }],
                },
                FileFixResult {
                    file: rel_path.to_owned(),
                    actions: Vec::new(),
                },
            ));
        }
        Err(e) => return Err(e).context(format!("reading {rel_path}")),
    };

    // Apply fixes in memory (or dry-run) before final validation.
    let (final_props, actions) = if matches!(fix, FixMode::Apply | FixMode::DryRun) {
        let mut mutable = properties.clone();
        let actions = apply_fixes(rel_path, &mut mutable, schema);
        if matches!(fix, FixMode::Apply) && !actions.is_empty() {
            match vault_root {
                Some(root) => write_frontmatter_within(root, full_path, &mutable),
                None => write_frontmatter(full_path, &mutable),
            }
            .with_context(|| format!("writing fixed frontmatter to {rel_path}"))?;
        }
        (mutable, actions)
    } else {
        (properties, Vec::new())
    };

    let mut violations = validate_properties(rel_path, &final_props, schema, case_insensitive);

    // Validate required_sections against the body outline. The effective type is
    // the explicit `type:` else the `[schema.bind]` path binding, so a bound ADR
    // gets its required sections checked even without frontmatter.
    let doc_type: Option<String> = final_props.get("type").and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        _ => None,
    });
    let effective_type: Option<String> = doc_type
        .clone()
        .or_else(|| schema.bound_type_for(rel_path).map(ToOwned::to_owned));
    let effective_schema: TypeSchema = match &effective_type {
        Some(t) => schema.merged_schema_for_type(t),
        None => schema.default_schema().clone(),
    };

    if !effective_schema.required_sections.is_empty()
        && !schema.exempt.is_exempt_ci(rel_path, case_insensitive)
    {
        let section_violations =
            validate_required_sections(full_path, rel_path, &effective_schema.required_sections)?;
        violations.extend(section_violations);
    }

    Ok((
        FileLintResult {
            file: rel_path.to_owned(),
            violations,
        },
        FileFixResult {
            file: rel_path.to_owned(),
            actions,
        },
    ))
}

// ---------------------------------------------------------------------------
// Required-sections body validation
// ---------------------------------------------------------------------------

/// Scan the body of `full_path` and check that each `required_sections` entry
/// appears in document order. Returns a `Violation` for each missing entry.
pub fn validate_required_sections(
    full_path: &Path,
    rel_path: &str,
    required_sections: &[String],
) -> Result<Vec<Violation>> {
    let mut ss = SectionScanner::new();
    scanner::scan_file_multi(full_path, &mut [&mut ss])
        .with_context(|| format!("scanning sections of {rel_path}"))?;
    let sections = ss.into_sections();

    let mut violations = Vec::new();
    let mut cursor = 0usize;
    for (ordinal, entry) in required_sections.iter().enumerate() {
        // parse_required_section_entry was validated at schema-load time, so this should
        // not fail here; treat errors as a lint violation rather than a hard error.
        let (level, text) = match parse_required_section_entry(entry) {
            Ok(t) => t,
            Err(e) => {
                violations.push(Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("invalid required-sections entry {entry:?} in schema: {e}"),
                });
                continue;
            }
        };

        // Walk sections forward from cursor, looking for matching level + trimmed text.
        let found = sections[cursor..].iter().enumerate().find(|(_, s)| {
            s.level == level
                && s.heading
                    .as_deref()
                    .is_some_and(|h| h.trim() == text.as_str())
        });

        if let Some((offset, _)) = found {
            cursor += offset + 1;
        } else {
            let hash_prefix = "#".repeat(level as usize);
            violations.push(Violation {
                severity: Severity::Error,
                kind: None,
                message: format!(
                    "missing required section: expected \"{hash_prefix} {text}\" at or after position {} in the outline",
                    ordinal + 1
                ),
            });
        }
    }

    Ok(violations)
}

// ---------------------------------------------------------------------------
// Auto-fix
// ---------------------------------------------------------------------------

/// Maximum Levenshtein distance accepted for an enum-typo fix.
/// Chosen so that single-letter slips (e.g. "planed" → "planned") are corrected
/// while unrelated values (e.g. "wip" vs. "in-progress") are left alone.
const ENUM_TYPO_MAX_DISTANCE: usize = 2;

/// Compute and apply in-memory auto-fixes to `props`. Returns the list of
/// actions that were taken. Caller is responsible for persisting `props` to
/// disk when appropriate.
pub fn apply_fixes(
    rel_path: &str,
    props: &mut IndexMap<String, Value>,
    schema: &SchemaConfig,
) -> Vec<FixAction> {
    let mut actions: Vec<FixAction> = Vec::new();

    // Step 1: infer `type` from filename-template if missing.
    if !props.contains_key("type")
        && let Some(inferred) = infer_type_from_path(rel_path, schema)
    {
        // Insert `type` at the front of the map so downstream logic picks it up.
        props.shift_insert(0, "type".to_owned(), Value::String(inferred.clone()));
        actions.push(FixAction {
            kind: "infer-type".to_owned(),
            property: "type".to_owned(),
            old: None,
            new: inferred,
        });
    }

    // Determine the effective schema after any type inference.
    let doc_type: Option<String> = props.get("type").and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        _ => None,
    });
    let effective_schema: TypeSchema = match &doc_type {
        Some(t) => schema.merged_schema_for_type(t),
        None => schema.default_schema().clone(),
    };

    // Step 2: insert defaults for missing properties.
    // Iterate in the schema's `required` order first, then any remaining defaults,
    // so the resulting frontmatter is ordered deterministically.
    let mut inserted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for req in &effective_schema.required {
        if !props.contains_key(req.as_str())
            && let Some(raw) = effective_schema.defaults.get(req.as_str())
        {
            let value = schema::expand_default(raw);
            props.insert(req.clone(), Value::String(value.clone()));
            inserted.insert(req.clone());
            actions.push(FixAction {
                kind: "insert-default".to_owned(),
                property: req.clone(),
                old: None,
                new: value,
            });
        }
    }
    // Also honour defaults for properties not listed in `required`.
    for (name, raw) in &effective_schema.defaults {
        if inserted.contains(name) || props.contains_key(name.as_str()) {
            continue;
        }
        let value = schema::expand_default(raw);
        props.insert(name.clone(), Value::String(value.clone()));
        actions.push(FixAction {
            kind: "insert-default".to_owned(),
            property: name.clone(),
            old: None,
            new: value,
        });
    }

    // Step 3: per-property fixes (enum typos, date normalization).
    let prop_names: Vec<String> = props.keys().cloned().collect();
    for name in prop_names {
        let Some(constraint) = effective_schema.properties.get(name.as_str()) else {
            continue;
        };
        // Snapshot the current value to avoid double-borrowing `props`.
        let Some(current) = props.get(name.as_str()).cloned() else {
            continue;
        };
        match constraint {
            PropertyConstraint::Enum { values } => {
                let Value::String(s) = &current else { continue };
                if values.iter().any(|v| v == s) {
                    continue;
                }
                if let Some((suggestion, dist)) = values
                    .iter()
                    .map(|v| (v, strsim::levenshtein(s, v.as_str())))
                    .min_by_key(|(_, d)| *d)
                    && dist <= ENUM_TYPO_MAX_DISTANCE
                {
                    let old = s.clone();
                    let new_value = suggestion.clone();
                    props.insert(name.clone(), Value::String(new_value.clone()));
                    actions.push(FixAction {
                        kind: "fix-enum-typo".to_owned(),
                        property: name.clone(),
                        old: Some(old),
                        new: new_value,
                    });
                }
            }
            PropertyConstraint::Date => {
                let Value::String(s) = &current else { continue };
                if is_iso8601_date(s) {
                    continue;
                }
                if let Some(normalized) = normalize_date(s) {
                    let old = s.clone();
                    props.insert(name.clone(), Value::String(normalized.clone()));
                    actions.push(FixAction {
                        kind: "normalize-date".to_owned(),
                        property: name.clone(),
                        old: Some(old),
                        new: normalized,
                    });
                }
            }
            _ => {}
        }
    }

    // Step 4: split comma-joined tags (e.g. ["cli,ux"] -> ["cli", "ux"]).
    if let Some(Value::Array(items)) = props.get("tags") {
        let needs_fix = items
            .iter()
            .any(|v| matches!(v, Value::String(s) if s.contains(',')));
        if needs_fix {
            let old_tags: Vec<Value> = items.clone();
            let new_tags: Vec<Value> = old_tags
                .iter()
                .flat_map(|v| match v {
                    Value::String(s) if s.contains(',') => s
                        .split(',')
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                        .map(|p| Value::String(p.to_owned()))
                        .collect::<Vec<_>>(),
                    other => vec![other.clone()],
                })
                .collect();
            let old_str = old_tags
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let new_str = new_tags
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            props.insert("tags".to_owned(), Value::Array(new_tags));
            actions.push(FixAction {
                kind: "split-comma-tags".to_owned(),
                property: "tags".to_owned(),
                old: Some(old_str),
                new: new_str,
            });
        }
    }

    actions
}

/// Returns `true` when `rel_path` is the bundle-root `index.md` (the vault-root
/// index of an OKF bundle). Matches only the top-level `index.md`, not
/// `index.md` files in subdirectories. Paths are normalized to forward slashes
/// so the check is cross-platform.
pub fn is_bundle_root_index(rel_path: &str) -> bool {
    let normalized = rel_path.replace('\\', "/");
    let trimmed = normalized.strip_prefix("./").unwrap_or(&normalized);
    trimmed == "index.md"
}

/// Try to infer a `type` value for a file at `rel_path` for `lint --fix` type
/// insertion. First matches against every `[schema.types.*].filename-template`
/// (unique match required); if that is ambiguous or empty, falls back to the
/// `[schema.bind]` path bindings (first-match-wins). Returns `None` when neither
/// yields a single type.
pub fn infer_type_from_path(rel_path: &str, schema: &SchemaConfig) -> Option<String> {
    let mut matches: Vec<String> = Vec::new();
    for (type_name, ts) in &schema.types {
        let Some(template_str) = &ts.filename_template else {
            continue;
        };
        let Ok(template) = FilenameTemplate::parse(template_str) else {
            continue;
        };
        if template.matches(rel_path) {
            matches.push(type_name.clone());
        }
    }
    if matches.len() == 1 {
        return matches.pop();
    }
    // Filename-template inference was empty or ambiguous — consult path bindings.
    schema.bound_type_for(rel_path).map(ToOwned::to_owned)
}

/// Normalize a loose date string to `YYYY-MM-DD`.
///
/// Accepts inputs of the form `Y-M-D` where `Y`, `M`, `D` are decimal digit
/// runs and month/day are in the valid calendar ranges. Returns `None` for
/// inputs that are ambiguous (e.g. natural-language dates, non-ISO separators,
/// or out-of-range values); those are reported as violations instead.
pub fn normalize_date(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y = parts[0];
    let m = parts[1];
    let d = parts[2];
    if y.len() != 4 || !y.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if m.is_empty() || m.len() > 2 || !m.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if d.is_empty() || d.len() > 2 || !d.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let yi: i32 = y.parse().ok()?;
    let mi: u32 = m.parse().ok()?;
    let di: u32 = d.parse().ok()?;
    if !(1..=12).contains(&mi) {
        return None;
    }
    let max_day = match mi {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (yi % 4 == 0 && yi % 100 != 0) || (yi % 400 == 0);
            if leap { 29 } else { 28 }
        }
        _ => return None,
    };
    if !(1..=max_day).contains(&di) {
        return None;
    }
    Some(format!("{y}-{mi:02}-{di:02}"))
}

/// Core property validation logic.
///
/// Separated so it can be used both by the disk-reading path (`lint_file`) and
/// the index-based path (`lint_counts_from_properties`).
pub fn validate_properties(
    rel_path: &str,
    properties: &IndexMap<String, Value>,
    schema: &SchemaConfig,
    case_insensitive: bool,
) -> Vec<Violation> {
    let mut violations: Vec<Violation> = Vec::new();

    // Reserved / exempt files (e.g. OKF `index.md`, `log.md`) are bound to no
    // schema: they skip the missing-`type` warning, required-property checks,
    // undeclared-property warnings, and per-property constraint validation.
    // `case_insensitive` mirrors the vault's resolved `[links]
    // case_insensitive` mode so `INDEX.md` is exempted the same way `hyalo
    // okf index` treats it on case-folding filesystems (macOS/Windows).
    if schema.exempt.is_exempt_ci(rel_path, case_insensitive) {
        return violations;
    }

    // Determine the document type.
    let type_value = properties.get("type");
    let doc_type: Option<String> = type_value.and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        _ => None,
    });

    // If `type` is present but not a string, report an error. A non-string `type`
    // still satisfies a bare `required = ["type"]` check, so without this error
    // invalid type values would slip through silently.
    if let Some(v) = type_value
        && doc_type.is_none()
    {
        violations.push(Violation {
            severity: Severity::Error,
            kind: None,
            message: format!("property \"type\" expected string, got {v}"),
        });
    }

    // Path-bound schema: when no explicit `type:` is present but the file's path
    // matches a `[schema.bind]` glob, the binding assigns the effective type.
    // Explicit frontmatter always wins over a binding.
    let bound_type: Option<&str> = schema.bound_type_for(rel_path);

    // Warn when no `type` property is present *and* no binding covers this file.
    // A bound file is fully typed by its path, so it neither warns nor validates
    // against the default-only schema.
    if type_value.is_none() && bound_type.is_none() && !schema.is_empty() {
        violations.push(Violation {
            severity: Severity::Warn,
            kind: Some(VIOLATION_KIND_MISSING_TYPE),
            message: "no 'type' property — validating against default schema only".to_owned(),
        });
    }

    // Frontmatter ↔ binding mismatch: an explicit `type:` that disagrees with
    // the path binding is a (warn-level) smell — usually a file in the wrong
    // directory, or a stale type. Explicit frontmatter still wins.
    if let (Some(explicit), Some(bound)) = (doc_type.as_deref(), bound_type)
        && explicit != bound
    {
        violations.push(Violation {
            severity: Severity::Warn,
            kind: Some(VIOLATION_KIND_BIND_MISMATCH),
            message: format!(
                "frontmatter type '{explicit}' disagrees with the path binding for this location (bound to '{bound}')"
            ),
        });
    }

    // Determine the effective schema for this file: explicit type, else the
    // path-bound type, else the default-only schema.
    let effective_type: Option<&str> = doc_type.as_deref().or(bound_type);
    let effective_schema: TypeSchema = match effective_type {
        Some(t) => schema.merged_schema_for_type(t),
        None => schema.default_schema().clone(),
    };

    // Check required properties.
    //
    // A required property must be both present AND carry a meaningful value.
    // Null (`tags: ~`) and an empty array (`tags: []`) are treated as
    // semantically equivalent to absent — they convey no information and a
    // required key whose value is "nothing here" should fail the same gate as
    // a missing key. Atomic-typed required properties only need to be present
    // (an empty string or zero satisfies them); checking those is a separate
    // constraint and not handled here.
    let type_hint = doc_type
        .as_deref()
        .map(|t| format!(" (type: {t})"))
        .unwrap_or_default();
    // Bind = typing: when a file's type is assigned by a `[schema.bind]` path
    // binding (no explicit `type:` frontmatter), the binding itself satisfies a
    // `required = ["type"]` gate — a spec-valid frontmatter-less bound file
    // (SKILL.md, ADR, CHANGELOG.md) must lint clean. Skip only the `type`
    // requirement in that case; every other required property is still checked.
    let type_satisfied_by_bind = doc_type.is_none() && bound_type.is_some();
    for req in &effective_schema.required {
        if type_satisfied_by_bind && req == "type" {
            continue;
        }
        // `--fix` can synthesize a missing/empty required property only when the
        // schema declares a `default` for it; otherwise no value can be invented
        // (mapl BUG-3), so the resulting violation is tagged not-autofixable.
        let missing_kind = if effective_schema.defaults.contains_key(req.as_str()) {
            None
        } else {
            Some(VIOLATION_KIND_MISSING_REQUIRED_NO_DEFAULT)
        };
        match properties.get(req.as_str()) {
            None => {
                violations.push(Violation {
                    severity: Severity::Error,
                    kind: missing_kind,
                    message: format!("missing required property \"{req}\"{type_hint}"),
                });
            }
            Some(v) if v.is_null() || v.as_array().is_some_and(Vec::is_empty) => {
                violations.push(Violation {
                    severity: Severity::Error,
                    kind: missing_kind,
                    message: format!("required property \"{req}\" must not be empty{type_hint}"),
                });
            }
            _ => {}
        }
    }

    // Build a per-call regex cache so the same pattern isn't recompiled across
    // properties (this matters in `hyalo summary`, which runs lint over the full
    // index).
    let mut regex_cache: HashMap<String, Result<Regex, String>> = HashMap::new();

    // Type-specific property constraint validation.
    for (name, value) in properties {
        // `tags` is validated against its declared constraint if present, but it
        // is never reported as an undeclared property: presence of a `tags` key
        // without a schema entry for it is intentional, not a misconfiguration.
        if name == "tags" {
            if let Some(constraint) = effective_schema.properties.get(name.as_str()) {
                violations.extend(validate_constraint(
                    name,
                    value,
                    constraint,
                    &mut regex_cache,
                ));
            }
            // Check for comma-joined tags (e.g. "cli,ux" instead of ["cli", "ux"]).
            if let Value::Array(items) = value {
                for item in items {
                    if let Value::String(tag) = item
                        && tag.contains(',')
                    {
                        violations.push(Violation {
                            severity: Severity::Warn,
                            kind: None,
                            message: format!(
                                "tag \"{tag}\" appears to be comma-joined -- should be separate list items"
                            ),
                        });
                    }
                }
            }
            continue;
        }
        // Never warn about "type" (type discriminator) or properties listed in `required`
        // — they're implicitly accepted even if not in the `properties` map.
        //
        // Additionally, the OKF bundle-root `index.md` may carry a lone
        // `okf_version` key (spec §2). This allowance is scoped to the root
        // index only (`rel_path == "index.md"`), so an `okf_version` key in an
        // arbitrary file is still flagged as undeclared.
        let implicitly_accepted = name == "type"
            || effective_schema.required.contains(name)
            || (name == "okf_version" && is_bundle_root_index(rel_path));

        if let Some(constraint) = effective_schema.properties.get(name.as_str()) {
            violations.extend(validate_constraint(
                name,
                value,
                constraint,
                &mut regex_cache,
            ));
        } else if !effective_schema.properties.is_empty() && !implicitly_accepted {
            // Property not declared in schema — warn only when the schema declares
            // some properties. Schemas that only specify `required` remain
            // intentionally permissive about extra fields.
            violations.push(Violation {
                severity: Severity::Warn,
                kind: Some(VIOLATION_KIND_UNDECLARED_PROPERTY),
                message: format!("property \"{name}\" is not declared in schema"),
            });
        }
    }

    violations
}

// ---------------------------------------------------------------------------
// Constraint validators
// ---------------------------------------------------------------------------

#[allow(clippy::implicit_hasher)] // moved verbatim from hyalo-cli (ARCH-2, iter-226)
pub fn validate_constraint(
    name: &str,
    value: &Value,
    constraint: &PropertyConstraint,
    regex_cache: &mut HashMap<String, Result<Regex, String>>,
) -> Vec<Violation> {
    match constraint {
        PropertyConstraint::String {
            pattern,
            min_length,
            max_length,
        } => {
            let Some(s) = value_as_str(value) else {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected string, got {value}"),
                }];
            };
            // Length bounds are measured in Unicode scalar values (chars), so a
            // 1024-char cap counts characters, not bytes — matching how humans
            // and the Agent Skills spec reason about a description's length.
            // Only pay for the O(n) char count when a bound is actually set —
            // most `string` properties have neither.
            if min_length.is_some() || max_length.is_some() {
                let len = s.chars().count();
                if let Some(min) = min_length
                    && len < *min
                {
                    return vec![Violation {
                        severity: Severity::Error,
                        kind: None,
                        message: format!(
                            "property \"{name}\" is {len} characters; minimum is {min}"
                        ),
                    }];
                }
                if let Some(max) = max_length
                    && len > *max
                {
                    return vec![Violation {
                        severity: Severity::Error,
                        kind: None,
                        message: format!(
                            "property \"{name}\" is {len} characters; maximum is {max}"
                        ),
                    }];
                }
            }
            if let Some(pat) = pattern {
                // Compile (or look up) the regex once per pattern per call.
                let entry = regex_cache
                    .entry(pat.clone())
                    .or_insert_with(|| Regex::new(pat).map_err(|e| e.to_string()));
                match entry {
                    Ok(re) => {
                        if !re.is_match(s) {
                            return vec![Violation {
                                severity: Severity::Error,
                                kind: None,
                                message: format!(
                                    "property \"{name}\" value {s:?} does not match pattern {pat:?}"
                                ),
                            }];
                        }
                    }
                    Err(e) => {
                        return vec![Violation {
                            severity: Severity::Error,
                            kind: None,
                            message: format!("property \"{name}\": invalid pattern {pat:?}: {e}"),
                        }];
                    }
                }
            }
            vec![]
        }
        PropertyConstraint::Date => {
            let Some(s) = value_as_str(value) else {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected date (YYYY-MM-DD), got {value}"),
                }];
            };
            if !is_iso8601_date(s) {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected date (YYYY-MM-DD), got \"{s}\""),
                }];
            }
            vec![]
        }
        PropertyConstraint::DateTime => {
            let Some(s) = value_as_str(value) else {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!(
                        "property \"{name}\" expected datetime (YYYY-MM-DDThh:mm:ss), got {value}"
                    ),
                }];
            };
            if !hyalo_core::is_iso8601_datetime(s) {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!(
                        "property \"{name}\" expected datetime (YYYY-MM-DDThh:mm:ss), got \"{s}\""
                    ),
                }];
            }
            vec![]
        }
        PropertyConstraint::DateTimeTz => {
            let Some(s) = value_as_str(value) else {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!(
                        "property \"{name}\" expected tz-aware datetime (YYYY-MM-DDThh:mm:ss with Z or ±hh:mm offset), got {value}"
                    ),
                }];
            };
            if !hyalo_core::is_iso8601_datetime_tz(s) {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!(
                        "property \"{name}\" expected tz-aware datetime (YYYY-MM-DDThh:mm:ss with Z or ±hh:mm offset), got \"{s}\""
                    ),
                }];
            }
            vec![]
        }
        PropertyConstraint::Number { minimum, maximum } => {
            let Value::Number(n) = value else {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected number, got {value}"),
                }];
            };
            // F3-3: enforce `minimum`/`maximum` when configured. `as_f64`
            // only returns `None` for a `Number` outside f64's range (not a
            // realistic frontmatter value); treat that as passing rather
            // than panic-adjacent unwrap, since it isn't a bound violation.
            let Some(f) = n.as_f64() else {
                return vec![];
            };
            if let Some(min) = minimum
                && f < *min
            {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" is {f}; minimum is {min}"),
                }];
            }
            if let Some(max) = maximum
                && f > *max
            {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" is {f}; maximum is {max}"),
                }];
            }
            vec![]
        }
        PropertyConstraint::Boolean => {
            if !matches!(value, Value::Bool(_)) {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected boolean, got {value}"),
                }];
            }
            vec![]
        }
        PropertyConstraint::List => {
            if !matches!(value, Value::Array(_)) {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected list, got {value}"),
                }];
            }
            vec![]
        }
        PropertyConstraint::Enum { values } => {
            let Some(s) = value_as_str(value) else {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!(
                        "property \"{name}\" expected one of [{}], got {value}",
                        values.join(", ")
                    ),
                }];
            };
            if values.contains(&s.to_owned()) {
                return vec![];
            }
            // Find nearest suggestion via Levenshtein.
            let suggestion = values
                .iter()
                .min_by_key(|v| strsim::levenshtein(s, v.as_str()))
                .map(|v| format!(" (did you mean \"{v}\"?)"))
                .unwrap_or_default();
            vec![Violation {
                severity: Severity::Error,
                kind: None,
                message: format!(
                    "property \"{name}\" value \"{s}\" not in [{}]{suggestion}",
                    values.join(", ")
                ),
            }]
        }
        PropertyConstraint::StringList { item_pattern } => {
            let Value::Array(items) = value else {
                return vec![Violation {
                    severity: Severity::Error,
                    kind: None,
                    message: format!("property \"{name}\" expected string-list, got {value}"),
                }];
            };
            let Some(pat) = item_pattern else {
                // No per-item pattern — collect a violation for every non-string item.
                return items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| !matches!(item, Value::String(_)))
                    .map(|(i, item)| Violation {
                        severity: Severity::Error,
                        kind: None,
                        message: format!(
                            "property \"{name}\" item {i}: expected string, got {item}"
                        ),
                    })
                    .collect();
            };
            // Compile (or look up) the regex once per pattern per call.
            let entry = regex_cache
                .entry(pat.clone())
                .or_insert_with(|| Regex::new(pat).map_err(|e| e.to_string()));
            let re = match entry {
                Err(e) => {
                    return vec![Violation {
                        severity: Severity::Error,
                        kind: None,
                        message: format!("property \"{name}\": invalid item_pattern {pat:?}: {e}"),
                    }];
                }
                Ok(re) => re,
            };
            // Collect a violation for every item that is not a string or fails the pattern.
            items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    let Value::String(s) = item else {
                        return Some(Violation {
                            severity: Severity::Error,
                            kind: None,
                            message: format!(
                                "property \"{name}\" item {i}: expected string, got {item}"
                            ),
                        });
                    };
                    if re.is_match(s) {
                        None
                    } else {
                        Some(Violation {
                            severity: Severity::Error,
                            kind: None,
                            message: format!(
                                "property \"{name}\" item {i}: value {s:?} does not match pattern {pat:?}"
                            ),
                        })
                    }
                })
                .collect()
        }
    }
}

/// Extract a `&str` from a `Value::String`, or `None` for other variants.
fn value_as_str(v: &Value) -> Option<&str> {
    if let Value::String(s) = v {
        Some(s.as_str())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Public validation helper (used by set/append --validate)
// ---------------------------------------------------------------------------

/// Validate a single property value against a constraint without a shared regex cache.
///
/// Returns `Some(error_message)` when the value violates the constraint, `None`
/// when it is valid. Regex patterns are compiled fresh for each call — use the
/// private [`validate_constraint`] with a shared cache in hot paths.
pub fn validate_constraint_simple(
    name: &str,
    value: &Value,
    constraint: &PropertyConstraint,
) -> Option<String> {
    let mut cache = HashMap::new();
    validate_constraint(name, value, constraint, &mut cache)
        .into_iter()
        .next()
        .map(|v| v.message)
}

// ---------------------------------------------------------------------------
// Tests — in-process API (ARCH-2, iter-226): lint behaviour unit-tested
// without spawning a CLI process.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hyalo_core::schema::{PropertyConstraint, TypeSchema};

    fn schema_with_required_date() -> SchemaConfig {
        let mut schema = SchemaConfig::default();
        schema.default.required = vec!["date".to_string()];
        schema
            .default
            .properties
            .insert("date".to_string(), PropertyConstraint::Date);
        schema
    }

    #[test]
    fn validate_properties_flags_missing_required_and_bad_date() {
        let schema = schema_with_required_date();
        let mut props: IndexMap<String, Value> = IndexMap::new();
        props.insert("date".to_string(), Value::String("2026-13-40".to_string()));
        let violations = validate_properties("note.md", &props, &schema, false);
        assert!(
            violations.iter().any(|v| v.message.contains("2026-13-40")),
            "invalid date should be flagged: {violations:?}"
        );
    }

    #[test]
    fn lint_file_reports_required_property_violation() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("note.md");
        std::fs::write(&file, "---\ntitle: T\n---\n\nBody.\n").unwrap();
        let schema = schema_with_required_date();
        let result = lint_file(&file, "note.md", &schema, false).unwrap();
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.message.contains("missing required property \"date\"")),
            "missing required `date` should be reported: {:?}",
            result.violations
        );
    }

    #[test]
    fn validate_constraint_simple_date_rejects_natural_language() {
        assert!(
            validate_constraint_simple(
                "date",
                &Value::String("April 13".to_string()),
                &PropertyConstraint::Date
            )
            .is_some()
        );
        assert!(
            validate_constraint_simple(
                "date",
                &Value::String("2026-04-13".to_string()),
                &PropertyConstraint::Date
            )
            .is_none()
        );
    }

    #[test]
    fn normalize_date_pads_month_and_day() {
        assert_eq!(normalize_date("2026-4-7").as_deref(), Some("2026-04-07"));
        assert_eq!(normalize_date("2026-02-30"), None);
    }

    #[test]
    fn terse_root_cause_strips_redundant_prefix() {
        let err = anyhow::anyhow!("failed to parse YAML frontmatter: real cause");
        assert_eq!(terse_root_cause(&err), "real cause");
    }

    #[test]
    fn type_schema_required_field_exists() {
        // Guard that the hyalo-core TypeSchema surface this module relies on
        // stays available to library consumers.
        let ts = TypeSchema::default();
        assert!(ts.required.is_empty());
    }
}
