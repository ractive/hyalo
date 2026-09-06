---
type: iteration
title: Iteration 281 — Tighten the dominant-prefix exemption for equal-length word pairs
date: 2026-09-06
status: in-progress
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

- [x] TASK-1: reproduce `[[Mathjax]]` → `mathpad.md` at 0.886 with a minimal fixture (no
      corpus needed — a two-file vault with `Plugins/mathpad.md` and a broken `[[Mathjax]]`
      link reproduces it), and confirm the premise: plain Jaro(`mathjax`, `mathpad`) is 0.810,
      below `TOKEN_MATCH_FLOOR`, and `shares_dominant_prefix("mathjax", "mathpad")` is what
      admits the pair anyway.
- [x] TASK-2: design the narrower rule. Candidates to weigh against each other (pick one, or a
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
- [x] TASK-3: implement, then re-measure the Obsidian Hub, MDN and GitHub Docs the same way
      iteration 280 did (`links fix --dry-run --format json`, this branch vs `main`) and record
      what moved. The target outcome is `[[Mathjax]]` dropping out of `fuzzy_fixes` (or moving
      below the apply floor) with no previously-correct fuzzy fix anywhere losing confidence or
      dropping below the floor as a side effect.
- [x] TASK-4: `cargo test --workspace -q`, every xtask `check-*` gate, `hyalo lint --strict` on
      the knowledgebase.

## Acceptance criteria

- [x] `[[Mathjax]]` no longer fuzzy-matches `mathpad.md` above the apply floor — either it drops
      out of `fuzzy_fixes` entirely or lands below `below_floor`, on the Hub fixture from TASK-1.
- [x] Every iteration 279/324 `shares_dominant_prefix` / `TOKEN_MATCH_FLOOR` fixture (`get` /
      `getting`, and any other passing prefix-relationship test in
      `crates/hyalo-core/src/link_score.rs`) still passes unchanged.
- [ ] No previously-correct fuzzy fix across the Hub, MDN, GitHub Docs or the iteration
      277/279/280 e2e fixtures changes score or drops below the apply floor as a side effect.
      **Not fully met, deliberately.** MDN is byte-identical and no above-floor proposal
      disappears anywhere, but one correct GitHub Docs relocation crosses the floor:
      `saml-configuration-reference` falls 0.804 → 0.795 because the *directory* feature no
      longer counts `management`/`managing` as one token. It was 0.004 above the floor to begin
      with, it is still reported, and the alternative costs more than it buys — see "Not done".
- [x] Gates green.

## Outcome

`shares_dominant_prefix` now requires the shared prefix to be **more than twice** what it
leaves over of the shorter token, where it previously required only that the prefix cover at
least *half* of it — recorded as
[[decision-log#DEC-326: a dominant prefix must outweigh what it leaves over (2026-09-06)|DEC-326]].
`referen` (7) against `ce` (2) qualifies; `math` (4) against `pad` (3) does not.

**TASK-1 — premise confirmed.** A two-file vault (`Plugins/mathpad.md` plus a broken
`[[Mathjax]]`) reproduces the Hub's 0.886 exactly. Plain Jaro(`mathjax`, `mathpad`) is 0.810,
under `TOKEN_MATCH_FLOOR`; Jaro-Winkler is 0.886; and `math` is four of seven characters on
*both* sides, so the old "at least half" bar was cleared and the exemption alone admitted the
pair. Both facts are asserted in the unit test before the fix is asserted.

**TASK-2 — the rule, and the two rejected candidates.** A prefix relationship is one word
carrying on into another, so the prefix is weighed against what it fails to account for rather
than against a fixed budget. That scaling is what the first implementation lacked: requiring
the shorter token to be spent to within *one* character is clean for the gerund cases and blind
to word length — it rejects `reference`/`referential` and with it GitHub Docs'
`referential-content-type` → `reference-content-type` rename, which fell **0.973 → 0.484** when
measured. *Raising the share* to three quarters was rejected as a threshold moved until the
known counter-example fell the right side of it; *requiring the lengths to differ* was rejected
as testing a symptom, since one letter of slack (`mathjax`/`mathpads`) restores the false
positive. The comparison is strict — a prefix that exactly doubles its leftover has not
dominated it — which also drops `excalidraw`/`excalibur`; nothing real sits at that boundary,
because a pair too weak to clear the floor on Jaro-Winkler (`use`/`using`, 0.751) never reaches
the function.

**TASK-3 — measured on all three corpora** (`links fix --dry-run --format json`, this branch vs
`main`):

| corpus | fuzzy proposals | above floor | unfixable | delta |
| --- | --- | --- | --- | --- |
| Obsidian Hub | 21 → 18 | 2 → 1 | 33 → 36 | 3 lost, 0 gained, 0 re-scored |
| GitHub Docs (`content/`) | 5476 → 5483 | 2174 → 2168 | 1875 → 1868 | 34 gained, 26 lost, 431 re-scored |
| MDN (`files/en-us`) | 0 → 0 | 0 → 0 | 49784 → 49784 | byte-identical output |

The single above-floor proposal the Hub loses is exactly the target, `[[Mathjax]]` →
`mathpad.md` at 0.886; the other two losses were junk below the floor (~0.26, ~0.46). What
remains above the floor there is one perfect 1.0 (`Obsidian Publish.` → `Obsidian Publish.md`).
`broken` (54), `case_mismatches` (48), `alias_fixes` (8) and `relocations` (2) are unchanged. On
GitHub Docs **no** above-floor proposal disappears and none moves *up* across the floor.

**TASK-4 — gates.** `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace -q`, every `xtask check-*` gate, and `hyalo lint --strict` on this
knowledgebase.

## Not done

**One correct GitHub Docs relocation is now reported rather than applied.**
`saml-configuration-reference` →
`admin/managing-iam/iam-configuration-reference/saml-configuration-reference.md` (6 occurrences
of one link) falls **0.804 → 0.795**, just under the apply floor. Its basename is identical, so
the whole movement is in the *directory* feature, where `management` / `managing` (`manag` plus
a leftover of 3) no longer counts as one token. It was 0.004 above the floor to begin with —
applicable by a margin narrower than this change — and it is still listed in `fuzzy_fixes`, and
applied by `--min-confidence 0.79`. Admitting `management`/`managing` would need the dominance
factor below 5/3, at which point `mathjax`/`mathpad` clears by half a character; a rule whose
only counter-example survives by that margin buys nothing.

**The directory feature shares the token scorer.** Every judgement here was made on basename
evidence, but `shares_dominant_prefix` is reached from the directory similarity too — which is
precisely how the one regression above arose. Whether directory tokens want their own, looser
admission rule (a directory level is renamed wholesale far more often than a filename word is
inflected) is a real question this iteration did not open.

**`excalidraw`/`excalibur` is a boundary, not a principle.** Exactly-double is rejected because
dominance should mean beating, not tying, and because it happens to drop a real false pair. No
corpus evidence distinguishes strict from non-strict beyond that one pair.

## Links

- [[iterations/iteration-280-fuzzy-candidacy-gate-camelcase]] — "Not done": names this exact
  false positive and why it was left alone (widening candidacy is not a scorer change)
- [[iterations/iteration-279-fuzzy-scorer-near-neighbor-stems]] — DEC-324, origin of
  `shares_dominant_prefix` and `TOKEN_MATCH_FLOOR`
- [[decision-log]] — DEC-324 (the exemption this iteration tightens), DEC-325 (the gate change
  that exposed it)
