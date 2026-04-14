use super::common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

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
// 12. Empty pattern is rejected
// ---------------------------------------------------------------------------

#[test]
fn bm25_empty_pattern_rejected() {
    let tmp = setup_bm25_vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", ""])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "empty pattern should return a non-zero exit code"
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
// 22. Whitespace-only query is rejected
// ---------------------------------------------------------------------------

#[test]
fn bm25_whitespace_only_query_rejected() {
    let tmp = setup_bm25_vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "   "])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "whitespace-only query should return a non-zero exit code"
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
