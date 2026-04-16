---
title: "Iteration 120 — Dogfood v0.12.0 Post-iter-119 Fixes"
type: iteration
date: 2026-04-16
status: planned
branch: iter-120/dogfood-post-iter119-fixes
tags:
  - iteration
  - dogfooding
  - bug-fix
  - ux
  - dry-run
related:
  - "[[dogfood-results/dogfood-v0120-post-iter119]]"
---

# Iteration 120 — Dogfood v0.12.0 Post-iter-119 Fixes

## Goal

Address the bugs and UX issues found in the post-iter-119 dogfood session. The two
highest-priority findings are missing `--dry-run` support on bulk mutation commands
(`properties rename`, `tags rename`) and `mv --dry-run` not previewing link rewrites.
Also includes two small UX wins carried over from post-iter-118.

## Bugs

### BUG-1: `mv --dry-run` doesn't preview link rewrites (MEDIUM)

`decision-log.md` has 14 backlinks. Running `mv decision-log.md --to reference/decision-log.md --dry-run`
reports `total_files_updated: 0, total_links_updated: 0, updated_files: []`. The dry-run
should scan for backlinks and show which files and links would be rewritten, so the user
can assess impact before applying.

**Fix**: In the `mv` dry-run path, still compute link rewrites (scan for backlinks, resolve
rewrite targets) but skip the actual file writes. Populate `total_files_updated`,
`total_links_updated`, and `updated_files` with the preview data.

### BUG-2: `properties rename` and `tags rename` lack `--dry-run` (MEDIUM)

Both commands perform bulk mutations across all matching files with no way to preview.
`properties rename --from origin --to source --dry-run` fails with `unexpected argument '--dry-run' found`.

Every other mutation command (`set`, `remove`, `append`, `task toggle`, `mv`, `links fix`)
supports `--dry-run`. The rename commands are the only gap.

**Fix**: Add `--dry-run` flag to both `properties rename` and `tags rename`. In dry-run mode,
scan files and report which would change, but don't write. Text output should show file
paths and the rename preview (e.g., `origin → source`). JSON output should include
`dry_run: true` and the list of affected files.

### BUG-3 (carried over): `--fields outline` is not a valid field name (LOW)

`find --fields outline` errors. `outline` is a natural alias for `sections` (which shows
heading structure). Adding it as an alias improves discoverability.

**Fix**: Accept `outline` as an alias for `sections` in the `--fields` parser.

### BUG-4 (carried over): `--stemmer` doesn't accept ISO 639-1 codes (LOW)

`--stemmer en` fails with "unknown stemming language". Only full names like `english` are
accepted. Two-letter codes are the natural form for many users.

**Fix**: Map common ISO 639-1 codes to Snowball stemmer names: `en` → `english`,
`de` → `german`, `fr` → `french`, `es` → `spanish`, `it` → `italian`, `pt` → `portuguese`,
`nl` → `dutch`, `sv` → `swedish`, `no` → `norwegian`, `da` → `danish`, `fi` → `finnish`,
`hu` → `hungarian`, `ro` → `romanian`, `tr` → `turkish`, `ru` → `russian`, `ar` → `arabic`.

## UX Improvements

### UX-1: `create-index` should note when overwriting (LOW)

Currently `create-index` silently overwrites an existing `.hyalo-index`. Adding a brief
note (e.g., `"note": "replaced existing index"`) in the JSON output helps users
understand what happened.

### UX-2: `lint --fix` hint for unfixable parse errors (LOW)

When `lint --fix --dry-run` encounters an unfixable error like unclosed frontmatter, the
hint says "See defined type schemas" which isn't helpful. A better hint would suggest
adding the file to `[lint] ignore` in `.hyalo.toml`.

## Tasks

### BUG-1: `mv --dry-run` link rewrite preview
- [ ] Refactor mv command to compute link rewrites before deciding to write
- [ ] Populate `updated_files` and counters in dry-run output
- [ ] Add text-format output showing affected files and link counts
- [ ] Add e2e test: mv --dry-run on file with backlinks shows non-zero counts

### BUG-2: `--dry-run` for `properties rename` and `tags rename`
- [ ] Add `--dry-run` flag to `properties rename` clap definition
- [ ] Add `--dry-run` flag to `tags rename` clap definition
- [ ] Implement dry-run logic: scan and report without writing
- [ ] Add text-format output for dry-run preview
- [ ] Add e2e tests for both commands with `--dry-run`

### BUG-3: `--fields outline` alias
- [ ] Accept `outline` as alias for `sections` in field parsing
- [ ] Add e2e test

### BUG-4: `--stemmer` ISO 639-1 codes
- [ ] Add ISO 639-1 to Snowball name mapping
- [ ] Update help text to mention both forms
- [ ] Add e2e test for `--stemmer en`

### UX improvements
- [ ] `create-index`: note when overwriting existing index
- [ ] `lint --fix` hint: suggest `[lint] ignore` for unfixable parse errors

### Documentation surfaces (keep all in sync)
- [ ] Update help texts for changed commands
- [ ] Update CHANGELOG
- [ ] Update knowledgebase skill if needed
- [ ] Update README if needed

### Quality gates
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace -q`

## Acceptance Criteria

- [ ] `mv --dry-run` on a file with backlinks shows non-zero `total_files_updated` and `total_links_updated`
- [ ] `properties rename --dry-run` previews affected files without writing
- [ ] `tags rename --dry-run` previews affected files without writing
- [ ] `find --fields outline` works as alias for `--fields sections`
- [ ] `find --stemmer en` works (maps to English stemmer)
- [ ] `create-index` output notes when replacing existing index
- [ ] All existing tests still pass
