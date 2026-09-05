//! Iteration 277 — link-graph parity, `links fix` reporting, hint threading
//! and write performance.
//!
//! Each test pins one finding from the post-batch-271-274 dogfood report. The
//! performance work itself (DEC-317's batched durability fsync, PREFIX-1's
//! in-memory resolution) is not timed here — a wall-clock assertion is a
//! flaky test on shared CI hardware — but its *observable* consequences are:
//! results must stay byte-identical to the serial path, and a bulk phase must
//! still leave every file atomically replaced.

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn hyalo_no_hints() -> Command {
    crate::common::hyalo_no_hints()
}

fn hyalo() -> Command {
    crate::common::hyalo()
}

fn write_md(dir: &std::path::Path, rel: &str, body: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

fn json(tmp: &TempDir, args: &[&str]) -> Value {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "`hyalo {}` did not produce JSON ({e}): {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

// ---------------------------------------------------------------------------
// Part C — one graph, one answer
// ---------------------------------------------------------------------------

/// BUG-16 / DEC-318: a note whose only outbound link is a *resolving*
/// attachment was a dead end to `find --dead-end` and a linked note to
/// `summary`, because the graph's own case index holds notes only and so never
/// recognised the attachment.
#[test]
fn summary_and_find_agree_on_a_note_whose_only_link_is_an_image() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("real.png"), b"x").unwrap();
    write_md(tmp.path(), "imgonly.md", "![[real.png]]\n");
    write_md(tmp.path(), "linker.md", "see [[imgonly]]\n");

    let dead_ends: Vec<String> = json(&tmp, &["find", "--dead-end"])["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["file"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(dead_ends, vec!["imgonly.md".to_owned()]);

    let summary = json(&tmp, &["summary"]);
    assert_eq!(
        summary["results"]["dead_ends"], 1,
        "summary must count the same dead end find reports: {summary}"
    );
    assert_eq!(summary["results"]["orphans"], 0);
}

/// The same predicate applies to a *broken* attachment reference: resolution
/// never crosses an explicit extension, so `![[missing.png]]` names an
/// attachment either way and is not a note-graph edge.
#[test]
fn a_broken_image_embed_is_not_a_graph_edge_but_is_still_broken() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "imgonly.md", "![[missing.png]]\n");

    let orphans: Vec<String> = json(&tmp, &["find", "--orphan"])["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["file"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        orphans,
        vec!["imgonly.md".to_owned()],
        "a missing image is not an edge to another note"
    );
    assert_eq!(json(&tmp, &["summary"])["results"]["orphans"], 1);

    let broken = json(&tmp, &["find", "--broken-links"]);
    assert_eq!(
        broken["total"], 1,
        "and it is still a broken link: {broken}"
    );
}

/// BUG-45: `links fix` reported `broken_anchors: 0` whenever any target was
/// broken — the normal case on a real corpus — instead of the count
/// `find --broken-links` computes.
#[test]
fn links_fix_reports_broken_anchors_even_when_targets_are_broken() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "target.md", "# Target\n\n## Real Heading\n");
    write_md(
        tmp.path(),
        "src.md",
        "a [[target#No Such Heading]] and a [[definitely-missing]]\n",
    );

    let fix = json(&tmp, &["links", "fix", "--dry-run"]);
    assert_eq!(
        fix["results"]["broken_anchors"], 1,
        "the dead anchor must be counted alongside the broken target: {fix}"
    );
}

// ---------------------------------------------------------------------------
// Part D — `links fix` reporting
// ---------------------------------------------------------------------------

/// BUG-18 / DEC-319: a fuzzy winner that only just outran a real runner-up is
/// contested, and its confidence is scaled down so it lands below the apply
/// floor and is reported for review instead of written.
#[test]
fn a_contested_fuzzy_winner_falls_below_the_apply_floor() {
    let tmp = TempDir::new().unwrap();
    for stem in ["catmuse", "catnip", "catalog", "cattle", "catcher"] {
        write_md(tmp.path(), &format!("{stem}.md"), "x\n");
    }
    write_md(tmp.path(), "src.md", "see [[Cat]]\n");

    let fix = json(&tmp, &["links", "fix", "--dry-run"]);
    let fuzzy = fix["results"]["fuzzy_fixes"].as_array().unwrap();
    for plan in fuzzy {
        assert!(
            plan["below_floor"].as_bool().unwrap_or(false),
            "a proposal contested by four near-neighbours must not be applicable: {plan}"
        );
    }
}

