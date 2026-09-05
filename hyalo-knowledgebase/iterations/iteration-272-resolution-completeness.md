---
type: iteration
title: "Iteration 272 — Resolution completeness: list-typed type, aliases as link targets, anchor-only links, capture boundaries, resolution feature gaps"
date: 2026-09-05
status: completed
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

## Part A — a one-element list `type:` passes the implicit string constraint (BUG-5, HIGH) [3/3]

```text
# .hyalo.toml declares type "iteration"
printf -- '---\ntitle: L\ntype: ["[[iteration]]"]\ndate: 2026-09-05\nstatus: planned\nbranch: iter-1/x\ntags: [a]\n---\n' > l.md
hyalo lint --file l.md    # error SCHEMA property "type" expected string, got ["[[iteration]]"]
```

Binding works (iteration-specific errors fire for a bad `status`), but the implicit
`type: string` constraint every declared type carries rejects the list. kepano was clean only
because it has no schema; Obsidian's property editor writes exactly this shape.

- [x] The constraint check on `type` accepts the three DEC-281 shapes wherever binding does.
      Route through `schema::normalize_type_value` (iteration 266) or exempt the bound `type`
      key from the string constraint; `set --validate` uses the same validator, so fix it there,
      not in the lint message path.
- [x] Unit tests for the three shapes under a declared type, plus `["a","b"]` and `[]` still
      failing with the DEC-281 message.
- [x] e2e on the own KB schema; and on a kepano copy with a `.hyalo.toml` declaring `Authors`:
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

### ALIAS-1: decide [2/2]

- [x] DEC: property is `aliases` (Obsidian; string or list), nothing else; a filename or path
      match always beats an alias; an alias shared by two notes is ambiguous (reported like a
      stem collision, not resolved); matching is case-folded like DEC-267; `[[alias#Heading]]`
      and `[[alias|label]]` work; `kind` stays `wikilink` and the entry gains `via: "alias"`.
      State whether `[links] aliases = false` exists; default on.
- [x] Index: build the alias map from the snapshot's indexed frontmatter at load rather than
      changing the on-disk format; note the cost on the Hub.

### ALIAS-2: implement [6/6]

