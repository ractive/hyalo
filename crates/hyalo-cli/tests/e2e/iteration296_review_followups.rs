//! Authored regressions: preservation, ownership, emitted identity and scaffold types.
use super::common::{hyalo_no_hints, write_md};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Output};
use tempfile::TempDir;

fn run(root: &Path, args: &[&str]) -> Output {
    hyalo_no_hints()
        .current_dir(root)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap()
}
fn ok(root: &Path, args: &[&str]) -> Value {
    let out = run(root, args);
    assert!(
        out.status.success(),
        "{args:?}: {} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn manifest(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join(".pi/package.json")).unwrap()).unwrap()
}

const PI_ARTIFACTS: [&str; 5] = [
    ".pi/skills/hyalo/SKILL.md",
    ".pi/skills/hyalo-tidy/SKILL.md",
    ".pi/extensions/hyalo.ts",
    ".pi/lib/hyalo-api.js",
    ".pi/lib/hyalo-api.d.ts",
];

#[test]
fn iteration296_pi_artifact_collisions_and_unrecognized_ownership_preserve_files() {
    for receipt in [None, Some("{}"), Some(r#"{"version":99}"#)] {
        let tmp = TempDir::new().unwrap();
        let shared = r#"{"type":"module","pi":{"extensions":["./extensions/hyalo.ts"]}}"#;
        write_md(tmp.path(), ".pi/package.json", shared);
        for path in PI_ARTIFACTS {
            write_md(tmp.path(), path, "User authored bytes\n");
        }
        if let Some(receipt) = receipt {
            write_md(tmp.path(), ".pi/.hyalo-manifest.json", receipt);
        }
        assert!(!run(tmp.path(), &["init", "--pi"]).status.success());
        assert!(!tmp.path().join(".hyalo.toml").exists());
        ok(tmp.path(), &["deinit"]);
        for path in PI_ARTIFACTS {
            assert_eq!(
                fs::read_to_string(tmp.path().join(path)).unwrap(),
                "User authored bytes\n"
            );
        }
        assert_eq!(
            fs::read_to_string(tmp.path().join(".pi/package.json")).unwrap(),
            shared
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join(".pi/.hyalo-manifest.json"))
                .ok()
                .as_deref(),
            receipt
        );
    }
}

#[test]
fn iteration296_pi_changed_artifacts_preserve_registration_and_receipt_but_owned_upgrade_works() {
    for path in PI_ARTIFACTS {
        let tmp = TempDir::new().unwrap();
        ok(tmp.path(), &["init", "--pi"]);
        let installed = fs::read_to_string(tmp.path().join(path)).unwrap();
        let receipt_path = tmp.path().join(".pi/.hyalo-manifest.json");
        let receipt = fs::read(&receipt_path).unwrap();
        let shared = fs::read(tmp.path().join(".pi/package.json")).unwrap();
        write_md(tmp.path(), path, "Edited after install\n");
        assert!(!run(tmp.path(), &["init", "--pi"]).status.success());
        let outcome = ok(tmp.path(), &["deinit"]);
        assert!(outcome.to_string().contains("unowned or changed"));
        assert_eq!(
            fs::read_to_string(tmp.path().join(path)).unwrap(),
            "Edited after install\n"
        );
        assert_eq!(fs::read(&receipt_path).unwrap(), receipt);
        assert_eq!(
            fs::read(tmp.path().join(".pi/package.json")).unwrap(),
            shared
        );
        // Model an unchanged older vendored artifact: the receipt and file agree.
        let mut old: Value = serde_json::from_slice(&receipt).unwrap();
        old["artifacts"][path] = json!("Previous shipped version\n");
        fs::write(&receipt_path, old.to_string()).unwrap();
        write_md(tmp.path(), path, "Previous shipped version\n");
        ok(tmp.path(), &["init", "--pi"]);
        assert_eq!(
            fs::read_to_string(tmp.path().join(path)).unwrap(),
            installed
        );
        ok(tmp.path(), &["deinit"]);
        for artifact in PI_ARTIFACTS {
            assert!(!tmp.path().join(artifact).exists());
        }
        assert!(!receipt_path.exists());
    }
}

#[test]
fn iteration296_manifest_only_legacy_receipt_cannot_own_artifacts() {
    let tmp = TempDir::new().unwrap();
    ok(tmp.path(), &["init", "--pi"]);
    let receipt_path = tmp.path().join(".pi/.hyalo-manifest.json");
    let mut receipt: Value = serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
    receipt["version"] = json!(1);
    receipt.as_object_mut().unwrap().remove("artifacts");
    fs::write(&receipt_path, receipt.to_string()).unwrap();
    let before = fs::read(tmp.path().join(".pi/extensions/hyalo.ts")).unwrap();
    assert!(!run(tmp.path(), &["init", "--pi"]).status.success());
    ok(tmp.path(), &["deinit"]);
    assert_eq!(
        fs::read(tmp.path().join(".pi/extensions/hyalo.ts")).unwrap(),
        before
    );
    assert!(receipt_path.exists());
    assert!(tmp.path().join(".pi/package.json").exists());
}

#[test]
fn iteration296_conflicting_manifest_preserves_pi_artifacts_and_receipt() {
    for (path, conflicting) in [
        (".pi/package.json", r#"{"pi":{"extensions":42}}"#),
        (".pi/lib/package.json", r#"{"type":"commonjs"}"#),
    ] {
        let tmp = TempDir::new().unwrap();
        ok(tmp.path(), &["init", "--pi"]);
        let receipt = fs::read(tmp.path().join(".pi/.hyalo-manifest.json")).unwrap();
        write_md(tmp.path(), path, conflicting);
        let outcome = ok(tmp.path(), &["deinit"]);
        assert!(outcome.to_string().contains("conflicting"));
        for artifact in PI_ARTIFACTS {
            assert!(tmp.path().join(artifact).is_file());
        }
        assert_eq!(
            fs::read_to_string(tmp.path().join(path)).unwrap(),
            conflicting
        );
        assert_eq!(
            fs::read(tmp.path().join(".pi/.hyalo-manifest.json")).unwrap(),
            receipt
        );
    }
}

#[test]
fn iteration296_link_emission_preserves_alias_and_authored_prefix_and_refuses_wrong_identity() {
    let alias = TempDir::new().unwrap();
    write_md(alias.path(), ".hyalo.toml", "dir = '.'\n");
    write_md(
        alias.path(),
        "C#.md",
        "---\naliases: ['C%23']\n---\n# C sharp\n",
    );
    write_md(alias.path(), "source.md", "Use C%23 here.\n");
    ok(
        alias.path(),
        &["links", "auto", "--apply", "--no-warn-common-titles"],
    );
    assert_eq!(
        fs::read_to_string(alias.path().join("source.md")).unwrap(),
        "Use [[C%23|C%23]] here.\n"
    );

    let collision = TempDir::new().unwrap();
    write_md(collision.path(), ".hyalo.toml", "dir = '.'\n");
    write_md(collision.path(), "new.md", "# Other\n");
    write_md(collision.path(), "new/index.md", "# Intended\n");
    write_md(collision.path(), "source.md", "[x](new/indx)\n");
    for apply in [false, true] {
        let mut args = vec!["links", "fix", "--apply-fuzzy", "--min-confidence", "0"];
        if apply {
            args.push("--apply");
        }
        let result = ok(collision.path(), &args);
        assert!(result.to_string().contains("new/index.md"));
        assert_eq!(
            fs::read_to_string(collision.path().join("source.md")).unwrap(),
            "[x](new/indx)\n"
        );
    }

    let prefix = TempDir::new().unwrap();
    write_md(prefix.path(), ".hyalo.toml", "dir = '.'\n");
    write_md(prefix.path(), "page.md", "# Page\n");
    write_md(
        prefix.path(),
        "source.md",
        "[x](/en-us/docs/paeg.md#Page)\n",
    );
    ok(
        prefix.path(),
        &[
            "--site-prefix",
            "en-US/docs",
            "links",
            "fix",
            "--apply",
            "--apply-fuzzy",
            "--min-confidence",
            "0",
        ],
    );
    assert_eq!(
        fs::read_to_string(prefix.path().join("source.md")).unwrap(),
        "[x](/en-us/docs/page.md#Page)\n"
    );
    ok(
        prefix.path(),
        &["--site-prefix", "en-US/docs", "mv", "page.md", "renamed.md"],
    );
    assert_eq!(
        fs::read_to_string(prefix.path().join("source.md")).unwrap(),
        "[x](/en-us/docs/renamed.md#Page)\n"
    );
    let found = ok(
        prefix.path(),
        &[
            "--site-prefix",
            "en-US/docs",
            "find",
            "--file",
            "source.md",
            "--fields",
            "links",
        ],
    );
    assert_eq!(found["results"][0]["links"][0]["path"], "renamed.md");
}

#[test]
fn iteration296_partial_and_invalid_postings_fall_back_to_correct_disk_results() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), ".hyalo.toml", "dir = '.'\n");
    write_md(tmp.path(), "a.md", "needle adjacent\n");
    write_md(tmp.path(), "b.md", "needle adjacent\n");
    ok(tmp.path(), &["create-index"]);
    let path = tmp.path().join(".hyalo-index");
    let mut snapshot: Value = rmp_serde::from_slice(&fs::read(&path).unwrap()).unwrap();
    let version = snapshot["bm25_index"]["tokenizer_version"].clone();
    snapshot["bm25_index"] = json!({"postings":{"needl":[{"doc_id":0,"term_freq":1,"positions":[0]}],"adjac":[{"doc_id":0,"term_freq":1,"positions":[1]}]},"doc_lengths":[2],"doc_paths":["a.md"],"avgdl":2.0,"tokenizer_version":version});
    fs::write(&path, rmp_serde::to_vec_named(&snapshot).unwrap()).unwrap();
    let disk = ok(tmp.path(), &["find", "needle"]);
    let indexed = ok(tmp.path(), &["find", "needle", "--index"]);
    assert_eq!(disk["results"].as_array().unwrap().len(), 2);
    assert_eq!(indexed["results"], disk["results"]);
    // A tiny malformed phrase position must never reach wrapping arithmetic.
    snapshot["bm25_index"]["postings"]["needl"][0]["positions"][0] = json!(u32::MAX);
    fs::write(&path, rmp_serde::to_vec_named(&snapshot).unwrap()).unwrap();
    let disk = ok(tmp.path(), &["find", "\"needle adjacent\""]);
    let indexed = ok(tmp.path(), &["find", "\"needle adjacent\"", "--index"]);
    assert_eq!(indexed["results"], disk["results"]);
}

