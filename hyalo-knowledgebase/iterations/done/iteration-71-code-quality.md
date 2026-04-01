---
title: "Code quality: clippy pedantic, visibility, scanner clone"
type: iteration
date: 2026-03-29
tags:
  - code-quality
  - refactor
  - api-design
status: completed
branch: iter-71/code-quality
---

## Goal

Clean up code quality issues found in the codebase review. Address clippy pedantic warnings, tighten API visibility, and tackle the deeper `FrontmatterCollector` clone issue.

## Tasks

### Clippy pedantic

- [x] Fix `uninlined_format_args` across both crates — see [[backlog/done/clippy-pedantic-cleanup]]
- [x] Fix `assigning_clones` → use `clone_from` where applicable
- [x] Fix `manual_let_else` → use `let…else` pattern
- [x] Fix `redundant_closure_for_method_calls`
- [x] Fix `stable_sort_primitive` → `.sort_unstable()`
- [x] Fix `cast_possible_wrap` / `cast_sign_loss` → `.cast_signed()` / `.cast_unsigned()`
- [x] Fix `map_unwrap_or` → `.is_some_and()`
- [x] Fix `unnecessary_wraps` in `mv.rs:121`
- [x] Enable `clippy::pedantic` workspace-wide in `Cargo.toml` with intentional allows
- [x] Fix all remaining actionable pedantic lints (single_match_else, format_push_string, wildcard_enum_match_arm, case_sensitive_file_extension_comparison, redundant_continue, match_same_arms, float_cmp, explicit_iter_loop, bool_to_int_with_if, etc.)

### API visibility

- [x] Mark internal helpers as `pub(crate)` in hyalo-core — see [[backlog/done/pub-crate-visibility]]

### Scanner optimization

- [x] Eliminate `FrontmatterCollector` clone of full `IndexMap` per file — see [[backlog/done/frontmatter-collector-clone]]

### Quality gate

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo clippy --workspace --all-targets` produces 0 warnings (pedantic configured in Cargo.toml)
- [x] `cargo test --workspace`
