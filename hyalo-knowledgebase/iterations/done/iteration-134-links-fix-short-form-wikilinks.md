---
title: Iteration 134 — `links fix` must respect Obsidian short-form wikilinks
type: iteration
date: 2026-05-11
status: completed
branch: iter-134/links-fix-short-form-wikilinks
tags:
  - iteration
  - bug-fix
  - links
  - obsidian-compat
related:
  - "[[iterations/done/iteration-123-auto-link]]"
  - "[[iterations/done/iteration-124-auto-link-refinements]]"
---

## Goal

Stop `hyalo links fix` from destructively rewriting valid Obsidian
short-form wikilinks. A bare `[[Corina]]` that resolves (case-insensitively)
to some `**/Corina.md` anywhere in the vault is **not broken** and must
not be reported as a `case_mismatch` nor expanded to a full path
(`[[Bilas/Archive/Corina]]`). Obsidian resolves short-form links by stem
across the whole vault — hyalo must preserve that form.

## Context

While dogfooding against a real Obsidian vault, `hyalo links fix --apply`
reported 257 "case-mismatch" findings on links of the form `[[Corina]]`,
`[[Daphne]]`, etc., that Obsidian had been resolving correctly to files
in subfolders like `Bilas/Archive/Corina.md`. Applying the fix rewrote
100+ files to full-path form, violating the vault's convention
(`[[Wikilink Form]]` short-form for people/notes).

Two distinct defects:

1. **Misclassification**: a short-form link (no `/` in the target) that
   has at least one case-insensitive stem match anywhere in the vault is
   a valid Obsidian link, not a broken one. It must not appear in
   `broken` or `case_mismatches`.
2. **Destructive fix shape**: even when a genuine case fix is warranted,
   the rewrite must preserve the *form* of the original link. A
   short-form target stays short-form (`[[corina]]` → `[[Corina]]`),
   never expands to a path (`[[Bilas/Archive/Corina]]`). Path expansion
   is only acceptable when the user opts in explicitly.

## Scope

In scope:

- `crates/hyalo-core/src/links/` resolver and `links fix` classifier
- `[links] case_insensitive` interaction with stem-only targets
- Reporting (`broken`, `case_mismatches`) and the `--apply` rewriter
- New opt-in flag for path expansion (off by default)

Out of scope:

- `hyalo links auto` (separate code path; already respects existing links)
- General refactor of the link resolver
- Changing the `.hyalo.toml` default for `case_insensitive`

## Tasks

- [x] Reproduce the bug with a minimal fixture: a file at
  `sub/Corina.md` and a link `[[Corina]]` in a root-level file; assert
  `links fix` reports 0 broken and 0 case-mismatches
- [x] Add a `resolve_short_form` helper that scans the vault index for
  any file whose stem matches the target (case-insensitively when
  enabled) and returns all matches
- [x] Update the classifier so a short-form target (no `/`) is valid
  iff `resolve_short_form` returns ≥1 match; only flag broken when 0
  matches
- [x] Update `case_mismatch` detection to only fire when the *stem*
  casing differs from the on-disk file — never when only the path
  differs
- [x] Update the `--apply` rewriter to preserve link form: short stays
  short, path stays path; only the segment that actually differs is
  rewritten
- [x] Add `--expand-short-form` opt-in flag for users who *do* want
  bare stems expanded to full paths (and document it as
  Obsidian-incompatible)
- [x] Handle ambiguous short-form targets (≥2 stem matches): leave the
  link alone and emit an `ambiguous` finding in the report (do not
  auto-pick a path)
- [x] Update `hyalo links fix --help` long-form text to describe
  short-form handling and the new flag
- [x] Add e2e tests: short-form valid, short-form case-mismatch on
  stem only, short-form ambiguous, path-form case-mismatch (still
  rewritten), `--expand-short-form` behavior
- [x] Update README and `crates/hyalo-cli/templates/rule-knowledgebase.md`
  if `links fix` examples need adjusting

## Acceptance criteria

- [x] Running `hyalo links fix` on a vault containing `[[Corina]]` →
  `sub/Corina.md` reports 0 broken, 0 case-mismatches, 0 ambiguous
- [x] Running `hyalo links fix --apply` on the same vault changes
  **zero files**
- [x] A genuine stem-case mismatch (`[[corina]]` on disk as `Corina.md`)
  is rewritten to `[[Corina]]`, **not** `[[sub/Corina]]`
- [x] Two `Corina.md` files in different folders produce an `ambiguous`
  report entry; `--apply` leaves the link untouched
- [x] `--expand-short-form` opts into the old behavior and is
  documented as Obsidian-incompatible
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace -q` all pass
- [x] Dogfood: re-run `hyalo links fix` against the user's Obsidian
  vault and confirm the 257 false positives are gone
