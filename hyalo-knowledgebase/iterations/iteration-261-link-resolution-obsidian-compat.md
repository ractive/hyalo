---
type: iteration
title: "Iteration 261 — Link resolution: external schemes, non-md targets, escaped pipes, case"
date: 2026-09-03
status: completed
tags:
  - iteration
  - links
  - obsidian
  - dogfooding
branch: iter-261/link-resolution-obsidian-compat
priority: 1
related:
  - "[[dogfood-results/dogfood-v0220-obsidian-vaults]]"
---

# Iteration 261 — Link resolution: external schemes, non-md targets, escaped pipes, case

## Goal

Make hyalo's link resolver agree with Obsidian on the four cases the two real
vaults in [[dogfood-results/dogfood-v0220-obsidian-vaults]] exposed: any
`scheme:` target is external (BUG-2, 2897 `obsidian://show-plugin` links counted
broken on Obsidian Hub), a target with an explicit non-`.md` extension resolves
by Obsidian shortest path against every vault file and is classified as an
attachment (BUG-5 `.base`, BUG-6 `.png`/`.gif`/`.jpg`), a `\|` table-escaped
alias pipe is part of the alias, not the target (BUG-7), and resolution is
case-insensitive everywhere, not only under `links fix --case-insensitive`
(BUG-10). On top of that the `--fields links` JSON grows a link `kind` so broken
links can be bucketed without a filesystem exercise (UX-6), the fuzzy matcher in
`links fix` stops proposing `.base → .md` and `Cat → CatMuse` rewrites (UX-8),
and the anchor-prefix wish from the report's Data Quality section gets a
decision. Together these should take Obsidian Hub's broken count from 3149 down
to the roughly 250 genuinely broken links.

Constraint: **no new CLI flags** from dogfood pressure (project rule). Every fix
here is a resolver behaviour change, an additive JSON field, or a `.hyalo.toml`
key; `links fix --case-insensitive` becomes redundant rather than gaining a
sibling. Out of scope: frontmatter wikilinks as graph edges
([[iterations/iteration-262-frontmatter-wikilinks-first-class]]), the
`links auto` common-word stop-list (UX-9, goes to
[[iterations/iteration-267-help-hints-text-polish]]), and any change to how
`.md` basenames already resolve, which the report confirms works.

## Tasks

### LINK-1: external schemes (BUG-2)

- [x] hyalo-core link parser: treat any target matching `^[a-zA-Z][a-zA-Z0-9+.-]*:`
      (RFC 3986 scheme, so `obsidian://`, `mailto:`, `file://`, `zotero:`) as
      external, exactly like `https://`. Keep the full target text; today it is
      truncated at `?`. A Windows drive letter like `C:\x` must not be
      misread as a scheme (unit test both).
- [x] Unit tests in hyalo-core for `obsidian://show-plugin?id=x`, `mailto:a@b`,
      `[text](file:///x)`, `<obsidian://open?vault=v>` autolinks, and the
      drive-letter negative.
- [x] e2e in `crates/hyalo-cli/tests/e2e`: a vault with one `obsidian://` link
      yields `links.broken == 0` in `summary`, no HYALO006, and
      `links fix` lists nothing under `unfixable`.
- [x] Docs: `find --help` link section, `.claude/skills/hyalo/SKILL.md` broken-link
      paragraph, changelog entry via `hyalo changelog add`.

### LINK-2: explicit non-`.md` targets resolve by shortest path (BUG-5, BUG-6, UX-8)

- [x] hyalo-core file index: index every vault file, not only `*.md`, keyed by
      basename and by vault-relative path, so `![[img.png]]`, `[[Books.base]]`,
      `![[sub/img2.png]]` and `[[Templates/Bases/Books.base]]` all resolve the
      way Obsidian's shortest-path setting does. Reuse the existing ambiguity
      handling (`ambiguous_links`) when two attachments share a basename.
- [x] Classify a resolved non-`.md` target as `attachment` in the link record
      (`path` set, new `kind` per LINK-4); it never counts as broken, never
      appears in `find --broken-links`, HYALO006 or `summary.links.broken`,
      and is not a graph edge for `--orphan`/`--dead-end`.
- [x] `links fix`: never fuzzy-match across an explicit extension. A broken
      `Companies.base` may only be matched against `*.base`; a bare `[[Foo]]`
      only against `.md`. Record this as DEC-266 (tentative) together with the
      attachment classification.
- [x] Add a fuzzy floor test: `[[lithou]]` must not list `lighthousedino.md` at
      confidence 0.0, and nothing at or below the floor may be printed as a
      candidate.
- [x] Unit tests in hyalo-core (basename, relative, full-path, ambiguous
      attachment); e2e with a vault containing `Templates/Bases/Books.base`,
      `02 Attachments/x.png`, and links from three folders.
