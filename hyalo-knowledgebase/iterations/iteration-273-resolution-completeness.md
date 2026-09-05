---
type: iteration
title: "Iteration 273 — Resolution completeness: list-typed type, aliases as link targets, anchor-only links, capture boundaries"
date: 2026-09-05
status: planned
tags:
  - iteration
  - links
  - schema
  - obsidian
  - dogfooding
branch: iter-273/resolution-completeness
priority: 3
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-261-270]]"
  - "[[iterations/iteration-261-link-resolution-obsidian-compat]]"
  - "[[iterations/iteration-262-frontmatter-wikilinks-first-class]]"
  - "[[iterations/iteration-266-properties-tags-schema-mutations]]"
  - "[[decision-log]]"
---

# Iteration 273 — Resolution completeness

## Goal

Cases where hyalo's *reading* of a vault disagrees with Obsidian's, from
[[dogfood-results/dogfood-v0220-post-batch-261-270]]: one schema-binding gap that breaks
DEC-281's motivating case, one missing resolution rule (`aliases:`) that also feeds wrong
`links fix` proposals, one mislabelled link kind, and three scanner capture-boundary bugs.
Group 3 of the report's recommendations. Part B is the only piece with design content and
needs a DEC; the rest are corrections with fixtures.

Constraint: **no new CLI flags**. Alias resolution is on by default like case folding
(DEC-267); if an opt-out is wanted it is a `[links]` config key, decided in the DEC.

## Part A — a one-element list `type:` must pass the implicit string constraint (BUG-5, HIGH)

```text
# .hyalo.toml declares type "iteration"
printf -- '---\ntitle: L\ntype: ["[[iteration]]"]\ndate: 2026-09-05\nstatus: planned\nbranch: iter-1/x\ntags: [a]\n---\n' > l.md
hyalo lint --file l.md    # error SCHEMA property "type" expected string, got ["[[iteration]]"]
```

Binding works (the iteration-specific errors fire for a bad `status`), but the implicit
`type: string` constraint every declared type carries then rejects the list. Plain
`type: "[[iteration]]"` is clean; kepano was clean only because it has no schema. Obsidian's
property editor writes exactly this list shape.

### TYPE-1

