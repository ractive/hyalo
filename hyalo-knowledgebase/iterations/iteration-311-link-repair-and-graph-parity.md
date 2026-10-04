---
title: "Iteration 311: link repair and graph parity"
type: iteration
date: 2026-10-04
tags: [iteration, links, anchors, mv, dogfooding]
status: in-progress
branch: iter-311/link-repair-and-graph-parity
priority: 1
related:
  - "[[dogfood-results/dogfood-v0250-pre-search-roadmap-2026-10-04]]"
---

# Link repair and graph parity

## Problem

[[dogfood-results/dogfood-v0250-pre-search-roadmap-2026-10-04]] found one HIGH and five MEDIUM/LOW bugs in the link graph and in
`links fix`: the anchor repair writes GFM slugs into Obsidian wikilinks (BUG-1),
`summary.links.broken` disagrees with `find --broken-links` and HYALO006 on bare
`[[alias]]` links (BUG-2), `mv` leaves bare attachment links unrewritten and the
vault-wide attachment resolver hides the stale link (BUG-3), HTML anchors are not
anchor targets (BUG-9), `[x](/)` does not resolve to `index.md` (BUG-15), the
case-mismatch plan reports a target the rewrite does not write (BUG-17), wikilink
resolution does not fold Unicode normalisation (BUG-13), and `--apply-fuzzy` without
`--apply` says "written" on a dry run (UX-3). DEC numbers reserved for this
iteration: **DEC-351 to DEC-354**.

## Tasks

- [ ] BUG-1: `links fix` writes the heading text as written (`#3. Deploy Steps`) into a wikilink fragment and the GFM slug into a markdown link fragment; the dry-run `anchor_fixes` entry shows the fragment that will be written per link kind; e2e test with both kinds in one file (DEC)
- [ ] BUG-2: `summary.links.broken` counts a bare `[[alias]]` link as broken under `[links] aliases = false`, exactly as `find --broken-links`, HYALO006 and `links fix` do; the Obsidian Hub reports 162 everywhere; DEC-318 amended
- [ ] BUG-3: `mv` rewrites bare relative attachment and embed links (`[img](img.png)`, `![e](img.png)`, `[cfg](.gitignore)`) so the relative path stays valid after the move, like the `.md` sibling; decide and record whether the resolver's vault-wide attachment fallback is narrowed (DEC)
- [ ] BUG-9: `<a id>`, `<a name>` and `<hN id>` are anchor targets for resolution, HYALO008 and `links fix`; help texts and `lint-rules show HYALO008` say so (DEC — or a DEC documenting the gap if resolution is rejected)
- [ ] BUG-15: a site-absolute `/` resolves to the vault-root `index.md` when it exists, consistent with `/dir` → `dir/index.md`
- [ ] BUG-17: the case-mismatch plan's `new_target` equals the text written, and `--fields links` `target` keeps what was written (DEC-310) for `[[sub/note.md]]`
- [ ] BUG-13: link targets and file stems are NFC-normalised before comparison, so `[[Café NFD]]` resolves an NFD-named file and vice versa (Obsidian semantics); `mv` and `links fix` unaffected
- [ ] UX-3: `--apply-fuzzy` and `--min-confidence` without `--apply` report "not written — pass --apply", `fuzzy_applied` is false on a dry run, and `links fix -h` says `--apply` is still required
- [ ] UX-7: `find --broken-links` and `links fix` choose the same anchor for the same broken fragment (one chooser); `deferred_anchor_fixes` entries carry `suggested_fragment` and a per-entry reason; the text "Fixable:" line counts anchor fixes
- [ ] Hints: a `links fix` dry run with any proposal offers `=> hyalo links fix --apply [writes]`; `lint --rule-prefix HYALO` hints `find --broken-links`
- [ ] `[[<placeholder>]]` angle-bracket targets go to the `templated` bucket with `{{…}}`, never to fuzzy candidates
- [ ] Docs in sync: help texts, `.claude/CLAUDE.md` claims paragraph, `skill-hyalo.md`, `rule-knowledgebase.md`, CHANGELOG `[Unreleased]` Fixed bullets, decision-log entries

## Acceptance criteria

- [ ] Every fix has a regression test named by behaviour; the Obsidian Hub dry-run counts (`summary`, `find --broken-links`, HYALO006) agree at 162
- [ ] A scratch vault with a numbered heading, one wikilink and one markdown link to it: after `links fix --apply` the wikilink carries the heading text and the markdown link the slug, both resolve, Obsidian's rule is satisfied
- [ ] Exit codes stay 0/1/2 (DEC-307); `check-jq-recipes` and help-drift gates pass
- [ ] fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check` and `hyalo lint --strict` are green; CI green on Linux, macOS and Windows