- [x] Perf check: indexing non-`.md` files must not slow `find --limit 1` on
      Obsidian Hub by more than noise (report baseline 0.14–0.18 s).

### LINK-3: `\|` escaped alias pipe (BUG-7)

- [x] hyalo-core wikilink parser: `[[target\|alias]]` (the form Obsidian writes
      inside tables) yields target `target`, alias `alias`; a lone trailing
      backslash is never part of the target. Cover `[[a\|b]]`, `[[a#h\|b]]`,
      `![[img.png\|200]]`, and the non-escaped `[[a|b]]` regression.
- [x] `links fix` and `mv` rewrite the link keeping the `\|` form byte-for-byte,
      so a table stays a table. e2e: `mv` a file referenced via `[[x\|alias]]`
      inside a table row and diff the row.

### LINK-4: case-insensitive resolution everywhere + link `kind` (BUG-10, UX-6)

- [x] DEC-267 (tentative): case-insensitive resolution becomes the default for
      every consumer of the resolver (`find --broken-links`, `summary`,
      HYALO006, `backlinks`, `--orphan`, `--dead-end`, `mv`). Decide what
      `links fix --case-insensitive` and `case_mismatches` now mean (proposal:
      the flag stays accepted as a no-op with a deprecation note in `--help`,
      `case_mismatches` keeps reporting links whose spelling differs from the
      file so an author can normalise them, but they are no longer broken).
      Exact-case match wins when both spellings exist.
- [x] Additive JSON: every entry in `--fields links` carries
      `kind: "wikilink" | "embed" | "markdown" | "external" | "attachment"`.
      Text mode appends the kind after the arrow only when it is not
      `wikilink`. Fix the `--help` jq example so it also catches
      `broken_anchor: true` entries with `path != null`.
- [x] Update the index snapshot: `kind` and attachment paths must round-trip
      through `.hyalo-index`; bump the snapshot schema version only if the
      record shape changes, and test old-index fallback.
- [x] e2e: `[[AidenLx]]` vs `People/aidenlx.md` resolves in `find --broken-links`
      (0 results), `summary`, and `lint --rule HYALO006`; `--fields links` JSON
      shows `kind` for all five categories.
- [x] Docs: `find --help` (`--fields links` shape), `links fix -h/--help`,
      `.claude/skills/hyalo/SKILL.md`, `.claude/CLAUDE.md` broken-link jq
      recipe, changelog.

### LINK-5: anchor-prefix decision (Data Quality)

- [x] DEC-268 (tentative): the own-KB `[[decision-log#DEC-068]]` anchors (10
      files, 25 links) are correctly broken per Obsidian. Choose between (a) a
      `links fix` suggestion that rewrites `#DEC-068` to the full heading text
      when exactly one heading starts with the anchor, and (b) an opt-in
      `[links] anchor_prefix_match = true` in `.hyalo.toml`. Recommend (a) as
      the default behaviour of `links fix` with (b) rejected because a silent
      prefix match hides typos; never a CLI flag. Record the reasoning in
      [[decision-log]].
- [x] Implement whichever wins, with a unit test for the unique-prefix and the
      ambiguous-prefix (two matching headings → no suggestion) cases, and run
      it on the own KB so the 25 links are either fixed or explicitly waived.

## Acceptance criteria

- [x] Obsidian Hub, cwd `../obsidian-hub`: `hyalo summary --format json --jq
      '.results.links.broken'` drops from 3149 to at most 300 (report estimate
      of real breakage ≈ 250); list the residue with `hyalo find --broken-links
      --format json --limit 0 --jq '.results[].links[] | select(.path==null and
      (.out_of_vault|not)) | .target' | sort | uniq -c | sort -rn | head` and
      confirm no `obsidian://`, `.png`, `.gif`, `.jpg` or `\` entries remain.
- [x] `../obsidian-hub`: `hyalo find --file "00 - Contribute to the Obsidian
      Hub/03 Contributor Notes/03.02 Design Decisions/Content Lists.md" --fields
      links --format json --jq '.results[0].links[] | select(.line==28) |
      {path,kind}'` → path
      `00 - Contribute to the Obsidian Hub/02 Attachments/task-plugins-sorted.png`,
      kind `attachment`.
- [x] `../obsidian-hub`: `hyalo find --file "04 - Guides, Workflows, &
      Courses/Guides/Controlling Obsidian via a Third-party App.md" --fields
      links --format json --jq '.results[0].links[] | select(.line==13) |
      .target'` → `obsidian-advanced-uri` (no trailing backslash), resolved.
