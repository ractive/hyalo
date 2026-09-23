use super::common::hyalo_no_hints;
use serde_json::Value;
use std::{fs, path::Path};
use tempfile::TempDir;

const MODES: [(&str, &str); 3] = [
    ("--claude", ".claude/skills/hyalo-tidy"),
    ("--codex", ".agents/skills/hyalo-tidy"),
    ("--pi", ".pi/skills/hyalo-tidy"),
];
const SCRIPT: &str = include_str!("../../templates/jev/jev.mjs");
const REFERENCE: &str = include_str!("../../templates/jev/jev.md");
fn init(root: &Path, mode: &str) -> std::process::Output {
    hyalo_no_hints()
        .current_dir(root)
        .args(["init", mode])
        .output()
        .unwrap()
}
fn success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn jev_resources_install_repeat_upgrade_and_remove_for_every_host() {
    for (mode, skill) in MODES {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        success(&init(root, mode));
        let script = root.join(skill).join("scripts/jev.mjs");
        let reference = root.join(skill).join("references/jev.md");
        assert_eq!(fs::read_to_string(&script).unwrap(), SCRIPT);
        assert_eq!(fs::read_to_string(&reference).unwrap(), REFERENCE);
        success(&init(root, mode));

        // Simulate an older receipt-owned helper, then upgrade it. Only the
        // exact bytes in the old receipt qualify for replacement.
        let receipt_path = if mode == "--pi" {
            root.join(".pi/.hyalo-manifest.json")
        } else {
            root.join(skill).join(".hyalo-jev-assets.json")
        };
        let mut receipt: Value = serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
        let key = if mode == "--pi" {
            ".pi/skills/hyalo-tidy/scripts/jev.mjs"
        } else {
            "scripts/jev.mjs"
        };
        receipt["artifacts"][key] = Value::String("// older helper\n".into());
        fs::write(&script, "// older helper\n").unwrap();
        fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        success(&init(root, mode));
        assert_eq!(fs::read_to_string(&script).unwrap(), SCRIPT);
        success(
            &hyalo_no_hints()
                .current_dir(root)
                .arg("deinit")
                .output()
                .unwrap(),
        );
        assert!(!script.exists());
        assert!(!reference.exists());
        assert!(!receipt_path.exists());
        assert!(!root.join(skill).exists());
    }
}

#[test]
fn jev_modified_assets_are_preserved_on_update_and_removal() {
    for (mode, skill) in MODES {
        for name in ["scripts/jev.mjs", "references/jev.md"] {
            let temp = TempDir::new().unwrap();
            let root = temp.path();
            success(&init(root, mode));
            let asset = root.join(skill).join(name);
            let mut edited = fs::read(&asset).unwrap();
            edited.extend_from_slice(b"\nuser edit\n");
            fs::write(&asset, &edited).unwrap();
            let config = fs::read(root.join(".hyalo.toml")).unwrap();
            assert!(!init(root, mode).status.success());
            assert_eq!(fs::read(root.join(".hyalo.toml")).unwrap(), config);
            success(
                &hyalo_no_hints()
                    .current_dir(root)
                    .arg("deinit")
                    .output()
                    .unwrap(),
            );
            assert_eq!(fs::read(&asset).unwrap(), edited);
        }
    }
}

#[test]
fn jev_unowned_assets_block_init_before_config_writes() {
    for (mode, skill) in MODES {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let script = root.join(skill).join("scripts/jev.mjs");
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(&script, SCRIPT).unwrap();
        assert!(!init(root, mode).status.success());
        assert!(!root.join(".hyalo.toml").exists());
        assert_eq!(fs::read_to_string(&script).unwrap(), SCRIPT);
    }
}

