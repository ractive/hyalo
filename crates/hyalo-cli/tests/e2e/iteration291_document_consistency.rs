use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

fn run_json(tmp: &TempDir, args: &[&str]) -> serde_json::Value {
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn twenty_kib_frontmatter_agrees_across_disk_index_and_direct_read() {
    let tmp = TempDir::new().unwrap();
    let prefix = "---\ntitle: Boundary\npad: \"";
    let padding = "x".repeat(16_384 - prefix.len() - 1);
    let value = format!("{padding}é{}", "y".repeat(4_200));
    let document = format!("{prefix}{value}\"\n---   \n# Visible\nbody sentinel\n");
    assert!(document.find('é').is_some_and(|at| at < 16_384));
    assert!(document.find("---   \n").is_some_and(|at| at > 20_000));
    write_md(tmp.path(), "boundary.md", &document);

    let disk = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "boundary.md",
            "--fields",
            "properties",
            "--format",
            "json",
        ],
    );
    assert_eq!(disk["results"][0]["properties"]["pad"], value);
    run_json(&tmp, &["create-index"]);
    let indexed = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "boundary.md",
            "--fields",
            "properties",
            "--index",
            "--format",
            "json",
        ],
    );
    assert_eq!(indexed["results"], disk["results"]);
    let read = run_json(
        &tmp,
        &[
            "read",
            "--file",
            "boundary.md",
            "--frontmatter",
            "--format",
            "json",
        ],
    );
    assert_eq!(read["results"]["frontmatter"]["pad"], value);
    let body = run_json(&tmp, &["read", "--file", "boundary.md", "--format", "json"]);
    assert!(
        body["results"]["content"]
            .as_str()
            .unwrap()
            .contains("body sentinel")
    );
    let full_text = run_json(
        &tmp,
        &[
            "find",
            "--regexp",
            "body sentinel",
            "--file",
            "boundary.md",
            "--format",
            "json",
        ],
    );
    assert_eq!(full_text["total"], 1);

    let closing_prefix = "---\ntitle: Closing\npad: \"";
    let closing_value = "z".repeat(16_383 - closing_prefix.len() - 2);
    let closing = format!("{closing_prefix}{closing_value}\"\n---\nbody\n");
    let closing_at = closing.rfind("---\nbody").unwrap();
    assert!((16_382..=16_386).contains(&closing_at));
    write_md(tmp.path(), "closing.md", &closing);
    let split = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "closing.md",
            "--fields",
            "properties",
            "--format",
            "json",
        ],
    );
    assert_eq!(split["results"][0]["properties"]["pad"], closing_value);
}

#[test]
fn hidden_comment_structure_is_absent_in_disk_index_read_and_task() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "hidden.md",
        "<!--\n# Hidden promoted title\n- [ ] hidden task\n-->\nplain literal needle\n",
    );
    for indexed in [false, true] {
        if indexed {
            run_json(&tmp, &["create-index"]);
        }
        let mut args = vec![
            "find",
            "--file",
            "hidden.md",
            "--fields",
            "title,sections,tasks",
            "--format",
            "json",
        ];
        if indexed {
            args.push("--index");
        }
        let found = run_json(&tmp, &args);
        let item = &found["results"][0];
        assert_eq!(item["title"], "hidden");
        assert_eq!(item["sections"], serde_json::json!([]));
        assert_eq!(item["tasks"], serde_json::json!([]));
    }
    let literal = run_json(
        &tmp,
        &["find", "Hidden", "--file", "hidden.md", "--format", "json"],
    );
    assert_eq!(literal["total"], 1);

    let read = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "read",
            "--file",
            "hidden.md",
            "--section",
            "Hidden",
        ])
        .output()
        .unwrap();
    assert!(!read.status.success());
    assert_eq!(read.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&read.stderr).contains("section not found"));
    let task = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "task",
            "read",
            "--file",
            "hidden.md",
            "--line",
            "3",
        ])
        .output()
        .unwrap();
    assert!(!task.status.success());
    assert_eq!(task.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&task.stderr).contains("not a task"));
}

