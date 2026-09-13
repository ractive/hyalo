---
type: research
title: Dogfood v0.23.0 — focused release readiness
date: 2026-09-13
status: active
tags:
  - dogfooding
  - release
  - verification
related:
  - "[[dogfood-results/dogfood-v0230-2026-09-12]]"
  - "[[dogfood-results/dogfood-iteration-296-2026-09-12]]"
  - "[[iterations/iteration-296-review-followups]]"
---
# Dogfood v0.23.0 — focused release readiness

A deliberately small release check on 13 September 2026. No new product blocker
was found. The code looks ready for release preparation; choose a fresh version
across distribution channels, recommended **0.24.0**, because npm already shipped
0.23.0 from older source.

## Artifact and scope

- Commit: `adbafda6c7fc8422a738ca6a0496880c7182e67e`, synced with remote main.
- Built with `cargo build --release`; installed with `cargo install --locked
  --path crates/hyalo-cli --target-dir target --force`. All counted commands used
  `/Users/james/.cargo/bin/hyalo`, verified as `hyalo 0.23.0 (adbafda6c7fc 2026-09-13)`.
- SHA-256: `6ec31d207b541d864ed6e76b531ac2dfe2afbaadba83ec8b2d82349ad267ec27`.
- Own KB: 496 notes before this report. MDN: 14,375 notes at
  `44a5fa2aace490e0114349d9d683675b2f5cacce`.
- 41 recorded CLI commands with assertions covering ordinary usage, the five
  prior findings, preservation, indexed mutation, output preflight and real
  corpus parity. Additional discovery, release checks and report authoring are
  outside that count. External notes were read only; MDN's snapshot and all
  mutating fixtures stayed in `/tmp/hyalo-dogfood-20260913/`.

Baselines: [[dogfood-results/dogfood-v0230-2026-09-12]] and
[[dogfood-results/dogfood-iteration-296-2026-09-12]]. The latter contains the
13 September repair verification for [[iterations/iteration-296-review-followups]].

## Regression results

| Prior finding or workflow | Result |
| --- | --- |
| F1: managed-marker content loss | STILL FIXED. MADR preview preserved bytes; apply retained the BOM, CRLF prefix, inline marker example, handwritten paragraph and footer. Repeat apply was byte-identical. |
| F2: Pi ownership | STILL FIXED. Init/re-init/deinit preserved unrelated dependency, registration and helper bytes. An edited Hyalo artifact blocked re-init; deinit retained that artifact, registration and ownership receipt. |
| F3: unsafe emitted filenames | STILL FIXED. Indexed moves through `C#.md` and `Release (final).md` preserved an inline-code link example inside a real link label. Both actual links resolved to each new target with valid fragments and exact disk/index parity. Preview changed nothing. |
| F4: scaffold corruption | STILL FIXED. `[Draft]`, `{draft}`, a multiline string and numeric `+007` defaults retained their values and types. Preview equaled written content. Symlink-parent preview/apply both refused with the same diagnostic and no new file. |
| F5: fresh MDN ranked-index rejection | STILL FIXED. Fresh index and disk search returned identical results: 65 matches, five selected. `direct_indexed_scoring` was 1; no BM25 rebuild/fallback diagnostic appeared. |
| Indexed mutations | Set plus task toggle followed by `find --fields all` returned exactly equal disk/index file objects, including graph data. |
| Invalid jq before write | Append refused an invalid filter with exit 1 and unchanged note bytes. |
| Everyday own-KB usage | Summary, ranked search, saved planned view, broken-link listing and replay of an actual summary hint worked. |

No new product bug was demonstrated. Early harness attempts assumed MADR drift
previews exit 0, errors always use stdout, and false `broken_anchor` is always
present. Those assumptions were corrected to the established contracts: drift
exit 1, JSON errors on stderr, and omitted false fields. The complete successful
attempt is `pass4/commands.json`; earlier attempt evidence is retained separately.

## Release preparation findings

