//! Iteration 323: a command answers the same on disk and under `--index`, an
//! incremental `create-index` holds what `--force` holds, and hyalo neither
//! falls over nor lies on awkward input (DEC-389 to DEC-391).

use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

fn run(dir: &Path, args: &[&str]) -> (Value, String, i32) {
    let out = hyalo_no_hints()
        .arg("--dir")
        .arg(dir)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    (
        serde_json::from_slice(&out.stdout).unwrap_or(Value::Null),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Longer than the staleness tolerance, so a change made after `create-index`
/// is visible to the directory-mtime probe.
fn outlive_tolerance() {
    std::thread::sleep(Duration::from_millis(2200));
}

fn files(dir: &Path, extra: &[&str]) -> Vec<String> {
    let mut args = vec!["find", "--fields", "file", "--limit", "0"];
    args.extend_from_slice(extra);
    let (v, _, _) = run(dir, &args);
    v["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["file"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a5_named_read_repairs_the_link_graph_in_memory() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "a/t.md", "# T\n");
    write_md(d, "a/s1.md", "[[t]]\n");
    run(d, &["create-index"]);
    outlive_tolerance();
    fs::remove_file(d.join("a/s1.md")).unwrap();
    write_md(d, "a/s2.md", "[[t]]\n");
    for args in [
        vec!["backlinks", "a/t.md", "--index"],
        vec![
            "find",
            "--file",
            "a/t.md",
            "--fields",
            "backlinks",
            "--index",
        ],
    ] {
        let (v, err, _) = run(d, &args);
        let text = v.to_string();
        assert!(text.contains("a/s2.md"), "{args:?}: {text}");
        assert!(!text.contains("a/s1.md"), "{args:?}: {text}");
        assert!(err.contains("repaired in memory"), "{args:?}: {err}");
    }
}

#[test]
fn a6_frontmatter_links_off_agrees_on_disk_and_under_index() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    fs::write(d.join(".hyalo.toml"), "[links]\nfrontmatter = false\n").unwrap();
    write_md(d, "src.md", "---\nother: \"[[target]]\"\n---\nbody\n");
    write_md(d, "target.md", "# T\n");
    run(d, &["create-index"]);
    for cmd in [vec!["backlinks", "target.md"], vec!["summary"]] {
        let mut disk = run(d, &cmd).0["results"].clone();
        let mut indexed = cmd.clone();
        indexed.push("--index");
        let mut idx = run(d, &indexed).0["results"].clone();
        // The fields that document where the answer came from.
        for key in ["source", "index_format_version", "code_blocks"] {
            if let Some(map) = disk.as_object_mut() {
                map.remove(key);
            }
            if let Some(map) = idx.as_object_mut() {
                map.remove(key);
            }
        }
        assert_eq!(disk, idx, "{cmd:?}");
    }
    assert_eq!(run(d, &["backlinks", "target.md"]).0["total"], 0);
    // The snapshot records the setting it was built under: another config is
    // refused rather than served.
    fs::write(d.join(".hyalo.toml"), "").unwrap();
    let (_, err, _) = run(d, &["summary", "--index"]);
    assert!(err.contains("`[links] frontmatter`"), "{err}");
}

#[test]
fn a12_a_note_fixed_in_place_after_an_unparsable_build_shows_up() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "ok.md", "x\n");
    write_md(d, "bad.md", "---\na: [\n---\nx\n");
    run(d, &["create-index"]);
    outlive_tolerance();
    write_md(d, "bad.md", "---\na: 1\n---\nx\n");
    assert_eq!(files(d, &["--index"]), ["bad.md", "ok.md"]);
}

#[test]
fn a18_a_newer_snapshot_is_refused_and_not_reused() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "a.md", "# a\n");
    run(d, &["create-index"]);
    let path = d.join(".hyalo-index");
    let mut snapshot: Value = rmp_serde::from_slice(&fs::read(&path).unwrap()).unwrap();
    let current = snapshot["header"]["format_version"].as_u64().unwrap();
    snapshot["header"]["format_version"] = (current + 1).into();
    fs::write(&path, rmp_serde::to_vec_named(&snapshot).unwrap()).unwrap();
    let (summary, err, _) = run(d, &["summary", "--index"]);
    assert!(err.contains("newer than this binary"), "{err}");
    assert_eq!(summary["results"]["source"], "disk", "{summary}");
    let (created, _, _) = run(d, &["create-index"]);
    assert_eq!(created["results"]["reused"], 0, "{created}");
}

#[test]
fn c16_a_note_added_in_an_included_hidden_directory_moves_the_probe() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    fs::write(
        d.join(".hyalo.toml"),
        "[scan]\ninclude = [\".claude/skills/**\"]\n",
    )
    .unwrap();
    write_md(d, ".claude/skills/s1.md", "# s1\n");
    write_md(d, "a.md", "# a\n");
    run(d, &["create-index"]);
    outlive_tolerance();
    write_md(d, ".claude/skills/s2.md", "# s2\n");
    assert_eq!(
        files(d, &["--index"]),
        [".claude/skills/s1.md", ".claude/skills/s2.md", "a.md"]
    );
}

