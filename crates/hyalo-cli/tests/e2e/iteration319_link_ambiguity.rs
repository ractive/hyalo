//! Iteration 319 (DEC-374): link ambiguity as a first-class field.
//!
//! A `[[stem]]` that names two files (or an alias two notes claim) used to look
//! exactly like one that names none: `path: null` and nothing else. Now
//! `find --fields links` says `ambiguous: true` and lists the sorted,
//! vault-relative `candidates`, and the three other places that reach the same
//! verdict — `mv`'s `skipped_ambiguous`, HYALO006's "matches N candidates"
//! message and `links fix`'s `ambiguous_links` — read the same computation.

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::common::hyalo_no_hints;

fn hyalo(tmp: &TempDir) -> Command {
    let mut cmd = hyalo_no_hints();
    cmd.current_dir(tmp.path());
    cmd
}

fn write(tmp: &TempDir, rel: &str, body: &str) {
    let path = tmp.path().join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

fn run(tmp: &TempDir, args: &[&str]) -> Value {
    let output = hyalo(tmp)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or(Value::Null)
}

/// Two notes sharing a stem, two notes claiming one alias, a unique alias, a
/// unique note, and one source file linking to all of them (and to nothing).
fn vault(config: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    write(&tmp, ".hyalo.toml", &format!("dir = \".\"\n{config}"));
    write(&tmp, "a/note.md", "# A\n");
    write(&tmp, "b/note.md", "# B\n");
    write(&tmp, "solo.md", "# Solo\n");
    write(&tmp, "c/one.md", "---\naliases: [Twin, Lone]\n---\n# One\n");
    write(&tmp, "c/two.md", "---\naliases: [Twin]\n---\n# Two\n");
    write(
        &tmp,
        "src.md",
        "---\nrelated: [\"[[note]]\"]\n---\n# Src\n\n\
         [[note]] [[nowhere]] [[solo]] [[Twin]] [[Lone]] [[note#Heading]]\n\
         [md](solo.md) [web](https://example.com)\n",
    );
    tmp
}

fn links(tmp: &TempDir, extra: &[&str]) -> Vec<Value> {
    let mut args = vec!["find", "--file", "src.md", "--fields", "links"];
    args.extend_from_slice(extra);
    run(tmp, &args)["results"][0]["links"]
        .as_array()
        .unwrap()
        .clone()
}

fn by_target<'a>(links: &'a [Value], target: &str) -> Vec<&'a Value> {
    links.iter().filter(|l| l["target"] == target).collect()
}

fn sorted_candidates() -> Value {
    json!(["a/note.md", "b/note.md"])
}

