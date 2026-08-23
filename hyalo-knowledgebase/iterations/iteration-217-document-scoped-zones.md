---
title: "Iteration 217 — document-scoped link zones and alias emission for links auto"
type: iteration
date: 2026-08-23
status: in-progress
branch: iter-217/document-scoped-zones
tags: [iteration, links, auto-link, integrity]
related:
  - "[[dogfood-results/dogfood-v0210-pre3-fix-waves-207-214]]"
  - "[[iterations/iteration-207-inert-zone-completion]]"
---

# Iteration 217 — document-scoped link zones and alias emission

## Goal

Close the two remaining silent-corruption classes in `links auto --apply`
(NEW-1 reference links, NEW-2 line-wrapped links, NEW-10 multi-line HTML —
one root cause: the zone scan in `hyalo-core/src/auto_link.rs` is
line-scoped) and stop the auto-linker from rewriting rendered prose
(NEW-3) or emitting links its own resolver calls ambiguous (NEW-4).
This is the v0.21.0 release gate named in
[[dogfood-results/dogfood-v0210-pre3-fix-waves-207-214]].

## Context

iter-207 made code spans, Liquid, and single-line HTML inert with a
CommonMark-correct per-block scan, verified at 0 corruptions across 68,984
insertions. But the link/tag zone detection still operates per line:

- **NEW-1 (HIGH)**: CommonMark reference links are not inert. In a vault
  with `gamma.md` (`title: Gamma`), `links auto --apply` rewrites all of
  `[click here][Gamma]`, `[Gamma][]`, `[Gamma]`, `![Gamma][Gamma]` **and
  the `[Gamma]: url` definition line** to `[[gamma]]` forms, destroying the
  links. 54 corruptions / 35 files on vscode-docs, 8 / 5 on GH Docs.
  Compounding: `links fix --apply --apply-fuzzy` then offers the mangled
  `"[gamma"` target at `[fuzzy-match 1.0]` and produces `[[gamma]]]`.
- **NEW-2 (HIGH)**: a `[[…]]` or `[…](…)` wrapped across a line boundary is
  invisible; hyalo writes into the target, label, and destination
  (`(target.md)` alone on a continuation line was corrupted). Fired on a
  copy of this KB (`iteration-161…md:17`, wrapped wikilink at ~72 cols →
  `[[research/[[release-pipeline-unification]]|reusable`). The KB wraps at
  ~72 columns: systemic exposure via the `=> links auto --apply [writes]`
  hint.
- **NEW-10 (MEDIUM)**: multi-line HTML tags leak — a wrapped `<video …`
  tag's continuation-line attributes are treated as prose (1 corruption on
  vscode-docs). Same root cause.
- **NEW-3 (MEDIUM-HIGH)**: matched text is replaced by the target stem, so
  22.2% of GH Docs insertions (7,968 / 35,860) change what the page renders
  (`pull requests` → `[[pulls]]`, `revocation` → `[[revoke]]`). The
  dry-run JSON already carries `matched_text` and `link_target` separately.
- **NEW-4 (MEDIUM-HIGH)**: ambiguity is checked on titles but the link is
  emitted as a filename stem. `graphql/reference/pulls.md` ("Pull
  requests") and `rest/pulls/pulls.md` ("REST API endpoints for pull
  requests") have distinct titles, shared stem `pulls`: 1,492 links written
  on GH Docs that `hyalo links` then reports as ambiguous (0 → 1,492).

## Tasks

- [ ] Make the link/label/destination zone scan document-scoped: carry the
      open-bracket state of wikilinks, inline links, and raw HTML tags
      across line boundaries (per block, consistent with iter-207's
      block-scoped code-span state). A line inside any unterminated link or
      tag construct is inert.
- [ ] Add CommonMark reference-link zones: full `[label][ref]`, collapsed
      `[ref][]`, shortcut `[ref]`, image `![ref][ref]`, and link reference
      definition lines `[ref]: url "title"` are all inert (label, ref, and
      definition). Shortcut-form detection must not blanket-ban all
      bracketed text: only labels that match a definition in the same
      document are reference links.
- [ ] Emit `[[target|matched_text]]` whenever `matched_text` differs from
      the target stem (case difference counts). Plain `[[target]]` only
      when the rendered text is unchanged.
- [ ] Check ambiguity in the emitted namespace: skip a candidate when the
      stem (or whatever link text would be written) resolves to 2+ files,
      in addition to the existing title-ambiguity skip. AC: `hyalo links`
      reports the same `ambiguous` count before and after
      `links auto --apply` on any corpus.
- [ ] Regression corpus: fixture vault covering all five reference-link
      forms, wrapped wikilinks/markdown links (open bracket on line N,
      close on N+1, destination alone on a line), multi-line HTML tags,
      and the alias-emission cases. e2e tests assert byte-identical
      non-prose zones after `--apply`.
- [ ] Re-run the pre3 verification protocol on scratch copies of GH Docs,
      vscode-docs, and the own KB: assert 0 insertions inside code spans,
      Liquid, HTML (incl. multi-line), reference links, and wrapped links;
      own-KB copy must stay at 0 broken links after `--apply`.
- [ ] Docs sync in the same PR: `links auto --help` inert-zone contract
      (add reference links + cross-line rule + alias emission), skill/rule
      templates (`crates/hyalo-cli/templates/rule-knowledgebase.md`),
      CHANGELOG, decision-log entry for alias emission and
      emitted-namespace ambiguity.

## Acceptance criteria

- [ ] The NEW-1 repro vault: `links auto --apply` leaves all five
      reference forms and the definition line byte-identical
- [ ] The NEW-2 repro vault: wrapped wikilink and wrapped markdown link
      (including the lone-destination line) byte-identical; same-line
      baselines still linked
- [ ] vscode-docs scratch copy: 0 reference-link/multi-line-HTML
      corruptions (was 54 + 1); broken links do not increase after
      `links auto --apply`
- [ ] GH Docs scratch copy: `matched_text != stem` insertions are emitted
      as `[[target|matched_text]]`; rendered prose is unchanged for 100%
      of insertions
- [ ] GH Docs scratch copy: `ambiguous` count unchanged by
      `links auto --apply` (was 0 → 1,492)
- [ ] Own-KB scratch copy: `links auto --apply` then `hyalo links` reports
      0 broken (was 0 → 1)

## Non-goals

- Fuzzy-matching mangled reference targets in `links fix` (the compounding
  half of NEW-1 disappears once nothing gets mangled)
- Liquid-heading anchor slugs ([[iterations/iteration-215-anchor-and-broken-links-followups]])
- `links` perf ([[iterations/iteration-206-links-perf-profiling]])
