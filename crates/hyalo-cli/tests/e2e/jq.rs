use super::common::{hyalo_no_hints, write_md, write_tagged};
use tempfile::TempDir;

#[cfg(windows)]
#[test]
fn jq_worker_windows_npm_forced_parent_termination_closes_worker_job() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
    };
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), ".hyalo.toml", "dir = \".\"\n");
    write_md(tmp.path(), "a.md", "---\ntags: [before]\n---\nBody\n");
    let mut parent = Command::new(env!("CARGO_BIN_EXE_hyalo"))
        .current_dir(tmp.path())
        .args([
            "--no-hints",
            "append",
            "a.md",
            "--property",
            "tags=after",
            "--jq",
            "def f: f; f",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut worker: windows_sys::Win32::Foundation::HANDLE = std::ptr::null_mut();
    while worker.is_null() && Instant::now() < deadline {
        if std::fs::read_to_string(tmp.path().join("a.md"))
            .unwrap()
            .contains("after")
        {
            // SAFETY: own snapshot handle and initialize the required structure
            // size; only open a process whose parent is our disposable CLI.
            unsafe {
                let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
                if snapshot != INVALID_HANDLE_VALUE {
                    let mut entry = PROCESSENTRY32W {
                        dwSize: u32::try_from(std::mem::size_of::<PROCESSENTRY32W>()).unwrap(),
                        ..Default::default()
                    };
                    let mut present = Process32FirstW(snapshot, &raw mut entry);
                    while present != 0 {
                        if entry.th32ParentProcessID == parent.id() {
                            worker = OpenProcess(
                                PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
                                0,
                                entry.th32ProcessID,
                            );
                            break;
                        }
                        present = Process32NextW(snapshot, &raw mut entry);
                    }
                    CloseHandle(snapshot);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = parent.kill(); // Node child.kill() also forcibly terminates on Windows.
    let _ = parent.wait();
    assert!(
        !worker.is_null(),
        "did not observe actual evaluation worker"
    );
    // SAFETY: live owned process handle. Clean up even if the assertion fails.
    let status = unsafe {
        let status = WaitForSingleObject(worker, 2000);
        if status != WAIT_OBJECT_0 {
            TerminateProcess(worker, 1);
        }
        CloseHandle(worker);
        status
    };
    assert_eq!(status, WAIT_OBJECT_0, "parent termination left jq running");
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn setup_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "b.md", &["rust", "iteration"]);
    write_md(tmp.path(), "c.md", "No frontmatter.\n");
    tmp
}

#[test]
fn jq_worker_abort_runtime_timeout_and_oversize_retain_append_effects() {
    for (filter, diagnostic) in [
        ("def f: [f]; f", "jq worker"),
        ("error(\"finite failure\")", "runtime error"),
        ("def f: f; f", "time limit"),
        ("\"x\" * 11000000", "10 MiB"),
    ] {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), ".hyalo.toml", "dir = \".\"\n");
        write_md(tmp.path(), "a.md", "---\ntags: [before]\n---\nBody\n");
        hyalo_no_hints()
            .current_dir(tmp.path())
            .arg("create-index")
            .assert()
            .success();
        let output = hyalo_no_hints()
            .current_dir(tmp.path())
            .args([
                "append",
                "a.md",
                "--property",
                "tags=after",
                "--index",
                "--jq",
                filter,
            ])
            .timeout(std::time::Duration::from_secs(12))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{filter}: {output:?}");
        assert!(output.stdout.is_empty());
        let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        let text = String::from_utf8_lossy(&output.stderr);
        assert!(
            text.contains(diagnostic)
                || (diagnostic == "time limit" && text.contains("resource budget")),
            "{text}"
        );
        assert_eq!(error["category"], "output_failure");
        assert_eq!(error["effects"]["paths"][0]["state"], "committed");
        assert_eq!(error["effects"]["index"], "updated");
        assert!(
            std::fs::read_to_string(tmp.path().join("a.md"))
                .unwrap()
                .contains("after")
        );
        hyalo_no_hints()
            .current_dir(tmp.path())
            .args(["find", "--index", "--tag", "after", "--count"])
            .assert()
            .success()
            .stdout("1\n");
    }
}

#[cfg(unix)]
#[test]
fn jq_worker_sigint_and_npm_sigterm_cancellation_preserve_append_effects() {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    for signal in [libc::SIGINT, libc::SIGTERM] {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), ".hyalo.toml", "dir = \".\"\n");
        write_md(tmp.path(), "a.md", "---\ntags: [before]\n---\nBody\n");
        let mut child = Command::new(env!("CARGO_BIN_EXE_hyalo"))
            .current_dir(tmp.path())
            .args([
                "--no-hints",
                "append",
                "a.md",
                "--property",
                "tags=after",
                "--jq",
                "def f: f; f",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !std::fs::read_to_string(tmp.path().join("a.md"))
            .unwrap()
            .contains("after")
        {
            if Instant::now() >= deadline || child.try_wait().unwrap().is_some() {
                let _ = child.kill();
                let _ = child.wait();
                panic!("append did not reach the commit");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(100));
        // SAFETY: signal only this live disposable CLI parent.
        assert_eq!(
            unsafe { libc::kill(i32::try_from(child.id()).unwrap(), signal) },
            0
        );
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("cancellation did not finish");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(status.code(), Some(2));
        let mut stderr = Vec::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_end(&mut stderr)
            .unwrap();
        let error: serde_json::Value = serde_json::from_slice(&stderr).unwrap();
        assert!(String::from_utf8_lossy(&stderr).contains("cancelled"));
        assert_eq!(error["effects"]["paths"][0]["state"], "committed");
    }
}

// ---------------------------------------------------------------------------
// Basic filter application
// ---------------------------------------------------------------------------

#[test]
fn jq_extracts_total_from_tags_summary() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".total"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn jq_maps_tag_names_to_array() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "[.results[].name] | sort | join(\", \")"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "cli, iteration, rust");
}

#[test]
fn jq_works_on_properties_command() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Hello\nstatus: draft\n---\n# Body\n",
    );

    // `properties` returns an array of {count, name, type} objects.
    // Extract just the property names and sort them.
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "[.results[].name] | sort | join(\", \")"])
        .args(["properties", "summary"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "status, title");
}

