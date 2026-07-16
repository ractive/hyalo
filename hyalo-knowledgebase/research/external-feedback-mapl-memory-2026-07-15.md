---
type: research
title: External agent feedback — hyalo in the mapl-memory repo (2026-07-15)
date: 2026-07-15
status: active
tags:
  - dogfooding
  - feedback
  - feature-ideas
related:
  - "[[research/feature-gaps-2026-07-10]]"
---

# External agent feedback — mapl-memory (2026-07-15)

First substantial feedback from an agent using hyalo in a repo we don't
control (`comparis/mapl-memory`, a frontmatter-schema knowledgebase with
`status`/`last_verified`/`protected`/`type` properties). Sample: mostly
`find`, one `set` pathway via skills. Every claim below was reproduced
against that repo with hyalo 0.16.1 and verified in source.

## What works (their words)

- `find --property --format text` is "the best triage tool I have here" —
  properties + section outline + links per file before deciding what to read.
  "Grep can't do that; Read is 10× the tokens."
- Property queries over frontmatter are "the tool's core value and it
  delivers"; `hyalo set` is "the right primitive" for frontmatter mutations.

## Friction 1 — `title~=` case-sensitivity trap (verified, real)

They ran `hyalo find --property 'title~=likec4'` → 0 results, though
`likec4.md` exists. Root cause chain, all confirmed in code:

- `likec4.md` has no `title:` frontmatter; the virtual `title` falls back to
  the first H1: "Architecture modeling with **LikeC4**"
  (`crates/hyalo-cli/src/commands/find/build.rs:12-31`; chain is frontmatter
  title → first H1 → null — filename stem is NOT in the chain).
- Bare `~=` compiles a **case-sensitive** regex
  (`crates/hyalo-core/src/filter/parse.rs:247-251`), so `likec4` ≠ `LikeC4`.
  `title~=/likec4/i` matches 2 files; `title~=LikeC4` matches 2 files.
- `--title` is **always case-insensitive** (both substring and `/regex/`
  forms, `find/build.rs:35-105`) — so the two title-matching paths disagree.
- The did-you-mean diagnostic only fires for misspelled property *keys*
  (edit distance ≤ 2, `find/mod.rs:964-1063`). A value that misses only due
  to case is silent — exactly the "empty result vs. no such property look
  identical" complaint.

They fell back to `ls`/`grep` instead of debugging the query. **This is the
worst kind of failure: silent, plausible, and it teaches the agent the tool
is unreliable.**

Fix candidates (for a future iteration):
- [ ] Make bare `~=` case-insensitive by default, consistent with `--title`
      (breaking-ish; or add a config knob)
- [ ] Empty-result hint: when a `~=`/`=` value filter yields 0 but a
      case-insensitive retry would yield N > 0, emit
      `hint: N files match case-insensitively; try ~=/pattern/i`
- [ ] Document virtual-property semantics (`title` fallback chain) in
      `--help` and the rule template

## Friction 2 — no body/log append (verified gap)

`LOG.md` in that repo is append-only by convention; the agent appends with
shell redirection. `hyalo append` only mutates frontmatter list properties
(`crates/hyalo-cli/src/commands/append.rs` — never touches the body).
A first-class body append (`hyalo append <file> --section "Log" --text ...`
or similar "add entry to list/log") would fit that repo's most common write
pattern. Candidate for the backlog.

- [ ] Feature: body append primitive (append text/list-item under a section
      or at EOF)

## Friction 3 — operator discoverability (features exist, agents can't find them)

They wished for `!=`, date comparison (`last_verified<2026-01-15`), and knew
only `=`/`~=` from the examples. **Everything they wanted already exists**:
full operator set is `=`, `!=`, `<`, `<=`, `>`, `>=`, `~=`, bare `KEY`
(exists), `!KEY` (absent) (`crates/hyalo-core/src/filter/parse.rs:1-21`);
date `<`/`>` works lexicographically on ISO dates — verified
`last_verified<2026-07-10` → 24 files on their repo. And `/regex/i` is the
case-insensitive form.

This is a pure discoverability failure: the rule template
(`crates/hyalo-cli/templates/rule-knowledgebase.md`) shows only `=` and a
bare `title~=link` example — the exact pattern that bit them.

- [ ] Add a 2–3 line operator cheatsheet to the rule template: `!=`,
      date comparison, `/regex/i`, `!KEY`, and note that `title` falls back
      to the H1

## Friction 4 — instruction framing fights the grain

Their honest workflow split: hyalo for discovery/metadata/bulk property ops;
Read/Edit for content surgery. Two structural reasons:
- The harness Edit tool requires a prior *Read* of the file; `hyalo read`
  doesn't satisfy that, so hyalo adds a step for read-modify-write of prose.
- When already in Read/Edit mode for a body change, switching tools for the
  frontmatter bump (`last_verified`) feels like overhead — "the workflow
  pulls toward one tool per file."

Our own CLAUDE.md already concedes body edits to Edit, but the template
leads with "Prefer hyalo for operations on files in this directory" without
naming the split. Reframing toward strengths (discovery, metadata, bulk ops,
lint) would likely *increase* adoption.

- [ ] Reword rule template to state the split explicitly instead of a
      blanket preference

## Bottom line

Core value proposition confirmed by an outside user on a repo designed the
way we hoped people would design them. The actionable set is small: one
behavioral fix (case-insensitive `~=` or an empty-result hint), one feature
(body append), two documentation edits (operator cheatsheet + honest tool
split in the template).