- [x] `../obsidian-hub`: `hyalo lint --rule HYALO006 --count` drops from 2897 to
      the same residue as `summary`; `hyalo links fix --format json --jq
      '.results.case_mismatches'` no longer contributes to `broken`, and no
      candidate is printed with confidence `0.0`.
- [x] kepano-obsidian, cwd `../kepano-obsidian`: `hyalo find --broken-links
      --format json --limit 0 --jq '[.results[].links[] | select(.path==null) |
      .target | select(endswith(".base"))] | length'` → `0` (was 53);
      `hyalo lint --strict --rule HYALO006 --count` → 0 errors from `.base`
      targets; `hyalo links fix --format json` proposes no `.base → .md` fix.
- [x] Own KB: `hyalo find --broken-links --count` is unchanged or lower, and the
      DEC-068-style anchors are handled per DEC-268.
- [x] `hyalo find --fields links --format json --limit 1 --jq
      '.results[0].links[0] | has("kind")'` → `true`; `find --help` documents
      the five kinds.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`, `hyalo lint --strict` on the KB,
      and the xtask help-drift check.
- [x] Changelog entry added with `hyalo changelog add`; DEC-266, DEC-267 and
      DEC-268 recorded in [[decision-log]]; skill and `.claude/CLAUDE.md`
      updated in the same PR.

## Links

- [[dogfood-results/dogfood-v0220-obsidian-vaults]]
- [[iterations/iteration-262-frontmatter-wikilinks-first-class]]
- [[decision-log]]

## Outcome (2026-09-03)

Measured with the release build of this branch against the two vaults the plan
names.

**Obsidian Hub** (`../obsidian-hub`, 6540 `.md`):

- `summary --jq '.results.links.broken'` **3149 → 163** (target: ≤ 300).
  93 distinct targets remain, all genuine missing notes (`LaTeX`,
  `plugins-galore`, `remotely-save`, `tldraw`, a literal `*.jpg` glob in prose).
  The residue contains no `obsidian://`, `.png`, `.gif`, `.jpg` or `\` entry.
- `find --file "…/Content Lists.md" --fields links --jq '…select(.line==28)|{path,kind}'`
  → `{"path":"00 - Contribute to the Obsidian Hub/02 Attachments/task-plugins-sorted.png","kind":"attachment"}`.
- `find --file "…/Controlling Obsidian via a Third-party App.md" … select(.line==13)`
  → target `obsidian-advanced-uri` (no trailing backslash), resolved to
  `02 - Community Expansions/02.05 All Community Expansions/Plugins/obsidian-advanced-uri.md`.
- `lint --rule HYALO006 --count` **2897 → 109**. Not identical to `summary`'s
  163 as the plan expected, and correctly so: HYALO006 does not check anchors
  and counts violations, while `summary.links.broken` adds the 99 ambiguous
  short-form links. Both are now the genuine-breakage residue.
- `links fix`: `broken 64, unfixable 38, case_mismatches 48, ambiguous 99,
  fuzzy 26`; **no candidate at confidence 0.0**. `case_mismatches` no longer
  contributes to `broken` on any platform (DEC-267).
- Perf: `find --limit 1` 0.13–0.18 s (baseline 0.14–0.18 s);
  `find --limit 1 --fields links`, which does pay the attachment walk, 0.17 s.

**kepano-obsidian** (`../kepano-obsidian`, 103 `.md`, 30 `.base`):

- `.base` targets with `path == null` **53 → 0**.
- `lint --strict --rule HYALO006`: zero violations from `.base` targets; the
  31 remaining `--strict` errors are unrelated (HYALO005 on Templater
  templates, plus genuinely missing notes such as `Everything is a remix`).
- `links fix`: `broken 11, fixable 0, unfixable 11, fuzzy 0` — no `.base → .md`
  proposal at all (DEC-266).

**Own KB**: `find --broken-links --count` → 10 files, unchanged. All 23 broken
`#DEC-nnn` anchors now carry a `suggested_fragment` with the full heading text,
which is the DEC-268 handling (reported, deliberately not auto-applied).

**Deviations from the plan, recorded in the DECs**

- DEC-267 keeps `links fix --case-insensitive` as a live flag rather than the
  proposed deprecated no-op: resolution no longer depends on it, but
  suppressing the cosmetic `link-case-mismatch` plans is still a real thing to
  want on a case-folded checkout.
- DEC-268's suggestion surfaces on `find --broken-links` (as
  `suggested_fragment`) rather than as a `links fix` rewrite plan. `links fix`
  already points at that report for anchors, and an auto-applied fragment
  rewrite is the hazard option (b) was rejected for.
- One pre-existing e2e (`index_journal::links_fix_apply_updates_persisted_graph`)
  used `[[beta.markdown]] → beta.md` as its fixture; DEC-266 makes that
  deliberately unfixable, so the fixture is now a plain misspelling.
