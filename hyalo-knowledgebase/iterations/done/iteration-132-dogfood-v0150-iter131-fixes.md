---
title: >-
  Iteration 132 — Dogfood v0.15.0/iter-131 follow-up (mv wikilinks, set date
  validation, fuzzy tag hints, links/views/index-file UX)
type: iteration
date: 2026-05-10
status: completed
branch: iter-132/dogfood-v0150-iter131-fixes
tags:
  - iteration
  - bug-fix
  - ux
  - mv
  - lint
  - find
related:
  - "[[dogfood-results/dogfood-v0150-iter131-followup]]"
  - "[[iterations/done/iteration-131-dogfood-v0150-fixes]]"
  - "[[iterations/done/iteration-128-llm-misuse-warning]]"
  - "[[iterations/done/iteration-130-cwd-aware-help-and-config]]"
---

## Goal

Address findings from [[dogfood-results/dogfood-v0150-iter131-followup]] against
v0.15.0 / merged iter-131: one HIGH `mv` link-rewrite correctness bug, two LOW
input-validation / hinting gaps, and a cluster of UX polish items around
`links`, `views`, `--index-file`, `lint --strict`, and `find --sort`.

BUG-A is the headline: `hyalo mv` silently breaks every inbound `[[wikilink]]`
to the moved file even though `--help` claims both link styles are rewritten.
On a wiki-heavy KB (own KB, Obsidian vaults) every rename today leaves behind
broken backlinks.

Numbering mirrors the dogfood report for traceability.

## Context

- iter-117 implemented case-insensitive wikilink resolution and the broken-link
  detector. Wikilinks are first-class on read, but the `mv` rewrite path was
  built against the markdown-link form only and never extended.
- iter-128 added LLM-misuse warnings and abs-path canonicalisation for `--file`
  consumers. The "did-you-mean" infrastructure exists but is wired only into
  subcommand dispatch, not `--tag` / `--property` filter values.
- iter-130 added a CWD-aware help banner, `hyalo config`, and made `--dir`
  global. `--index-file` was deliberately left per-subcommand at the time;
  dogfooding now shows the inconsistency is the largest remaining friction
  point for read-only commands against external KBs.

## High

### BUG-A: `hyalo mv` does not rewrite `[[wikilinks]]` to the moved file

**Bug:** `mv` rewrites `[..](old.md)` markdown links but leaves `[[old]]` and
`[[old|alias]]` wikilinks untouched. After a move the wikilinks become broken
(`find --broken-links` reports them as unresolved) and `backlinks` on the new
path returns "No backlinks found". `--help` and `--dry-run` both still claim
wikilinks are rewritten.

Repro from dogfood report:

```text
$ printf -- '---\ntitle: A\n---\nSee [B](b.md) and [[b]] and [[b|alias]].\n' > a.md
$ printf -- '---\ntitle: B\n---\nhi\n' > b.md
$ hyalo --dir . mv b.md --to sub/b.md --format text
Moved b.md → sub/b.md
  a.md: [B](b.md) → [B](sub/b.md)
$ cat a.md
See [B](sub/b.md) and [[b]] and [[b|alias]].
```

**Fix:** Extend the rewrite walker in `mv` to also match wikilink targets using
the same resolution semantics as `find --broken-links` (case-insensitive,
basename or path, alias-preserving). Cover plain `[[target]]`,
`[[target|alias]]`, `[[target#section]]`, and `[[target#section|alias]]`.

- [x] Locate the `mv` link-rewrite implementation and identify where markdown
      links are matched
- [x] Reuse the wikilink resolver from iter-117 / `find --broken-links` to
      decide whether a given `[[..]]` points at the moved file
- [x] Rewrite each matching wikilink, preserving alias and `#section` suffix
- [x] Report wikilink rewrites in the same per-file list shown for markdown
      links (text + JSON envelopes)
- [x] Make sure `--dry-run` previews wikilink rewrites too
- [x] E2E test: mv with `[[b]]`, `[[b|alias]]`, `[[b#sec]]`, `[[b#sec|a]]`,
      `[[B]]` (case mismatch), `[[./b]]` (relative), `[[sub/b]]` (path form)