#[test]
fn iteration296_initial_bom_managed_regions_preview_apply_and_repeat() {
    for (command, namespace, path) in [
        (["madr", "toc"], "madr:toc", "kb/docs/decisions/README.md"),
        (["okf", "index"], "okf:index", "kb/index.md"),
    ] {
        for eol in ["\n", "\r\n"] {
            let tmp = TempDir::new().unwrap();
            write_md(tmp.path(), ".hyalo.toml", "dir = 'kb'\n");
            write_md(
                tmp.path(),
                "kb/docs/decisions/0001-choice.md",
                "---\ntitle: Choice\nstatus: accepted\n---\n# Choice\n",
            );
            let begin = format!("\u{feff}<!-- {namespace}:begin -->");
            let suffix = format!("<!-- {namespace}:end -->{eol}KEEP FOOTER{eol}");
            let old = format!("{begin}{eol}OLD TABLE{eol}{suffix}");
            write_md(tmp.path(), path, &old);
            let preview = run(tmp.path(), &command);
            assert_eq!(preview.status.code(), Some(i32::from(command[0] == "madr")));
            assert_eq!(fs::read_to_string(tmp.path().join(path)).unwrap(), old);
            ok(tmp.path(), &[command[0], command[1], "--apply"]);
            let first = fs::read_to_string(tmp.path().join(path)).unwrap();
            assert!(first.starts_with(&begin), "{first:?}");
            assert!(first.ends_with(&suffix), "{first:?}");
            assert!(!first.contains("OLD TABLE"), "{first:?}");
            ok(tmp.path(), &[command[0], command[1], "--apply"]);
            assert_eq!(fs::read_to_string(tmp.path().join(path)).unwrap(), first);
        }
    }
}

