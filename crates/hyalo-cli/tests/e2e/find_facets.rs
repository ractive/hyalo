//! `find --facet SPEC` (DEC-335): per-value file counts over the full match
//! set, computed before `--limit`.

use super::common::{hyalo, hyalo_no_hints, write_md};
use tempfile::TempDir;

fn run(tmp: &TempDir, args: &[&str]) -> (serde_json::Value, std::process::Output) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(args)
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
    (json, output)
}

fn buckets(json: &serde_json::Value, facet_index: usize) -> Vec<(Option<&str>, u64)> {
    json["facets"][facet_index]["buckets"]
        .as_array()
        .unwrap_or_else(|| panic!("no buckets in {json}"))
        .iter()
        .map(|b| (b["value"].as_str(), b["count"].as_u64().unwrap_or_default()))
        .collect()
}

/// Six files exercising every bucket shape at once:
/// - tags: exact-bucket tags (`project/backend` never folds into `project`),
///   plus two files with no tags at all (the null bucket)
/// - property:status: a plain scalar, a missing key (null bucket), and a list
///   property whose elements each count on their own
/// - dir: two root files' siblings plus a nested `notes/` and `notes/sub/`
///   pair, both of which bucket under the top-level `notes` segment
fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntags: [rust, project/backend]\nstatus: done\n---\nbody\n",
    );
    write_md(
        tmp.path(),
        "b.md",
        "---\ntags: [rust]\nstatus: done\n---\nbody\n",
    );
    write_md(
        tmp.path(),
        "c.md",
        "---\ntags: [project/backend]\nstatus: planned\n---\nbody\n",
    );
    write_md(tmp.path(), "d.md", "---\ntags: []\n---\nbody\n");
    write_md(
        tmp.path(),
        "notes/e.md",
        "---\ntags: [cli]\nstatus: [planned, done]\n---\nbody\n",
    );
    write_md(
        tmp.path(),
        "notes/sub/f.md",
        "---\ntags: []\nstatus: archived\n---\nbody\n",
    );
    tmp
}

