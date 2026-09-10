//! Iteration 294: execute scope-preserving continuation hints as advertised.

use super::common::hyalo;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn run(root: &Path, args: &[&str]) -> Output {
    hyalo()
        .current_dir(root)
        .args(args)
        .output()
        .expect("run hyalo")
}

fn hint_command(value: &serde_json::Value, needle: &str) -> String {
    value["hints"]
        .as_array()
        .expect("hints array")
        .iter()
        .find(|hint| {
            hint["description"]
                .as_str()
                .is_some_and(|description| description.contains(needle))
        })
        .and_then(|hint| hint["cmd"].as_str())
        .filter(|command| !command.is_empty())
        .unwrap_or_else(|| panic!("missing executable {needle:?} hint: {value}"))
        .to_owned()
}

fn run_advertised_shell(root: &Path, command: &str) -> Output {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_hyalo"));
    let bin_dir = binary.parent().expect("binary parent");
    let path = std::env::var_os("PATH").unwrap_or_default();
    let joined = std::env::join_paths(
        std::iter::once(bin_dir.to_path_buf()).chain(std::env::split_paths(&path)),
    )
    .expect("PATH");
    Command::new("sh")
        .args(["-c", command])
        .current_dir(root)
        .env("PATH", joined)
        .output()
        .expect("execute advertised shell command")
}

