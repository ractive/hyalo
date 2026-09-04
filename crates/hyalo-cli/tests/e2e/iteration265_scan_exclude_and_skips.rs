//! Iteration 265 — vault-wide `[scan] exclude`, collapsed skip diagnostics,
//! index/disk parity, and the malformed-config gate.
//!
//! Each test pins one finding from the v0.22.0 Obsidian-vault dogfood run:
//! 251 lines of per-file YAML excerpts on every scanning command (UX-1) with
//! no accounting for the files they described (UX-2); no vault-wide exclusion
//! knob at all (DEC-277); `links auto --index` aborting where the disk scan
//! skipped (BUG-8); `create-index` counting an invalid-UTF-8 file in the BM25
//! corpus the disk scan drops (BUG-14); an unaligned stale-index check (UX-7);
//! and a malformed `.hyalo.toml` letting `lint` pass in CI with the rules it
//! was supposed to apply silently missing (BUG-19).

use super::common::{hyalo, hyalo_no_hints, write_md};
use tempfile::TempDir;

/// Frontmatter an Obsidian Templater template carries: `{{date}}` is a
/// template expression, not YAML, so the block will never parse.
const TEMPLATE_FM: &str = "---\ntitle: Album\ncreated: {{date}}\nrating:\n---\n\nBody.\n";

/// A vault with two unparsable templates under `Templates/` and three good
/// notes, the shape that produced the dogfood report's numbers in miniature.
fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n\nAlpha body.\n");
    write_md(tmp.path(), "b.md", "---\ntitle: B\n---\n\nBeta body.\n");
    write_md(
        tmp.path(),
        "notes/c.md",
        "---\ntitle: C\n---\n\nGamma body.\n",
    );
    write_md(tmp.path(), "Templates/album.md", TEMPLATE_FM);
    write_md(tmp.path(), "Templates/book.md", TEMPLATE_FM);
    tmp
}

/// Stderr of a command run in `dir`, as lines.
fn stderr_lines(dir: &std::path::Path, args: &[&str]) -> Vec<String> {
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Parsed JSON envelope of a command run in `dir`.
fn json(dir: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

// ---------------------------------------------------------------------------
// SCAN-2 / DEC-278 — one summary line, not one excerpt per file
// ---------------------------------------------------------------------------

/// UX-1: every scanning command collapses its per-file YAML diagnostics into a
/// single stderr line naming the count and where to see the detail.
#[test]
fn unparsable_frontmatter_collapses_to_one_line_on_every_scanning_command() {
    let tmp = vault();
    for args in [
        vec!["find", "--limit", "1"],
        vec!["summary"],
        vec!["tags"],
        vec!["properties"],
        vec!["mv", "a.md", "--to", "renamed.md", "--dry-run"],
    ] {
        let lines = stderr_lines(tmp.path(), &args);
        let skip_lines: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("unparsable frontmatter"))
            .collect();
        assert_eq!(
            skip_lines.len(),
            1,
            "{args:?} should print exactly one skip line, got: {lines:?}"
        );
        assert!(
            skip_lines[0].contains("skipped 2 files"),
            "{args:?}: {}",
            skip_lines[0]
        );
        assert!(
            skip_lines[0].contains("HYALO005"),
            "the line must point at the command that lists them: {}",
            skip_lines[0]
        );
        // The multi-line serde_yaml excerpt is what buried the real output.
        assert!(
            !lines.iter().any(|l| l.contains("unexpected end of input")),
            "{args:?} still streams the YAML excerpt: {lines:?}"
        );
    }
}

/// `-q` silences the summary line, exactly as it silences every other warning.
#[test]
fn quiet_suppresses_the_skip_summary_line() {
    let tmp = vault();
    let lines = stderr_lines(tmp.path(), &["find", "--limit", "1", "-q"]);
    assert!(lines.is_empty(), "-q should leave stderr empty, got {lines:?}");
}

/// `[scan] verbose_skips = true` restores the per-file diagnostics for a vault
/// that wants them, so nothing is lost — only relocated behind a switch.
#[test]
fn verbose_skips_restores_the_per_file_diagnostics() {
    let tmp = vault();
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \".\"\n\n[scan]\nverbose_skips = true\n",
    )
    .unwrap();
    let lines = stderr_lines(tmp.path(), &["find", "--limit", "1"]);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("skipping Templates/album.md")),
        "expected the per-file line, got {lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("unparsable frontmatter")),
        "the collapsed summary must not double up with the detail: {lines:?}"
    );
}