#[test]
fn iteration296_madr_only_changes_the_real_region_and_refuses_malformed_pairs() {
    for eol in ["\n", "\r\n"] {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), ".hyalo.toml", "dir = 'kb'\n");
        write_md(
            tmp.path(),
            "kb/docs/decisions/0001-choice.md",
            "---\ntitle: Choice\nstatus: accepted\n---\n# Choice\n",
        );
        let before = "# Decisions\n\nExample marker: `<!-- madr:toc:begin -->`\n\nKEEP THIS USER PARAGRAPH\n\n```md\n<!-- madr:toc:begin -->\n<!-- madr:toc:end -->\n```\n\n    <!-- madr:toc:begin -->\n    <!-- madr:toc:end -->\n\n".replace('\n', eol);
        let after = "\n\nKEEP THIS FOOTER".replace('\n', eol);
        let old = format!(
            "{before}<!-- madr:toc:begin -->{eol}Old table{eol}<!-- madr:toc:end -->{after}"
        );
        let path = "kb/docs/decisions/README.md";
        write_md(tmp.path(), path, &old);
        let preview = run(tmp.path(), &["madr", "toc"]);
        assert_eq!(
            preview.status.code(),
            Some(1),
            "MADR preview drift keeps its existing exit contract"
        );
        assert_eq!(fs::read_to_string(tmp.path().join(path)).unwrap(), old);
        ok(tmp.path(), &["madr", "toc", "--apply"]);
        let first = fs::read_to_string(tmp.path().join(path)).unwrap();
        assert!(first.starts_with(&before));
        assert!(first.ends_with(&after));
        assert!(!first.contains("Old table"));
        ok(tmp.path(), &["madr", "toc", "--apply"]);
        assert_eq!(fs::read_to_string(tmp.path().join(path)).unwrap(), first);
        for bad in [
            "<!-- madr:toc:begin -->\nKeep",
            "<!-- madr:toc:end -->\n<!-- madr:toc:begin -->",
            "<!-- madr:toc:begin -->\n<!-- madr:toc:begin -->\n<!-- madr:toc:end -->",
            "<!-- madr:toc:begin -->\n<!-- madr:toc:end -->\n<!-- madr:toc:end -->",
        ] {
            write_md(tmp.path(), path, bad);
            for args in [
                vec!["madr", "toc"],
                vec!["madr", "toc", "--apply"],
                vec!["madr", "toc", "--apply", "--replace"],
            ] {
                let out = run(tmp.path(), &args);
                assert_eq!(out.status.code(), Some(1));
                assert_eq!(fs::read_to_string(tmp.path().join(path)).unwrap(), bad);
            }
        }
    }
}

