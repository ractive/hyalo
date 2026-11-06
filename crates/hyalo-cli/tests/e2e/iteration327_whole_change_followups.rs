//! Iteration 327: the whole-change follow-ups — the readers agree on a shared
//! partial path (W8), `new`/`types` ignore a lint severity they never read
//! (W10), `deinit` keeps an edited config (W14), `autofixable` follows the
//! findings (W7), one snapshot-refusal answer (W9), `links fix` keys its
//! report by occurrence (W11) and a snapshot answers like disk or is refused
//! over ignore files and `[scan] include` (W2). DEC-399, DEC-400.

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

fn outlive_tolerance() {
    std::thread::sleep(Duration::from_millis(2200));
}

fn names(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

// ---- W8 --------------------------------------------------------------------

/// What each reader says about one authored target, on disk or under `--index`.
struct Verdict {
    path: Value,
    ambiguous: bool,
    candidates: Vec<String>,
    lint: String,
    fix_ambiguous: Vec<String>,
    fix_unfixable: u64,
    broken: u64,
    mv_skipped: Vec<String>,
}

fn verdict(d: &Path, target: &str, moving: &str, index: bool) -> Verdict {
    let extra: &[&str] = if index { &["--index"] } else { &[] };
    let with = |base: &[&str]| -> Vec<String> {
        base.iter().chain(extra).map(|s| (*s).to_owned()).collect()
    };
    let call = |base: &[&str]| {
        let owned = with(base);
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        run(d, &refs)
    };
    let (found, _, _) = call(&["find", "--file", "src.md", "--fields", "links"]);
    let link = found["results"][0]["links"]
        .as_array()
        .and_then(|links| links.iter().find(|l| l["target"] == target))
        .cloned()
        .unwrap_or(Value::Null);
    let (report, _, _) = call(&["lint", "src.md", "--rule", "HYALO006"]);
    let message = report["results"]["files"][0]["rule_groups"][0]["violations"]
        .as_array()
        .and_then(|all| {
            all.iter().find_map(|v| {
                v["message"]
                    .as_str()
                    .filter(|m| m.contains(&format!("`{target}`")))
            })
        })
        .unwrap_or("")
        .to_owned();
    let (fix, _, _) = call(&["links", "fix"]);
    let fix_ambiguous = fix["results"]["ambiguous_links"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|l| l["target"] == target)
                .flat_map(|l| names(&l["candidates"]))
                .collect()
        })
        .unwrap_or_default();
    let (summary, _, _) = call(&["summary"]);
    let (mv, _, _) = run(d, &["mv", moving, "--to", "moved/target.md", "--dry-run"]);
    let mv_skipped = mv["results"]["skipped_ambiguous"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|s| s["target"] == target)
                .flat_map(|s| names(&s["candidates"]))
                .collect()
        })
        .unwrap_or_default();
    Verdict {
        path: link["path"].clone(),
        ambiguous: link["ambiguous"] == true,
        candidates: names(&link["candidates"]),
        lint: message,
        fix_ambiguous,
        fix_unfixable: fix["results"]["unfixable"].as_u64().unwrap_or(0),
        broken: summary["results"]["links"]["broken"].as_u64().unwrap_or(99),
        mv_skipped,
    }
}

fn w8_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "a/note.md", "# n\n");
    write_md(d, "b/note.md", "# n\n");
    write_md(d, "a/x/deep.md", "# d\n");
    write_md(d, "b/x/deep.md", "# d\n");
    write_md(d, "a/y/solo.md", "# s\n");
    write_md(d, "src.md", "# S\n\n[[note]] [[x/deep]] [[y/solo]]\n");
    tmp
}

