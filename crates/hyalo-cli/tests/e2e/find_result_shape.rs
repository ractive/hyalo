//! `find`/`read` result shape (iteration 252).
//!
//! The default `find` payload was `properties, tags, sections, links` for
//! every item, which made a 20-file listing roughly nine times larger than
//! the metadata most callers asked for, and nothing in a result said how big
//! a file was before you read it. This suite pins the replacement contract:
//!
//! - the default field set is `file, modified, size, lines, title,
//!   properties, tags`, and nothing else;
//! - every filter or sort that *implies* a field still returns it, with no
//!   `--fields` needed;
//! - `size`/`lines` are present on `find` and `read`, agree with the bytes on
//!   disk (CRLF and multi-byte UTF-8 included), and are identical between a
//!   disk scan and a `--index` snapshot;
//! - `--fields all` reproduces the pre-252 shape.

use super::common::{hyalo_no_hints, write_md};
use std::path::Path;
use tempfile::TempDir;

/// Run `hyalo find …` (JSON, no hints) and return the `results` array.
fn find_results(dir: &Path, args: &[&str]) -> Vec<serde_json::Value> {
    let output = hyalo_no_hints()
        .current_dir(dir)
        .arg("find")
        .args(args)
        .args(["--format", "json"])
        .output()
        .expect("hyalo should run");
    assert!(
        output.status.success(),
        "find {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("find should print an envelope");
    envelope["results"]
        .as_array()
        .expect("results should be an array")
        .clone()
}

/// The keys of the first result item, sorted (serde_json orders them itself).
fn keys(item: &serde_json::Value) -> Vec<String> {
    item.as_object()
        .expect("result item should be an object")
        .keys()
        .cloned()
        .collect()
}

/// A vault of `n` iteration-shaped notes: frontmatter, headings, tasks and a
/// wikilink each — every field the old default shape used to carry.
fn vault(n: usize) -> TempDir {
    let tmp = TempDir::new().unwrap();
    for i in 0..n {
        write_md(
            tmp.path(),
            &format!("note-{i:02}.md"),
            &format!(
                "---\ntitle: Note {i}\ntype: note\nstatus: planned\ntags:\n  - note\n  - batch\n---\n\n\
                 # Note {i}\n\nSee [[note-00]] for context.\n\n## Tasks\n\n- [ ] first task {i}\n- [x] second task {i}\n\n\
                 ## Notes\n\nSome prose about note {i} that makes the body worth measuring.\n"
            ),
        );
    }
    tmp
}

// ---------------------------------------------------------------------------
// Default field set
// ---------------------------------------------------------------------------

#[test]
fn default_shape_is_the_compact_field_set() {
    let tmp = vault(3);
    let results = find_results(tmp.path(), &["--tag", "note"]);
    assert_eq!(results.len(), 3);
    let mut expected = vec![
        "file",
        "lines",
        "modified",
        "properties",
        "size",
        "tags",
        "title",
    ];
    expected.sort_unstable();
    assert_eq!(
        keys(&results[0]),
        expected,
        "default find item must carry exactly the compact field set"
    );
}

#[test]
fn default_shape_stays_under_the_byte_budget() {
    // The regression this iteration exists to prevent: a 20-file listing must
    // stay small enough for an agent to read without a projection. The old
    // default was ~9x this on the same kind of vault.
    let tmp = vault(20);
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--tag", "note", "--limit", "20", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let bytes = output.stdout.len();
    assert!(
        bytes <= 12 * 1024,
        "20-file default listing must stay within 12 KB, was {bytes} bytes"
    );

    let all = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "find", "--tag", "note", "--limit", "20", "--fields", "all", "--format", "json",
        ])
        .output()
        .unwrap();
    assert!(
        all.stdout.len() > bytes * 2,
        "--fields all should be markedly larger ({} vs {bytes} bytes)",
        all.stdout.len()
    );
}

#[test]
fn fields_all_restores_the_full_shape() {
    let tmp = vault(2);
    let results = find_results(tmp.path(), &["--tag", "note", "--fields", "all"]);
    for key in [
        "file",
        "modified",
        "size",
        "lines",
        "title",
        "properties",
        "properties_typed",
        "tags",
        "sections",
        "tasks",
        "links",
        "backlinks",
    ] {
        assert!(
            results[0].get(key).is_some(),
            "--fields all must include {key}: {:?}",
            keys(&results[0])
        );
    }
}

#[test]
fn promoted_title_is_not_repeated_inside_properties() {
    let tmp = vault(1);
    let results = find_results(tmp.path(), &["--tag", "note"]);
    assert_eq!(results[0]["title"], "Note 0");
    assert!(
        results[0]["properties"].get("title").is_none(),
        "title is promoted to its own field: {:?}",
        results[0]["properties"]
    );
    // Asking for properties *without* the title field keeps the property, so
    // no request can lose the value entirely.
    let narrowed = find_results(tmp.path(), &["--tag", "note", "--fields", "properties"]);
    assert_eq!(narrowed[0]["properties"]["title"], "Note 0");
}

