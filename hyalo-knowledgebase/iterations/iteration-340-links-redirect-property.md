---
type: iteration
title: "Iteration 340: [links] redirect_property resolves a declared URL path"
date: 2026-11-08
status: completed
tags:
  - iteration
  - links
  - resolution
branch: iter-340/links-redirect-property
related:
  - "[[backlog/done/link-resolution-redirect-property]]"
  - "[[iterations/iteration-272-resolution-completeness]]"
---

# Iteration 340: `[links] redirect_property` resolves a declared URL path

GitHub Docs notes declare every URL they used to live at in `redirect_from:`, and links
elsewhere in the corpus still use those URLs. 91 % of its 6 082 broken markdown links name
one. DEC-300 declined to treat that like DEC-296's alias map; this iteration builds the
second, path-keyed map DEC-300 asked for, opt-in, and measures it on the corpus that
motivated it. Decision: [[decision-log]] DEC-415 (written before the code).

## Tasks

- [x] Branch from `origin/main` `43b5bab9`, pristine release binary kept as the baseline
- [x] DEC-415 written first: precedence, ambiguity, `via`, snapshot, `mv`, `links fix`
- [x] `[links] redirect_property` in the config, `hyalo config`, process-wide install
- [x] Redirect map in `CaseInsensitiveIndex`, built from disk and from snapshot properties
- [x] Last-resort lookup in `catalog::resolve`; writers resolve with the step off
- [x] `via: "redirect"`, ambiguity (`find`, HYALO006, `links fix`), graph edges
- [x] ts-rs declarations, Pi companions and both freshness gates
- [x] e2e tests: resolution, precedence, ambiguity, unset, `via`, graph, `links fix`, `--index`, `mv`
- [x] Mutation-check the precedence and the ambiguity rule
- [x] Measure GitHub Docs (a clone with its own config), Obsidian Hub and MDN unchanged
- [x] Docs: `.claude/CLAUDE.md`, rule and skill templates, `docs/configuration.md`, CHANGELOG
- [x] fmt, clippy, tests, `hyalo lint`, help-drift and Pi gates
- [x] Open the PR

## Results

**Verdict: shipped, opt-in.** Measured read-only on APFS clones of `~/devel/docs` (GitHub
Docs, 3 710 notes, `dir = "content"`, effective `site_prefix` `content`), one clone with the
setting added to its own `.hyalo.toml` and one without, so base and new saw the same file set
(no `.hyalo-index` in either; the real checkout was never written). Base is main `43b5bab9`.

| | base, unset | new, unset | new, `redirect_property = "redirect_from"` |
|---|---|---|---|
| `summary` | | byte-identical to base | |
| `links.broken` (target) | 7 406 | 7 406 | **1 871** |
| of which markdown links | 6 082 | 6 082 | **547** |
| of which embeds | 1 324 | 1 324 | 1 324 |
| `links.broken_anchors` | 366 | 366 | 603 |
| orphans / dead ends | 843 / 369 | 843 / 369 | 699 / 513 |
| links resolved `via: "redirect"` | | | 5 535 (all markdown) |
| links `ambiguous` | | | 1 |

- **Per link** (17 047 non-external links, same row count and order): 5 536 rows differ, 5 535
  newly resolved via redirect and 1 newly ambiguous. **0 previously resolved links changed**
  and 0 rows changed in any other way.
- **237 of the 5 535** newly resolved links carry a `#fragment` that matches no heading of the
  declaring note, which is where `broken_anchors` 366 to 603 comes from: those links were
  broken before, and now point at a page that exists but moved its headings. Orphans fall by
  144 and dead ends rise by the same 144: notes that were linked from nowhere are now linked
  from somewhere, and have no outbound link of their own.
- **Ambiguity: 1 link, not the 30 the first read predicted.** That read stripped the
  `#fragment` from the declared values: 23 values (`/github/site-policy/github-community-guidelines#bullying-and-harassment`
  and nine siblings, `/admin/.../configuring-tls#configuring-cipher-suites-...`) then claim the page
  they point into, and 8 URL paths have two or more claimants, 30 links in all. A value with a
  fragment names a section of another page, so DEC-415 ignores it; 29 of those 30 links then
  resolve uniquely to the one note that declares the plain URL, and the single ambiguous link
  (`/billing/managing-the-plan-for-your-github-account/discounted-plans-for-github-accounts`)
  is declared by both `billing/concepts/discounted-plans.md` and `nonprofit/index.md`.
  An independent script over the raw `redirect_from` values reproduces both counts (5 535 and
  1; 5 506 and 30 with the fragment stripped) and 546 links that match nothing.
- **Obsidian Hub (6 519 notes) and MDN `files/en-us` (14 375), setting unset, base against
  new:** `summary` byte-identical, and `find --fields links,backlinks --limit 0` byte-identical
  (23.1 MB and 28.6 MB of JSON).
