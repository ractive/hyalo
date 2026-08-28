//! Iteration 245 — deferral carry-overs from the iteration 244 review.
//!
//! - **UX-3 follow-up — dot-paths through sequences of maps**: iteration 244
//!   taught `--property 'a.b=v'` to traverse nested *mappings*; a frontmatter
//!   list of maps (`contacts: [{name, email}, …]`) still resolved to nothing.
//!   Traversal now also descends sequences: a numeric segment indexes one
//!   element (`contacts.0.email`), any other segment auto-descends into every
//!   element and collects the hits, so the established list semantics apply
//!   (`=`/`~=` match when any element matches, `!=` when none does).

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn hyalo_no_hints() -> Command {
    crate::common::hyalo_no_hints()
}

fn run(tmp: &TempDir, args: &[&str]) -> (std::process::Output, Value) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "`hyalo {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout not JSON: {e}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    (output, json)
}

/// Result count from the standard envelope.
fn total(json: &Value) -> u64 {
    json["total"]
        .as_u64()
        .or_else(|| json["results"].as_array().map(|a| a.len() as u64))
        .unwrap_or_else(|| panic!("no total/results in envelope: {json}"))
}

/// A vault with one file whose frontmatter holds a sequence of maps, and one
/// control file that must never match.
fn write_vault(tmp: &TempDir) {
    std::fs::write(
        tmp.path().join("team.md"),
        "---\n\
         title: Team\n\
         contacts:\n\
         \x20 - name: Ada\n\
         \x20   email: ada@example.com\n\
         \x20 - name: Grace\n\
         \x20   email: grace@example.com\n\
         ---\n\n# Team\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("solo.md"),
        "---\n\
         title: Solo\n\
         contacts:\n\
         \x20 - name: Alan\n\
         \x20   email: alan@example.com\n\
         ---\n\n# Solo\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("plain.md"),
        "---\ntitle: Plain\n---\n\n# Plain\n",
    )
    .unwrap();
}

/// `contacts.email=v` must auto-descend into every element of a sequence of
/// maps — on the disk-scan path and on the persisted-index path alike.
#[test]
fn find_property_dot_path_descends_sequence_of_maps() {
    let tmp = tempfile::tempdir().unwrap();
    write_vault(&tmp);

    // Second element of the two-element list.
    let (_, json) = run(
        &tmp,
        &["find", "--property", "contacts.email=grace@example.com"],
    );
    assert_eq!(
        total(&json),
        1,
        "auto-descent must match a non-first element: {json}"
    );
    assert_eq!(json["results"][0]["file"], "team.md", "{json}");

    // First element of a different file.
    let (_, json) = run(
        &tmp,
        &["find", "--property", "contacts.email=alan@example.com"],
    );
    assert_eq!(total(&json), 1, "{json}");
    assert_eq!(json["results"][0]["file"], "solo.md", "{json}");

    // A value that is in no element genuinely returns nothing.
    let (_, json) = run(
        &tmp,
        &["find", "--property", "contacts.email=nobody@example.com"],
    );
    assert_eq!(total(&json), 0, "{json}");

    // Same verdicts through the persisted index.
    run(&tmp, &["create-index"]);
    let (_, json) = run(
        &tmp,
        &[
            "find",
            "--property",
            "contacts.email=grace@example.com",
            "--index",
        ],
    );
    assert_eq!(
        total(&json),
        1,
        "index path must agree with the disk scan: {json}"
    );
}

/// A numeric segment pins one element: `contacts.0.email` is the first
/// contact only, and an out-of-range index matches nothing.
#[test]
fn find_property_dot_path_sequence_index_segment() {
    let tmp = tempfile::tempdir().unwrap();
    write_vault(&tmp);

    let (_, json) = run(
        &tmp,
        &["find", "--property", "contacts.0.email=ada@example.com"],
    );
    assert_eq!(
        total(&json),
        1,
        "index 0 must select the first element: {json}"
    );

    let (_, json) = run(
        &tmp,
        &["find", "--property", "contacts.1.email=ada@example.com"],
    );
    assert_eq!(
        total(&json),
        0,
        "index 1 must not match the first element's value: {json}"
    );

    let (_, json) = run(
        &tmp,
        &["find", "--property", "contacts.1.email=grace@example.com"],
    );
    assert_eq!(total(&json), 1, "{json}");

    let (_, json) = run(
        &tmp,
        &["find", "--property", "contacts.9.email=ada@example.com"],
    );
    assert_eq!(
        total(&json),
        0,
        "out-of-range index must match nothing: {json}"
    );
}

/// Existence, absence, inequality and regex all follow the established list
/// semantics once the sequence has been descended.
#[test]
fn find_property_dot_path_sequence_operators() {
    let tmp = tempfile::tempdir().unwrap();
    write_vault(&tmp);

    // Existence: both files with contacts, not `plain.md`.
    let (_, json) = run(&tmp, &["find", "--property", "contacts.email"]);
    assert_eq!(total(&json), 2, "{json}");

    // Absence: only the file with no contacts at all.
    let (_, json) = run(&tmp, &["find", "--property", "!contacts.email"]);
    assert_eq!(total(&json), 1, "{json}");
    assert_eq!(json["results"][0]["file"], "plain.md", "{json}");

    // A key no element carries is absent everywhere.
    let (_, json) = run(&tmp, &["find", "--property", "contacts.phone"]);
    assert_eq!(total(&json), 0, "{json}");

    // `!=` means "no element equals": team.md has Ada, solo.md does not.
    let (_, json) = run(
        &tmp,
        &["find", "--property", "contacts.email!=ada@example.com"],
    );
    assert_eq!(total(&json), 1, "{json}");
    assert_eq!(json["results"][0]["file"], "solo.md", "{json}");

    // Regex matches when any element matches.
    let (_, json) = run(&tmp, &["find", "--property", "contacts.name~=^Grac"]);
    assert_eq!(total(&json), 1, "{json}");
    assert_eq!(json["results"][0]["file"], "team.md", "{json}");
}

/// The same resolution feeds every filter consumer, not just `find` — a
/// mutating command's `--where-property` sees sequence dot-paths too.
#[test]
fn where_property_dot_path_descends_sequence_of_maps() {
    let tmp = tempfile::tempdir().unwrap();
    write_vault(&tmp);

    let (_, json) = run(
        &tmp,
        &[
            "set",
            "--property",
            "status=reviewed",
            "--where-property",
            "contacts.email=grace@example.com",
            "--glob",
            "**/*.md",
        ],
    );
    assert_eq!(
        json["results"]["modified"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(1),
        "--where-property must select exactly the file with that contact: {json}"
    );
    assert_eq!(json["results"]["modified"][0], "team.md", "{json}");

    let (_, json) = run(&tmp, &["find", "--property", "status=reviewed"]);
    assert_eq!(total(&json), 1, "{json}");
    assert_eq!(json["results"][0]["file"], "team.md", "{json}");
}