#[test]
fn iteration296_pi_shared_manifest_round_trip_and_repeat_install() {
    let tmp = TempDir::new().unwrap();
    let original = json!({"private":true,"dependencies":{"custom-extension":"1.0.0"},"scripts":{"test":"custom"},"pi":{"extensions":["./other", "./extensions"],"skills":["./custom"]},"unknown":{"preserve":42}});
    write_md(tmp.path(), ".pi/package.json", &original.to_string());
    ok(tmp.path(), &["init", "--pi"]);
    let first = manifest(tmp.path());
    let receipt = fs::read_to_string(tmp.path().join(".pi/.hyalo-manifest.json")).unwrap();
    let manifest_receipt: Value = serde_json::from_str(&receipt).unwrap();
    for key in [
        "original",
        "installed",
        "runtime_original",
        "runtime_installed",
    ] {
        let projected = manifest_receipt[key].to_string();
        assert!(!projected.contains("custom-extension") && !projected.contains("preserve"));
    }
    assert_eq!(
        first["pi"]["skills"],
        json!(["./custom", "./skills/hyalo", "./skills/hyalo-tidy"])
    );

    assert_eq!(first["dependencies"], original["dependencies"]);
    assert_eq!(first["scripts"], original["scripts"]);
    assert_eq!(first["pi"]["extensions"], original["pi"]["extensions"]);
    assert!(first.get("type").is_none());
    assert_eq!(
        serde_json::from_slice::<Value>(
            &fs::read(tmp.path().join(".pi/lib/package.json")).unwrap()
        )
        .unwrap()["type"],
        "module"
    );
    ok(tmp.path(), &["init", "--pi"]);
    assert_eq!(manifest(tmp.path()), first);
    assert!(tmp.path().join(".pi/lib/hyalo-api.js").is_file());
    ok(tmp.path(), &["deinit"]);
    assert_eq!(manifest(tmp.path()), original);
    assert!(!tmp.path().join(".pi/.hyalo-manifest.json").exists());
}

#[test]
fn iteration296_pi_reinit_records_new_additions_and_preserves_user_edits() {
    let tmp = TempDir::new().unwrap();
    let original = json!({"private":true,"pi":{"extensions":["./extensions"],"skills":["./skills"]},"dependencies":{"custom":"1"}});
    write_md(tmp.path(), ".pi/package.json", &original.to_string());
    write_md(
        tmp.path(),
        ".pi/lib/package.json",
        r#"{"type":"module","private":true}"#,
    );
    ok(tmp.path(), &["init", "--pi"]);
    let mut edited = original;
    edited["pi"]["extensions"] = json!(["./user-extension"]);
    edited["dependencies"]["custom"] = json!("2");
    write_md(tmp.path(), ".pi/package.json", &edited.to_string());
    let runtime_edited = json!({"private":true,"user":{"keep":42}});
    write_md(
        tmp.path(),
        ".pi/lib/package.json",
        &runtime_edited.to_string(),
    );
    ok(tmp.path(), &["init", "--pi"]);
    assert_eq!(
        manifest(tmp.path())["pi"]["extensions"],
        json!(["./user-extension", "./extensions/hyalo.ts"])
    );
    // A third install must retain ownership of additions from the second.
    ok(tmp.path(), &["init", "--pi"]);
    ok(tmp.path(), &["deinit"]);
    assert_eq!(manifest(tmp.path()), edited);
    assert_eq!(
        serde_json::from_slice::<Value>(
            &fs::read(tmp.path().join(".pi/lib/package.json")).unwrap()
        )
        .unwrap(),
        runtime_edited
    );
    assert!(!tmp.path().join(".pi/.hyalo-manifest.json").exists());

    let fresh = TempDir::new().unwrap();
    ok(fresh.path(), &["init", "--pi"]);
    ok(fresh.path(), &["init", "--pi"]);
    ok(fresh.path(), &["deinit"]);
    assert!(!fresh.path().join(".pi/package.json").exists());
    assert!(!fresh.path().join(".pi/lib/package.json").exists());

    let changed = TempDir::new().unwrap();
    ok(changed.path(), &["init", "--pi"]);
    let mut user = manifest(changed.path());
    user["name"] = json!("user-project");
    user["pi"]["extensions"] = json!(["./extensions", "./extensions", "./mine"]);
    write_md(changed.path(), ".pi/package.json", &user.to_string());
    ok(changed.path(), &["init", "--pi"]);
    ok(changed.path(), &["deinit"]);
    let kept = manifest(changed.path());
    assert_eq!(kept["name"], user["name"]);
    assert_eq!(kept["pi"]["extensions"], user["pi"]["extensions"]);
}

