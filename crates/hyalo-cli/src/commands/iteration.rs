//! `--iteration <ID>` natural-key resolution (iter-235).
//!
//! Agents address sequence-keyed documents (iteration plans, etc.) by their
//! natural key — the bare number encoded in the type schema's
//! `filename_template` `{n}` slot — instead of inventing a glob every time.
//! This module turns an [`IterationId`] into the globs that resolve it,
//! shared by `find --iteration` (returns every match; it's a filter) and
//! `set --iteration` (errors unless exactly one match; it's a mutation).

use std::fmt::Write as _;

use hyalo_core::filename_template::FilenameTemplate;
use hyalo_core::iteration_id::IterationId;
use hyalo_core::schema::SchemaConfig;

use crate::output::{CommandOutcome, Format};

/// Outcome of resolving an `--iteration` ID against the configured type
/// schemas. On success, `globs` holds one glob per type whose template has an
/// `{n}` slot; the caller filters/collects files with them. On failure, an
/// `Outcome` carries a formatted error for the agent.
#[derive(Debug)]
pub(crate) enum IterationGlobs {
    /// One or more globs; union them to find the iteration's files.
    Globs(Vec<String>),
    /// No configured type carries an `{n}` filename template, so `--iteration`
    /// is meaningless until one is added.
    Outcome(CommandOutcome),
}

/// Resolve `--iteration <ID>` to glob patterns from the schema's type
/// `filename_template`s.
///
/// Iterates every type with a `filename_template` containing an `{n}`
/// placeholder and substitutes the ID (verbatim) into that slot, with `*`
/// for the other placeholders. A vault with a single `iteration` type whose
/// template is `iterations/iteration-{n}-{slug}.md` resolves `--iteration
/// 206` to `["iterations/iteration-206-*.md"]`.
///
/// When no type has a template with an `{n}` slot, returns an error outcome
/// naming the types that *do* have a template (so the user can see what to
/// adjust) — the `--iteration` flag is meaningless without a natural key to
/// substitute into.
pub(crate) fn resolve_iteration_globs(
    schema: &SchemaConfig,
    id: &IterationId,
    format: Format,
) -> IterationGlobs {
    let mut globs: Vec<String> = Vec::new();
    // Types whose template carries an {n} slot — these are the ones --iteration
    // can address. Kept only for the no-match diagnostic.
    let mut n_types: Vec<&str> = Vec::new();
    // Types with *some* filename_template but no {n} — surfaced in the error
    // so a user who set a template without the numeric slot sees why their
    // --iteration call didn't resolve.
    let mut templated_types: Vec<&str> = Vec::new();

    for (name, ts) in &schema.types {
        let Some(template_str) = &ts.filename_template else {
            continue;
        };
        let Ok(tpl) = FilenameTemplate::parse(template_str) else {
            // A malformed template is reported elsewhere (lint --type, new);
            // skip it here rather than turning an iteration lookup into a
            // template-parse error.
            templated_types.push(name);
            continue;
        };
        if tpl.has_n_placeholder() {
            n_types.push(name);
            globs.push(tpl.to_glob_for_id(id.raw()));
        } else {
            templated_types.push(name);
        }
    }

    if globs.is_empty() {
        let mut msg = String::from(
            "no type schema has a filename_template with an {n} placeholder, so --iteration cannot resolve any file",
        );
        if !n_types.is_empty() {
            // Unreachable (globs empty ⇒ n_types empty), but kept defensive.
            let _ = write!(msg, "; types with an {{n}} slot: {}", join(&n_types));
        }
        // Offer the next step: either configure a template with {n}, or fall
        // back to an explicit --file/--glob path the ID cannot disambiguate.
        // Built as an owned String so a dynamic templated-type list can flow
        // into the error without leaking (`format_error` borrows for the call).
        let hint: String = if templated_types.is_empty() {
            "set a filename_template containing {n} on a type (e.g. `hyalo types set iteration --filename-template 'iterations/iteration-{n}-{slug}.md`)".to_owned()
        } else {
            let names = join(&templated_types);
            format!(
                "these types have a filename_template but no {{n}} slot: {names}; add {{n}} to one of them, or address the file with --file/--glob"
            )
        };
        return IterationGlobs::Outcome(CommandOutcome::UserError(crate::output::format_error(
            format,
            &msg,
            Some(id.raw()),
            Some(&hint),
            None,
        )));
    }

    IterationGlobs::Globs(globs)
}