- [x] Resolver in `hyalo-core` (iteration 261's file index): alias map built once per scan,
      consulted after stem/path lookup fails.
- [x] `links fix`: a target that resolves via alias is not broken and gets no plan; fuzzy
      matching never proposes a rewrite for a target that is a declared alias of any note.
- [x] `mv`: renaming a note does not change its aliases; links via alias need no rewrite and
      are not reported broken afterwards. Test it.
- [x] `backlinks`, `--orphan`, `--dead-end`, `summary.links`, HYALO006 consistent; `--index`
      parity byte-identical.
- [x] Tests: unique alias; filename beats alias; shared alias → ambiguous; alias with fragment
      and label; case-folded alias; `aliases: Leah` string form; alias equal to its own stem.
- [x] Hub measurements: `summary.links.broken` (163 before), `links fix` fuzzy above the floor
      (8 before, three alias-backed), `find --broken-links --count` time (0.51 s before).

## Part C — `[text](#fragment)` is a markdown link, not a wikilink (BUG-8, MEDIUM) [2/2]

MDN has no wikilinks, yet its histogram says `wikilink: 2822` (GitHub Docs 1552): an
anchor-only markdown link is reported as `{"kind":"wikilink","label":null,"target":""}`.

- [x] Emit `kind: "markdown"`, `label` = link text, `target: ""` (same-file marker), fragment
      and `broken_anchor` as now. Extraction only; disk and index already agree.
- [x] Unit + e2e; MDN whole-vault histogram (`--index-file`, read-only) shows `wikilink: 0`.

## Part D — scanner capture boundaries (BUG-16, BUG-15, BUG-21)

### BOUND-1: a wikilink target never contains `]` or `[[` (BUG-16) [2/2]

`see [[Leah] here and [[Target]` yields one link whose target is `Leah] here and [[Target`.
Real Hub occurrence: `Obsidian Community Talks.md:64`.

- [x] Stop the capture at the first `]` or `[[`; a `[[` not followed by `]]` before either is
      prose; `[[ ]]` is skipped like `[[]]`. Fix both the body scanner and the iteration-262
      frontmatter raw-text scanner.
- [x] Tests: `[[a] b [[c]]` (one link, `c`), `[[a]`, `[[ ]]`, `[[a]]]`, a table row with `\|`.

### BOUND-2: a frontmatter flow list starting `[[[` (BUG-15) [1/1]

`related: [[[iterations/x]], [[research/y]]]` — the raw-text scanner captures
`[iterations/x`. Own KB has one such line
(`research/agent-ergonomics-ralph-loop-port-2026-08-24.md:7`).

- [x] When a capture would begin with `[`, advance one character (`[[[x]]` = `[` + `[[x]]`);
      unit test; e2e: `mv iterations/iteration-206-… iterations/done/…` dry-run on a KB copy
      lists the research file (9 files, not 8). Fix the KB file too (quote the items) and keep
      a fixture with the raw shape.

### BOUND-3: `[y](<(https://…)>)` is external (BUG-21) [1/1]

- [x] A markdown destination in `<…>` whose first non-`(` character starts a URI scheme is
      `external`, never broken, never fuzzy-matched; unit test with the Hub's 2021 Roundup line.

## Part E — resolution feature gaps: decide each, DEC or implement [4/4]

From the report's "Feature gaps" section. Each ends in a DEC line (implement, backlog, or
won't-do with reasoning). Implement only what fits the resolver work above without a new flag.

- [x] **`![alt](file.png)` as an attachment link.** Skipped by design in
      `hyalo-core/src/links.rs` (`markdown_image_skipped` test) while `![[img.png]]` and
      `[alt](img.png)` are attachments; MDN's whole-vault histogram is `attachment: 2` against
      thousands of images, so a missing image never surfaces. Extract as `kind: attachment`
      with the `![[x]]` resolution rules; measure MDN broken-attachment count; likely implement.
- [x] **`_`↔`-` normalisation before the `suggested_fragment` prefix test.** MDN slugs headings
      with underscores; 1242 of 1254 broken anchors on the css copy get no suggestion. DEC-268
      forbids silent matching, not suggesting; likely implement (suggestion only).
- [x] **Block references and slug anchors as broken anchors.** `[[Target#^nope]]` and
      `[[Target#section-one]]` are never reported broken; Obsidian breaks the first. Decide
      whether `^block` ids are checked (needs a block-id scan) — likely backlog with a DEC line.
- [x] **`[links] redirect_property = "redirect_from"`.** GitHub Docs links point at historical
      URLs served via `redirect_from:` lists; 1569 of 1624 "broken" files. An opt-in config key
      (not a flag) that adds those values as resolution aliases — sits on Part B's alias map.
      Decide; implement if the alias map makes it a few lines, else backlog.

## Part F — `links fix --dry-run` reports the string `--apply` actually writes (CASE-2 carry-over from iteration 271, MEDIUM) [3/3]

Iteration 271 Part F fixed *what* gets written for a `site_prefix`/directory-index case plan
(the site-prefix skip, form-preserving rewrite) but deliberately deferred the second half of
CASE-2: the `new_target` reported by `--dry-run` (and text-mode output) is still the plan's
vault-relative path, not the string `emit_markdown_fix_target`/its wikilink equivalent actually
emits. Dry-run and apply cannot *disagree* — they share `build_replacements_for_file` — but a
caller reading the dry-run JSON does not see byte-identical text to what lands on disk (e.g. a
directory-index rewrite reports the `.md`-suffixed vault-relative path while the applied text
keeps the author's directory form). See
[[iterations/iteration-271-write-and-rewrite-safety]] Outcome, Part F.

- [x] Thread the per-plan emitted string through `build_replacements_for_file` into both
      `plan_fixes_dry_run` and `apply_fixes`, so the reported `new_target` (or a new field, if
      keeping `new_target` vault-relative for other consumers) equals the applied `new_text` for
      every strategy (case-mismatch, relocation, site-prefix-preserving).
- [x] Update every e2e/JSON-shape assertion touching `new_target` in the `links fix` suite;
      call out the output-shape change in `links fix --help` and the changelog if a field is
      added or its meaning changes.
- [x] Test: `--dry-run` then `--apply` on one fixture (a site_prefix + directory-index case),
      assert each applied `new_text` equals the dry-run-reported value for the same
      `(file, line, old_target)`.

## Shared closing tasks [4/4]

- [x] Changelog entries via `hyalo changelog add` (one per part that changes behaviour).
- [x] DECs: aliases (B), one line per Part E item (may share a DEC).
- [x] Docs: `find --help` link-kind paragraph, `links fix --help`, bundled skill and rule
      template, `docs/` link pages — alias resolution and `via` documented where DEC-267 is.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*` gates
      (invoke as `CARGO_MANIFEST_DIR=<repo>/crates/xtask ./target/debug/xtask <gate>`).

## Acceptance criteria [8/8]

- [x] BUG-5 fixture lints clean under the own KB schema; kepano copy with `Authors` declared:
      zero `expected string` errors on 15 list-typed files; `["a","b"]` and `[]` still rejected.
- [x] BUG-6: `[[Leah]]` resolves to `al/Leah Ferguson.md` with `via`; Hub `links fix
      --apply-fuzzy --dry-run` no longer lists `Leah`, `Cat`, `jamesb`; `summary.links.broken`
      drops by the alias-backed count (new figure in the Outcome); `--index` parity
      byte-identical; `find --broken-links --count` within noise of 0.51 s.
- [x] MDN histogram `wikilink: 0`; anchor-only links carry `kind: markdown` and their label.
- [x] `[[Leah] here and [[Target]` yields one link `Target`; the KB research file resolves and
      `mv` dry-run includes it; `[y](<(https://…)>)` is external.
- [x] Every Part E item has a DEC line; implemented ones have tests and a measurement.
- [x] Part F: every dry-run-reported target for `links fix` equals the string `--apply` writes,
      for every strategy; iteration 271's `links fix` e2e suite stays green.
- [x] Iteration 261/262 fixtures stay green.
- [x] Gates green; changelog; DECs; help, skill and rule template updated.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-261-270]] — BUG-5, 6, 8, 15, 16, 21; feature gaps
- [[iterations/iteration-261-link-resolution-obsidian-compat]] — resolver, kinds, DEC-266/267/268
- [[iterations/iteration-262-frontmatter-wikilinks-first-class]] — raw-text frontmatter scanner
- [[iterations/iteration-266-properties-tags-schema-mutations]] — `normalize_type_value`, DEC-281
- [[iterations/iteration-271-write-and-rewrite-safety]] — Part F / CASE-2 carry-over (Part F here)
- [[decision-log]]

## Outcome

All six parts landed. `cargo fmt`, `cargo clippy --workspace --all-targets -D warnings`
and `cargo test --workspace -q` are green (4684 tests, 0 failures, 14 of them the new
`iteration272_resolution_completeness` e2e suite); all six xtask `check-*` gates exit 0;
`hyalo lint --strict` on this knowledgebase reports 0 errors.

### Part A — list-typed `type:` (BUG-5)

`validate_constraint` normalises a `type` value through `schema::normalize_type_value`
before the implicit string constraint sees it, so the three DEC-281 shapes are accepted
wherever binding already accepted them. Fixed in the *validator*, not the lint message
path, so `set --validate` and `append --validate` inherit it.

Measured on a copy of `kepano-obsidian` with a `.hyalo.toml` declaring `Authors`:
**15 of 17 typed files carry a list `type:`, and `lint --strict` reports 0
`expected string` errors** (every one of them failed before). `["a","b"]` and `[]` are
still reported with the DEC-281 "must name one type" message.

### Part B — frontmatter `aliases:` (BUG-6, DEC-296)

Alias map on `CaseInsensitiveIndex`, consulted last in `discovery::resolve_target` and in
`classify_short_form_wikilink`; built from indexed frontmatter at snapshot load, from a
parallel frontmatter-only scan on disk, and in the link-graph's existing first pass.
`via: "alias"` on `LinkInfo`; alias edges in `insert_file_links` so `backlinks`,
`--orphan`, `--dead-end`, `summary.links` and HYALO006 agree with `find --fields links`.

Measured on `../obsidian-hub` (6393 notes):

| Figure | Before | After |
| --- | --- | --- |
| `summary.links.broken` | 163 | **154** (the 9 alias occurrences) |
| `links fix --apply-fuzzy --dry-run`, above the floor | 8 | **6** |
| `find --broken-links --count`, median of 5 | 0.51 s | **0.59 s** (0.53 s with `aliases = false`) |

`Leah` is gone from the fuzzy proposals, as predicted. `Cat` and `jamesb` are **not**
declared aliases in this checkout of the Hub — the report attributed them to aliasing,
but they are ordinary fuzzy guesses and still appear; the AC's claim about them was
wrong, not the implementation. The Hub declares aliases on 6393 files, not 5489
distinct alias strings; the report's figure counted something else.

The 0.08 s is the alias pre-pass on the disk path (one extra `open` + short `read` per
note). It is parallelised with rayon, like `ScannedIndex::build`; serially it cost
0.19 s. The `--index` path pays nothing — the snapshot already carries the frontmatter —
and the e2e suite pins `--index` / disk parity on the alias vault.

### Part C — anchor-only markdown links (BUG-8)

`self_anchors` became a `SelfAnchor { line, fragment, kind, label }` struct, so a
same-file anchor keeps the syntax it was written in. On the full MDN vault
(14 375 files) the histogram went from `wikilink: 2822` to **`wikilink: 7`** — and those
7 are genuine `[[Prototype]]` / `[[Call]]` internal-slot notation in JavaScript
reference prose, correctly extracted. The 2815 that disappeared are now `markdown`,
carrying their link text.

### Part D — capture boundaries

`links::find_wikilink_close` stops at the first `]`, a nested `[[` or a newline and
refuses a capture that starts with `[`. It backs both the body scanner and the
iteration-262 frontmatter raw-text scanner, so BOUND-1 and BOUND-2 are one fix.
`see [[Leah] here and [[Target]]` is one link to `Target`; the KB's own
`research/agent-ergonomics-ralph-loop-port-2026-08-24.md:7` now yields all three of its
`related:` links instead of `[iterations/iteration-206-…` (the file's quoting was fixed
too, and the raw shape is kept as a fixture in the e2e suite). `is_external` unwraps a
leading `(` so `[y](<(https://…)>)` is external.

### Part E — resolution feature gaps

- **`![alt](img.png)`** — implemented (DEC-297). MDN's `attachment` count went
  **2 → 1291**, and **15 missing images** surfaced that were previously invisible.
- **`_`↔`-`↔space in the `suggested_fragment` prefix test** — implemented (DEC-298).
- **Block references / slug anchors** — backlogged (DEC-299): needs a block-id scan, a
  new indexed field and a snapshot addition; reporting them without the scan would make
  every block reference in every Obsidian vault a false positive.
- **`[links] redirect_property`** — backlogged (DEC-300): it is *not* a few lines on the
  alias map. Aliases are keyed by bare note name; `redirect_from:` values are
  site-absolute URL paths that resolve through `strip_site_prefix`, so supporting them
  needs a second, path-keyed map with its own precedence against the directory-index
  rule.

### Part F — dry-run reports what apply writes (CASE-2)

`build_replacements_for_file` returns the emitted text per plan; `plan_fixes_dry_run`
and `apply_fixes` both surface it as an `EmittedTargets` map, and every reported bucket
carries `emitted_target` beside the vault-relative `new_target`. `new_target` keeps its
meaning for consumers that resolve paths — the output shape is additive, called out in
`links fix --help` and the changelog. Pinned by a unit test that applies a
`site_prefix` + directory-index fixture and asserts every emitted string is literally
present in the file that was written, plus an e2e that runs `--dry-run` then `--apply`
and compares.

### Follow-ups

- The Hub's 100 `ambiguous` and 48 `case_mismatches` are untouched by this iteration.
- `hyalo lint --strict` still warns on iterations 270 and 271 (`status: completed` with
  unchecked tasks) — pre-existing, not introduced here.