#[test]
fn type_scoped_lint_includes_one_digit_and_matches_explicit_lint() {
    let tmp = TempDir::new().unwrap();
    run_json(
        &tmp,
        &[
            "types",
            "set",
            "decision",
            "--filename-template",
            "decisions/{n}-{slug}.md",
        ],
    );
    for name in [
        "1-test.md",
        "12-test.md",
        "123-test.md",
        "001-test.md",
        "abc-test.md",
    ] {
        write_md(
            tmp.path(),
            &format!("decisions/{name}"),
            "# Title\ntrailing   \n",
        );
    }
    let scoped = run_json(
        &tmp,
        &[
            "lint", "--type", "decision", "--rule", "MD009", "--format", "json",
        ],
    );
    assert_eq!(scoped["results"]["files_checked"], 4);
    assert!(scoped["results"]["violations"].as_u64().unwrap() >= 4);
    let explicit = run_json(
        &tmp,
        &[
            "lint",
            "--file",
            "decisions/1-test.md",
            "--rule",
            "MD009",
            "--format",
            "json",
        ],
    );
    assert_eq!(explicit["results"]["files_checked"], 1);
    assert_eq!(explicit["results"]["violations"], 1);
    let scoped_one = scoped["results"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["file"] == "decisions/1-test.md")
        .expect("one-digit filename must carry its actual finding");
    assert_eq!(
        scoped_one["rule_groups"], explicit["results"]["files"][0]["rule_groups"],
        "type-scoped and explicitly targeted lint must report the same MD009 location"
    );
}

#[test]
fn lint_fix_preserves_yaml_tab_and_unicode_sections_agree_on_disk_and_index() {
    let tmp = TempDir::new().unwrap();
    let original = "---\n---note: \"a\tb\"\n---   \n# C#\n## Résumé\nbody\ttext\n";
    write_md(tmp.path(), "unicode.md", original);
    let fixed = run_json(
        &tmp,
        &[
            "lint",
            "--file",
            "unicode.md",
            "--fix",
            "--rule",
            "MD010",
            "--format",
            "json",
        ],
    );
    assert_eq!(fixed["results"]["total_fixed"], 1);
    let after = std::fs::read_to_string(tmp.path().join("unicode.md")).unwrap();
    assert!(after.contains("---note: \"a\tb\""));
    assert!(!after.contains("body\ttext"));

    for indexed in [false, true] {
        if indexed {
            run_json(&tmp, &["create-index"]);
        }
        let mut args = vec![
            "find",
            "--file",
            "unicode.md",
            "--section",
            "Résumé",
            "--fields",
            "title,sections",
            "--format",
            "json",
        ];
        if indexed {
            args.push("--index");
        }
        let found = run_json(&tmp, &args);
        assert_eq!(found["results"][0]["title"], "C#");
        assert_eq!(found["total"], 1);
    }
    let read = run_json(
        &tmp,
        &[
            "read",
            "--file",
            "unicode.md",
            "--section",
            "Résumé",
            "--format",
            "json",
        ],
    );
    assert!(
        read["results"]["content"]
            .as_str()
            .unwrap()
            .contains("## Résumé")
    );
}

#[test]
fn block_scalar_links_drive_backlinks_and_broken_checks_but_comments_do_not() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "Target.md", "# Target\n");
    write_md(
        tmp.path(),
        "source.md",
        "---\ndescription:\n  - |-\n    # [[Target]]\n# [[CommentOnly]]\n---\nbody\n",
    );
    let backlinks = run_json(&tmp, &["backlinks", "Target.md", "--format", "json"]);
    assert_eq!(backlinks["total"], 1);
    assert_eq!(backlinks["results"]["backlinks"][0]["source"], "source.md");
    let broken = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "source.md",
            "--broken-links",
            "--fields",
            "links",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        broken["total"], 0,
        "YAML comment must not create CommentOnly"
    );
    std::fs::remove_file(tmp.path().join("Target.md")).unwrap();
    let broken = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "source.md",
            "--broken-links",
            "--fields",
            "links",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        broken["total"], 1,
        "block scalar Target must participate in broken-link checks"
    );
}

