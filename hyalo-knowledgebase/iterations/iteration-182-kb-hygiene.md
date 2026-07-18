---
title: "Iteration 182 — knowledgebase hygiene (stale links, duplicates, statuses)"
type: iteration
date: 2026-07-18
status: planned
branch: iter-182/kb-hygiene
tags: [iteration, knowledgebase, hygiene]
related:
  - "[[dogfood-results/dogfood-v0180-final-pre-release]]"
---

# Iteration 182 — knowledgebase hygiene

## Goal

Docs-only cleanup of the real issues the dogfood's own-KB sweep found in
`hyalo-knowledgebase/` itself
([[dogfood-results/dogfood-v0180-final-pre-release]], "Own-KB Hygiene").
Use hyalo for every step (`links fix`, `mv`, `set`, `find`) — this is
itself a dogfooding exercise.

## Tasks

### 1. Stale broken links (5 genuinely stale, fix by hand — fuzzy would mislink)

- [ ] `research/release-pipeline-unification.md`:
  `[[iteration-80-musl-targets-winget]]` — locate the real
  musl/winget iteration and retarget (do NOT fuzzy-fix to
  `iteration-80-smarter-hints`, which is wrong)
- [ ] `iterations/done/iteration-150-link-handling-refactor.md`:
  `[[iterations/done/iteration-132-mv-wikilinks]]` → actual file is
  `iteration-132-dogfood-v0150-iter131-fixes.md` (verify intent first)
- [ ] `iterations/done/iteration-118-split-index-flag.md`: `[[CLAUDE]]` —
  retarget or backtick
- [ ] `iteration-148...` + `dogfood-v0160-iter-144-147.md`:
  `[[feedback_keep_docs_in_sync]]` points at a Claude memory file outside
  the KB — backtick or replace with prose
- [ ] `iterations/iteration-168-skills-profile.md`: `[[doc]]` — retarget
  or backtick

### 2. Pseudo-links in dogfood reports

- [ ] Backtick the illustrative wikilinks (`[[schema.bind]]` ×8,
  `[[fake-login]]` ×2, `[[NEW-1/3/4]]`, `[[old]]`, `[[new]]`,
  `[[target]]`, `[[wikilinks]]`, `[[links]]`) so link health reads clean —
  single-line code spans only (multi-line spans hit BUG-16 / review
  finding L-3 until iter-183 lands)

### 3. Duplicate iteration files

- [ ] `iterations/done/iteration-25-release-profile-and-quick-wins.md` vs
  `iteration-25-release-profile-quick-wins.md`: diff, keep the canonical
  one, mark the other superseded or delete (check backlinks with
  `hyalo backlinks` before removing)
- [ ] Verify the two `iteration-22-*` files are genuinely distinct
  iterations (section-filter vs security-hardening) and leave a note if so

### 4. Status truth

- [ ] Triage the 34 `status: completed` files with open tasks (view
  `completed-with-todos`): tick verified-done tasks, reopen or annotate
  genuinely open ones — at minimum the iterations from the last two
  fix-waves
- [ ] `iteration-171-setup-hyalo-action` stays `in-progress` by design
  (blocked on the v0.18.0 release) — add a note referencing the blocker
  instead of leaving it silently open

### 5. Vendored subtree

- [ ] Decide handling for `research/setup-hyalo-action/` (vendored
  README/PUBLISH/fixture files polluting summary counters): exclude from
  the vault scan, or give the files minimal frontmatter; record the
  decision (interacts with iter-180 task 3)

### 6. Verification

- [ ] `hyalo find --broken-links` reports only intentional/unfixable
  links; `hyalo lint --strict` stays exit 0; `hyalo summary` counters
  reflect the cleanup

## Acceptance Criteria

- [ ] Broken-link count drops from 25 to the documented unfixable rest
- [ ] No duplicate iteration numbers in `iterations/done/`
- [ ] All changes made through hyalo commands where hyalo can do the job