#[test]
fn lint_apply_hint_keeps_files_from_scope_and_sentinel_bytes() {
    let project = TempDir::new().unwrap();
    let vault = project.path().join("vault");
    fs::create_dir(&vault).unwrap();
    fs::write(project.path().join(".hyalo.toml"), "dir = \"vault\"\n").unwrap();
    let bad = "---\ntitle: selected\n---\n\ntrailing spaces   \n";
    fs::write(vault.join("a note.md"), bad).unwrap();
    fs::write(vault.join("b sentinel.md"), bad).unwrap();
    fs::write(project.path().join("changed.txt"), "vault/a note.md\n").unwrap();
    let sentinel = fs::read(vault.join("b sentinel.md")).unwrap();

    let preview = run(
        project.path(),
        &[
            "lint",
            "--files-from",
            "changed.txt",
            "--fix",
            "--dry-run",
            "--fix-rule",
            "MD009",
            "--format",
            "json",
            "--hints",
        ],
    );
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let apply = hint_command(&json, "Apply");
    assert!(apply.contains("--file='a note.md'") || apply.contains("'--file=a note.md'"));
    assert!(!apply.contains("--files-from"));
    let applied = run_advertised_shell(project.path(), &apply);
    assert!(
        applied.status.success(),
        "{apply}\n{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_ne!(fs::read(vault.join("a note.md")).unwrap(), bad.as_bytes());
    assert_eq!(fs::read(vault.join("b sentinel.md")).unwrap(), sentinel);
}

#[test]
fn find_show_all_shell_hint_keeps_query_section_and_exact_result_set() {
    let project = TempDir::new().unwrap();
    let vault = project.path().join("vault");
    fs::create_dir(&vault).unwrap();
    fs::write(
        project.path().join(".hyalo.toml"),
        "dir = \"vault\"\ndefault_limit = 2\n",
    )
    .unwrap();
    for index in 0..3 {
        fs::write(
            vault.join(format!("match {index}.md")),
            format!("# Other\n\ntimeout outside\n\n## Wanted\n\ntimeout selected {index}\n"),
        )
        .unwrap();
    }
    fs::write(
        vault.join("wrong-section.md"),
        "# Other\n\ntimeout only here\n",
    )
    .unwrap();
    fs::write(vault.join("no-match.md"), "## Wanted\n\nunrelated\n").unwrap();

    let limited = run(
        project.path(),
        &[
            "find",
            "timeout",
            "--section",
            "Wanted",
            "--sort",
            "title",
            "--format",
            "json",
            "--hints",
        ],
    );
    assert!(limited.status.success());
    let json: serde_json::Value = serde_json::from_slice(&limited.stdout).unwrap();
    assert_eq!(json["results"].as_array().unwrap().len(), 2);
    assert_eq!(json["total"], 3);
    let show_all = hint_command(&json, "Show all 3");
    assert!(show_all.contains("--section Wanted"));
    assert!(show_all.contains("--limit 0"));
    assert!(show_all.ends_with("-- timeout"));
    let output = run_advertised_shell(project.path(), &show_all);
    assert!(
        output.status.success(),
        "{show_all}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let all: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut files: Vec<_> = all["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|result| result["file"].as_str().unwrap().to_owned())
        .collect();
    files.sort();
    assert_eq!(files, ["match 0.md", "match 1.md", "match 2.md"]);
}

#[test]
fn auto_link_apply_shell_hint_keeps_first_only_and_target_exclusion() {
    let project = TempDir::new().unwrap();
    let vault = project.path().join("vault");
    fs::create_dir_all(vault.join("templates")).unwrap();
    fs::write(project.path().join(".hyalo.toml"), "dir = \"vault\"\n").unwrap();
    fs::write(vault.join("target.md"), "---\ntitle: QuasarTarget\n---\n").unwrap();
    fs::write(
        vault.join("templates/template.md"),
        "---\ntitle: ExcludedWidget\n---\n",
    )
    .unwrap();
    fs::write(
        vault.join("source.md"),
        "---\ntitle: Source\n---\n\nQuasarTarget appears. QuasarTarget repeats. ExcludedWidget remains.\n",
    )
    .unwrap();
    let preview = run(
        project.path(),
        &[
            "links",
            "auto",
            "--file",
            "source.md",
            "--first-only",
            "--exclude-target-glob",
            "templates/*",
            "--format",
            "json",
            "--hints",
        ],
    );
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let apply = hint_command(&json, "Apply");
    assert!(apply.contains("--first-only"));
    assert!(apply.contains("--exclude-target-glob 'templates/*'"));
    let output = run_advertised_shell(project.path(), &apply);
    assert!(
        output.status.success(),
        "{apply}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = fs::read_to_string(vault.join("source.md")).unwrap();
    assert_eq!(
        source.matches("[[target|QuasarTarget]]").count(),
        1,
        "{source}"
    );
    assert!(source.contains("QuasarTarget repeats"), "{source}");
    assert_eq!(source.matches("ExcludedWidget").count(), 1, "{source}");
    assert!(!source.contains("[[ExcludedWidget]]"), "{source}");
}

#[test]
fn links_fix_shell_hints_preserve_ordinary_and_fuzzy_scope() {
    let fuzzy = TempDir::new().unwrap();
    fs::write(
        fuzzy.path().join(".hyalo.toml"),
        "dir = \".\"\n[links]\ncase_insensitive = \"false\"\n",
    )
    .unwrap();
    fs::write(fuzzy.path().join("draft-note.md"), "# Draft\n").unwrap();
    fs::write(fuzzy.path().join("other-note.md"), "# Other\n").unwrap();
    fs::write(
        fuzzy.path().join("source.md"),
        "[[draft-noet]] and [[other-noet]]\n",
    )
    .unwrap();
    let preview = run(
        fuzzy.path(),
        &[
            "links",
            "fix",
            "--ignore-target",
            "draft",
            "--case-insensitive",
            "--format",
            "json",
            "--hints",
        ],
    );
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let fuzzy_apply = hint_command(&json, "lower-confidence fuzzy fixes");
    for token in [
        "--apply-fuzzy",
        "--ignore-target draft",
        "--case-insensitive",
    ] {
        assert!(fuzzy_apply.contains(token), "{token}: {fuzzy_apply}");
    }
    let output = run_advertised_shell(fuzzy.path(), &fuzzy_apply);
    assert!(
        output.status.success(),
        "{fuzzy_apply}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = fs::read_to_string(fuzzy.path().join("source.md")).unwrap();
    assert!(
        source.contains("[[draft-noet]]"),
        "ignored target changed: {source}"
    );
    assert!(
        source.contains("[[other-note]]"),
        "fuzzy target was not repaired: {source}"
    );

    let ordinary = TempDir::new().unwrap();
    fs::create_dir(ordinary.path().join("sub")).unwrap();
    fs::write(ordinary.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    fs::write(ordinary.path().join("sub/Corina.md"), "# Corina\n").unwrap();
    fs::write(ordinary.path().join("source.md"), "[[Corina]]\n").unwrap();
    let preview = run(
        ordinary.path(),
        &[
            "links",
            "fix",
            "--expand-short-form",
            "--format",
            "json",
            "--hints",
        ],
    );
    let json: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let apply = hint_command(&json, "Apply 1 fixes");
    assert!(apply.contains("links fix --apply"), "{apply}");
    assert!(!apply.contains("--apply-fuzzy"), "{apply}");
    assert!(apply.contains("--expand-short-form"), "{apply}");
    let output = run_advertised_shell(ordinary.path(), &apply);
    assert!(
        output.status.success(),
        "{apply}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(ordinary.path().join("source.md")).unwrap(),
        "[[sub/Corina]]\n"
    );
}