#[test]
fn jq_works_on_find_property_filter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\nstatus: draft\n---\n");
    write_md(tmp.path(), "b.md", "---\nstatus: done\n---\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results[].file"])
        .args(["find", "--property", "status=draft"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().contains("a.md"), "got: {stdout}");
    assert!(!stdout.contains("b.md"));
}

// ---------------------------------------------------------------------------
// --jq on find --fields links and find --fields sections
// ---------------------------------------------------------------------------

#[test]
fn jq_works_on_find_links() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "source.md",
        "---\ntitle: Source\n---\nSee [[target]] and [[other]].\n",
    );
    write_md(tmp.path(), "target.md", "# Target\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results[0].links | length"])
        .args(["find", "--file", "source.md", "--fields", "links"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "2", "expected 2 links, got: {stdout}");
}

#[test]
fn jq_works_on_find_sections() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "doc.md",
        "# Introduction\n\nSome text.\n\n## Details\n\nMore text.\n\n### Sub\n\nDeep.\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results[0].sections | length"])
        .args(["find", "--file", "doc.md", "--fields", "sections"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3", "expected 3 sections, got: {stdout}");
}

// ---------------------------------------------------------------------------
// --jq conflicts with --format text
// ---------------------------------------------------------------------------

#[test]
fn jq_with_format_text_errors() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["--jq", ".total"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    // iter-181 task 2: --jq + --format text is a user error → exit 1, not 2.
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit code 1 (user error)"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--jq cannot be combined with --format text"),
        "expected conflict error, got: {stderr}"
    );
}

#[test]
fn jq_with_format_json_works() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["--jq", ".total"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn jq_invalid_filter_exits_nonzero() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "this is not %%% valid jq"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should mention "jq" and give some indication of a syntax/filter problem
    assert!(
        stderr.contains("jq"),
        "expected 'jq' in error message, got: {stderr}"
    );
    assert!(
        stderr.contains("syntax") || stderr.contains("filter") || stderr.contains("parse"),
        "expected description of what went wrong, got: {stderr}"
    );
}

#[test]
fn jq_runtime_error_exits_nonzero() {
    let tmp = setup_vault();

    // Explicit jq runtime error raised via error("deliberate") to verify non-zero exit on runtime failure
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "error(\"deliberate\")"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should mention "jq" and include the error value "deliberate"
    assert!(
        stderr.contains("jq"),
        "expected 'jq' in error message, got: {stderr}"
    );
    assert!(
        stderr.contains("deliberate"),
        "expected the raised error value 'deliberate' in output, got: {stderr}"
    );
}

