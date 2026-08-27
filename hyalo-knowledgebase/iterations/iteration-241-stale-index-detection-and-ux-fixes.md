---
type: iteration
title: "Iteration 241 — stale-index detection for links fix, UX fixes"
date: 2026-08-27
status: in-progress
tags: [iteration]
branch: iter-241/stale-index-detection-and-ux-fixes
---

# Iteration 241 — stale-index detection for `links fix`, UX fixes

## Goal

Close the top four carry-over items from the
[[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]]
backlog (as recorded in
[[iterations/iteration-240-review-followups-bugfixes]]): BUG-2's detection
half (DEC-241), the UX-1 `--file <glob>` hint lie, UX-2's unreachable
zero-padded/archived iterations, and UX-4's truncation-hidden lint errors.

## Tasks

- [ ] DEC-241 — stale-index detection for `--index` mutations: mtime-check
      indexed entries before the `links fix` / `links auto` discovery pass,
      rescan drift from disk, warn; record the decision in
      [[decision-log]]
- [ ] UX-1 — every "run `hyalo find --file <glob>`" hint says `--glob`
      instead (`--file` does not glob)
- [ ] UX-2 — `--iteration` also matches zero-padded IDs and files in
      subdirectories of the template's directory (`--iteration 2` reaches
      `iterations/done/iteration-02-links.md`)
- [ ] UX-4 — lint sorts error-carrying files first so a display cap can
      never hide them, and hints at the errors hidden by truncation; MD011
      false positives on regex prose like `(3rd|[Tt]hird)[-_]` are
      suppressed when the "text" part contains regex metacharacters
- [ ] E2e tests for every changed command surface (`links fix`,
      `links auto`, `find --file`/`--glob` hint, `--iteration` resolution,
      `lint` ordering/hint, MD011 suppression)
- [ ] Keep help text, COMMAND REFERENCE, CHANGELOG [Unreleased] in sync;
      xtask gates green (`check-help-drift`, `check-command-reference`,
      `check-bundled-skills`, `check-mutation-journal`)
- [ ] `cargo fmt` / `clippy -D warnings` / `cargo test --workspace` clean

## Acceptance criteria

- [ ] BUG-2 repro from the dogfood report: append a broken `[[…]]` to an
      indexed file with an editor, `links fix --apply --apply-fuzzy
      --index` → the link is discovered (broken > 0) and fixed, with a
      stale-index warning on stderr
- [ ] `hyalo set nonexistent.md …` hint suggests `find --glob`, and the
      suggested command actually works for a glob
- [ ] `hyalo read --iteration 2` resolves `iterations/done/iteration-02-*.md`
      in this vault; `find --iteration 2` lists it
- [ ] A vault with 4 errors in files ranked below 50 shows them in the
      default listing (errors-first sort) or a hidden-errors hint

## Non-goals

- UX-3 (nested YAML dot-path filters), UX-5 (`read --iteration` on a
  body-less file), UX-6 (MDN case-insensitive resolve), BUG-4/5 (BM25 /
  backlink order parity) — stay in the carry-over list unless trivially
  cheap
- No release — v0.21.0 is ON HOLD