#[test]
fn iteration296_pi_unmanaged_legacy_fresh_and_user_edits() {
    for original in [
        json!({"private":true,"dependencies":{"custom-extension":"1.0.0"}}),
        serde_json::from_str(include_str!("../../templates/pi/package.json")).unwrap(),
    ] {
        let tmp = TempDir::new().unwrap();
        let bytes = original.to_string();
        write_md(tmp.path(), ".pi/package.json", &bytes);
        ok(tmp.path(), &["deinit"]);
        assert_eq!(
            fs::read_to_string(tmp.path().join(".pi/package.json")).unwrap(),
            bytes
        );
    }
    let tmp = TempDir::new().unwrap();
    ok(tmp.path(), &["init", "--pi"]);
    ok(tmp.path(), &["deinit"]);
    assert!(!tmp.path().join(".pi/package.json").exists());
    ok(tmp.path(), &["init", "--pi"]);
    let mut edited = manifest(tmp.path());
    edited["name"] = json!("my-project");
    edited["dependencies"] = json!({"custom":"2"});
    edited["pi"]["extensions"] = json!(["./extensions", "./extensions", "./mine"]);
    fs::write(tmp.path().join(".pi/package.json"), edited.to_string()).unwrap();
    let out = ok(tmp.path(), &["deinit"]);
    let kept = manifest(tmp.path());
    assert_eq!(kept["name"], "my-project");
    assert_eq!(kept["dependencies"], edited["dependencies"]);
    assert_eq!(kept["pi"]["extensions"], edited["pi"]["extensions"]);
    assert!(out.to_string().contains("ambiguous"));
}

#[test]
fn iteration296_pi_invalid_and_runtime_conflicts_fail_before_artifacts() {
    for bytes in ["not json", "[]", r#"{"pi":{"extensions":42}}"#] {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), ".pi/package.json", bytes);
        let out = run(tmp.path(), &["init", "--pi"]);
        assert!(!out.status.success());
        assert_eq!(
            fs::read_to_string(tmp.path().join(".pi/package.json")).unwrap(),
            bytes
        );
        assert!(!tmp.path().join(".hyalo.toml").exists());
        assert!(!tmp.path().join(".pi/extensions").exists());
        assert!(!tmp.path().join(".pi/.hyalo-manifest.json").exists());
    }
    for (parent, nested) in [
        (r#"{"type":"module"}"#, Some("{}")),
        (r#"{"type":"module"}"#, Some(r#"{"type":"commonjs"}"#)),
        (r#"{"type":"commonjs"}"#, None),
        ("{}", None),
    ] {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), ".pi/package.json", parent);
        if let Some(nested) = nested {
            write_md(tmp.path(), ".pi/lib/package.json", nested);
        }
        let helper = "module.exports = 42;\n";
        write_md(tmp.path(), ".pi/lib/helper.js", helper);
        assert!(!run(tmp.path(), &["init", "--pi"]).status.success());
        assert_eq!(
            fs::read_to_string(tmp.path().join(".pi/package.json")).unwrap(),
            parent
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join(".pi/lib/helper.js")).unwrap(),
            helper
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join(".pi/lib/package.json"))
                .ok()
                .as_deref(),
            nested
        );
        assert!(!tmp.path().join(".hyalo.toml").exists());
        assert!(!tmp.path().join(".pi/extensions").exists());
        assert!(!tmp.path().join(".pi/.hyalo-manifest.json").exists());
    }
}

