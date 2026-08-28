//! `hyalo lint` — validate frontmatter properties against the `.hyalo.toml` schema.
//!
//! Reads each file's frontmatter, applies the type-specific schema (or the
//! default schema if `type` is absent), and reports violations at two severity
//! levels:
//!
//!   - **error**  — schema violation (missing required field, wrong value type,
//!     invalid enum value, failed pattern match, empty value on a list-typed
//!     required property)
//!   - **warn**   — soft issue (no `type` property, property not declared in
//!     schema)
//!
//! Exit code: 0 = clean, 1 = errors found, 2 = internal error.
// ARCH-2 (iter-226): the schema-lint core (types, per-file validation,
// auto-fix, constraint helpers) lives in `hyalo_mdlint::schema`. Re-exported
// here so existing `commands::lint::` call sites keep working.
pub use hyalo_mdlint::schema::{
    FileFixResult, FileLintResult, FixAction, FixMode, LintCounts, RULE_ID_BROKEN_LINK,
    RULE_ID_FRONTMATTER_PARSE_ERROR, Severity, VIOLATION_KIND_BIND_MISMATCH,
    VIOLATION_KIND_MISSING_REQUIRED_NO_DEFAULT, VIOLATION_KIND_MISSING_TYPE,
    VIOLATION_KIND_SCHEMA_MALFORMED, VIOLATION_KIND_UNDECLARED_PROPERTY, Violation, apply_fixes,
    infer_type_from_path, is_bundle_root_index, lint_counts_from_properties, lint_counts_only,
    lint_file, lint_file_with_fix, normalize_date, validate_constraint, validate_constraint_simple,
    validate_properties, validate_required_sections,
};

// ---------------------------------------------------------------------------
// Submodules (iteration 247)
// ---------------------------------------------------------------------------
//
// `commands/lint.rs` had grown to 4,005 lines -- a review hotspot flagged by
// the 2026-08-27 deep review. It is split by pass: the config-level checks, the
// output shapes, the vault-level pass, the per-file pass, the body-fix
// application, and the dispatch arm. This file keeps only the shared imports
// and re-exports, so `commands::lint::` call sites are unaffected.

mod config_checks;
mod engine;
mod file;
mod fix;
mod run;
mod types;

// Glob re-exports rather than named lists: every item below kept the exact
// visibility it had inside the pre-split module, and a glob re-export cannot
// widen it (a `pub(super)` item stays `pub(super)`). That is what makes the
// split invisible to callers -- and it is also how the submodules see each
// other, since each one imports this parent.
pub use config_checks::*;
pub use engine::*;
use file::lint_one_file_extended;
use fix::{apply_body_fixes, find_body_line_offset, find_body_start, schema_has_completed_status};
pub use types::*;

pub(crate) use run::run;

#[cfg(test)]
mod tests;
