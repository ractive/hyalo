---
type: backlog
title: "[links] redirect_property for GitHub-Docs-style redirect_from resolution"
date: 2026-09-05
status: planned
priority: low
origin: "iter-272 (PR #320) carry-over sweep, 2026-09-05, DEC-300"
---

## Problem

The post-batch dogfood report (feeding [[iterations/iteration-272-resolution-completeness]])
found that on a GitHub Docs copy, 1569 of 1624 "broken" links are actually links to a
`redirect_from:` value declared in the target file's own frontmatter — GitHub Docs pages
declare every historical URL they used to live at, and internal links elsewhere in the corpus
still use those old URLs.

iter-272 Part B built a name-keyed alias map (DEC-296) for exactly this shape of problem
(`aliases:` → note), and it was tempting to treat `redirect_from:` the same way. DEC-300
declined: *"It looked like a few lines on top of DEC-296's alias map, and it is not — aliases
are keyed by bare note name; `redirect_from:` values are site-absolute URL paths that resolve
through `strip_site_prefix`, so supporting them needs a second, path-keyed map with its own
precedence against the directory-index rule."*

## Proposal

Not yet designed — this is a placeholder for the follow-up DEC-300 deferred, not a committed
shape. Whoever picks this up should:

- Design the second map: keyed by the *resolved, site-prefix-stripped path* a `redirect_from:`
  entry names (not the bare name DEC-296's alias map uses), pointing at the file that declares
  it.
- Decide precedence explicitly and write it into the DEC before implementing, mirroring
  DEC-296's `ALIAS-1` decide step: does a real file at that path always win over a
  `redirect_from:` claim (as a filename always wins over an alias)? Is a path claimed by two
  files' `redirect_from:` lists ambiguous, or does the directory-index rule (`foo/index.md` for
  `foo/`) take precedence over a `redirect_from:` collision, or vice versa?
- Decide the config key name and default: the iteration's constraint against new CLI flags
  still applies, so this is `[links] redirect_property = "redirect_from"` (opt-in, since unlike
  `aliases` it is not a stable Obsidian-wide convention) or similar — confirm the property name
  GitHub Docs and other target corpora actually use before hardcoding it.
- Reuse iter-272 Part B's implementation shape where it transfers: a resolver consulted last
  (after path, stem, and the alias map), a `via` value on `LinkInfo` (e.g. `"redirect"`), and
  `links fix` never proposing a rewrite for a target that resolves via a redirect.
- Measure the target corpus's actual improvement (a GitHub Docs copy is the report's source)
  the way DEC-296 measured the Obsidian Hub, since this is opt-in and unmeasured benefit does
  not justify the second map's maintenance cost.

## Acceptance criteria

- [ ] A DEC records the path-keyed map's precedence rules against directory-index resolution
      and against a real file at the same path, before any code is written.
- [ ] `[links] redirect_property = "<name>"` (opt-in, off by default) resolves a link whose
      target matches a declared value of that property in some file's frontmatter, the same way
      an alias does; `via: "redirect"` (or the chosen name) on the resolved `LinkInfo`.
- [ ] `links fix` does not propose a rewrite for a target that resolves via redirect.
- [ ] Measured on the GitHub Docs corpus that motivated this: the 1569/1624 figure drops to
      (near) zero for links whose only defect was pointing at a declared `redirect_from:` value.
- [ ] If staying backlog (won't-do): a DEC amendment records why, with the property name
      confirmed or ruled out.
