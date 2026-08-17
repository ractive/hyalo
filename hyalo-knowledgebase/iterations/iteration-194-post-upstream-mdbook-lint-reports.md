---
type: iteration
title: Iteration 194 — post drafted upstream mdbook-lint reports
date: 2026-08-17
status: planned
tags:
  - iteration
  - upstream
  - mdbook-lint
  - carry-over
branch: iter-194/post-upstream-mdbook-lint-reports
related:
  - "[[iteration-193-vault-side-effects-and-dep-diet]]"
  - "[[docs/upstream-mdbook-lint-reports]]"
---

# Iteration 194 — post drafted upstream mdbook-lint reports

## Goal

Post the two upstream `mdbook-lint` submissions that iter-193 wrote in full
but could not send: an unattended run is correctly blocked by the permission
classifier from writing to a third-party GitHub repository
(`joshrotenberg/mdbook-lint`). Both texts already exist, reviewed and
verified against `mdbook-lint-core` 0.15.2 — this iteration is "post it and
record the URL," not "figure out what to say."

**Requires a human or an explicitly-elevated agent.** This cannot run
unattended under the current classifier; do not attempt to route around it.

## Context

Source texts: [[docs/upstream-mdbook-lint-reports]], written during
[[iteration-193-vault-side-effects-and-dep-diet]] (Part B / Part C).

- Item 1 is a **comment** on an existing, open upstream issue.
- Item 2 is a **new issue** to be filed.

Both were verified still applicable against `mdbook-lint-core` 0.15.2 (the
version iter-193 bumped hyalo to) at plan time — re-verify only if a newer
`mdbook-lint-core` has shipped by the time this runs, since a fix could have
landed in the interim.

## Tasks

- [x] Post the comment in [[docs/upstream-mdbook-lint-reports]] §1 to
      <https://github.com/joshrotenberg/mdbook-lint/issues/456>. Re-verify the
      cited line numbers / behavior against whatever `mdbook-lint-core`
      version is current before posting — if it has moved, update the text's
      version references rather than posting stale specifics.
- [ ] File the issue in [[docs/upstream-mdbook-lint-reports]] §2 (MD018
      false-positive on paragraph continuation lines beginning with `#`)
      against <https://github.com/joshrotenberg/mdbook-lint>. Re-run the three
      reproduction cases from the draft before posting to confirm they still
      hold.
- [ ] Record both resulting URLs in [[docs/upstream-mdbook-lint-reports]]
      (flip `status: draft` → `status: posted` in its frontmatter and add the
      URLs inline under each section).
- [ ] Record the same two URLs in
      [[iteration-193-vault-side-effects-and-dep-diet]]: tick its "Comment on
      upstream issue #456" and "File an upstream issue: MD018 …" boxes, tick
      the "upstream #456 comment link recorded in this file" acceptance
      criterion, and paste the URLs into that file's Results section.

## Acceptance criteria

- [x] A comment exists on `joshrotenberg/mdbook-lint#456` matching (or
      knowingly updating) the drafted text; its URL is recorded in both
      [[docs/upstream-mdbook-lint-reports]] and
      [[iteration-193-vault-side-effects-and-dep-diet]]
- [ ] A new issue exists on `joshrotenberg/mdbook-lint` for the MD018
      paragraph-continuation false positive; its URL is recorded in both of
      the same two files
- [ ] `hyalo lint` on the knowledgebase reports no new findings after the edits

## Non-goals

- Do not implement a fix for either upstream issue here — this is reporting
  only. A follow-up iteration to remove hyalo's `convert_fix` workarounds is
  gated on upstream actually shipping #456's fix, not on this iteration.
- Do not re-derive the report texts. If either has gone stale (upstream
  already fixed it, or the reproduction no longer holds), say so in this
  file's Results section and close out the corresponding task as "not
  applicable" rather than posting something no longer true.