/// UX-2: `summary` accounts for the files it could not use instead of quietly
/// reporting a smaller vault than the one on disk.
#[test]
fn summary_reports_skipped_and_excluded_counts() {
    let tmp = vault();
    let val = json(tmp.path(), &["summary"]);
    let files = &val["results"]["files"];
    assert_eq!(files["total"], 3);
    assert_eq!(files["skipped"], 2);
    assert_eq!(files["excluded"], 0);

    // Per-directory attribution says *where* the unusable files are.
    let dirs = files["directories"].as_array().unwrap();
    let templates = dirs
        .iter()
        .find(|d| d["directory"] == "Templates")
        .expect("Templates/ must appear even though every file in it was skipped");
    assert_eq!(templates["skipped"], 2);

    // A clean directory omits the key entirely rather than carrying a zero.
    let root = dirs.iter().find(|d| d["directory"] == ".").unwrap();
    assert!(root.get("skipped").is_none(), "clean dirs stay compact: {root}");
}

/// Text mode says the same thing in one line.
#[test]
fn summary_text_names_the_skipped_count() {
    let tmp = vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Files: 3 (2 skipped, 0 excluded)"),
        "got: {stdout}"
    );
}

/// A vault with nothing to skip keeps the bare `Files: N` line it always had.
#[test]
fn summary_text_stays_bare_when_nothing_was_skipped() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n\nAlpha.\n");
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Files: 1\n"), "got: {stdout}");
    assert!(!stdout.contains("skipped"), "got: {stdout}");
}

// ---------------------------------------------------------------------------
// SCAN-1 / DEC-277 — `[scan] exclude` is honoured everywhere
// ---------------------------------------------------------------------------

/// A vault whose `Templates/` tree is excluded outright.
fn excluded_vault() -> TempDir {
    let tmp = vault();
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \".\"\n\n[scan]\nexclude = [\"Templates/**\"]\n",
    )
    .unwrap();
    tmp
}

/// Every command that discovers vault files agrees on the same file set.
#[test]
fn scan_exclude_is_honoured_by_every_discovering_command() {
    let tmp = excluded_vault();

    let find = json(tmp.path(), &["find"]);
    assert_eq!(find["results"].as_array().unwrap().len(), 3);

    let summary = json(tmp.path(), &["summary"]);
    assert_eq!(summary["results"]["files"]["total"], 3);
    assert_eq!(summary["results"]["files"]["excluded"], 2);

    let index = json(tmp.path(), &["create-index"]);
    assert_eq!(
        index["results"]["files_indexed"], 3,
        "create-index must not index what the disk scan excludes"
    );

    // Nothing under Templates/ can be reached by name through a glob either.
    let globbed = json(tmp.path(), &["find", "--glob", "Templates/*.md"]);
    assert!(globbed["results"].as_array().unwrap().is_empty());
}

/// The excluded files are gone *before* they can be parsed, so their YAML
/// diagnostics never happen — exclusion is also the cure for the noise.
#[test]
fn scan_exclude_removes_the_skip_warnings_entirely() {
    let tmp = excluded_vault();
    let lines = stderr_lines(tmp.path(), &["find", "--limit", "1"]);
    assert!(
        !lines.iter().any(|l| l.contains("unparsable frontmatter")),
        "excluded files must not be parsed at all: {lines:?}"
    );
}

/// An explicitly named excluded file is refused, naming the glob — never
/// silently reported as "nothing to do".
#[test]
fn explicitly_named_excluded_file_is_refused_naming_the_glob() {
    let tmp = excluded_vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--file", "Templates/album.md", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "an excluded target must fail");
    let val: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let error = val["error"].as_str().unwrap();
    assert!(error.contains("[scan] exclude"), "got: {error}");
    assert!(error.contains("Templates/**"), "must name the glob: {error}");
}