/// HYALO006's messages for `src.md`.
fn hyalo006_messages(tmp: &TempDir) -> Vec<String> {
    let json = run(tmp, &["lint", "--rule", "HYALO006", "--max-per-rule", "0"]);
    json["results"]["files"][0]["rule_groups"][0]["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["message"].as_str().unwrap().to_owned())
        .collect()
}

/// The candidates HYALO006 names for `target`, parsed back out of its message.
fn hyalo006_candidates(messages: &[String], target: &str) -> Vec<String> {
    let prefix = format!("ambiguous wikilink: `{target}` matches ");
    let message = messages
        .iter()
        .find(|m| m.starts_with(&prefix))
        .unwrap_or_else(|| panic!("no ambiguous message for {target}: {messages:?}"));
    message
        .split_once(" candidates: ")
        .unwrap()
        .1
        .split(", ")
        .map(str::to_owned)
        .collect()
}

#[test]
fn find_reports_ambiguous_and_sorted_candidates() {
    let tmp = vault("");
    let links = links(&tmp, &[]);

    let notes = by_target(&links, "note");
    assert_eq!(notes.len(), 3, "body, fragment and frontmatter: {links:?}");
    for link in &notes {
        assert_eq!(link["path"], Value::Null, "{link}");
        assert_eq!(link["ambiguous"], json!(true), "{link}");
        assert_eq!(link["candidates"], sorted_candidates(), "{link}");
    }
    // An alias two notes claim is ambiguous too (DEC-308).
    let twin = by_target(&links, "Twin")[0];
    assert_eq!(twin["ambiguous"], json!(true), "{twin}");
    assert_eq!(
        twin["candidates"],
        json!(["c/one.md", "c/two.md"]),
        "{twin}"
    );
}

#[test]
fn every_other_link_keeps_its_shape() {
    let tmp = vault("");
    let links = links(&tmp, &[]);
    for target in ["nowhere", "solo", "Lone", "solo.md", "https://example.com"] {
        for link in by_target(&links, target) {
            let object = link.as_object().unwrap();
            assert!(
                !object.contains_key("ambiguous") && !object.contains_key("candidates"),
                "{target} must carry neither key: {link}"
            );
        }
    }
    let nowhere = by_target(&links, "nowhere")[0];
    assert_eq!(nowhere["path"], Value::Null);
}

#[test]
fn a_root_level_note_wins_a_bare_stem_so_it_is_not_ambiguous() {
    // Reads keep Obsidian's shortest-path rule: `[[note]]` resolves to the
    // vault-root note, so nothing is unresolved. (`mv` is deliberately
    // stricter — it still refuses to guess — and is left unchanged.)
    let tmp = vault("");
    write(&tmp, "note.md", "# Root\n");
    let links = links(&tmp, &[]);
    for link in by_target(&links, "note") {
        assert_eq!(link["path"], json!("note.md"), "{link}");
        assert!(link.get("ambiguous").is_none(), "{link}");
        assert!(link.get("candidates").is_none(), "{link}");
    }
}

#[test]
fn find_mv_and_hyalo006_report_the_same_candidates() {
    let tmp = vault("");

    // 1. The field.
    let links = links(&tmp, &[]);
    let field: Vec<String> = by_target(&links, "note")[0]["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(field, ["a/note.md", "b/note.md"]);

    // 2. HYALO006's message.
    let messages = hyalo006_messages(&tmp);
    assert_eq!(
        hyalo006_candidates(&messages, "note"),
        field,
        "{messages:?}"
    );

    // 3. `mv`'s skipped_ambiguous, whichever candidate is moved.
    for moved in ["a/note.md", "b/note.md"] {
        let json = run(&tmp, &["mv", moved, "--to", "moved/other.md", "--dry-run"]);
        let skipped = json["results"]["skipped_ambiguous"].as_array().unwrap();
        let notes: Vec<&Value> = skipped.iter().filter(|s| s["target"] == "note").collect();
        assert_eq!(
            notes.len(),
            3,
            "two body links and the frontmatter one: {json}"
        );
        for entry in notes {
            assert_eq!(entry["candidates"], json!(field), "moving {moved}: {entry}");
        }
        assert!(
            skipped
                .iter()
                .any(|s| s["target"] == "note" && s["property"] == "related"),
            "the frontmatter link keeps its `property`: {json}"
        );
    }

    // 4. `links fix`'s ambiguous_links bucket.
    let fix = run(&tmp, &["links", "fix", "--dry-run"]);
    let ambiguous = fix["results"]["ambiguous_links"].as_array().unwrap();
    for entry in ambiguous.iter().filter(|a| a["target"] == "note") {
        assert_eq!(entry["candidates"], json!(field), "{entry}");
    }
    assert!(ambiguous.iter().any(|a| a["target"] == "note"), "{fix}");
}

#[test]
fn an_alias_collision_is_reported_by_find_hyalo006_and_links_fix_but_not_mv() {
    let tmp = vault("");
    let twin: Vec<String> = vec!["c/one.md".into(), "c/two.md".into()];

    let links = links(&tmp, &[]);
    assert_eq!(by_target(&links, "Twin")[0]["candidates"], json!(twin));
    assert_eq!(hyalo006_candidates(&hyalo006_messages(&tmp), "Twin"), twin);
    let fix = run(&tmp, &["links", "fix", "--dry-run"]);
    let entry = fix["results"]["ambiguous_links"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["target"] == "Twin")
        .unwrap()
        .clone();
    assert_eq!(entry["candidates"], json!(twin));

    // `mv` never resolves a link through an alias (DEC-296), so an alias two
    // notes claim is not something it has to refuse over.
    let json = run(&tmp, &["mv", "c/one.md", "--to", "c/first.md", "--dry-run"]);
    let skipped = json["results"]["skipped_ambiguous"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        skipped.iter().all(|s| s["target"] != "Twin"),
        "mv does not guard alias collisions: {json}"
    );
}

#[test]
fn the_aliases_setting_never_changes_stem_ambiguity() {
    for config in [
        "",
        "[links]\naliases = false\n",
        "[links]\naliases = true\n",
    ] {
        let tmp = vault(config);
        // An alias on a third note must not tie-break the ambiguous stem.
        write(&tmp, "solo.md", "---\naliases: [note]\n---\n# Solo\n");
        let links = links(&tmp, &[]);
        for link in by_target(&links, "note") {
            assert_eq!(link["ambiguous"], json!(true), "[{config}] {link}");
            assert_eq!(link["candidates"], sorted_candidates(), "[{config}] {link}");
            assert_eq!(link["path"], Value::Null, "[{config}] {link}");
        }
        let twin = by_target(&links, "Twin")[0];
        assert_eq!(twin["ambiguous"], json!(true), "[{config}] {twin}");

        // A unique alias is not ambiguous in either mode: broken by default
        // (DEC-308), resolved when the vault opted in.
        let lone = by_target(&links, "Lone")[0];
        assert!(lone.get("ambiguous").is_none(), "[{config}] {lone}");
        let resolved = config.contains("true");
        assert_eq!(
            lone["path"] != Value::Null,
            resolved,
            "[{config}] unique alias: {lone}"
        );
    }
}

#[test]
fn an_index_scan_reports_the_same_candidates_as_a_disk_scan() {
    let tmp = vault("");
    let disk = links(&tmp, &[]);

    let output = hyalo(&tmp).arg("create-index").output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let indexed = links(&tmp, &["--index"]);

    assert_eq!(indexed, disk, "--index and a disk scan must agree");
    let note = by_target(&indexed, "note")[0];
    assert_eq!(note["candidates"], sorted_candidates());
    let twin = by_target(&indexed, "Twin")[0];
    assert_eq!(twin["candidates"], json!(["c/one.md", "c/two.md"]));
}

#[test]
fn text_mode_prints_the_candidates_in_place_of_unresolved() {
    let tmp = vault("");
    let output = hyalo(&tmp)
        .args(["find", "--file", "src.md", "--fields", "links"])
        .args(["--format", "text"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    let line_of = |needle: &str| {
        text.lines()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no line for {needle}: {text}"))
            .to_owned()
    };
    let note = line_of("\"note\"");
    assert!(note.contains("(ambiguous: a/note.md, b/note.md)"), "{note}");
    assert!(!note.contains("(unresolved)"), "{note}");
    assert!(
        line_of("\"Twin\"").contains("(ambiguous: c/one.md, c/two.md)"),
        "{text}"
    );
    assert!(
        line_of("\"nowhere\"").contains("(unresolved)"),
        "a genuinely missing target keeps its wording: {text}"
    );
}

#[test]
fn an_ambiguous_link_is_still_counted_as_broken_by_every_command() {
    // DEC-351: `summary.links.broken`, `find --broken-links` and HYALO006 agree.
    // `path` stays null for an ambiguous link, so none of them stops counting it.
    let tmp = vault("");

    let unresolved = links(&tmp, &[])
        .iter()
        .filter(|l| l["path"] == Value::Null && l["kind"] != "external")
        .count();

    let summary = run(&tmp, &["summary"]);
    let broken = summary["results"]["links"]["broken"].as_u64().unwrap();
    assert_eq!(broken, u64::try_from(unresolved).unwrap(), "{summary}");
    // note x3 (body, fragment, frontmatter) + nowhere + Twin + Lone (aliases off).
    assert_eq!(broken, 6, "{summary}");

    let found = run(&tmp, &["find", "--broken-links", "--fields", "links"]);
    assert_eq!(found["results"].as_array().unwrap().len(), 1, "{found}");

    let messages = hyalo006_messages(&tmp);
    assert_eq!(messages.len(), 6, "{messages:?}");
    assert_eq!(
        messages
            .iter()
            .filter(|m| m.starts_with("ambiguous "))
            .count(),
        4,
        "{messages:?}"
    );
}
