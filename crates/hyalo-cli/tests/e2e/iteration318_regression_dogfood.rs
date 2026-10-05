//! Iteration 318: regression dogfood fixes before 0.25.0.

use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: Alpha\ntags: [search]\n---\n# Alpha\n\nsnapshot index body\n",
    );
    write_md(
        tmp.path(),
        "b.md",
        "---\ntitle: Beta\n---\n# Beta\n\nanother snapshot\n",
    );
    tmp
}

fn run_json(dir: &Path, args: &[&str]) -> (Value, std::process::Output) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    (json, output)
}

// ---------------------------------------------------------------------------
// DEC-369: `--jq` runs when SIGINT is ignored
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod jq_under_ignored_sigint {
    use super::*;
    use std::process::Command;

    /// Runs `script` under `sh -c`, with `$0` the hyalo binary and `$1` the vault.
    fn sh(script: &str, dir: &Path) -> std::process::Output {
        Command::new("sh")
            .arg("-c")
            .arg(script)
            .arg(assert_cmd::cargo::cargo_bin("hyalo"))
            .arg(dir)
            .output()
            .unwrap()
    }

    fn assert_prints(output: &std::process::Output, expected: &str) {
        assert!(
            output.status.success(),
            "exit {:?}, stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
    }

    #[test]
    fn jq_find_runs_as_a_background_job_of_a_non_interactive_shell() {
        let tmp = vault();
        let out = sh(
            r#""$0" --dir "$1" find snapshot --jq .total & wait $!"#,
            tmp.path(),
        );
        assert_prints(&out, "2");
    }

    #[test]
    fn jq_summary_runs_with_sigint_trapped_to_ignore() {
        let tmp = vault();
        let out = sh(
            r#"trap '' INT; "$0" --dir "$1" summary --jq .results.files.total"#,
            tmp.path(),
        );
        assert_prints(&out, "2");
    }

    #[test]
    fn jq_set_dry_run_runs_with_sigint_ignored() {
        let tmp = vault();
        let out = sh(
            r#"trap '' INT; "$0" --dir "$1" set a.md --property x=1 --dry-run --jq '.results.modified | length'"#,
            tmp.path(),
        );
        assert_prints(&out, "1");
    }

    #[test]
    fn jq_runs_under_nohup() {
        let tmp = vault();
        let out = sh(
            r#"command -v nohup >/dev/null || { echo 2; exit 0; }; nohup "$0" --dir "$1" find snapshot --jq .total 2>/dev/null"#,
            tmp.path(),
        );
        assert_prints(&out, "2");
    }
}

// ---------------------------------------------------------------------------
// DEC-370: invalid UTF-8 in frontmatter
// ---------------------------------------------------------------------------

/// `ok.md` plus `bad.md`, whose frontmatter title holds the bytes FF FE.
fn utf8_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "ok.md",
        "---\ntitle: Ok\ntags: [a]\n---\nbody ok\n",
    );
    std::fs::write(
        tmp.path().join("bad.md"),
        b"---\ntitle: Bad \xff\xfe\ntags: [b]\n---\nbody\n",
    )
    .unwrap();
    tmp
}

const LOSSY_WARNING: &str = "1 file contains invalid UTF-8 — read lossily";

#[test]
fn frontmatter_only_scans_read_a_non_utf8_frontmatter_lossily_on_disk_and_index() {
    let tmp = utf8_vault();
    for index in [false, true] {
        if index {
            let (_, out) = run_json(tmp.path(), &["create-index", "--format", "json"]);
            assert!(out.status.success(), "{out:?}");
        }
        let extra: &[&str] = if index { &["--index"] } else { &[] };
        for (args, expected) in [
            (&["tags", "--jq", ".results | length"][..], "2"),
            (&["properties", "--jq", ".results | length"][..], "2"),
            (&["find", "--fields", "file", "--jq", ".total"][..], "2"),
            (&["find", "--count"][..], "2"),
        ] {
            let out = hyalo_no_hints()
                .arg("--dir")
                .arg(tmp.path())
                .args(args)
                .args(extra)
                .output()
                .unwrap();
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(out.status.success(), "{args:?} index={index}: {stderr}");
            assert_eq!(
                String::from_utf8_lossy(&out.stdout).trim(),
                expected,
                "{args:?}"
            );
            assert!(
                stderr.contains(LOSSY_WARNING),
                "{args:?} index={index}: {stderr}"
            );
            assert!(!stderr.contains("unreadable"), "{stderr}");
        }
    }
}

#[test]
fn named_write_or_frontmatter_read_of_a_non_utf8_frontmatter_exits_1_naming_the_file() {
    let tmp = utf8_vault();
    for args in [
        &["set", "bad.md", "--property", "x=1"][..],
        &["read", "bad.md", "--frontmatter"][..],
    ] {
        let (_, out) = run_json(tmp.path(), &[args, &["--format", "json"]].concat());
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(text.contains("bad.md"), "{args:?}: {text}");
        assert!(text.contains("not valid UTF-8"), "{args:?}: {text}");
    }
    let bytes = std::fs::read(tmp.path().join("bad.md")).unwrap();
    assert!(bytes.windows(2).any(|w| w == b"\xff\xfe"), "file untouched");
}

#[test]
fn plain_read_of_a_non_utf8_frontmatter_note_prints_its_body() {
    let tmp = utf8_vault();
    let out = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["read", "bad.md", "--format", "text"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("body"));
}

/// Runs `hyalo --dir <dir> <args>` and returns (exit code, stdout, stderr).
fn run_text(dir: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let out = hyalo_no_hints()
        .arg("--dir")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).trim().to_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// ---------------------------------------------------------------------------
// DEC-371: `prefix*` and `terms PREFIX` fold accents and case
// ---------------------------------------------------------------------------

#[test]
fn accented_prefix_and_terms_prefix_fold_like_a_bare_word_on_disk_and_index() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "cv.md",
        "# CV\n\nMy résumé lists get_user_name.\n",
    );
    write_md(
        tmp.path(),
        "other.md",
        "# Other\n\nResume the work later.\n",
    );
    write_md(tmp.path(), "none.md", "# None\n\nNothing here.\n");
    for index in [false, true] {
        if index {
            let (code, _, err) = run_text(tmp.path(), &["create-index"]);
            assert_eq!(code, Some(0), "{err}");
        }
        let extra: &[&str] = if index { &["--index"] } else { &[] };
        let count = |q: &str| {
            run_text(
                tmp.path(),
                &[&["find", "--count"], extra, &["--", q]].concat(),
            )
            .1
        };
        let plain = count("resume*");
        assert_eq!(plain, "2");
        for q in ["résumé*", "Résum*", "RESUM*"] {
            assert_eq!(count(q), plain, "{q} index={index}");
        }
        let terms = |p: &str| {
            run_text(
                tmp.path(),
                &[&["terms", p, "--format", "text"], extra].concat(),
            )
            .1
        };
        assert_eq!(terms("rés"), terms("res"), "index={index}");
        assert_eq!(terms("conf*"), terms("conf"), "index={index}");
        assert!(terms("get_user").contains("getusernam"), "index={index}");
    }
}
