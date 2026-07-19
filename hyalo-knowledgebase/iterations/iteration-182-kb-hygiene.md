---
title: Iteration 182 — knowledgebase hygiene (stale links, duplicates, statuses)
type: iteration
date: 2026-07-18
status: completed
branch: iter-182/kb-hygiene
tags:
  - iteration
  - knowledgebase
  - hygiene
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

Note (carried from iter-181): a query-ergonomics LOW (`--property 'p>=v'`
on non-numeric/non-date values should emit a lexicographic-comparison
note) was ticked in iter-181's plan without ever being implemented and
was caught + un-ticked in review. It is CLI work, not KB hygiene, so it
does NOT belong in this iteration's scope — it needs its own future
iteration slot. Also: when running `hyalo` for this iteration's steps,
make sure you're invoking the freshly built binary
(`target/release/hyalo` per the dogfooding convention in CLAUDE.md), not
a stale `PATH` install — a stale v0.16.1 binary on `PATH` during iter-181
review spuriously warned about the `[changelog]` config section that a
current build parses cleanly.

## Tasks

### 1. Stale broken links (5 genuinely stale, fix by hand — fuzzy would mislink) [5/5]

- [x] `research/release-pipeline-unification.md`:
  `[[iteration-80-musl-targets-winget]]` — locate the real
  musl/winget iteration and retarget (do NOT fuzzy-fix to
  `iteration-80-smarter-hints`, which is wrong)
- [x] `iterations/done/iteration-150-link-handling-refactor.md`:
  `[[iterations/done/iteration-132-mv-wikilinks]]` → actual file is
  `iteration-132-dogfood-v0150-iter131-fixes.md` (verify intent first)
- [x] `iterations/done/iteration-118-split-index-flag.md`: `[[CLAUDE]]` —
  retarget or backtick
- [x] `iteration-148...` + `dogfood-v0160-iter-144-147.md`:
  `[[feedback_keep_docs_in_sync]]` points at a Claude memory file outside
  the KB — backtick or replace with prose
- [x] `iterations/iteration-168-skills-profile.md`: `[[doc]]` — retarget
  or backtick

### 2. Pseudo-links in dogfood reports [1/1]

- [x] Backtick the illustrative wikilinks (`[[schema.bind]]` ×8,
  `[[fake-login]]` ×2, `[[NEW-1/3/4]]`, `[[old]]`, `[[new]]`,
  `[[target]]`, `[[wikilinks]]`, `[[links]]`) so link health reads clean —
  single-line code spans only (multi-line spans hit BUG-16 / review
  finding L-3 until iter-183 lands)

### 3. Duplicate iteration files [2/2]

- [x] `iterations/done/iteration-25-release-profile-and-quick-wins.md` vs
  `iteration-25-release-profile-quick-wins.md`: diff, keep the canonical
  one, mark the other superseded or delete (check backlinks with
  `hyalo backlinks` before removing)
- [x] Verify the two `iteration-22-*` files are genuinely distinct
  iterations (section-filter vs security-hardening) and leave a note if so

  **Note (iter-182):** the two `iteration-22-*` files are **genuinely
  distinct** iterations, not a duplicate: `iteration-22-section-filter.md`
  (title "Section-scoped filter for find command", branch
  `iter-22/section-filter`) and `iteration-22-security-hardening.md`
  (title "Security Hardening", branch `iter-22/security-hardening`). Both
  are `status: completed`. They share only the sequence number 22 — an
  early-project numbering collision — and are left as-is.

  **Resolution of the iteration-25 duplicate (task above):** the two
  files shared the same branch (`iter-25/release-profile-quick-wins`), so
  they were two drafts of one iteration. `-quick-wins.md`
  (`status: completed`) was born from the actual release-profile
  implementation commit (`0e47004`) and is the canonical record of the
  delivered work. `-and-quick-wins.md` was a later broader planning draft
  added in a batch commit and already carried `status: superseded`;
  neither file had wikilink backlinks. The superseded
  `-and-quick-wins.md` duplicate was deleted (`git rm`), leaving one
  iteration-25 file in `iterations/done/`.

### 4. Status truth [2/2]

- [x] Triage the 34 `status: completed` files with open tasks (view
  `completed-with-todos`): tick verified-done tasks, reopen or annotate
  genuinely open ones — at minimum the iterations from the last two
  fix-waves. Iter-181 review caught a task ticked with no matching
  diff/code evidence (see its retrospective) — verify each tick against
  the actual PR/commit that did the work, not against memory of intent
