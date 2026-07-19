---
title: Dogfood v0.19.0-pre — slim pass over the unified link stack
type: research
date: 2026-07-19
status: active
tags: [dogfooding, links]
related: "[[reviews/link-handling-review-2026-07-18]]"
---

# Dogfood v0.19.0-pre — slim pass over the unified link stack

Slim, targeted pass after the 183–186 chain + the L-A1/L-A2 fix-forward
(PR #220, merge 045f6cb). Binary: release build at 045f6cb. Vaults: own KB
(347 files) + a scratch vault with L-A1/L-A2 fixture content.

## Verified

- **L-A1 fixed**: `[spaced link](<notes/my dest.md>)` resolves in
  `find --broken-links` (not flagged) and appears in `backlinks`.
- **L-A2 fixed**: `[Contains \[test\] brackets](notes/other.md)` is
  extracted and found by `backlinks`.
- **mv rewrites** preserve the angle-bracket form and survive a titled
  link with a paren in the title (`[titled](<x.md> "a (note)") tail` —
  the corruption case caught in PR #220 review) with no splice damage.
  Wikilinks and escaped-label links rewritten correctly in the same pass.
- **links fix**: scratch vault reports 1 broken / 0 fixable / dry-run
  default — correct; own KB reports 0 broken.
- **find --broken-links** on own KB: clean (0 results).
- **Diff-aware lint pipe** (`git diff --name-only A...B | hyalo lint
  --files-from -`): honest counters — non-md skipped, out-of-vault path
  reported missing, lint-ignored file surfaced as a warning.

## Findings

- No bugs found. No UX blockers.
- `hyalo lint` on own KB: 7 HYALO002 warnings (was 4). The 3 new ones are
  iterations 183/184/185 — `status: completed` with unchecked tasks that
  are *honestly deferred* scope (184's carried refactors; 185's anchors,
  HYALO006 lint rule, L-19/L-23). Truthful, deliberately left, consistent
  with the existing 4. A follow-up iteration sweeping the deferred link
  work would clear them naturally.

## Verdict

Release-ready for v0.19.0 from the link-stack perspective.