#[test]
fn jq_filter_error_is_json_when_format_json() {
    let tmp = setup_vault();

    // With --format json (the default), jq errors should be emitted as structured JSON
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["--jq", "error(\"structured-error\")"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let parsed: serde_json::Value = serde_json::from_str(&stderr)
        .unwrap_or_else(|_| panic!("expected JSON on stderr for --format json, got: {stderr}"));
    assert!(
        parsed.get("error").is_some(),
        "expected 'error' field in JSON error, got: {stderr}"
    );
    assert!(
        parsed.get("cause").is_some(),
        "expected 'cause' field in JSON error, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// --jq on mutation commands (set, remove, append)
// ---------------------------------------------------------------------------

#[test]
fn jq_works_on_set_command() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Note\n---\n# Body\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results.modified | length"])
        .args(["set", "--property", "status=done", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "1",
        "expected 1 modified file, got: {stdout}"
    );
}

#[test]
fn jq_works_on_remove_command() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Note\nstatus: draft\n---\n# Body\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results.modified | length"])
        .args(["remove", "--property", "status", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "1",
        "expected 1 modified file, got: {stdout}"
    );
}

#[test]
fn jq_works_on_append_command() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Note\naliases:\n  - old\n---\n# Body\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results.modified | length"])
        .args(["append", "--property", "aliases=new", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "1",
        "expected 1 modified file, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Filter producing multiple output values
// ---------------------------------------------------------------------------

#[test]
fn jq_multiple_outputs_joined_by_newline() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results[].name"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    // 3 tags, each on its own line
    assert_eq!(lines.len(), 3, "expected 3 lines, got: {stdout}");
    assert!(lines.contains(&"rust"));
    assert!(lines.contains(&"cli"));
    assert!(lines.contains(&"iteration"));
}

// ---------------------------------------------------------------------------
// iter-181 task 2: exit-code contract — user errors exit 1, not 2
// ---------------------------------------------------------------------------

#[test]
fn jq_with_format_text_exits_one() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".total"])
        .args(["--format", "text"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    // 1 = user error (help defines 2 = internal error).
    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--jq"),
        "expected a --jq/--format conflict message, got: {stderr}"
    );
}

#[test]
fn count_with_jq_exits_one() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".total"])
        .arg("--count")
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// F3-1 (deep-analysis-3-2026-08-23.md): --jq resource limits
// ---------------------------------------------------------------------------
//
// `--jq` runs user-supplied input with no interpreter-level step/fuel hook
// available (jaq-core 3.0.0's public API has none), so both repros below are
// bounded by the parent's deadline on a killable worker process.
// `.timeout()` here is a CI safety net: if the fix ever regresses, these
// tests fail cleanly with a timeout error instead of hanging the whole
// suite (and, for the second test, exhausting CI memory).

#[test]
fn jq_infinite_recursion_errors_within_time_limit_instead_of_hanging() {
    // Before the fix: `def f: f; f` never emits a value, so the byte-count
    // output cap (which only fires once something is produced) never
    // triggers — the process spun forever.
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "def f: f; f"])
        .arg("find")
        .timeout(std::time::Duration::from_secs(10))
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "an infinitely-recursing filter with no output must error, not hang or succeed"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("time limit") || stderr.contains("resource budget"),
        "expected a deadline or OS resource-limit error, got: {stderr}"
    );
}

#[test]
fn jq_unbounded_intermediate_array_errors_within_time_limit_instead_of_oom() {
    // Before the fix: `[range(3e8)]` built its entire 300M-element
    // intermediate array — inside a single, uninterruptible jaq evaluation
    // step — before ever yielding the one `length` value to Rust: ~8.7s and
    // ~4.8 GB peak RSS to print a single number. This test genuinely costs a
    // few seconds and a few hundred MB; that is an acceptable, *bounded*
    // cost for a regression test on what was rated the batch's highest-
    // severity finding, and is a large improvement over the unmitigated
    // behavior it guards against.
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "[range(3e8)] | length"])
        .arg("find")
        .timeout(std::time::Duration::from_secs(20))
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "an unbounded intermediate array must error, not hang, OOM, or succeed"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("time limit") || stderr.contains("resource budget"),
        "expected a deadline or OS resource-limit error, got: {stderr}"
    );
}

#[test]
fn jq_legitimate_heavy_filter_over_many_files_still_works() {
    // The deadline must not be so tight that ordinary, non-pathological use
    // over a large-ish result set is at risk of a false-positive timeout.
    let tmp = TempDir::new().unwrap();
    for i in 0..500 {
        write_tagged(tmp.path(), &format!("note-{i:04}.md"), &["rust", "cli"]);
    }

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "[.results[] | select(.tags | length > 0)] | length"])
        .args(["find", "--fields", "tags"])
        .timeout(std::time::Duration::from_secs(10))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "500");
}
