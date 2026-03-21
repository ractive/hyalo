---
branch: iter-7/text-output-jaq
date: 2026-03-21
status: completed
tags:
- iteration
- cli
- output
title: Iteration 7 — Human-Readable Text Output via jaq
type: iteration
---

# Iteration 7 — Human-Readable Text Output via jaq

## Goal

Replace the generic key=value text formatter with proper human-readable output for all commands using jq filter strings executed via the `jaq` crate (pure Rust jq interpreter).

## Motivation

During iteration 6 dogfooding, `--format text` produced output like `properties: [{name=title, type=text, value=My Note}]`. Every text-oriented use case required piping JSON through Python or `jq`. Text output should be immediately useful without post-processing.

## Approach

JSON is the single source of truth. Each output type (typed struct) gets a `&'static str` jq filter. Filters are looked up by computing a "key signature" — the sorted comma-joined top-level keys of the JSON object. Unknown shapes fall back to the old generic formatter. See [[decision-log#DEC-027]].

## Tasks

- [x] Add jaq dependencies to Cargo.toml (`jaq-core = "2.2.1"`, `jaq-json = "1.1.3"` with `serde_json` feature, `jaq-std = "2.1.2"`)
- [x] Build `apply_jq_filter(filter_code, value) -> Option<String>` in `src/output.rs`
- [x] Build `key_signature(map) -> String` and `lookup_filter(sig) -> Option<&'static str>`
- [x] Define jq filter constants for all 16+ output types (PropertyInfo, FileProperties, PropertySummaryEntry, PropertyRemoved, PropertyFindResult, PropertyMutationResult, FileTags, TagSummary, TagSummaryEntry, TagFindResult, TagMutationResult, LinkInfo variants ×4, FileLinks, TaskCount, OutlineSection ×2, FileOutline)
- [x] Replace `format_value_as_text` with jaq-based renderer
- [x] Handle list-type property values — join array elements with ", " instead of rendering as JSON
- [x] Unit tests for `apply_jq_filter`, each filter constant, fallback behavior, array of objects
- [x] Update existing e2e text tests (3 in e2e_links.rs, 1 in e2e_outline.rs) to assert new format
- [x] Update existing e2e text tests in e2e_properties.rs, e2e_tags.rs, e2e_property_read.rs
- [x] Add new e2e text tests: `properties_summary_text_format`, `tag_add_text_format`, `tag_remove_text_format`, `tag_find_text_format` improvements
- [x] cargo fmt, cargo clippy --all-targets -- -D warnings, cargo test --workspace (all pass)
- [x] Decision log entry DEC-027
- [x] Dogfood: all commands with `--format text` against `hyalo-knowledgebase/`

## Key Technical Note

Rust 2024 edition treats `"#` inside `r#"..."#` raw strings as the closing delimiter. The `"#" * .level` jq expression (string multiplication for heading prefix) requires using `r##"..."##` instead.

## Dogfooding Observations

- `--format text properties summary`: clean tabular output with name, type, file count
- `--format text tags summary`: readable "N unique tags" header with per-tag counts
- `--format text tags list --glob`: one line per file with comma-joined tags
- `--format text tag find --name`: shows matching files indented under count line
- `--format text outline --file`: file path, tags, props, then `#`-prefixed headings with task counts
- `--format text properties list --file`: file path header, then indented `name (type): value` lines
- No issues with JSON output path (completely unchanged)
