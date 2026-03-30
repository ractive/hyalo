---
title: "Code quality: clippy pedantic, visibility, scanner clone"
type: iteration
date: 2026-03-29
tags:
  - code-quality
  - refactor
  - api-design
status: planned
branch: iter-71/code-quality
---

## Goal

Clean up code quality issues found in the codebase review. Address clippy pedantic warnings, tighten API visibility, and tackle the deeper `FrontmatterCollector` clone issue.

## Tasks

### Clippy pedantic

- [ ] Fix `uninlined_format_args` across both crates — see [[backlog/clippy-pedantic-cleanup]]
- [ ] Fix `assigning_clones` → use `clone_from` where applicable
- [ ] Fix `manual_let_else` → use `let…else` pattern
- [ ] Fix `redundant_closure_for_method_calls`
- [ ] Fix `stable_sort_primitive` → `.sort_unstable()`
- [ ] Fix `cast_possible_wrap` / `cast_sign_loss` → `.cast_signed()` / `.cast_unsigned()`
- [ ] Fix `map_unwrap_or` → `.is_some_and()`
- [ ] Fix `unnecessary_wraps` in `mv.rs:121`

### API visibility

- [ ] Mark internal helpers as `pub(crate)` in hyalo-core — see [[backlog/pub-crate-visibility]]

### Scanner optimization

- [ ] Eliminate `FrontmatterCollector` clone of full `IndexMap` per file — see [[backlog/frontmatter-collector-clone]]

### Quality gate

- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo clippy --workspace --all-targets -- -W clippy::pedantic` produces fewer than 30 warnings
- [ ] `cargo test --workspace`