/// `hyalo config` reports the effective list in both surfaces.
#[test]
fn config_reports_the_effective_scan_exclude() {
    let tmp = excluded_vault();
    let val = json(tmp.path(), &["config"]);
    assert_eq!(
        val["results"]["scan"]["exclude"],
        serde_json::json!(["Templates/**"])
    );
    assert_eq!(val["results"]["scan"]["include"], serde_json::json!([]));
    assert_eq!(val["results"]["scan"]["verbose_skips"], false);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("scan.exclude: Templates/**"),
        "got: {stdout}"
    );
}

/// An unconfigured vault reports an empty exclusion list, not a missing key.
#[test]
fn config_reports_an_empty_scan_exclude_by_default() {
    let tmp = vault();
    let val = json(tmp.path(), &["config"]);
    assert_eq!(val["results"]["scan"]["exclude"], serde_json::json!([]));
}

/// A `--index` read honours the exclusion even against a snapshot built before
/// it was configured, so turning the knob needs no rebuild.
#[test]
fn scan_exclude_applies_to_an_index_built_before_it_was_configured() {
    let tmp = vault();
    let built = json(tmp.path(), &["create-index"]);
    assert_eq!(built["results"]["files_indexed"], 3);
    write_md(tmp.path(), "notes/d.md", "---\ntitle: D\n---\n\nDelta.\n");
    let rebuilt = json(tmp.path(), &["create-index"]);
    assert_eq!(rebuilt["results"]["files_indexed"], 4);

    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \".\"\n\n[scan]\nexclude = [\"notes/**\"]\n",
    )
    .unwrap();
    let val = json(tmp.path(), &["find", "--index"]);
    let files: Vec<&str> = val["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["file"].as_str().unwrap())
        .collect();
    assert_eq!(files, vec!["a.md", "b.md"], "index load must drop excluded entries");
}

// ---------------------------------------------------------------------------
// INDEX-1 — index/disk parity (BUG-8, BUG-14)
// ---------------------------------------------------------------------------

/// A scratch vault carrying one invalid-UTF-8 file alongside real notes.
fn utf8_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\n---\n\nindex term index alpha\n",
    );
    write_md(
        tmp.path(),
        "b.md",
        "---\ntitle: B\n---\n\nindex beta gamma delta epsilon zeta eta theta\n",
    );
    std::fs::write(tmp.path().join("bad.md"), b"line ok\n\xff x\n").unwrap();
    tmp
}

/// BUG-14: the invalid-UTF-8 file is out of the BM25 corpus on both paths, so
/// `--index` scores match the disk scan's exactly rather than being shifted by
/// a phantom document.
#[test]
fn invalid_utf8_file_scores_identically_on_disk_and_via_the_index() {
    let tmp = utf8_vault();
    let disk = json(tmp.path(), &["find", "index", "--limit", "3"]);
    let disk_score = disk["results"][0]["score"].as_f64().unwrap();

    let built = json(tmp.path(), &["create-index"]);
    assert_eq!(
        built["results"]["warnings"], 1,
        "the excluded file must be reported, not silently dropped"
    );

    let indexed = json(tmp.path(), &["find", "index", "--limit", "3", "--index"]);
    let index_score = indexed["results"][0]["score"].as_f64().unwrap();
    assert!(
        (disk_score - index_score).abs() < 1e-6,
        "disk {disk_score} vs index {index_score}"
    );
}

/// A vault whose only file is unreadable still reports it exactly once.
#[test]
fn create_index_reports_the_invalid_utf8_file_once() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    std::fs::write(tmp.path().join("bad.md"), b"line ok\n\xff x\n").unwrap();
    let val = json(tmp.path(), &["create-index"]);
    assert_eq!(val["results"]["warnings"], 1);
}

