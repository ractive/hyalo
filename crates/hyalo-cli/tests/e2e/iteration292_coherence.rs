use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

fn run(tmp: &TempDir, args: &[&str]) -> serde_json::Value {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn names(value: &serde_json::Value) -> Vec<String> {
    let mut paths: Vec<_> = value["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["file"].as_str().unwrap().to_owned())
        .collect();
    paths.sort();
    paths
}
#[test]
fn boolean_phrase_sets_and_scores_agree_on_disk_index_and_named_refresh() {
    let tmp = TempDir::new().unwrap();
    for (path, body) in [
        ("a.md", "green red x blue"),
        ("b.md", "red"),
        ("c.md", "red blue"),
        ("d.md", "neither"),
        ("e.md", "rust memory allocation"),
        ("f.md", "rust memory safety"),
        ("g.md", "rust memory\nsafety"),
    ] {
        write_md(tmp.path(), path, body);
    }
    run(&tmp, &["create-index"]);
    for (query, expected) in [
        ("\"red blue\" OR green", vec!["a.md", "c.md"]),
        ("rust -\"memory safety\"", vec!["e.md"]),
    ] {
        let disk = run(&tmp, &["find", query]);
        assert_eq!(names(&disk), expected);
        let indexed = run(&tmp, &["find", query, "--index"]);
        assert_eq!(indexed["results"], disk["results"]);
        for path in ["a.md", "b.md", "c.md", "d.md", "e.md", "f.md", "g.md"] {
            let disk = run(&tmp, &["find", query, "--file", path]);
            let indexed = run(&tmp, &["find", query, "--file", path, "--index"]);
            assert_eq!(indexed["results"], disk["results"], "{query} {path}");
        }
    }
}
#[test]
fn named_refresh_replaces_equal_size_same_second_tokens() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "oldtoken\n");
    run(&tmp, &["create-index"]);
    write_md(tmp.path(), "a.md", "newtoken\n");
    for query in ["newtoken", "oldtoken"] {
        let disk = run(&tmp, &["find", query, "--file", "a.md"]);
        let indexed = run(&tmp, &["find", query, "--file", "a.md", "--index"]);
        assert_eq!(indexed["results"], disk["results"]);
        assert_eq!(names(&indexed).len(), usize::from(query == "newtoken"));
    }
}
#[test]
fn alias_catalog_changes_update_untouched_sources_and_reload() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \".\"\n[links]\naliases = true\n",
    )
    .unwrap();
    write_md(tmp.path(), "target.md", "---\naliases: [Nickname]\n---\n");
    write_md(
        tmp.path(),
        "source.md",
        "---\nstatus: draft\n---\n[[Nickname]] [[Newname]]\n",
    );
    run(&tmp, &["create-index"]);
    run(
        &tmp,
        &["set", "source.md", "--property", "status=done", "--index"],
    );
    let disk = run(&tmp, &["backlinks", "target.md"]);
    assert_eq!(
        run(&tmp, &["backlinks", "target.md", "--index"])["results"],
        disk["results"]
    );
    run(
        &tmp,
        &[
            "set",
            "target.md",
            "--property",
            "aliases=[Newname]",
            "--index",
        ],
    );
    let disk = run(&tmp, &["find", "--file", "source.md", "--fields", "links"]);
    assert_eq!(
        run(
            &tmp,
            &[
                "find",
                "--file",
                "source.md",
                "--fields",
                "links",
                "--index"
            ]
        )["results"],
        disk["results"]
    );
    let disk = run(&tmp, &["backlinks", "target.md"]);
    assert_eq!(
        run(&tmp, &["backlinks", "target.md", "--index"])["results"],
        disk["results"]
    );
}
#[test]
fn relative_link_and_frontmatter_anchor_mutation_parity() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "b.md", "# Root\n");
    write_md(tmp.path(), "sub/b.md", "# Nested\n");
    write_md(
        tmp.path(),
        "sub/a.md",
        "# Here\n[b](b.md) [self](#missing)\n- [ ] todo\n",
    );
    run(&tmp, &["create-index"]);
    run(
        &tmp,
        &["set", "sub/a.md", "--property", "title=Author", "--index"],
    );
    for path in ["b.md", "sub/b.md"] {
        let disk = run(&tmp, &["backlinks", path]);
        assert_eq!(
            run(&tmp, &["backlinks", path, "--index"])["results"],
            disk["results"]
        );
    }
    for args in [
        vec![
            "find",
            "--file",
            "sub/a.md",
            "--fields",
            "links,tasks,sections,properties",
        ],
        vec!["find", "--broken-links"],
    ] {
        let disk = run(&tmp, &args);
        let mut indexed = args;
        indexed.push("--index");
        assert_eq!(run(&tmp, &indexed)["results"], disk["results"]);
    }
}
#[test]
fn broken_authored_frontmatter_never_uses_old_named_search_tokens() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "oldtoken\n");
    run(&tmp, &["create-index"]);
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: [unfinished\n---\noldtoken\n",
    );
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find", "oldtoken", "--file", "a.md", "--index", "--format", "json",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unparseable frontmatter"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("a.md")).unwrap(),
        "---\ntitle: [unfinished\n---\noldtoken\n"
    );
}