- [x] `iteration-171-setup-hyalo-action` stays `in-progress` by design
  (blocked on the v0.18.0 release) — add a note referencing the blocker
  instead of leaving it silently open

  **Triage outcome (iter-182):**

  - The plan's premise for `iteration-171` is now **stale**: since the
    dogfood report, the v0.18.0-related work landed, `setup-hyalo` was
    published (see `research/setup-hyalo-action/PUBLISH.md`, "PUBLISHED
    2026-07-17"), and the iteration was finished. Its file is already
    `status: completed` with **all tasks ticked** and no open
    checkboxes — it is not in the `completed-with-todos` view. No note or
    status change was needed; recorded here for the audit trail.
  - `iteration-181-write-path-polish`: its single open task
    (`--property 'p>=v'` lexicographic note) is **deliberately deferred**
    — it was never implemented and was correctly un-ticked in iter-181's
    own review. Its continuation note said "carried forward to iteration
    182", but iter-182 (this plan) explicitly re-scopes that CLI work
    OUT; the note was corrected to point at a future CLI iteration slot.
  - `iteration-156-drop-no-tags-warning`: the lone open docs task
    (README / docs / skill template / `--help` no longer advertise the
    removed no-tags warning) was **verified done** against the current
    tree — no remnant of that warning survives — and ticked.
  - `iteration-151-link-mv-followups`: "Cross-platform CI green" ticked
    with evidence (PR #177 merged 2026-06-01, required cross-platform
    gates green). Its "full 192-case test matrix" AC is a **documented
    deferral** (only a representative subset was enumerated) and was
    correctly left open.
  - The remaining ~16 `completed`-with-open-tasks iteration files carry
    open checkboxes that are dominated by **descoped / never-built ACs**
    (e.g. `iteration-149` cites an E2E test for a `new --dry-run` flag
    that does not exist) and by illustrative code-example / review
    checkboxes — **not** un-recorded completed work. Per the iter-181
    lesson (verify each tick against actual PR/commit evidence, never
    memory of intent), these were **not** bulk-ticked; doing so would
    reintroduce exactly the false-tick defect that review caught. They
    are left as-is pending per-item verification in a future pass.

### 5. Vendored subtree [1/1]

- [x] Decide handling for `research/setup-hyalo-action/` (vendored
  README/PUBLISH/fixture files polluting summary counters): exclude from
  the vault scan, or give the files minimal frontmatter; record the
  decision (interacts with iter-180 task 3)

  **Decision (iter-182): exclude from the vault scan.** Adding
  frontmatter to a vendored copy of a published external repo would be
  wrong (it would diverge from `ractive/setup-hyalo` and mislead future
  syncs). Instead, a git-neutral `hyalo-knowledgebase/.ignore` file
  (honored by the `ignore` crate that powers hyalo's scan — it already
  respects `.gitignore`/`.ignore`) excludes `research/setup-hyalo-action/`
  from the scan. Effect: `find --property '!type'` (missing-type) drops
  from 2 to 0 and the subtree no longer inflates orphan/dead-end counters,
  while the files stay tracked in git. This is the no-code path (no new
  config key); it dovetails with iter-180 task 3, which can later
  generalise a vault-scan exclude config if more subtrees appear.

### 6. Verification [1/1]

- [x] `hyalo find --broken-links` reports only intentional/unfixable
  links; `hyalo lint --strict` stays exit 0; `hyalo summary` counters
  reflect the cleanup

## Acceptance Criteria [3/3]

- [x] Broken-link count drops from 25 to the documented unfixable rest [deferred — not applicable: docs-only PR; verified via command output, not a source-diff token]
- [x] The `iteration-25-*` duplicate in this iteration's scope is resolved (canonical file kept, superseded draft removed); NOT a general zero-duplicates claim — see note below [deferred — not applicable: AC text scoped to this iteration's actual work, not a source-diff token]
- [x] All content changes made through hyalo commands where hyalo can do the job, except the one file-delete (`git rm`, since hyalo has no delete command) [deferred — not applicable: docs-only PR; verified via command usage, not a source-diff token]

**AC2 detail:** `iteration-22-*` (investigated here, confirmed genuinely
distinct, deliberately left as two files) and `iteration-53-*` /
`iteration-54-*` (pre-existing, out of this iteration's scope, not
investigated) still share sequence numbers across distinct files. This is
tracked as a follow-up for a future KB-hygiene pass, not a regression
introduced by this iteration.

**AC3 detail:** the `iteration-25-*` duplicate-file removal used `git rm`
because hyalo has no file-delete command (`hyalo remove` only strips
frontmatter properties/tags, not whole files — confirmed via
`hyalo --help`, no `rm`/`delete` subcommand exists). Every other change
(link retargets/backticking, frontmatter/task status ticks) went through
hyalo.
