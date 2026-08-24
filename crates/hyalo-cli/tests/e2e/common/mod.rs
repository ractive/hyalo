use assert_cmd::Command;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;

/// The standard `{"results": ..., "hints": [...], "total": ...}` envelope
/// every JSON command output is wrapped in (`output.rs::build_envelope_value`).
///
/// T-3 (iter-224): e2e assertions historically parsed into a raw
/// `serde_json::Value` and indexed it by string key
/// (`json["results"]["links"]["total"]`), so an output-shape regression
/// surfaced as a generic `expect`/index panic rather than tying the test to
/// the actual typed output structs (DEC-025's `crates/hyalo-core/src/types.rs`
/// family, plus per-command structs like `lint`'s `LintOutput`). Deserializing
/// into `Envelope<T>` for a real `T` makes a field rename in production a
/// compile error in the converted suites instead of a silent `Value::Null`
/// a stringly-keyed lookup would tolerate.
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct Envelope<T> {
    pub results: T,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub hints: Vec<serde_json::Value>,
}

/// Parse `stdout` as the standard envelope and return its typed `results`.
///
/// Panics with the raw JSON on any parse failure, matching the existing
/// `unwrap_or_else`-with-context convention used throughout the e2e suite for
/// untyped `serde_json::Value` parses — a shape mismatch (or a genuine CLI
/// error whose output isn't the envelope at all) fails loudly with the
/// offending bytes attached rather than an opaque serde error.
#[allow(dead_code)]
pub fn typed_results<T: DeserializeOwned>(stdout: &[u8]) -> T {
    let envelope: Envelope<T> = serde_json::from_slice(stdout).unwrap_or_else(|e| {
        panic!(
            "failed to deserialize typed results: {e}\nstdout: {}",
            String::from_utf8_lossy(stdout)
        )
    });
    envelope.results
}

/// Strip the leading newline from a raw string so the content aligns at column 0.
macro_rules! md {
    ($s:expr) => {
        $s.strip_prefix('\n').unwrap_or($s)
    };
}
#[allow(unused_imports)]
pub(crate) use md;

/// Returns a `Command` pre-configured to run the `hyalo` binary built by Cargo.
#[allow(dead_code)]
pub fn hyalo() -> Command {
    Command::cargo_bin("hyalo").unwrap()
}

/// Returns a `Command` pre-configured to run `hyalo` with `--no-hints`.
///
/// Use this in tests that verify plain (non-wrapped) JSON output and do not
/// test hint behaviour. Hints are on by default in the built-in config, so
/// without `--no-hints` the output would be wrapped in a hints envelope.
#[allow(dead_code)]
pub fn hyalo_no_hints() -> Command {
    let mut cmd = Command::cargo_bin("hyalo").unwrap();
    cmd.arg("--no-hints");
    cmd
}

/// Writes a file at `relative_path` inside `dir`, creating parent directories as needed.
pub fn write_md(dir: &Path, relative_path: &str, content: &str) {
    let full = dir.join(relative_path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(full, content).unwrap();
}

/// Write a markdown file with YAML frontmatter containing the given tags.
/// Used by tag-related tests and any future tests that need pre-tagged files.
#[allow(dead_code)]
pub fn write_tagged(dir: &Path, name: &str, tags: &[&str]) {
    let tags_yaml = if tags.is_empty() {
        "tags: []\n".to_owned()
    } else {
        let items = tags.iter().fold(String::new(), |mut s, t| {
            use std::fmt::Write as _;
            let _ = writeln!(s, "  - {t}");
            s
        });
        format!("tags:\n{items}")
    };
    write_md(
        dir,
        name,
        &format!("---\ntitle: {name}\n{tags_yaml}---\n# Body\n"),
    );
}

/// Returns a sample markdown document with YAML frontmatter containing various property types.
#[allow(dead_code)]
pub fn sample_frontmatter() -> &'static str {
    md!(r#"
---
title: My Note
priority: 3
draft: true
created: "2026-03-20"
updated: "2026-03-20T14:30:00"
tags:
  - rust
  - cli
---
# Body

Some content here.
"#)
}

/// Split a hint's `cmd` string into argv, undoing the single-quote escaping
/// `hints::shell_quote` applies (`'...'` with embedded `'` escaped as `'\''`).
///
/// A plain `str::split_whitespace()` is *not* sufficient here: on Windows a
/// `--dir` value is a path containing `\`, which forces `shell_quote` to wrap
/// it in single quotes, and any embedded whitespace inside the quoted token
/// (e.g. under `...\Local Settings\Temp\...`) must not become a token break.
/// Since `shell_quote` never emits double quotes or bare unescaped spaces
/// inside a token, this only needs to understand its own single-quote form.
#[allow(dead_code)]
pub fn shell_split(cmd: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut chars = cmd.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let mut token = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                break;
            }
            if c == '\'' {
                chars.next(); // opening quote
                loop {
                    match chars.next() {
                        Some('\'') => {
                            // Either the closing quote, or the start of an
                            // escaped-quote sequence: close(') + \' + reopen(').
                            // That's 4 chars total: `'` `\` `'` `'`; we've
                            // already consumed the first `'` here.
                            let mut lookahead = chars.clone();
                            if lookahead.next() == Some('\\') && lookahead.next() == Some('\'') {
                                chars.next(); // consume '\\'
                                chars.next(); // consume the escaped '
                                if chars.next() == Some('\'') {
                                    // consumed the reopening quote
                                    token.push('\'');
                                    continue;
                                }
                                // Malformed input (shell_quote never produces
                                // this) — treat conservatively as end-of-token.
                                break;
                            }
                            break;
                        }
                        Some(inner) => token.push(inner),
                        None => break,
                    }
                }
            } else {
                token.push(c);
                chars.next();
            }
        }
        args.push(token);
    }
    args
}
