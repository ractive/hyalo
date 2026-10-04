---
title: "Iteration 309: review leftovers"
type: iteration
date: 2026-10-04
tags: [iteration, review, lint, performance]
status: in-progress
branch: iter-309/review-leftovers
---

# Iteration 309: review leftovers

Three findings left over from the post-306 review round.

## Tasks

- [ ] List every broken heading anchor (`lint --strict --rule HYALO008`, `find --broken-links`)
- [ ] Repair the anchors with `links fix` where it proposes a safe repair, by hand otherwise
- [ ] Fix wikilink fragments losing their inline code spans (DEC-348) with unit tests
- [ ] Route the scanner's oversized-file skip through the warning facility
- [ ] Sweep hyalo-core and hyalo-cli for other raw `eprintln!` advisories
- [ ] e2e test: oversized file with and without `-q`
- [ ] Count gitignore-dropped files in `summary` without a second walk (DEC-349)
- [ ] Measure disk `summary` on MDN (best of three, release build)
- [ ] CHANGELOG `[Unreleased]` Fixed bullets, DEC-342 amended if its wording changes

## Acceptance criteria

- [ ] `hyalo lint --strict` on the whole vault exits 0
- [ ] The oversized-file skip is shown once without `-q` and silenced by `-q`
- [ ] Disk `summary` on MDN is within 2 % of 1.55 s
- [ ] The iteration 306 e2e tests (gitignore_named_file, summary parity) pass
- [ ] All gates green: fmt, clippy, tests, xtask gates, cargo deny

## Notes