/// BUG-8: an unparsable file created after the snapshot used to abort
/// `links auto --index` with exit 2 while the disk run skipped it and carried
/// on. Both now exit 0 with the same match count.
#[test]
fn links_auto_index_skips_an_unparsable_file_instead_of_aborting() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(tmp.path(), "target.md", "---\ntitle: Kestrel\n---\n\nBody.\n");
    write_md(
        tmp.path(),
        "source.md",
        "---\ntitle: Source\n---\n\nA note about Kestrel here.\n",
    );
    let built = hyalo_no_hints()
        .current_dir(tmp.path())
        .arg("create-index")
        .output()
        .unwrap();
    assert!(built.status.success());

    // Created outside hyalo *after* the snapshot: the refresh path picks it up
    // and used to fail on it.
    write_md(tmp.path(), "Templates/album.md", TEMPLATE_FM);

    let disk = json(tmp.path(), &["links", "auto", "--dry-run"]);
    let indexed = json(tmp.path(), &["links", "auto", "--index", "--dry-run"]);
    assert_eq!(
        indexed["results"]["matched"], disk["results"]["matched"],
        "the index refresh must reach the same verdict as the disk scan"
    );
}

// ---------------------------------------------------------------------------
// INDEX-2 / DEC-280 — refresh what you were asked for, warn only otherwise
// ---------------------------------------------------------------------------

/// A `--index` read that names its target reports the file as it is now, not
/// as the snapshot remembers it.
#[test]
fn index_read_of_a_named_file_refreshes_it_from_disk() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(tmp.path(), "new.md", "---\ntitle: New\n---\n\nline1\n");
    assert!(
        hyalo()
            .current_dir(tmp.path())
            .arg("create-index")
            .output()
            .unwrap()
            .status
            .success()
    );

    std::fs::OpenOptions::new()
        .append(true)
        .open(tmp.path().join("new.md"))
        .and_then(|mut f| std::io::Write::write_all(&mut f, b"line2\nline3\nline4\n"))
        .unwrap();

    let disk = json(tmp.path(), &["find", "--file", "new.md", "--fields", "lines"]);
    let indexed = json(
        tmp.path(),
        &["find", "--index", "--file", "new.md", "--fields", "lines"],
    );
    assert_eq!(
        indexed["results"][0]["lines"], disk["results"][0]["lines"],
        "a named --index target must not answer from a stale snapshot"
    );
}

// ---------------------------------------------------------------------------
// CONFIG-1 / DEC-279 — a broken config fails a gate, not just a write
// ---------------------------------------------------------------------------

/// A vault whose `.hyalo.toml` does not parse.
fn malformed_config_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    // `exclude` at the top level is not a key hyalo has — a plausible typo for
    // `[scan] exclude`, and exactly the shape that used to pass CI silently.
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \"kb\"\nexclude = [\"Templates/**\"]\n",
    )
    .unwrap();
    write_md(tmp.path(), "kb/a.md", "---\ntitle: A\n---\n\nAlpha.\n");
    tmp
}

/// BUG-19: `lint` is a CI gate, and a gate computed without the config's rules
/// must fail rather than report a verdict nobody configured.
#[test]
fn malformed_config_makes_lint_exit_one() {
    let tmp = malformed_config_vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .arg("lint")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("TOML parse error"),
        "the refusal must carry the parse error: {stderr}"
    );
}

/// The other gates behave the same way.
#[test]
fn malformed_config_fails_the_other_gate_commands() {
    let tmp = malformed_config_vault();
    for args in [
        vec!["find", "--broken-links", "--strict"],
        vec!["views", "run", "anything"],
    ] {
        let output = hyalo_no_hints()
            .current_dir(tmp.path())
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{args:?} should refuse to answer on a broken config"
        );
    }
}

/// A plain read still answers — with a warning `-q` cannot hide, because the
/// answer was computed against a configuration the user did not write.
#[test]
fn malformed_config_still_answers_a_plain_read_with_a_quiet_proof_warning() {
    let tmp = malformed_config_vault();
    for extra in [vec![], vec!["-q"]] {
        let output = hyalo_no_hints()
            .current_dir(tmp.path())
            .args(["find", "--count"])
            .args(&extra)
            .output()
            .unwrap();
        assert!(output.status.success(), "read should still answer {extra:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("malformed .hyalo.toml"),
            "the warning must survive {extra:?}"
        );
    }
}

/// `find` without `--strict` is a report, not a gate, so it keeps answering.
#[test]
fn malformed_config_does_not_gate_a_non_strict_find() {
    let tmp = malformed_config_vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--broken-links"])
        .output()
        .unwrap();
    assert!(output.status.success());
}