#[test]
fn append_task_and_move_keep_indexed_scan_products_current() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\n---\n# Heading\n- [ ] todo\n[[b]]\n",
    );
    write_md(tmp.path(), "b.md", "# B\n");
    run(&tmp, &["create-index"]);
    run(
        &tmp,
        &["append", "a.md", "--property", "tags=newtoken", "--index"],
    );
    run(&tmp, &["task", "toggle", "a.md", "--all", "--index"]);
    run(&tmp, &["mv", "b.md", "renamed.md", "--index"]);
    for args in [
        vec!["find", "--fields", "links,tasks,sections,properties"],
        vec!["find", "todo"],
        vec!["backlinks", "renamed.md"],
    ] {
        let disk = run(&tmp, &args);
        let mut indexed = args;
        indexed.push("--index");
        assert_eq!(run(&tmp, &indexed)["results"], disk["results"]);
    }
}

#[test]
fn oversized_snapshot_expansion_refuses_set_before_note_publication() {
    let tmp = TempDir::new().unwrap();
    let original = "---\nstatus: draft\n---\nAuthored body\n";
    write_md(tmp.path(), "a.md", original);
    run(&tmp, &["create-index"]);
    let path = tmp.path().join(".hyalo-index");
    let mut snapshot: serde_json::Value =
        rmp_serde::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let term = "x".repeat(8192);
    snapshot["bm25_index"] = serde_json::json!({"postings": {term: [{"doc_id":0,"term_freq":32768,"positions":(0..32768).collect::<Vec<_>>()}]}, "doc_lengths":[32768], "doc_paths":["a.md"],"avgdl":32768.0,"tokenizer_version":1});
    std::fs::write(&path, rmp_serde::to_vec_named(&snapshot).unwrap()).unwrap();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["set", "a.md", "--property", "status=done", "--index"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("expansion budget"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("a.md")).unwrap(),
        original
    );
}

#[test]
fn legacy_reconstruction_omits_bm25_and_uses_real_disk_fallback() {
    for missing_metadata in [false, true] {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "a.md", "---\nstatus: draft\n---\nalpha\n");
        write_md(tmp.path(), "b.md", "oldtoken\n");
        run(&tmp, &["create-index"]);
        let path = tmp.path().join(".hyalo-index");
        let mut snapshot: serde_json::Value =
            rmp_serde::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        if missing_metadata {
            for entry in snapshot["entries"].as_array_mut().unwrap() {
                entry["bm25_tokenizer_version"] = serde_json::Value::Null;
            }
        } else {
            snapshot["bm25_index"]["tokenizer_version"] = serde_json::json!(0);
        }
        std::fs::write(&path, rmp_serde::to_vec_named(&snapshot).unwrap()).unwrap();
        run(
            &tmp,
            &["set", "a.md", "--property", "status=done", "--index"],
        );
        let persisted: serde_json::Value =
            rmp_serde::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert!(persisted["bm25_index"].is_null());
        let indexed = run(&tmp, &["find", "oldtoken", "--index"]);
        assert_eq!(names(&indexed), vec!["b.md"]);
        assert_eq!(
            indexed["results"],
            run(&tmp, &["find", "oldtoken"])["results"]
        );
    }
}

#[test]
fn configured_case_policy_matches_disk_and_snapshot_graphs() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "folder/Target.md", "target\n");
    write_md(tmp.path(), "a.md", "[x](folder/target.md)\n");
    for (policy, expected) in [("false", 0), ("true", 1)] {
        std::fs::write(
            tmp.path().join(".hyalo.toml"),
            format!("dir = \".\"\n[links]\ncase_insensitive = \"{policy}\"\n"),
        )
        .unwrap();
        run(&tmp, &["create-index"]);
        let disk = run(&tmp, &["backlinks", "folder/Target.md"]);
        assert_eq!(disk["total"], expected);
        assert_eq!(
            run(&tmp, &["backlinks", "folder/Target.md", "--index"]),
            disk
        );
    }
}

