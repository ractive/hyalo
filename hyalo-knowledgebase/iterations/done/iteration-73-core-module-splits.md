---
title: "Core library: split large hyalo-core modules"
type: iteration
date: 2026-03-30
tags:
  - refactor
  - structure
  - ai-friendliness
status: completed
branch: iter-73/core-module-splits
---

## Goal

Split the three largest modules in `hyalo-core` into focused submodules. Pure structural refactor — no behavioral changes.

## Context

After iteration 70 addressed the CLI crate, `hyalo-core` still has three modules over 1300 lines that mix distinct responsibilities.

## Tasks

### Split scanner.rs (1719 lines)

- [x] Extract `FenceTracker` and fence-related helpers into `scanner/fence.rs`
- [x] Extract `FileVisitor` trait and multi-visitor machinery into `scanner/visitor.rs`
- [x] Extract `FrontmatterCollector` into `scanner/frontmatter.rs`
- [x] Extract inline-code/comment stripping helpers into `scanner/strip.rs`
- [x] Keep `scan_file`, `scan_reader`, `scan_file_multi`, `scan_reader_multi` entry points in `scanner/mod.rs`
- [x] Re-export all public types from `scanner/mod.rs`

### Split filter.rs (1591 lines)

- [x] Extract `PropertyFilter` parsing (`parse_property_filter`, operator enums) into `filter/parse.rs`
- [x] Extract property matching logic (`matches_property`, `matches_filters_with_tags`) into `filter/match_props.rs`
- [x] Extract `TaskFilter` parsing and matching into `filter/tasks.rs`
- [x] Extract `SortField` parsing, `compare_property_values` into `filter/sort.rs`
- [x] Extract `Fields` struct and `Fields::parse` into `filter/fields.rs`
- [x] Keep re-exports in `filter/mod.rs`

### Split frontmatter.rs (1331 lines)

- [x] Extract YAML parsing (`extract_frontmatter`, `parse_yaml_to_indexmap`) into `frontmatter/parse.rs`
- [x] Extract type inference (`infer_type`, `format_value`) into `frontmatter/types.rs`
- [x] Mutation helpers (set/remove) kept as `Document` impl methods in `parse.rs` (no standalone `mutate.rs` needed)
- [x] Keep re-exports in `frontmatter/mod.rs`

### Quality gate

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace` (498 passed)
- [x] No public API changes (all re-exports preserved)
