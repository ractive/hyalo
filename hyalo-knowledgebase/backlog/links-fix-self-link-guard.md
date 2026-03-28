---
title: "links fix: reject fuzzy matches that resolve to the source file"
type: backlog
date: 2026-03-28
status: planned
priority: medium
origin: dogfooding v0.5.0 link health
tags:
  - backlog
  - links
  - bug
---

# links fix: reject fuzzy matches that resolve to the source file

## Problem

When `hyalo links fix` encounters a broken wikilink whose target doesn't exist, the fuzzy matcher (Jaro-Winkler) can propose a match that points back to the file containing the link. This creates a self-referential link, which is never the intended fix.

Observed case: `sort-by-property-value.md` contained a broken link `[[backlog/sort-reverse.md]]`. The fuzzy matcher resolved it to `sort-by-property-value.md` itself (the source file), because the stems were similar enough to exceed the threshold.

## Proposal

After the fuzzy matcher finds a candidate, add a guard that rejects any match where the resolved target path equals the source file path. If the self-match is rejected, fall back to the next-best candidate or mark the link as unfixable.

## Acceptance criteria

- [ ] `links fix` never proposes a fix where `new_target` resolves to the same file as `source`
- [ ] A self-link candidate is skipped and the next-best match is tried
- [ ] If no non-self match exists, the link is reported as unfixable
- [ ] Unit test covering the self-link rejection case
