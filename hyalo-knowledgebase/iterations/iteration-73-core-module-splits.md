---
title: "Core library: split large hyalo-core modules"
type: iteration
date: 2026-03-30
tags:
  - refactor
  - structure
  - ai-friendliness
status: planned
branch: iter-73/core-module-splits
---

## Goal

Split the three largest modules in `hyalo-core` into focused submodules. Pure structural refactor — no behavioral changes.

## Context

After iteration 70 addressed the CLI crate, `hyalo-core` still has three modules over 1300 lines that mix distinct responsibilities.

## Tasks

### Split scanner.rs (1719 lines)

- [ ] Extract `FenceTracker` and fence-related helpers into `scanner/fence.rs`
- [ ] Extract `FileVisitor` trait and multi-visitor machinery into `scanner/visitor.rs`
- [ ] Extract `FrontmatterCollector` into `scanner/frontmatter.rs`
- [ ] Extract inline-code/comment stripping helpers into `scanner/strip.rs`
- [ ] Keep `scan_file`, `scan_reader`, `scan_file_multi`, `scan_reader_multi` entry points in `scanner/mod.rs`
- [ ] Re-export all public types from `scanner/mod.rs`

### Split filter.rs (1591 lines)

- [ ] Extract `PropertyFilter` parsing (`parse_property_filter`, operator enums) into `filter/parse.rs`
- [ ] Extract property matching logic (`matches_property`, `matches_filters_with_tags`) into `filter/match_props.rs`
- [ ] Extract `TaskFilter` parsing and matching into `filter/tasks.rs`
- [ ] Extract `SortField` parsing, `compare_property_values` into `filter/sort.rs`
- [ ] Extract `Fields` struct and `Fields::parse` into `filter/fields.rs`
- [ ] Keep re-exports in `filter/mod.rs`

### Split frontmatter.rs (1331 lines)

- [ ] Extract YAML parsing (`extract_frontmatter`, `parse_yaml_to_indexmap`) into `frontmatter/parse.rs`
- [ ] Extract type inference (`infer_type`, `format_value`) into `frontmatter/types.rs`
- [ ] Extract manipulation helpers (set/remove/append operations) into `frontmatter/mutate.rs`
- [ ] Keep re-exports in `frontmatter/mod.rs`

### Quality gate

- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] No public API changes (all re-exports preserved)