#[test]
fn partial_comments_and_multiline_inline_code_are_inert_to_fix_and_read_structure() {
    let tmp = TempDir::new().unwrap();
    let original = "Example `\n[] literal\n` end\n<!--\n# Hidden --> visible\n# Visible\n";
    write_md(tmp.path(), "literal.md", original);
    let fixed = run_json(
        &tmp,
        &[
            "lint",
            "--file",
            "literal.md",
            "--fix",
            "--rule",
            "HYALO001",
            "--format",
            "json",
        ],
    );
    assert_eq!(fixed["results"]["total_fixed"], 0);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("literal.md")).unwrap(),
        original
    );

    let hidden = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "read",
            "--file",
            "literal.md",
            "--section",
            "Hidden",
        ])
        .output()
        .unwrap();
    assert_eq!(hidden.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&hidden.stderr).contains("section not found"));
    let visible = run_json(
        &tmp,
        &[
            "read",
            "--file",
            "literal.md",
            "--section",
            "Visible",
            "--format",
            "json",
        ],
    );
    assert!(
        visible["results"]["content"]
            .as_str()
            .unwrap()
            .contains("# Visible")
    );
}

#[test]
fn invalid_backtick_info_string_is_visible_in_read_disk_and_index() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "invalid-fence.md",
        "```not`a-fence\n# Visible\nbody\n",
    );
    run_json(
        &tmp,
        &[
            "read",
            "--file",
            "invalid-fence.md",
            "--section",
            "Visible",
            "--format",
            "json",
        ],
    );
    for indexed in [false, true] {
        if indexed {
            run_json(&tmp, &["create-index"]);
        }
        let mut args = vec![
            "find",
            "--file",
            "invalid-fence.md",
            "--fields",
            "title,sections",
            "--format",
            "json",
        ];
        if indexed {
            args.push("--index");
        }
        let found = run_json(&tmp, &args);
        assert_eq!(found["results"][0]["title"], "Visible");
        assert!(
            found["results"][0]["sections"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["heading"] == "Visible")
        );
    }
}

#[test]
fn percent_comment_fence_does_not_hide_later_mv_rewrite() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "old.md", "# Old\n");
    write_md(tmp.path(), "source.md", "%%\n```\n%%\n[[old]]\n");
    run_json(&tmp, &["mv", "old.md", "--to", "new.md"]);
    assert!(
        std::fs::read_to_string(tmp.path().join("source.md"))
            .unwrap()
            .contains("[[new]]")
    );
}

#[test]
fn type_glob_escapes_literal_metacharacters() {
    let tmp = TempDir::new().unwrap();
    run_json(
        &tmp,
        &[
            "types",
            "set",
            "decision",
            "--filename-template",
            "decisions/[draft]/{n}-{slug}.md",
        ],
    );
    write_md(
        tmp.path(),
        "decisions/[draft]/1-test.md",
        "# Title\ntrailing   \n",
    );
    let scoped = run_json(
        &tmp,
        &[
            "lint", "--type", "decision", "--rule", "MD009", "--format", "json",
        ],
    );
    let explicit = run_json(
        &tmp,
        &[
            "lint",
            "--file",
            "decisions/[draft]/1-test.md",
            "--rule",
            "MD009",
            "--format",
            "json",
        ],
    );
    assert_eq!(scoped["results"]["files_checked"], 1);
    assert_eq!(
        scoped["results"]["files"][0]["rule_groups"],
        explicit["results"]["files"][0]["rule_groups"]
    );
}

#[test]
fn set_preserves_exact_authored_frame_while_splicing() {
    let tmp = TempDir::new().unwrap();
    let original = "\u{feff}--- \t\r\ntitle: old\r\nkeep: val # keep-comment\r\n---  \r\nbody\r\n";
    write_md(tmp.path(), "exact.md", original);
    run_json(
        &tmp,
        &["set", "--file", "exact.md", "--property", "title=new"],
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("exact.md")).unwrap(),
        "\u{feff}--- \t\r\ntitle: new\r\nkeep: val # keep-comment\r\n---  \r\nbody\r\n"
    );
}