- **`--index` against disk** on the GitHub Docs clone: `find --fields links,backlinks`
  byte-identical, `summary`, `--orphan`, `--dead-end` and `--broken-links` equal outside the
  provenance keys. A snapshot built with the setting and read without it is byte-identical to the
  disk scan without it. There is nothing in the file to refuse, see DEC-415 point 9.
- **Time:** `summary` on the clone 0.34 s (base), 0.29 s (new, unset), 0.33 s (new, set); the
  Obsidian Hub 0.38 s to 0.43 s and MDN 1.41 s to 1.16 s, all inside the noise of a shared
  machine.

**Tests.** `tests/e2e/iteration340_redirect_property.rs` (9): a declared value resolves as
markdown and wikilink, with a fragment, with another case and as a directory index; the site
prefix on the link and on the value; a real file and a directory index win over a claim;
two claimants are ambiguous on disk and under `--index` and still broken in `find`, `summary`,
HYALO006 and `links fix`, and neither claimant gets the backlink; one note listing a path twice is
not ambiguous; unset changes nothing and `config` reports it; graph edges (backlinks, orphans,
`summary.links.broken`, HYALO006) on disk and indexed; `links fix --apply --apply-fuzzy` leaves
the file byte-identical; a snapshot built and read under every combination of set/unset answers
like disk with no warning; `mv` rewrites the real wikilink and neither the redirect link nor the
declaration, and the link keeps resolving at the new path. Unit tests in `case_index.rs` (value
reading, site prefix, case policy, claimants, projected moves) and `catalog.rs` (the last-resort
order); a same-binary fixture in `npm/hyalo/test/api.test.ts`.

**Mutation checks** (each applied alone, the tests rerun, then restored): moving the redirect
step before the literal path candidates is caught by the unit test and by
`a_real_file_wins_over_a_claim_on_its_path`; resolving two claimants to the first by
`a_value_two_notes_claim_is_ambiguous_and_still_broken` and the catalog test; dropping the
de-duplication of claimants by `claimants_are_sorted_distinct_and_two_make_an_ambiguity` and the
e2e ambiguity test; turning the redirect step on for `mv`'s `LinkResolver` by two e2e `mv`
tests. **One survivor, and it is equivalent:** moving the step ahead of the stem and alias
step changes nothing, because that step runs for a bare target (`!absolute`) and the redirect
step for a site-absolute one (`absolute`); the two cannot both apply to one link. The precedence
that can differ is against the literal path and the directory index, which the first mutation
covers.

## Review of PR #422 (five findings, one commit)

- **`links fix --expand-short-form` offered to repoint a redirect link** (target.md declares
  `/guide/setup`, `guides/setup.md` exists, the fuzzy matcher proposed `/guides/setup` at 0.772).
  The flag turns the reader guard off, so the broken-link pass now asks the map on its own
  (`resolves_via_redirect`) whatever the flag says; the ambiguous classification is unchanged.
  e2e `expand_short_form_does_not_offer_to_repoint_a_redirect_link` (with a control vault);
  removing the new arm fails it.
- **Wording: "`links fix` proposes nothing"** was wrong. The anchor pass resolves with the step on,
  so a broken `#fragment` is repaired against the declaring note (`#success-metrics` to
  `#6-success-metrics`), the target text untouched. Reworded in DEC-415 point 7, the docs, both
  templates, `find --help`, the CHANGELOG and `.claude/CLAUDE.md`; pinned by
  `links_fix_repairs_the_fragment_of_a_redirect_link_and_keeps_its_target`.
- **The `site_prefix` "stripped 0 of N" warning ignored redirects**, so a vault whose site-absolute
  links all resolve through the map was told its prefix was wrong.
  `site_prefix_plausible_resolution_stats` takes the case index and counts a link the map resolves
  as plausible; a unit test and an e2e test with a control.
- **A file moved onto a claimed path takes over its links.** Decided, not changed: no new `mv`
  logic. "A real file wins" cuts both ways, and an alias under `[links] aliases = true` behaves
  the same. The docs no longer say a claim "never repoints an existing link"; DEC-415 point 4 states
  the rule and DEC-388's scope (a move never changes what a link it *rewrites* names).
- **A blank `redirect_property` is now a config error**, as DEC-415 said: it joins the diagnostics
  that make the config unloadable for the gates (`lint`, `find --strict`, `views run` exit 1) and
  `hyalo config` reports `malformed: true`, like an invalid `[lint.rules.X] severity` (DEC-399);
  a read that is not a gate keeps the warning, and `new`, `types` and a plain write are unaffected.
  e2e `a_blank_redirect_property_stops_the_gates_like_an_invalid_severity`; replacing the
  diagnostic with `None` fails it.
