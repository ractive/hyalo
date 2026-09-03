//! Unit tests for the output formatters.
//!
//! Split out of `output.rs` in iteration 247: the module body is unchanged, it
//! just lives in its own file now.

use super::*;
// Items that stayed inside a sibling submodule after the iteration-247 file
// split: the parent imports only what it calls itself, and these are exercised
// only from here.
use super::filters::{
    CONTENT_MATCH_FILTER, FIND_TASK_INFO_FILTER, LINK_INFO_FILTER, MV_RESULT_FILTER,
    OUTLINE_SECTION_FILTER, OUTLINE_SECTION_WITH_TASKS_FILTER, PROPERTY_INFO_FILTER,
    PROPERTY_MUTATION_FILTER, PROPERTY_SUMMARY_ENTRY_FILTER, PROPERTY_VALUE_MUTATION_FILTER,
    TAG_MUTATION_FILTER, TAG_SUMMARY_FILTER, TASK_COUNT_FILTER,
};
use super::text::format_scalar;
use serde_json::json;

// Convenience wrappers so individual tests don't have to construct a cache.
fn jq(filter: &str, val: &serde_json::Value) -> Option<String> {
    apply_jq_filter(filter, val, &mut JaqFilterCache::new())
}

fn fmt(val: &serde_json::Value) -> String {
    format_value_as_text(val, &mut JaqFilterCache::new())
}

fn scalar(val: &serde_json::Value) -> String {
    format_scalar(val, &mut JaqFilterCache::new())
}

// -----------------------------------------------------------------------
// iter-213 UX-5 — a rule is never shown as both fixed and conflicted
// -----------------------------------------------------------------------

fn fix_output(fixed_rule: Option<&str>, conflict_rule: &str) -> String {
    let fixed_groups = match fixed_rule {
        Some(rule) => json!([{"rule": rule, "count": 1, "violations": []}]),
        None => json!([]),
    };
    let value = json!({
        "dry_run": false,
        "files_checked": 1,
        "total_fixed": 1,
        "total_remaining": 0,
        "total_conflicts": 1,
        "files": [{
            "file": "a.md",
            "fixed_groups": fixed_groups,
            "remaining_groups": [],
            "conflicts": [{"rule": conflict_rule, "reason": "range overlap with MD009"}],
        }],
    });
    format_lint_fix_output_text(value.as_object().expect("object"))
}

#[test]
fn fix_text_suppresses_a_conflict_for_a_rule_already_shown_as_fixed() {
    let out = fix_output(Some("MD047"), "MD047");
    assert!(
        !out.contains("conflict  MD047"),
        "a rule shown as fixed must not also be shown as conflicted: {out}"
    );
    assert!(
        out.contains("MD047"),
        "the fixed line itself must survive: {out}"
    );
}

#[test]
fn fix_text_keeps_a_conflict_for_a_rule_that_was_not_fixed() {
    let out = fix_output(Some("MD047"), "MD013");
    assert!(
        out.contains("conflict  MD013"),
        "an unrelated rule's conflict is the whole point of the line: {out}"
    );
}

// --- error formatting ---

#[test]
fn format_json_error() {
    let out = format_error(
        Format::Json,
        "file not found",
        Some("foo/bar"),
        Some("did you mean foo/bar.md?"),
        None,
    );
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["error"], "file not found");
    assert_eq!(parsed["hint"], "did you mean foo/bar.md?");
    assert!(parsed.get("cause").is_none());
}

#[test]
fn format_text_error() {
    let out = format_error(Format::Text, "file not found", Some("foo"), None, None);
    assert!(out.contains("Error: file not found"));
    assert!(out.contains("path: foo"));
}

#[test]
fn format_json_success() {
    let val = json!({"name": "test", "value": 42});
    let out = format_success(Format::Json, &val);
    assert!(out.contains("\"name\": \"test\""));
}

// --- apply_jq_filter ---

