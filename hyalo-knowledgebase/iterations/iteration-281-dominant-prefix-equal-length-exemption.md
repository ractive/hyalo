---
type: iteration
title: >-
  Iteration 281 — Tighten the dominant-prefix exemption for equal-length word
  pairs
date: 2026-09-06
status: planned
tags: [iteration, links, fuzzy-match]
branch: iter-281/dominant-prefix-equal-length-exemption
priority: 3
related:
  - "[[iterations/iteration-280-fuzzy-candidacy-gate-camelcase]]"
  - "[[iterations/iteration-279-fuzzy-scorer-near-neighbor-stems]]"
  - "[[decision-log]]"
---

# Iteration 281 — Tighten the dominant-prefix exemption for equal-length word pairs

## Goal

Carried over from [[iterations/iteration-280-fuzzy-candidacy-gate-camelcase]]'s "Not done"
section (DEC-325). Widening the fuzzy *candidacy* gate in iteration 280 did not change the
scorer — it only let more pairs reach it — and doing so exposed a scorer defect that iteration
280 deliberately left alone: on the Obsidian Hub, `[[Mathjax]]` (no such note exists) now offers
`Plugins/mathpad.md` at confidence **0.886**, high enough for `--apply-fuzzy` to write it.

The false match is DEC-324's `shares_dominant_prefix` exemption. `mathjax` and `mathpad`
tokenise to one token each; plain Jaro between them is 0.810, *below*
`TOKEN_MATCH_FLOOR` (0.85) — DEC-324's own floor for treating two tokens as the same word — but
the pair is admitted anyway because `shares_dominant_prefix` only checks that the common prefix
covers at least half of the shorter token: `math` is 4 of `mathjax`'s 7 characters and 4 of
`mathpad`'s 7, so both clear "half" and the exemption fires. The exemption exists for genuine
prefix relationships (`get`/`getting`), where one token actually extends the other; two
*different* words that happen to share a prefix are not that case, and the gate widened in
iteration 280 now lets this specific shape of mismatch through where it used to be filtered out
by accident, not by design.

This iteration is scoped to `shares_dominant_prefix` (or an equivalent tightening of the
single-token match path in `crates/hyalo-core/src/link_score.rs`) only. It does not reopen
iteration 280's candidacy-gate change, DEC-319's margin damping, or the multi-token scoring path
that iteration 279 already settled.

## Tasks

- [ ] TASK-1: reproduce `[[Mathjax]]` → `mathpad.md` at 0.886 with a minimal fixture (no
      corpus needed — a two-file vault with `Plugins/mathpad.md` and a broken `[[Mathjax]]`
      link reproduces it), and confirm the premise: plain Jaro(`mathjax`, `mathpad`) is 0.810,
      below `TOKEN_MATCH_FLOOR`, and `shares_dominant_prefix("mathjax", "mathpad")` is what
      admits the pair anyway.
- [ ] TASK-2: design the narrower rule. Candidates to weigh against each other (pick one, or a
      combination, and record why the others were rejected):
      - Require the common prefix to cover a *larger* share of the shorter token than "at least
        half" — high enough that `math`/`mathjax` (4/7) and `math`/`mathpad` (4/7) both fail
        while `get`/`getting` (3/7) is judged on its own merits, not assumed to pass either.
      - Require the *lengths* to differ by more than some minimum, so two tokens of equal or
        near-equal length (`mathjax` vs `mathpad`, both 7) are never treated as a prefix
        relationship — a real prefix case is definitionally shorter-extends-into-longer.
      - Require the *suffix* after the shared prefix to be a plausible continuation (e.g. the
        shorter token is empty past the prefix — `get` fully consumed by `getting` — rather
        than both tokens having their own distinct remainder, `jax` vs `pad`).
      Whichever is chosen, it must keep `get`/`getting`, `plugin`/`plugins` and other iteration
      279/324 fixtures passing — re-run their existing tests, don't just eyeball the rule.
- [ ] TASK-3: implement, then re-measure the Obsidian Hub, MDN and GitHub Docs the same way
      iteration 280 did (`links fix --dry-run --format json`, this branch vs `main`) and record
      what moved. The target outcome is `[[Mathjax]]` dropping out of `fuzzy_fixes` (or moving
      below the apply floor) with no previously-correct fuzzy fix anywhere losing confidence or
      dropping below the floor as a side effect.
- [ ] TASK-4: `cargo test --workspace -q`, every xtask `check-*` gate, `hyalo lint --strict` on
      the knowledgebase.

## Acceptance criteria

- [ ] `[[Mathjax]]` no longer fuzzy-matches `mathpad.md` above the apply floor — either it drops
      out of `fuzzy_fixes` entirely or lands below `below_floor`, on the Hub fixture from TASK-1.
- [ ] Every iteration 279/324 `shares_dominant_prefix` / `TOKEN_MATCH_FLOOR` fixture (`get` /
      `getting`, and any other passing prefix-relationship test in
      `crates/hyalo-core/src/link_score.rs`) still passes unchanged.
- [ ] No previously-correct fuzzy fix across the Hub, MDN, GitHub Docs or the iteration
      277/279/280 e2e fixtures changes score or drops below the apply floor as a side effect.
- [ ] Gates green.

## Not done

(to be filled in when this iteration closes)

## Links

- [[iterations/iteration-280-fuzzy-candidacy-gate-camelcase]] — "Not done": names this exact
  false positive and why it was left alone (widening candidacy is not a scorer change)
- [[iterations/iteration-279-fuzzy-scorer-near-neighbor-stems]] — DEC-324, origin of
  `shares_dominant_prefix` and `TOKEN_MATCH_FLOOR`
- [[decision-log]] — DEC-324 (the exemption this iteration tightens), DEC-325 (the gate change
  that exposed it)
