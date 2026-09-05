---
type: iteration
title: Iteration 279 — Fuzzy scorer for near-neighbor stems
date: 2026-09-05
status: planned
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

- [ ] SCORE-1: reproduce all three examples on a Hub copy and inspect exactly what the
      current composite score rewards — stem overlap, length ratio, token boundary, edit
      distance component. Identify which sub-score is responsible for each over-generous
      match (`Cat`/`CatMuse` likely a substring/prefix bonus; `jamesb`/`jamesgreenblue`
      already closed by DEC-319; `paulbricman`/`paultreanor` and `…toc-plugin`/`…plugin-toc`
      likely a token-set-overlap bonus scoring reordered/partial token matches too highly).
- [ ] SCORE-2: propose one additional scoring signal (e.g. penalize a match where the
      candidate's own extra/reordered tokens are not a subset of the query's, or weight
      whole-token equality above substring/prefix overlap) and prototype it against the three
      examples plus a regression corpus of known-correct fuzzy fixes (reuse the existing
      `link_fix.rs` unit tests and the iteration 277 e2e fixtures) to confirm no correct match
      newly drops below floor.
- [ ] SCORE-3: implement the signal behind the same `LinkMatcher::find_match` path DEC-319
      used, record it as a DEC (margin only, or margin + the new signal), and re-measure all
      four named examples plus the Hub/MDN corpora for new false positives or negatives.
- [ ] SCORE-4: `cargo test --workspace -q`, every xtask `check-*` gate, `hyalo lint --strict`
      on the knowledgebase.

## Acceptance criteria

- [ ] `[[Cat]]`, `[[paulbricman]]` and `[[obsidian-floating-toc-plugin]]` fall below the 0.8
      apply floor on the Hub corpus (`[[jamesb]]` already does, per DEC-319) — or a DEC records
      why no signal closes them without a new false negative, and names the corpora checked.
- [ ] No previously-correct fuzzy fix (Hub, MDN, or the iteration 277 e2e fixtures) drops
      below the apply floor as a side effect.
- [ ] Gates green.

## Links

- [[iterations/iteration-277-link-graph-parity-and-write-performance]] — FIX-2, DEC-319, the
  four named examples and their measured scores
- [[decision-log]] — DEC-319 (contested-match margin)