#[test]
fn mv_refuses_over_budget_frontmatter_rewrite_before_any_effect() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "# A\n");
    let link = "ref: \"[[a]]\"\n";
    let padding = "x".repeat(65_536 - link.len() - 2);
    let original = format!("---\n{link}#{padding}\n---\nbody\n");
    write_md(tmp.path(), "ref.md", &original);
    let moved = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "mv",
            "a.md",
            "--to",
            "longer-name.md",
        ])
        .output()
        .unwrap();
    assert!(
        !moved.status.success(),
        "{}",
        String::from_utf8_lossy(&moved.stdout)
    );
    assert_eq!(moved.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&moved.stderr).contains("frontmatter"));
    assert!(tmp.path().join("a.md").exists());
    assert!(!tmp.path().join("longer-name.md").exists());
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("ref.md")).unwrap(),
        original
    );
}

#[test]
fn read_adapter_and_full_scan_share_non_frontmatter_boundaries() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "opening-only.md", "---\n");
    let opening = run_json(
        &tmp,
        &["read", "--file", "opening-only.md", "--format", "json"],
    );
    assert_eq!(opening["results"]["content"], "---");

    let long = format!("{}\n# Visible\n", "x".repeat(70_000));
    write_md(tmp.path(), "long-body.md", &long);
    let found = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "long-body.md",
            "--fields",
            "sections",
            "--format",
            "json",
        ],
    );
    assert_eq!(found["total"], 1);
    assert!(
        found["results"][0]["sections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["heading"] == "Visible")
    );

    let huge_yaml_line = format!("---\n{}\n---\nbody\n", "x".repeat(70_000));
    write_md(tmp.path(), "huge-yaml.md", &huge_yaml_line);
    let read = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "read",
            "--file",
            "huge-yaml.md",
            "--lines",
            "1:",
        ])
        .output()
        .unwrap();
    assert!(!read.status.success());
    assert!(String::from_utf8_lossy(&read.stderr).contains("frontmatter too large"));

    let find = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "find",
            "body",
            "--file",
            "huge-yaml.md",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(find.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&find.stderr).contains("frontmatter too large"));
}

#[test]
fn long_delimiter_candidates_agree_across_read_and_find() {
    let tmp = TempDir::new().unwrap();
    let ordinary = format!("---{}x\n# Visible\n", " ".repeat(70_000));
    write_md(tmp.path(), "candidate.md", &ordinary);
    let read = run_json(
        &tmp,
        &["read", "--file", "candidate.md", "--format", "json"],
    );
    assert!(
        read["results"]["content"]
            .as_str()
            .unwrap()
            .contains("# Visible")
    );
    let found = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "candidate.md",
            "--fields",
            "sections",
            "--format",
            "json",
        ],
    );
    assert!(
        found["results"][0]["sections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|section| section["heading"] == "Visible")
    );

    let oversized = format!("---{}\ntitle: x\n---\nbody\n", " ".repeat(1024 * 1024));
    write_md(tmp.path(), "oversized-opening.md", &oversized);
    for args in [
        vec!["read", "--file", "oversized-opening.md", "--format", "json"],
        vec![
            "find",
            "--file",
            "oversized-opening.md",
            "--fields",
            "properties",
            "--format",
            "json",
        ],
    ] {
        let output = hyalo_no_hints()
            .args(["--dir", tmp.path().to_str().unwrap()])
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("frontmatter too large"));
    }
}

