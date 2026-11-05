//! Iteration 322: everything hyalo writes as a link resolves the way it reads
//! one (DEC-384) — `mv`, `links fix` and the counters share the reader's split.

use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::path::Path;
use std::process::Output;
use tempfile::TempDir;

fn run(dir: &Path, args: &[&str]) -> Output {
    hyalo_no_hints()
        .current_dir(dir)
        .arg("--dir")
        .arg(".")
        .args(args)
        .output()
        .unwrap()
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(&out.stdout)))
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}

fn broken_total(dir: &Path) -> u64 {
    let out = json(&run(dir, &["summary", "--format", "json"]));
    out["results"]["links"]["broken"].as_u64().unwrap()
}

fn alias_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "sub/Target.md",
        "---\naliases: [AKA]\n---\n# Head\n## Second\n",
    );
    write_md(tmp.path(), "other/Target.md", "# Twin\n");
    tmp
}

// A-1: a frontmatter alias fix writes what the body gets, LF and CRLF alike.
#[test]
fn alias_fix_in_frontmatter_keeps_the_alias_and_the_path() {
    for eol in ["\n", "\r\n"] {
        let tmp = alias_vault();
        let src = format!("---{eol}a: \"[[AKA]]\"{eol}---{eol}body [[AKA]]{eol}");
        write_md(tmp.path(), "s.md", &src);
        let out = json(&run(tmp.path(), &["links", "fix", "--apply"]));
        let plans = out["results"]["alias_fix_plans"].as_array().unwrap();
        assert!(
            plans
                .iter()
                .all(|p| p["emitted_target"] == "sub/Target|AKA"),
            "{plans:?}"
        );
        let after = read(tmp.path(), "s.md");
        assert!(after.contains("a: \"[[sub/Target|AKA]]\""), "{after:?}");
        assert!(after.contains("body [[sub/Target|AKA]]"), "{after:?}");
        assert_eq!(broken_total(tmp.path()), 0);
    }
}

// A-2: the fragment stays on the target, the alias follows it.
#[test]
fn alias_fix_keeps_the_fragment_on_the_target() {
    let tmp = alias_vault();
    write_md(
        tmp.path(),
        "s.md",
        "b [[AKA#Second]] [[AKA#Nope]] ![[AKA#Second]] [[AKA#Second|shown]]\n",
    );
    run(tmp.path(), &["links", "fix", "--apply"]);
    assert_eq!(
        read(tmp.path(), "s.md"),
        "b [[sub/Target#Second|AKA]] [[sub/Target#Nope|AKA]] ![[sub/Target#Second]] \
         [[sub/Target#Second|shown]]\n"
    );
}

// A-4: every link `backlinks` counts, `mv` rewrites.
#[test]
fn mv_rewrites_query_and_dot_slash_spellings() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "# H\n");
    write_md(
        tmp.path(),
        "s.md",
        "---\nr: \"[[./note]]\"\nq: \"[[note?x=1#H|a]]\"\n---\n[[note?]] [[note?x=1]] [[my%20note?]] [[note]]\n",
    );
    write_md(tmp.path(), "my note.md", "# other\n");
    let backlinks = json(&run(tmp.path(), &["backlinks", "note.md"]));
    assert_eq!(backlinks["total"], 5);
    let moved = json(&run(tmp.path(), &["mv", "note.md", "--to", "moved.md"]));
    assert_eq!(moved["results"]["total_links_updated"], 5);
    let after = read(tmp.path(), "s.md");
    assert!(after.contains("r: \"[[moved]]\""), "{after}");
    assert!(after.contains("q: \"[[moved?x=1#H|a]]\""), "{after}");
    assert!(after.contains("[[moved?]] [[moved?x=1]]"), "{after}");
    // The link to `my note` is another note's: untouched.
    assert!(after.contains("[[my%20note?]]"), "{after}");
    assert_eq!(broken_total(tmp.path()), 0);
}

// A-7: the counters agree with the reader on short-form and relocated links.
#[test]
fn broken_counters_agree_on_short_forms_and_relocations() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a/note.md", "#a\n");
    write_md(tmp.path(), "a/my note.md", "#a\n");
    write_md(
        tmp.path(),
        "s/src.md",
        "[[note?]] [[my%20note]] [[n%6Fte]] [[note?x=1]]\n",
    );
    let fix = json(&run(tmp.path(), &["links", "fix"]));
    assert_eq!(fix["results"]["broken"], 0, "{fix}");
    assert_eq!(broken_total(tmp.path()), 0);

    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a/note.md", "#a\n");
    write_md(tmp.path(), "s/src.md", "[m](../note.md) [[./note]]\n");
    let found = json(&run(
        tmp.path(),
        &[
            "find",
            "--broken-links",
            "--jq",
            "[.results[].links[] | select(.path == null)] | length",
        ],
    ));
    assert_eq!(found, 2);
    assert_eq!(broken_total(tmp.path()), 2);
}