#[test]
fn apply_jq_filter_simple() {
    let val = json!({"name": "hello", "count": 3});
    let result = jq(r#""\(.name): \(.count)""#, &val);
    assert_eq!(result.as_deref(), Some("hello: 3"));
}

#[test]
fn apply_jq_filter_array_map() {
    let val = json!(["a", "b", "c"]);
    let result = jq(".[]", &val);
    assert_eq!(result.as_deref(), Some("a\nb\nc"));
}

#[test]
fn apply_jq_filter_invalid_returns_none() {
    let val = json!({"x": 1});
    let result = jq("this is not valid jq %%%", &val);
    assert!(result.is_none());
}

// --- F-3: jq runtime errors are bounded, not whole-input dumps ---

#[test]
fn jq_runtime_error_on_large_input_is_bounded_and_names_the_filter() {
    // A filter that fails against a large array (e.g. mistakenly indexing
    // an array with a string, the `--jq '.results | .file'` shape from
    // deep-analysis-2's F-3) must not embed the whole array — jaq's
    // runtime Display would otherwise dump every element into the error.
    let big_array: Vec<serde_json::Value> = (0..5000)
        .map(|i| json!({"file": format!("file-{i}.md"), "padding": "x".repeat(200)}))
        .collect();
    let val = json!(big_array);
    let err = apply_jq_filter_result(".file", &val).expect_err("indexing an array must fail");

    assert!(
        err.len() < 1000,
        "error should be bounded to roughly 2x the char cap, got {} bytes",
        err.len()
    );
    assert!(
        err.contains(".file"),
        "error should name the failing filter: {err}"
    );
    assert!(
        !err.contains("file-4999.md"),
        "error must not embed the tail of the huge input value: {err}"
    );
}

#[test]
fn jq_runtime_error_on_large_object_value_does_not_leak_its_content() {
    // Array-indexing an object fails at runtime; the object (which holds
    // a large string field) must not appear verbatim in the error.
    let mut map = serde_json::Map::new();
    let needle = "x".repeat(5000);
    map.insert("huge".to_owned(), serde_json::Value::String(needle.clone()));
    let val = serde_json::Value::Object(map);

    let err = apply_jq_filter_result(".huge.nope", &val)
        .expect_err("field-indexing a string value must fail");
    assert!(
        err.len() < 1000,
        "error should be bounded, got {} bytes",
        err.len()
    );
    assert!(
        !err.contains(&needle),
        "error must not embed the large field value verbatim"
    );
}

#[test]
fn truncate_diagnostic_appends_ellipsis_only_when_truncated() {
    let short = "short message";
    assert_eq!(truncate_diagnostic(short), short);

    let long = "x".repeat(500);
    let truncated = truncate_diagnostic(&long);
    assert_eq!(truncated.chars().count(), JQ_ERROR_DIAGNOSTIC_CHAR_CAP + 1);
    assert!(truncated.ends_with('…'));
}

#[test]
fn truncate_diagnostic_never_splits_a_multibyte_codepoint() {
    // 300 multi-byte CJK characters — truncation must cut on char
    // boundaries, not bytes, or this would panic on a split codepoint.
    let long: String = "日".repeat(300);
    let truncated = truncate_diagnostic(&long);
    assert_eq!(truncated.chars().count(), JQ_ERROR_DIAGNOSTIC_CHAR_CAP + 1);
    assert!(truncated.ends_with('…'));
}

// --- jq output size cap ---

#[test]
fn jq_output_cap_constant_is_10_mib() {
    assert_eq!(JQ_OUTPUT_CAP, 10 * 1024 * 1024);
}

#[test]
fn jq_output_within_cap_succeeds() {
    // A small output must pass through without hitting the cap.
    let val = json!({"msg": "hello"});
    let result = apply_jq_filter_result(".msg", &val);
    assert_eq!(result.as_deref(), Ok("hello"));
}

#[test]
fn jq_output_cap_triggers_on_large_output() {
    // Build a JSON array large enough to exceed JQ_OUTPUT_CAP when expanded.
    // Each element is "aaaa...a" (1000 chars). 11_000 elements = 11 MB > 10 MB cap.
    let big_string = "a".repeat(1000);
    let val = serde_json::Value::Array(
        std::iter::repeat_n(serde_json::Value::String(big_string), 11_000).collect(),
    );
    // ".[]" emits each element as a separate output value.
    let result = apply_jq_filter_result(".[]", &val);
    assert!(result.is_err(), "expected cap error but got Ok output");
    let err = result.unwrap_err();
    assert!(
        err.contains("exceeds") && err.contains("MiB"),
        "unexpected error message: {err}"
    );
}

#[test]
fn jq_single_huge_string_value_rejected_via_raw_length_before_copying() {
    // Finding 2 (review round on PR #254): a single value already over
    // JQ_OUTPUT_CAP on its own must be rejected using its *existing*
    // borrowed bytes, not measured only after being duplicated into an
    // owned copy. Uses an 11 MiB *input* string (cheap to build in a
    // unit test) piped through the identity filter `.` — this emits it
    // as a single `Val::TStr`, exercising the exact single-value
    // pre-check path the real repro (`"x" * 2000000000`, ~4 GB
    // unmitigated) hits, without needing gigabytes in a fast test.
    let big_string = "a".repeat(11 * 1024 * 1024);
    let val = serde_json::Value::String(big_string);
    let result = apply_jq_filter_result(".", &val);
    assert!(
        result.is_err(),
        "a single value over the byte cap must be rejected"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("exceeds") && err.contains("MiB"),
        "unexpected error message: {err}"
    );
}

// --- F3-1: jq resource limits (deep-analysis-3-2026-08-23.md) ---

#[test]
fn jq_max_output_values_constant_is_one_million() {
    assert_eq!(JQ_MAX_OUTPUT_VALUES, 1_000_000);
}

#[test]
fn jq_time_limit_constant_is_three_seconds() {
    assert_eq!(JQ_TIME_LIMIT, std::time::Duration::from_secs(3));
}

#[test]
fn jq_value_count_cap_triggers_before_byte_cap_on_many_tiny_values() {
    // `range(...)` without array-collection is a streaming generator: our
    // for loop in `execute_jq_filter` pulls one value at a time, so this
    // is cheap (no huge intermediate) and exercises JQ_MAX_OUTPUT_VALUES
    // directly rather than JQ_TIME_LIMIT or JQ_OUTPUT_CAP. Each emitted
    // value is a single-digit-or-more number (a few bytes), so the byte
    // cap (10 MiB) would take far longer than 1,000,000 iterations to
    // reach — the value-count cap must fire first.
    // A generous deadline: this test pins the value-count cap, not the
    // clock. Under QEMU-emulated aarch64 (`cross test`) 1,000,000 jaq
    // iterations take longer than JQ_TIME_LIMIT and the timeout won the
    // race in the v0.21.0 release pipeline.
    let val = json!(null);
    let result =
        apply_jq_filter_with_limit("range(2000000)", &val, std::time::Duration::from_secs(300));
    assert!(
        result.is_err(),
        "expected the value-count cap to trigger, got Ok"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("1000000") && err.contains("values"),
        "expected a value-count-limit error, got: {err}"
    );
}

#[test]
fn jq_infinite_recursion_with_no_output_errors_within_time_limit() {
    // `def f: f; f` never emits a value at all, so the loop body in
    // `execute_jq_filter` never runs even once — neither JQ_OUTPUT_CAP
    // nor JQ_MAX_OUTPUT_VALUES can catch it, only the wall-clock
    // deadline on the worker thread can. This genuinely blocks for
    // ~JQ_TIME_LIMIT (3s) — an acceptable one-time cost for a real
    // regression test on a HIGH-severity finding; CPU cost is trivial
    // (no allocation), unlike the array-collection case covered by the
    // e2e tests in `tests/e2e/jq.rs`.
    let val = json!(null);
    let started = std::time::Instant::now();
    let result = apply_jq_filter_result("def f: f; f", &val);
    assert!(
        result.is_err(),
        "expected a time-limit error, got Ok output"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("time limit"),
        "expected a time-limit error, got: {err}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(9),
        "must return well within a small multiple of JQ_TIME_LIMIT, not hang"
    );
}

// --- property type filters ---

#[test]
fn property_info_filter() {
    let val = json!({"name": "title", "type": "text", "value": "My Note"});
    let out = jq(PROPERTY_INFO_FILTER, &val).unwrap();
    assert!(out.contains("title"));
    assert!(out.contains("text"));
    assert!(out.contains("My Note"));
}

#[test]
fn property_info_filter_list_value() {
    let val = json!({"name": "tags", "type": "list", "value": ["rust", "cli"]});
    let out = jq(PROPERTY_INFO_FILTER, &val).unwrap();
    assert!(out.contains("tags"));
    assert!(out.contains("list"));
    // Array values should be wrapped in brackets and joined with ", "
    assert!(out.contains("[rust, cli]"), "expected [rust, cli]: {out}");
    assert!(!out.contains("[\"rust\""));
}

#[test]
fn property_summary_entry_filter() {
    let val = json!({"count": 7, "name": "title", "type": "text"});
    let out = jq(PROPERTY_SUMMARY_ENTRY_FILTER, &val).unwrap();
    assert!(out.contains("title"));
    assert!(out.contains("text"));
    assert!(out.contains("7 files"));
}

#[test]
fn tag_summary_filter() {
    let val = json!({
        "tags": [{"name": "rust", "count": 3}, {"name": "cli", "count": 1}],
        "total": 2
    });
    let out = jq(TAG_SUMMARY_FILTER, &val).unwrap();
    assert!(out.contains("2 unique tags"));
    assert!(out.contains("rust"));
    assert!(out.contains("3 files"));
}

// --- link type filters ---

#[test]
fn link_info_target_only_filter() {
    let val = json!({"target": "broken-link"});
    let out = jq(LINK_INFO_FILTER, &val).unwrap();
    assert!(out.contains("broken-link"));
    assert!(out.contains("unresolved"));
}

#[test]
fn link_info_with_path_filter() {
    let val = json!({"path": "note-b.md", "target": "note-b"});
    let out = jq(LINK_INFO_FILTER, &val).unwrap();
    assert!(out.contains("note-b"));
    assert!(out.contains("note-b.md"));
}

#[test]
fn link_info_anchored_resolved_filter() {
    // Target and anchor both resolve.
    let val = json!({"path": "foo.md", "target": "Foo", "fragment": "Real"});
    let out = jq(LINK_INFO_FILTER, &val).unwrap();
    assert!(out.contains("Foo#Real"));
    assert!(out.contains("foo.md"));
    assert!(!out.contains("broken anchor"));
    assert!(!out.contains("unresolved"));
}

#[test]
fn link_info_anchored_broken_anchor_filter() {
    // Target resolves but the anchor does not — distinct from unresolved.
    let val = json!({
        "path": "foo.md", "target": "Foo", "fragment": "Nope", "broken_anchor": true
    });
    let out = jq(LINK_INFO_FILTER, &val).unwrap();
    assert!(out.contains("Foo#Nope"));
    assert!(out.contains("foo.md"));
    assert!(out.contains("broken anchor"));
    assert!(!out.contains("unresolved"));
}

#[test]
fn link_info_anchored_broken_target_filter() {
    // Target unresolved: anchor check never ran, path is null.
    let val = json!({"target": "Nope", "fragment": "x"});
    let out = jq(LINK_INFO_FILTER, &val).unwrap();
    assert!(out.contains("Nope#x"));
    assert!(out.contains("unresolved"));
    assert!(!out.contains("broken anchor"));
}

#[test]
fn link_info_anchored_with_label_filter() {
    let val = json!({
        "path": "foo.md", "target": "Foo", "fragment": "Real", "label": "click"
    });
    let out = jq(LINK_INFO_FILTER, &val).unwrap();
    assert!(out.contains("Foo#Real"));
    assert!(out.contains("[click]"));
}

#[test]
fn link_info_external_renders_its_kind_and_no_verdict() {
    // iter-261 / BUG-2: an external URI is neither resolved nor broken.
    let val = json!({
        "target": "obsidian://show-plugin?id=x", "path": null,
        "kind": "external", "line": 24
    });
    let out = jq(LINK_INFO_FILTER, &val).unwrap();
    assert!(out.contains("obsidian://show-plugin?id=x"));
    assert!(out.contains("(external)"));
    assert!(!out.contains("unresolved"));
}

#[test]
fn link_info_attachment_renders_its_kind_after_the_arrow() {
    let val = json!({
        "target": "task-plugins-sorted.png",
        "path": "02 Attachments/task-plugins-sorted.png",
        "kind": "attachment", "line": 28
    });
    let out = jq(LINK_INFO_FILTER, &val).unwrap();
    assert!(out.contains("→ \"02 Attachments/task-plugins-sorted.png\""));
    assert!(out.contains("(attachment)"));
}

#[test]
fn link_info_plain_wikilink_kind_is_not_rendered() {
    let val = json!({"target": "note", "path": "note.md", "kind": "wikilink", "line": 3});
    let out = jq(LINK_INFO_FILTER, &val).unwrap();
    assert!(!out.contains("(wikilink)"));
}

#[test]
fn link_info_renders_the_dec_268_anchor_suggestion() {
    let val = json!({
        "target": "decision-log", "path": "decision-log.md", "kind": "wikilink",
        "fragment": "DEC-068", "broken_anchor": true,
        "suggested_fragment": "DEC-068: Snapshot index format", "line": 9
    });
    let out = jq(LINK_INFO_FILTER, &val).unwrap();
    assert!(out.contains("broken anchor"));
    assert!(out.contains("did you mean \"#DEC-068: Snapshot index format\"?"));
}

#[test]
fn link_info_out_of_vault_still_renders_its_verdict() {
    let val = json!({
        "target": "../../CONTRIBUTING.md", "path": null, "kind": "markdown",
        "out_of_vault": true, "line": 5
    });
    let out = jq(LINK_INFO_FILTER, &val).unwrap();
    assert!(out.contains("(out of vault)"));
    assert!(!out.contains("unresolved"));
}

// --- outline type filters ---

#[test]
fn task_count_filter() {
    let val = json!({"done": 3, "total": 5});
    let out = jq(TASK_COUNT_FILTER, &val).unwrap();
    assert_eq!(out, "[3/5]");
}

#[test]
fn outline_section_filter() {
    let val = json!({
        "code_blocks": [],
        "heading": "Introduction",
        "level": 1,
        "line": 5,
        "links": ["[[other]]"]
    });
    let out = jq(OUTLINE_SECTION_FILTER, &val).unwrap();
    assert!(out.contains('#'));
    assert!(out.contains("Introduction"));
    assert!(out.contains("[[other]]"));
}

#[test]
fn outline_section_with_tasks_filter() {
    let val = json!({
        "code_blocks": [],
        "heading": "Tasks",
        "level": 2,
        "line": 10,
        "links": [],
        "tasks": {"done": 2, "total": 4}
    });
    let out = jq(OUTLINE_SECTION_WITH_TASKS_FILTER, &val).unwrap();
    assert!(out.contains("##"));
    assert!(out.contains("Tasks"));
    assert!(out.contains("[2/4]"));
}

/// NEW-16 (dogfood pre3): a hand-written `[n/m]` already in the heading
/// text must render once — the computed count replaces it rather than
/// appending a second bracket group (`## Tasks [6/6] [2/4]`).
#[test]
fn outline_section_with_tasks_filter_replaces_a_stale_hand_written_count() {
    let val = json!({
        "code_blocks": [],
        "heading": "Tasks [6/6]",
        "level": 2,
        "line": 10,
        "links": [],
        "tasks": {"done": 2, "total": 4}
    });
    let out = jq(OUTLINE_SECTION_WITH_TASKS_FILTER, &val).unwrap();
    assert!(
        out.contains("Tasks [2/4]"),
        "expected the computed count, got: {out}"
    );
    assert!(
        !out.contains("[6/6]"),
        "the stale hand-written count must not survive: {out}"
    );
    assert_eq!(
        out.matches('[').count(),
        1,
        "exactly one bracket group must render: {out}"
    );
}

// --- FindTaskInfo filter ---

#[test]
fn find_task_info_filter_done() {
    let val = json!({
        "done": true,
        "line": 42,
        "section": "Implementation",
        "status": "x",
        "text": "Write the tests"
    });
    let out = jq(FIND_TASK_INFO_FILTER, &val).unwrap();
    assert!(out.contains("[x]"));
    assert!(out.contains("Write the tests"));
    assert!(out.contains("line 42"));
    assert!(out.contains("Implementation"));
}

#[test]
fn find_task_info_filter_not_done() {
    let val = json!({
        "done": false,
        "line": 7,
        "section": "Todo",
        "status": " ",
        "text": "Review PR"
    });
    let out = jq(FIND_TASK_INFO_FILTER, &val).unwrap();
    assert!(out.contains("[ ]"));
    assert!(out.contains("Review PR"));
    assert!(out.contains("line 7"));
    assert!(out.contains("Todo"));
}

#[test]
fn find_task_info_via_format_value_as_text() {
    // Verify that format_value_as_text dispatches to the correct filter.
    let val = json!({
        "done": true,
        "line": 5,
        "section": "Goals",
        "status": "x",
        "text": "Ship it"
    });
    let out = fmt(&val);
    assert!(out.contains("[x]"));
    assert!(out.contains("Ship it"));
    assert!(
        !out.contains("done: true"),
        "should not use generic fallback"
    );
}

// --- ContentMatch filter ---

#[test]
fn content_match_filter() {
    let val = json!({
        "line": 15,
        "section": "Background",
        "text": "This is the matching line"
    });
    let out = jq(CONTENT_MATCH_FILTER, &val).unwrap();
    assert!(out.contains("line 15"));
    assert!(out.contains("Background"));
    assert!(out.contains("This is the matching line"));
}

#[test]
fn content_match_via_format_value_as_text() {
    let val = json!({
        "line": 3,
        "section": "Intro",
        "text": "hello world"
    });
    let out = fmt(&val);
    assert!(out.contains("line 3"));
    assert!(out.contains("hello world"));
    assert!(!out.contains("line: 3"), "should not use generic fallback");
}

// --- Mutation result filters ---

#[test]
fn property_value_mutation_filter_with_modified() {
    // SetPropertyResult / AppendPropertyResult / RemovePropertyResult (with value)
    // scanned == total: no "(N scanned)" suffix
    let val = json!({
        "modified": ["note-a.md", "note-b.md"],
        "property": "status",
        "scanned": 2,
        "skipped": [],
        "skipped_count": 0,
        "total": 2,
        "value": "done"
    });
    let out = jq(PROPERTY_VALUE_MUTATION_FILTER, &val).unwrap();
    assert!(out.contains("status=done"));
    assert!(out.contains("2/2 modified"));
    assert!(
        !out.contains("scanned"),
        "no scanned suffix when scanned == total"
    );
    assert!(out.contains("note-a.md"));
    assert!(out.contains("note-b.md"));
}

#[test]
fn property_value_mutation_filter_all_skipped() {
    let val = json!({
        "modified": [],
        "property": "priority",
        "scanned": 1,
        "skipped": ["note-a.md"],
        "skipped_count": 1,
        "total": 1,
        "value": "high"
    });
    let out = jq(PROPERTY_VALUE_MUTATION_FILTER, &val).unwrap();
    assert!(out.contains("priority=high"));
    assert!(out.contains("0/1 modified"));
    // No file paths should appear when nothing was modified
    assert!(!out.contains("note-a.md"));
}

#[test]
fn property_value_mutation_filter_with_where_filter() {
    // scanned > total: "(N scanned)" suffix should appear
    let val = json!({
        "modified": ["note-a.md"],
        "property": "status",
        "scanned": 5,
        "skipped": [],
        "skipped_count": 0,
        "total": 1,
        "value": "done"
    });
    let out = jq(PROPERTY_VALUE_MUTATION_FILTER, &val).unwrap();
    assert!(out.contains("status=done"));
    assert!(out.contains("1/1 modified"));
    assert!(out.contains("(5 scanned)"));
}

#[test]
fn property_value_mutation_via_format_value_as_text() {
    let val = json!({
        "dry_run": false,
        "modified": ["notes/a.md"],
        "property": "status",
        "scanned": 1,
        "skipped": [],
        "skipped_count": 0,
        "total": 1,
        "value": "done"
    });
    let out = fmt(&val);
    assert!(out.contains("status=done"));
    assert!(
        !out.contains("modified: "),
        "should not use generic fallback"
    );
}

#[test]
fn property_mutation_filter_no_value() {
    // RemovePropertyResult without value; scanned == total
    let val = json!({
        "dry_run": false,
        "modified": ["note.md"],
        "property": "draft",
        "scanned": 1,
        "skipped": [],
        "skipped_count": 0,
        "total": 1
    });
    let out = jq(PROPERTY_MUTATION_FILTER, &val).unwrap();
    assert!(out.contains("draft"));
    assert!(out.contains("1/1 modified"));
    assert!(
        !out.contains("scanned"),
        "no scanned suffix when scanned == total"
    );
    assert!(out.contains("note.md"));
}

#[test]
fn property_mutation_filter_no_value_with_where_filter() {
    // RemovePropertyResult without value; scanned > total
    let val = json!({
        "dry_run": false,
        "modified": ["note.md"],
        "property": "draft",
        "scanned": 7,
        "skipped": [],
        "skipped_count": 0,
        "total": 1
    });
    let out = jq(PROPERTY_MUTATION_FILTER, &val).unwrap();
    assert!(out.contains("draft"));
    assert!(out.contains("1/1 modified"));
    assert!(out.contains("(7 scanned)"));
}

#[test]
fn tag_mutation_filter_with_modified() {
    // SetTagResult / RemoveTagResult; scanned == total
    let val = json!({
        "dry_run": false,
        "modified": ["a.md", "b.md"],
        "scanned": 3,
        "skipped": ["c.md"],
        "skipped_count": 1,
        "tag": "rust",
        "total": 3
    });
    let out = jq(TAG_MUTATION_FILTER, &val).unwrap();
    assert!(out.contains("rust"));
    assert!(out.contains("2/3 modified"));
    assert!(
        !out.contains("scanned"),
        "no scanned suffix when scanned == total"
    );
    assert!(out.contains("a.md"));
    assert!(out.contains("b.md"));
    assert!(!out.contains("c.md"));
}

#[test]
fn tag_mutation_filter_with_where_filter() {
    // scanned > total: "(N scanned)" suffix
    let val = json!({
        "dry_run": false,
        "modified": ["a.md"],
        "scanned": 10,
        "skipped": [],
        "skipped_count": 0,
        "tag": "rust",
        "total": 1
    });
    let out = jq(TAG_MUTATION_FILTER, &val).unwrap();
    assert!(out.contains("rust"));
    assert!(out.contains("1/1 modified"));
    assert!(out.contains("(10 scanned)"));
}

#[test]
fn tag_mutation_via_format_value_as_text() {
    let val = json!({
        "dry_run": false,
        "modified": [],
        "scanned": 1,
        "skipped": ["note.md"],
        "skipped_count": 1,
        "tag": "cli",
        "total": 1
    });
    let out = fmt(&val);
    assert!(out.contains("cli"));
    assert!(!out.contains("tag: cli"), "should not use generic fallback");
}

// --- dry-run prefix in text output ---

#[test]
fn property_value_mutation_dry_run_prefix() {
    let val = json!({
        "dry_run": true,
        "modified": ["note.md"],
        "property": "status",
        "scanned": 1,
        "skipped": [],
        "skipped_count": 0,
        "total": 1,
        "value": "done"
    });
    let out = fmt(&val);
    assert!(
        out.contains("[dry-run] status=done"),
        "dry-run prefix missing: {out}"
    );
}

#[test]
fn tag_mutation_dry_run_prefix() {
    let val = json!({
        "dry_run": true,
        "modified": ["note.md"],
        "scanned": 1,
        "skipped": [],
        "skipped_count": 0,
        "tag": "rust",
        "total": 1
    });
    let out = fmt(&val);
    assert!(
        out.contains("[dry-run] rust"),
        "dry-run prefix missing: {out}"
    );
}

#[test]
fn property_value_mutation_no_dry_run_prefix() {
    let val = json!({
        "dry_run": false,
        "modified": ["note.md"],
        "property": "status",
        "scanned": 1,
        "skipped": [],
        "skipped_count": 0,
        "total": 1,
        "value": "done"
    });
    let out = fmt(&val);
    assert!(
        !out.contains("[dry-run]"),
        "should not have dry-run prefix: {out}"
    );
}

// --- build_file_object_filter ---

#[test]
fn build_file_object_filter_minimal() {
    // Only the required `file` and `modified` fields.
    let map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(r#"{"file": "notes/foo.md", "modified": "2024-01-01"}"#).unwrap();
    let filter = build_file_object_filter(&map);
    let val = json!({"file": "notes/foo.md", "modified": "2024-01-01"});
    let out = jq(&filter, &val).unwrap();
    assert!(out.contains("notes/foo.md"));
    assert!(out.contains("2024-01-01"));
}

#[test]
fn build_file_object_filter_with_tags() {
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"file": "foo.md", "modified": "2024-01-01", "tags": ["rust", "cli"]}"#,
    )
    .unwrap();
    let filter = build_file_object_filter(&map);
    let val = json!({"file": "foo.md", "modified": "2024-01-01", "tags": ["rust", "cli"]});
    let out = jq(&filter, &val).unwrap();
    assert!(out.contains("foo.md"));
    assert!(out.contains("tags: [rust, cli]"));
}

