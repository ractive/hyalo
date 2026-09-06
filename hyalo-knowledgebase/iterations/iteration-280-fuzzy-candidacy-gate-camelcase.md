---
type: iteration
title: Iteration 280 — Widen the fuzzy candidacy gate for camelCase/separator mismatches
date: 2026-09-06
status: planned
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

- [ ] GATE-1: reproduce the gap with a minimal fixture (`MyLongNote.md` vs
      `[[my-long-note]]`, and `HTMLParser.md` vs `[[html-parser]]`) and confirm the shortlist
      never includes the correct file at the default `--threshold` (0.8 fuzzy floor per
      `crates/hyalo-cli/src/cli/args.rs`, `LinkMatcher::new`'s configured threshold in
      practice).
- [ ] GATE-2: propose the cheapest fix that keeps `fuzzy_shortlist`'s per-target-stem cache
      valid — e.g. gate on a camelCase-tokenized comparison (reuse
      `link_score::tokenize`/`basename_similarity`, or a cheaper proxy) instead of/alongside
      the raw Jaro-Winkler prefilter — and confirm it does not change the shortlist size
      enough to regress the fuzzy_shortlist cache's iter-206 performance win on a large corpus
      (GitHub Docs `content/`, or the `bench-scale` synthetic vault).
- [ ] GATE-3: implement, re-measure all three corpora used since iteration 212/277/279 (Hub,
      MDN, GitHub Docs) for new fuzzy proposals gained and lost, and record the result in a DEC
      naming what changed and what didn't.
- [ ] GATE-4: `cargo test --workspace -q`, every xtask `check-*` gate, `hyalo lint --strict`
      on the knowledgebase.

## Acceptance criteria

- [ ] `[[my-long-note]]` shortlists and fuzzy-matches `MyLongNote.md` at (or near) 1.0
      confidence — or a DEC records why widening the gate is not worth its cost (e.g. shortlist
      size blows up on a large vault) and this plan is closed without the change.
- [ ] No regression in `fuzzy_shortlist` candidate-set size or measured `links fix` wall time
      on the `bench-scale` synthetic vault beyond a documented, justified margin.
- [ ] No previously-correct fuzzy fix across the Hub, MDN, GitHub Docs or the iteration
      277/279 e2e fixtures changes score or drops below the apply floor as a side effect.
- [ ] Gates green.

## Links

- [[iterations/iteration-279-fuzzy-scorer-near-neighbor-stems]] — "Not done": names this exact
  gap and why it was left alone (separate change, separate cost)
- [[decision-log]] — DEC-324 (basename scorer signals this gate would finally let through)
