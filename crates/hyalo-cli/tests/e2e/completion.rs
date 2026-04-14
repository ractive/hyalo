use super::common::hyalo_no_hints;

#[test]
fn completion_all_shells_produce_output() {
    for shell in ["bash", "zsh", "fish", "elvish", "powershell"] {
        let output = hyalo_no_hints()
            .args(["completion", shell])
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
fn completion_missing_shell_fails() {
    let output = hyalo_no_hints().args(["completion"]).output().unwrap();

    assert!(
        !output.status.success(),
        "completion without a shell argument should fail"
    );
}

#[test]
fn completion_invalid_shell_fails() {
    let output = hyalo_no_hints()
        .args(["completion", "invalid-shell"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "completion with an invalid shell should fail"
    );
}

#[test]
fn completion_listed_in_help() {
    let output = hyalo_no_hints().arg("-h").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("completion"),
        "completion command should appear in help output"
    );
}
