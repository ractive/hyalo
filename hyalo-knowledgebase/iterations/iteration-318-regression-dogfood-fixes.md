---
title: "Iteration 318: regression dogfood fixes before 0.25.0"
type: iteration
date: 2026-10-05
tags: [iteration, dogfooding, search, jq, utf8, hints, help]
status: planned
branch: iter-318/regression-dogfood-fixes
priority: 1
---

# Regression dogfood fixes before 0.25.0

## Problem

The regression dogfood of 2026-10-05 on main at 61600876 (three explorers; report to
follow under `dogfood-results/`) found no regression of the 2026-10-04 fixes, but one
HIGH and several MEDIUM/LOW defects that should not ship in a breaking release:

- **HIGH** every `--jq` invocation fails with "cannot install jq cancellation handler:
  Ctrl-C signal handler already registered" (exit 1) when SIGINT is ignored — the state
  of a shell background job (`cmd &` in a non-interactive shell), `nohup`, and any runner
  that ignores SIGINT. Introduced by iteration 295 (2eebf90a). Plain JSON and `--count`
  are unaffected.
- **MEDIUM** one note whose FRONTMATTER holds invalid UTF-8 makes `find --fields file`,
  `properties` and `tags` exit 2 on disk with no file named, while `find`, `summary`,
  `lint` and every `--index` read skip it; `read`/`set` on that named file exit 2.
- **MEDIUM** `prefix*` and `terms PREFIX` are not accent-folded although bare words are
  (`résumé` 12, `resume*` 13, `résumé*` 0; the hinted `terms 'rés'` is a dead end).
- **MEDIUM** `find -snapshot index` (a `-s…` cluster followed by a PATTERN) still runs
  `--section napshot` silently; the DEC-356 tip only appears when no PATTERN follows,
  and `-q` silences it.
- **MEDIUM (UX)** the DEC-366 migration notice appears only on zero results:
  `title:dogfood` silently returns 134 files (91 under field semantics).

DEC numbers reserved: **DEC-369 to DEC-371**.

## Tasks

- [ ] `--jq` runs when SIGINT is ignored or a handler is already registered: skip installing the cancellation handler in that case (never an error), keep Ctrl-C cancellation when it can be installed; e2e tests on Unix for a background job (`sh -c 'hyalo … --jq … & wait $!'`), `nohup`, and `trap '' INT`, for a read (`find`), `summary`, and a `set --dry-run --jq`; DEC
- [ ] Invalid UTF-8 in frontmatter: the frontmatter-only scan shapes (`find --fields file`, `--count`, `properties`, `tags`) skip and count the file exactly like `find`/`summary`/`lint` do, disk and `--index` identical; a NAMED unreadable file (`read fm.md`, `set fm.md`, `find --file fm.md`) exits 1 with the file named in the envelope (DEC-307, DEC-301), never 2; the skip warning's wording matches what happens (a lossily-read file is not "skipped"); DEC-365 amended
- [ ] `prefix*` and `terms PREFIX` fold accents and case exactly like a bare word before matching stems (`résumé*`, `Résum*`, `terms rés`), on disk and `--index`; `terms 'conf*'` either strips the trailing `*` or says PREFIX takes none; `terms get_user` behaves like the query tokenizer or says why not
- [ ] DEC-356 completed: a `-s<letters>` / `-t<letters>` cluster in argv gets the `hyalo find -- '-term'` tip whether or not a PATTERN follows, and the tip is `-q`-proof like the other query warnings
- [ ] Field-term migration notice whenever a positive query word has the shape `title:x` / `heading:x` / `tag:x` / `path:x`, results or not: one stderr line naming the flag (not repeated per word; a normal warning), and `title:(a OR b)`'s unbalanced-parenthesis error mentions that field terms were removed; the zero-result hint stays; the migration hint strips a trailing `*` for `--title` (substring flag)
- [ ] Teaching hints trimmed to what helps: remove the PATTERN-less `--section X` → section-mode hint (it widens 299 files to 1 007 sections); when a `"a b"~N` query returns zero and the same words AND-match, hint the AND form and the reversed order instead of `terms`; FIND 101 gains nothing (it must stay ten lines) but `find --help` QUERY SYNTAX says slop is ordered and shows `'"a b"~5 OR "b a"~5'`; DEC-367 amended
- [ ] Small correctness: `mv` does not rewrite a bare link that differs from the target only in Unicode composition (DEC-354); `set`/`remove --dry-run` under an unusable `.hyalo.toml` refuse exactly like the real run (a dry run predicts); the false "no files matched --property map.b; did you mean: map?" warning is gone when the dot-path exists; `set --property p=9223372036854775808` writes the exact integer or a quoted string, never a lossy float; an operator-only or dash-only query says the query is empty, not "negating every word"; the duplicate hint under `--tag X --facet tags` is deduplicated; `new --dry-run` hints `=> hyalo new … [writes]`
- [ ] Help drift: `read --help` says `size`/`lines` are whole-file numbers; `find --help` TOKENIZATION no longer hard-codes "snapshot format 4"; `terms --help` says a tag-only term counts; `lint-rules -h` / `types -h` do not advertise `--count` for subcommands that refuse it (or say "list only"); `hyalo --help` says an ERROR envelope omits `hints` under `--no-hints`/`--jq`
- [ ] Leftovers that are NOT fixed here are filed as one backlog note `backlog/regression-dogfood-2026-10-05-leftovers.md` (backslash links: `backlinks` vs `find` disagreement and `%5C` in wikilinks; signed hex/octal per YAML 1.2; `.NaN` spelling; `types show` under an unloadable schema; `init` "updated" wording and overwritten managed-block edits; `read --frontmatter` on a file without frontmatter; SCHEMA `autofixable: true`; facets-only text output; hint object key order)
- [ ] Docs in sync: help, claims paragraph, templates and bundled copies, CHANGELOG `[Unreleased]`, DEC-369..371 and the amended DECs

## Acceptance criteria

- [ ] `sh -c 'hyalo find snapshot --jq .total & wait $!'`, `nohup hyalo find snapshot --jq .total` and `(trap "" INT; hyalo summary --jq .results.files.total)` print the number and exit 0
- [ ] A vault with one invalid-UTF-8-frontmatter note: `properties`, `tags`, `find --fields file`, `find --count` exit 0 with the skip counted, identical under `--index`; `read`/`set` on that file exit 1 naming it
- [ ] `find 'résumé*' --count` equals `find 'resume*' --count`; `terms rés` lists what `terms res` lists
- [ ] `find -snapshot index` prints the `--` tip, also under `-q`
- [ ] `find 'title:dogfood'` prints the migration notice on stderr and still returns its plain-word results; `-q` silences it; `--jq` output is unchanged
- [ ] The five grammar counts are unchanged from main and identical on disk and `--index`
- [ ] No new CLI flag or subcommand; fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check`, all xtask gates and `hyalo lint --strict` green; CI green on three platforms