#[test]
fn w8_twins_a_shared_partial_path_and_a_resolved_one_agree_everywhere() {
    for index in [false, true] {
        let tmp = w8_vault();
        let d = tmp.path();
        if index {
            run(d, &["create-index"]);
        }
        // A bare stem two files share, and a partial path two files end in:
        // the same verdict, down to the wording and the candidate order.
        for (target, files, moving) in [
            ("note", ["a/note.md", "b/note.md"], "a/note.md"),
            ("x/deep", ["a/x/deep.md", "b/x/deep.md"], "a/x/deep.md"),
        ] {
            let v = verdict(d, target, moving, index);
            let want: Vec<String> = files.iter().map(|s| (*s).to_owned()).collect();
            assert_eq!(v.path, Value::Null, "{target} index={index}");
            assert!(v.ambiguous, "{target} index={index}");
            assert_eq!(v.candidates, want, "{target} index={index}");
            assert!(
                v.lint.contains("matches 2 candidates") && v.lint.contains(&files.join(", ")),
                "{target} index={index}: {}",
                v.lint
            );
            assert_eq!(v.fix_ambiguous, want, "{target} index={index}");
            assert_eq!(v.mv_skipped, want, "{target} index={index}");
            // Ambiguous is still broken: the summary counts both.
            assert_eq!(v.broken, 2, "{target} index={index}");
        }
        // One note ends in `y/solo`: it resolves, and nothing calls it ambiguous.
        let v = verdict(d, "y/solo", "a/y/solo.md", index);
        assert_eq!(v.path, Value::String("a/y/solo.md".into()), "index={index}");
        assert!(!v.ambiguous && v.candidates.is_empty(), "index={index}");
        assert!(
            v.lint.is_empty() || !v.lint.contains("candidates"),
            "{}",
            v.lint
        );
        assert!(v.fix_ambiguous.is_empty(), "index={index}");
        assert_eq!(v.fix_unfixable, 0, "index={index}");
    }
}

#[test]
fn w8_an_exact_path_beats_a_shared_suffix() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "x/deep.md", "# d\n");
    write_md(d, "a/x/deep.md", "# d\n");
    write_md(d, "b/x/deep.md", "# d\n");
    write_md(d, "src.md", "[[x/deep]]\n");
    let (v, _, _) = run(d, &["find", "--file", "src.md", "--fields", "links"]);
    let link = &v["results"][0]["links"][0];
    assert_eq!(link["path"], "x/deep.md");
    assert!(link.get("ambiguous").is_none_or(|a| a != true), "{link}");
}

// ---- W10 -------------------------------------------------------------------

#[test]
fn w10_new_and_types_work_under_an_invalid_lint_severity_but_the_gates_refuse() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    fs::write(
        d.join(".hyalo.toml"),
        "[lint.rules.MD009]\nseverity = \"eror\"\n[schema.types.note]\nrequired = [\"title\"]\n",
    )
    .unwrap();
    write_md(d, "a.md", "# T\n");
    for args in [
        vec!["types", "list"],
        vec!["types", "show", "note"],
        vec!["new", "--type", "note", "--file", "n.md", "--dry-run"],
    ] {
        let (_, err, code) = run(d, &args);
        assert_eq!(code, 0, "{args:?}: {err}");
    }
    let (_, err, code) = run(d, &["lint"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("severity must be"), "{err}");
    // An unloadable `[schema]` still stops what reads the schema.
    fs::write(
        d.join(".hyalo.toml"),
        "[schema.types.note]\nproperties = { k = { type = \"enum\" } }\n",
    )
    .unwrap();
    let (_, _, code) = run(d, &["types", "list"]);
    assert_eq!(code, 1);
}

// ---- W14 -------------------------------------------------------------------

#[test]
fn w14_deinit_removes_only_the_config_init_wrote() {
    for (content, removed) in [
        ("dir = \".\"\n", true),
        ("# mine\ndir = \".\"\n", false),
        ("dir = \".\"  # mine\n", false),
        ("dir  =  \".\"\n", false),
        ("dir = \".\"\n\n", false),
    ] {
        let tmp = TempDir::new().unwrap();
        let d = tmp.path();
        fs::write(d.join(".hyalo.toml"), content).unwrap();
        let out = hyalo_no_hints()
            .current_dir(d)
            .args(["deinit", "--format", "text"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            !d.join(".hyalo.toml").exists(),
            removed,
            "{content:?}: {text}"
        );
        if !removed {
            assert!(text.contains("skipped  .hyalo.toml"), "{content:?}: {text}");
        }
    }
}

// ---- W7 --------------------------------------------------------------------

#[test]
fn w7_autofixable_follows_the_findings_not_the_rule() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    // Only a tab inside a code block: MD010 reports it, its fix is withheld.
    write_md(d, "code.md", "# T\n\n```make\nall:\n\techo hi\n```\n");
    write_md(d, "prose.md", "# T\n\nprose \ttab\n");
    let (v, _, _) = run(d, &["lint", "--rule", "MD010"]);
    let fixable = |file: &str| -> Value {
        v["results"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["file"] == file)
            .map(|f| f["rule_groups"][0]["autofixable"].clone())
            .unwrap()
    };
    assert_eq!(fixable("code.md"), false);
    assert_eq!(fixable("prose.md"), true);
    // Nothing for `--fix` to change, so no hint offers it.
    let out = hyalo_no_hints()
        .arg("--dir")
        .arg(d)
        .args(["lint", "code.md", "--rule", "MD010", "--format", "json"])
        .output()
        .unwrap();
    let hinted = String::from_utf8_lossy(&out.stdout);
    assert!(!hinted.contains("--fix"), "{hinted}");
    // And `--fix` agrees: it changes nothing.
    let (fix, _, _) = run(
        d,
        &["lint", "code.md", "--rule", "MD010", "--fix", "--dry-run"],
    );
    assert_eq!(fix["results"]["total_fixed"], 0, "{fix}");
}