/// A genuinely unique match keeps its score — the damping must not punish the
/// case fuzzy matching exists for.
#[test]
fn an_uncontested_fuzzy_winner_keeps_its_confidence() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "Obsidian Publish.md", "x\n");
    write_md(tmp.path(), "src.md", "see [[Obsidian Publish.]]\n");

    let fix = json(&tmp, &["links", "fix", "--dry-run"]);
    let all = format!("{}", fix["results"]);
    assert!(
        all.contains("Obsidian Publish.md"),
        "the unique match must still be proposed: {all}"
    );
}

/// BUG-17: every reported fuzzy plan carries `emitted_target` — including the
/// below-floor ones, which are the whole reason the bucket is printed.
#[test]
fn every_fuzzy_plan_carries_an_emitted_target() {
    let tmp = TempDir::new().unwrap();
    for stem in ["jamesgreenblue", "jamesred", "jamesblack"] {
        write_md(tmp.path(), &format!("{stem}.md"), "x\n");
    }
    write_md(tmp.path(), "src.md", "see [[jamesb]]\n");

    let fix = json(&tmp, &["links", "fix", "--dry-run"]);
    let fuzzy = fix["results"]["fuzzy_fixes"].as_array().unwrap();
    assert!(!fuzzy.is_empty(), "expected a fuzzy proposal: {fix}");
    for plan in fuzzy {
        assert!(
            plan.get("emitted_target").is_some_and(|v| v.is_string()),
            "`links fix --help` promises emitted_target on every plan: {plan}"
        );
    }
}

// ---------------------------------------------------------------------------
// Part E — hints that keep the answer stable
// ---------------------------------------------------------------------------

/// BUG-15: `--site-prefix` decides which links resolve, so a hint that drops
/// it answers a different question than the command that printed it.
#[test]
fn hints_thread_site_prefix_when_it_came_from_the_cli() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "web/api.md", "# API\n");
    write_md(tmp.path(), "src.md", "see [API](/docs/web/api)\n");

    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(["--site-prefix", "docs"])
        .args(["find", "--format", "text"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    let hint_lines: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("hyalo ") && l.contains("--dir"))
        .collect();
    assert!(!hint_lines.is_empty(), "expected hints: {text}");
    for line in &hint_lines {
        assert!(
            line.contains("--site-prefix"),
            "every hint must carry the prefix the run used: {line}"
        );
    }
}

/// BUG-47: an external URI carries `path: null` but is not broken, so it must
/// not out-vote the site-absolute links in the "all broken links are
/// site-absolute" test.
#[test]
fn the_all_site_absolute_note_survives_a_page_full_of_external_urls() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "src.md",
        "[a](https://a.example) [b](https://b.example) [c](https://c.example) \
         and [gone](/en-US/docs/nowhere)\n",
    );

    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(["find", "--broken-links", "--format", "text"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("site-absolute"),
        "external URIs must not mask the site-prefix diagnosis: {text}"
    );
    assert!(
        !text.contains("Auto-fix broken links"),
        "and `links fix` must not be offered for them: {text}"
    );
}

/// UX-13: a named file that does not exist is not evidence of a stale index.
#[test]
fn a_missing_named_target_does_not_trigger_the_stale_index_warning() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "# A\n");
    hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .arg("create-index")
        .assert()
        .success();

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(["find", "--index", "--file", "nope.md"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("index older than vault"),
        "the file is missing, not the index stale: {stderr}"
    );
    assert!(
        stderr.contains("not found") || !output.status.success(),
        "and the run must still report the missing file: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Part F — read-side UX
// ---------------------------------------------------------------------------

/// UX-6: text mode under `--broken-links` prints the broken links only. JSON
/// keeps every link and its per-link verdict.
#[test]
fn broken_links_text_prints_only_the_broken_ones() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "good.md", "# Good\n");
    write_md(
        tmp.path(),
        "src.md",
        "[[good]] and [[missing]] and <https://x.example>\n",
    );

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(["find", "--broken-links", "--format", "text"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("\"missing\""), "text: {text}");
    assert!(
        !text.contains("→ \"good.md\""),
        "a working link is not why the file matched: {text}"
    );

    let value = json(&tmp, &["find", "--broken-links"]);
    let links = value["results"][0]["links"].as_array().unwrap();
    assert!(
        links.len() >= 3,
        "JSON keeps the full inventory: {value:#}"
    );
}

/// UX-10: a CommonMark autolink is a link the author marked up, and belongs in
/// the inventory as `external`.
#[test]
fn autolinks_are_inventoried_as_external() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "<https://example.com/x> and <obsidian://open?vault=v> and <div> and <not a url>\n",
    );

    let value = json(&tmp, &["find", "--fields", "links"]);
    let links = value["results"][0]["links"].as_array().unwrap();
    let targets: Vec<&str> = links
        .iter()
        .map(|l| l["target"].as_str().unwrap())
        .collect();
    assert_eq!(
        targets,
        vec!["https://example.com/x", "obsidian://open?vault=v"],
        "an HTML tag and a bracketed phrase are not autolinks: {value:#}"
    );
    for link in links {
        assert_eq!(link["kind"], "external");
        assert!(link["path"].is_null(), "never resolved: {link}");
    }
}

