//! The extended (frontmatter + body) lint output shapes.
//!
//! Split out of the single 4,005-line `commands/lint.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `commands::lint::...` paths and behaviour are
//! unchanged.

use hyalo_mdlint::schema::{FileFixResult, FixMode};
use std::path::Path;

// ---------------------------------------------------------------------------
// Text formatter
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Extended body-lint types (new output shape per plan)
// ---------------------------------------------------------------------------

/// A group of violations for one rule within one file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RuleGroup {
    pub rule: String,
    pub count: usize,
    pub shown: usize,
    pub truncated: bool,
    pub severity: String,
    pub autofixable: bool,
    pub violations: Vec<BodyViolation>,
}

/// A single body violation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BodyViolation {
    pub line: usize,
    pub column: usize,
    /// Per-violation severity (`"error"` / `"warn"`). Carried alongside the
    /// group severity because a folded group (notably `SCHEMA`) can mix the
    /// two, and the text renderer must label each line with its own severity
    /// so the display agrees with the `errors`/`warnings` counts (BUG-17).
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<serde_json::Value>,
}

/// Extended lint output for one file (read-only shape).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ExtFileLintResult {
    pub file: String,
    /// Frontmatter `type:` discriminator, if the file declared one. Used by
    /// the hint layer to surface `hyalo types show <T>` for SCHEMA failures.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub doc_type: Option<String>,
    /// Frontmatter + body violations grouped by rule.
    pub rule_groups: Vec<RuleGroup>,
}

/// One entry in `fixed_groups`: a rule + count of violations that were fixed.
/// Includes `violations` so text renderers can show line/message details.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FixedGroup {
    pub rule: String,
    pub count: usize,
    /// Violations that were fixed (same shape as `RuleGroup.violations`).
    pub violations: Vec<BodyViolation>,
}

/// One entry in `conflicts`: a rule whose fix was skipped due to range overlap.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConflictEntry {
    pub rule: String,
    pub reason: String,
}

/// Extended lint output for one file in fix-mode.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ExtFileLintFixResult {
    pub file: String,
    /// Frontmatter `type:` discriminator, if the file declared one. Mirrors
    /// [`ExtFileLintResult::doc_type`] so the iter-143 SCHEMA-→-`types show`
    /// hint also fires in `--fix` / `--fix --dry-run` output.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub doc_type: Option<String>,
    /// Rules that had fixes applied (or would be in DryRun).
    pub fixed_groups: Vec<FixedGroup>,
    /// Rules with violations that remain after fixing.
    pub remaining_groups: Vec<RuleGroup>,
    /// Rules whose fixes were skipped due to conflicts.
    pub conflicts: Vec<ConflictEntry>,
}

/// Full extended lint output (read-only mode).
///
/// `Default` backs the empty-result shape `run.rs` emits when
/// `--files-from` resolves to zero files — serializing `Self::default()`
/// (with only `dry_run` overridden) keeps that shape from drifting out of
/// sync with the real field set as fields are added, renamed or removed
/// here, mirroring [`ExtLintFixOutput`]'s empty-result shape.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ExtLintOutput {
    pub files: Vec<ExtFileLintResult>,
    /// Total number of violations found across all files.
    ///
    /// iter-216 D-1..D-5: named `violations`, not `total`. `results.total` is
    /// reserved for "the number of items the command considered" (the
    /// denominator); this is a count of findings, and the envelope's own
    /// `total` on the very same document is the *file* count, so one name
    /// used to carry two quantities in one payload.
    pub violations: usize,
    pub rules_fired: usize,
    pub files_with_violations: usize,
    /// Total number of files that were examined (including clean files).
    pub files_checked: usize,
    pub files_truncated: bool,
    /// Number of error-severity violations.
    pub errors: usize,
    /// Number of warn-severity violations.
    pub warnings: usize,
    /// Files dropped from this run by `[lint] ignore` (UX-1, dogfood pre3).
    /// Always present (not skipped when zero) — consistent with the other
    /// count fields on this struct.
    pub files_ignored: usize,
    /// `true` when `--fix --dry-run` previewed fixes without writing them.
    ///
    /// iter-216 D-4: always present, never skipped when `false`. Top-level
    /// `results` keys are always present; `set`/`remove`/`append`/`mv` already
    /// emit `dry_run: false`, and a script switching between those and `lint`
    /// used to get `null` from one and `false` from the other.
    pub dry_run: bool,
    /// Frontmatter fix actions applied (or previewed) per file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<FileFixResult>,
}