#[cfg(unix)]
#[test]
fn jev_symlink_resources_refuse_init_and_deinit_before_any_writes() {
    use std::os::unix::fs::symlink;
    for (mode, skill) in MODES {
        for leaf in [true, false] {
            let temp = TempDir::new().unwrap();
            let root = temp.path();
            success(&init(root, mode));
            let scripts = root.join(skill).join("scripts");
            let outside = TempDir::new().unwrap();
            fs::write(outside.path().join("jev.mjs"), "user-owned").unwrap();
            if leaf {
                fs::remove_file(scripts.join("jev.mjs")).unwrap();
                symlink(outside.path().join("jev.mjs"), scripts.join("jev.mjs")).unwrap();
            } else {
                fs::remove_dir_all(&scripts).unwrap();
                symlink(outside.path(), &scripts).unwrap();
            }
            let config = fs::read(root.join(".hyalo.toml")).unwrap();
            assert!(!init(root, mode).status.success());
            assert!(
                !hyalo_no_hints()
                    .current_dir(root)
                    .arg("deinit")
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
            assert_eq!(fs::read(root.join(".hyalo.toml")).unwrap(), config);
            assert_eq!(
                fs::read_to_string(outside.path().join("jev.mjs")).unwrap(),
                "user-owned"
            );
        }
    }
}

#[test]
fn jev_legacy_install_without_optional_resources_can_be_upgraded() {
    for (mode, skill) in MODES {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        success(&init(root, mode));
        fs::remove_dir_all(root.join(skill).join("scripts")).unwrap();
        fs::remove_dir_all(root.join(skill).join("references")).unwrap();
        if mode == "--pi" {
            let path = root.join(".pi/.hyalo-manifest.json");
            let mut receipt: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            let artifacts = receipt["artifacts"].as_object_mut().unwrap();
            artifacts.remove(".pi/skills/hyalo-tidy/scripts/jev.mjs");
            artifacts.remove(".pi/skills/hyalo-tidy/references/jev.md");
            fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        } else {
            fs::remove_file(root.join(skill).join(".hyalo-jev-assets.json")).unwrap();
        }
        success(&init(root, mode));
        assert_eq!(
            fs::read_to_string(root.join(skill).join("scripts/jev.mjs")).unwrap(),
            SCRIPT
        );
    }
}

#[test]
fn installed_tidy_diagnostics_work_without_saved_views() {
    for (mode, skill) in [MODES[0], MODES[2]] {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(root.join(".hyalo.toml"), "dir = \".\"\n").unwrap();
        fs::write(
            root.join("note.md"),
            "---\ntitle: Note\nstatus: in-progress\ndate: 2026-09-23\nbranch: iter-300/example\n---\n# Note\n\n- [ ] Task\n",
        )
        .unwrap();
        success(&init(root, mode));
        let config = fs::read(root.join(".hyalo.toml")).unwrap();
        let installed = fs::read_to_string(root.join(skill).join("SKILL.md")).unwrap();
        let phase = installed
            .split("## Phase 3")
            .nth(1)
            .unwrap()
            .split("## Phase 4")
            .next()
            .unwrap();
        let mut checked = 0;
        for line in phase.lines().filter(|line| line.starts_with("hyalo find ")) {
            // Execute the shipped one-line recipes without invoking a shell.
            // Their selectors contain no whitespace; jq is one quoted argument.
            let (selectors, query) = line.split_once(" --jq '").unwrap();
            let query = query.strip_suffix('\'').unwrap();
            let selectors = selectors
                .split_whitespace()
                .skip(1)
                .map(|argument| argument.trim_matches('\''));
            let output = hyalo_no_hints()
                .current_dir(root)
                .args(selectors)
                .args(["--jq", query])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode}: installed recipe failed: {line}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            if line.contains("--property status=in-progress") {
                let results: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(results.as_array().unwrap().len(), 1, "{mode}: {line}");
                assert_eq!(results[0]["file"], "note.md", "{mode}: {line}");
                assert_eq!(results[0]["date"], "2026-09-23", "{mode}: {line}");
                if query.contains("branch:") {
                    assert_eq!(results[0]["branch"], "iter-300/example", "{mode}: {line}");
                }
            }
            checked += 1;
        }
        assert_eq!(checked, 9, "must execute every Phase 3 find recipe");
        assert_eq!(fs::read(root.join(".hyalo.toml")).unwrap(), config);
        assert!(!root.join(".hyalo-index").exists());
    }
}