// ---------------------------------------------------------------------------
// Parts A and B — write phase and snapshot parity
// ---------------------------------------------------------------------------

/// DEC-317: a bulk write phase trades the per-file durability fsync for one
/// per directory, and must still replace every file atomically and completely.
#[test]
fn a_bulk_set_writes_every_file_completely() {
    let tmp = TempDir::new().unwrap();
    for i in 0..40 {
        write_md(
            tmp.path(),
            &format!("dir{}/note{i}.md", i % 4),
            "---\ntitle: T\n---\nbody\n",
        );
    }

    let value = json(
        &tmp,
        &["set", "--glob", "**/*.md", "--property", "status=done"],
    );
    assert_eq!(
        value["results"]["modified"].as_array().unwrap().len(),
        40,
        "{value:#}"
    );

    let found = json(&tmp, &["find", "--property", "status=done"]);
    assert_eq!(found["total"], 40, "{found:#}");
    for i in 0..40 {
        let text =
            std::fs::read_to_string(tmp.path().join(format!("dir{}/note{i}.md", i % 4))).unwrap();
        assert!(text.contains("status: done"), "note{i}: {text}");
        assert!(text.ends_with("body\n"), "body survived: {text}");
    }
}

/// BUG-24: a snapshot records the files it skipped for unparsable frontmatter,
/// so `summary --index` reports the same figure as a disk scan.
#[test]
fn summary_index_reports_the_same_skipped_count_as_a_disk_scan() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "ok.md", "---\ntitle: OK\n---\nbody\n");
    write_md(tmp.path(), "bad.md", "---\ntitle: [unclosed\n---\nbody\n");

    let disk = json(&tmp, &["summary"]);
    hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .arg("create-index")
        .assert()
        .success();
    let indexed = json(&tmp, &["summary", "--index"]);

    assert_eq!(
        disk["results"]["files"]["skipped"], 1,
        "the disk scan must see the unparsable file: {disk:#}"
    );
    assert_eq!(
        disk["results"]["files"]["skipped"], indexed["results"]["files"]["skipped"],
        "an indexed summary must agree: disk {disk:#}\nindex {indexed:#}"
    );
}

/// PREFIX-1: resolution answered from the in-memory file set must reach the
/// same verdicts as the filesystem probe it replaces, on and off the index.
#[test]
fn site_prefix_resolution_is_identical_on_disk_and_from_a_snapshot() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "web/api/index.md", "# API\n");
    write_md(tmp.path(), "web/css.md", "# CSS\n");
    write_md(
        tmp.path(),
        "src.md",
        "[a](/docs/web/api) [b](/docs/web/css) [c](/docs/web/nope)\n",
    );

    let disk = json(&tmp, &["--site-prefix", "docs", "find", "--fields", "links"]);
    hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(["--site-prefix", "docs"])
        .arg("create-index")
        .assert()
        .success();
    let indexed = json(
        &tmp,
        &["--site-prefix", "docs", "find", "--fields", "links", "--index"],
    );
    assert_eq!(
        disk["results"], indexed["results"],
        "disk and snapshot must resolve identically"
    );

    let paths: Vec<Option<&str>> = disk["results"][0]["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["path"].as_str())
        .collect();
    assert_eq!(
        paths,
        vec![Some("web/api/index.md"), Some("web/css.md"), None],
        "{disk:#}"
    );
}
