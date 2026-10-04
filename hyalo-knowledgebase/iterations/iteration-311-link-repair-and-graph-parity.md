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

- [x] BUG-1: `links fix` writes the heading text as written (`#3. Deploy Steps`) into a wikilink fragment and the GFM slug into a markdown link fragment; the dry-run `anchor_fixes` entry shows the fragment that will be written per link kind; e2e test with both kinds in one file (DEC)
- [x] BUG-2: `summary.links.broken` counts a bare `[[alias]]` link as broken under `[links] aliases = false`, exactly as `find --broken-links`, HYALO006 and `links fix` do; the Obsidian Hub reports 162 everywhere; DEC-318 amended
- [x] BUG-3: `mv` rewrites bare relative attachment and embed links (`[img](img.png)`, `![e](img.png)`, `[cfg](.gitignore)`) so the relative path stays valid after the move, like the `.md` sibling; decide and record whether the resolver's vault-wide attachment fallback is narrowed (DEC)
- [x] BUG-9: `<a id>`, `<a name>` and `<hN id>` are anchor targets for resolution, HYALO008 and `links fix`; help texts and `lint-rules show HYALO008` say so (DEC — or a DEC documenting the gap if resolution is rejected)
- [x] BUG-15: a site-absolute `/` resolves to the vault-root `index.md` when it exists, consistent with `/dir` → `dir/index.md`
- [ ] BUG-17: the case-mismatch plan's `new_target` equals the text written, and `--fields links` `target` keeps what was written (DEC-310) for `[[sub/note.md]]`
- [x] BUG-13: link targets and file stems are NFC-normalised before comparison, so `[[Café NFD]]` resolves an NFD-named file and vice versa (Obsidian semantics); `mv` and `links fix` unaffected
- [x] UX-3: `--apply-fuzzy` and `--min-confidence` without `--apply` report "not written — pass --apply", `fuzzy_applied` is false on a dry run, and `links fix -h` says `--apply` is still required
- [x] UX-7: `find --broken-links` and `links fix` choose the same anchor for the same broken fragment (one chooser); `deferred_anchor_fixes` entries carry `suggested_fragment` and a per-entry reason; the text "Fixable:" line counts anchor fixes
- [x] Hints: a `links fix` dry run with any proposal offers `=> hyalo links fix --apply [writes]`; `lint --rule-prefix HYALO` hints `find --broken-links`
- [x] `[[<placeholder>]]` angle-bracket targets go to the `templated` bucket with `{{…}}`, never to fuzzy candidates
- [x] Docs in sync: help texts, `.claude/CLAUDE.md` claims paragraph, `skill-hyalo.md`, `rule-knowledgebase.md`, CHANGELOG `[Unreleased]` Fixed bullets, decision-log entries

## Acceptance criteria

- [x] Every fix has a regression test named by behaviour; the Obsidian Hub dry-run counts (`summary`, `find --broken-links`, HYALO006) agree at 162
- [x] A scratch vault with a numbered heading, one wikilink and one markdown link to it: after `links fix --apply` the wikilink carries the heading text and the markdown link the slug, both resolve, Obsidian's rule is satisfied
- [ ] Exit codes stay 0/1/2 (DEC-307); `check-jq-recipes` and help-drift gates pass
- [ ] fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check` and `hyalo lint --strict` are green; CI green on Linux, macOS and Windows

## Outcome

Every task shipped except the `--fields links` half of BUG-17 (below). DEC
numbers used: **DEC-351** (BUG-1, anchor fragment form by link kind),
**DEC-352** (BUG-3, `mv` bare-attachment/embed rewrite; the vault-wide
read-side fallback stays unchanged by decision), **DEC-353** (BUG-9,
explicit HTML anchors `<a id>`/`<a name>`/`<hN id>`), **DEC-354** (BUG-13,
NFC normalisation in wikilink resolution). BUG-2 amends DEC-318 in place
(no new number, per the task's own DEC allocation — only BUG-1/3/9/13 were
listed as needing a fresh `DEC-35n`). UX-3, UX-7, the hints task and the
`[[<placeholder>]]` bucket needed no DEC — they are straight bug fixes to
an existing, undisputed contract.

**Obsidian Hub counts, before → after:** `summary.links.broken` 154 → **162**;
`find --broken-links` (path-null, non-attachment/external) 162 → 162
(unchanged — it was already right); `lint --rule HYALO006` violations 162 →
162 (unchanged). All three agree at **162** on the current `main`-plus-this-
branch build, confirmed with the final release binary.

**Deliberately left out:**
- **BUG-17, second half.** `--fields links` reports a wikilink's `target`
  with the `.md` suffix already stripped (`parse_wikilink` /
  `strip_wikilink_md_suffix` strips it at *parse* time, not at display
  time), so `[[sub/note.md]]` reports `target: "sub/note"` — DEC-310 says
  `target` should keep exactly what was written. The *links fix* half of
  BUG-17 (case-mismatch `new_target` vs. the byte-truthful write) is fixed
  and tested. The parse-time stripping is a much older, far more deeply
  depended-on convention (`mv`, lint, alias matching, resolution and
  `find`'s own output all assume `Link.target` is already stem-form for a
  wikilink) — changing it to preserve the authored suffix while keeping
  every *other* consumer's stripped-stem expectation intact is a
  resolution/parsing-layer redesign, not a bug fix, and risks silently
  changing behaviour in dozens of call sites I did not have the time budget
  to audit safely in this iteration. Left the line unticked rather than
  claim it done; flagging for its own iteration.
- **`check-help-drift` gate.** Could not be run to completion: it shells out
  to `cargo run -q -p hyalo-cli -- <cmd> --help` for dozens of subcommands,
  and two independent runs both hung at near-zero CPU for 8+ minutes. A
  second, concurrently running sibling-iteration worktree (iter-312) hit
  the identical hang on the identical command at the same wall-clock time —
  external cross-process cargo lock contention, not something this
  iteration's changes caused. `check-jq-recipes` passed cleanly (47
  recipes). The help-text additions here are purely additive prose appended
  to existing `long_about` blocks at matching indentation; manually
  verified via `hyalo links fix --help`/`-h`, `hyalo summary --help`, and
  `hyalo lint-rules show HYALO008` — all render correctly, no panics, no
  dangling fragments.

**Gates run:** `cargo fmt` (clean), `cargo clippy --workspace --all-targets
-- -D warnings` (clean), `cargo test --workspace -q` (all green, 2446 +
1659 + 1105 + 256 + 71 unit/e2e/doc tests across the workspace), `cargo
deny check` (advisories/bans/licenses/sources all ok), `hyalo lint --strict`
on this knowledgebase (0 errors, exit 0). `check-help-drift` not verified
(see above) — left for a retry once the external contention clears.

Status is **not** moved to `completed`: the two unticked acceptance-criteria
lines above (`check-jq-recipes`/help-drift; the CI line) are left
unticked honestly rather than claimed. The CI line is the orchestrator's to
tick after a real CI run; the help-drift half of the gates line should be
re-run once the lock contention is gone.