#[test]
fn build_file_object_filter_with_properties() {
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"file": "foo.md", "modified": "2024-01-01", "properties": {"status": "done"}}"#,
    )
    .unwrap();
    let filter = build_file_object_filter(&map);
    let val = json!({
        "file": "foo.md",
        "modified": "2024-01-01",
        "properties": {"status": "done"}
    });
    let out = jq(&filter, &val).unwrap();
    assert!(out.contains("foo.md"));
    assert!(out.contains("properties:"));
    assert!(out.contains("status: done"));
}

#[test]
fn build_file_object_filter_with_tasks() {
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"file": "foo.md", "modified": "2024-01-01", "tasks": [{"done": true, "line": 5, "section": "Goals", "status": "x", "text": "Ship it"}]}"#,
    )
    .unwrap();
    let filter = build_file_object_filter(&map);
    let val = json!({
        "file": "foo.md",
        "modified": "2024-01-01",
        "tasks": [{"done": true, "line": 5, "section": "Goals", "status": "x", "text": "Ship it"}]
    });
    let out = jq(&filter, &val).unwrap();
    assert!(out.contains("foo.md"));
    assert!(out.contains("tasks:"));
    assert!(out.contains("[x] Ship it"));
    assert!(out.contains("line 5"));
}

#[test]
fn build_file_object_filter_with_sections() {
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"file": "foo.md", "modified": "2024-01-01", "sections": [{"code_blocks": 0, "heading": "Intro", "level": 1, "line": 1, "links": []}]}"#,
    )
    .unwrap();
    let filter = build_file_object_filter(&map);
    let val = json!({
        "file": "foo.md",
        "modified": "2024-01-01",
        "sections": [{"code_blocks": 0, "heading": "Intro", "level": 1, "line": 1, "links": []}]
    });
    let out = jq(&filter, &val).unwrap();
    assert!(out.contains("foo.md"));
    assert!(out.contains("sections:"));
    assert!(out.contains("# Intro"));
}

