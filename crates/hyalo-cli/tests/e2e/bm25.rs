use super::common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

#[test]
fn ranked_snippets_streaming_unicode_and_crlf_boundaries() {
    let tmp = TempDir::new().unwrap();
    let prefix = "---\ntitle: Boundary\n---\n";
    let padding = "a".repeat(8191 - prefix.len());
    let unicode_line = format!("{padding}日本語");
    write_md(
        tmp.path(),
        "unicode.md",
        &format!("{prefix}{unicode_line}\n"),
    );
    // Keep the authored CRLF bytes within the shared 64-KiB frontmatter
    // budget while still crossing many streaming buffer boundaries.
    let comments = format!("#{}\r\n", "a".repeat(63)).repeat(990);
    write_md(
        tmp.path(),
        "crlf.md",
        &format!("---\r\ntitle: Near budget\r\n{comments}---\r\nrust\r\n"),
    );
    let index = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .arg("create-index")
        .output()
        .unwrap();
    assert!(index.status.success(), "{index:?}");
    for (query, file, line, expected) in [
        ("日本語", "unicode.md", 4, unicode_line.as_str()),
        ("rust", "crlf.md", 994, "rust"),
    ] {
        let mut previous = None;
        for indexed in [false, true] {
            let mut command = hyalo_no_hints();
            command
                .arg("--dir")
                .arg(tmp.path())
                .args(["find", query, "--format", "json"]);
            if indexed {
                command.arg("--index");
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "{output:?}");
            let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(json["total"], 1);
            assert_eq!(json["results"][0]["file"], file);
            assert_eq!(
                json["results"][0]["matches"],
                serde_json::json!([
                    {"line": line, "section": "", "text": expected}
                ])
            );
            if let Some(previous) = previous {
                assert_eq!(output.stdout, previous);
            }
            previous = Some(output.stdout);
        }
    }
}

#[test]
fn ranked_snippets_semantics_and_complete_index_parity() {
    let tmp = TempDir::new().unwrap();
    let body = "---\ntitle: titleonly rust\n---\n# Guide\nrun\nrust rust rust\nrust golang\ngolang rust\nrust\n## Other\nrun fast\nfast run\n日本語Docker入門\n```rust\n# rust code\n```\n%% rust comment %%\n";
    write_md(tmp.path(), "guide.md", body);
    write_md(
        tmp.path(),
        "title.md",
        "---\ntitle: titleonly\n---\nUnrelated body.\n",
    );
    write_md(tmp.path(), "crossline.md", "# Crossline\nalpha\nbeta\n");
    write_md(
        tmp.path(),
        "german.md",
        "\u{feff}---\r\nlanguage: german\r\n---\r\nHaus\r\n",
    );
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .arg("create-index")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    for args in [
        vec!["rust"],
        vec!["running"],
        vec!["rust OR golang"],
        vec!["rust golang"],
        vec!["\"run fast\""],
        vec!["\"run fast\" OR golang"],
        vec!["日本語"],
        vec!["Docker"],
        vec!["Häuser", "--language", "german"],
        vec!["rust", "--section", "Other"],
        vec!["titleonly"],
        vec!["\"alpha beta\""],
        vec!["rust", "--limit", "0"],
        vec![
            "rust OR titleonly",
            "--limit",
            "1",
            "--sort",
            "file",
            "--reverse",
        ],
    ] {
        let disk = hyalo_no_hints()
            .arg("--dir")
            .arg(tmp.path())
            .args(["--format", "json", "find"])
            .args(&args)
            .output()
            .unwrap();
        let indexed = hyalo_no_hints()
            .arg("--dir")
            .arg(tmp.path())
            .args(["--format", "json", "find", "--index"])
            .args(&args)
            .output()
            .unwrap();
        assert!(disk.status.success(), "{args:?}: {disk:?}");
        assert!(indexed.status.success(), "{args:?}: {indexed:?}");
        assert_eq!(
            disk.stdout, indexed.stdout,
            "whole envelope parity: {args:?}"
        );
        let json: serde_json::Value = serde_json::from_slice(&disk.stdout).unwrap();
        for result in json["results"].as_array().unwrap() {
            let matches = result["matches"].as_array().unwrap();
            assert!(matches.len() <= 3);
            for m in matches {
                assert_eq!(m.as_object().unwrap().len(), 3);
                let content =
                    std::fs::read_to_string(tmp.path().join(result["file"].as_str().unwrap()))
                        .unwrap();
                let line = usize::try_from(m["line"].as_u64().unwrap()).unwrap();
                assert_eq!(
                    content.lines().nth(line - 1).unwrap(),
                    m["text"].as_str().unwrap()
                );
                if result["file"] == "guide.md" {
                    assert!(line > 3);
                }
            }
        }
    }
    let (_, json, _) = find_json(&tmp, &["rust OR golang"]);
    assert_eq!(
        json["results"][0]["matches"],
        serde_json::json!([
            {"line": 7, "section": "# Guide", "text": "rust golang"},
            {"line": 8, "section": "# Guide", "text": "golang rust"},
            {"line": 6, "section": "# Guide", "text": "rust rust rust"},
        ])
    );
    let (_, json, _) = find_json(&tmp, &["running"]);
    assert_eq!(json["results"][0]["matches"][0]["text"], "run");
    let (_, json, _) = find_json(&tmp, &["日本語"]);
    assert_eq!(json["results"][0]["matches"][0]["text"], "日本語Docker入門");
    let (_, json, _) = find_json(&tmp, &["Häuser", "--language", "german"]);
    assert_eq!(
        json["results"][0]["matches"],
        serde_json::json!([
            {"line": 4, "section": "", "text": "Haus"}
        ])
    );
    let (_, json, _) = find_json(&tmp, &["\"run fast\""]);
    assert_eq!(
        json["results"][0]["matches"],
        serde_json::json!([
            {"line": 11, "section": "## Other", "text": "run fast"}
        ])
    );
    let (_, json, _) = find_json(&tmp, &["rust", "--section", "Other"]);
    assert_eq!(
        json["results"][0]["matches"],
        serde_json::json!([
            {"line": 14, "section": "## Other", "text": "```rust"},
            {"line": 15, "section": "## Other", "text": "# rust code"},
            {"line": 17, "section": "## Other", "text": "%% rust comment %%"}
        ])
    );
    for query in ["titleonly", "\"alpha beta\""] {
        let (_, json, _) = find_json(&tmp, &[query]);
        for result in json["results"].as_array().unwrap() {
            assert_eq!(result["matches"], serde_json::json!([]));
        }
    }
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "running", "--format", "text"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("line 5 (# Guide): run"), "{text}");
}

