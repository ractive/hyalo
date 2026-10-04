//! Dispatching a JSON value to its text rendering, plus the generic object/scalar fallbacks.
//!
//! Split out of the single 3,744-line `output.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `output::...` paths and behaviour are unchanged.

use std::fmt::Write as _;

use super::{
    JaqFilterCache, apply_jq_filter, build_file_object_filter, format_generator_output_text,
    format_lint_fix_output_text, format_lint_output_text, format_lint_rules_list_text,
    format_lint_rules_mutation_text, format_type_list_entry_text, format_type_show_text,
    key_signature, lookup_filter,
};

// ---------------------------------------------------------------------------
// Text formatting
// ---------------------------------------------------------------------------

/// Format a JSON value as human-readable text using jq filters where available.
pub(super) fn format_value_as_text(
    value: &serde_json::Value,
    cache: &mut JaqFilterCache,
) -> String {
    match value {
        serde_json::Value::Array(arr) => {
            // TypeList: array of type list entries — use custom formatter with blank-line separation.
            let is_type_list = arr.first().and_then(|v| v.as_object()).is_some_and(|m| {
                key_signature(m) == "has_filename_template,property_count,required,type"
            });
            if is_type_list {
                return arr
                    .iter()
                    .filter_map(|v| v.as_object())
                    .map(format_type_list_entry_text)
                    .collect::<Vec<_>>()
                    .join("\n\n");
            }
            // `find --granularity section` hits (iteration 303):
            // `file#heading (lines A-B)  score`, snippets indented below.
            let is_section_hits = arr
                .first()
                .and_then(|v| v.as_object())
                .is_some_and(|m| key_signature(m) == "file,matches,score,section");
            if is_section_hits {
                return arr
                    .iter()
                    .map(format_section_hit_text)
                    .collect::<Vec<_>>()
                    .join("\n");
            }
            // `terms` listing: aligned `<term>  <docs>` rows (iteration 302).
            let is_term_list = arr
                .first()
                .and_then(|v| v.as_object())
                .is_some_and(|m| key_signature(m) == "docs,term");
            if is_term_list {
                let rows: Vec<(&str, u64)> = arr
                    .iter()
                    .filter_map(|v| Some((v.get("term")?.as_str()?, v.get("docs")?.as_u64()?)))
                    .collect();
                let width = rows
                    .iter()
                    .map(|(t, _)| t.chars().count())
                    .max()
                    .unwrap_or(0);
                return rows
                    .iter()
                    .map(|(term, docs)| {
                        format!(
                            "{term:<width$}  {docs} doc{}",
                            if *docs == 1 { "" } else { "s" }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
            }
            // LintRules list: array of rule entries with id, effective_enabled, etc.
            let is_lint_rules = arr.first().and_then(|v| v.as_object()).is_some_and(|m| {
                m.contains_key("id")
                    && m.contains_key("effective_enabled")
                    && m.contains_key("autofixable")
                    && m.contains_key("source")
            });
            if is_lint_rules {
                let result = format_lint_rules_list_text(arr);
                if !result.is_empty() {
                    return result;
                }
            }
            // A `find` listing: render each item through the FileObject filter
            // directly, with a blank-line separator for readability.
            //
            // iter-254: routing the items here rather than through the generic
            // per-value dispatch also keeps an exact `--fields` projection from
            // colliding with a single-payload key signature — `--fields
            // backlinks` yields `{file, backlinks}`, which is byte-identical to
            // a `backlinks` command result and used to render as one.
            let is_file_objects = arr
                .first()
                .and_then(|v| v.as_object())
                .is_some_and(is_file_object);
            if is_file_objects {
                return arr
                    .iter()
                    .map(|v| match v.as_object() {
                        Some(m) if is_file_object(m) => {
                            apply_jq_filter(&build_file_object_filter(m), v, cache)
                                .unwrap_or_else(|| format_value_as_text(v, cache))
                        }
                        _ => format_value_as_text(v, cache),
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");
            }
            arr.iter()
                .map(|v| format_value_as_text(v, cache))
                .collect::<Vec<_>>()
                .join("\n")
        }
        serde_json::Value::Object(map) => {
            let sig = key_signature(map);
            if let Some(filter) = lookup_filter(&sig)
                && let Some(output) = apply_jq_filter(filter, value, cache)
            {
                return output;
            }
            // TypeShow: detected by presence of "properties" object + "required" array + "type" string.
            // Accept both the old signature (without required_sections) and the new one.
            if sig == "defaults,filename_template,properties,required,type"
                || sig == "defaults,filename_template,properties,required,required_sections,type"
            {
                return format_type_show_text(map);
            }
            // LintFixOutput (fix-mode): detected by `total_fixed` + `files`.
            if map.contains_key("total_fixed")
                && map.contains_key("files")
                && map.contains_key("total_remaining")
            {
                return format_lint_fix_output_text(map);
            }
            // Lint output (`ExtLintOutput`): detected by "files" array of
            // {file, violations} plus the run-level violation count. That count
            // was renamed `total` -> `violations` in iter-216 (D-2); `total`
            // stays in the predicate so a payload produced by an older hyalo
            // still renders as lint text.
            if (map.contains_key("violations") || map.contains_key("total"))
                && map.contains_key("files")
                && let Some(serde_json::Value::Array(arr)) = map.get("files")
            {
                let is_lint = arr
                    .first()
                    .and_then(|v| v.as_object())
                    .is_some_and(|m| {
                        m.contains_key("file")
                            && (m.contains_key("violations") || m.contains_key("rule_groups"))
                    })
                    // Empty file list with new-shape totals counts as lint.
                    || (arr.is_empty()
                        && (map.contains_key("rules_fired")
                            || map.contains_key("files_checked")));
                if is_lint {
                    return format_lint_output_text(map);
                }
            }
            // LintRules mutation (set/remove): detected by `action` = "set" or "remove"
            // with `rule_id`, `before`, `after`, `config_path` fields.
            if matches!(
                map.get("action").and_then(serde_json::Value::as_str),
                Some("set" | "remove")
            ) && map.contains_key("rule_id")
                && map.contains_key("before")
                && map.contains_key("after")
                && map.contains_key("config_path")
            {
                return format_lint_rules_mutation_text(map);
            }
            // FileObject: dynamically compose filter from present fields.
            //
            // iter-254: `modified` is no longer an unconditional key (an exact
            // `--fields` projection can drop it), so it cannot be the
            // discriminator any more. A FileObject is instead recognised by
            // `file` plus the absence of any key outside the FileObject
            // vocabulary — precise enough that a lint entry (`file` +
            // `violations`) or any other `file`-bearing payload still falls
            // through to its own renderer.
            if is_file_object(map) {
                let filter = build_file_object_filter(map);
                if let Some(output) = apply_jq_filter(&filter, value, cache) {
                    return output;
                }
            }
            // Generator results (`okf index`, `okf log`, `madr toc`): keyed on the
            // `command` string. These carry nested `files` arrays / action fields
            // that the generic key:value dump renders unreadably.
            if let Some(cmd) = map.get("command").and_then(serde_json::Value::as_str)
                && let Some(out) = format_generator_output_text(cmd, map)
            {
                return out;
            }
            // Fallback: generic key: value lines
            format_object_generic(map, cache)
        }
        other => format_scalar(other, cache),
    }
}

/// One section hit: `file#heading (lines A-B)  score`, then `  line N: text`
/// per snippet. The preamble (heading null) prints without the `#heading`.
fn format_section_hit_text(hit: &serde_json::Value) -> String {
    let file = hit
        .get("file")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let section = hit.get("section");
    let field = |key: &str| {
        section
            .and_then(|s| s.get(key))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
    };
    let heading = section
        .and_then(|s| s.get("heading"))
        .and_then(serde_json::Value::as_str);
    let score = hit
        .get("score")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    let mut out = match heading {
        Some(heading) => format!("{file}#{heading}"),
        None => file.to_owned(),
    };
    let _ = write!(
        out,
        " (lines {}-{})  {score:.2}",
        field("line_start"),
        field("line_end")
    );
    // UX-10 text polish: `matches` is relevance-ordered (JSON keeps that —
    // it is the ranking), but printing a file's own lines out of order
    // (145, 149, 147) reads as broken rather than ranked. The text renderer
    // sorts by line number; JSON is unaffected.
    let mut matches: Vec<&serde_json::Value> = hit
        .get("matches")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .collect();
    matches.sort_by_key(|m| {
        m.get("line")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
    });
    for m in matches {
        let line = m
            .get("line")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let text = m
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let _ = write!(out, "\n  line {line}: {}", text.trim());
    }
    out
}

/// Generic key: value rendering for unknown object shapes.
pub(super) fn format_object_generic(
    map: &serde_json::Map<String, serde_json::Value>,
    cache: &mut JaqFilterCache,
) -> String {
    map.iter()
        .map(|(k, v)| {
            let rendered = format_value_as_text(v, cache);
            // iter-267 (UX-14): a multi-line value used to print its first
            // line after `key: ` and every later line at column 0, so
            // `modified: References/Bass on Top.md` followed by
            // `References/Blade Runner.md` read as two more keys. Indenting
            // the continuation two spaces makes the nesting visible — the
            // same shape the purpose-built mutation renderers already use.
            if rendered.contains('\n') {
                let indented = rendered.replace('\n', "\n  ");
                format!("{k}:\n  {indented}")
            } else {
                format!("{k}: {rendered}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format a scalar JSON value as text.
pub(super) fn format_scalar(value: &serde_json::Value, cache: &mut JaqFilterCache) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_owned(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(|v| format_scalar(v, cache)).collect();
            items.join(", ")
        }
        serde_json::Value::Object(_) => format_value_as_text(value, cache),
    }
}

/// Whether `map` is a [`hyalo_core::types::FileObject`] — a `file` key and no
/// key outside the FileObject vocabulary.
///
/// Since iteration 254 an exact `--fields` projection can reduce an item to
/// `{file}`, so no single optional key can serve as the marker; the test is
/// "nothing foreign present" instead.
fn is_file_object(map: &serde_json::Map<String, serde_json::Value>) -> bool {
    map.contains_key("file") && map.keys().all(|k| is_file_object_key(k))
}

/// Whether `key` belongs to the [`hyalo_core::types::FileObject`] vocabulary.
///
/// Used to recognise a `find` result item in an untyped JSON value; keep in
/// sync with the struct's serialised field names.
fn is_file_object_key(key: &str) -> bool {
    matches!(
        key,
        "file"
            | "modified"
            | "size"
            | "lines"
            | "title"
            // Rides along with `title` since iter-267 (DEC-283).
            | "title_source"
            | "properties"
            | "properties_typed"
            | "tags"
            | "sections"
            | "tasks"
            | "links"
            | "backlinks"
            | "matches"
            | "score"
    )
}

#[cfg(test)]
mod section_hit_tests {
    use super::*;

    /// UX-10 text polish: `matches` arrives relevance-ordered (JSON keeps
    /// that), but a file's own lines printing out of order (145, 149, 147)
    /// reads as broken. The text renderer must print them by line number.
    #[test]
    fn section_hit_text_sorts_snippet_lines_ascending() {
        let hit = serde_json::json!({
            "file": "a.md",
            "section": {"heading": "Problem", "line_start": 145, "line_end": 153},
            "score": 3.69,
            "matches": [
                {"line": 149, "text": "third"},
                {"line": 145, "text": "first"},
                {"line": 147, "text": "second"},
            ],
        });
        let text = format_section_hit_text(&hit);
        let line_pos = |needle: &str| text.find(needle).unwrap();
        assert!(
            line_pos("line 145") < line_pos("line 147")
                && line_pos("line 147") < line_pos("line 149"),
            "{text}"
        );
    }
}
