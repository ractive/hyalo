---
type: backlog
title: The link inventory, the heading outline and the link writers ignore the protected-region model
date: 2026-11-06
status: planned
priority: low
origin: "iter-327 whole-change read (W3), 2026-11-06, DEC-381, DEC-400"
---

## Problem

The protected-region model of [[decision-log|DEC-381]]
(`hyalo_core::protected`) decides what is prose for the `lint` prose rules, `lint --fix` and
`links auto`. `links fix`, `mv`, the link inventory (`find --fields links`, `backlinks`, the
graph and everything built on it), the heading outline and task scanning do not use it. They read
a body through the older fence, code-span, HTML-comment and `%%` scanner.

Repro, 2026-11-06, `main` at 74cfa6de, with `sub/Target.md` and this `s.md`:

````markdown
<pre>
# not a heading
[[target]] in pre
</pre>

math $$ [[target]] $$ and <code>[[target]]</code> and `[[target]]` and real [[target]]
````

- `find --file s.md --fields title,links` gives `title: "not a heading"` (`title_source:
  h1`) and four links, on lines 3 and 6 (three on line 6: the `$$` span, the `<code>` span and
  the real one). Only the backtick code span is left out.
- `links fix --apply` rewrites all four to `[[Target]]`, including the one inside `<pre>`, the
  `$$` span and the `<code>` element.

Each is a link the reader reports, and the writer rewrites what the reader reports (DEC-384), so
the two agree. The question is whether the reader is right.

## Open question

What does Obsidian count as a link inside `<pre>`, `<code>`, `$$` math and a raw HTML block, and
does it treat `# x` inside `<pre>` as a heading? Nothing in the repository answers it. Without
that evidence, changing what counts as a link would move `backlinks`, `--orphan`, `--dead-end`,
`--broken-links` and `summary.links` on notes nobody has complained about, and a link `mv` stops
rewriting is a link it breaks on the next move.

## Plan

- [ ] Find out, in Obsidian, what is a link and what is a heading in each region
- [ ] If it differs from today, route the link and heading extractors through `BodySyntax::with_protected`
- [ ] Make `links fix` and `mv` skip what the extractors skip, in one change, so the reader and the writer keep agreeing
- [ ] Run MDN, GitHub Docs and the Obsidian Hub through a before/after differential of `find --fields links` and `summary.links`