#[test]
fn tags_facet_buckets_count_desc_then_value_with_null_last_on_ties() {
    let tmp = vault();
    let (json, output) = run(&tmp, &["find", "--facet", "tags", "--format", "json"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["facets"][0]["facet"], "tags");
    assert_eq!(
        buckets(&json, 0),
        vec![
            (Some("project/backend"), 2),
            (Some("rust"), 2),
            (None, 2),
            (Some("cli"), 1),
        ]
    );
    assert_eq!(json["facets"][0]["truncated"], false);
}

#[test]
fn property_facet_counts_every_list_element_and_a_null_bucket() {
    let tmp = vault();
    let (json, output) = run(
        &tmp,
        &["find", "--facet", "property:status", "--format", "json"],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        buckets(&json, 0),
        vec![
            (Some("done"), 3),
            (Some("planned"), 2),
            (Some("archived"), 1),
            (None, 1),
        ]
    );
}

#[test]
fn dir_facet_uses_top_level_segment_including_root() {
    let tmp = vault();
    let (json, output) = run(&tmp, &["find", "--facet", "dir", "--format", "json"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(buckets(&json, 0), vec![(Some("."), 4), (Some("notes"), 2)]);
}

#[test]
fn type_is_an_alias_of_property_type_but_keeps_its_own_label() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntype: Note\n---\nbody\n");
    write_md(tmp.path(), "b.md", "---\ntype: Note\n---\nbody\n");
    write_md(tmp.path(), "c.md", "---\ntype: Guide\n---\nbody\n");

    let (type_json, out1) = run(&tmp, &["find", "--facet", "type", "--format", "json"]);
    assert!(out1.status.success(), "{out1:?}");
    let (prop_json, out2) = run(
        &tmp,
        &["find", "--facet", "property:type", "--format", "json"],
    );
    assert!(out2.status.success(), "{out2:?}");

    assert_eq!(type_json["facets"][0]["facet"], "type");
    assert_eq!(prop_json["facets"][0]["facet"], "property:type");
    assert_eq!(buckets(&type_json, 0), buckets(&prop_json, 0));
    assert_eq!(
        buckets(&type_json, 0),
        vec![(Some("Note"), 2), (Some("Guide"), 1)]
    );
}

#[test]
fn buckets_cap_at_fifty_with_truncated_flag() {
    let tmp = TempDir::new().unwrap();
    for i in 0..55 {
        write_md(
            tmp.path(),
            &format!("f{i}.md"),
            &format!("---\nid: v{i}\n---\nbody\n"),
        );
    }
    let (json, output) = run(
        &tmp,
        &["find", "--facet", "property:id", "--format", "json"],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["facets"][0]["buckets"].as_array().unwrap().len(), 50);
    assert_eq!(json["facets"][0]["truncated"], true);
    assert_eq!(json["total"], 55);
}

#[test]
fn facets_are_computed_before_limit_cuts_the_result_set() {
    let tmp = TempDir::new().unwrap();
    for i in 0..5 {
        write_md(
            tmp.path(),
            &format!("f{i}.md"),
            &format!("---\ntags: [grp{i}]\n---\nzephyr100 body.\n"),
        );
    }
    let (json, output) = run(
        &tmp,
        &[
            "find",
            "zephyr100",
            "--facet",
            "tags",
            "--limit",
            "1",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"].as_array().unwrap().len(), 1);
    assert_eq!(json["total"], 5);
    let total_bucketed: u64 = json["facets"][0]["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["count"].as_u64().unwrap())
        .sum();
    assert_eq!(total_bucketed, 5);
}

#[test]
fn count_with_facet_prints_only_the_total() {
    let tmp = vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--facet", "tags", "--count"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "6", "got: {stdout:?}");
}

#[test]
fn jq_can_extract_the_facets_array() {
    let tmp = vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["--jq", ".facets[0].facet"])
        .args(["find", "--facet", "tags"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tags"), "got: {stdout:?}");
}

#[test]
fn facets_work_on_a_filter_only_query() {
    let tmp = vault();
    let (json, output) = run(
        &tmp,
        &[
            "find",
            "--property",
            "status=done",
            "--facet",
            "tags",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    // Only a.md, b.md, e.md have status=done.
    assert_eq!(
        buckets(&json, 0),
        vec![
            (Some("rust"), 2),
            (Some("cli"), 1),
            (Some("project/backend"), 1)
        ]
    );
}

#[test]
fn facets_work_on_broken_links() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "broken.md",
        "---\ntags: [brk]\n---\n[[does-not-exist]]\n",
    );
    write_md(
        tmp.path(),
        "healthy.md",
        "---\ntags: [ok]\n---\nno links here.\n",
    );

    let (json, output) = run(
        &tmp,
        &[
            "find",
            "--broken-links",
            "--facet",
            "tags",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(buckets(&json, 0), vec![(Some("brk"), 1)]);
}

#[test]
fn unknown_facet_spec_exits_1() {
    let tmp = vault();
    let (_, output) = run(&tmp, &["find", "--facet", "bogus"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(
        json["error"].as_str().unwrap().contains("unknown facet"),
        "{json}"
    );
}

#[test]
fn empty_property_key_exits_1() {
    let tmp = vault();
    let (_, output) = run(&tmp, &["find", "--facet", "property:"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(
        json["error"].as_str().unwrap().contains("needs a key"),
        "{json}"
    );
}

#[test]
fn text_mode_prints_a_facet_block_after_results() {
    let tmp = vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--facet", "dir", "--format", "text"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("facet dir:"), "{stdout:?}");
    assert!(stdout.contains('.'), "{stdout:?}");
    assert!(stdout.contains("notes"), "{stdout:?}");
}

#[test]
fn drilldown_hints_target_the_largest_buckets_and_skip_whole_match_or_root() {
    let tmp = vault();
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "--facet",
            "tags",
            "--facet",
            "property:status",
            "--facet",
            "dir",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let cmds: Vec<&str> = json["hints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["cmd"].as_str().unwrap())
        .collect();
    assert!(
        cmds.iter()
            .any(|c| c.contains("--tag") && c.contains("project/backend")),
        "{cmds:?}"
    );
    assert!(
        cmds.iter()
            .any(|c| c.contains("--property") && c.contains("status=done")),
        "{cmds:?}"
    );
    assert!(
        cmds.iter()
            .any(|c| c.contains("--glob") && c.contains("notes/**")),
        "{cmds:?}"
    );
    // The root dir bucket "." never drills down (it narrows nothing useful).
    assert!(
        !cmds
            .iter()
            .any(|c| c.contains("--glob") && c.contains("'./**'")),
        "{cmds:?}"
    );
}

#[test]
fn no_facet_flag_means_no_facets_key() {
    let tmp = vault();
    let (json, output) = run(&tmp, &["find", "--format", "json"]);
    assert!(output.status.success(), "{output:?}");
    assert!(json.get("facets").is_none(), "{json}");
}

#[test]
fn section_mode_facet_counts_files_not_sections() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "multi.md",
        "---\ntags: [proj]\n---\n# A\n\nyak123 one.\n\n# B\n\nyak123 two.\n",
    );
    write_md(
        tmp.path(),
        "single.md",
        "---\ntags: [proj]\n---\n# C\n\nyak123 lonely.\n",
    );

    let (json, output) = run(
        &tmp,
        &[
            "find",
            "yak123",
            "--granularity",
            "section",
            "--facet",
            "tags",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    // Three section hits total (A, B, C)...
    assert_eq!(json["total"], 3);
    // ...but the facet counts files with at least one hit: two, not three.
    assert_eq!(buckets(&json, 0), vec![(Some("proj"), 2)]);
}
