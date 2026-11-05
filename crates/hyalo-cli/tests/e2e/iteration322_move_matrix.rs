//! Iteration 322: the move matrix. Every link form, in body and frontmatter,
//! under every kind of move, with a twin note present and absent.
//!
//! Invariant: after `mv`, each link resolves to the note it named before
//! (renamed if that note moved), or `mv` listed it as skipped (ambiguous).
//! Never silently different, never silently broken. The disk reading and the
//! `--index` reading of the vault agree before and after.

use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use tempfile::TempDir;

/// Link forms valid from `src/a.md`. `None` frontmatter = body only.
const FORMS: &[(&str, bool)] = &[
    ("[[x]]", true),
    ("[[./x]]", true),
    ("[[../up]]", true),
    ("[[src/x]]", true),
    ("[[x/note]]", true),
    ("[[x?q=1]]", true),
    ("[[x#H]]", true),
    ("[[x|l]]", true),
    ("![[x]]", false),
    ("[[dir]]", true),
    ("[[what?]]", true),
    ("[[x.md]]", true),
    ("[m](x.md)", false),
    ("[m](../up.md)", false),
    ("[m](x.md?v=2#H)", false),
];

/// The same forms valid from the vault root (`inb.md`).
const ROOT_FORMS: &[(&str, bool)] = &[
    ("[[x]]", true),
    ("[[./src/x]]", true),
    ("[[up]]", true),
    ("[[src/x]]", true),
    ("[[x/note]]", true),
    ("[[x?q=1]]", true),
    ("[[x#H]]", true),
    ("[[x|l]]", true),
    ("![[x]]", false),
    ("[[dir]]", true),
    ("[[what?]]", true),
    ("[m](src/x.md)", false),
    ("[m](up.md)", false),
];

fn source(forms: &[(&str, bool)]) -> String {
    let mut s = String::from("---\n");
    let mut n = 0;
    for (form, in_fm) in forms {
        if *in_fm {
            n += 1;
            let _ = writeln!(s, "p{n}: \"{}\"", form.replace('"', "\\\""));
        }
    }
    s.push_str("---\n");
    for (form, _) in forms {
        s.push_str(form);
        s.push('\n');
    }
    s
}

fn build(twin: bool) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "src/a.md", &source(FORMS));
    write_md(d, "inb.md", &source(ROOT_FORMS));
    write_md(d, "src/x.md", "# H\n");
    write_md(d, "up.md", "# up\n");
    write_md(d, "lib/x/note.md", "# n\n");
    write_md(d, "dir/index.md", "# d\n");
    write_md(d, "src/what?.md", "# w\n");
    if twin {
        write_md(d, "other/x.md", "# H\n");
        write_md(d, "other/up.md", "# up\n");
        write_md(d, "other/x/note.md", "# n\n");
        write_md(d, "other/dir/index.md", "# d\n");
        write_md(d, "other/what?.md", "# w\n");
    }
    tmp
}

fn run(dir: &Path, args: &[&str]) -> (Value, String) {
    let out = hyalo_no_hints()
        .current_dir(dir)
        .arg("--dir")
        .arg(".")
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    let v = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{args:?}: not JSON ({e}): {} / {err}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    (v, err)
}

/// file -> ordered (target, path) of every link.
fn links(dir: &Path, extra: &[&str]) -> HashMap<String, Vec<(String, Option<String>)>> {
    let mut args = vec!["find", "--fields", "links", "--limit", "0"];
    args.extend_from_slice(extra);
    let (v, _) = run(dir, &args);
    v["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            let ls = f["links"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|l| {
                            (
                                l["target"].as_str().unwrap().to_owned(),
                                l["path"].as_str().map(str::to_owned),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            (f["file"].as_str().unwrap().to_owned(), ls)
        })
        .collect()
}

struct Case {
    label: &'static str,
    /// `(from, to)` single move, or a batch glob when `batch` is set.
    moves: &'static [(&'static str, &'static str)],
    batch: Option<(&'static str, &'static str)>,
}