- [x] E2E test: mv leaves unrelated wikilinks alone (`[[c]]`, `[[bb]]`)
- [x] E2E test after fix: `find --broken-links` on the moved KB reports 0
- [x] E2E test after fix: `backlinks <new-path>` lists the rewritten files
- [x] Cross-check `links auto` / `links fix` don't double-rewrite

## Low

### BUG-B: `set --property date=<garbage>` accepts any string with no warning

**Bug:** On a schema-less KB, `hyalo set x.md --property "date=not-a-date"`
writes the literal string and reports success. Later `find --sort date` will
silently misorder. The own KB is protected by `lint --strict` + schema, but
external KBs (MDN, docs) and ad-hoc vaults are not.

**Fix:** When the property name is one of the known date-typed keys (`date`,
`created`, `modified`, `updated`, …), heuristically check the value parses as
ISO-8601 / `chrono::NaiveDate` and emit a `note:` (not an error) suggesting
the value will sort lexicographically. Add a lint rule (or extend an existing
one) flagging non-date frontmatter under a `date` key so `lint` surfaces
this on schema-less KBs.

- [x] Decide on the canonical list of date-typed property names (cross-check
      with the templates and prior reports)
- [x] Add the parse-on-write `note:` to `hyalo set` (text + JSON)
- [x] Add the lint rule (HYALO0NN) or extend the nearest existing one;
      schema-less default = `warn`, escalated by `--strict`
- [x] E2E test: `set date=not-a-date` emits the note but still writes
- [x] E2E test: `set date=2026-05-10` does not emit the note
- [x] E2E test: `lint` on a KB with bogus `date:` shows the new rule

### BUG-C: `find --tag <typo>` and `--property <typo>` return empty with no fuzzy hint

**Bug:** `hyalo find --tag iteraton` returns empty silently. Subcommand typos
(`hyalo finnd` → "did you mean `find`") get a hint. Tag/property typos are
the more common LLM mistake.

**Fix:** When `find --tag X` (or `--property name=…`, or `--property
'name~=…'`) yields zero results, run a Levenshtein/Jaro-Winkler pass against
the known tags/properties in the index and append a `hint:` line on the empty
text output and a `hint` field on the JSON envelope. Only suggest when the
distance is below a small threshold to avoid noisy false positives.

- [x] Pull the known-tag set from the index / discovered frontmatter
- [x] Pull the known-property-name set the same way
- [x] Wire the suggestion into the empty-result path for `--tag`, `--tags`,
      `--property name=…`, `--property 'name~=…'`
- [x] Re-use the existing fuzzy-match helper from the subcommand dispatcher
      if one exists; otherwise extract a shared util
- [x] E2E test: `find --tag iteraton` suggests `iteration`
- [x] E2E test: `find --property stauts=completed` suggests `status`
- [x] E2E test: zero-match with no close tag (e.g. `--tag xyzzy`) emits no
      hint (no false positive)

## UX

### UX-A: `create-index -o <outside-vault>` text output omits the `--allow-outside-vault` hint

The JSON envelope includes `"hint": "use --allow-outside-vault to override"`
but the text formatter prints only the error and path. The natural workflow
("index MDN into /tmp/mdn.idx") fails twice — once on `create-index`, then
again on `find --index-file` (silent fallback with a separate warning).