fn join(names: &[&str]) -> String {
    let mut sorted: Vec<&str> = names.to_vec();
    sorted.sort_unstable();
    sorted
        .iter()
        .map(|n| format!("'{n}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn schema_with_iteration(template: Option<&str>) -> SchemaConfig {
        let mut types = HashMap::new();
        let mut ts = hyalo_core::schema::TypeSchema::default();
        if let Some(t) = template {
            ts.filename_template = Some(t.to_owned());
        }
        types.insert("iteration".to_owned(), ts);
        SchemaConfig {
            types,
            ..SchemaConfig::default()
        }
    }

    #[test]
    fn resolves_bare_integer_to_iteration_glob() {
        let schema = schema_with_iteration(Some("iterations/iteration-{n}-{slug}.md"));
        let id = hyalo_core::iteration_id::parse_iteration_id("206").unwrap();
        match resolve_iteration_globs(&schema, &id, Format::Text) {
            IterationGlobs::Globs(g) => assert_eq!(g, vec!["iterations/iteration-206-*.md"]),
            other @ IterationGlobs::Outcome(_) => panic!("expected globs, got {other:?}"),
        }
    }

    #[test]
    fn resolves_letter_suffix() {
        let schema = schema_with_iteration(Some("iterations/iteration-{n}-{slug}.md"));
        let id = hyalo_core::iteration_id::parse_iteration_id("16b").unwrap();
        match resolve_iteration_globs(&schema, &id, Format::Text) {
            IterationGlobs::Globs(g) => assert_eq!(g, vec!["iterations/iteration-16b-*.md"]),
            other @ IterationGlobs::Outcome(_) => panic!("expected globs, got {other:?}"),
        }
    }

    #[test]
    fn no_template_with_n_slot_is_error_outcome() {
        let schema = schema_with_iteration(Some("journal/{date}.md"));
        let id = hyalo_core::iteration_id::parse_iteration_id("206").unwrap();
        match resolve_iteration_globs(&schema, &id, Format::Json) {
            IterationGlobs::Outcome(CommandOutcome::UserError(s)) => {
                let v: serde_json::Value = serde_json::from_str(&s).unwrap();
                let error = v["error"].as_str().unwrap();
                assert!(
                    error.contains("no type schema has a filename_template with an {n}"),
                    "got: {error}"
                );
                // Hint names the templated-but-slotless type and the fix.
                let hint = v["hint"].as_str().unwrap();
                assert!(
                    hint.contains("'iteration'") && hint.contains("add {n}"),
                    "hint should name the type and the fix: {hint}"
                );
            }
            other => panic!("expected error outcome, got {other:?}"),
        }
    }

    #[test]
    fn no_types_with_any_template_still_errors_with_setup_hint() {
        let schema = SchemaConfig::default();
        let id = hyalo_core::iteration_id::parse_iteration_id("206").unwrap();
        match resolve_iteration_globs(&schema, &id, Format::Json) {
            IterationGlobs::Outcome(CommandOutcome::UserError(s)) => {
                let v: serde_json::Value = serde_json::from_str(&s).unwrap();
                assert!(
                    v["hint"]
                        .as_str()
                        .unwrap()
                        .contains("set a filename_template containing {n}")
                );
            }
            other => panic!("expected error outcome, got {other:?}"),
        }
    }
}