const CASES: &[Case] = &[
    Case {
        label: "target x moved",
        moves: &[("src/x.md", "elsewhere/x2.md")],
        batch: None,
    },
    Case {
        label: "target up moved",
        moves: &[("up.md", "up2.md")],
        batch: None,
    },
    Case {
        label: "partial-path note moved",
        moves: &[("lib/x/note.md", "lib/y/note2.md")],
        batch: None,
    },
    Case {
        label: "dir index moved",
        moves: &[("dir/index.md", "dir2/index.md")],
        batch: None,
    },
    Case {
        label: "? note moved",
        moves: &[("src/what?.md", "src/what2?.md")],
        batch: None,
    },
    Case {
        label: "source deeper",
        moves: &[("src/a.md", "src/deep/er/a.md")],
        batch: None,
    },
    Case {
        label: "source shallower",
        moves: &[("src/a.md", "a.md")],
        batch: None,
    },
    Case {
        label: "source sideways",
        moves: &[("src/a.md", "side/a.md")],
        batch: None,
    },
    Case {
        label: "source and targets in one batch",
        moves: &[
            ("src/a.md", "moved/a.md"),
            ("src/x.md", "moved/x.md"),
            ("src/what?.md", "moved/what?.md"),
        ],
        batch: Some(("src/*.md", "moved/")),
    },
];

fn map_path(p: &str, moves: &[(&str, &str)]) -> String {
    moves
        .iter()
        .find(|(from, _)| *from == p)
        .map_or_else(|| p.to_owned(), |(_, to)| (*to).to_owned())
}

#[test]
fn every_link_form_survives_every_move_with_and_without_a_twin() {
    let mut checks = 0usize;
    let mut skipped_listed = 0usize;
    let mut failures = Vec::new();
    for twin in [false, true] {
        for case in CASES {
            let tmp = build(twin);
            let d = tmp.path();
            let before = links(d, &[]);
            // The disk and index readings agree before the move.
            run(d, &["create-index", "--force"]);
            assert_eq!(
                before,
                links(d, &["--index"]),
                "{} twin={twin}: disk and --index differ before the move",
                case.label
            );
            run(d, &["drop-index"]);

            let mut listed = String::new();
            if let Some((glob, to)) = case.batch {
                let (v, err) = run(d, &["mv", "--glob", glob, "--to", to, "--apply"]);
                listed.push_str(&v.to_string());
                listed.push_str(&err);
            } else {
                for (from, to) in case.moves {
                    let (v, err) = run(d, &["mv", from, "--to", to]);
                    listed.push_str(&v.to_string());
                    listed.push_str(&err);
                }
            }
            let after = links(d, &[]);
            run(d, &["create-index", "--force"]);
            assert_eq!(
                after,
                links(d, &["--index"]),
                "{} twin={twin}: disk and --index differ after the move",
                case.label
            );

            for (file, before_links) in &before {
                let new_file = map_path(file, case.moves);
                let Some(after_links) = after.get(&new_file) else {
                    failures.push(format!("{} twin={twin}: {file} vanished", case.label));
                    continue;
                };
                if before_links.len() != after_links.len() {
                    failures.push(format!(
                        "{} twin={twin}: {file} had {} links, {new_file} has {}",
                        case.label,
                        before_links.len(),
                        after_links.len()
                    ));
                    continue;
                }
                for (i, ((target, was), (now_target, now))) in
                    before_links.iter().zip(after_links).enumerate()
                {
                    let Some(was) = was else { continue };
                    checks += 1;
                    let expected = map_path(was, case.moves);
                    if now.as_deref() == Some(expected.as_str()) {
                        continue;
                    }
                    // Not the same note: it must have been listed.
                    let named = listed.contains(&format!("[[{target}]]"))
                        || listed.contains(&format!("\"target\":\"{target}\""));
                    if named && listed.contains("mbiguous") {
                        skipped_listed += 1;
                        continue;
                    }
                    failures.push(format!(
                        "{} twin={twin}: {file} link #{i} `{target}` -> {was} before, \
                         `{now_target}` -> {now:?} after (expected {expected}), not listed",
                        case.label
                    ));
                }
            }
        }
    }
    eprintln!("move matrix: {checks} link checks, {skipped_listed} listed as skipped");
    assert!(
        failures.is_empty(),
        "{} of {checks} link checks failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(checks > 600, "the matrix shrank: {checks}");
}