// C-12: `[[../x]]` names an in-vault note, so it is read, counted and rewritten.
#[test]
fn relative_parent_wikilinks_are_in_vault_links() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "t.md", "#t\n");
    write_md(tmp.path(), "other/o.md", "#o\n");
    write_md(
        tmp.path(),
        "sub/src.md",
        "[[../t]] [[../other/o]] ![[../t.md]]\n",
    );
    let summary = json(&run(tmp.path(), &["summary", "--format", "json"]));
    assert_eq!(summary["results"]["links"]["broken"], 0);
    assert!(summary["results"]["links"].get("out_of_vault").is_none());
    let backlinks = json(&run(tmp.path(), &["backlinks", "t.md"]));
    assert_eq!(backlinks["total"], 2);
    let moved = json(&run(tmp.path(), &["mv", "t.md", "--to", "t9.md"]));
    assert_eq!(moved["results"]["total_links_updated"], 2);
    assert_eq!(
        read(tmp.path(), "sub/src.md"),
        "[[t9]] [[../other/o]] ![[t9.md]]\n"
    );
    assert_eq!(broken_total(tmp.path()), 0);
}

// C-1: a partially qualified wikilink resolves by path suffix.
#[test]
fn partial_path_wikilinks_resolve_and_move_with_their_note() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a/x/note.md", "#\n");
    write_md(tmp.path(), "b/y/note.md", "#\n");
    write_md(tmp.path(), "s.md", "[[x/note]] [[y/note]] [[z/note]]\n");
    let found = json(&run(
        tmp.path(),
        &[
            "find",
            "--fields",
            "links",
            "--file",
            "s.md",
            "--jq",
            ".results[0].links | map(.path)",
        ],
    ));
    assert_eq!(
        found,
        serde_json::json!(["a/x/note.md", "b/y/note.md", null])
    );
    run(tmp.path(), &["mv", "a/x/note.md", "--to", "a/x/n2.md"]);
    assert_eq!(
        read(tmp.path(), "s.md"),
        "[[a/x/n2]] [[y/note]] [[z/note]]\n"
    );
}

// C-1: a suffix two notes share is ambiguous, not resolved.
#[test]
fn shared_path_suffix_is_ambiguous() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a/x/note.md", "#\n");
    write_md(tmp.path(), "b/x/note.md", "#\n");
    write_md(tmp.path(), "s.md", "[[x/note]]\n");
    assert_eq!(broken_total(tmp.path()), 1);
}

// B-16: the suggestion for a markdown link is a pasteable slug.
#[test]
fn markdown_fragment_suggestion_is_a_slug() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "t.md", "# Index pages\n");
    write_md(
        tmp.path(),
        "s.md",
        "[x](t.md#index-page) [[t#Index page]]\n",
    );
    let found = json(&run(
        tmp.path(),
        &[
            "find",
            "--broken-links",
            "--jq",
            ".results[0].links | map(.suggested_fragment)",
        ],
    ));
    assert_eq!(found, serde_json::json!(["index-pages", "Index pages"]));
}

// B-8: the text summary names the fixes plain --apply writes.
#[test]
fn links_fix_text_names_the_plain_apply_fixes() {
    let tmp = alias_vault();
    write_md(tmp.path(), "s.md", "[[AKA]]\n");
    let out = run(tmp.path(), &["links", "fix", "--format", "text"]);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        text.contains("Also written by plain --apply: 1 alias fixes"),
        "{text}"
    );
}

// R2: a frontmatter link spelling a moved directory index keeps naming the
// directory, even with another `index.md` in the vault.
#[test]
fn frontmatter_directory_links_follow_a_moved_index() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "index.md", "#\n");
    write_md(tmp.path(), "sub/index.md", "#\n");
    write_md(
        tmp.path(),
        "s.md",
        "---\nr: \"[[sub]]\"\nq: \"[[sub?x=1#H|l]]\"\n---\n[[sub]]\n",
    );
    let moved = json(&run(
        tmp.path(),
        &["mv", "sub/index.md", "--to", "other/index.md"],
    ));
    assert_eq!(moved["results"]["total_links_updated"], 3, "{moved}");
    let after = read(tmp.path(), "s.md");
    assert!(after.contains("r: \"[[other]]\""), "{after}");
    assert!(after.contains("q: \"[[other?x=1#H|l]]\""), "{after}");
    assert_eq!(broken_total(tmp.path()), 0);
}

// R3: `[links] case_insensitive = "false"` means exact bytes for a partial path.
#[test]
fn partial_paths_honour_exact_case_mode() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links]\ncase_insensitive = \"false\"\n",
    )
    .unwrap();
    write_md(tmp.path(), "a/x/note.md", "#\n");
    write_md(tmp.path(), "s.md", "[[x/note]] [[X/Note]] [[A/X/NOTE]]\n");
    // The reader (not `links fix`'s file-system probe, which a case-folding
    // volume answers differently) is what must keep exact bytes.
    let found = json(&run(
        tmp.path(),
        &[
            "find",
            "--fields",
            "links",
            "--file",
            "s.md",
            "--jq",
            ".results[0].links | map(.path)",
        ],
    ));
    assert_eq!(found, serde_json::json!(["a/x/note.md", null, null]));
}

