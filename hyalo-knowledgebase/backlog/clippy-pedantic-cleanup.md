---
title: "Address ~150 clippy pedantic warnings"
type: backlog
date: 2026-03-29
status: planned
origin: codebase review 2026-03-29
priority: low
tags:
  - code-quality
  - refactor
---

## Problem

`cargo clippy -- -W clippy::pedantic` reports ~150 warnings across both crates. None are errors under the default lint level, but the most actionable ones improve code quality.

## Key categories (by impact)

1. **`uninlined_format_args`** — `format!("… {}", x)` → `format!("… {x}")` (many sites)
2. **`assigning_clones`** — `.props = props.clone()` → `clone_from` (scanner.rs:737 and 8 CLI sites)
3. **`manual_let_else`** — `match` → `let…else` (index.rs, link_fix.rs, tasks.rs, hints.rs)
4. **`redundant_closure_for_method_calls`** — `.map(|s| s.to_string())` → `.map(ToString::to_string)` (many sites)
5. **`stable_sort_primitive`** — `.sort()` → `.sort_unstable()` for String vecs (link_graph.rs, find.rs tests)
6. **`cast_possible_wrap` / `cast_sign_loss`** — use `.cast_signed()` / `.cast_unsigned()` (index.rs, summary.rs)
7. **`map_unwrap_or`** — `.map(f).unwrap_or(false)` → `.is_some_and(f)` (filter.rs)
8. **`unnecessary_wraps`** — `mv.rs:121` `validate_target` returns `Result<Result<…>>` unnecessarily
9. **`missing_errors_doc` / `missing_panics_doc`** — many public fns lack `# Errors` / `# Panics` sections
10. **`must_use_candidate`** — many public fns should be `#[must_use]`

## Acceptance criteria

- [ ] Categories 1-8 addressed (mechanical fixes)
- [ ] `cargo clippy -- -W clippy::pedantic` produces fewer than 30 warnings
- [ ] All existing tests pass