/// Full extended lint output in fix-mode.
///
/// `Default` backs the empty-result shape `run.rs` emits when
/// `--files-from` resolves to zero files (review finding #5) — serializing
/// `Self::default()` (with only `dry_run` overridden) keeps that shape from
/// drifting out of sync with the real field set as fields are added or
/// removed here. `dry_run` is always serialized (iter-216 D-4), so the
/// empty shape carries it as a plain `false`/`true`, same as the non-empty
/// path.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ExtLintFixOutput {
    pub files: Vec<ExtFileLintFixResult>,
    pub total_fixed: usize,
    pub total_remaining: usize,
    pub total_conflicts: usize,
    pub rules_fired: usize,
    pub files_with_violations: usize,
    pub files_checked: usize,
    pub files_truncated: bool,
    /// Error-severity violations left unfixed after this run.
    ///
    /// Named `remaining_errors`/`remaining_warnings` (not `errors`/
    /// `warnings`, iter-218 NEW-6b) because those keys mean something
    /// different on [`ExtLintOutput`]: whole-run severity counts, not a
    /// remaining-after-fix count. The two commands used to share a key name
    /// for two different quantities — a script reading `.errors` off both
    /// `lint` and `lint --fix` JSON silently got answers to different
    /// questions.
    pub remaining_errors: usize,
    pub remaining_warnings: usize,
    /// `true` when `--dry-run` previewed fixes without writing them.
    /// Always present (iter-216 D-4) — see [`ExtLintOutput::dry_run`].
    pub dry_run: bool,
}

/// Options for the extended lint run.
pub struct ExtLintOptions<'a> {
    pub fix: FixMode,
    pub detailed: bool,
    pub rule_filter: Option<&'a str>,
    pub rule_prefix: Option<&'a str>,
    pub max_per_rule: usize,
    pub max_files: usize,
    pub fix_rules: &'a [String],
    /// Snapshot index for patching after fixes.
    pub snapshot_index: &'a mut Option<hyalo_core::index::SnapshotIndex>,
    pub index_path: Option<&'a Path>,
    pub vault_dir: &'a Path,
    /// When `true`, promote "no 'type' property" and "undeclared property in
    /// frontmatter" from `Severity::Warn` to `Severity::Error`.
    pub strict: bool,
    /// When `true`, run the OKF conformance profile's advisory (warn-level)
    /// rules in addition to the schema pass. Set by `hyalo lint --profile okf`.
    pub okf_profile: bool,
    /// When `true`, run the MADR conformance profile's advisory (warn-level)
    /// rules in addition to the schema pass. Set by `hyalo lint --profile madr`.
    pub madr_profile: bool,
    /// When `true`, run the Agent Skills conformance profile's rules
    /// (mostly warn-level; `SKILL-RESERVED-NAME` defaults to error) in
    /// addition to the schema pass. Set by `hyalo lint --profile skills`.
    pub skills_profile: bool,
    /// When `true`, run the Keep a Changelog conformance profile's rules
    /// (mostly error-level grammar/ordering; empty-section and link-ref
    /// cross-check default to warn) in addition to the schema pass. Set by
    /// `hyalo lint --profile changelog`.
    pub changelog_profile: bool,
    /// Resolved `[links] case_insensitive` mode for `vault_dir` (see
    /// [`hyalo_core::mode_enabled`]). Used so `[schema] exempt`
    /// globs (e.g. `**/index.md`) fold case the same way `hyalo okf index`
    /// does on case-insensitive filesystems.
    pub case_insensitive: bool,
    /// Vault-wide context for the HYALO006 broken-link rule, built ONCE per
    /// invocation in the dispatch arm (link graph / case index) and shared by
    /// reference across the rayon workers. `None` when HYALO006 is disabled or
    /// filtered out, so no graph is built and the rule never runs.
    pub link_lint_ctx: Option<hyalo_mdlint::profiles::link::LinkLintContext>,
    /// Files dropped from this run by `[lint] ignore`, regardless of scope
    /// (bare sweep, `--glob`, or `--file`). UX-1 (dogfood pre3): surfaced in
    /// the bare-sweep summary line so a full-vault run stops silently hiding
    /// how much of the vault it skipped (`68 files checked (318 ignored by
    /// [lint] ignore)`) — the named-file/glob-all-ignored cases already get a
    /// dedicated stderr notice from the caller.
    pub files_ignored: usize,
}
