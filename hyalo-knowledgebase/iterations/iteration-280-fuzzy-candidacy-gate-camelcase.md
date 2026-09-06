---
type: iteration
title: Iteration 280 — Widen the fuzzy candidacy gate for camelCase/separator mismatches
date: 2026-09-06
status: completed
tags:
  - iteration
  - links
  - fuzzy-match
branch: iter-280/fuzzy-candidacy-gate-camelcase
priority: 3
related:
  - "[[iterations/iteration-279-fuzzy-scorer-near-neighbor-stems]]"
  - "[[decision-log]]"
---

# Iteration 280 — Widen the fuzzy candidacy gate for camelCase/separator mismatches

## Goal

Carried over from [[iterations/iteration-279-fuzzy-scorer-near-neighbor-stems]]'s "Not done"
section (DEC-324). Iteration 279 taught `link_score::basename_similarity` to tokenize
camelCase runs, so `MyNote` and `my-note` are now scored identically (1.0) once they reach the
composite scorer. They never do: `LinkMatcher::fuzzy_shortlist`'s *candidacy* gate — the cheap
filter that decides which files are even worth scoring against a broken target — is still a
raw, case-sensitive Jaro-Winkler over the unsplit stems (`crates/hyalo-core/src/link_fix.rs`,
around `fuzzy_shortlist`, iter-206/iter-212). `[[my-long-note]]` never shortlists
`MyLongNote.md`: the two raw strings score too low on stem Jaro-Winkler for `--threshold` to
admit the candidate, so the improved scorer never sees the pair.

This iteration is scoped to the candidacy gate only. It does not touch `basename_similarity`,
`token_similarity`, or any of DEC-324's three signals — those are correct and settled.

## Tasks

- [x] GATE-1: reproduce the gap with a minimal fixture (`MyLongNote.md` vs
      `[[my-long-note]]`, and `HTMLParser.md` vs `[[html-parser]]`) and confirm the shortlist
      never includes the correct file at the default `--threshold` (0.8 fuzzy floor per
      `crates/hyalo-cli/src/cli/args.rs`, `LinkMatcher::new`'s configured threshold in
      practice).
- [x] GATE-2: propose the cheapest fix that keeps `fuzzy_shortlist`'s per-target-stem cache
      valid — e.g. gate on a camelCase-tokenized comparison (reuse
      `link_score::tokenize`/`basename_similarity`, or a cheaper proxy) instead of/alongside
      the raw Jaro-Winkler prefilter — and confirm it does not change the shortlist size
      enough to regress the fuzzy_shortlist cache's iter-206 performance win on a large corpus
      (GitHub Docs `content/`, or the `bench-scale` synthetic vault).
- [x] GATE-3: implement, re-measure all three corpora used since iteration 212/277/279 (Hub,
      MDN, GitHub Docs) for new fuzzy proposals gained and lost, and record the result in a DEC
      naming what changed and what didn't.
- [x] GATE-4: `cargo test --workspace -q`, every xtask `check-*` gate, `hyalo lint --strict`
      on the knowledgebase.

## Acceptance criteria

- [x] `[[my-long-note]]` shortlists and fuzzy-matches `MyLongNote.md` at (or near) 1.0
      confidence — or a DEC records why widening the gate is not worth its cost (e.g. shortlist
      size blows up on a large vault) and this plan is closed without the change.
- [x] No regression in `fuzzy_shortlist` candidate-set size or measured `links fix` wall time
      on the `bench-scale` synthetic vault beyond a documented, justified margin.
- [x] No previously-correct fuzzy fix across the Hub, MDN, GitHub Docs or the iteration
      277/279 e2e fixtures changes score or drops below the apply floor as a side effect.
- [x] Gates green.

## Outcome

Implemented as [[decision-log]] DEC-325. `link_score::gate_key` is the candidacy gate's normal
form — `tokenize`'s words joined with `-` — and `LinkMatcher` precomputes one per file
(`gate_stems`) instead of the raw stem, so `fuzzy_shortlist` compares the same tokens
`basename_similarity` reads. The iter-206 per-stem shortlist cache is keyed on the normal form
too, which only makes it warmer (`MyNote` and `my-note` share an entry).

**GATE-1 — the gap, reproduced.** Raw Jaro-Winkler rates `my-long-note` / `MyLongNote` at 0.53
and `html-parser` / `HTMLParser` at 0.45, both far under the 0.8 `--threshold`, so `find_match`
returned `None` for both while `basename_similarity` rated each pair 1.0. Both are now unit
tests (`matcher_shortlists_across_camel_case_and_separators`) that assert the premise before
asserting the fix.

**GATE-2 — the cheapest fix.** One `strsim::jaro_winkler` call per file, exactly as before, over
strings precomputed at matcher build. The `-` join (rather than concatenating the tokens) is what
makes the change free where it matters: a plain lowercase-hyphen slug is its own gate key byte
for byte, so GitHub Docs, MDN and the `bench-scale` synthetic vault (`note-NNNNN.md`,
`linker-NNNNN.md`) compare exactly the strings they compared before. `xtask bench-scale` on the
14 000-file synthetic vault: `find` 330 ms (budget 3 s), `links fix` **1.05 s** (budget 15 s,
iter-278 baseline ~1.1 s), `mv` at 2 000 backlinks 522 ms — PASS, no regression.

**GATE-3 — measured on all three corpora** (`links fix --dry-run --format json`, this branch vs
`main`):

| corpus | fuzzy proposals | below floor | unfixable | delta |
| --- | --- | --- | --- | --- |
| GitHub Docs (`content/`) | 5476 → 5476 | 3302 → 3302 | 1875 → 1875 | byte-identical output |
| MDN (`files/en-us`) | 0 → 0 | 0 → 0 | 49784 → 49784 | byte-identical output |
| Obsidian Hub | 16 → 21 | 15 → 19 | 38 → 33 | +7 gained, −2 lost, 4 re-scored |

`broken` (54), `case_mismatches` (48), `alias_fixes` (8) and `relocations` (2) are unchanged on
the Hub. The two lost proposals were junk (~0.23, ~0.31); the four re-scored all moved *down*
and all stay below the floor. Wall time, median of 3: GitHub Docs 4.39 s → 4.46 s, MDN 2.02 s →
2.05 s, Hub 0.52 s → 0.49 s — inside run-to-run noise.

## Not done

**One new wrong above-floor proposal on the Hub.** `[[Mathjax]]` (no MathJax note exists there)
now offers `Plugins/mathpad.md` at 0.886, which `--apply-fuzzy` would write. It is the *scorer's*
verdict, not the gate's: both stems are one token, plain Jaro is 0.810 — under
`TOKEN_MATCH_FLOOR` — and the pair is admitted only by DEC-324's `shares_dominant_prefix`
exemption, because `math` is four of seven characters, just over half. Had the plugin been named
`Mathjax.md` the identical fix would already be offered today; the raw gate suppressed it purely
by the accident of one capital letter, and no normalisation that reaches `MyLongNote` can keep
that accident. Tightening `shares_dominant_prefix` for two whole words of *equal length* is a
scorer change (DEC-324's territory), explicitly out of this iteration's scope — carried over to
[[iterations/iteration-281-dominant-prefix-equal-length-exemption]].

## Links

- [[iterations/iteration-279-fuzzy-scorer-near-neighbor-stems]] — "Not done": names this exact
  gap and why it was left alone (separate change, separate cost)
- [[iterations/iteration-281-dominant-prefix-equal-length-exemption]] — carries this exact
  false positive forward
- [[decision-log]] — DEC-324 (basename scorer signals this gate would finally let through),
  DEC-325 (this iteration)