#[test]
fn iteration296_move_encoding_round_trips_identity_and_index_backlinks() {
    for name in [
        "C#.md",
        "Release (final).md",
        "Space name.md",
        "literal%23.md",
    ] {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), ".hyalo.toml", "dir = '.'\n");
        write_md(tmp.path(), "note.md", "---\ntitle: Note\n---\n# Intro\n");
        write_md(
            tmp.path(),
            "ref.md",
            "---\nrelated: '[[note#Intro|FM label]]'\n---\n[Note label](note.md#Intro)\n\n[[note#Intro|Wiki label]]\n",
        );
        ok(tmp.path(), &["create-index"]);
        ok(tmp.path(), &["mv", "note.md", name, "--dry-run", "--index"]);
        assert!(tmp.path().join("note.md").exists());
        ok(tmp.path(), &["mv", "note.md", name, "--index"]);
        assert!(!tmp.path().join("note.md").exists());
        let disk = ok(
            tmp.path(),
            &["find", "--file", "ref.md", "--fields", "links,backlinks"],
        );
        let indexed = ok(
            tmp.path(),
            &[
                "find",
                "--file",
                "ref.md",
                "--fields",
                "links,backlinks",
                "--index",
            ],
        );
        assert_eq!(disk["results"], indexed["results"]);
        let links = disk["results"][0]["links"].as_array().unwrap();
        assert_eq!(links.len(), 3);
        for link in links {
            assert_eq!(link["path"], name, "{link}");
            assert_ne!(link["broken_anchor"], true);
        }
        let back = ok(
            tmp.path(),
            &["find", "--file", name, "--fields", "backlinks"],
        );
        let back_index = ok(
            tmp.path(),
            &["find", "--file", name, "--fields", "backlinks", "--index"],
        );
        assert_eq!(back["results"], back_index["results"]);
        assert!(back.to_string().contains("ref.md"));
        ok(tmp.path(), &["mv", name, "again.md", "--index"]);
        let again = ok(
            tmp.path(),
            &["find", "--file", "ref.md", "--fields", "links", "--index"],
        );
        for link in again["results"][0]["links"].as_array().unwrap() {
            assert_eq!(link["path"], "again.md", "{link}");
        }
    }
}

#[test]
fn iteration296_small_batch_and_destination_collision_refusal() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), ".hyalo.toml", "dir = '.'\n");
    for name in ["a", "b"] {
        write_md(tmp.path(), &format!("{name}.md"), "# Note\n");
    }
    write_md(tmp.path(), "ref.md", "[A](a.md) [[b]]\n");
    ok(tmp.path(), &["create-index"]);
    ok(
        tmp.path(),
        &[
            "mv",
            "--glob",
            "[ab].md",
            "--to",
            "Release (final)/",
            "--apply",
            "--index",
        ],
    );
    let disk = ok(
        tmp.path(),
        &["find", "--file", "ref.md", "--fields", "links"],
    );
    let indexed = ok(
        tmp.path(),
        &["find", "--file", "ref.md", "--fields", "links", "--index"],
    );
    assert_eq!(disk["results"], indexed["results"]);
    for link in disk["results"][0]["links"].as_array().unwrap() {
        assert!(link["path"].is_string(), "{link}");
    }
}

#[test]
fn iteration296_scaffold_exact_defaults_types_preview_and_invalid_control() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        ".hyalo.toml",
        r#"dir = '.'
[schema.types.note]
required = ["title", "missing"]
[schema.types.note.defaults]
title = "[Draft]"
braces = "{draft}"
multiline = "first\nsecond"
"quoted: key" = "a: b"
number = "3.5"
enabled = "true"
items = "[one, two]"
[schema.types.note.properties.title]
type = "string"
[schema.types.note.properties.missing]
type = "number"
[schema.types.note.properties.number]
type = "number"
[schema.types.note.properties.enabled]
type = "boolean"
[schema.types.note.properties.items]
type = "list"
"#,
    );
    let preview = ok(
        tmp.path(),
        &[
            "new",
            "--type",
            "note",
            "--file",
            "nested/note.md",
            "--dry-run",
        ],
    );
    assert!(!tmp.path().join("nested").exists());
    ok(
        tmp.path(),
        &["new", "--type", "note", "--file", "nested/note.md"],
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join("nested/note.md")).unwrap(),
        preview["results"]["content"]
    );
    let found = ok(
        tmp.path(),
        &["find", "--file", "nested/note.md", "--fields", "properties"],
    );
    let props = &found["results"][0]["properties"];
    assert_eq!(props["title"], "[Draft]");
    assert_eq!(props["braces"], "{draft}");
    assert_eq!(props["multiline"], "first\nsecond");
    assert_eq!(props["quoted: key"], "a: b");
    assert_eq!(props["number"], 3.5);
    assert_eq!(props["enabled"], true);
    assert_eq!(props["items"], json!(["one", "two"]));
    assert!(props["missing"].is_null());
    let original = fs::read(tmp.path().join("nested/note.md")).unwrap();
    assert!(
        !run(
            tmp.path(),
            &["new", "--type", "note", "--file", "nested/note.md"]
        )
        .status
        .success()
    );
    assert_eq!(
        fs::read(tmp.path().join("nested/note.md")).unwrap(),
        original
    );
    write_md(
        tmp.path(),
        ".hyalo.toml",
        "dir = '.'\n[schema.types.note.defaults]\nnumber = 'invalid'\n[schema.types.note.properties.number]\ntype = 'number'\n",
    );
    for dry in [false, true] {
        let mut args = vec!["new", "--type", "note", "--file", "invalid/new.md"];
        if dry {
            args.push("--dry-run");
        }
        assert_eq!(run(tmp.path(), &args).status.code(), Some(1));
        assert!(!tmp.path().join("invalid").exists());
    }
}

