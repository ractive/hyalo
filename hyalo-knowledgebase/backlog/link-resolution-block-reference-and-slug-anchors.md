---
type: backlog
title: Block references and slug anchors are never reported broken
date: 2026-09-05
status: wont-do
priority: low
origin: "iter-272 (PR #320) carry-over sweep, 2026-09-05, DEC-299"
---

## Problem

[[iterations/iteration-272-resolution-completeness]] Part E surveyed the report's resolution
feature gaps and backlogged this one (DEC-299, 2026-09-05) rather than implementing it:
`[[Target#^nope]]` (a block-reference anchor) and `[[Target#section-one]]` (a pre-slugified
heading anchor, the form Obsidian itself writes into `redirect_from:` style links) are never
reported broken by `find --broken-links`, even though Obsidian breaks the first outright and
would not resolve the second unless it happens to match a real heading's rendered slug.

DEC-268's anchor checker validates a fragment against the raw heading text or its GitHub-style
rendered slug (`hyalo-core::anchor`). Neither path knows about:

- **Block references** (`^block-id`) — Obsidian lets a link point at a specific block via a
  trailing `^abc123` identifier the author places at the end of a paragraph or list item, not a
  heading. `find --fields links` and the scanner currently skip `^block` refs outright (see the
  `rule-knowledgebase.md` / `skill-hyalo.md` line: "`^block-id` refs are skipped").
- **Slug-only anchors** that happen to coincide with a real slug but not the raw heading text —
  already handled by the existing rendered-slug fallback, so this half of the DEC-299 title may
  already be closed; confirm before starting (see Proposal).

## Proposal

Not yet designed — this is a placeholder for the follow-up DEC-299 deferred, not a committed
shape. Whoever picks this up should:

- Confirm exactly what is and is not already covered: re-run DEC-268's rendered-slug fallback
  against a fixture using `[[Target#section-one]]` where `section-one` is the real slug of
  `## Section One` — if that already resolves, DEC-299's scope narrows to block references only.
- Decide whether block-id checking needs a full scan for `^block-id` markers in every file (a
  new indexed field: which files declare which block ids, and at which line), or whether it can
  piggyback on an existing per-file scan pass. Note DEC-299's own reasoning: "reporting them
  without the scan would make every block reference in every Obsidian vault a false positive" —
  so implementing this without the scan is explicitly out.
- If a new indexed field is added, it changes the snapshot format; follow the precedent
  iter-272 Part C set for `self_anchors` (`#[serde(default, skip_serializing_if = ...)]` so an
  older snapshot degrades to "falls back to disk scan" rather than failing to load) rather than
  bumping a format version.
- Decide whether the target file's block ids need indexing (for the target to be checked) or
  only local ones — a link into another file's block (`[[Other#^abc]]`) needs foreign block-id
  data at scan time, which is a bigger index than a same-file check.

## Acceptance criteria

- [x] Confirm and record whether the rendered-slug half of DEC-299's title is already closed by
      DEC-268; if so, retitle this item to cover block references only.
- [x] Not applicable (not implemented, DEC-410). If implementing block-id checking: a block-id scan populates an indexed field; a broken
      `[[Target#^nope]]` is reported the same way a broken heading anchor is
      (`broken_anchor: true`, distinct from a broken target); `--index` and disk-scan parity
      holds; an older snapshot without the field falls back to disk scan rather than erroring.
- [x] If staying backlog (won't-do or deferred again): a DEC amendment records why, with
      whatever new information the confirmation step surfaced.

## Outcome (iteration 337)

Closed `wont-do`; DEC-410 amends DEC-299. Measured with a release build of `main` (`85260414`); nothing was written
to any corpus.

- **The slug half is already closed.** `[[Target#section-one]]` against `## Section One` resolves, and
  `[[Target#nope]]` carries `broken_anchor` and is reported by HYALO008. The item's title is therefore about block
  references only.
- **Block references are inventoried and never checked** (`anchor.rs:72` `is_block_ref`, returned early at
  `anchor.rs:500-503`).
- **No finding to show for it.** The Obsidian Hub (`a56b648b`) has 6 block-reference links: 1 has a target file that
  does not exist, the other 5 point at ids that exist, so 0 are genuinely broken. The hyalo knowledgebase has 0. A
  block-id index (a new snapshot field) is not worth its cost. `protected.rs:1150` `trailing_block_id` already parses
  the markers if this is ever revisited.