#[test]
fn a8_a16_a17_frontmatter_comments_survive_writes() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "last.md", "---\n# keep\nk: 1\n---\nb\n");
    run(d, &["remove", "last.md", "--property", "k"]);
    assert_eq!(
        fs::read_to_string(d.join("last.md")).unwrap(),
        "---\n# keep\n---\nb\n"
    );

    write_md(d, "tpl.md", "---\n# template comment\n---\nb\n");
    run(d, &["set", "tpl.md", "--property", "k=v"]);
    assert_eq!(
        fs::read_to_string(d.join("tpl.md")).unwrap(),
        "---\n# template comment\nk: v\n---\nb\n"
    );

    write_md(
        d,
        "u.md",
        "---\nstatus: single # keep\ntags:\n  - one\n  - three # c3\n---\n",
    );
    run(d, &["set", "u.md", "--property", "status=done"]);
    run(d, &["tags", "rename", "--from", "three", "--to", "four"]);
    assert_eq!(
        fs::read_to_string(d.join("u.md")).unwrap(),
        "---\nstatus: done # keep\ntags:\n  - one\n  - four # c3\n---\n"
    );

    write_md(d, "flow.md", "---\ntags: [one, \"two\", ONE]\n---\n");
    run(d, &["tags", "rename", "--from", "one", "--to", "two"]);
    let flow = fs::read_to_string(d.join("flow.md")).unwrap();
    assert!(flow.starts_with("---\ntags: ["), "{flow}");
}

#[test]
fn a9_a14_init_refuses_a_broken_config_and_reports_an_identical_one_unchanged() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    let broken = "# mine\ndir = \"docs\"\n[schema.types.note]\nrequired = [\"title\"\n";
    fs::write(d.join(".hyalo.toml"), broken).unwrap();
    let out = hyalo_no_hints()
        .current_dir(d)
        .args(["init", "--dir", "docs"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(fs::read_to_string(d.join(".hyalo.toml")).unwrap(), broken);
    assert!(!d.join("docs").exists(), "a refusal creates nothing");

    fs::remove_file(d.join(".hyalo.toml")).unwrap();
    for expected in ["created  .hyalo.toml", "unchanged  .hyalo.toml"] {
        let out = hyalo_no_hints()
            .current_dir(d)
            .args(["init", "--dir", "docs"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains(expected), "{text}");
    }
}

#[test]
fn a10_a_changelog_or_madr_dry_run_exits_zero_and_reports_drift_in_the_payload() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(
        d,
        "CHANGELOG.md",
        "# Changelog\n\n## [Unreleased]\n\n### Added\n\n- thing\n",
    );
    let (v, _, code) = run(
        d,
        &["changelog", "release", "1.0.0", "--date", "2026-02-03"],
    );
    assert_eq!(code, 0);
    assert_eq!(v["results"]["changed"], true);
    let (_, _, code) = run(
        d,
        &["changelog", "add", "--category", "Added", "--message", "m"],
    );
    assert_eq!(code, 0);
}

#[test]
fn a11_append_onto_a_mapping_is_a_user_error() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "a.md", "---\nm: {a: 1}\n---\n");
    let (_, _, code) = run(d, &["append", "a.md", "--property", "m=1"]);
    assert_eq!(code, 1);
}

#[test]
fn l5_an_invalid_lint_severity_makes_the_config_unusable() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    fs::write(
        d.join(".hyalo.toml"),
        "[lint.rules.MD009]\nseverity = \"eror\"\n",
    )
    .unwrap();
    write_md(d, "a.md", "# T\n\ntrailing   \n");
    let (_, err, code) = run(d, &["lint"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("severity must be"), "{err}");
}

#[test]
fn l6_leading_empty_strings_keep_their_lines() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "x\n");
    let out = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--limit", "1", "--jq", r#""", "", "a", "b""#])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "\n\na\nb\n");
}

#[test]
fn l14_new_requires_a_note_name() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    fs::write(
        d.join(".hyalo.toml"),
        "[schema.types.note]\nrequired = [\"title\"]\n",
    )
    .unwrap();
    for bad in ["sub/n", ".md", "x.MD", "sub/"] {
        let (_, _, code) = run(d, &["new", "--type", "note", "--file", bad]);
        assert_eq!(code, 1, "{bad}");
    }
    #[cfg(unix)]
    {
        let (_, _, code) = run(d, &["new", "--type", "note", "--file", "sub\\w.md"]);
        assert_eq!(code, 0);
        assert!(
            d.join("sub\\w.md").is_file(),
            "a backslash is a name byte off Windows"
        );
    }
}

#[test]
fn l15_a_failure_that_wrote_nothing_lists_no_effects() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    fs::write(d.join(".hyalo.toml"), "dir = [\n").unwrap();
    let out = hyalo_no_hints()
        .current_dir(d)
        .args(["init", "--profile", "okf", "--format", "text"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("cause:"), "{err}");
    assert!(!err.contains("Effects:"), "{err}");
}

