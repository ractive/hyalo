---
type: iteration
title: "Iteration 207 — post mdbook-lint 0.16.0 follow-up (upstream #456 comment, MD047 CRLF issue)"
date: 2026-08-22
status: superseded
tags:
  - iteration
  - upstream
  - mdbook-lint
  - carry-over
branch: iter-207/post-mdbook-lint-0160-followup
related:
  - "[[docs/upstream-mdbook-lint-reports]]"
  - "[[iterations/iteration-196-mdlint-workaround-strip]]"
  - "[[iterations/iteration-194-post-upstream-mdbook-lint-reports]]"
---

# Iteration 207 — post mdbook-lint 0.16.0 follow-up (upstream #456 comment, MD047 CRLF issue)

## Superseded — 2026-08-23 disposition

Renumbered 207 → 207a to free the number for
[[iterations/iteration-207-inert-zone-completion]] (release blocker; the
`iteration-207-*` plan glob must be unambiguous). Substance resolved the
day after filing:

- The MD047 CRLF issue was filed, user-authorized, as
  [joshrotenberg/mdbook-lint#495](https://github.com/joshrotenberg/mdbook-lint/issues/495).
- The #456 follow-up comment was **dropped by user choice** — do not post.
- The remaining `md047_fix` code-comment cross-reference to #495 is folded
  into [[iterations/iteration-213-config-ux-polish]].

## Goal

Carry over the two third-party-repo writes that
[[iterations/iteration-196-mdlint-workaround-strip]] identified but could not
perform unattended (third-party repo writes are user-gated — same
classifier constraint [[iterations/iteration-194-post-upstream-mdbook-lint-reports]]
hit for the original #456 comment / #491 issue). This is the same pattern as
iter-194: the analysis and text are largely done, this iteration is "verify
still true, then post."

**Requires a human or an explicitly-elevated agent.** Do not attempt to
route around the classifier.

## Context

Source: [[docs/upstream-mdbook-lint-reports]] §"Outcome — mdbook-lint 0.16.0
(2026-08-22)", written during iter-196 after `mdbook-lint-core` /
`mdbook-lint-rulesets` 0.16.0 shipped (upstream release PR
[#484](https://github.com/joshrotenberg/mdbook-lint/pull/484)).

Two items were deferred there:

1. **Follow-up comment on
   [#456](https://github.com/joshrotenberg/mdbook-lint/issues/456).**
   iter-196 deleted ~200 lines of downstream compensation
   (`rule_uses_byte_columns`, `line_col_to_byte`, the MD011 `end += 1` guard,
   `trim_md034_liquid`, the `line_len + 1` heuristic) because upstream PR
   [#493](https://github.com/joshrotenberg/mdbook-lint/pull/493) shipped the
   exact contract #456 asked for. That is worth reporting back as concrete
   embedder confirmation the contract works, closing the loop on the
   original 2026-08-17 comment
   (<https://github.com/joshrotenberg/mdbook-lint/issues/456#issuecomment-5319878913>).
2. **New issue: MD047 CRLF gap.** Shipped 0.16.0
   `mdbook-lint-rulesets/src/standard/md047.rs` still:
   - hard-codes `Fix::insertion("Add newline at end of file", "\n", …)` for
     the missing-trailing-newline branch, which would flip a CRLF file's
     final line to a bare LF; and
   - counts trailing terminators via
     `content.chars().rev().take_while(|&c| c == '\n')`, which stops at the
     `\r` of the preceding CRLF pair, so the rule never fires at all on a
     CRLF file with several trailing blank lines (a detection gap, not just
     a fix-output one).

   hyalo's `md047_fix` in `crates/hyalo-mdlint/src/engine.rs` keeps a
   CRLF-only local computation as the one surviving workaround from iter-196;
   removing it is gated on this issue shipping a fix, the same way iter-196
   itself was gated on #486/#492/#493.

## Tasks

- [ ] Re-verify both reproduction cases against whatever `mdbook-lint-core` /
      `mdbook-lint-rulesets` version is current at run time (re-run the
      fixtures in `crates/hyalo-mdlint/src/engine.rs` — search for
      `md047_crlf_body_keeps_crlf_when_adding_the_final_terminator` — against
      the shipped crate, not just read the source). If either upstream
      symptom is already fixed, say so and skip posting that half rather than
      filing something already stale.
- [ ] Post a follow-up comment on
      <https://github.com/joshrotenberg/mdbook-lint/issues/456> reporting the
      embedder result: naming #493, and that it let hyalo delete the
      byte-column allowlist, the hand-rolled coordinate walk, and the
      MD011/MD034 guards outright, with a link back to
      [[iterations/iteration-196-mdlint-workaround-strip]] or the hyalo PR
      for anyone who wants the diff.
- [ ] File a new issue against `joshrotenberg/mdbook-lint` for the MD047 CRLF
      gap, with both sub-bugs (hard-coded `"\n"` insertion; undercounted
      CRLF terminators in `check_file_ending`) and a minimal repro for each.
      Cite the exact `md047.rs` lines/behavior from the current shipped
      version, not from memory of iter-196's notes.
- [ ] Record both resulting URLs in
      [[docs/upstream-mdbook-lint-reports]] (new subsection under the 0.16.0
      outcome) and in
      [[iterations/iteration-196-mdlint-workaround-strip]] (append the URLs
      next to the "Deferred — needs the user" section rather than rewriting
      history there).
- [ ] Once the MD047 issue is filed, add a code comment cross-reference in
      `md047_fix` (`crates/hyalo-mdlint/src/engine.rs`) pointing at the new
      issue number, replacing the current "**Not filed yet**" language in
      both the doc comment and [[docs/upstream-mdbook-lint-reports]].

## Acceptance criteria

- [ ] A follow-up comment exists on `joshrotenberg/mdbook-lint#456`
      reporting the 0.16.0 embedder result; its URL is recorded in
      [[docs/upstream-mdbook-lint-reports]] and
      [[iterations/iteration-196-mdlint-workaround-strip]]
- [ ] A new issue exists on `joshrotenberg/mdbook-lint` for the MD047 CRLF
      gap (both sub-bugs described, each with a minimal repro); its URL is
      recorded in the same two files
- [ ] `md047_fix`'s doc comment and code cross-reference the filed issue
      number instead of saying "not filed yet"
- [ ] `hyalo lint` on the knowledgebase reports no new findings after the
      edits

## Non-goals

- Do not implement a fix for the MD047 CRLF gap here — this is reporting
  only, mirroring iter-194. A follow-up iteration to delete `md047_fix`
  entirely is gated on upstream shipping a release that fixes it, the same
  way iter-196 was gated on #486/#492/#493.
- Do not re-derive the #456 follow-up comment or the MD047 issue text from
  scratch if iter-196's analysis has gone stale by run time — re-verify
  against the current shipped crate first (see Tasks), and note in this
  file's Results section if either report needs to change before posting.