#[test]
fn build_file_object_filter_with_matches() {
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"file": "foo.md", "modified": "2024-01-01", "matches": [{"line": 3, "section": "Intro", "text": "hello world"}]}"#,
    )
    .unwrap();
    let filter = build_file_object_filter(&map);
    let val = json!({
        "file": "foo.md",
        "modified": "2024-01-01",
        "matches": [{"line": 3, "section": "Intro", "text": "hello world"}]
    });
    let out = jq(&filter, &val).unwrap();
    assert!(out.contains("foo.md"));
    assert!(out.contains("matches:"));
    assert!(out.contains("line 3 (Intro): hello world"));
}

#[test]
fn build_file_object_filter_with_links() {
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"file": "foo.md", "modified": "2024-01-01", "links": [{"target": "bar", "path": "bar.md"}]}"#,
    )
    .unwrap();
    let filter = build_file_object_filter(&map);
    let val = json!({
        "file": "foo.md",
        "modified": "2024-01-01",
        "links": [{"target": "bar", "path": "bar.md"}]
    });
    let out = jq(&filter, &val).unwrap();
    assert!(out.contains("foo.md"));
    assert!(out.contains("links:"));
    assert!(out.contains(r#""bar" → "bar.md""#));
}

#[test]
fn build_file_object_filter_unresolved_link() {
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"file": "foo.md", "modified": "2024-01-01", "links": [{"target": "missing"}]}"#,
    )
    .unwrap();
    let filter = build_file_object_filter(&map);
    let val = json!({
        "file": "foo.md",
        "modified": "2024-01-01",
        "links": [{"target": "missing"}]
    });
    let out = jq(&filter, &val).unwrap();
    assert!(out.contains(r#""missing" (unresolved)"#));
}

// --- FileObject text rendering through format_value_as_text ---

#[test]
fn file_object_text_rendering_minimal() {
    let val = json!({"file": "notes/foo.md", "modified": "2024-01-15"});
    let out = fmt(&val);
    assert!(out.contains("notes/foo.md"));
    assert!(out.contains("2024-01-15"));
    // Should not look like generic fallback
    assert!(!out.contains("file: notes/foo.md"));
}

#[test]
fn file_object_text_rendering_full() {
    let val = json!({
        "file": "notes/project.md",
        "modified": "2024-03-01",
        "tags": ["rust", "work"],
        "properties": {"status": "active"},
        "tasks": [
            {"done": false, "line": 10, "section": "Todo", "status": " ", "text": "Fix bug"},
            {"done": true, "line": 20, "section": "Done", "status": "x", "text": "Write docs"}
        ]
    });
    let out = fmt(&val);
    assert!(out.contains("notes/project.md"));
    assert!(out.contains("properties:"));
    assert!(out.contains("status: active"));
    assert!(out.contains("tags: [rust, work]"));
    assert!(out.contains("tasks:"));
    assert!(out.contains("[ ] Fix bug"));
    assert!(out.contains("[x] Write docs"));
}

// --- Array of FileObjects with blank-line separator ---

#[test]
fn array_of_file_objects_uses_blank_line_separator() {
    let val = json!([
        {"file": "a.md", "modified": "2024-01-01"},
        {"file": "b.md", "modified": "2024-01-02"}
    ]);
    let out = fmt(&val);
    assert!(out.contains("a.md"));
    assert!(out.contains("b.md"));
    // Should have a blank line between entries
    assert!(
        out.contains("\n\n"),
        "expected blank-line separator between file objects"
    );
}

#[test]
fn array_of_non_file_objects_uses_single_newline() {
    let val = json!([
        {"count": 1, "name": "status", "type": "text"},
        {"count": 3, "name": "title", "type": "text"}
    ]);
    let out = fmt(&val);
    assert!(out.contains("status"));
    assert!(out.contains("title"));
    // Should NOT have a blank line separator
    assert!(
        !out.contains("\n\n"),
        "non-file-objects should use single newline"
    );
}

// --- format_scalar nested object delegation ---

#[test]
fn format_scalar_delegates_nested_objects() {
    // A nested object with a known shape should get its filter applied,
    // not the k=v flat format.
    let inner = json!({"count": 2, "name": "status", "type": "text"});
    let out = scalar(&inner);
    // Should NOT look like the old "count=2, name=status, type=text" format.
    assert!(
        !out.contains("count=2"),
        "should delegate to format_value_as_text"
    );
    // Should look like the PropertySummaryEntry filter output.
    assert!(out.contains("status"));
    assert!(out.contains("2 files"));
}

// --- format_value_as_text integration ---

#[test]
fn format_value_as_text_uses_filter_for_known_shape() {
    // PropertySummaryEntry has a known shape: {count, name, type}
    let val = json!({"count": 3, "name": "status", "type": "text"});
    let out = fmt(&val);
    assert!(out.contains("status"));
    assert!(out.contains("3 files"));
    // Should NOT look like "count: 3" (that's the generic fallback)
    assert!(!out.contains("count: 3"));
}

#[test]
fn format_value_as_text_falls_back_for_unknown_shape() {
    let val = json!({"foo": "bar", "baz": 42});
    let out = fmt(&val);
    // Generic fallback: key: value
    assert!(out.contains("foo: bar") || out.contains("baz: 42"));
}

#[test]
fn mv_result_filter_applied() {
    let val = json!({
        "dry_run": false,
        "from": "sub/b.md",
        "to": "archive/b.md",
        "total_files_updated": 1,
        "total_links_updated": 1,
        "updated_files": [
            {
                "file": "a.md",
                "replacements": [
                    {"old_text": "[[sub/b]]", "new_text": "[[archive/b]]", "line": 1}
                ]
            }
        ]
    });
    // Verify key signature matches expected
    let sig = {
        let map = val.as_object().unwrap();
        let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
        keys.sort_unstable();
        keys.join(",")
    };
    assert_eq!(
        sig,
        "dry_run,from,to,total_files_updated,total_links_updated,updated_files"
    );
    // Verify the jq filter itself works
    let filter_result = apply_jq_filter_result(MV_RESULT_FILTER, &val);
    assert!(filter_result.is_ok(), "filter error: {filter_result:?}");
    let out = filter_result.unwrap();
    assert!(out.contains("Moved sub/b.md"), "out: {out}");
    assert!(out.contains("archive/b.md"), "out: {out}");
    assert!(out.contains("[[sub/b]]"), "out: {out}");
    assert!(out.contains("[[archive/b]]"), "out: {out}");
    // Verify lookup_filter finds the filter for this shape
    let found_filter =
        lookup_filter("dry_run,from,to,total_files_updated,total_links_updated,updated_files");
    assert!(
        found_filter.is_some(),
        "lookup_filter returned None for MvResult shape"
    );
    // format_value_as_text should pick up the filter
    let formatted = fmt(&val);
    assert!(
        formatted.contains("Moved sub/b.md"),
        "formatted: {formatted}"
    );
}

#[test]
fn format_value_as_text_array_of_typed_objects() {
    let val = json!([
        {"path": "a.md", "tags": ["rust"]},
        {"path": "b.md", "tags": ["cli"]}
    ]);
    let out = fmt(&val);
    assert!(out.contains("a.md"));
    assert!(out.contains("b.md"));
    assert!(out.contains("rust"));
    assert!(out.contains("cli"));
}

// --- sanitize_control_chars ---

#[test]
fn sanitize_control_chars_strips_escape_sequences() {
    let input = "Hello\x1b[31mRED\x1b[0m World";
    let output = sanitize_control_chars(input);
    assert!(
        !output.contains('\x1b'),
        "escape sequences should be stripped"
    );
    assert!(output.contains("Hello"));
    assert!(output.contains("RED"));
    assert!(output.contains("World"));
}

#[test]
fn sanitize_control_chars_preserves_newline_and_tab() {
    let input = "line1\nline2\ttabbed";
    let output = sanitize_control_chars(input);
    assert_eq!(output, input);
}

#[test]
fn text_output_sanitizes_escape_sequences() {
    let value = serde_json::json!({
        "results": {
            "title": "Hello\x1b[31mRED\x1b[0m World",
            "file": "test\x1b[2J.md"
        }
    });
    let output = format_success(Format::Text, &value);
    assert!(
        !output.contains('\x1b'),
        "escape sequences should be stripped"
    );
    assert!(output.contains("Hello") && output.contains("World"));
}

// --- UX-6: lint-rules mutation text renderer ---

/// `lint-rules set` with enabled change renders before→after and config path.
#[test]
fn lint_rules_set_enabled_change_renders_diff() {
    let value = serde_json::json!({
        "action": "set",
        "rule_id": "MD013",
        "dry_run": false,
        "before": {"enabled": true, "severity": "warn"},
        "after": {"enabled": false, "severity": "warn"},
        "config_path": ".hyalo.toml"
    });
    let out = format_success(Format::Text, &value);
    assert!(out.contains("MD013:"), "should include rule id");
    assert!(
        out.contains("on") && out.contains("off"),
        "should show enabled change"
    );
    assert!(out.contains("wrote .hyalo.toml"), "should mention write");
}

/// `lint-rules set` with severity change only.
#[test]
fn lint_rules_set_severity_change_renders_diff() {
    let value = serde_json::json!({
        "action": "set",
        "rule_id": "HYALO001",
        "dry_run": false,
        "before": {"enabled": true, "severity": "warn"},
        "after": {"enabled": true, "severity": "error"},
        "config_path": ".hyalo.toml"
    });
    let out = format_success(Format::Text, &value);
    assert!(out.contains("HYALO001:"));
    assert!(out.contains("warn") && out.contains("error"));
    assert!(out.contains("wrote .hyalo.toml"));
}

/// `lint-rules set` with dry-run says "dry-run, would write".
#[test]
fn lint_rules_set_dry_run_says_would_write() {
    let value = serde_json::json!({
        "action": "set",
        "rule_id": "MD013",
        "dry_run": true,
        "before": {"enabled": true, "severity": "warn"},
        "after": {"enabled": false, "severity": "warn"},
        "config_path": ".hyalo.toml"
    });
    let out = format_success(Format::Text, &value);
    assert!(out.contains("dry-run"), "should mention dry-run");
    assert!(!out.contains("wrote "), "should not say wrote");
}

/// `lint-rules remove` with a removed override shows reverted state.
#[test]
fn lint_rules_remove_shows_reverted_state() {
    let value = serde_json::json!({
        "action": "remove",
        "rule_id": "MD013",
        "dry_run": false,
        "removed": true,
        "before": {"enabled": false, "severity": "warn"},
        "after": {"enabled": true, "severity": "warn"},
        "config_path": ".hyalo.toml"
    });
    let out = format_success(Format::Text, &value);
    assert!(out.contains("MD013:"));
    assert!(out.contains("removed override"));
    assert!(out.contains("wrote .hyalo.toml"));
}

/// `lint-rules remove` when nothing to remove shows no-op message.
#[test]
fn lint_rules_remove_noop_shows_reason() {
    let value = serde_json::json!({
        "action": "remove",
        "rule_id": "MD013",
        "dry_run": false,
        "removed": false,
        "reason": "no override found",
        "before": {"enabled": true, "severity": "warn"},
        "after": {"enabled": true, "severity": "warn"},
        "config_path": ".hyalo.toml"
    });
    let out = format_success(Format::Text, &value);
    assert!(out.contains("MD013:"));
    assert!(out.contains("no override to remove") || out.contains("no override found"));
}

// --- build_envelope_value: NEW-5 dir dedup ---

#[test]
fn envelope_hoists_dir_and_removes_from_results() {
    let results = json!({
        "dir": "hyalo-knowledgebase",
        "files": {"total": 10},
        "other": "value"
    });
    let envelope = build_envelope_value(&results, None, &[]);
    // dir is hoisted to top level.
    assert_eq!(
        envelope["dir"].as_str().unwrap(),
        "hyalo-knowledgebase",
        "dir must be at envelope root"
    );
    // dir is removed from results.
    assert!(
        envelope["results"]
            .get("dir")
            .is_none_or(serde_json::Value::is_null),
        "dir must be removed from results; envelope: {envelope}"
    );
    // Other results fields are preserved.
    assert_eq!(envelope["results"]["files"]["total"].as_u64().unwrap(), 10);
    assert_eq!(envelope["results"]["other"].as_str().unwrap(), "value");
}

#[test]
fn envelope_without_dir_in_results_has_no_top_dir() {
    let results = json!({"files": {"total": 5}});
    let envelope = build_envelope_value(&results, None, &[]);
    assert!(
        envelope.get("dir").is_none() || envelope["dir"].is_null(),
        "no dir should be at envelope root when results has none"
    );
}

// --- iteration 254: the header degrades to whatever the projection kept ---

#[test]
fn file_object_header_carries_only_the_present_metadata_keys() {
    for (payload, expected) in [
        (
            json!({"file": "a.md", "modified": "2024-01-01", "size": 12, "lines": 3}),
            "\"a.md\"  (2024-01-01, 12 B, 3 lines)",
        ),
        (
            json!({"file": "a.md", "modified": "2024-01-01"}),
            "\"a.md\"  (2024-01-01)",
        ),
        (json!({"file": "a.md", "size": 12}), "\"a.md\"  (12 B)"),
        (
            json!({"file": "a.md", "lines": 3, "size": 12}),
            "\"a.md\"  (12 B, 3 lines)",
        ),
        // An exact projection can leave nothing but the path.
        (json!({"file": "a.md"}), "\"a.md\""),
    ] {
        let map = payload.as_object().unwrap().clone();
        let filter = build_file_object_filter(&map);
        assert_eq!(jq(&filter, &payload).unwrap(), expected, "for {payload}");
    }
}

#[test]
fn a_projected_file_object_still_renders_its_optional_sections() {
    // `--fields title` yields `{file, title}`: no metadata group, but the
    // title line must still appear under the bare path.
    let payload = json!({"file": "a.md", "title": "Alpha"});
    let map = payload.as_object().unwrap().clone();
    let filter = build_file_object_filter(&map);
    assert_eq!(jq(&filter, &payload).unwrap(), "\"a.md\"\n  title: Alpha");
}
