---
title: "Iteration 309: review leftovers"
type: iteration
date: 2026-10-04
tags: [iteration, review, lint, performance]
status: completed
branch: iter-309/review-leftovers
---

# Iteration 309: review leftovers

Three findings left over from the post-306 review round.

## Tasks

- [x] List every broken heading anchor (`lint --strict --rule HYALO008`, `find --broken-links`)
- [x] Repair the anchors with `links fix` where it proposes a safe repair, by hand otherwise
- [x] Fix wikilink fragments losing their inline code spans (DEC-348) with unit tests
- [x] Route the scanner's oversized-file skip through the warning facility
- [x] Sweep hyalo-core and hyalo-cli for other raw `eprintln!` advisories
- [x] e2e test: oversized file with and without `-q`
- [x] Count gitignore-dropped files in `summary` without a second walk (DEC-349)
- [x] Measure disk `summary` on MDN (best of three, release build)
- [x] CHANGELOG `[Unreleased]` Fixed bullets, DEC-342 amended if its wording changes

## Acceptance criteria

- [x] `hyalo lint --strict` on the whole vault exits 0
- [x] The oversized-file skip is shown once without `-q` and silenced by `-q`
- [x] Disk `summary` on MDN is within 2 % of 1.55 s
- [x] The iteration 306 e2e tests (gitignore_named_file, summary parity) pass
- [x] All gates green: fmt, clippy, tests, xtask gates, cargo deny

## Notes

- **Broken anchors.** All 23 broken links were bare `#DEC-NNN` prefixes of
  decision-log headings (19 reported by lint, the rest in lint-ignored
  files). `links fix --dry-run` deferred every one as advisory, so they were
  rewritten to the `suggested_fragment` by hand. The headings that carry
  inline code exposed a real bug: wikilink text was read from the
  code-blanked line ([[decision-log#DEC-348: A wikilink's inner text is read from the original line (2026-10-04)]]).
  The slug generation in `anchor.rs` was not involved.
- **Advisories.** Oversized-file skips (scanner, `links fix`, `links auto`,
  `mv`), the out-of-vault symlink skip and `mv`'s skipped-link notes go
  through `hyalo_core::warn::advisory` or the CLI's `warn`/`note`. Kept
  `-q`-proof on purpose: snapshot refusals, walk errors, write failures, batch
  `mv`'s stderr-only ambiguous-link warning, and the two core warnings that
  must never be silent.
- **Single-pass count.** [[decision-log#DEC-349: `summary` counts gitignore drops in the walk that finds the files (2026-10-04)]].
  MDN `files/en-us`, release build, best of three:

  | command | before | after |
  |---|---|---|
  | `summary` (disk) | 1.75 s | 1.04 s |
  | `create-index` | 2.95 s | 2.64 s |

  The baseline is this branch before the change, which still had the
  iteration 306 extra walk. The 1.55 s reference predates that walk. The
  machine's load average was 6 to 12 during measurement.
- **Dogfooding.** `hyalo append --property 'related=[[note#Heading, with comma]]'`
  writes a nested YAML list, because a `[[…]]` value containing a comma is
  read as a flow list (documented in `set --help`). There is no way to force
  a string, so the one frontmatter link was edited directly.