// ---- W9 --------------------------------------------------------------------

#[test]
fn w9_config_and_hints_do_not_promise_a_snapshot_the_loader_refuses() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "a.md", "# A\n");
    let snap = d.join("snap.idx");
    let snap = snap.to_str().unwrap();
    run(d, &["create-index", "--output", snap]);
    // The hints `config` offers thread the snapshot only when it would be read.
    let hinted = |d: &Path| -> String {
        let out = super::common::hyalo()
            .arg("--dir")
            .arg(d)
            .args(["--index-file", snap, "config", "--format", "json"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    assert!(hinted(d).contains("--index-file"), "{}", hinted(d));
    // A `[links] frontmatter` list the snapshot was not built under.
    fs::write(d.join(".hyalo.toml"), "[links]\nfrontmatter = false\n").unwrap();
    assert!(!hinted(d).contains("--index-file"), "{}", hinted(d));
    let (v, err, _) = run(d, &["summary", "--index-file", snap]);
    assert!(err.contains("`[links] frontmatter`"), "{err}");
    assert_eq!(v["results"]["source"], "disk", "{v}");
    assert_eq!(v["results"]["code_blocks"], "index", "{v}");
}

// ---- W11 -------------------------------------------------------------------

#[test]
fn w11_links_fix_reports_each_occurrence_and_the_hint_counts_all_of_them() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "Real.md", "---\naliases: [AKA]\n---\n# Real\n\n## S\n");
    write_md(d, "src.md", "# s\n[[AKA]] [[AKA#S]] [[AKA|x]]\n");
    let (v, _, _) = run(d, &["links", "fix"]);
    let emitted: Vec<&str> = v["results"]["alias_fix_plans"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["emitted_target"].as_str().unwrap())
        .collect();
    assert_eq!(emitted, ["Real|AKA", "Real#S|AKA", "Real|x"]);
    let (v, _, _) = run(d, &["links", "fix", "--apply"]);
    let applied: Vec<&str> = v["results"]["applied_fixes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["emitted_target"].as_str().unwrap())
        .collect();
    assert_eq!(applied, emitted);
    assert_eq!(
        fs::read_to_string(d.join("src.md")).unwrap(),
        "# s\n[[Real|AKA]] [[Real#S|AKA]] [[Real|x]]\n"
    );
}

#[test]
fn w11_the_apply_hint_counts_every_bucket_apply_writes() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "sub/x.md", "# x\n\n## 3. Deploy Steps\n");
    write_md(d, "src.md", "# s\n[[X]] [[x#Deploy Steps]]\n");
    let out = super::common::hyalo()
        .arg("--dir")
        .arg(d)
        .args(["links", "fix", "--format", "json"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let hint = v["hints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| {
            h["cmd"]
                .as_str()
                .is_some_and(|c| c.contains("links fix --apply"))
        })
        .unwrap();
    // One anchor fix plus one bucketed repair: `--apply` writes both.
    assert!(
        hint["description"]
            .as_str()
            .unwrap()
            .starts_with("Apply 2 fixes"),
        "{hint}"
    );
    let (applied, _, _) = run(d, &["links", "fix", "--apply"]);
    let anchors = usize::try_from(applied["results"]["anchors_applied"].as_u64().unwrap()).unwrap();
    let written = applied["results"]["applied_fixes"]
        .as_array()
        .unwrap()
        .len()
        + anchors;
    assert_eq!(written, 2, "{applied}");
}

