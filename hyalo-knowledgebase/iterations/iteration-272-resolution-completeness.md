---
type: iteration
title: "Iteration 272 — Resolution completeness: list-typed type, aliases as link targets, anchor-only links, capture boundaries, resolution feature gaps"
date: 2026-09-05
status: planned
tags:
  - iteration
  - links
  - schema
  - obsidian
  - dogfooding
branch: iter-272/resolution-completeness
priority: 2
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-261-270]]"
  - "[[iterations/iteration-261-link-resolution-obsidian-compat]]"
  - "[[iterations/iteration-262-frontmatter-wikilinks-first-class]]"
  - "[[iterations/iteration-266-properties-tags-schema-mutations]]"
  - "[[decision-log]]"
---

# Iteration 272 — Resolution completeness

## Goal

Cases where hyalo's *reading* of a vault disagrees with Obsidian's, from
[[dogfood-results/dogfood-v0220-post-batch-261-270]]: one schema-binding gap that breaks
DEC-281's motivating case, one missing resolution rule (`aliases:`) that also feeds wrong
`links fix` proposals, one mislabelled link kind, three scanner capture-boundary bugs, and the
report's four resolution-related feature gaps as decide-or-implement tasks. Group 3 of the
report's recommendations plus the resolution half of its "feature gaps" section. Parts B and E
carry design content and need DECs; the rest are corrections with fixtures.

Constraint: **no new CLI flags**. Alias resolution is on by default like case folding
(DEC-267); an opt-out, if wanted, is a `[links]` config key decided in the DEC.

## Part A — a one-element list `type:` passes the implicit string constraint (BUG-5, HIGH)

```text
# .hyalo.toml declares type "iteration"
printf -- '---\ntitle: L\ntype: ["[[iteration]]"]\ndate: 2026-09-05\nstatus: planned\nbranch: iter-1/x\ntags: [a]\n---\n' > l.md
hyalo lint --file l.md    # error SCHEMA property "type" expected string, got ["[[iteration]]"]
```

Binding works (iteration-specific errors fire for a bad `status`), but the implicit
`type: string` constraint every declared type carries rejects the list. kepano was clean only
because it has no schema; Obsidian's property editor writes exactly this shape.

- [ ] The constraint check on `type` accepts the three DEC-281 shapes wherever binding does.
      Route through `schema::normalize_type_value` (iteration 266) or exempt the bound `type`
      key from the string constraint; `set --validate` uses the same validator, so fix it there,
      not in the lint message path.
- [ ] Unit tests for the three shapes under a declared type, plus `["a","b"]` and `[]` still
      failing with the DEC-281 message.
- [ ] e2e on the own KB schema; and on a kepano copy with a `.hyalo.toml` declaring `Authors`:
      `lint --strict` reports zero `expected string` errors for the 15 `type: ["[[Authors]]"]`
      files.

## Part B — frontmatter `aliases:` resolve wikilinks (BUG-6, MEDIUM; DEC required)

```text
printf -- '---\ntitle: Leah Ferguson\naliases:\n- Leah\n---\n' > 'al/Leah Ferguson.md'
printf -- 'see [[Leah]]\n' > al/src.md
hyalo find --file al/src.md --fields links --jq '.results[0].links[0].path'   # null
```

On `../obsidian-hub` 7 of 47 genuinely-broken targets are declared aliases (9 occurrences),
the vault has 5489 distinct aliases, and `links fix --apply-fuzzy` would rewrite
`Leah → Lewuathe.md` (0.87), `Cat → CatMuse.md`, `jamesb → jamesgreenblue.md`.

### ALIAS-1: decide

- [ ] DEC: property is `aliases` (Obsidian; string or list), nothing else; a filename or path
      match always beats an alias; an alias shared by two notes is ambiguous (reported like a
      stem collision, not resolved); matching is case-folded like DEC-267; `[[alias#Heading]]`
      and `[[alias|label]]` work; `kind` stays `wikilink` and the entry gains `via: "alias"`.
      State whether `[links] aliases = false` exists; default on.
- [ ] Index: build the alias map from the snapshot's indexed frontmatter at load rather than
      changing the on-disk format; note the cost on the Hub.

### ALIAS-2: implement