// ---------------------------------------------------------------------------
// Vault fixture
// ---------------------------------------------------------------------------

fn setup_bm25_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();

    // rust_deep.md — highly relevant for "rust" (many mentions)
    write_md(
        tmp.path(),
        "rust_deep.md",
        md!(r"
---
title: Deep Dive into Rust
status: published
tags:
  - rust
  - programming
---
# Deep Dive into Rust

Rust is a systems programming language focused on safety and performance.
Rust ownership model prevents memory bugs at compile time.
The Rust type system is expressive and powerful.
Rust is used for embedded systems, WebAssembly, and network services.
Rust's borrow checker enforces memory safety without a garbage collector.
Learning Rust takes time but the Rust community is very welcoming.
"),
    );

    // rust_brief.md — somewhat relevant for "rust" (one mention)
    write_md(
        tmp.path(),
        "rust_brief.md",
        md!(r"
---
title: Programming Languages
status: published
tags:
  - programming
---
# Programming Languages

There are many programming languages to choose from, including Python, Go, Java, and Rust.
Each language has its strengths and weaknesses depending on the use case.
"),
    );

    // cooking.md — not relevant for "rust"
    write_md(
        tmp.path(),
        "cooking.md",
        md!(r"
---
title: Cooking Guide
status: draft
tags:
  - cooking
---
# Cooking Guide

Cooking is an art and a science. Great recipes require fresh ingredients.
Start by chopping the vegetables and preparing your mise en place.
Slow cooking brings out deep flavours in soups and stews.
"),
    );

    // french.md — French language document
    write_md(
        tmp.path(),
        "french.md",
        md!(r"
---
title: Guide de programmation
language: french
tags:
  - programming
---
# Guide de programmation

La programmation est une activité créative et technique.
Les programmeurs utilisent des langages de programmation pour résoudre des problèmes.
La programmation fonctionnelle et la programmation orientée objet sont deux paradigmes importants.
"),
    );

    // running.md — for stemming tests
    write_md(
        tmp.path(),
        "running.md",
        md!(r"
---
title: Running Guide
tags:
  - fitness
---
# Running Guide

I enjoy running every morning. Running is great exercise.
The runners love it when the weather is perfect for running.
"),
    );

    tmp
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn find_json(
    tmp: &TempDir,
    extra_args: &[&str],
) -> (std::process::ExitStatus, serde_json::Value, String) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["--format", "json"])
        .arg("find")
        .args(extra_args)
        .output()
        .unwrap();
    let status = output.status;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null);
    (status, json, stderr)
}

fn unwrap_results(json: &serde_json::Value) -> &Vec<serde_json::Value> {
    json.get("results")
        .expect("expected {total, results} envelope")
        .as_array()
        .expect("results should be an array")
}

// ---------------------------------------------------------------------------
// 1. Ranked results in score order
// ---------------------------------------------------------------------------

#[test]
fn bm25_ranked_results_in_score_order() {
    let tmp = setup_bm25_vault();
    let (status, json, stderr) = find_json(&tmp, &["rust"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(arr.len() >= 2, "expected at least 2 results: {arr:?}");

    // rust_deep.md should rank first because it mentions "rust" many more times
    let first_file = arr[0]["file"]
        .as_str()
        .expect("file field should be string");
    assert_eq!(
        first_file, "rust_deep.md",
        "rust_deep.md should rank first due to higher BM25 score, got: {arr:?}"
    );

    // Verify scores are in descending order
    let scores: Vec<f64> = arr.iter().filter_map(|v| v["score"].as_f64()).collect();
    assert!(!scores.is_empty(), "expected score fields in BM25 results");
    for i in 1..scores.len() {
        assert!(
            scores[i - 1] >= scores[i],
            "results should be sorted by score descending: {scores:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Score field present in JSON output
// ---------------------------------------------------------------------------

#[test]
fn bm25_score_field_present_in_json() {
    let tmp = setup_bm25_vault();
    let (status, json, stderr) = find_json(&tmp, &["rust"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(!arr.is_empty(), "expected at least one result");

    for entry in arr {
        let score = entry["score"].as_f64();
        assert!(
            score.is_some(),
            "each BM25 result should have a score field, got: {entry}"
        );
        assert!(
            score.unwrap() > 0.0,
            "score should be positive, got: {entry}"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Stemming matches
// ---------------------------------------------------------------------------

#[test]
fn bm25_stemming_matches() {
    let tmp = setup_bm25_vault();

    // "running" query should match running.md (contains "running", "runners")
    let (status, json, stderr) = find_json(&tmp, &["running"]);
    assert!(status.success(), "stderr: {stderr}");
    let arr = unwrap_results(&json);
    assert!(
        arr.iter().any(|v| v["file"] == "running.md"),
        "query 'running' should match running.md: {arr:?}"
    );

    // "run" query (stem of "running") should also match running.md via stemming
    let (status2, json2, stderr2) = find_json(&tmp, &["run"]);
    assert!(status2.success(), "stderr: {stderr2}");
    let arr2 = unwrap_results(&json2);
    assert!(
        arr2.iter().any(|v| v["file"] == "running.md"),
        "query 'run' should match running.md via stemming: {arr2:?}"
    );
}

// ---------------------------------------------------------------------------
// 4. No matches returns empty results
// ---------------------------------------------------------------------------

#[test]
fn bm25_no_matches_returns_empty() {
    let tmp = setup_bm25_vault();
    let (status, json, stderr) = find_json(&tmp, &["xyzzy42quux"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(
        arr.is_empty(),
        "expected empty results for nonsense query: {arr:?}"
    );
}

// ---------------------------------------------------------------------------
// 5. Combined with property filter
// ---------------------------------------------------------------------------

#[test]
fn bm25_combined_with_property_filter() {
    let tmp = setup_bm25_vault();
    // Both rust_deep.md and rust_brief.md have status=published;
    // cooking.md has status=draft and no "rust" content anyway
    let (status, json, stderr) = find_json(&tmp, &["rust", "--property", "status=published"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(!arr.is_empty(), "expected at least one result: {arr:?}");

    // All returned docs must have status=published
    for entry in arr {
        let file = entry["file"].as_str().unwrap_or("");
        // cooking.md is draft and irrelevant — must not appear
        assert_ne!(
            file, "cooking.md",
            "draft doc should be excluded by property filter"
        );
    }

    // rust_deep.md must be present (published and highly relevant)
    assert!(
        arr.iter().any(|v| v["file"] == "rust_deep.md"),
        "rust_deep.md (published) should appear: {arr:?}"
    );
}

// ---------------------------------------------------------------------------
// 6. Combined with tag filter
// ---------------------------------------------------------------------------

#[test]
fn bm25_combined_with_tag_filter() {
    let tmp = setup_bm25_vault();
    // rust_deep.md has both "rust" and "programming" tags
    // rust_brief.md has only "programming" tag (no "rust" tag, but content mentions rust)
    let (status, json, stderr) = find_json(&tmp, &["rust", "--tag", "rust"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(!arr.is_empty(), "expected results: {arr:?}");

    // All results must have the "rust" tag
    // (we can't check tags without --fields tags, but we can verify rust_brief.md is absent)
    assert!(
        !arr.iter().any(|v| v["file"] == "rust_brief.md"),
        "rust_brief.md has no rust tag and should be excluded: {arr:?}"
    );
    assert!(
        arr.iter().any(|v| v["file"] == "rust_deep.md"),
        "rust_deep.md has rust tag and should be included: {arr:?}"
    );
}

// ---------------------------------------------------------------------------
// 7. Regex still works (produces matches field, not score)
// ---------------------------------------------------------------------------

#[test]
fn bm25_regex_still_works() {
    let tmp = setup_bm25_vault();
    let (status, json, stderr) = find_json(&tmp, &["--regexp", "rust.*programming"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(!arr.is_empty(), "regex should return matches: {arr:?}");

    // Regex results should have matches field, not score field
    for entry in arr {
        assert!(
            !entry["matches"].is_null(),
            "regex result should have matches field: {entry}"
        );
    }
}

// ---------------------------------------------------------------------------
// 8. Regex results should NOT have a score field
// ---------------------------------------------------------------------------

#[test]
fn bm25_regex_unranked() {
    let tmp = setup_bm25_vault();
    let (status, json, stderr) = find_json(&tmp, &["--regexp", "Rust"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(!arr.is_empty(), "expected regex matches: {arr:?}");

    for entry in arr {
        assert!(
            entry["score"].is_null(),
            "regex result should NOT have a score field: {entry}"
        );
    }
}

// ---------------------------------------------------------------------------
// 9. Language flag — French stemmer
// ---------------------------------------------------------------------------

#[test]
fn bm25_language_flag() {
    let tmp = setup_bm25_vault();
    // With --language french, "programmation" is stemmed with the French stemmer,
    // matching french.md which is also tokenized with its frontmatter language: french
    let (status, json, stderr) = find_json(&tmp, &["programmation", "--language", "french"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(
        arr.iter().any(|v| v["file"] == "french.md"),
        "french.md should match 'programmation' with --language french: {arr:?}"
    );
}

// ---------------------------------------------------------------------------
// 10. Frontmatter language — French doc matched with French query language
// ---------------------------------------------------------------------------

#[test]
fn bm25_frontmatter_language() {
    let tmp = setup_bm25_vault();
    // french.md has `language: french` in frontmatter, so its tokens are French-stemmed.
    // When we query with --language french, the query "programmation" is also French-stemmed,
    // so both sides use the same stemmer and the match is found.
    let (status, json, stderr) = find_json(&tmp, &["programmation", "--language", "french"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(
        arr.iter().any(|v| v["file"] == "french.md"),
        "french.md should be found when query language matches doc language: {arr:?}"
    );

    // Verify score field is present
    if let Some(entry) = arr.iter().find(|v| v["file"] == "french.md") {
        let score = entry["score"].as_f64();
        assert!(score.is_some(), "french.md result should have score field");
        assert!(score.unwrap() > 0.0, "score should be positive");
    }
}

// ---------------------------------------------------------------------------
// 11. Text output includes score
// ---------------------------------------------------------------------------

#[test]
fn bm25_text_output_includes_score() {
    let tmp = setup_bm25_vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["--format", "text"])
        .args(["find", "rust"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("rust_deep.md"),
        "text output should contain rust_deep.md: {stdout}"
    );
    assert!(
        stdout.contains("score:"),
        "text output should include score field for BM25 results: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// 12. Empty pattern is treated as no pattern — matches all files (UX-6)
// ---------------------------------------------------------------------------

#[test]
fn bm25_empty_pattern_matches_all_files() {
    let tmp = setup_bm25_vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "", "--format", "json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "empty pattern should exit zero (matches all files)"
    );
}

// ---------------------------------------------------------------------------
// 13. Sort override — --sort file overrides BM25 score order
// ---------------------------------------------------------------------------

#[test]
fn bm25_sort_override() {
    let tmp = setup_bm25_vault();
    let (status, json, stderr) = find_json(&tmp, &["rust", "--sort", "file"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(arr.len() >= 2, "expected at least 2 results: {arr:?}");

    // With --sort file, results must be in alphabetical file order
    let files: Vec<&str> = arr
        .iter()
        .map(|v| v["file"].as_str().expect("file field should be string"))
        .collect();
    let mut sorted = files.clone();
    sorted.sort_unstable();
    assert_eq!(
        files, sorted,
        "with --sort file, results should be sorted alphabetically: {files:?}"
    );
}

// ---------------------------------------------------------------------------
// 14. Limit — --limit 1 returns only 1 result
// ---------------------------------------------------------------------------

#[test]
fn bm25_limit() {
    let tmp = setup_bm25_vault();
    let (status, json, stderr) = find_json(&tmp, &["rust", "--limit", "1"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert_eq!(
        arr.len(),
        1,
        "expected exactly 1 result with --limit 1: {arr:?}"
    );

    // The single result should be the top BM25 match
    assert_eq!(
        arr[0]["file"].as_str().unwrap(),
        "rust_deep.md",
        "top result with --limit 1 should be rust_deep.md: {arr:?}"
    );
}

// ---------------------------------------------------------------------------
// 15. Via index — BM25 still ranks results when --index is used
// ---------------------------------------------------------------------------

#[test]
fn bm25_via_index() {
    let tmp = setup_bm25_vault();

    // Build the index first
    let index_output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("create-index")
        .output()
        .unwrap();
    assert!(
        index_output.status.success(),
        "create-index failed: {}",
        String::from_utf8_lossy(&index_output.stderr)
    );

    let (status, json, stderr) = find_json(&tmp, &["rust", "--index"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(
        arr.len() >= 2,
        "expected at least 2 BM25 results via index: {arr:?}"
    );

    // rust_deep.md should still rank first
    assert_eq!(
        arr[0]["file"].as_str().unwrap(),
        "rust_deep.md",
        "rust_deep.md should rank first via --index: {arr:?}"
    );

    // Score field should be present
    let score = arr[0]["score"].as_f64();
    assert!(
        score.is_some(),
        "BM25 result via --index should have score field"
    );
    assert!(score.unwrap() > 0.0, "score should be positive");
}

// ---------------------------------------------------------------------------
// 16. Negation — -term excludes matching docs
// ---------------------------------------------------------------------------

#[test]
fn bm25_negation_excludes_docs() {
    let tmp = setup_bm25_vault();
    // "programming -rust" should return results about programming but exclude
    // docs that mention "rust" (after stemming)
    let (status, json, stderr) = find_json(&tmp, &["programming -rust"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    // rust_deep.md and rust_brief.md mention "rust" → should be excluded
    for entry in arr {
        let file = entry["file"].as_str().unwrap_or("");
        assert_ne!(
            file, "rust_deep.md",
            "rust_deep.md should be excluded by -rust"
        );
        assert_ne!(
            file, "rust_brief.md",
            "rust_brief.md should be excluded by -rust"
        );
    }
}

// ---------------------------------------------------------------------------
// 17. Implicit AND — all terms required
// ---------------------------------------------------------------------------

#[test]
fn bm25_implicit_and_requires_both_terms() {
    let tmp = setup_bm25_vault();
    // "rust programming" → AND semantics: both terms must be present
    let (status, json, stderr) = find_json(&tmp, &["rust programming"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();

    // rust_deep.md has both "rust" (many times) and "programming language"
    assert!(
        files.contains(&"rust_deep.md"),
        "rust_deep.md should match AND query (has both terms): {files:?}"
    );
    // rust_brief.md has both "Rust" and "programming languages"
    assert!(
        files.contains(&"rust_brief.md"),
        "rust_brief.md should match AND query (has both terms): {files:?}"
    );
    // cooking.md has neither → must not appear
    assert!(
        !files.contains(&"cooking.md"),
        "cooking.md should NOT match AND query: {files:?}"
    );
}

// ---------------------------------------------------------------------------
// 18. AND with no overlap — returns empty
// ---------------------------------------------------------------------------

#[test]
fn bm25_and_no_overlap_returns_empty() {
    let tmp = setup_bm25_vault();
    // "rust cooking" — no document contains both terms
    let (status, json, stderr) = find_json(&tmp, &["rust cooking"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(
        arr.is_empty(),
        "expected empty results: no doc has both 'rust' and 'cooking': {arr:?}"
    );
}

// ---------------------------------------------------------------------------
// 19. Explicit OR — either term matches
// ---------------------------------------------------------------------------

#[test]
fn bm25_explicit_or_returns_union() {
    let tmp = setup_bm25_vault();
    // "rust OR cooking" → any doc with either term
    let (status, json, stderr) = find_json(&tmp, &["rust OR cooking"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();

    assert!(
        files.contains(&"rust_deep.md"),
        "rust_deep.md should match OR query: {files:?}"
    );
    assert!(
        files.contains(&"rust_brief.md"),
        "rust_brief.md should match OR query: {files:?}"
    );
    assert!(
        files.contains(&"cooking.md"),
        "cooking.md should match OR query: {files:?}"
    );
    // running.md and french.md have neither → should not appear
    assert!(
        !files.contains(&"running.md"),
        "running.md should NOT match 'rust OR cooking': {files:?}"
    );
}

// ---------------------------------------------------------------------------
// 20. Phrase search — consecutive words required
// ---------------------------------------------------------------------------

#[test]
fn bm25_phrase_search_consecutive_match() {
    let tmp = setup_bm25_vault();
    // "systems programming" appears consecutively in rust_deep.md
    // ("Rust is a systems programming language")
    // rust_brief.md has both words but NOT consecutively
    let (status, json, stderr) = find_json(&tmp, &["\"systems programming\""]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();

    assert!(
        files.contains(&"rust_deep.md"),
        "rust_deep.md should match phrase 'systems programming': {files:?}"
    );
    assert!(
        !files.contains(&"rust_brief.md"),
        "rust_brief.md should NOT match phrase 'systems programming' (words not adjacent): {files:?}"
    );
    assert!(
        !files.contains(&"cooking.md"),
        "cooking.md should NOT match phrase 'systems programming': {files:?}"
    );
}

// ---------------------------------------------------------------------------
// 21. Mixed OR + negation
// ---------------------------------------------------------------------------

#[test]
fn bm25_mixed_or_negation() {
    let tmp = setup_bm25_vault();
    // "rust OR cooking -safety"
    // rust_deep.md has "safety" → excluded
    // rust_brief.md has "rust" but not "safety" → included
    // cooking.md has "cooking" and not "safety" → included
    let (status, json, stderr) = find_json(&tmp, &["rust OR cooking -safety"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();

    assert!(
        !files.contains(&"rust_deep.md"),
        "rust_deep.md should be excluded (contains 'safety'): {files:?}"
    );
    assert!(
        files.contains(&"rust_brief.md"),
        "rust_brief.md should be included (has 'rust', no 'safety'): {files:?}"
    );
    assert!(
        files.contains(&"cooking.md"),
        "cooking.md should be included (has 'cooking', no 'safety'): {files:?}"
    );
}

// ---------------------------------------------------------------------------
// 22. Whitespace-only query is treated as no pattern — matches all files (UX-6)
// ---------------------------------------------------------------------------

#[test]
fn bm25_whitespace_only_query_matches_all_files() {
    let tmp = setup_bm25_vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "   ", "--format", "json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "whitespace-only query should exit zero (treated as no pattern)"
    );
}

// ---------------------------------------------------------------------------
// 23. Reverse — lowest BM25 score first
// ---------------------------------------------------------------------------

#[test]
fn bm25_reverse_puts_lowest_score_first() {
    let tmp = setup_bm25_vault();
    let (status, json, stderr) = find_json(&tmp, &["rust", "--reverse"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(
        arr.len() >= 2,
        "expected at least 2 results for reverse test: {arr:?}"
    );

    // With --reverse the lowest-scoring result should be first
    let scores: Vec<f64> = arr.iter().filter_map(|v| v["score"].as_f64()).collect();
    assert!(!scores.is_empty(), "expected score fields in BM25 results");
    for i in 1..scores.len() {
        assert!(
            scores[i - 1] <= scores[i],
            "with --reverse, scores should be ascending (lowest first): {scores:?}"
        );
    }

    // rust_deep.md has the highest BM25 score so it should be last in reverse order
    let last_file = arr.last().unwrap()["file"].as_str().unwrap();
    assert_eq!(
        last_file, "rust_deep.md",
        "rust_deep.md should be last (highest score) with --reverse: {arr:?}"
    );
}

// ---------------------------------------------------------------------------
// 24. Phrase search via index
// ---------------------------------------------------------------------------

#[test]
fn bm25_phrase_search_via_index() {
    let tmp = setup_bm25_vault();

    // Build the index first (it will store positions for phrase matching)
    let index_output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("create-index")
        .output()
        .unwrap();
    assert!(
        index_output.status.success(),
        "create-index failed: {}",
        String::from_utf8_lossy(&index_output.stderr)
    );

    // Phrase search via the built index
    let (status, json, stderr) = find_json(&tmp, &["\"systems programming\"", "--index"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();

    assert!(
        files.contains(&"rust_deep.md"),
        "rust_deep.md should match phrase via --index: {files:?}"
    );
    assert!(
        !files.contains(&"rust_brief.md"),
        "rust_brief.md should NOT match phrase via --index: {files:?}"
    );
}

// ---------------------------------------------------------------------------
// 25. Section-scoped BM25 search
// ---------------------------------------------------------------------------

fn setup_multisection_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();

    write_md(
        tmp.path(),
        "multi_section.md",
        md!(r"
---
title: Multi-section Doc
tags:
  - test
---
# Multi-section Doc

## Rust Section
Rust is a systems programming language with great performance.

## Python Section
Python is popular for data science and scripting.
"),
    );

    tmp
}

#[test]
fn bm25_section_scoped_search_includes_correct_section() {
    let tmp = setup_multisection_vault();

    // "rust" appears only in "Rust Section" — searching with --section "Rust Section" should find it
    let (status, json, stderr) = find_json(&tmp, &["rust", "--section", "Rust Section"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(
        arr.iter().any(|v| v["file"] == "multi_section.md"),
        "multi_section.md should match 'rust' in --section 'Rust Section': {arr:?}"
    );
}

#[test]
fn bm25_section_scoped_search_excludes_other_section() {
    let tmp = setup_multisection_vault();

    // "rust" is NOT in "Python Section" — searching with --section "Python Section" should not find it
    let (status, json, stderr) = find_json(&tmp, &["rust", "--section", "Python Section"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = unwrap_results(&json);
    assert!(
        !arr.iter().any(|v| v["file"] == "multi_section.md"),
        "multi_section.md should NOT match 'rust' in --section 'Python Section': {arr:?}"
    );
}

// ---------------------------------------------------------------------------
// BUG-3: Bare boolean operator queries warn the user
// ---------------------------------------------------------------------------

#[test]
fn bm25_bare_and_operator_emits_warning() {
    let tmp = setup_bm25_vault();
    // "and" is consumed as a boolean operator keyword, producing an empty query.
    // The user should receive a warning directing them to quote the literal word.
    let (status, json, stderr) = find_json(&tmp, &["and"]);
    assert!(status.success(), "command should still succeed: {stderr}");

    let arr = unwrap_results(&json);
    assert!(
        arr.is_empty(),
        "bare 'and' query should return no results: {arr:?}"
    );
    assert!(
        stderr.contains("was interpreted as a boolean operator"),
        "expected operator warning in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains(r#""and""#),
        "warning should suggest quoting the word, got: {stderr:?}"
    );
}

#[test]
fn bm25_bare_or_operator_emits_warning() {
    let tmp = setup_bm25_vault();
    let (status, json, stderr) = find_json(&tmp, &["or"]);
    assert!(status.success(), "command should still succeed: {stderr}");

    let arr = unwrap_results(&json);
    assert!(
        arr.is_empty(),
        "bare 'or' query should return no results: {arr:?}"
    );
    assert!(
        stderr.contains("was interpreted as a boolean operator"),
        "expected operator warning in stderr, got: {stderr:?}"
    );
}

#[test]
fn bm25_uppercase_and_operator_emits_warning() {
    let tmp = setup_bm25_vault();
    // "AND" (uppercase) is also consumed as an operator keyword.
    let (status, _json, stderr) = find_json(&tmp, &["AND"]);
    assert!(status.success(), "command should still succeed: {stderr}");
    assert!(
        stderr.contains("was interpreted as a boolean operator"),
        "uppercase AND should also trigger the warning, got: {stderr:?}"
    );
}

#[test]
fn bm25_combined_and_or_operators_listed_together() {
    // BUG-3 follow-up (iter-114): when both AND and OR are stripped as bare
    // operators, the warning must mention BOTH — not just the first.
    let tmp = setup_bm25_vault();
    let (status, _json, stderr) = find_json(&tmp, &["AND OR"]);
    assert!(status.success(), "command should still succeed: {stderr}");
    assert!(
        stderr.contains(r#""AND""#),
        "warning should mention AND, got: {stderr:?}"
    );
    assert!(
        stderr.contains(r#""OR""#),
        "warning should mention OR, got: {stderr:?}"
    );
    assert!(
        stderr.contains("were interpreted as boolean operators"),
        "warning should use plural phrasing when multiple operators stripped, got: {stderr:?}"
    );
}

#[test]
fn bm25_real_word_does_not_emit_operator_warning() {
    let tmp = setup_bm25_vault();
    // "rust" is a real search term — no operator warning should be emitted.
    let (status, _json, stderr) = find_json(&tmp, &["rust"]);
    assert!(status.success(), "command should succeed: {stderr}");
    assert!(
        !stderr.contains("was interpreted as a boolean operator"),
        "real word should not trigger operator warning, got: {stderr:?}"
    );
}

#[test]
fn bm25_quoted_and_finds_literal_word() {
    let tmp = TempDir::new().unwrap();
    // Create a document that actually contains the word "and".
    write_md(
        tmp.path(),
        "connector.md",
        md!(r"
---
title: Connector Words
---
# Connector Words

Words like and, or, but are conjunctions.
"),
    );
    // Searching for the quoted phrase '"and"' should find the document.
    let (status, json, stderr) = find_json(&tmp, &[r#""and""#]);
    assert!(status.success(), "stderr: {stderr}");
    assert!(
        !stderr.contains("was interpreted as a boolean operator"),
        "quoted 'and' should not trigger operator warning, got: {stderr:?}"
    );
    let arr = unwrap_results(&json);
    assert!(
        arr.iter().any(|v| v["file"] == "connector.md"),
        "quoted 'and' should match connector.md: {arr:?}"
    );
}

// ---------------------------------------------------------------------------
// --stemmer / --language ISO 639-1 codes
// ---------------------------------------------------------------------------

#[test]
fn stemmer_iso_639_1_code_accepted() {
    let tmp = setup_bm25_vault();
    let dir = tmp.path().to_str().unwrap();

    // --stemmer en (ISO code for English) should work the same as --stemmer english
    let output = hyalo_no_hints()
        .args(["--dir", dir, "find", "rust", "--stemmer", "en"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    assert!(!results.is_empty(), "ISO code 'en' should produce results");
}

#[test]
fn stemmer_iso_639_1_de_accepted() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "doc.md",
        "---\ntitle: Test\n---\nDeutsche Sprache schwere Sprache.\n",
    );

    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "find",
            "sprache",
            "--stemmer",
            "de",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------------------------------------------------------------------------
// 30. iter-260 — mutating commands must not drop the persisted BM25 section
// ---------------------------------------------------------------------------

/// Snapshot loading defers the BM25 section (iter-260), so a mutating command
/// that re-saves the snapshot re-serializes a field it never asked for. If the
/// deferred section were not forced before the write, `set`, `remove`,
/// `append`, `task toggle`, `mv` and `lint --fix --index` would each silently
/// delete the search index they were only meant to patch — leaving `find
/// --index` quietly falling back to a live scan for the rest of the vault's
/// life.
#[test]
fn bm25_section_survives_a_mutating_command() {
    use hyalo_core::index::SnapshotIndex;

    let tmp = setup_bm25_vault();
    let index_path = tmp.path().join(".hyalo-index");

    let index_output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("create-index")
        .output()
        .unwrap();
    assert!(
        index_output.status.success(),
        "create-index failed: {}",
        String::from_utf8_lossy(&index_output.stderr)
    );

    let before = SnapshotIndex::load(&index_path)
        .unwrap()
        .expect("freshly built index loads");
    let docs_before = before
        .bm25_index()
        .expect("create-index must persist a BM25 section")
        .doc_count();
    drop(before);
    let bytes_before = std::fs::read(&index_path).unwrap();

    // Mutating commands that touch a file and re-save the snapshot. `--index`
    // is what makes them patch the index in place instead of leaving it alone.
    for args in [
        ["set", "rust_deep.md", "--property", "reviewed=true"],
        ["set", "rust_brief.md", "--tag", "reviewed"],
    ] {
        let out = hyalo_no_hints()
            .args(["--dir", tmp.path().to_str().unwrap()])
            .args(args)
            .arg("--index")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    // The snapshot must actually have been rewritten — otherwise this test
    // would pass without ever exercising the save path it exists to guard.
    assert_ne!(
        std::fs::read(&index_path).unwrap(),
        bytes_before,
        "the mutating commands must have re-saved the snapshot"
    );

    // The rewritten snapshot still carries a BM25 section.
    let after = SnapshotIndex::load(&index_path)
        .unwrap()
        .expect("rewritten index loads");
    let docs_after = after
        .bm25_index()
        .expect("a mutating command must not drop the BM25 section")
        .doc_count();
    assert_eq!(
        docs_after, docs_before,
        "the rewritten BM25 section must still cover every document"
    );
    drop(after);

    // And a text query through the rewritten snapshot is still BM25-ranked.
    let (status, json, stderr) = find_json(&tmp, &["rust", "--index"]);
    assert!(status.success(), "stderr: {stderr}");
    let arr = unwrap_results(&json);
    assert!(
        arr.len() >= 2,
        "expected BM25 results from the rewritten index: {arr:?}"
    );
    assert_eq!(
        arr[0]["file"].as_str().unwrap(),
        "rust_deep.md",
        "rust_deep.md should still rank first after a mutation: {arr:?}"
    );
    assert!(
        arr[0]["score"].as_f64().is_some_and(|s| s > 0.0),
        "results must still carry a positive BM25 score: {arr:?}"
    );
}

// Iteration 289: a cached path is not authority to read its current destination.
fn replace_with_symlink(target: &std::path::Path, link: &std::path::Path, directory: bool) -> bool {
    #[cfg(unix)]
    let result = {
        let _ = directory;
        std::os::unix::fs::symlink(target, link)
    };
    #[cfg(windows)]
    let result = if directory {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    };
    #[cfg(windows)]
    if result
        .as_ref()
        .is_err_and(|error| error.raw_os_error() == Some(1314))
    {
        eprintln!("symlink capability unavailable: Windows requires Developer Mode or privilege");
        return false;
    }
    result.unwrap();
    true
}

fn indexed_ranked_fixture() -> TempDir {
    let vault = TempDir::new().unwrap();
    std::fs::write(
        vault.path().join(".hyalo.toml"),
        "[scan]\nverbose_skips = true\n",
    )
    .unwrap();
    write_md(
        vault.path(),
        "notes/note.md",
        "# Fruit\npineapple safe fixture\n",
    );
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(vault.path())
        .arg("create-index")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    vault
}

#[test]
fn ranked_live_reads_refuse_file_and_directory_symlink_escape() {
    for directory in [false, true] {
        let vault = indexed_ranked_fixture();
        let outside = TempDir::new().unwrap();
        write_md(
            outside.path(),
            "note.md",
            "# Fruit\npineapple EXTERNAL_FIXTURE_MARKER\n",
        );
        let (link, target) = if directory {
            std::fs::remove_dir_all(vault.path().join("notes")).unwrap();
            (vault.path().join("notes"), outside.path().to_owned())
        } else {
            std::fs::remove_file(vault.path().join("notes/note.md")).unwrap();
            (
                vault.path().join("notes/note.md"),
                outside.path().join("note.md"),
            )
        };
        if !replace_with_symlink(&target, &link, directory) {
            continue;
        }
        // Persisted snippets, section fallback, and language-miss fallback.
        for extra in [
            vec![],
            vec!["--section", "Fruit"],
            vec!["--language", "german"],
        ] {
            let output = hyalo_no_hints()
                .arg("--dir")
                .arg(vault.path())
                .args(["find", "pineapple", "--index"])
                .args(&extra)
                .output()
                .unwrap();
            assert!(
                !output.status.success(),
                "{directory}, {extra:?}: {output:?}"
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("outside vault"), "{stderr}");
            assert!(stderr.contains("notes/note.md"), "{stderr}");
            assert!(!String::from_utf8_lossy(&output.stdout).contains("EXTERNAL_FIXTURE_MARKER"));
            assert!(!stderr.contains("EXTERNAL_FIXTURE_MARKER"));
        }
        // No selected snippets means no body reads on the compatible fast path.
        // Public --limit 0 means unlimited, so narrow with metadata instead.
        let output = hyalo_no_hints()
            .arg("--dir")
            .arg(vault.path())
            .args([
                "find",
                "pineapple",
                "--index",
                "--limit",
                "0",
                "--tag",
                "absent",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["results"], serde_json::json!([]));
    }
}

#[test]
fn ranked_live_reads_allow_in_vault_symlink_destinations() {
    for directory in [false, true] {
        let vault = indexed_ranked_fixture();
        std::fs::rename(vault.path().join("notes"), vault.path().join("real")).unwrap();
        let (link, target) = if directory {
            (vault.path().join("notes"), vault.path().join("real"))
        } else {
            std::fs::create_dir(vault.path().join("notes")).unwrap();
            (
                vault.path().join("notes/note.md"),
                vault.path().join("real/note.md"),
            )
        };
        if !replace_with_symlink(&target, &link, directory) {
            continue;
        }
        for extra in [
            vec![],
            vec!["--section", "Fruit"],
            vec!["--language", "german"],
        ] {
            let output = hyalo_no_hints()
                .arg("--dir")
                .arg(vault.path())
                .args(["find", "pineapple", "--index"])
                .args(extra)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(String::from_utf8_lossy(&output.stdout).contains("pineapple safe fixture"));
        }
    }
}

#[test]
fn ranked_live_reads_preserve_missing_target_diagnostics() {
    let vault = indexed_ranked_fixture();
    std::fs::remove_file(vault.path().join("notes/note.md")).unwrap();
    for fallback in [false, true] {
        let mut command = hyalo_no_hints();
        command
            .current_dir(vault.path())
            .arg("--dir")
            .arg(vault.path())
            .args(["find", "pineapple", "--index"]);
        if fallback {
            command.args(["--section", "Fruit"]);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.success(), fallback, "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("notes/note.md"), "{stderr}");
        assert!(!stderr.contains("outside vault"), "{stderr}");
    }
}

#[cfg(unix)]
#[test]
fn ranked_live_reads_preserve_permission_errors() {
    use std::os::unix::fs::PermissionsExt;
    let vault = indexed_ranked_fixture();
    let note = vault.path().join("notes/note.md");
    std::fs::set_permissions(&note, std::fs::Permissions::from_mode(0o0)).unwrap();
    if std::fs::File::open(&note).is_ok() {
        eprintln!("permission test unavailable: privileged process can read mode-000 fixture");
        return;
    }
    for fallback in [false, true] {
        let mut command = hyalo_no_hints();
        command
            .current_dir(vault.path())
            .arg("--dir")
            .arg(vault.path())
            .args(["find", "pineapple", "--index"]);
        if fallback {
            command.args(["--section", "Fruit"]);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.success(), fallback, "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Permission denied"), "{stderr}");
        assert!(!stderr.contains("outside vault"), "{stderr}");
    }
    std::fs::set_permissions(note, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn ranked_language_overrides_match_live_scoring_and_snippets() {
    for config_language in ["english", "german"] {
        let vault = TempDir::new().unwrap();
        write_md(vault.path(), "default.md", "# Sport\nrunning\n");
        write_md(
            vault.path(),
            "english.md",
            "---\nlanguage: english\ntags: [selected]\n---\n# Sport\nrunning run\n",
        );
        write_md(
            vault.path(),
            "german.md",
            "---\nlanguage: german\n---\n# Sport\nrunning\n",
        );
        let output = hyalo_no_hints()
            .arg("--dir")
            .arg(vault.path())
            .arg("create-index")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        // Change only effective config after indexing with the English default.
        std::fs::write(
            vault.path().join(".hyalo.toml"),
            format!("[search]\nlanguage = \"{config_language}\"\n"),
        )
        .unwrap();
        for args in [
            vec!["run"],
            vec!["run", "--language", "english"],
            vec!["run", "--language", "german"],
            vec!["run", "--language", "german", "--section", "Sport"],
            // Candidate language stays English; excluded default.md changes corpus IDF.
            vec!["run", "--language", "german", "--tag", "selected"],
            vec!["run", "--language", "german", "--glob", "english.md"],
        ] {
            let mut envelopes = Vec::new();
            for indexed in [false, true] {
                let mut command = hyalo_no_hints();
                command
                    .current_dir(vault.path())
                    .arg("--dir")
                    .arg(vault.path())
                    .args(["find", "--format", "json"])
                    .args(&args);
                if indexed {
                    command.arg("--index");
                }
                let output = command.output().unwrap();
                assert!(
                    output.status.success(),
                    "{config_language}, {args:?}: {output:?}"
                );
                let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                let german = args.contains(&"german")
                    || (config_language == "german" && !args.contains(&"english"));
                let files: Vec<_> = envelope["results"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|result| result["file"].as_str().unwrap())
                    .collect();
                assert!(
                    files.contains(&"english.md"),
                    "frontmatter wins: {envelope}"
                );
                if german {
                    assert!(!files.contains(&"default.md"), "{envelope}");
                }
                for result in envelope["results"].as_array().unwrap() {
                    assert!(
                        !result["matches"].as_array().unwrap().is_empty(),
                        "{result}"
                    );
                }
                envelopes.push(envelope);
            }
            assert_eq!(envelopes[0], envelopes[1], "{config_language}, {args:?}");
        }
    }
}

#[test]
fn ranked_legacy_language_metadata_falls_back_and_checks_containment() {
    use hyalo_core::index::SnapshotIndex;
    let vault = indexed_ranked_fixture();
    let snapshot_path = vault.path().join(".hyalo-index");
    let mut snapshot = SnapshotIndex::load(&snapshot_path).unwrap().unwrap();
    assert!(snapshot.bm25_index().is_some());
    let entry = snapshot.get_mut("notes/note.md").unwrap();
    assert_eq!(entry.bm25_language.as_deref(), Some("english"));
    assert!(entry.bm25_tokens.is_none());
    entry.bm25_language = None; // Snapshot shape produced before iteration 289.
    snapshot.finish_changes(); // Legacy mutable adapter requires coherent publication.
    snapshot.save_to(&snapshot_path).unwrap();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(vault.path())
        .args(["find", "pineapple", "--index"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("safe fixture"));
    let outside = TempDir::new().unwrap();
    write_md(
        outside.path(),
        "note.md",
        "pineapple EXTERNAL_FIXTURE_MARKER\n",
    );
    let link = vault.path().join("notes/note.md");
    std::fs::remove_file(&link).unwrap();
    if !replace_with_symlink(&outside.path().join("note.md"), &link, false) {
        return;
    }
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(vault.path())
        .args(["find", "pineapple", "--index"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("outside vault"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("EXTERNAL_FIXTURE_MARKER"));
}

#[test]
fn ranked_compatible_persisted_corpus_does_not_read_nonselected_bodies() {
    let vault = TempDir::new().unwrap();
    write_md(vault.path(), "keep.md", "pineapple safe\n");
    write_md(vault.path(), "other.md", "pineapple unrelated\n");
    // This indexed note has no BM25 tokens/language and does not belong to
    // the stored corpus; it must not invalidate otherwise compatible languages.
    std::fs::write(vault.path().join("invalid.md"), b"# Invalid\n\xff\xfe").unwrap();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(vault.path())
        .arg("create-index")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let query = || {
        hyalo_no_hints()
            .arg("--dir")
            .arg(vault.path())
            .args([
                "find",
                "pineapple",
                "--index",
                "--sort",
                "file",
                "--limit",
                "1",
            ])
            .output()
            .unwrap()
    };
    let before = query();
    std::fs::remove_file(vault.path().join("other.md")).unwrap();
    let after = query();
    assert!(after.status.success(), "{after:?}");
    assert_eq!(before.stdout, after.stdout);
    assert!(!String::from_utf8_lossy(&after.stderr).contains("unreadable"));
}

#[test]
fn ranked_language_fallback_reuses_compatible_persisted_document_tokens() {
    let vault = TempDir::new().unwrap();
    write_md(
        vault.path(),
        "keep.md",
        "---\nlanguage: english\n---\nrunning run\n",
    );
    write_md(
        vault.path(),
        "compatible.md",
        "---\nlanguage: english\n---\nrunning\n",
    );
    write_md(vault.path(), "changed.md", "running\n");
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(vault.path())
        .arg("create-index")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let query = || {
        hyalo_no_hints()
            .arg("--dir")
            .arg(vault.path())
            .args([
                "find",
                "run",
                "--index",
                "--language",
                "german",
                "--sort",
                "file",
                "--reverse",
                "--limit",
                "1",
            ])
            .output()
            .unwrap()
    };
    let before = query();
    std::fs::remove_file(vault.path().join("compatible.md")).unwrap();
    let after = query();
    assert!(after.status.success(), "{after:?}");
    assert_eq!(before.stdout, after.stdout);
    assert!(!String::from_utf8_lossy(&after.stderr).contains("unreadable"));
}
