use super::common::hyalo_no_hints;

#[test]
fn completions_all_shells_produce_output() {
    for shell in ["bash", "zsh", "fish", "elvish", "powershell"] {
        let output = hyalo_no_hints()
            .args(["completions", shell])
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{shell}: stderr: {stderr}");

        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains("hyalo"),
            "{shell} completions should reference the binary name"
        );
    }
}

#[test]
fn completion_singular_alias_still_works() {
    // `completion` was the original name; it remains a visible alias so
    // existing scripts and docs don't break.
    let output = hyalo_no_hints()
        .args(["completion", "bash"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "alias failed: stderr: {stderr}");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("hyalo"),
        "alias output should reference the binary name"
    );
}

#[test]
fn completions_missing_shell_fails() {
    let output = hyalo_no_hints().args(["completions"]).output().unwrap();

    assert!(
        !output.status.success(),
        "completions without a shell argument should fail"
    );
}

#[test]
fn completions_invalid_shell_fails() {
    let output = hyalo_no_hints()
        .args(["completions", "invalid-shell"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "completions with an invalid shell should fail"
    );
}

#[test]
fn completions_listed_in_help() {
    let output = hyalo_no_hints().arg("-h").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("completions"),
        "completions command should appear in help output"
    );
}
