//! Execution contracts: preflight and whole-batch preparation precede effects.
use crate::common::hyalo_no_hints;
use std::fs;
use tempfile::TempDir;

fn vault() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    fs::write(
        dir.path().join("a.md"),
        "---\nstatus: draft\ntags: [a]\n---\n- [ ] task\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("b.md"),
        "---\nstatus: draft\ntags: {bad: mapping}\n---\nplain text\n",
    )
    .unwrap();
    dir
}
fn run(dir: &TempDir, args: &[&str]) -> std::process::Output {
    hyalo_no_hints()
        .current_dir(dir.path())
        .args(args)
        .output()
        .unwrap()
}
fn contents(dir: &TempDir) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}
#[test]
fn unsupported_output_and_malformed_jq_have_no_effects() {
    for args in [
        vec!["set", "a.md", "--property", "status=done", "--count"],
        vec!["append", "a.md", "--property", "tags=b", "--jq", "["],
        vec![
            "task", "toggle", "a.md", "--all", "--format", "text", "--jq", ".",
        ],
        vec!["types", "set", "note", "--count"],
        vec!["init", "--jq", "["],
        vec!["deinit", "--jq", "["],
    ] {
        let dir = vault();
        let before = contents(&dir);
        assert!(!run(&dir, &args).status.success(), "{args:?}");
        assert_eq!(contents(&dir), before, "{args:?}");
    }
}
#[test]
fn single_selection_rejects_multiple_and_empty_but_missing_target_is_clap_usage() {
    for prefix in [
        vec!["read"],
        vec!["backlinks"],
        vec!["task", "read", "--all"],
    ] {
        let dir = vault();
        let mut args = prefix.clone();
        args.extend(["--file", "a.md", "--file", "missing.md", "--format", "json"]);
        let output = run(&dir, &args);
        assert_eq!(output.status.code(), Some(1));
        let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert!(error["error"].as_str().unwrap().contains("exactly one"));
        fs::write(dir.path().join("inputs.txt"), "missing.md\n").unwrap();
        let mut args = prefix.clone();
        args.extend(["--files-from", "inputs.txt", "--format", "json"]);
        let output = run(&dir, &args);
        assert_eq!(output.status.code(), Some(1));
        let text = String::from_utf8_lossy(&output.stderr);
        let error: serde_json::Value =
            serde_json::from_str(&text[text.find('{').unwrap()..]).unwrap();
        assert_eq!(error["files_missing"], 1);
        assert_eq!(run(&dir, &prefix).status.code(), Some(2));
    }
}
#[test]
fn append_and_task_validate_all_selected_files_before_writing() {
    for args in [
        vec![
            "append",
            "--file",
            "a.md",
            "--file",
            "b.md",
            "--property",
            "tags=b",
        ],
        vec![
            "task", "toggle", "--file", "a.md", "--file", "b.md", "--all",
        ],
        vec![
            "task", "toggle", "--file", "a.md", "--file", "b.md", "--line", "5",
        ],
    ] {
        let dir = vault();
        let before = contents(&dir);
        assert!(!run(&dir, &args).status.success());
        assert_eq!(contents(&dir), before);
    }
}
#[test]
fn post_commit_jq_failure_retains_effects_and_coherent_index() {
    let dir = vault();
    assert!(run(&dir, &["create-index"]).status.success());
    let output = run(
        &dir,
        &[
            "set",
            "a.md",
            "--property",
            "status=done",
            "--index",
            "--jq",
            "error(\"deliberate\")",
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["effects"]["paths"][0]["state"], "committed");
    assert_eq!(error["effects"]["index"], "updated");
    let output = run(
        &dir,
        &["find", "--index", "--property", "status=done", "--count"],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"1\n");
}
#[test]
fn create_drop_aliases_are_destinations_and_views_use_effective_projection() {
    let dir = vault();
    assert!(
        run(&dir, &["--index-file", "custom.idx", "create-index"])
            .status
            .success()
    );
    assert!(dir.path().join("custom.idx").is_file());
    assert!(
        run(&dir, &["--index-file", "custom.idx", "drop-index"])
            .status
            .success()
    );
    assert!(!dir.path().join("custom.idx").exists());
    fs::write(
        dir.path().join(".hyalo.toml"),
        "dir = \".\"\n[views.names]\nfilenames0 = true\n",
    )
    .unwrap();
    let output = run(&dir, &["views", "run", "names"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"a.md\0b.md\0");
    assert!(
        !run(&dir, &["views", "run", "names", "--count"])
            .status
            .success()
    );
    let output = run(&dir, &["find", "--file", "missing.md", "--filenames0"]);
    assert!(!output.status.success());
    let output = run(
        &dir,
        &["find", "--property", "status=absent", "--filenames0"],
    );
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}
#[cfg(unix)]
#[test]
fn indexed_regex_refuses_static_escape_and_mutations_reject_physical_aliases() {
    let dir = vault();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("secret.md"), "secret marker\n").unwrap();
    assert!(run(&dir, &["create-index"]).status.success());
    fs::remove_file(dir.path().join("a.md")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret.md"), dir.path().join("a.md")).unwrap();
    assert!(
        !run(&dir, &["find", "--index", "--regexp", "secret"])
            .status
            .success()
    );
    fs::remove_file(dir.path().join("a.md")).unwrap();
    fs::hard_link(dir.path().join("b.md"), dir.path().join("a.md")).unwrap();
    let before = fs::read(dir.path().join("a.md")).unwrap();
    assert_eq!(
        run(
            &dir,
            &[
                "set",
                "--file",
                "a.md",
                "--file",
                "b.md",
                "--property",
                "status=done"
            ]
        )
        .status
        .code(),
        Some(1)
    );
    assert_eq!(fs::read(dir.path().join("a.md")).unwrap(), before);
}

#[test]
fn internal_mutation_protocol_exposes_real_effects_and_rejects_bad_output_before_write() {
    let dir = vault();
    let before = contents(&dir);
    let output = run(
        &dir,
        &[
            "set",
            "a.md",
            "--property",
            "status=done",
            "--internal-mutation-report",
            "--format",
            "text",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(contents(&dir), before);
    for expected in ["committed", "unchanged"] {
        let output = run(
            &dir,
            &[
                "set",
                "a.md",
                "--property",
                "status=done",
                "--internal-mutation-report",
                "--format",
                "json",
            ],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["effects"]["paths"][0]["state"], expected);
        assert_eq!(report["hints"], serde_json::json!([]));
    }
    let ordinary = run(
        &dir,
        &[
            "set",
            "a.md",
            "--property",
            "status=done",
            "--format",
            "json",
        ],
    );
    let report: serde_json::Value = serde_json::from_slice(&ordinary.stdout).unwrap();
    assert!(report.get("effects").is_none());
}

#[test]
fn alias_aware_index_stays_coherent_after_unrelated_source_edits() {
    let dir = vault();
    fs::write(
        dir.path().join(".hyalo.toml"),
        "dir = \".\"\n[links]\naliases = true\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("target.md"),
        "---\naliases: [Nickname]\n---\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("source.md"),
        "---\nstatus: draft\n---\n[[Nickname]]\n",
    )
    .unwrap();
    assert!(run(&dir, &["create-index"]).status.success());
    let output = run(
        &dir,
        &[
            "set",
            "source.md",
            "--property",
            "status=done",
            "--index",
            "--internal-mutation-report",
            "--format",
            "json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["effects"]["index"], "updated");
    assert!(dir.path().join(".hyalo-index").exists());
    let output = run(&dir, &["backlinks", "target.md", "--index", "--count"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"1\n");
}

#[test]
fn captured_baseline_preserves_streams_exits_and_file_bytes() {
    fn decode(hex: &str) -> Vec<u8> {
        hex.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/iteration290-baseline.json")).unwrap();
    let mtime = std::time::UNIX_EPOCH
        + std::time::Duration::from_secs(fixture["mtime_unix"].as_u64().unwrap());
    for case in fixture["cases"].as_array().unwrap() {
        let dir = TempDir::new().unwrap();
        for (name, bytes) in fixture["initial_files"].as_object().unwrap() {
            let path = dir.path().join(name);
            fs::write(&path, decode(bytes.as_str().unwrap())).unwrap();
            fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(mtime)
                .unwrap();
        }
        let args: Vec<_> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| arg.as_str().unwrap())
            .collect();
        let output = run(&dir, &args);
        assert_eq!(
            output.status.code().map(i64::from),
            case["exit_code"].as_i64(),
            "{args:?}"
        );
        assert_eq!(
            output.stdout,
            decode(case["stdout"].as_str().unwrap()),
            "{args:?}"
        );
        assert_eq!(
            output.stderr,
            case["stderr"].as_str().unwrap().as_bytes(),
            "{args:?}"
        );
        for (name, bytes) in case["files"].as_object().unwrap() {
            assert_eq!(
                fs::read(dir.path().join(name)).unwrap(),
                decode(bytes.as_str().unwrap()),
                "{args:?}: {name}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn named_index_refresh_is_confined_and_follows_single_cardinality() {
    for parent_escape in [false, true] {
        let dir = vault();
        let outside = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        fs::write(dir.path().join("nested/secret.md"), "short").unwrap();
        assert!(run(&dir, &["create-index"]).status.success());
        fs::write(
            outside.path().join("secret.md"),
            "external marker with different length",
        )
        .unwrap();
        let name = if parent_escape {
            fs::remove_file(dir.path().join("nested/secret.md")).unwrap();
            fs::remove_dir(dir.path().join("nested")).unwrap();
            std::os::unix::fs::symlink(outside.path(), dir.path().join("nested")).unwrap();
            "nested/secret.md"
        } else {
            fs::remove_file(dir.path().join("a.md")).unwrap();
            std::os::unix::fs::symlink(outside.path().join("secret.md"), dir.path().join("a.md"))
                .unwrap();
            "a.md"
        };
        let output = run(
            &dir,
            &[
                "find", "--index", "--file", name, "--regexp", "marker", "--format", "json",
            ],
        );
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("outside root"));
        for prefix in [
            vec!["read"],
            vec!["backlinks"],
            vec!["task", "read", "--all"],
        ] {
            let mut args = prefix;
            args.extend([
                "--index", "--file", name, "--file", "b.md", "--format", "json",
            ]);
            let output = run(&dir, &args);
            assert_eq!(output.status.code(), Some(1));
            let diagnostic: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
            assert!(
                diagnostic["error"]
                    .as_str()
                    .unwrap()
                    .contains("exactly one")
            );
        }
    }
}

#[test]
fn skipped_frontmatter_summary_precedes_post_commit_error_envelope() {
    let dir = vault();
    fs::write(dir.path().join("b.md"), "---\ntags: [unterminated\n---\n").unwrap();
    let output = run(
        &dir,
        &[
            "append",
            "--glob",
            "*.md",
            "--property",
            "tags=new",
            "--format",
            "json",
            "--jq",
            "error(\"after write\")",
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("warning: skipped 1 file"));
    let error: serde_json::Value =
        serde_json::from_str(&stderr[stderr.find('{').unwrap()..]).unwrap();
    assert_eq!(error["effects"]["paths"][0]["state"], "committed");
    assert_eq!(error["category"], "output_failure");
    assert!(
        fs::read_to_string(dir.path().join("a.md"))
            .unwrap()
            .contains("new")
    );
}

#[cfg(unix)]
#[test]
fn hostile_filename_post_commit_errors_are_safe_text_and_exact_json() {
    use std::process::Stdio;
    let dir = vault();
    let name = "hostile\u{1b}[31m.md";
    fs::write(dir.path().join(name), "---\nstatus: old\n---\n").unwrap();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_hyalo"))
        .arg("--no-hints")
        .current_dir(dir.path())
        .args([
            "set",
            "--file",
            name,
            "--property",
            "status=new",
            "--format",
            "text",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(141));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Effects: hostile"), "{stderr}");
    assert!(!stderr.contains('\u{1b}'));
    assert!(
        fs::read_to_string(dir.path().join(name))
            .unwrap()
            .contains("status: new")
    );
    let output = run(
        &dir,
        &[
            "set",
            "--file",
            name,
            "--property",
            "status=again",
            "--format",
            "json",
            "--jq",
            "error(\"after write\")",
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["effects"]["paths"][0]["file"], name);
    assert_eq!(error["effects"]["paths"][0]["state"], "committed");
}

#[cfg(unix)]
#[test]
fn indexed_absolute_paths_through_vault_alias_preserve_normalization() {
    let dir = vault();
    let aliases = TempDir::new().unwrap();
    let alias = aliases.path().join("vault-alias");
    std::os::unix::fs::symlink(dir.path(), &alias).unwrap();
    assert!(run(&dir, &["create-index"]).status.success());
    let absolute = alias.join("a.md");
    let absolute = absolute.to_str().unwrap();
    fs::write(
        dir.path().join("a.md"),
        "---\nstatus: draft\n---\nnew marker longer than old index content\n",
    )
    .unwrap();
    let output = run(
        &dir,
        &[
            "find", "--index", "--file", absolute, "--regexp", "marker", "--count",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"1\n");
    let output = run(
        &dir,
        &[
            "set",
            "--index",
            "--file",
            absolute,
            "--property",
            "status=done",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fs::read_to_string(dir.path().join("a.md"))
            .unwrap()
            .contains("status: done")
    );
    let output = run(&dir, &["read", "--index", "--file", absolute]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn named_fifo_targets_fail_promptly_and_regular_aliases_remain_compatible() {
    use std::os::unix::fs::{FileTypeExt as _, symlink};
    use std::time::Duration;

    for state in ["existing", "absent", "disk"] {
        let dir = vault();
        if state == "absent" {
            assert!(run(&dir, &["create-index"]).status.success());
        }
        let pipe = dir.path().join("pipe.md");
        fs::write(&pipe, "regular marker\n").unwrap();
        symlink("pipe.md", dir.path().join("alias.md")).unwrap();
        if state == "existing" {
            assert!(run(&dir, &["create-index"]).status.success());
        }
        let index_path = dir.path().join(".hyalo-index");
        let initial_index = (state != "disk").then(|| fs::read(&index_path).unwrap());
        // Exercise the same names and routes while regular, then replace only
        // the referent with a FIFO that has no writer. This also covers stale
        // indexed entries and a symlink whose own entry remains unchanged.
        for fifo in [false, true] {
            if fifo {
                // Regular compatibility queries may insert absent entries;
                // restore the fixture snapshot to keep the FIFO routes distinct.
                if let Some(bytes) = &initial_index {
                    fs::write(&index_path, bytes).unwrap();
                }
                fs::remove_file(&pipe).unwrap();
                assert_cmd::Command::new("mkfifo")
                    .arg(&pipe)
                    .timeout(Duration::from_secs(5))
                    .assert()
                    .success();
                assert!(fs::metadata(&pipe).unwrap().file_type().is_fifo());
            }
            for target in ["pipe.md", "alias.md"] {
                for regex in [false, true] {
                    let mut command = hyalo_no_hints();
                    command
                        .current_dir(dir.path())
                        .args(["find", "--file", target, "--format", "json"]);
                    if state != "disk" {
                        command.arg("--index");
                    }
                    if regex {
                        command.args(["--regexp", "marker"]);
                    }
                    // assert_cmd kills AND reaps timed-out children; status
                    // assertions below fail on that signal instead of hanging.
                    let output = command.timeout(Duration::from_secs(5)).output().unwrap();
                    let context = format!("{state}/{target}/regex={regex}/fifo={fifo}");
                    if fifo {
                        assert_eq!(output.status.code(), Some(1), "{context}");
                        assert!(output.stdout.is_empty(), "{context}");
                        let error: serde_json::Value =
                            serde_json::from_slice(&output.stderr).unwrap();
                        assert!(
                            error["error"]
                                .as_str()
                                .unwrap()
                                .contains("not a regular file"),
                            "{context}: {error}"
                        );
                    } else {
                        assert!(
                            output.status.success(),
                            "{context}: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                        let result: serde_json::Value =
                            serde_json::from_slice(&output.stdout).unwrap();
                        assert_eq!(result["results"][0]["file"], target, "{context}");
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn normalized_target_matrix_preserves_one_identity_for_refresh_insert_and_disk() {
    for state in ["existing", "absent", "disk"] {
        for spelling in ["relative", "prefix", "absolute", "alias"] {
            let project = TempDir::new().unwrap();
            let kb = project.path().join("kb");
            fs::create_dir_all(kb.join("kb")).unwrap();
            fs::write(project.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
            fs::write(
                kb.join("note.md"),
                "---\nmarker: outer\n---\nouter marker\n",
            )
            .unwrap();
            fs::write(
                kb.join("kb/note.md"),
                "---\nmarker: nested\n---\nnested marker\n",
            )
            .unwrap();
            if state == "existing" {
                assert!(run(&project, &["create-index"]).status.success());
            }
            if state == "absent" {
                fs::remove_file(kb.join("kb/note.md")).unwrap();
                assert!(run(&project, &["create-index"]).status.success());
            }
            fs::write(
                kb.join("kb/note.md"),
                "---\nmarker: nested-updated\n---\nnested marker with new length\n",
            )
            .unwrap();
            let aliases = TempDir::new().unwrap();
            std::os::unix::fs::symlink(&kb, aliases.path().join("alias")).unwrap();
            // Explicit CLI prefix policy strips ONE kb. The canonical nested
            // identity kb/note.md must not be interpreted as CLI text again.
            let target = match spelling {
                "relative" => "kb/kb/note.md".to_owned(),
                "prefix" => "./kb/kb/note.md".to_owned(),
                "absolute" => fs::canonicalize(kb.join("kb/note.md"))
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                _ => aliases
                    .path()
                    .join("alias/kb/note.md")
                    .to_string_lossy()
                    .into_owned(),
            };
            let mut args = vec![
                "find", "--file", &target, "--regexp", "nested", "--format", "json",
            ];
            if state != "disk" {
                args.push("--index");
            }
            let output = run(&project, &args);
            assert!(
                output.status.success(),
                "{state}/{spelling}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                json["results"][0]["file"], "kb/note.md",
                "{state}/{spelling}"
            );
            assert_eq!(
                json["results"][0]["properties"]["marker"], "nested-updated",
                "{state}/{spelling}"
            );
            if matches!(spelling, "absolute" | "alias") {
                let output = run(&project, &["read", "--file", &target, "--format", "json"]);
                assert!(output.status.success());
                let read: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(read["results"]["file"], "kb/note.md");
                let mut args = vec!["lint", "--file", &target, "--format", "json"];
                if state != "disk" {
                    args.push("--index");
                }
                let output = run(&project, &args);
                let lint: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(lint["results"]["files_checked"], 1);
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn normalized_external_target_matrix_refuses_all_scanner_routes() {
    for state in ["existing", "absent", "disk"] {
        for parent in [false, true] {
            for spelling in ["relative", "prefix", "absolute", "alias"] {
                let project = TempDir::new().unwrap();
                let kb = project.path().join("kb");
                fs::create_dir_all(kb.join("parent")).unwrap();
                fs::write(project.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
                fs::write(kb.join("safe.md"), "safe marker").unwrap();
                let rel = if parent {
                    "parent/escape.md"
                } else {
                    "escape.md"
                };
                if state == "existing" {
                    fs::write(kb.join(rel), "old").unwrap();
                }
                if state != "disk" {
                    assert!(run(&project, &["create-index"]).status.success());
                }
                if state == "existing" {
                    fs::remove_file(kb.join(rel)).unwrap();
                }
                let outside = TempDir::new().unwrap();
                fs::write(outside.path().join("escape.md"), "external marker longer").unwrap();
                if parent {
                    fs::remove_dir(kb.join("parent")).unwrap();
                    std::os::unix::fs::symlink(outside.path(), kb.join("parent")).unwrap();
                } else {
                    std::os::unix::fs::symlink(outside.path().join("escape.md"), kb.join(rel))
                        .unwrap();
                }
                let aliases = TempDir::new().unwrap();
                std::os::unix::fs::symlink(&kb, aliases.path().join("alias")).unwrap();
                let target = match spelling {
                    "relative" => rel.to_owned(),
                    "prefix" => format!("kb/{rel}"),
                    "absolute" => fs::canonicalize(&kb)
                        .unwrap()
                        .join(rel)
                        .to_string_lossy()
                        .into_owned(),
                    _ => aliases
                        .path()
                        .join("alias")
                        .join(rel)
                        .to_string_lossy()
                        .into_owned(),
                };
                for positional in [false, true] {
                    let mut args = vec!["find", "--format", "json"];
                    if state != "disk" {
                        args.push("--index");
                    }
                    if positional {
                        args.extend(["--", "marker", "safe.md", &target]);
                    } else {
                        args.extend(["--regexp", "marker", "--file", "safe.md", "--file", &target]);
                    }
                    let output = run(&project, &args);
                    assert_eq!(
                        output.status.code(),
                        Some(1),
                        "{state}/{parent}/{spelling}/{positional}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert!(!String::from_utf8_lossy(&output.stdout).contains("external marker"));
                }
                if state != "disk" {
                    let output = run(
                        &project,
                        &["lint", "--index", "--file", &target, "--format", "json"],
                    );
                    assert_eq!(output.status.code(), Some(1));
                    assert!(String::from_utf8_lossy(&output.stderr).contains("outside"));
                }
            }
        }
    }
}

#[test]
fn files_from_keeps_literal_first_identity_membership_and_counters() {
    let project = TempDir::new().unwrap();
    let kb = project.path().join("kb");
    fs::create_dir_all(kb.join("kb")).unwrap();
    fs::write(project.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
    fs::write(kb.join("note.md"), "---\nmarker: outer\n---\n").unwrap();
    fs::write(kb.join("kb/note.md"), "---\nmarker: nested\n---\n").unwrap();
    assert!(run(&project, &["create-index"]).status.success());
    fs::write(kb.join("unindexed.md"), "unindexed marker").unwrap();
    fs::write(
        project.path().join("list.txt"),
        "kb/note.md\nkb/note.md\nunindexed.md\nmissing.md\nplain.txt\n../outside.md\n",
    )
    .unwrap();
    for indexed in [false, true] {
        let mut args = vec!["find", "--files-from", "list.txt", "--format", "json"];
        if indexed {
            args.push("--index");
        }
        let output = run(&project, &args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let paths: Vec<_> = json["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["file"].as_str().unwrap())
            .collect();
        assert!(paths.contains(&"kb/note.md"));
        assert!(!paths.contains(&"note.md"));
        assert_eq!(paths.contains(&"unindexed.md"), !indexed);
        assert_eq!(json["files_missing"], if indexed { 2 } else { 1 });
        assert_eq!(json["files_skipped_non_md"], 1);
        assert_eq!(json["files_skipped_outside_vault"], 1);
    }
    // Explicit CLI uses prefix-first policy; files-from above used literal-first.
    let output = run(
        &project,
        &["find", "--file", "kb/note.md", "--format", "json"],
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"][0]["file"], "note.md");
    fs::remove_file(kb.join("kb/note.md")).unwrap();
    let output = run(
        &project,
        &[
            "find",
            "--index",
            "--files-from",
            "list.txt",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"][0]["file"], "kb/note.md");
    let output = run(
        &project,
        &["find", "--file", "kb/kb/note.md", "--format", "json"],
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "missing normalized nested path must not retarget outer note"
    );
    fs::write(project.path().join("empty.txt"), "").unwrap();
    let output = run(
        &project,
        &[
            "find",
            "--index",
            "--files-from",
            "empty.txt",
            "--filenames0",
        ],
    );
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}
