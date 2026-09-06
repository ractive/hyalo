---
type: iteration
title: "Iteration 282 — Give the directory-similarity feature its own dominant-prefix rule"
date: 2026-09-06
status: completed
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

- [x] TASK-1: confirm the regression still stands on GitHub Docs at HEAD (main, post iter-281):
      `saml-configuration-reference`'s directory rename `management` → `managing` scores below
      the apply floor solely because of `shares_dominant_prefix`'s DEC-326 tightening. Build a
      minimal two-file fixture (mirroring iter-281's e2e style) that reproduces the exact
      before/after confidence numbers from DEC-326's writeup (0.804 → 0.795) without needing the
      full GitHub Docs corpus.
- [x] TASK-2: decide whether `link_score`'s directory-similarity path should call
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
- [x] TASK-3: implement the decision, then re-measure the Obsidian Hub, MDN and GitHub Docs the
      same way iter-281 did (`links fix --dry-run --format json`, this branch vs `main`) and
      record what moved — specifically whether `saml-configuration-reference` returns above the
      floor and whether `[[Mathjax]]`/`mathpad` (the pair DEC-326 closed) stays closed.
- [x] TASK-4: `cargo test --workspace -q`, every xtask `check-*` gate, `hyalo lint --strict` on
      the knowledgebase.

## Acceptance criteria

- [x] The directory-similarity feature's admission rule for a shared-prefix token pair is a
      documented, deliberate choice (a looser factor, no exemption, or "unchanged, and here is
      why") rather than an accidental side effect of a basename-motivated change.
- [x] Every iter-279/280/281 fixture (`get`/`getting`, `[[Mathjax]]`/`mathpad`, the
      `shares_dominant_prefix` unit tests) still passes unchanged.
- [x] The chosen rule is measured on all three corpora (Hub, MDN, GitHub Docs) against `main`,
      with the `saml-configuration-reference` outcome explicitly reported either way.
- [x] Gates green.

## Links

- [[iterations/iteration-281-dominant-prefix-equal-length-exemption]] — "Not done": names this
  exact open question and the regression that motivates it (DEC-326)
- [[iterations/iteration-279-fuzzy-scorer-near-neighbor-stems]] — DEC-324, origin of the
  basename/directory split in `link_score` and the character-charge asymmetry between them
- [[decision-log]] — DEC-324, DEC-326

## Outcome

Implemented as [[decision-log]] DEC-329. Directory tokens use a strict majority
prefix: the common prefix must outweigh the leftover in the shorter token
(factor one). Basenames retain DEC-326's factor two, and both features retain
the 0.85 Jaro-Winkler floor. DEC-329 weighs the separate looser rule against
plain-Jaro/no-exemption admission, unrestricted Winkler admission and leaving
one shared rule unchanged.

The two-file SAML fixture reproduces **0.795 → 0.8037** (displayed as **0.804**),
exactly the regression in DEC-326. It verifies dry-run confidence and floor
reporting, plain apply withholding, fuzzy apply rewriting, and below-floor
reporting at a 0.805 threshold, on both disk and index. Additional fixtures cover
inline Markdown/wikilink emission and protected code/reference bytes, strict
majority boundaries, Unicode counting and the retained Winkler floor. The
existing iteration 279/280/281 fixtures are unchanged.

### Corpus comparison

Baseline is `main` at `bfa47325328f997f6f0352d41448367a1f1a2b2a`, built before
source edits and copied aside. Candidate uses this iteration's working tree.
Both identify as `hyalo 0.22.0 (bfa47325328f+dirty 2026-09-06)`; baseline was
only dirty because this plan had entered `in-progress`. Binary SHA-256:

- Baseline: `7bddf4ef6cd21813bd4970356fe9bc387f6e96884c2b9c2aea472f00b4be7d77`
- Candidate: `8d3233354907aed7439bc09a8911736b1aa5f1cf32c3db7cd71067dbdc5c9f63`

Commands match iteration 281: each binary runs
`--dir <corpus> links fix --dry-run --format json`, three times serially per
corpus, without index or writes. Corpora are `/Users/james/devel/obsidian-hub`,
`/Users/james/devel/mdn/files/en-us` and `/Users/james/devel/docs/content`.
Full JSON, stderr/timings, binary copies, corpus revisions/status and the
comparison script are retained under
`.git/ralph-loop/run-20260906-284-287/282/`. Corpus heads and working-tree status
are unchanged; each set of three outputs is byte-identical.

| Corpus | Fuzzy proposals | Above floor | Unfixable | Median wall time |
| --- | --- | --- | --- | --- |
| Obsidian Hub | 18 → 18 | 1 → 1 | 36 → 36 | 0.47 → 0.48 s |
| MDN (historical defaults) | 0 → 0 | 0 → 0 | 49784 → 49784 | 1.89 → 1.80 s |
| GitHub Docs | 5483 → 5479 | 2168 → 2174 | 1868 → 1872 | 4.41 → 4.45 s |

Hub output is byte-identical, so `[[Mathjax]]`/`mathpad` remains closed. On
GitHub Docs, exactly the six SAML occurrences rise across the floor. No
previously applicable proposal disappears or loses confidence, and no proposal
crosses downward. There are 377 re-scored proposals: 373 rise and four fall;
the four falling ones remain below the floor. Four proposals disappear, all
previously below the floor at 0.039–0.133. No proposal is gained. Broken counts
and deterministic case/alias/relocation results are unchanged.

**MDN limitation and extra check.** The historical command's derived `en-us`
prefix fails to resolve 49767 site-absolute links and skips their fuzzy scoring;
its zero proposals are not evidence of scorer quality. An additional baseline
and candidate run with `--site-prefix en-US/docs` is also byte-identical:
522 broken, 521 unfixable and one applicable fuzzy proposal at 0.822461243
(`Global_attributes/tabindex` → `global_attributes/index.md`). All 14375 MDN
Markdown files are named `index.md`; the correctly configured run still
exercises that one proposal. The Hub's existing one-file frontmatter warning
and MDN's default-prefix warnings are preserved in the captures.

### Validation

`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, then
`cargo test --workspace -q` passed sequentially: **4872 passed, 0 failed**, with
two existing ignored doctest usage examples (`warn.rs` and
`scanner/body_state.rs`). Candidate release build passed. Every discovered
`xtask check-*` command exited 0: feature-fanout, help-drift,
command-reference, bundled-skills, pi-package-sync, codex-package, jq-recipes,
dead-primitives, todo-annotations and mutation-journal. The dead-primitives and
todo-annotations commands explicitly report that they are **unimplemented
stubs**, so their zero exits establish no coverage.

Full `hyalo lint --strict` exits 0 with zero errors and four existing
`HYALO002` warnings in iterations 270, 271, 277 and 281 (completed plans with
unchecked work). Changed-file strict lint reports no findings.
`git diff --check` passes. Initial new-fixture assumptions about index flag
position, wikilink slash emission and inert reference definitions were corrected
before the final complete gate run; no existing fixture was weakened.

Independent read-only review found no actionable defects. On 2026-09-06 the
supervisor reconciled every upcoming pending plan: 283, 285, 286, 287 and 288.
No plan adaptation was needed: this change affects link-repair directory scoring,
with no JSON shape, ranked-search, package or integration contract change.
The external requirements in 286/287 remain required, and 288 desktop verification
remains explicitly deferred.