#[test]
fn ordered_visibility_drives_move_task_and_directive_consumers() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "old.md", "# Old\n");
    let source = "%% <!-- markdownlint-disable MD019 --> %%\n[[old]]\n<!--\n` -->\n[[old]] `\n<!--\n\n    -->\n[[old]]\n- [x] parent\n\n    - [ ] child [[old]]\ntext\n\n    - [ ] literal [[old]]\n";
    write_md(tmp.path(), "source.md", source);
    let task = run_json(
        &tmp,
        &[
            "task",
            "read",
            "--file",
            "source.md",
            "--line",
            "12",
            "--format",
            "json",
        ],
    );
    assert_eq!(task["results"]["line"], 12);
    let backlinks = run_json(&tmp, &["backlinks", "old.md", "--format", "json"]);
    assert_eq!(backlinks["total"], 4);
    run_json(&tmp, &["mv", "old.md", "--to", "new.md"]);
    let after = std::fs::read_to_string(tmp.path().join("source.md")).unwrap();
    assert_eq!(after.matches("[[new]]").count(), 4);
    assert!(after.contains("    - [ ] literal [[old]]"));

    write_md(
        tmp.path(),
        "inline-directive.md",
        "%% <!-- markdownlint-disable MD019 --> %%\n#  Heading\n",
    );
    let lint = run_json(
        &tmp,
        &[
            "lint",
            "--file",
            "inline-directive.md",
            "--rule",
            "MD019",
            "--format",
            "json",
        ],
    );
    assert_eq!(lint["results"]["violations"], 1);
    assert!(
        lint["results"]["files"][0]["rule_groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|group| group["rule"] == "MD019")
    );

    write_md(
        tmp.path(),
        "real-directive.md",
        "<!-- markdownlint-disable MD019 -->\n#  Heading\n",
    );
    let real = run_json(
        &tmp,
        &[
            "lint",
            "--file",
            "real-directive.md",
            "--rule",
            "MD019",
            "--format",
            "json",
        ],
    );
    assert_eq!(real["results"]["violations"], 0);
}

#[test]
fn native_checkbox_fix_preserves_authored_suffix_and_indented_code() {
    let tmp = TempDir::new().unwrap();
    let original =
        "[] run `cmd` <!-- keep -->\n\ntext\n\n    - [] literal `code` <!-- comment -->\n";
    write_md(tmp.path(), "checkbox.md", original);
    let fixed = run_json(
        &tmp,
        &[
            "lint",
            "--file",
            "checkbox.md",
            "--fix",
            "--rule",
            "HYALO001",
            "--format",
            "json",
        ],
    );
    assert_eq!(fixed["results"]["total_fixed"], 1);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("checkbox.md")).unwrap(),
        "- [ ] run `cmd` <!-- keep -->\n\ntext\n\n    - [] literal `code` <!-- comment -->\n"
    );
}

#[test]
fn nested_sequence_scalar_links_are_discovered_and_rewritten() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "Target.md", "# Target\n");
    write_md(
        tmp.path(),
        "source.md",
        "---\ndescription:\n  - - |-\n      # [[Target]]\n---\nbody\n",
    );
    let backlinks = run_json(&tmp, &["backlinks", "Target.md", "--format", "json"]);
    assert_eq!(backlinks["total"], 1);
    run_json(&tmp, &["mv", "Target.md", "--to", "Renamed.md"]);
    assert!(
        std::fs::read_to_string(tmp.path().join("source.md"))
            .unwrap()
            .contains("# [[Renamed]]")
    );
}

#[test]
fn removing_last_property_rejects_invalid_final_frame_transactionally() {
    let tmp = TempDir::new().unwrap();
    let original = "---\ntitle: x\n---\n---\nbody\n";
    write_md(tmp.path(), "remove.md", original);
    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "remove",
            "--file",
            "remove.md",
            "--property",
            "title",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("frontmatter"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("remove.md")).unwrap(),
        original
    );
}

#[test]
fn removing_last_property_refuses_body_promotion_and_allows_ordinary_body() {
    let tmp = TempDir::new().unwrap();
    let promoted = "---\ntitle: x\n---\n---\nNote: Keep this text.\n---\n# Content\n";
    write_md(tmp.path(), "promoted.md", promoted);
    let refused = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "remove",
            "--file",
            "promoted.md",
            "--property",
            "title",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("reinterpreted as frontmatter"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("promoted.md")).unwrap(),
        promoted
    );

    write_md(
        tmp.path(),
        "ordinary.md",
        "---\ntitle: x\n---\n# Content\nKeep this text.\n",
    );
    let removed = run_json(
        &tmp,
        &[
            "remove",
            "--file",
            "ordinary.md",
            "--property",
            "title",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        removed["results"]["modified"],
        serde_json::json!(["ordinary.md"])
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("ordinary.md")).unwrap(),
        "# Content\nKeep this text.\n"
    );
}