// R4: a partial path two notes share is listed by `mv`, not silently re-aimed.
#[test]
fn mv_lists_a_shared_partial_path_and_backlinks_ignores_it() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a/x/note.md", "#\n");
    write_md(tmp.path(), "b/x/note.md", "#\n");
    write_md(tmp.path(), "s.md", "[[x/note]]\n");
    assert_eq!(
        json(&run(tmp.path(), &["backlinks", "a/x/note.md"]))["total"],
        0
    );
    let moved = json(&run(
        tmp.path(),
        &["mv", "a/x/note.md", "--to", "a/y/moved.md"],
    ));
    assert_eq!(moved["results"]["total_links_updated"], 0);
    assert_eq!(
        moved["results"]["skipped_ambiguous"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(read(tmp.path(), "s.md"), "[[x/note]]\n");
}

// A-7 (third disagreement): an ambiguous `[[./note]]` is not a backlink.
#[test]
fn an_ambiguous_dot_slash_link_is_not_a_backlink() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "#\n");
    write_md(tmp.path(), "b/note.md", "#\n");
    write_md(tmp.path(), "s/src.md", "[[./note]]\n");
    assert_eq!(
        json(&run(tmp.path(), &["backlinks", "note.md"]))["total"],
        0
    );
    assert_eq!(broken_total(tmp.path()), 1);
}

// R5: under --allow-ambiguous body and frontmatter rewrite identical text alike.
#[test]
fn allow_ambiguous_rewrites_body_and_frontmatter_alike() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a/note.md", "#\n");
    write_md(tmp.path(), "b/note.md", "#\n");
    write_md(
        tmp.path(),
        "s.md",
        "---\nf: \"[[note?x=1]]\"\n---\n[[note?x=1]]\n",
    );
    run(
        tmp.path(),
        &["mv", "a/note.md", "--to", "a/moved.md", "--allow-ambiguous"],
    );
    assert_eq!(
        read(tmp.path(), "s.md"),
        "---\nf: \"[[moved?x=1]]\"\n---\n[[moved?x=1]]\n"
    );
}

// R6 + R7: links fix keeps the query in the body, and reports the whole text.
#[test]
fn links_fix_keeps_query_and_reports_full_text_in_body_and_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "sub/Note.md", "# H\n");
    write_md(
        tmp.path(),
        "s.md",
        "---\nr: \"[[SUB/note?x=1#H]]\"\n---\n[[SUB/note?x=1#H]]\n[[SUB/note?x=1]]\n",
    );
    let out = json(&run(tmp.path(), &["links", "fix", "--apply"]));
    assert_eq!(
        read(tmp.path(), "s.md"),
        "---\nr: \"[[sub/Note?x=1#H]]\"\n---\n[[sub/Note?x=1#H]]\n[[sub/Note?x=1]]\n"
    );
    let emitted: Vec<&str> = out["results"]["case_mismatch_fixes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["emitted_target"].as_str().unwrap())
        .collect();
    assert_eq!(
        emitted,
        ["sub/Note?x=1#H", "sub/Note?x=1#H", "sub/Note?x=1"]
    );
}

// R9: a padded relative wikilink counts the way the reader reads it.
#[test]
fn padded_relative_wikilinks_agree_across_counters() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "sub/sib.md", "#\n");
    write_md(tmp.path(), "other/sib.md", "#\n");
    write_md(tmp.path(), "sub/src.md", "[[ ./sib ]]\n");
    assert_eq!(broken_total(tmp.path()), 0);
    let fix = json(&run(tmp.path(), &["links", "fix"]));
    assert_eq!(fix["results"]["ambiguous"], 0, "{fix}");
}

// M1: a `?` in a note name is read, counted and moved like any other.
#[test]
fn question_marks_in_note_names_agree_everywhere() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "What is this?.md", "#\n");
    write_md(tmp.path(), "Why?.md", "# Head\n");
    write_md(tmp.path(), "sub/deep?.md", "#\n");
    write_md(tmp.path(), "plain.md", "#\n");
    write_md(
        tmp.path(),
        "src.md",
        "[[What is this?]] [[Why?#Head]] [[Why?|alias]] [[sub/deep?]] [[plain?x=1]]\n",
    );
    assert_eq!(broken_total(tmp.path()), 0);
    assert_eq!(
        json(&run(tmp.path(), &["links", "fix"]))["results"]["broken"],
        0
    );
    let moved = json(&run(
        tmp.path(),
        &["mv", "sub/deep?.md", "--to", "other/deep?.md"],
    ));
    assert_eq!(moved["results"]["total_links_updated"], 1);
    assert!(read(tmp.path(), "src.md").contains("[[other/deep?]]"));
    // A wikilink to a new `?` name is written literally, not as `%3F`.
    write_md(tmp.path(), "note.md", "#\n");
    write_md(tmp.path(), "s2.md", "[[note]]\n");
    run(tmp.path(), &["mv", "note.md", "--to", "sub/What2?.md"]);
    assert_eq!(read(tmp.path(), "s2.md"), "[[What2?]]\n");
    assert_eq!(broken_total(tmp.path()), 0);
}