#[test]
fn sort_by_title_property_still_orders_by_the_frontmatter_value() {
    // The de-duplication happens after sorting, so `--sort property:title`
    // compares exactly what it compared before.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: Zulu\ntype: note\n---\n\n# A\n");
    write_md(tmp.path(), "b.md", "---\ntitle: Alpha\ntype: note\n---\n\n# B\n");
    let results = find_results(tmp.path(), &["--sort", "property:title"]);
    let files: Vec<&str> = results
        .iter()
        .map(|r| r["file"].as_str().unwrap())
        .collect();
    assert_eq!(files, vec!["b.md", "a.md"], "Alpha sorts before Zulu");
}

// ---------------------------------------------------------------------------
// Auto-includes: every filter that implies a field still returns it
// ---------------------------------------------------------------------------

#[test]
fn auto_include_matrix() {
    let tmp = vault(2);
    // (extra args, field the query implies)
    let cases: [(&[&str], &str); 5] = [
        (&["--section", "Tasks"], "sections"),
        (&["--task", "todo"], "tasks"),
        (&["--sort", "links_count"], "links"),
        (&["--sort", "backlinks_count"], "backlinks"),
        (&["--broken-links"], "links"),
    ];
    for (extra, field) in cases {
        let mut args: Vec<&str> = vec!["--glob", "*.md"];
        args.extend_from_slice(extra);
        let results = find_results(tmp.path(), &args);
        if field == "links" && extra == ["--broken-links"] {
            // This vault has no broken links, so the filter matches nothing —
            // the auto-include is asserted by the two sort cases instead.
            continue;
        }
        assert!(
            !results.is_empty(),
            "{extra:?} should match something in the fixture vault"
        );
        assert!(
            results[0].get(field).is_some(),
            "{extra:?} implies {field}, which must be present without --fields: {:?}",
            keys(&results[0])
        );
    }
}

#[test]
fn broken_links_filter_auto_includes_links() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\n---\n\n# A\n\nSee [[nowhere]].\n",
    );
    let results = find_results(tmp.path(), &["--broken-links"]);
    assert_eq!(results.len(), 1);
    assert!(
        results[0]["links"].as_array().is_some_and(|l| !l.is_empty()),
        "--broken-links must return the links it filtered on"
    );
}

// ---------------------------------------------------------------------------
// size / lines
// ---------------------------------------------------------------------------

/// Byte length and line count of a file, computed independently of hyalo.
fn on_disk(dir: &Path, name: &str) -> (u64, usize) {
    let bytes = std::fs::read(dir.join(name)).unwrap();
    let newlines = bytes.iter().filter(|b| **b == b'\n').count();
    let lines = match bytes.last() {
        None => 0,
        Some(b'\n') => newlines,
        Some(_) => newlines + 1,
    };
    (bytes.len() as u64, lines)
}

#[test]
fn size_and_lines_match_disk_for_crlf_and_utf8() {
    let tmp = TempDir::new().unwrap();
    // Written through std::fs directly: `write_md` is LF-only, and CRLF is
    // exactly what this test is about.
    std::fs::write(
        tmp.path().join("crlf.md"),
        "---\r\ntitle: CRLF\r\n---\r\n\r\n# CRLF\r\n\r\nTwo lines here.\r\n",
    )
    .unwrap();
    // Multi-byte UTF-8: `size` counts bytes, `lines` counts lines — a naive
    // char count would disagree with both.
    std::fs::write(
        tmp.path().join("utf8.md"),
        "---\ntitle: Üñïçôdé\n---\n\n# Üñïçôdé\n\n日本語のテキスト。\n",
    )
    .unwrap();
    // No trailing newline: the last unterminated line still counts.
    std::fs::write(tmp.path().join("noeol.md"), "---\ntitle: No EOL\n---\n\n# End").unwrap();

    let results = find_results(tmp.path(), &[]);
    for item in &results {
        let file = item["file"].as_str().unwrap();
        let (size, lines) = on_disk(tmp.path(), file);
        assert_eq!(item["size"].as_u64(), Some(size), "size mismatch for {file}");
        assert_eq!(
            item["lines"].as_u64(),
            Some(lines as u64),
            "lines mismatch for {file}"
        );
    }
    assert_eq!(results.len(), 3);
}

#[test]
fn size_and_lines_are_identical_with_and_without_index() {
    let tmp = vault(4);
    std::fs::write(
        tmp.path().join("crlf.md"),
        "---\r\ntitle: CRLF\r\n---\r\n\r\n# CRLF\r\n",
    )
    .unwrap();
    let disk = find_results(tmp.path(), &[]);

    let created = hyalo_no_hints()
        .current_dir(tmp.path())
        .arg("create-index")
        .output()
        .unwrap();
    assert!(created.status.success());
    let indexed = find_results(tmp.path(), &["--index"]);

    assert_eq!(disk.len(), indexed.len());
    for (d, i) in disk.iter().zip(indexed.iter()) {
        assert_eq!(d["file"], i["file"]);
        assert_eq!(d["size"], i["size"], "size parity for {}", d["file"]);
        assert_eq!(d["lines"], i["lines"], "lines parity for {}", d["file"]);
    }
}

