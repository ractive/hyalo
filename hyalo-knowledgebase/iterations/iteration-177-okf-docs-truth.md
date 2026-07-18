---
title: "Iteration 177 — OKF skill/docs truth (spaced types, stale help, hints)"
type: iteration
date: 2026-07-18
status: planned
branch: iter-177/okf-docs-truth
tags: [iteration, okf, documentation, skills]
depends-on: "[[iterations/iteration-176-okf-generator-hardening]]"
related:
  - "[[dogfood-results/dogfood-v0180-final-pre-release]]"
---

# Iteration 177 — OKF skill/docs truth

## Goal

Every claim the bundled okf skill, the help texts, and the README make
about the OKF toolchain is true. Fixes **BUG-4** (the third recommended
pre-release fix: the skill teaches a `types set` invocation the CLI
rejects) and all doc mismatches from
[[dogfood-results/dogfood-v0180-final-pre-release]]. Depends on iter-176
because it documents the generator behavior that iteration finalizes.

## Tasks

### 1. Spaced type names end-to-end (BUG-4, MEDIUM-HIGH)

- [ ] Relax `types set` name validation to accept names with spaces
  (anything valid as a quoted TOML key and safe as a frontmatter `type`
  value) — hand-declared `[schema.types."Data Table"]` already works end
  to end, so the CLI restriction is the odd one out
- [ ] e2e: `types set "Data Table" --required type,title` →
  `new --type "Data Table"` → `lint` round-trip passes
- [ ] Re-verify the bundled okf skill's "Adding domain types" walkthrough
  works command-for-command (its example uses `type: BigQuery Table`);
  the `check-bundled-skills` xtask gate should catch future drift if
  extensible to command claims — note feasibility either way

### 2. Stale help texts

- [ ] `--site-prefix` help (all subcommands): replace "pass
  `--site-prefix \"\"` to disable absolute-link resolution entirely" with
  the actual semantics (empty prefix = resolve absolute links from the
  vault/bundle root)
- [ ] Document the dry-run drift exit-code contract (1 = drift, 2 = error)
  in `okf index --help` for CI users

### 3. README accuracy

- [ ] Conflict-line format: update README to the actual output
  (`warning: conflict: <key> <old> -> <new> (profile <name>)`) or change
  the output to match the documented form — pick one
- [ ] Document that directories with no concepts and no subdirectories get
  no `index.md` and are omitted from the parent's `Subdirectories` list
- [ ] Re-verify the "pure Markdown link list" claim against iter-176's
  link escaping (should become true; keep the wording)
- [ ] No action needed — verified accurate during iter-176's PR review: a
  BEGIN marker nested between another BEGIN and its paired END is now
  correctly classified `Duplicate` (previously silently spliced as
  `Healthy`, deleting the nested marker), and `resolve_log_target`'s
  parent-must-exist check now covers both the bare-directory *and* the
  explicit `<dir>/log.md` target forms. The README's "duplicated" /
  "nonexistent directory target is rejected consistently" wording already
  covered both cases correctly — implementation caught up to the docs, no
  doc change required. See the iter-176 PR for the two regression tests.

### 4. okf_version enforcement gap

- [ ] Decide: add an okf lint rule flagging extra frontmatter keys on the
  bundle-root `index.md` (spec says `okf_version` and nothing else) and
  frontmatter on nested reserved files — or soften the README/skill
  wording to match the permissive generator. Record the decision in the
  decision log

### 5. okf hints (research-doc claim)

- [ ] okf commands emit standard drill-down hints in text mode (e.g.
  `-> hyalo lint --profile okf` after `okf index --apply`), honoring
  `--no-hints`
- [ ] Move the non-standard JSON `results.hint` string into the standard
  `hints` envelope array
- [ ] Update `research/okf-open-knowledge-format.md` if the final hint set
  differs from what it describes

### 6. Retrospective

- [ ] Sync README, help texts, knowledgebase docs, and bundled skills in
  this same PR (keep-docs-in-sync rule); update remaining planned
  iterations with anything learned

## Acceptance Criteria

- [ ] Grep-audit: no remaining doc claim contradicted by observed behavior
  from the dogfood report's "Documentation Mismatches" section
- [ ] Bundled okf skill passes a command-by-command execution check
- [ ] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