**A missing GitHub tag is insufficient evidence that a version is available.**
At this check, GitHub's latest release was 0.21.0, but
`npm view @ractive-ch/hyalo@0.23.0 version dist.integrity --json` confirmed an
existing immutable 0.23.0 package. The successful npm publication workflow ran
on 8 September at `73769bd76720669ba0688aaeff58b7c00479807e`, before these repairs.
See the [publication run](https://github.com/ractive/hyalo/actions/runs/34201309749).

The read-only release preflight for 0.23.0 passed on the initial clean tree.
However, `CHANGELOG.md` has both `[0.23.0] - 2026-09-08` and substantial
`[Unreleased]` content. `hyalo changelog release 0.23.0 --dry-run --format json`
correctly exits 1: `version [0.23.0] already exists in CHANGELOG.md` (the
actual error encloses the version in backticks). A 0.24.0 preview succeeds.

Before publishing the new work:

- Choose an unused version across the intended channels; 0.24.0 fits the
  unreleased features and breaking changes. Recheck registry state at release time.
- Align workspace, package and generated distribution versions using the
  repository's release/package workflow.
- Preserve the published 0.23.0 history, rotate Unreleased into the fresh version,
  and include the iteration-296 preservation, ownership, filename, scaffold and
  ranked-index fixes in curated notes.
- Reconcile the stale Unreleased comparison link, which currently points at
  `v0.17.0...HEAD`, against the actual GitHub tag history.

The release skill at `.claude/skills/release/SKILL.md` now explicitly requires
cross-channel version checks and describes the preflight's blind spots, per the
owner's request during this run. It distinguishes a same-artifact partial-release
retry from publishing new source under an already used version. The skill
validator passed. No version, changelog, tag or published package was changed.

## Knowledgebase health and UX

Strict lint: **zero errors, 15 MD012 blank-line warnings** in five existing
iteration notes; 156 checked and 340 ignored by configured exclusions. Summary
reported 82 orphans, 89 dead ends, zero unresolved file targets and 23 broken
anchors. The broken-link query selected 11 notes. These match the existing
backlog, not a new release regression. Contrary to the initial September 12
report's wording, the current 15 warnings are MD012, not open-task warnings.

The iteration-296 follow-up lacks the `dogfooding` tag, so the skill's tag-only
latest-report query misses it. Searching `dogfood-results/*` found it. This is
report metadata friction; the historical report was left unchanged.

## Performance spot checks

Single wall-time samples on the same Mac and unchanged MDN corpus; uncontrolled
cache/background load, not benchmark guarantees. No measured operation exceeded
2x the comparable previous-round sample.

| Operation | This round | Prior final repair round |
| --- | ---: | ---: |
| Own-KB summary | 0.060 s | 0.069 s |
| Own-KB ranked search | 0.271 s | 0.271 s |
| Own-KB strict lint | 0.190 s | 0.176 s |
| MDN fresh index | 2.857 s | 2.665 s |
| MDN ranked disk / indexed | 3.899 / 0.565 s | 3.885 / 0.571 s |
| MDN title filter disk / indexed | 0.586 / 0.301 s | 0.724 / 0.291 s |

The title filter matched the same 12 results in both modes. A limited metadata
listing also matched exactly. The indexed ranked query was approximately 6.9x
faster than disk and recorded the direct-scoring counter.

## Validation boundaries

The exact tested main commit has green formatting, Clippy, full-KB lint and native
macOS/Linux/Windows test jobs in [CI](https://github.com/ractive/hyalo/actions/runs/34771512092).
[PR 353](https://github.com/ractive/hyalo/pull/353) also passed its applicable
quality gates and npm launcher checks on all three platforms. This small round
used that existing CI evidence; it did not rerun the full Rust/npm suites or
perform a live Pi/LLM session, fuzzing, exhaustion, race tests or scale benchmarks.
Product source was unchanged. Release readiness is conditional on the ordinary
version/changelog preparation and checks on any resulting release commit.