#[test]
fn index_size_and_lines_survive_a_mutation() {
    // The journal refreshes body-derived fields on every write path; `size`
    // and `lines` are body-derived, so a `set` under --index must not leave
    // them describing the pre-mutation file.
    let tmp = vault(2);
    assert!(
        hyalo_no_hints()
            .current_dir(tmp.path())
            .arg("create-index")
            .output()
            .unwrap()
            .status
            .success()
    );
    let before = find_results(tmp.path(), &["--index", "--file", "note-00.md"]);
    let set = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "set",
            "note-00.md",
            "--property",
            "status=in-progress-with-a-much-longer-value",
            "--index",
        ])
        .output()
        .unwrap();
    assert!(
        set.status.success(),
        "set failed: {}",
        String::from_utf8_lossy(&set.stderr)
    );
    let after = find_results(tmp.path(), &["--index", "--file", "note-00.md"]);
    let (size, lines) = on_disk(tmp.path(), "note-00.md");
    assert_ne!(before[0]["size"], after[0]["size"], "the file grew");
    assert_eq!(after[0]["size"].as_u64(), Some(size));
    assert_eq!(after[0]["lines"].as_u64(), Some(lines as u64));
}

#[test]
fn read_reports_whole_file_size_and_lines() {
    let tmp = vault(1);
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["read", "note-00.md", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let (size, lines) = on_disk(tmp.path(), "note-00.md");
    assert_eq!(envelope["results"]["size"].as_u64(), Some(size));
    assert_eq!(envelope["results"]["lines"].as_u64(), Some(lines as u64));

    // A narrowed read still describes the whole file, so the numbers can be
    // compared against the `find` result that led here.
    let section = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["read", "note-00.md", "--section", "Tasks", "--format", "json"])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&section.stdout).unwrap();
    assert_eq!(envelope["results"]["size"].as_u64(), Some(size));
    assert_eq!(envelope["results"]["lines"].as_u64(), Some(lines as u64));
}

// ---------------------------------------------------------------------------
// Hints
// ---------------------------------------------------------------------------

#[test]
fn read_of_a_large_file_hints_at_reading_less() {
    let tmp = TempDir::new().unwrap();
    let body: String = (0..600)
        .map(|i| format!("Line {i} of a document that is comfortably over the hint threshold.\n"))
        .collect();
    write_md(tmp.path(), "big.md", &format!("---\ntitle: Big\n---\n\n# Big\n\n{body}"));

    let output = super::common::hyalo()
        .current_dir(tmp.path())
        .args(["read", "big.md", "--format", "json", "--hints"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = envelope["hints"].as_array().expect("hints array");
    assert!(
        hints
            .iter()
            .any(|h| h["cmd"].as_str().is_some_and(|c| c.contains("--lines 1:80"))),
        "a large read should offer a line-range read: {hints:?}"
    );

    // A read that already narrowed does not get told to narrow.
    let narrowed = super::common::hyalo()
        .current_dir(tmp.path())
        .args(["read", "big.md", "--lines", "1:20", "--format", "json", "--hints"])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&narrowed.stdout).unwrap();
    let hints = envelope["hints"].as_array().expect("hints array");
    assert!(
        !hints
            .iter()
            .any(|h| h["cmd"].as_str().is_some_and(|c| c.contains("--lines 1:80"))),
        "an already-narrowed read must not repeat the suggestion: {hints:?}"
    );
}

#[test]
fn small_result_set_offers_fields_all() {
    let tmp = vault(2);
    let output = super::common::hyalo()
        .current_dir(tmp.path())
        .args(["find", "--tag", "note", "--format", "json", "--hints"])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = envelope["hints"].as_array().expect("hints array");
    assert!(
        hints
            .iter()
            .any(|h| h["cmd"].as_str().is_some_and(|c| c.contains("--fields all"))),
        "a small listing should say how to get the omitted fields: {hints:?}"
    );
}

// ---------------------------------------------------------------------------
// Text mode
// ---------------------------------------------------------------------------

#[test]
fn text_mode_names_the_fields_and_shows_the_size() {
    let tmp = vault(1);
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--tag", "note", "--format", "text"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (size, lines) = on_disk(tmp.path(), "note-00.md");
    assert!(
        stdout.contains(&format!("{size} B, {lines} lines")),
        "the header line should carry size and lines: {stdout}"
    );
    assert!(
        stdout.contains("fields: file, modified, size, lines, title, properties, tags"),
        "the summary line should name the included fields: {stdout}"
    );
    assert!(
        stdout.contains("--fields all adds sections, tasks, links, backlinks"),
        "the summary line should name what is missing: {stdout}"
    );
}