- [x] Surface the JSON `hint` on text output as a `hint:` trailing line
- [x] (Optional) When `--index-file` falls back because the file is missing,
      mention the likely cause ("did you skip --allow-outside-vault on
      create-index?")
- [x] E2E test: `create-index -o /tmp/x.idx` text output contains the hint

### UX-B: `hyalo links` requires a subcommand though `--help` reads as if the default is dry-run

`hyalo links` errors with "requires a subcommand". `hyalo links --help` reads:
*"Default behaviour is a dry run — no files are modified."* `hyalo links
--dry-run` also fails (`unexpected argument`).

**Fix:** Match `hyalo views` behaviour: when no subcommand is supplied, run
`links fix --dry-run`. Update help text to make the default explicit.

- [x] Make the default subcommand of `hyalo links` equivalent to `fix
      --dry-run`
- [x] Update `--help` to reflect the default
- [x] E2E test: `hyalo links` and `hyalo links fix --dry-run` produce the
      same output

### UX-C: Promote `--index-file` to a global flag (alongside `--dir`)

Every read-only subcommand accepts `--index-file` and the per-subcommand
placement after `--dir` (which is global since iter-130) is jarring. Clap
already emits a "tip" pointing at the subcommand form, so users are clearly
hitting this.

**Fix:** Add `--index-file` as a global flag with the existing
per-subcommand variant kept for back-compat. Resolution order: subcommand
value wins over global if both are supplied.

- [x] Add the global flag to the top-level `Cli` definition
- [x] Plumb the value through to each consumer; subcommand value takes
      precedence when present
- [x] Decide on a deprecation note (silent, or `note:` when only the
      subcommand form is used) — probably silent for now
- [x] E2E test: `hyalo --index-file /tmp/x.idx find …` works
- [x] E2E test: `hyalo --index-file A find --index-file B …` uses B

### UX-D: `hyalo views <name>` should map to `find --view <name>`

After `hyalo views list` the natural next command is `hyalo views
open-tasks`. Today it errors with `unrecognized subcommand`. Either map bare
name to `find --view <name>`, or have `views list` output explicitly tell
the user to use `find --view`.

- [x] Implement `views <name>` → `find --view <name>` (forwarding any extra
      flags)
- [x] If forwarding is too invasive, emit a `hint:` on the unrecognized
      subcommand path pointing at `find --view`
- [x] Update `views list` text output to reference the invocation form
- [x] E2E test: `views open-tasks` matches `find --view open-tasks`
- [x] E2E test: extra flags forward (`views open-tasks --tag iteration`)

### UX-E: `lint --strict` help should mention schema dependency

On a schema-less KB, `lint --strict` does not surface any missing-type or
undeclared-property promotions because there is nothing declared. Document
this in `--help` and on the empty-strict path.

- [x] Update `lint --strict` help text to clarify that missing-type /
      undeclared-property promotions require a `types` block in
      `.hyalo.toml`
- [x] (Optional) Emit a one-line `note:` when `--strict` is supplied but no
      schema is found
- [x] E2E test: help text contains the schema reference
- [x] E2E test: schema-less `lint --strict` either silent (status quo) or
      emits the note

### UX-F: Accept `--sort path` and `--desc` as aliases

`hyalo find --sort path` errors (`valid values are 'file', 'modified', …`).
`path` is the more common term. Likewise `--desc` is more familiar than
`--reverse`. The current error message is friendly, but accepting the
aliases is a one-line change with no downside.

- [x] Add `path` as a value alias for `--sort file`
- [x] Add `--desc` as a long-flag alias for `--reverse`
- [x] E2E test: `find --sort path` matches `find --sort file`
- [x] E2E test: `find --desc` matches `find --reverse`
- [x] Update `--help` to list both spellings

## Non-Goals

- No changes to `links auto` heuristics — out of scope here.
- No new index format work; this iteration is pure UX + correctness on top
  of the existing index.
- No schema-on-write enforcement (handled by `lint --strict`).
- Date-type detection in BUG-B is heuristic only — not introducing a full
  property-type system in this iteration.

## Quality Gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace -q`
- [x] Help texts, README, and `crates/hyalo-cli/templates/rule-knowledgebase.md`
      updated for any changed flags
- [x] Dogfood the merged branch on own KB + MDN + docs before closing

## References

- [[dogfood-results/dogfood-v0150-iter131-followup]] — source report (all
  bugs and UX items above)
- [[iterations/done/iteration-131-dogfood-v0150-fixes]] — previous follow-up
- [[iterations/done/iteration-128-llm-misuse-warning]] — fuzzy-suggestion
  infrastructure to reuse for BUG-C
- [[iterations/done/iteration-117-case-insensitive-link-resolution]] — wikilink
  resolver to reuse for BUG-A
- [[iterations/done/iteration-130-cwd-aware-help-and-config]] — `--dir` global
  precedent for UX-C
