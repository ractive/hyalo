---
title: "Iteration 185 — link semantics extensions (Phase D: anchors, lint rule, escapes)"
type: iteration
date: 2026-07-18
status: planned
branch: iter-185/link-semantics
tags: [iteration, links, lint, features]
depends-on: "[[iterations/iteration-184-link-resolver-writer-unification]]"
related:
  - "[[reviews/link-handling-review-2026-07-18]]"
---

# Iteration 185 — link semantics extensions (Phase D)

## Goal

With one scanner (iter-183) and one resolver/writer (iter-184) in place,
add the semantics the review showed are missing — each lands in exactly
one place now. Findings L-16, L-19, L-21, L-22, L-23 from
[[reviews/link-handling-review-2026-07-18]].

## Tasks

### 1. Anchor validation (L-21)

- [ ] Anchors are carried through resolution (not discarded at parse,
  links.rs:488-490): `[[Foo#nonexistent-heading]]` is reportable as
  broken-anchor by `find --broken-links` (distinct category from
  broken-target, since heading checks read file content)
- [ ] Heading→anchor slugging matches the wiki/Obsidian convention used
  by `read --section`; decide case/whitespace normalization and record
  in the decision log
- [ ] Perf guard: anchor validation only reads target files that are
  actually linked with an anchor; MDN-scale timing within budget

### 2. Broken-link lint rule (L-22)

- [ ] New HYALO004 lint rule: broken wikilink/markdown link targets
  (and optionally broken anchors, severity-configurable) so link health
  can gate CI via `lint --strict`
- [ ] Vault-level cache so lint doesn't rebuild the link graph per file;
  respects `[lint] ignore` and `[okf]`/exempt semantics
- [ ] Docs: lint-rules list/show entries, README, knowledgebase

### 3. Escapes and normalization (L-16, L-19, L-23)

- [ ] L-16: `\[[not-a-link]]` is not extracted (backslash escape per
  CommonMark/Obsidian); rewiters leave it untouched
- [ ] L-19: `.md`-suffix normalization happens at `Link` construction
  (with an as-written field preserved); remove the manual re-strip at
  auto_link.rs:552-557 and audit consumers comparing targets across
  link kinds
- [ ] L-23: percent-decode markdown link targets in `resolve_target`
  (discovery.rs:714-842) so `my%20page.md` resolves; encoding kept
  as-written on rewrite

### 4. Retrospective

- [ ] Re-run the full link-review fixture corpus (multi-line spans,
  BOM, CRLF, anchors, aliases, escapes) across find/mv/fix/auto/lint —
  all consistent
- [ ] Close out [[reviews/link-handling-review-2026-07-18]]: mark each
  L-finding fixed/deferred with a pointer

## Acceptance Criteria

- [ ] `hyalo lint --strict` can gate broken links in CI
- [ ] Anchored-link health visible in `find --broken-links` output
- [ ] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
