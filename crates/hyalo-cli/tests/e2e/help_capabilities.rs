//! Advertised shared flags must agree with the execution capability contract.

use super::common::hyalo;
use tempfile::TempDir;

#[test]
fn help_and_descriptors_match_runtime_capabilities() {
    let tmp = TempDir::new().unwrap();
    // Required values are parsed, never executed: this covers mutations without
    // writing anything. Optional-action groups cover their default actions too.
    let invocations: &[&[&str]] = &[
        &["find"],
        &["read"],
        &["properties"],
        &["tags"],
        &["summary"],
        &["backlinks"],
        &["mv"],
        &["set"],
        &["remove"],
        &["append", "--property", "key=value"],
        &["init"],
        &["deinit"],
        &["create-index"],
        &["drop-index"],
        &["views"],
        &["links"],
        &["lint"],
        &["lint-rules"],
        &["types"],
        &["new", "--type", "note", "--file", "new.md"],
        &["config"],
        &["completions", "bash"],
        &["properties", "summary"],
        &["properties", "rename", "--from", "a", "--to", "b"],
        &["tags", "summary"],
        &["tags", "rename", "--from", "a", "--to", "b"],
        &["task", "read"],
        &["task", "toggle"],
        &["task", "set", "--status", "x"],
        &["views", "list"],
        &["views", "set", "saved"],
        &["views", "remove", "saved"],
        &["views", "run", "saved"],
        &["links", "fix"],
        &["links", "auto"],
        &["lint-rules", "list"],
        &["lint-rules", "show", "MD013"],
        &["lint-rules", "set", "MD013"],
        &["lint-rules", "remove", "MD013"],
        &["types", "list"],
        &["types", "show", "note"],
        &["types", "set", "note"],
        &["types", "remove", "note"],
        &["okf", "index"],
        &["okf", "log", "--message", "test"],
        &["madr", "toc"],
        &["changelog", "release", "1.2.3"],
        &[
            "changelog",
            "add",
            "--category",
            "Added",
            "--message",
            "test",
        ],
    ];
    for args in invocations {
        let argv: Vec<String> = std::iter::once("hyalo")
            .chain(args.iter().copied())
            .map(str::to_owned)
            .collect();
        let descriptor = hyalo_cli::describe_invocation(&argv).unwrap();
        let capabilities = &descriptor["capabilities"];
        let options = descriptor["options"].as_array().unwrap();
        let has = |flag: &str| options.iter().any(|option| option["long"] == flag);
        assert_eq!(
            has("count"),
            capabilities["count"].as_bool().unwrap(),
            "{args:?}"
        );
        assert_eq!(
            has("index-file"),
            capabilities["index"] != "none",
            "{args:?}"
        );
        if capabilities["targets"] == "single" {
            assert!(!has("glob"), "{args:?}");
        }
        let path: Vec<_> = descriptor["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|word| word.as_str().unwrap())
            .collect();
        let output = hyalo()
            .current_dir(tmp.path())
            .args(&path)
            .arg("--help")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{path:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let help = String::from_utf8(output.stdout).unwrap();
        for (flag, supported) in [
            ("--count", has("count")),
            ("--index-file", has("index-file")),
            ("--site-prefix", has("site-prefix")),
        ] {
            let advertised = help.lines().any(|line| {
                line.trim_start()
                    .strip_prefix(flag)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
            });
            assert_eq!(advertised, supported, "{path:?}: {flag}");
        }
        if !capabilities["github"].as_bool().unwrap() {
            assert!(!help.contains("- github:"), "{path:?}");
        }
        let short = hyalo()
            .current_dir(tmp.path())
            .args(&path)
            .arg("-h")
            .output()
            .unwrap();
        assert!(short.status.success());
        let short = String::from_utf8(short.stdout).unwrap();
        let pointer = short
            .lines()
            .find(|line| line.starts_with("Global: "))
            .unwrap();
        assert_eq!(pointer.contains("--count"), has("count"), "{path:?}");
        assert_eq!(
            pointer.contains("--site-prefix"),
            has("site-prefix"),
            "{path:?}"
        );
    }
}

#[test]
fn site_prefix_remains_discoverable_for_snapshot_validation() {
    // Plain readers do not resolve links, but a prefix mismatch still makes
    // the shared snapshot loader fall back to disk.
    for (args, expected) in [
        (vec!["read"], true),
        (vec!["task", "read"], true),
        (vec!["tags"], true),
        (vec!["tags", "summary"], true),
        (vec!["properties", "summary"], true),
        (vec!["drop-index"], false),
        (vec!["backlinks"], true),
        (vec!["find"], true),
        (vec!["summary"], true),
        (vec!["create-index"], true),
        (vec!["task", "toggle"], true),
        (vec!["config"], true),
    ] {
        let argv: Vec<String> = std::iter::once("hyalo")
            .chain(args.iter().copied())
            .map(str::to_owned)
            .collect();
        let descriptor = hyalo_cli::describe_invocation(&argv).unwrap();
        let advertised = descriptor["options"]
            .as_array()
            .unwrap()
            .iter()
            .any(|option| option["long"] == "site-prefix");
        assert_eq!(advertised, expected, "{args:?}");
    }

    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("note.md"),
        "---\ntags: [example]\n---\n# Note\n",
    )
    .unwrap();
    let created = hyalo()
        .current_dir(tmp.path())
        .args(["create-index", "--site-prefix", "custom-prefix"])
        .output()
        .unwrap();
    assert!(created.status.success());
    for args in [&["tags"][..], &["properties"], &["read", "note.md"]] {
        let mismatched = hyalo()
            .current_dir(tmp.path())
            .args(args)
            .arg("--index")
            .output()
            .unwrap();
        assert!(mismatched.status.success());
        assert!(
            String::from_utf8_lossy(&mismatched.stderr).contains("index does not match this run")
        );
        let matched = hyalo()
            .current_dir(tmp.path())
            .args(args)
            .args(["--index", "--site-prefix", "custom-prefix"])
            .output()
            .unwrap();
        assert!(matched.status.success());
        assert!(
            !String::from_utf8_lossy(&matched.stderr).contains("index does not match this run")
        );
        let help = hyalo()
            .current_dir(tmp.path())
            .args(args)
            .arg("--help")
            .output()
            .unwrap();
        assert!(help.status.success());
        assert!(String::from_utf8_lossy(&help.stdout).contains("Match the site prefix stored"));
    }
}

#[test]
fn unsupported_selectors_are_hidden_but_still_diagnosed() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("note.md"), "# Note\n- [ ] Task\n").unwrap();
    for path in [&["read"][..], &["show"], &["backlinks"], &["task", "read"]] {
        let help = hyalo()
            .current_dir(tmp.path())
            .args(path)
            .arg("-h")
            .output()
            .unwrap();
        assert!(help.status.success());
        assert!(!String::from_utf8_lossy(&help.stdout).contains("--glob"));
        let rejected = hyalo()
            .current_dir(tmp.path())
            .args(path)
            .args(["--glob", "*.md"])
            .output()
            .unwrap();
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("not supported"));
    }
}

#[test]
fn views_set_refuses_unsavable_files_from_before_writing() {
    let tmp = TempDir::new().unwrap();
    let config = tmp.path().join(".hyalo.toml");
    let original = "dir = '.'\n";
    std::fs::write(&config, original).unwrap();
    let output = hyalo()
        .current_dir(tmp.path())
        .args([
            "views",
            "set",
            "saved",
            "--tag",
            "test",
            "--files-from",
            "-",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be saved in a view"));
    assert_eq!(std::fs::read_to_string(config).unwrap(), original);
}