- [ ] Resolver in `hyalo-core` (iteration 261's file index): alias map built once per scan,
      consulted after stem/path lookup fails.
- [ ] `links fix`: a target that resolves via alias is not broken and gets no plan; fuzzy
      matching never proposes a rewrite for a target that is a declared alias of any note.
- [ ] `mv`: renaming a note does not change its aliases; links via alias need no rewrite and
      are not reported broken afterwards. Test it.
- [ ] `backlinks`, `--orphan`, `--dead-end`, `summary.links`, HYALO006 consistent; `--index`
      parity byte-identical.
- [ ] Tests: unique alias; filename beats alias; shared alias → ambiguous; alias with fragment
      and label; case-folded alias; `aliases: Leah` string form; alias equal to its own stem.
- [ ] Hub measurements: `summary.links.broken` (163 before), `links fix` fuzzy above the floor
      (8 before, three alias-backed), `find --broken-links --count` time (0.51 s before).

## Part C — `[text](#fragment)` is a markdown link, not a wikilink (BUG-8, MEDIUM)

MDN has no wikilinks, yet its histogram says `wikilink: 2822` (GitHub Docs 1552): an
anchor-only markdown link is reported as `{"kind":"wikilink","label":null,"target":""}`.

- [ ] Emit `kind: "markdown"`, `label` = link text, `target: ""` (same-file marker), fragment
      and `broken_anchor` as now. Extraction only; disk and index already agree.
- [ ] Unit + e2e; MDN whole-vault histogram (`--index-file`, read-only) shows `wikilink: 0`.

## Part D — scanner capture boundaries (BUG-16, BUG-15, BUG-21)

### BOUND-1: a wikilink target never contains `]` or `[[` (BUG-16)

`see [[Leah] here and [[Target]` yields one link whose target is `Leah] here and [[Target`.
Real Hub occurrence: `Obsidian Community Talks.md:64`.

- [ ] Stop the capture at the first `]` or `[[`; a `[[` not followed by `]]` before either is
      prose; `[[ ]]` is skipped like `[[]]`. Fix both the body scanner and the iteration-262
      frontmatter raw-text scanner.
- [ ] Tests: `[[a] b [[c]]` (one link, `c`), `[[a]`, `[[ ]]`, `[[a]]]`, a table row with `\|`.

### BOUND-2: a frontmatter flow list starting `[[[` (BUG-15)

`related: [[[iterations/x]], [[research/y]]]` — the raw-text scanner captures
`[iterations/x`. Own KB has one such line
(`research/agent-ergonomics-ralph-loop-port-2026-08-24.md:7`).

- [ ] When a capture would begin with `[`, advance one character (`[[[x]]` = `[` + `[[x]]`);
      unit test; e2e: `mv iterations/iteration-206-… iterations/done/…` dry-run on a KB copy
      lists the research file (9 files, not 8). Fix the KB file too (quote the items) and keep
      a fixture with the raw shape.

### BOUND-3: `[y](<(https://…)>)` is external (BUG-21)

- [ ] A markdown destination in `<…>` whose first non-`(` character starts a URI scheme is
      `external`, never broken, never fuzzy-matched; unit test with the Hub's 2021 Roundup line.

## Part E — resolution feature gaps: decide each, DEC or implement

From the report's "Feature gaps" section. Each ends in a DEC line (implement, backlog, or
won't-do with reasoning). Implement only what fits the resolver work above without a new flag.

- [ ] **`![alt](file.png)` as an attachment link.** Skipped by design in
      `hyalo-core/src/links.rs` (`markdown_image_skipped` test) while `![[img.png]]` and
      `[alt](img.png)` are attachments; MDN's whole-vault histogram is `attachment: 2` against
      thousands of images, so a missing image never surfaces. Extract as `kind: attachment`
      with the `![[x]]` resolution rules; measure MDN broken-attachment count; likely implement.
- [ ] **`_`↔`-` normalisation before the `suggested_fragment` prefix test.** MDN slugs headings
      with underscores; 1242 of 1254 broken anchors on the css copy get no suggestion. DEC-268
      forbids silent matching, not suggesting; likely implement (suggestion only).
- [ ] **Block references and slug anchors as broken anchors.** `[[Target#^nope]]` and
      `[[Target#section-one]]` are never reported broken; Obsidian breaks the first. Decide
      whether `^block` ids are checked (needs a block-id scan) — likely backlog with a DEC line.
- [ ] **`[links] redirect_property = "redirect_from"`.** GitHub Docs links point at historical
      URLs served via `redirect_from:` lists; 1569 of 1624 "broken" files. An opt-in config key
      (not a flag) that adds those values as resolution aliases — sits on Part B's alias map.
      Decide; implement if the alias map makes it a few lines, else backlog.

## Shared closing tasks

- [ ] Changelog entries via `hyalo changelog add` (one per part that changes behaviour).
- [ ] DECs: aliases (B), one line per Part E item (may share a DEC).
- [ ] Docs: `find --help` link-kind paragraph, `links fix --help`, bundled skill and rule
      template, `docs/` link pages — alias resolution and `via` documented where DEC-267 is.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*` gates
      (invoke as `CARGO_MANIFEST_DIR=<repo>/crates/xtask ./target/debug/xtask <gate>`).

## Acceptance criteria

- [ ] BUG-5 fixture lints clean under the own KB schema; kepano copy with `Authors` declared:
      zero `expected string` errors on 15 list-typed files; `["a","b"]` and `[]` still rejected.
- [ ] BUG-6: `[[Leah]]` resolves to `al/Leah Ferguson.md` with `via`; Hub `links fix
      --apply-fuzzy --dry-run` no longer lists `Leah`, `Cat`, `jamesb`; `summary.links.broken`
      drops by the alias-backed count (new figure in the Outcome); `--index` parity
      byte-identical; `find --broken-links --count` within noise of 0.51 s.
- [ ] MDN histogram `wikilink: 0`; anchor-only links carry `kind: markdown` and their label.
- [ ] `[[Leah] here and [[Target]` yields one link `Target`; the KB research file resolves and
      `mv` dry-run includes it; `[y](<(https://…)>)` is external.
- [ ] Every Part E item has a DEC line; implemented ones have tests and a measurement.
- [ ] Iteration 261/262 fixtures stay green.
- [ ] Gates green; changelog; DECs; help, skill and rule template updated.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-261-270]] — BUG-5, 6, 8, 15, 16, 21; feature gaps
- [[iterations/iteration-261-link-resolution-obsidian-compat]] — resolver, kinds, DEC-266/267/268
- [[iterations/iteration-262-frontmatter-wikilinks-first-class]] — raw-text frontmatter scanner
- [[iterations/iteration-266-properties-tags-schema-mutations]] — `normalize_type_value`, DEC-281
- [[decision-log]]