#[test]
fn iteration296_scaffold_preserves_accepted_integer_default_spellings() {
    for (default, expected) in [("+1", 1), ("007", 7), ("-007", -7), ("+0", 0)] {
        let tmp = TempDir::new().unwrap();
        write_md(
            tmp.path(),
            ".hyalo.toml",
            &format!(
                "dir = '.'\n[schema.types.note.defaults]\nnumber = '{default}'\n[schema.types.note.properties.number]\ntype = 'number'\n"
            ),
        );
        let preview = ok(
            tmp.path(),
            &[
                "new",
                "--type",
                "note",
                "--file",
                "nested/note.md",
                "--dry-run",
            ],
        );
        assert!(!tmp.path().join("nested").exists());
        ok(
            tmp.path(),
            &["new", "--type", "note", "--file", "nested/note.md"],
        );
        let content = fs::read_to_string(tmp.path().join("nested/note.md")).unwrap();
        assert_eq!(content, preview["results"]["content"]);
        assert!(
            content.contains(&format!("number: {expected}\n")),
            "{default}: {content}"
        );
        let found = ok(
            tmp.path(),
            &["find", "--file", "nested/note.md", "--fields", "properties"],
        );
        assert_eq!(
            found["results"][0]["properties"]["number"].as_i64(),
            Some(expected),
            "{default}"
        );
    }
}

#[cfg(unix)]
#[test]
fn iteration296_scaffold_symlink_parent_has_same_preview_apply_refusal() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        ".hyalo.toml",
        "dir = '.'\n[schema.types.note]\nrequired = ['title']\n",
    );
    fs::create_dir(tmp.path().join("real")).unwrap();
    std::os::unix::fs::symlink("real", tmp.path().join("alias")).unwrap();
    let preview = run(
        tmp.path(),
        &[
            "new",
            "--type",
            "note",
            "--file",
            "alias/sub/new.md",
            "--dry-run",
        ],
    );
    let apply = run(
        tmp.path(),
        &["new", "--type", "note", "--file", "alias/sub/new.md"],
    );
    assert_eq!(preview.status.code(), Some(1));
    assert_eq!(apply.status.code(), Some(1));
    assert_eq!(preview.stdout, apply.stdout);
    assert!(!tmp.path().join("real/sub").exists());
}

#[test]
fn iteration296_auto_apply_and_ambiguous_emission_refusal() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), ".hyalo.toml", "dir = '.'\n");
    write_md(
        tmp.path(),
        "C#.md",
        "---\ntitle: C Sharp Language\n---\n# Intro\n",
    );
    write_md(
        tmp.path(),
        "source.md",
        "The C Sharp Language is documented here.\n",
    );
    ok(tmp.path(), &["links", "auto", "--apply"]);
    let found = ok(
        tmp.path(),
        &["find", "--file", "source.md", "--fields", "links"],
    );
    assert_eq!(found["results"][0]["links"][0]["path"], "C#.md");
    write_md(tmp.path(), "note.md", "Keep source bytes\n");
    write_md(tmp.path(), "other.md", "Existing same stem\n");
    write_md(tmp.path(), "ref.md", "[[note]]\n");
    fs::create_dir(tmp.path().join("sub")).unwrap();
    for dry in [false, true] {
        let mut args = vec!["mv", "note.md", "sub/other.md"];
        if dry {
            args.push("--dry-run");
        }
        let out = run(tmp.path(), &args);
        assert_eq!(out.status.code(), Some(1));
        let diagnostic = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(diagnostic.contains("emitted destination"), "{diagnostic}");
        assert_eq!(
            fs::read_to_string(tmp.path().join("note.md")).unwrap(),
            "Keep source bytes\n"
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join("ref.md")).unwrap(),
            "[[note]]\n"
        );
        assert!(!tmp.path().join("sub/other.md").exists());
    }
}

