use super::common::hyalo_no_hints;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Output;
use tempfile::TempDir;

fn run(root: &Path, args: &[&str]) -> Output {
    hyalo_no_hints()
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

fn ok(root: &Path, args: &[&str]) -> Value {
    let mut args = args.to_vec();
    args.extend(["--format", "json"]);
    let out = run(root, &args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn codex_install_update_remove_preserves_config_and_user_content() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join("custom notes")).unwrap();
    let config = "# keep this\ndir = \"custom notes\"\nformat = \"text\"\n";
    fs::write(root.join(".hyalo.toml"), config).unwrap();
    let user = "# User guidance\r\nPreserve this.\r\n";
    fs::write(root.join("AGENTS.md"), user).unwrap();
    let first = ok(root, &["init", "--codex"]);
    assert_eq!(first["dir"], "custom notes");
    assert_eq!(
        fs::read_to_string(root.join(".hyalo.toml")).unwrap(),
        config
    );
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(agents.starts_with(user));
    assert!(agents.contains("custom notes"));
    let skill = root.join(".agents/skills/hyalo/SKILL.md");
    assert!(skill.is_file());
    fs::write(&skill, "<!-- hyalo:managed -->\nold version").unwrap();
    let second = ok(root, &["init", "--codex"]);
    assert!(
        second["results"]["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["action"] == "updated" && a["target"] == ".agents/skills/hyalo/SKILL.md")
    );
    assert_eq!(fs::read_to_string(root.join("AGENTS.md")).unwrap(), agents);
    fs::write(root.join(".agents/skills/hyalo/personal.txt"), "keep").unwrap();
    ok(root, &["deinit"]);
    assert!(!skill.exists());
    assert!(root.join(".agents/skills/hyalo/personal.txt").exists());
    assert_eq!(fs::read_to_string(root.join("AGENTS.md")).unwrap(), user);
    ok(root, &["deinit"]);
}

#[test]
fn codex_profiles_are_recovered_and_scanned_in_their_real_location() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    ok(root, &["init", "--dir", ".", "--profile", "skills"]);
    ok(root, &["init", "--profile", "okf"]);
    ok(root, &["init", "--codex"]);
    assert!(root.join(".agents/skills/hyalo-skills/SKILL.md").exists());
    assert!(root.join(".agents/skills/hyalo-okf/SKILL.md").exists());
    assert!(!root.join(".agents/skills/hyalo-madr/SKILL.md").exists());
    let scan = ok(root, &["find", "--glob", ".agents/skills/**/SKILL.md"]);
    assert_eq!(scan["results"].as_array().unwrap().len(), 4);
    let lint = run(
        root,
        &[
            "lint",
            ".agents/skills/hyalo/SKILL.md",
            "--profile",
            "skills",
            "--format",
            "text",
        ],
    );
    assert!(
        lint.status.success(),
        "{}{}",
        String::from_utf8_lossy(&lint.stdout),
        String::from_utf8_lossy(&lint.stderr)
    );
}

#[test]
fn codex_flag_combinations_and_plugin_mode_have_no_duplicate_local_skills() {
    for mask in 0..8 {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let mut args = vec!["init"];
        for (bit, flag) in [(1, "--claude"), (2, "--pi"), (4, "--codex")] {
            if mask & bit != 0 {
                args.push(flag);
            }
        }
        ok(root, &args);
        assert_eq!(
            root.join(".claude/skills/hyalo/SKILL.md").exists(),
            mask & 1 != 0
        );
        assert_eq!(root.join(".pi/extensions/hyalo.ts").exists(), mask & 2 != 0);
        assert_eq!(root.join(".pi/lib/hyalo-api.js").exists(), mask & 2 != 0);
        assert_eq!(root.join(".pi/lib/hyalo-api.d.ts").exists(), mask & 2 != 0);
        assert_eq!(
            root.join(".agents/skills/hyalo/SKILL.md").exists(),
            mask & 4 != 0
        );
    }
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    assert!(!run(root, &["init", "--codex-plugin"]).status.success());
    assert!(!root.join(".hyalo.toml").exists());
    ok(root, &["init", "--codex"]);
    ok(root, &["init", "--codex", "--codex-plugin"]);
    assert!(!root.join(".agents/skills").exists());
    assert!(
        fs::read_to_string(root.join("AGENTS.md"))
            .unwrap()
            .contains("installed Hyalo plugin")
    );
    ok(root, &["init", "--codex", "--codex-plugin"]);
    ok(root, &["init", "--codex"]);
    assert!(root.join(".agents/skills/hyalo/SKILL.md").is_file());
}

#[test]
fn pi_install_is_self_contained_and_deinit_removes_companions() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    ok(root, &["init", "--pi"]);

    let extension = fs::read_to_string(root.join(".pi/extensions/hyalo.ts")).unwrap();
    let runtime = fs::read_to_string(root.join(".pi/lib/hyalo-api.js")).unwrap();
    let declaration = fs::read_to_string(root.join(".pi/lib/hyalo-api.d.ts")).unwrap();
    assert!(extension.contains("../lib/hyalo-api.js"));
    assert!(runtime.contains("createPiTransport"));
    assert!(runtime.contains("Copyright 2017 Lovell Fuller and others."));
    assert!(runtime.contains("SPDX-License-Identifier: Apache-2.0"));
    assert!(runtime.contains("END OF TERMS AND CONDITIONS"));
    assert!(declaration.contains("declare function find"));
    assert!(!root.join(".pi/node_modules").exists());

    ok(root, &["deinit"]);
    assert!(!root.join(".pi/extensions/hyalo.ts").exists());
    assert!(!root.join(".pi/lib/hyalo-api.js").exists());
    assert!(!root.join(".pi/lib/hyalo-api.d.ts").exists());
}

