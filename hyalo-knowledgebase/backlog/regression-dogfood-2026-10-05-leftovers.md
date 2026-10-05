---
title: Regression dogfood 2026-10-05 leftovers
type: backlog
date: 2026-10-05
status: planned
priority: low
origin: regression dogfood 2026-10-05 (three explorers), deferred from iteration 318
---
# Regression dogfood 2026-10-05 leftovers

Findings of the 2026-10-05 regression dogfood that
[[iterations/iteration-318-regression-dogfood-fixes]] did not fix. None blocks
0.25.0.

## Deferred from iteration 318's own task list

- `mv` rewrites a bare `[[Café]]` (precomposed) to the decomposed bytes of
  `notes/Café.md` after a directory move, although a bare link needs no
  change; the rewrite decision should compare NFC-normalized stems (DEC-354).
- `new --dry-run` emits no hints; it should hint the real
  `=> hyalo new … [writes]`.

## Other findings

- Backslash links: `backlinks` and `find` disagree on a `\` in a link
  target, and a `%5C` in a wikilink is not decoded the same way everywhere.
- YAML 1.2 core: signed hex/octal (`-0x1F`, `+0o17`) and the `.NaN`
  spelling are not resolved per the core schema.
- `types show` under an unloadable `[schema]` does not say the schema is
  unusable.
- `init`: the "updated" wording when nothing changed; a user's edits inside
  the managed block are overwritten without notice.
- `read --frontmatter` on a file without frontmatter returns an empty
  result without saying there is none.
- `lint-rules show SCHEMA` reports `autofixable: true` for a pass that has
  no autofix of its own.
- `find --facet` with `--limit 0`-style facets-only use has no compact
  text output.
- Hint objects serialize their keys in an unstable order relative to the
  documented `{description, cmd, writes}`.
- HYALO006 counts out-of-vault links that `find`/`summary` exclude
  (GitHub Docs 7409 vs 7406; `README.md:104` `../contributing/redirects.md`).
- `find --broken-links` suggests `#Intro-duction` where `links fix` plans
  `#1. Intro`: the two anchor choosers still disagree on a
  prefix-vs-numbered case.
- `links fix` JSON `fixable: 0` next to text `Fixable: 5`.
- `--index-file <missing outside vault>`: the hint omits
  `--allow-outside-vault`.
- The stale-index note says "changed" for added and deleted files.
- `lint --rule HYALO008 -q` still prints unknown-markdownlint-rule warnings
  for unrelated directives.
- A `--jq` output over the 10 MiB limit exits 2 with no way out named; it
  should exit 1 with "use --format json or a narrower filter".
- Disk `summary` has no `source` key (the indexed one does).
- A no-op `create-index` on MDN still takes 1.09 s (2.5× `summary --index`).
