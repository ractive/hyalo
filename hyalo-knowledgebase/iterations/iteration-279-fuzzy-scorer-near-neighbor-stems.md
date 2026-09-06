---
type: iteration
title: Iteration 279 — Fuzzy scorer for near-neighbor stems
date: 2026-09-05
status: completed
tags:
  - iteration
  - links
  - fuzzy-match
  - dogfooding
branch: iter-279/fuzzy-scorer-near-neighbor-stems
priority: 3
related:
  - "[[iterations/iteration-277-link-graph-parity-and-write-performance]]"
  - "[[decision-log]]"
---

# Iteration 279 — Fuzzy scorer for near-neighbor stems

## Goal

Carried over from [[iterations/iteration-277-link-graph-parity-and-write-performance]] FIX-2 /
DEC-319. DEC-319's runner-up-margin damping closes one of the four wrong above-floor fuzzy
proposals the post-batch-271-274 dogfood report named: `[[jamesb]]` → `jamesgreenblue.md`
fell from 0.885 to 0.337. The other three did **not** move even at a widened 0.10 margin,
because their runner-up is not a near-tie — it is simply absent or far away:

| target | wrong winner | score | why the margin can't catch it |
|---|---|---|---|
| `[[Cat]]` | `CatMuse.md` | 0.867 | five `cat*` notes exist, but none scores close enough to `CatMuse` to trigger the margin |
| `[[paulbricman]]` | `paultreanor.md` | 0.855 | same shape — a genuinely wrong candidate scored too high on its own, not narrowly ahead of a right one |
| `[[obsidian-floating-toc-plugin]]` | `obsidian-plugin-toc.md` | 0.857 | same |

DEC-319 recorded this as a **scorer** problem, not a margin problem: widening the margin
until it reaches these three would also damp genuinely unique matches elsewhere, since the
runner-up gap is not what is wrong here — the *absolute* score is too generous for how little
these targets actually share. This iteration is scoped to the scorer itself.

Rule: do not simply lower the confidence floor or drop these three examples from a test —
show a scoring signal that pulls all three below floor while `[[Obsidian Publish.]]` →
`Obsidian Publish.md` (1.0, a real match) and every other correct fuzzy fix in the Hub and
MDN corpora used for iteration 277 keeps its confidence. If no such signal exists without
new false negatives, record that in a DEC and close the plan rather than force a fit.

## Tasks

- [x] SCORE-1: reproduce all three examples on a Hub copy and inspect exactly what the
      current composite score rewards — stem overlap, length ratio, token boundary, edit
      distance component. Identify which sub-score is responsible for each over-generous
      match (`Cat`/`CatMuse` likely a substring/prefix bonus; `jamesb`/`jamesgreenblue`
      already closed by DEC-319; `paulbricman`/`paultreanor` and `…toc-plugin`/`…plugin-toc`
      likely a token-set-overlap bonus scoring reordered/partial token matches too highly).
- [x] SCORE-2: propose one additional scoring signal (e.g. penalize a match where the
      candidate's own extra/reordered tokens are not a subset of the query's, or weight
      whole-token equality above substring/prefix overlap) and prototype it against the three
      examples plus a regression corpus of known-correct fuzzy fixes (reuse the existing
      `link_fix.rs` unit tests and the iteration 277 e2e fixtures) to confirm no correct match
      newly drops below floor.
- [x] SCORE-3: implement the signal behind the same `LinkMatcher::find_match` path DEC-319
      used, record it as a DEC (margin only, or margin + the new signal), and re-measure all
      four named examples plus the Hub/MDN corpora for new false positives or negatives.
- [x] SCORE-4: `cargo test --workspace -q`, every xtask `check-*` gate, `hyalo lint --strict`
      on the knowledgebase.

## Acceptance criteria

- [x] `[[Cat]]`, `[[paulbricman]]` and `[[obsidian-floating-toc-plugin]]` fall below the 0.8
      apply floor on the Hub corpus (`[[jamesb]]` already does, per DEC-319) — or a DEC records
      why no signal closes them without a new false negative, and names the corpora checked.
- [x] No previously-correct fuzzy fix (Hub, MDN, or the iteration 277 e2e fixtures) drops
      below the apply floor as a side effect.
- [x] Gates green.

## Links

- [[iterations/iteration-277-link-graph-parity-and-write-performance]] — FIX-2, DEC-319, the
  four named examples and their measured scores
- [[decision-log]] — DEC-319 (contested-match margin)

## Outcome

A scoring signal does exist. Three of them, all inside the **basename** feature
of `link_score`, recorded as [[decision-log#DEC-324: near-neighbour stems are a scorer problem, closed in the basename feature (2026-09-06)|DEC-324]]:

1. **camelCase is a word boundary.** `CatMuse` → `["cat", "muse"]`, so `[[Cat]]`
   is a name missing a word rather than a typo of one opaque token.
2. **Jaro admits, Winkler only sharpens.** The 0.85 token floor is measured
   against plain Jaro unless one token is a prefix of the other. A shared given
   name (`paul…`) buys a Winkler bonus, not token identity.
3. **An unmatched token costs its character share.** `explained_mass` scales the
   token F1 by the fraction of the two names' characters the pairing accounts
   for, so a dropped *word* is not absorbed by a forgiving harmonic mean.

Plus one correction to DEC-319: a winner scored an exact 1.0 is exempt from the
contested-margin damping. camelCase tokenisation made this reachable —
`ObsidianPublisher.md` rose from 0.596 to 0.978 against `[[Obsidian Publish.]]`,
which damped the *correct* 1.0 winner to 0.444.

### Measured (`links fix --dry-run`, before → after)

| corpus | fuzzy proposals | above floor |
|---|---|---|
| Obsidian Hub | 18 → 14 | **4 → 1** |
| GitHub Docs (`content/`) | 5 493 → 5 409 | 2 226 → 2 225 |
| MDN (`files/en-us`) | 0 → 0 | 0 → 0 |

All three named examples are closed: `[[Cat]]`'s winner moved from `CatMuse.md`
(0.867) to `CattailNu.md` (0.481), `[[paulbricman]]` is no longer proposed at
all, `[[obsidian-floating-toc-plugin]]` fell to 0.694 — and
`[[Obsidian Publish.]]` → `Obsidian Publish.md` keeps its 1.0. The Hub's only
applicable fuzzy fix is now the correct one.

MDN contributes nothing either way: all 14 375 of its files are `index.md`, so
no basename ever clears the candidacy gate and it produces zero fuzzy proposals
at any threshold. GitHub Docs — the corpus the scorer was built on in
iteration 212 — was substituted as the third regression corpus and is what
caught the one real mistake in this iteration (see below).

### What the corpora caught

Charging unmatched mass in `directory_similarity` as well as the basename took
**828** GitHub Docs fixes whose basename matched byte-for-byte below the apply
floor. A directory reorganisation renames whole levels — that *is* the move
being described — so the charge is now the basename's alone.

### Known narrowing

A basename that gains a whole word scores below the floor: `decision-log` →
`decision-log-archive` is 0.607, not 0.8. Still reported in `fuzzy_fixes`;
`--min-confidence 0.5` applies it. This is the intended trade and is what the
one dropped GitHub Docs fix was.

### Not done

The fuzzy *candidacy* gate is still a case-sensitive Jaro-Winkler over raw
stems, so `[[my-long-note]]` never shortlists `MyLongNote.md` even though the
scorer now rates that pair 1.0. Widening the gate is a separate change with its
own cost and was left alone.
