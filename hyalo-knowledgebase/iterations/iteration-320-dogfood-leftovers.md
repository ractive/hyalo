---
title: "Iteration 320: regression dogfood leftovers"
type: iteration
date: 2026-11-05
tags: [iteration, dogfooding, links, release, hints, init]
status: completed
branch: iter-320/dogfood-leftovers
priority: 2
related: ["[[backlog/regression-dogfood-2026-10-05-leftovers]]", "[[iterations/iteration-318-regression-dogfood-fixes]]", "[[iterations/iteration-319-link-ambiguity-field]]", "[[decision-log]]"]
---

# Regression dogfood leftovers

## Problem

[[backlog/regression-dogfood-2026-10-05-leftovers]] collected the findings of the
2026-10-05 regression dogfood that [[iterations/iteration-318-regression-dogfood-fixes]]
did not fix, plus what cutting 0.25.0 and 0.25.1 turned up. The list is a month old
and the code has moved since (serde-saphyr 2.1 / DEC-375, snapshot v8, DEC-374), so
every item was reproduced on a fresh build before it was touched. Each ends in one of
four outcomes: fixed with a test, already fixed, decided against with a DEC, or left
open in the backlog with what was learned.

DEC numbers: **DEC-376 to DEC-380**.

## Tasks

The two inconsistencies between commands, first:

- [x] `links fix` JSON `fixable: 0` next to text `Fixable: 5`: the text line prints the sum and spells out the split (`Fixable: 5 (0 links + 5 anchor repairs)`); JSON keeps `fixable` as the link bucket (DEC-376)
- [x] `find --broken-links` and `links fix` choose the same heading for a prefix-vs-numbered anchor: one `anchor::suggest_heading`, numbered repair first (DEC-376)

Deferred from iteration 318:

- [x] `mv` leaves a bare `[[Café]]` alone when the target differs only in Unicode composition (DEC-380)
- [x] `new --dry-run` hints the real `=> hyalo new --type T --file F [writes]` (DEC-380)

Other findings:

- [x] Backslash links: `backlinks` and `find` disagree on `back\slash` — reproduced and recorded in the backlog with the cause; fix left open (link resolution core, risky)
- [x] YAML 1.2 core signed hex/octal and `.NaN` — already fixed by serde-saphyr 2.1 and DEC-364/DEC-375; pinned by an e2e test; `.NaN` keeps the documented `".nan"` string fallback (DEC-377)
- [x] `types list` / `types show` under an unusable `[schema]` say the schema is unusable (exit 1) instead of "type not found" (DEC-380)
- [x] `init`: an identical re-run reports `unchanged`; a hand-edited managed block is replaced with a warning (DEC-380)
- [x] `read --frontmatter` on a note without frontmatter warns instead of printing nothing (DEC-380)
- [x] `lint-rules show SCHEMA` `autofixable` — already fixed (reports `false`; pinned by an e2e test)
- [x] `find --facet` compact text output — decided against: `--fields file --limit 1` already prints just the facet block (DEC-377)
- [x] Hint key order — decided against: JSON objects are alphabetical by design (DEC-377)
- [x] HYALO006 no longer counts links that walk above the vault root (DEC-380)
- [x] `--index-file <missing>` hint names `--allow-outside-vault` when the path is outside the vault (DEC-380)
- [x] The in-memory repair note says how many files were modified, added or deleted (DEC-380)
- [x] `lint --rule X -q` no longer prints unknown-markdownlint-rule warnings for unrelated directives (DEC-380)
- [x] `--jq` output over 10 MiB exits 1 naming `--format json` / a narrower filter on a read-only run; a run that already wrote keeps exit 2 with its effects (DEC-378)
- [x] Disk `summary` has no `source` key — decided against: the key reports a fallback, and there is none to report (DEC-377)
- [x] No-op `create-index` on MDN takes 1.09 s — measured (0.78 s user + 2.5 s sys, 550 MB) and recorded in the backlog with the candidate causes; fix left open

Found while cutting 0.25.0 and 0.25.1:

- [x] `changelog release` writes `[X.Y.Z]: …/compare/vPREV...vX.Y.Z` and `[Unreleased]: …/compare/vX.Y.Z...HEAD`; unit and e2e tests; `docs/releasing.md` no longer says to check for `TBD` (DEC-379)
- [x] `generate-npm-packages --generate` rewrites `npm/hyalo/package-lock.json` (root version, pins, platform `version`/`resolved`) and `--check` fails on a stale one; a moved platform entry loses its stale `integrity`; the seven 0.25.1 entries got their registry `integrity` (DEC-379)
- [x] The release skill and `docs/releasing.md` list all 16 files a version bump touches (DEC-379)

Housekeeping:

- [x] CHANGELOG, DEC-376 to DEC-380, help text and skill text in the same PR
- [x] Gates: fmt, clippy, tests, every xtask gate, `generate-npm-packages --check`, `cargo deny check`, `hyalo lint`

## Acceptance criteria

- [x] Every item of the backlog note has an outcome above; the two left open are still listed in the backlog
- [x] Each behaviour change has an e2e test in `tests/e2e/iteration320_dogfood_leftovers.rs` (or the unit/xtask test named in the DEC)
- [x] No new CLI flag or subcommand; exit codes stay 0 / 1 / 2 (DEC-307)

## Outcome

23 backlog items plus the lockfile `integrity` addition: 16 fixed with tests, 2
already fixed (YAML core spellings, `lint-rules show SCHEMA`), 3 decided against
(DEC-377), 2 reproduced and left open in [[backlog/regression-dogfood-2026-10-05-leftovers]]
(backslash links, no-op `create-index` cost). DEC-376 to DEC-380. e2e:
`tests/e2e/iteration320_dogfood_leftovers.rs`. No snapshot format change, no new flag.