#[test]
fn codex_external_scope_and_nested_queries_use_the_selected_vault() {
    let caller = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    fs::write(caller.path().join("AGENTS.md"), "caller").unwrap();
    ok(
        caller.path(),
        &[
            "init",
            "--codex",
            "--dir",
            external.path().to_str().unwrap(),
        ],
    );
    fs::create_dir(external.path().join("nested")).unwrap();
    fs::write(
        external.path().join("note.md"),
        "---\ntitle: Note\nstatus: planned\n---\n",
    )
    .unwrap();
    let found = ok(
        &external.path().join("nested"),
        &["find", "--property", "status=planned"],
    );
    assert_eq!(found["results"].as_array().unwrap().len(), 1);
    ok(
        caller.path(),
        &["deinit", "--dir", external.path().to_str().unwrap()],
    );
    assert!(!external.path().join("AGENTS.md").exists());
    assert_eq!(
        fs::read_to_string(caller.path().join("AGENTS.md")).unwrap(),
        "caller"
    );
}

#[test]
fn codex_conflicts_and_malformed_markers_fail_before_writes() {
    for content in ["<!-- hyalo:start -->", "<!-- hyalo:end -->"] {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), content).unwrap();
        assert!(!run(tmp.path(), &["init", "--codex"]).status.success());
        assert!(!tmp.path().join(".hyalo.toml").exists());
        assert_eq!(
            fs::read_to_string(tmp.path().join("AGENTS.md")).unwrap(),
            content
        );
    }
    let tmp = TempDir::new().unwrap();
    let skill = tmp.path().join(".agents/skills/hyalo/SKILL.md");
    fs::create_dir_all(skill.parent().unwrap()).unwrap();
    fs::write(&skill, "user owned").unwrap();
    let out = run(tmp.path(), &["init", "--codex"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("conflict"));
    assert!(!tmp.path().join(".hyalo.toml").exists());
    ok(tmp.path(), &["deinit"]);
    assert_eq!(fs::read_to_string(&skill).unwrap(), "user owned");
    assert!(
        !run(tmp.path(), &["init", "--codex", "--profile", "unknown"])
            .status
            .success()
    );
    assert!(!tmp.path().join(".hyalo.toml").exists());
}

#[test]
fn codex_override_warning_is_part_of_the_json_report() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("AGENTS.override.md"), "override").unwrap();
    let report = ok(tmp.path(), &["init", "--codex"]);
    assert!(
        report["results"]["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["action"] == "warning" && a["target"] == "AGENTS.override.md")
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join("AGENTS.override.md")).unwrap(),
        "override"
    );
}

#[cfg(unix)]
#[test]
fn codex_updates_preserve_existing_permission_bits() {
    use std::os::unix::fs::PermissionsExt;

    for mode in [0o640, 0o444, 0o4755, 0o2755] {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        ok(root, &["init", "--codex"]);
        let paths = ["AGENTS.md", ".agents/skills/hyalo/SKILL.md"];
        for relative in paths {
            let path = root.join(relative);
            let stale = if relative == "AGENTS.md" {
                "<!-- hyalo:start -->\nold\n<!-- hyalo:end -->\n"
            } else {
                "<!-- hyalo:managed -->\nold\n"
            };
            fs::write(&path, stale).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
                mode,
                "fixture permissions"
            );
        }
        ok(root, &["init", "--codex"]);
        for relative in paths {
            let actual = fs::metadata(root.join(relative))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777;
            assert_eq!(actual, mode, "permissions changed for {relative}");
        }
    }
}

#[test]
fn codex_guidance_uses_the_project_root_or_configured_vault() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("notes/nested")).unwrap();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join(".hyalo.toml"), "dir = \"notes\"\n").unwrap();
    fs::write(
        root.join("notes/note.md"),
        "---\ntitle: Note\nstatus: planned\n---\n",
    )
    .unwrap();
    ok(root, &["init", "--codex"]);
    for cwd in [root.to_path_buf(), root.join("notes/nested")] {
        let found = ok(&cwd, &["find", "--property", "status=planned"]);
        assert_eq!(found["results"].as_array().unwrap().len(), 1);
    }
    let outside = ok(&root.join("src"), &["find", "--property", "status=planned"]);
    assert!(outside["results"].as_array().unwrap().is_empty());
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(agents.contains(
        "Run commands from the project root containing .hyalo.toml or inside the configured vault."
    ));
    assert!(!agents.contains("this project or its descendants"));
}

#[cfg(unix)]
#[test]
fn codex_symlink_destinations_never_modify_the_referent() {
    use std::os::unix::fs::symlink;
    for relative in ["AGENTS.md", ".agents", ".hyalo.toml"] {
        let tmp = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let target = if relative == ".agents" {
            outside.path().to_path_buf()
        } else {
            outside.path().join("user.md")
        };
        if relative != ".agents" {
            fs::write(&target, "user").unwrap();
        }
        symlink(&target, tmp.path().join(relative)).unwrap();
        assert!(!run(tmp.path(), &["init", "--codex"]).status.success());
        if relative == ".agents" {
            assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
        } else {
            assert_eq!(fs::read_to_string(&target).unwrap(), "user");
        }
    }
}