#[test]
fn iteration296_duplicate_manifest_keys_refuse_init_and_survive_deinit() {
    for (target, bytes) in [
        (
            ".pi/package.json",
            r#"{"private":true,"dependencies":{"first":"1"},"dependencies":{"second":"2"}}"#,
        ),
        (
            ".pi/package.json",
            r#"{"dependencies":{"first":"1","first":"2"}}"#,
        ),
        (
            ".pi/lib/package.json",
            r#"{"type":"module","type":"commonjs"}"#,
        ),
        (
            ".pi/lib/package.json",
            r#"{"type":"module","unknown":[{"key":"first","key":"second"}]}"#,
        ),
    ] {
        let fresh = TempDir::new().unwrap();
        write_md(fresh.path(), target, bytes);
        let refused = run(fresh.path(), &["init", "--pi"]);
        assert_eq!(refused.status.code(), Some(1));
        let diagnostic = format!(
            "{}{}",
            String::from_utf8_lossy(&refused.stdout),
            String::from_utf8_lossy(&refused.stderr)
        );
        assert!(
            diagnostic.contains("duplicate JSON object key"),
            "{diagnostic}"
        );
        assert_eq!(
            fs::read_to_string(fresh.path().join(target)).unwrap(),
            bytes
        );
        for artifact in [
            ".hyalo.toml",
            ".pi/extensions",
            ".pi/skills",
            ".pi/lib/hyalo-api.js",
            ".pi/.hyalo-manifest.json",
        ] {
            assert!(
                !fresh.path().join(artifact).exists(),
                "refusal installed {artifact}"
            );
        }

        let installed = TempDir::new().unwrap();
        ok(installed.path(), &["init", "--pi"]);
        let receipt = fs::read(installed.path().join(".pi/.hyalo-manifest.json")).unwrap();
        write_md(installed.path(), target, bytes);
        let removed = ok(installed.path(), &["deinit"]);
        assert!(removed.to_string().contains("duplicate JSON object key"));
        assert_eq!(
            fs::read_to_string(installed.path().join(target)).unwrap(),
            bytes
        );
        assert_eq!(
            fs::read(installed.path().join(".pi/.hyalo-manifest.json")).unwrap(),
            receipt
        );
    }
}

#[test]
fn iteration296_duplicate_receipt_keys_preserve_ownership_and_shared_manifests() {
    for nested in [false, true] {
        let installed = TempDir::new().unwrap();
        ok(installed.path(), &["init", "--pi"]);
        let receipt_path = installed.path().join(".pi/.hyalo-manifest.json");
        let receipt_value: Value =
            serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
        let receipt = receipt_value.to_string();
        let ambiguous = if nested {
            receipt.replace(
                r#""runtime_installed":{"type":"module"}"#,
                r#""runtime_installed":{"type":"module","type":"module"}"#,
            )
        } else {
            receipt.replace(r#""version":2"#, r#""version":2,"version":2"#)
        };
        assert_ne!(ambiguous, receipt);
        fs::write(&receipt_path, &ambiguous).unwrap();
        let shared = fs::read(installed.path().join(".pi/package.json")).unwrap();
        let runtime = fs::read(installed.path().join(".pi/lib/package.json")).unwrap();
        let removed = ok(installed.path(), &["deinit"]);
        assert!(removed.to_string().contains("duplicate JSON object key"));
        assert_eq!(
            fs::read(installed.path().join(".pi/package.json")).unwrap(),
            shared
        );
        assert_eq!(
            fs::read(installed.path().join(".pi/lib/package.json")).unwrap(),
            runtime
        );
        assert_eq!(fs::read_to_string(&receipt_path).unwrap(), ambiguous);

        let fresh = TempDir::new().unwrap();
        write_md(fresh.path(), ".pi/.hyalo-manifest.json", &ambiguous);
        let refused = run(fresh.path(), &["init", "--pi"]);
        assert_eq!(refused.status.code(), Some(1));
        for artifact in [
            ".hyalo.toml",
            ".pi/extensions",
            ".pi/skills",
            ".pi/package.json",
            ".pi/lib",
        ] {
            assert!(
                !fresh.path().join(artifact).exists(),
                "refusal installed {artifact}"
            );
        }
        assert_eq!(
            fs::read_to_string(fresh.path().join(".pi/.hyalo-manifest.json")).unwrap(),
            ambiguous
        );
    }
    // Ordinary JSON semantics remain those of serde_json, including surrogate
    // pairs and exact unsigned integers; strictness only removes duplicate keys.
    let tmp = TempDir::new().unwrap();
    let bytes =
        r#"{"private":true,"unknown":[null,false,1,18446744073709551615,1.25,"\ud83d\ude00"]}"#;
    write_md(tmp.path(), ".pi/package.json", bytes);
    ok(tmp.path(), &["init", "--pi"]);
    ok(tmp.path(), &["deinit"]);
    assert_eq!(
        manifest(tmp.path()),
        serde_json::from_str::<Value>(bytes).unwrap()
    );
}