- [ ] The constraint check on the `type` property must accept the three DEC-281 shapes — a
      string, a `[[Wikilink]]`, a one-element list of either — wherever binding accepts them.
      Route the constraint through `schema::normalize_type_value` (iteration 266's helper) or
      exempt the bound `type` key from the string constraint; do not special-case in the lint
      message path only, `set --validate` uses the same validator.
- [ ] Unit tests for all three shapes under a declared type, plus the two shapes that must still
      fail (`["a","b"]`, `[]`) with the DEC-281 message.
- [ ] e2e on the own KB schema with a fixture file; and on a kepano copy with a `.hyalo.toml`
      that declares `Authors`: `lint --strict` must report zero `expected string` errors for
      `type: ["[[Authors]]"]` files (15 of them).

## Part B — frontmatter `aliases:` resolve wikilinks (BUG-6, MEDIUM; DEC required)

```text
printf -- '---\ntitle: Leah Ferguson\naliases:\n- Leah\n---\n' > 'al/Leah Ferguson.md'
printf -- 'see [[Leah]]\n' > al/src.md
hyalo find --file al/src.md --fields links --jq '.results[0].links[0].path'   # null
```

Obsidian resolves `[[Leah]]`. On `../obsidian-hub` 7 of 47 genuinely-broken targets are
declared aliases (9 occurrences), the vault has 5489 distinct aliases, and `links fix
--apply-fuzzy` would rewrite `Leah → Lewuathe.md` (0.87), `Cat → CatMuse.md`,
`jamesb → jamesgreenblue.md`. No DEC covers alias resolution.

### ALIAS-1: decide

- [ ] Write the DEC: which property (Obsidian: `aliases`, string or list; nothing else),
      precedence (a filename or path match always wins over an alias, as in Obsidian), alias
      shared by two notes → ambiguous (reported like a stem collision, not resolved), alias
      matching is case-folded like DEC-267, `[[alias#Heading]]` and `[[alias|label]]` work,
      `kind` stays `wikilink` and the entry gains `via: "alias"` (or similar) so a consumer can
      tell. State whether `[links] aliases = false` exists as an opt-out; default on.
- [ ] Decide index handling: the alias map must be available on `--index` reads. Properties
      are already in the snapshot, so build it from indexed frontmatter at load rather than
      changing the on-disk format; confirm and note the cost on the Hub (5489 aliases).

### ALIAS-2: implement

- [ ] Resolution in `hyalo-core` (file index / link resolver from iteration 261): alias map
      built once per scan, consulted after stem/path lookup fails.
- [ ] `links fix`: a target that resolves via alias is not broken and gets no plan; fuzzy
      matching must never propose a rewrite for a target that is a declared alias of *any*
      note. Text and JSON say `resolved via alias` where a consumer would otherwise wonder.
- [ ] `mv`: renaming a note does not change its aliases, so links via alias need no rewrite —
      but `mv` must not report them as broken afterwards either. Test it.
- [ ] `backlinks`, `--orphan`, `--dead-end`, `summary.links`, HYALO006: all consistent with
      the new resolution; `--index` parity byte-identical.
- [ ] Tests: unique alias resolves; filename beats alias; two notes sharing an alias →
      ambiguous; alias with fragment and label; case-folded alias; alias listed as a plain
      string (`aliases: Leah`) and as a list; a note whose alias equals its own stem.
- [ ] Measure on the Hub: `summary.links.broken` (163 before), `links fix` `fuzzy` above the
      floor (8 before, three of them alias-backed), `find --broken-links --count` time (0.51 s
      before) — must stay within noise.

## Part C — `[text](#fragment)` is a markdown link, not a wikilink (BUG-8, MEDIUM)

MDN has no wikilinks, yet its histogram says `wikilink: 2822`; GitHub Docs `1552`. An
anchor-only markdown link is reported as `{"kind":"wikilink","label":null,"target":""}`.

### ANCHOR-1

- [ ] Emit `kind: "markdown"`, `label` = link text, `target: ""` (same-file marker, as today
      for `[[#frag]]`), fragment and `broken_anchor` as now. Disk and index agree today, so the
      fix is in extraction only.
- [ ] Unit + e2e; MDN whole-vault histogram (`--index-file`, read-only) shows `wikilink: 0`.

## Part D — scanner capture boundaries (BUG-16, BUG-15, BUG-21; LOW/MEDIUM)

### BOUND-1: a wikilink target never contains `]` or `[[` (BUG-16)

`see [[Leah] here and [[Target]` yields one link whose target is `Leah] here and [[Target`.
Stop the capture at the first `]` or `[[`; a `[[` that is not followed by `]]` before either is
prose. `[[ ]]` (whitespace target) is skipped like `[[]]`. Real Hub occurrence:
`Obsidian Community Talks.md:64`.

- [ ] Fix in the body scanner and in the frontmatter raw-text scanner (iteration 262) — both
      have their own capture loop.
- [ ] Tests for `[[a] b [[c]]` (one link, `c`), `[[a]`, `[[ ]]`, `[[a]]]` (link `a`, stray
      `]`), and a table row with `\|`.

### BOUND-2: a frontmatter flow list starting `[[[` (BUG-15)

`related: [[[iterations/x]], [[research/y]]]` — the raw-text scanner starts at the first `[[`
so the third `[` becomes part of the target. Treat `[[[x]]` as `[` + `[[x]]`: when the capture
would begin with `[`, advance one character. Own KB has exactly one such line
(`research/agent-ergonomics-ralph-loop-port-2026-08-24.md:7`); fix the file too in this PR
(quote the items) once the scanner handles it, and keep a test fixture with the raw shape.

- [ ] Scanner fix + unit test; e2e: `mv iterations/iteration-206-… iterations/done/…` dry-run
      on a KB copy now lists the research file (9 files, not 8).

### BOUND-3: `[y](<(https://…)>)` is external (BUG-21)

A markdown destination in `<…>` whose first non-`(` character starts a URI scheme is
`external`; today it is an internal `markdown` link with target `(https://…`, counted broken and
fuzzy-matched at 0.57. Real Hub occurrence in a 2021 Roundup.

- [ ] Fix + unit test; `links fix` never proposes a candidate for it.

## Shared closing tasks

- [ ] Changelog entries via `hyalo changelog add` (one per part).
- [ ] The alias DEC recorded in [[decision-log]].
- [ ] Docs: `find --help` link-kind paragraph, `links fix --help`, the bundled skill and rule
      template, `docs/` link pages — alias resolution and the `via` marker documented where
      DEC-267 case folding is.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*` gates
      (invoke as `CARGO_MANIFEST_DIR=<repo>/crates/xtask ./target/debug/xtask <gate>`).

## Acceptance criteria

- [ ] BUG-5 fixture lints clean under the own KB schema; kepano copy with a declared `Authors`
      type: zero `expected string` errors on the 15 list-typed files; `["a","b"]` and `[]`
      still rejected with the DEC-281 message.
- [ ] BUG-6 fixture: `[[Leah]]` resolves to `al/Leah Ferguson.md` with the `via` marker; Hub:
      `links fix --apply-fuzzy --dry-run` no longer lists `Leah`, `Cat`, `jamesb`;
      `summary.links.broken` drops by the alias-backed count and the Outcome records the new
      figure; `--index` parity byte-identical; `find --broken-links --count` within noise of
      0.51 s.
- [ ] MDN whole-vault link histogram reports `wikilink: 0`; anchor-only links carry
      `kind: markdown` and their label.
- [ ] `[[Leah] here and [[Target]` yields one link, `Target`; the own KB research file's
      `related` list resolves and `mv` dry-run includes it.
- [ ] `[y](<(https://example.com/x)>)` is `kind: external`, never broken, never fuzzy-matched.
- [ ] Iteration 261/262 fixtures (link kinds, attachments, `\|`, case folding, frontmatter
      scanner) stay green.
- [ ] Gates green; four changelog entries; one DEC; help, skill and rule template updated.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-261-270]] — BUG-5, BUG-6, BUG-8, BUG-15, BUG-16, BUG-21
- [[iterations/iteration-261-link-resolution-obsidian-compat]] — resolver, kinds, DEC-266/267/268
- [[iterations/iteration-262-frontmatter-wikilinks-first-class]] — raw-text frontmatter scanner
- [[iterations/iteration-266-properties-tags-schema-mutations]] — `normalize_type_value`, DEC-281
- [[decision-log]]
