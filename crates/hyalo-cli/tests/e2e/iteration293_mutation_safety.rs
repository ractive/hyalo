//! Focused iteration 293 regressions for deterministic refusal and identity safety.
use crate::common::hyalo_no_hints;
use std::fs;
use tempfile::TempDir;

fn vault() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    dir
}

fn run(dir: &TempDir, args: &[&str]) -> std::process::Output {
    hyalo_no_hints()
        .current_dir(dir.path())
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn append_empty_list_then_mapping_refuses_before_writing_either_file() {
    let dir = vault();
    let a = b"---\nx: []\n---\nA\n";
    let b = b"---\nx: {k: v}\n---\nB\n";
    fs::write(dir.path().join("a.md"), a).unwrap();
    fs::write(dir.path().join("b.md"), b).unwrap();

    let output = run(
        &dir,
        &[
            "append",
            "--file",
            "a.md",
            "--file",
            "b.md",
            "--property",
            "x=one",
        ],
    );
    assert!(!output.status.success());
    assert_eq!(fs::read(dir.path().join("a.md")).unwrap(), a);
    assert_eq!(fs::read(dir.path().join("b.md")).unwrap(), b);
}

#[test]
fn task_line_six_preflight_refuses_before_toggling_first_file() {
    let dir = vault();
    let a = b"---\ntitle: A\n---\nintro\n- [ ] task\n";
    let b = b"---\ntitle: B\n---\nintro\nordinary text\n";
    fs::write(dir.path().join("a.md"), a).unwrap();
    fs::write(dir.path().join("b.md"), b).unwrap();

    let output = run(
        &dir,
        &[
            "task", "toggle", "--file", "a.md", "--file", "b.md", "--line", "6",
        ],
    );
    assert!(!output.status.success());
    assert_eq!(fs::read(dir.path().join("a.md")).unwrap(), a);
    assert_eq!(fs::read(dir.path().join("b.md")).unwrap(), b);
}

#[test]
fn unsupported_count_refuses_early_index_and_remaining_write_leaves() {
    let commands: &[&[&str]] = &[
        &["create-index", "--count"],
        &["drop-index", "--count"],
        &["new", "--type", "note", "--file", "new.md", "--count"],
        &["views", "set", "mine", "--count"],
        &["lint-rules", "set", "MD013", "--severity", "off", "--count"],
        &["madr", "toc", "--apply", "--count"],
        &[
            "changelog",
            "add",
            "--category",
            "Fixed",
            "--message",
            "x",
            "--apply",
            "--count",
        ],
    ];
    for args in commands {
        let dir = vault();
        fs::write(dir.path().join(".hyalo-index"), b"preserve index").unwrap();
        let before: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (entry.file_name(), fs::read(entry.path()).unwrap())
            })
            .collect();
        let output = run(&dir, args);
        assert!(!output.status.success(), "{args:?}");
        let after: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (entry.file_name(), fs::read(entry.path()).unwrap())
            })
            .collect();
        assert_eq!(after, before, "{args:?}");
    }
}

#[test]
fn custom_drop_removes_only_the_selected_index() {
    let dir = vault();
    fs::write(dir.path().join("custom.idx"), b"custom").unwrap();
    fs::write(dir.path().join(".hyalo-index"), b"default").unwrap();

    let output = run(&dir, &["--index-file", "custom.idx", "drop-index"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!dir.path().join("custom.idx").exists());
    assert_eq!(
        fs::read(dir.path().join(".hyalo-index")).unwrap(),
        b"default"
    );
}

#[cfg(unix)]
#[test]
fn task_toggle_rejects_duplicate_symlink_and_hard_link_targets() {
    for hard_link in [false, true] {
        let dir = vault();
        let original = b"- [ ] task\n";
        let source = dir.path().join("a.md");
        let alias = dir.path().join("alias.md");
        fs::write(&source, original).unwrap();
        if hard_link {
            fs::hard_link(&source, &alias).unwrap();
        } else {
            std::os::unix::fs::symlink("a.md", &alias).unwrap();
        }
        let output = run(
            &dir,
            &[
                "task", "toggle", "--file", "a.md", "--file", "alias.md", "--all",
            ],
        );
        assert!(!output.status.success(), "hard_link={hard_link}");
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(fs::read(&alias).unwrap(), original);
    }
}

#[test]
fn task_toggle_rejects_duplicate_path_spellings() {
    let dir = vault();
    let original = b"- [ ] task\n";
    fs::write(dir.path().join("a.md"), original).unwrap();
    let output = run(
        &dir,
        &[
            "task", "toggle", "--file", "a.md", "--file", "./a.md", "--all",
        ],
    );
    assert!(!output.status.success());
    assert_eq!(fs::read(dir.path().join("a.md")).unwrap(), original);
}

#[test]
fn indexed_generators_reconcile_every_created_markdown_note() {
    let dir = vault();
    fs::create_dir_all(dir.path().join("docs/decisions")).unwrap();
    fs::write(
        dir.path().join("docs/decisions/0001-one.md"),
        "---\ntitle: One\ntype: adr\n---\n# One\n",
    )
    .unwrap();
    assert!(run(&dir, &["create-index"]).status.success());

    let changelog = run(
        &dir,
        &[
            "--index-file",
            ".hyalo-index",
            "changelog",
            "add",
            "--category",
            "Added",
            "--message",
            "created",
            "--apply",
        ],
    );
    assert!(
        changelog.status.success(),
        "{}",
        String::from_utf8_lossy(&changelog.stderr)
    );
    assert_eq!(
        run(
            &dir,
            &["find", "--index", "--file", "CHANGELOG.md", "--count"]
        )
        .stdout,
        b"1\n"
    );

    let okf_index = run(
        &dir,
        &["--index-file", ".hyalo-index", "okf", "index", "--apply"],
    );
    assert!(
        okf_index.status.success(),
        "{}",
        String::from_utf8_lossy(&okf_index.stderr)
    );
    assert_eq!(
        run(&dir, &["find", "--index", "--file", "index.md", "--count"]).stdout,
        b"1\n"
    );

    let log = run(
        &dir,
        &[
            "--index-file",
            ".hyalo-index",
            "okf",
            "log",
            "--message",
            "created",
            "--apply",
        ],
    );
    assert!(
        log.status.success(),
        "{}",
        String::from_utf8_lossy(&log.stderr)
    );
    assert_eq!(
        run(&dir, &["find", "--index", "--file", "log.md", "--count"]).stdout,
        b"1\n"
    );

    let madr = run(
        &dir,
        &["--index-file", ".hyalo-index", "madr", "toc", "--apply"],
    );
    assert!(
        madr.status.success(),
        "{}",
        String::from_utf8_lossy(&madr.stderr)
    );
    assert_eq!(
        run(
            &dir,
            &[
                "find",
                "--index",
                "--file",
                "docs/decisions/README.md",
                "--count",
            ],
        )
        .stdout,
        b"1\n"
    );
}
