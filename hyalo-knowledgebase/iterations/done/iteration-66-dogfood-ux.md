---
title: "Iteration 66: Dogfood UX improvements"
type: iteration
date: 2026-03-28
tags:
  - iteration
  - ux
  - dogfooding
status: completed
branch: iter-66/dogfood-ux
---

# Iteration 66: Dogfood UX improvements

## Context

Dogfooding v0.5.0 surfaced 4 UX issues. These are usability gaps — not bugs — but they affect real workflows. UX-1 (title filter) is the most impactful: users expect `--property title~=X` to match the displayed title, but it only searches frontmatter.

## UX issues

| # | Impact | Summary |
|---|--------|---------|
| 1 | HIGH | No way to filter by derived/display title (H1 fallback) |
| 2 | MEDIUM | No reverse sort (`--reverse` flag) |
| 3 | MEDIUM | `links fix` false positives on template/external paths |
| 4 | LOW | `hyalo links` help text doesn't mention `summary` / `find --broken-links` |

## Tasks

### UX-4: `links` help text (trivial, do first)

- [x] Add tip to `Links` command help pointing to `summary` and `find --broken-links`

### UX-2: `--reverse` flag for sort

- [x] Add `#[arg(long)] reverse: bool` to `Find` variant in main.rs
- [x] Thread `reverse` param through to `find()` and `find_from_index()` in find.rs
- [x] After `apply_sort()`, call `results.reverse()` when flag is set
- [x] Disable short-circuit optimization when `--reverse` is active
- [x] Add e2e tests: `--sort title --reverse`, `--reverse --limit`, `--reverse` alone

### UX-1: `--title` filter flag

- [x] Add `#[arg(long)] title: Option<String>` to `Find` variant in main.rs
- [x] Force `fields.title = true` when `--title` is given (same pattern as `--broken-links`)
- [x] After outline sections computed (line 242), extract title and match against pattern
- [x] Pattern matching: case-insensitive substring by default, `~=` prefix for regex
- [x] `continue` on mismatch (same pattern as property filters at line 235)
- [x] Add e2e tests: H1 match, case-insensitive, regex mode, frontmatter title match

### UX-3: `--ignore-target` for `links fix`

- [x] Add `#[arg(long)] ignore_target: Vec<String>` to `LinksAction::Fix` in main.rs
- [x] Filter broken links before passing to `LinkMatcher::plan_fixes()` in links.rs
- [x] Report ignored count in output: `"ignored": N`
- [x] Add e2e tests: substring match, multiple patterns (OR), Hugo template skip

### Quality gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] Manual smoke tests on both doc directories

## Design details

### UX-4: `links` help text
**File:** `crates/hyalo-cli/src/main.rs:853`

Add `#[command(long_about = ...)]` to the `Links` variant:
```
TIP: For read-only auditing, use 'hyalo summary' (link health overview)
or 'hyalo find --broken-links' (list files with unresolved links).
```

### UX-2: `--reverse` flag
**Files:** `crates/hyalo-cli/src/main.rs` (near line 528), `crates/hyalo-cli/src/commands/find.rs`

1. Add clap arg to `Find` in main.rs
2. Thread through to `find()` and `find_from_index()` in find.rs
3. After `apply_sort(&mut results, ...)`, add `if reverse { results.reverse(); }`
4. Disable short-circuit optimization when `--reverse` is active (need all results before reversing + limiting)

Note: `backlinks_count` and `links_count` already sort descending. With `--reverse` they become ascending. Correct and expected.

### UX-1: `--title` filter flag
**Files:** `crates/hyalo-cli/src/main.rs`, `crates/hyalo-cli/src/commands/find.rs`

1. Add clap arg to `Find` in main.rs
2. Force `fields.title = true` when `--title` given (line 155 pattern)
3. This ensures `SectionScanner` runs (line 176 already checks `fields.title`)
4. After outline sections computed (line 242), extract title via `extract_title(&props, outline_sections.as_deref())` and match
5. Case-insensitive substring default, `~=` prefix or `/regex/` for regex mode
6. `continue` on mismatch — runs per-file in scan loop, efficient

### UX-3: `--ignore-target` for `links fix`
**Files:** `crates/hyalo-cli/src/main.rs:860-873`, `crates/hyalo-cli/src/commands/links.rs`, `crates/hyalo-core/src/link_fix.rs`

1. Add `--ignore-target` flag (repeatable) to `Fix` subcommand — substring match against broken link target
2. Filter before passing to `LinkMatcher::plan_fixes()` — simplest approach, no core logic changes
3. Report `"ignored": N` in output

## Files to modify

| File | UX issues |
|------|-----------|
| `crates/hyalo-cli/src/main.rs` | 1, 2, 3, 4 (clap args + help text) |
| `crates/hyalo-cli/src/commands/find.rs` | 1, 2 (title filter + reverse) |
| `crates/hyalo-cli/src/commands/links.rs` | 3 (ignore pattern filtering) |