#[test]
fn l16_changelog_release_validates_date_and_order() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(
        d,
        "CHANGELOG.md",
        "# Changelog\n\n## [Unreleased]\n\n### Added\n\n- thing\n\n## [1.0.0] - 2026-01-01\n",
    );
    let (_, _, code) = run(
        d,
        &["changelog", "release", "1.1.0", "--date", "2026-13-45"],
    );
    assert_eq!(code, 1);
    let (_, _, code) = run(d, &["changelog", "release", "0.5.0"]);
    assert_eq!(code, 1);
    let (_, _, code) = run(
        d,
        &[
            "changelog",
            "add",
            "--category",
            "Added",
            "--message",
            "ok\n## [9.9.9] - 2026-02-02",
        ],
    );
    assert_eq!(code, 1);
}

#[test]
fn a20_a_fenced_example_is_not_the_changelog_footer() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(
        d,
        "CHANGELOG.md",
        "# Changelog\n\n## [Unreleased]\n\n- x\n\n```\n[Unreleased]: https://h/o/r/compare/v0.1.0...HEAD\n```\n\n[Unreleased]: https://h/o/r/compare/v1.0.0...HEAD\n[1.0.0]: https://h/o/r/compare/v0.9.0...v1.0.0\n",
    );
    run(d, &["changelog", "release", "1.1.0", "--apply"]);
    let text = fs::read_to_string(d.join("CHANGELOG.md")).unwrap();
    assert!(
        text.contains("```\n[Unreleased]: https://h/o/r/compare/v0.1.0...HEAD\n```"),
        "{text}"
    );
    assert!(
        text.contains("[Unreleased]: https://h/o/r/compare/v1.1.0...HEAD"),
        "{text}"
    );
}

#[test]
fn c3_a_non_utf8_argument_is_an_error_not_a_panic() {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let tmp = TempDir::new().unwrap();
        let out = hyalo_no_hints()
            .arg("--dir")
            .arg(tmp.path())
            .arg("find")
            .arg(std::ffi::OsStr::from_bytes(b"\xff"))
            .output()
            .unwrap();
        assert!(
            out.status.code().is_some_and(|c| c <= 2),
            "no abort: {:?}",
            out.status
        );
    }
}

#[cfg(unix)]
#[test]
fn c4_a_created_file_follows_the_umask() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "a.md", "# a\n");
    let reference = d.join("reference");
    fs::File::create(&reference).unwrap();
    run(d, &["create-index"]);
    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&d.join(".hyalo-index")), mode(&reference));
}

#[test]
fn c5_a_bom_note_gets_its_first_block_after_the_bom_and_keeps_its_line_endings() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    fs::write(d.join("h.md"), b"\xef\xbb\xbf# My Title\r\ntext\r\n").unwrap();
    let (v, _, _) = run(d, &["find", "--file", "h.md", "--fields", "title"]);
    assert_eq!(v["results"][0]["title"], "My Title", "{v}");
    run(d, &["set", "h.md", "--property", "k=v"]);
    assert_eq!(
        fs::read(d.join("h.md")).unwrap(),
        b"\xef\xbb\xbf---\r\nk: v\r\n---\r\n# My Title\r\ntext\r\n"
    );
}

#[test]
fn c14_a_checkbox_needs_whitespace_after_the_bracket() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "t.md", "- [x]ample\n- [ ] real\n- [ ]\n");
    let (v, _, _) = run(d, &["find", "--file", "t.md", "--fields", "tasks"]);
    assert_eq!(v["results"][0]["tasks"].as_array().unwrap().len(), 2, "{v}");
}

#[test]
fn c17_a_regional_language_tag_picks_the_stemmer() {
    assert_eq!(
        hyalo_core::bm25::parse_language("de-CH").unwrap(),
        hyalo_core::bm25::parse_language("de").unwrap()
    );
    assert!(hyalo_core::bm25::parse_language("klingon").is_err());
}

#[test]
fn l7_deinit_keeps_a_config_that_holds_more_than_dir() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    fs::write(
        d.join(".hyalo.toml"),
        "dir = \".\"\n[views.todo]\nstatus = \"x\"\n",
    )
    .unwrap();
    let out = hyalo_no_hints()
        .current_dir(d)
        .args(["deinit", "--format", "text"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("skipped  .hyalo.toml"), "{text}");
    assert!(d.join(".hyalo.toml").is_file());
}

#[test]
fn m2_a_relative_wikilink_stays_relative_after_the_note_moves() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "t.md", "# t\n");
    write_md(d, "sub/sib.md", "# sib\n");
    write_md(
        d,
        "sub/src.md",
        "---\nrel: \"[[../t]]\"\n---\n[[../t]] [[./sib]] [[../t#H]] [[../t|al]]\n",
    );
    run(d, &["mv", "sub/src.md", "--to", "deep/er/src.md"]);
    assert_eq!(
        fs::read_to_string(d.join("deep/er/src.md")).unwrap(),
        "---\nrel: \"[[../../t]]\"\n---\n[[../../t]] [[../../sub/sib]] [[../../t#H]] [[../../t|al]]\n"
    );
}
