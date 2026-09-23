//! Conservative fragment repairs included in ordinary --apply (iteration 301).
use super::common::{hyalo, hyalo_no_hints, write_md};
use serde_json::Value;
use tempfile::TempDir;

fn run(dir: &std::path::Path, extra: &[&str]) -> Value {
    let mut cmd = hyalo_no_hints();
    cmd.arg("--dir")
        .arg(dir)
        .args(["links", "fix", "--format", "json"])
        .args(extra);
    let output = cmd.assert().success().get_output().stdout.clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    value["results"].clone()
}

#[test]
fn numbered_anchor_is_included_in_apply_and_second_apply_is_noop() {
    let tmp = TempDir::new().unwrap();
    let source = "---\r\ntitle: Source\r\n---\r\n[x](target.md?q=1#success-metrics \"title\")\r\n";
    write_md(tmp.path(), "source.md", source);
    write_md(tmp.path(), "target.md", "## 6. Success metrics\n");
    for flags in [&[][..], &["--dry-run"][..], &["--apply-fuzzy"][..]] {
        let result = run(tmp.path(), flags);
        assert_eq!(result["broken"], 0);
        assert_eq!(result["broken_anchors"], 1);
        assert_eq!(result["anchor_fixable"], 1);
        assert_eq!(result["fixable"], 0);
        assert_eq!(result["anchors_applied"], 0);
        assert_eq!(result["anchor_fixes"][0]["line"], 4);
        assert_eq!(result["anchor_fixes"][0]["heading"], "6. Success metrics");
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("source.md")).unwrap(),
            source
        );
    }
    for flags in [&["--apply"][..], &["--apply", "--apply-fuzzy"][..]] {
        write_md(tmp.path(), "source.md", source);
        let result = run(tmp.path(), flags);
        assert_eq!(result["anchors_applied"], 1);
        assert_eq!(
            result["broken_anchors"], 1,
            "counts describe pre-apply state"
        );
        assert_eq!(result["applied_fixes"].as_array().unwrap().len(), 0);
        assert_eq!(result["applied"], true);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("source.md")).unwrap(),
            source.replace("#success-metrics", "#6-success-metrics")
        );
        let again = run(tmp.path(), flags);
        assert_eq!(again["broken_anchors"], 0);
        assert_eq!(again["anchors_applied"], 0);
        assert_eq!(again["applied"], false);
    }
}

#[test]
fn anchor_preview_text_scope_ignored_targets_and_ambiguous_deferrals() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "source.md",
        "[x](target.md#success-metrics)\n[x](ambiguous.md#success-metrics)\n[x](missing.md#success-metrics)\n",
    );
    write_md(tmp.path(), "target.md", "## 6. Success metrics\n");
    write_md(
        tmp.path(),
        "ambiguous.md",
        "## 6. Success metrics\n## 7. Success metrics\n",
    );
    let result = run(tmp.path(), &["--dry-run"]);
    assert_eq!(result["broken"], 1);
    assert_eq!(result["broken_anchors"], 2);
    assert_eq!(result["anchor_fixable"], 1);
    assert_eq!(result["anchors_deferred"], 1);
    assert!(
        result["deferred_anchor_fixes"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("ambiguous")
    );
    let result = run(tmp.path(), &["--dry-run", "--ignore-target", "target"]);
    assert_eq!(result["anchor_fixable"], 0);
    let result = run(tmp.path(), &["--dry-run", "--glob", "target.md"]);
    assert_eq!(result["broken_anchors"], 0);
    let mut cmd = hyalo_no_hints();
    let output = cmd
        .arg("--dir")
        .arg(tmp.path())
        .args(["links", "fix", "--dry-run", "--format", "text"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(
        text.contains("#success-metrics → #6-success-metrics"),
        "{text}"
    );
    assert!(text.contains("Deferred anchors"), "{text}");
    assert!(text.contains("source.md line 1"), "{text}");
    assert!(text.contains("included with --apply"), "{text}");
    let applied = run(tmp.path(), &["--apply"]);
    assert_eq!(applied["anchors_applied"], 1);
    assert_eq!(applied["anchors_deferred"], 1);
    let source = std::fs::read_to_string(tmp.path().join("source.md")).unwrap();
    assert!(source.contains("target.md#6-success-metrics"));
    assert!(source.contains("ambiguous.md#success-metrics"));
}

#[test]
fn anchor_only_preview_offers_the_normal_apply_hint() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "source.md", "[x](target.md#success-metrics)\n");
    write_md(tmp.path(), "target.md", "## 6. Success metrics\n");
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["links", "fix", "--dry-run", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    let hints = value["hints"].as_array().unwrap();
    let apply: Vec<_> = hints.iter().filter(|hint| hint["writes"] == true).collect();
    assert_eq!(apply.len(), 1, "{value}");
    assert_eq!(apply[0]["description"], "Apply 1 fixes");
    let command = apply[0]["cmd"].as_str().unwrap();
    assert!(command.contains("links fix --apply"), "{command}");
    assert!(!command.contains("--apply-anchors"), "{command}");
}

#[test]
fn snapshot_anchor_application_refreshes_the_index() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "source.md", "[x](target.md#success-metrics)\n");
    write_md(tmp.path(), "target.md", "## 6. Success metrics\n");
    hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .arg("create-index")
        .assert()
        .success();
    let disk = run(tmp.path(), &["--dry-run"]);
    let snapshot = run(tmp.path(), &["--dry-run", "--index"]);
    assert_eq!(disk, snapshot);
    let applied = run(tmp.path(), &["--apply", "--index"]);
    assert_eq!(applied["anchors_applied"], 1);
    let again = run(tmp.path(), &["--dry-run", "--index"]);
    assert_eq!(again["broken_anchors"], 0);
    hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--broken-links", "--strict", "--index"])
        .assert()
        .success();
}