#[test]
fn broken_frontmatter_keeps_filename_precedence_and_repair_clears_skips() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "[[b]]\n");
    write_md(tmp.path(), "sub/b.md", "valid\n");
    write_md(tmp.path(), "b.md", "---\ntitle: [unfinished\n---\nbody\n");
    write_md(
        tmp.path(),
        "other-unfinished.md",
        "---\ntitle: [unfinished\n---\n",
    );
    run(&tmp, &["create-index"]);
    let disk = run(&tmp, &["find", "--file", "a.md", "--fields", "links"]);
    assert_eq!(disk["results"][0]["links"][0]["path"], "b.md");
    assert_eq!(
        run(
            &tmp,
            &["find", "--file", "a.md", "--fields", "links", "--index"]
        )["results"],
        disk["results"]
    );
    let disk = run(&tmp, &["backlinks", "b.md"]);
    assert_eq!(disk["total"], 1);
    assert_eq!(
        run(&tmp, &["backlinks", "b.md", "--index"])["results"],
        disk["results"]
    );
    assert_eq!(
        run(&tmp, &["summary", "--index"])["results"]["files"]["skipped"],
        2
    );
    write_md(tmp.path(), "b.md", "---\ntitle: repaired\n---\nbody\n");
    run(
        &tmp,
        &["set", "b.md", "--property", "status=done", "--index"],
    );
    let disk = run(&tmp, &["summary"]);
    let indexed = run(&tmp, &["summary", "--index"]);
    assert_eq!(indexed["results"]["files"]["skipped"], 1);
    assert_eq!(indexed["results"]["files"], disk["results"]["files"]);
    assert_eq!(run(&tmp, &["backlinks", "b.md", "--index"])["total"], 1);
}

#[test]
fn boolean_sections_and_projections_cover_disk_fallback_and_named_refresh() {
    let tmp = TempDir::new().unwrap();
    for (path, body) in [
        ("a.md", "green red x blue"),
        ("b.md", "red"),
        ("c.md", "red blue"),
        ("e.md", "rust memory allocation"),
        ("f.md", "rust memory safety"),
    ] {
        write_md(
            tmp.path(),
            path,
            &format!("# Scope\n{body}\n# Other\nred blue rust memory allocation\n"),
        );
    }
    write_md(
        tmp.path(),
        "unfinished.md",
        "---\ntitle: [unfinished\n---\n",
    );
    assert_eq!(run(&tmp, &["create-index"])["results"]["warnings"], 1);
    for indexed in [false, true] {
        let mut command = hyalo_no_hints();
        command.arg("--dir").arg(tmp.path()).args([
            "find",
            "green",
            "--section",
            "Scope",
            "--fields",
            "file",
            "--format",
            "json",
        ]);
        if indexed {
            command.arg("--index");
        }
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("skipped 1 file"));
    }
    for (query, expected) in [
        ("\"red blue\" OR green", vec!["a.md", "c.md"]),
        ("rust -\"memory safety\"", vec!["e.md"]),
    ] {
        for fields in ["file", "title,sections"] {
            let base = ["find", query, "--section", "Scope", "--fields", fields];
            let disk = run(&tmp, &base);
            assert_eq!(names(&disk), expected);
            let mut indexed_args = base.to_vec();
            indexed_args.push("--index");
            let indexed = run(&tmp, &indexed_args);
            assert_eq!(indexed["results"], disk["results"]);
            // Section selection uses real body fallback even with --index.
            for path in ["a.md", "c.md", "e.md", "f.md"] {
                let mut named = base.to_vec();
                named.extend(["--file", path]);
                let disk = run(&tmp, &named);
                named.push("--index");
                assert_eq!(run(&tmp, &named)["results"], disk["results"]);
            }
            for use_index in [false, true] {
                let mut limited = base.to_vec();
                limited.extend(["--limit", "1"]);
                if use_index {
                    limited.push("--index");
                }
                let result = run(&tmp, &limited);
                assert_eq!(result["total"], expected.len());
                assert_eq!(names(&result).len(), 1);
            }
        }
    }
    // Named refresh replaces the section tokens; the old phrase must disappear.
    write_md(tmp.path(), "c.md", "# Scope\ngreen\n# Other\nred blue\n");
    let args = [
        "find",
        "\"red blue\"",
        "--section",
        "Scope",
        "--fields",
        "file",
        "--file",
        "c.md",
    ];
    let disk = run(&tmp, &args);
    assert!(names(&disk).is_empty());
    let mut indexed = args.to_vec();
    indexed.push("--index");
    assert_eq!(run(&tmp, &indexed)["results"], disk["results"]);
}
