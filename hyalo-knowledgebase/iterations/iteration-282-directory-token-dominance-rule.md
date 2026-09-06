---
type: iteration
title: "Iteration 282 — Give the directory-similarity feature its own dominant-prefix rule"
date: 2026-09-06
status: planned
tags: [iteration, links, fuzzy-match]
branch: iter-282/directory-token-dominance-rule
priority: 3
related:
  - "[[iterations/iteration-281-dominant-prefix-equal-length-exemption]]"
  - "[[iterations/iteration-279-fuzzy-scorer-near-neighbor-stems]]"
  - "[[decision-log]]"
---

# Iteration 282 — Give the directory-similarity feature its own dominant-prefix rule

## Goal

Carried over from [[iterations/iteration-281-dominant-prefix-equal-length-exemption]]'s "Not
done" section (DEC-326), and from that plan's third acceptance criterion, left unticked on
purpose. DEC-326 tightened `shares_dominant_prefix` — the exemption that lets a token pair count
as one word on Jaro-Winkler even though plain Jaro puts it under `TOKEN_MATCH_FLOOR` — so that
the shared prefix must outweigh its leftover by more than a factor of two. That closed the
`[[Mathjax]]` → `mathpad.md` false match, but the same function is reached from
`link_score`'s *directory*-similarity feature, not only the basename one, and one real
GitHub Docs relocation regressed as a result:
`saml-configuration-reference` → `admin/managing-iam/iam-configuration-reference/…` fell
**0.804 → 0.795**, just under the 0.8 apply floor, because `management`/`managing`
(`manag` + leftover `5`/`3`) no longer clears the new factor-of-two bar.

Iteration 281 deliberately did not open this question — its own "Not done" section says so:
*"Whether directory tokens want their own, looser admission rule … is a real question this
iteration did not open."* A directory level is renamed wholesale far more often than a filename
word is inflected (`managing-iam` for the whole component, not a grammatical form of one word),
so the basename's morphological standard may simply be the wrong standard for a directory
segment. This iteration is scoped to answering that question — and only that question.

## Tasks

- [ ] TASK-1: confirm the regression still stands on GitHub Docs at HEAD (main, post iter-281):
      `saml-configuration-reference`'s directory rename `management` → `managing` scores below
      the apply floor solely because of `shares_dominant_prefix`'s DEC-326 tightening. Build a
      minimal two-file fixture (mirroring iter-281's e2e style) that reproduces the exact
      before/after confidence numbers from DEC-326's writeup (0.804 → 0.795) without needing the
      full GitHub Docs corpus.
- [ ] TASK-2: decide whether `link_score`'s directory-similarity path should call
      `shares_dominant_prefix` at all, or a directory-specific variant with its own dominance
      factor (or no floor at all, given DEC-324's basename-only character charge already treats
      directories more leniently in other ways). Weigh at least:
      - A separate, looser `PREFIX_DOMINANCE`-equivalent for directory tokens only.
      - No prefix exemption for directories at all — score every directory token pair on plain
        Jaro/Jaro-Winkler alone, accepting whatever that does to `management`/`managing`.
      - Leaving basename and directory sharing one rule, and closing this as "the 0.004-margin
        relocation was never safely applicable" instead of changing the code.
      Whichever is chosen, re-run every iter-279/280/281 fixture that touches
      `shares_dominant_prefix` or directory scoring and confirm none regresses.
- [ ] TASK-3: implement the decision, then re-measure the Obsidian Hub, MDN and GitHub Docs the
      same way iter-281 did (`links fix --dry-run --format json`, this branch vs `main`) and
      record what moved — specifically whether `saml-configuration-reference` returns above the
      floor and whether `[[Mathjax]]`/`mathpad` (the pair DEC-326 closed) stays closed.
- [ ] TASK-4: `cargo test --workspace -q`, every xtask `check-*` gate, `hyalo lint --strict` on
      the knowledgebase.

## Acceptance criteria

- [ ] The directory-similarity feature's admission rule for a shared-prefix token pair is a
      documented, deliberate choice (a looser factor, no exemption, or "unchanged, and here is
      why") rather than an accidental side effect of a basename-motivated change.
- [ ] Every iter-279/280/281 fixture (`get`/`getting`, `[[Mathjax]]`/`mathpad`, the
      `shares_dominant_prefix` unit tests) still passes unchanged.
- [ ] The chosen rule is measured on all three corpora (Hub, MDN, GitHub Docs) against `main`,
      with the `saml-configuration-reference` outcome explicitly reported either way.
- [ ] Gates green.

## Links

- [[iterations/iteration-281-dominant-prefix-equal-length-exemption]] — "Not done": names this
  exact open question and the regression that motivates it (DEC-326)
- [[iterations/iteration-279-fuzzy-scorer-near-neighbor-stems]] — DEC-324, origin of the
  basename/directory split in `link_score` and the character-charge asymmetry between them
- [[decision-log]] — DEC-324, DEC-326