// ---- W2 --------------------------------------------------------------------

fn git_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    fs::create_dir(d.join(".git")).unwrap();
    write_md(d, "a.md", "# a\n");
    write_md(d, "b.md", "# b\n");
    write_md(d, "sub/n.md", "# n\n");
    tmp
}

fn listed(d: &Path, index: bool) -> (Vec<String>, String) {
    let args: &[&str] = if index {
        &["find", "--fields", "file", "--limit", "0", "--index"]
    } else {
        &["find", "--fields", "file", "--limit", "0"]
    };
    let (v, err, _) = run(d, args);
    let files = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["file"].as_str().unwrap().to_owned())
        .collect();
    (files, err)
}

#[test]
fn w2_an_edited_ignore_file_refuses_the_snapshot_and_disk_answers() {
    for (ignore_file, nested) in [
        (".gitignore", false),
        (".ignore", false),
        (".gitignore", true),
    ] {
        let tmp = git_vault();
        let d = tmp.path();
        let rel = if nested { "sub/" } else { "" };
        let path = format!("{rel}{ignore_file}");
        if nested {
            fs::write(d.join(&path), "unrelated.md\n").unwrap();
        }
        run(d, &["create-index"]);
        let (clean, err) = listed(d, true);
        assert_eq!(clean, ["a.md", "b.md", "sub/n.md"], "{path}");
        assert!(!err.contains("ignore file"), "{path}: {err}");
        outlive_tolerance();
        let rule = if nested { "n.md\n" } else { "b.md\n" };
        fs::write(d.join(&path), rule).unwrap();
        let (disk, _) = listed(d, false);
        let (indexed, err) = listed(d, true);
        assert_eq!(indexed, disk, "{path}");
        assert!(!disk.iter().any(|f| f.ends_with(rule.trim())), "{path}");
        assert!(
            err.contains("ignore file that shaped the index changed"),
            "{path}: {err}"
        );
        // Re-indexing writes a header that matches again, and clears the warning.
        run(d, &["create-index"]);
        let (again, err) = listed(d, true);
        assert_eq!(again, disk, "{path}");
        assert!(!err.contains("ignore file"), "{path}: {err}");
    }
}

#[test]
fn w2_a_new_root_ignore_file_is_noticed_and_a_changed_scan_include_refuses() {
    let tmp = git_vault();
    let d = tmp.path();
    write_md(d, ".notes/h.md", "# h\n");
    run(d, &["create-index"]);
    outlive_tolerance();
    fs::write(d.join(".gitignore"), "b.md\n").unwrap();
    let (disk, _) = listed(d, false);
    let (indexed, err) = listed(d, true);
    assert_eq!(indexed, disk);
    assert!(err.contains("ignore file"), "{err}");

    fs::remove_file(d.join(".gitignore")).unwrap();
    run(d, &["create-index"]);
    fs::write(d.join(".hyalo.toml"), "[scan]\ninclude = [\".notes/**\"]\n").unwrap();
    let (disk, _) = listed(d, false);
    assert!(disk.contains(&".notes/h.md".to_owned()), "{disk:?}");
    let (indexed, err) = listed(d, true);
    assert_eq!(indexed, disk);
    assert!(err.contains("`[scan] include`"), "{err}");
    // A rebuild under the new globs is accepted again.
    run(d, &["create-index"]);
    let (again, err) = listed(d, true);
    assert_eq!(again, disk);
    assert!(!err.contains("`[scan] include`"), "{err}");
}

#[test]
fn w2_an_unchanged_rerun_stays_a_no_op_and_a_changed_ignore_file_rewrites() {
    let tmp = git_vault();
    let d = tmp.path();
    fs::write(d.join(".gitignore"), "zzz.md\n").unwrap();
    // Notes written within the snapshot's own second are re-scanned (DEC-339).
    outlive_tolerance();
    run(d, &["create-index"]);
    let (v, _, _) = run(d, &["create-index"]);
    assert_eq!(v["results"]["written"], false, "{v}");
    outlive_tolerance();
    fs::write(d.join(".gitignore"), "zzz.md\nyyy.md\n").unwrap();
    let (v, _, _) = run(d, &["create-index"]);
    assert_eq!(v["results"]["written"], true, "{v}");
}
