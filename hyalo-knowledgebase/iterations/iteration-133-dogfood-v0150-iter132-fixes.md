---
title: >-
  Iteration 133 — Dogfood v0.15.0/iter-132 follow-up (lint exit-code, mv relative
  wikilinks, date calendar validity, HYALO001 dash form, views/find fuzzy hints)
type: iteration
date: 2026-05-11
status: in-progress
branch: iter-133/dogfood-v0150-iter132-fixes
tags:
  - iteration
  - bug-fix
  - ux
  - lint
  - mv
  - ci
related:
  - "[[dogfood-results/dogfood-v0150-iter132-followup]]"
  - "[[iterations/iteration-132-dogfood-v0150-iter131-fixes]]"
  - "[[iterations/iteration-129-tidy-report-followup]]"
  - "[[iterations/iteration-126-markdown-linter]]"
---

## Goal

Address findings from [[dogfood-results/dogfood-v0150-iter132-followup]]
against v0.15.0 / merged iter-132: one HIGH CI-breaking regression in
`hyalo lint` exit codes, three MEDIUM correctness gaps (mv relative
wikilinks, HYALO003 shape-only date validation, HYALO001 missing the `- []`
form), one LOW config-tidy bug, and six UX polish items around `views`,
`find --view`, mixed-type properties, and `lint-rules`.

BUG-3 is the headline: `hyalo lint` and `hyalo lint --strict` both exit 0
even when errors are reported. That breaks every CI pipeline that gates on
hyalo lint — the exact use-case `--strict` was introduced for in iter-129.
The other items polish the iter-132 deliverable by closing the small ACs
that were left at the edges (relative wikilinks, true date parsing, `- []`
checklist form, bare `views <name>`).

Numbering mirrors the dogfood report for traceability.

## Context

- iter-126 introduced the markdown linter; iter-129 added `--strict` and
  explicitly promised non-zero exit when promoted findings fire.
- iter-132 fixed `mv` for the canonical wikilink shapes but missed the
  relative-path form `[[./b]]`, and the date-validator was implemented as a
  regex check (shape-only) rather than a real `chrono::NaiveDate::parse`.
- iter-132 implemented `views run <name>` instead of bare `views <name>`,
  but the bare-name path still gives the bare clap "unrecognized
  subcommand" error with no hint pointing at `views run`.

## High

### BUG-3: `hyalo lint` always exits 0 even with errors / under `--strict`

**Bug:** `hyalo lint` returns exit code 0 in every observed configuration:
plain `lint` with 134 findings on own KB, `lint --rule HYALO001` with
unambiguous errors on the synthetic scratch KB, and `lint --strict` with
schema-promoted errors. This breaks the iter-129 contract and makes
`hyalo lint` unusable as a CI gate — the canonical reason `--strict`
exists.

Repros (all from dogfood-v0150-iter132 BUG-3):

```text
$ hyalo lint ; echo "EXIT: $?"            # 134 findings on own KB
EXIT: 0
$ hyalo lint --strict ; echo "EXIT: $?"
EXIT: 0
$ hyalo lint --rule HYALO001 file.md ; echo "EXIT: $?"
EXIT: 0
```

**Fix:** Exit 1 from the `lint` command when:
- Any finding has severity `error` (after `--strict` promotion), OR
- `--strict` is supplied and at least one warning was promoted to error.

Keep exit 0 when only warnings/info are reported without `--strict` (so
the default mode stays advisory). `--fix` semantics: exit 1 only on
remaining findings after the fix pass, not on findings that were fixed.

- [ ] Locate the `lint` command's return path; identify why it currently
      ignores finding severities
- [ ] Wire a "any error-severity finding?" decision into the exit code
- [ ] Make sure `--strict` promotion happens before the exit-code check
- [ ] Make sure `--fix` only counts post-fix `remain` findings toward the
      exit code, not `fixed` / `would fix`
- [ ] E2E test: `lint` on a clean file → exit 0
- [ ] E2E test: `lint` on a file with HYALO001 errors → exit 1
- [ ] E2E test: `lint --strict` on a file with promoted warnings → exit 1
- [ ] E2E test: `lint --fix` that fixes everything → exit 0
- [ ] E2E test: `lint --fix` that leaves remainders → exit 1
- [ ] E2E test: warnings-only without `--strict` → exit 0 (unchanged)
- [ ] Add a CI-style assertion in the test suite so this can't regress
      silently again
- [ ] Update README / help text if the prior behavior was documented
      anywhere

## Medium

### BUG-1: `hyalo mv` does not rewrite `[[./relative]]` wikilinks

**Bug:** iter-132's AC explicitly listed `[[./b]]` as a case to cover.
After `mv b.md --to sub/b.md`, `[[./b]]` survives untouched and
`find --broken-links` reports it as `"./b" (unresolved)`. Every other
shape (plain, alias, case-mismatch, section, alias+section) rewrites
correctly.

**Fix:** Extend the wikilink rewrite walker to also match the
`./<basename>` and any `./<path/segments>` forms, normalising via the
same resolver used for `[[basename]]` and `[[path/segments]]`.

- [ ] Add `./` (current-dir) handling to the wikilink-target matcher
      in `mv`'s rewrite walker
- [ ] Decide on the rewritten form: `[[./sub/b]]` (preserve `./`) vs
      `[[sub/b]]` (collapse to path form). Prefer the latter unless
      there's a strong precedent — `./` is rare in vault wikilinks
- [ ] E2E test: `mv b.md --to sub/b.md` rewrites `[[./b]]` →
      `[[sub/b]]` (or `[[./sub/b]]` per decision above)
- [ ] E2E test: `[[./b|alias]]`, `[[./b#sec]]`, `[[./b#sec|alias]]`
      all rewrite
- [ ] E2E test: `find --broken-links` on the moved KB reports 0

### BUG-2: HYALO003 / `set` date-note is shape-only, accepts invalid calendar dates

**Bug:** `2026-13-50`, `2026-02-30`, `0000-00-00` all pass the
shape-regex check and write silently. `hyalo find --sort modified` then
puts `2026-13-50` after a real `2026-12-31`.

**Fix:** Replace the regex check with `chrono::NaiveDate::parse_from_str("%Y-%m-%d")`
so calendar validity is enforced. Same parser for the `set` note and the
HYALO003 lint rule (extract a shared helper).

- [ ] Extract a `is_valid_iso_date(&str) -> bool` helper using
      `chrono::NaiveDate::parse_from_str`
- [ ] Wire into the `set` write-time note path
- [ ] Wire into the HYALO003 lint check
- [ ] E2E test: `set modified=2026-13-50` emits the note
- [ ] E2E test: `set modified=2026-02-30` emits the note
- [ ] E2E test: `set modified=2026-12-31` is silent
- [ ] E2E test: `lint --rule HYALO003` on a file with a bad calendar
      date now flags it

### BUG-5: HYALO001 misses the `- []` and `* []` checklist forms

**Bug:** `- [] task` is the most common Obsidian / GitHub-checklist
typo. Today HYALO001 only fires on the bare `[] task` form (no leading
bullet), missing the much more frequent variants. Fixability also
applies: `- []` → `- [ ]`, `* []` → `* [ ]`.

**Fix:** Extend the HYALO001 pattern to also match `^\s*[-*]\s+\[\]`,
and extend the `--fix` rewrite to insert the space between brackets.

- [ ] Extend the HYALO001 match regex
- [ ] Extend the HYALO001 fixer
- [ ] E2E test: `lint --rule HYALO001` on a file with `- []` flags it
- [ ] E2E test: `lint --rule HYALO001` on a file with `* []` flags it
- [ ] E2E test: `lint --fix --rule HYALO001` rewrites both forms
- [ ] E2E test: `- [x]`, `- [ ]`, `- [X]` continue to pass clean
- [ ] Cross-check `hyalo task toggle` still operates on the rewritten
      forms

## Low

### BUG-4: `lint-rules remove <ID>` leaves empty `[lint]` / `[lint.rules]` tables

**Bug:** After `lint-rules set <ID> ...` then `lint-rules remove <ID>`,
`.hyalo.toml` still contains empty `[lint]` and `[lint.rules]` headers
(no body). Cosmetic but messy on a previously clean file.

**Fix:** Drop empty parent tables when serialising. Symmetric to the
iter-131 BUG-2 fix but one level up.

- [ ] When removing the last rule in `[lint.rules]`, drop the table
- [ ] When `[lint]` has no remaining children, drop it too
- [ ] E2E test: clean `.hyalo.toml` → `set` → `remove` round-trips to
      clean

## UX

### UX-1: `views <name>` (bare) should hint at `views run <name>`

The natural follow-up after `views list` is `hyalo views open-tasks`.
Today this errors with `unrecognized subcommand 'open-tasks'` and no
hint. iter-132 introduced `views run <name>` but did not wire the bare
form to suggest it.

- [ ] Intercept the unrecognised-subcommand path for `views` and emit a
      `did you mean 'views run <name>'?` hint when the arg matches a
      known view name
- [ ] (Optional) Mention `views run <name>` in `views list` output
- [ ] E2E test: `views open-tasks` exits non-zero with the hint
- [ ] E2E test: `views run open-tasks` (positive control) still works

### UX-2: `find --view <typo>` should suggest the closest view name

Today exact-match only. The BUG-C fuzzy infrastructure (iter-132) is
already in the codebase for `--tag` / `--property` — extend it to
`--view`.

- [ ] Pull the known-views set from the loaded config
- [ ] Wire fuzzy suggestion into the unknown-view error path
- [ ] E2E test: `find --view plannned` suggests `planned`
- [ ] E2E test: `find --view xyzzy` emits no hint (no false positive)

### UX-3: `properties` reports type-inconsistent values without flagging

When a property has mixed types across files (e.g. `priority` as
`number` in 6 files and `text` in 84), `hyalo properties` lists two
rows that look like two distinct properties. Real signal but hidden.

**Fix options (pick one):**
A. Collapse to one row: `priority (mixed: 6 number, 84 text)` in text
output; `types` array on JSON.
B. Add a new lint rule `HYALO0NN mixed-property-types` that flags the
inconsistency. Properties listing stays as-is.

Recommend A (cheaper, fixes the immediate confusion). B can come later
if the lint signal proves valuable.

- [ ] Decide A vs B (or both)
- [ ] Implement the chosen option
- [ ] E2E test: a synthetic KB with a mixed-type property shows the
      collapsed/flagged output

### UX-4: `create-index -o <outside-vault>` hint duplication between text and JSON

The text output already includes `hint: use --allow-outside-vault to
override`. The JSON envelope also has a `hint` field with the same
string. Minor — but readers comparing both will wonder which is
canonical.

- [ ] Decide: text + JSON both carry the hint (status quo, document the
      pattern), or only one carries it (collapse). Status quo is fine if
      the pattern is documented; if not, prefer carrying the hint in
      JSON only and let the formatter render it in text
- [ ] E2E test: hint surfaces in the chosen channel(s) consistently
      across error paths

### UX-5: `lint-rules` `--dir is redundant` note fires inconsistently

`lint-rules show <ID> --dir .` from inside the project root prints the
redundancy note; `lint-rules set <ID> --enabled true --dir .` from the
same directory does not. The note should fire uniformly across all
`lint-rules` subcommands.

- [ ] Move the redundancy check into a shared spot so all `lint-rules`
      subcommands hit it
- [ ] E2E test: `lint-rules show --dir .` from the vault prints the
      note
- [ ] E2E test: `lint-rules set --dir .` from the vault prints the
      note

### UX-6: `find ""` (empty body pattern) — accept as "no pattern"

The current behaviour errors with `body pattern must not be empty;
omit the pattern to match all files`. Friendly but unhelpful for
scripted callers that build query strings from variables. Accept empty
as "no pattern" (with a one-shot `note:` for interactive callers).

- [ ] Allow empty string as a synonym for "no positional pattern"
- [ ] On interactive (TTY) stderr, emit a one-line `note:` so users
      don't accidentally rely on this and forget to filter
- [ ] E2E test: `find ""` matches the same files as `find` (no
      positional)
- [ ] E2E test: `find "" --tag iteration` continues to filter

## Non-Goals

- No further `mv` work beyond closing the `[[./relative]]` AC.
- No new lint rules beyond the `HYALO001` recall extension and the
  optional `HYALO0NN mixed-property-types` if UX-3 picks option B.
- No `views run` redesign — only the bare-name hint path.
- Date-type detection stays scoped to known date-typed keys (BUG-2);
  no full property-type system.
- No changes to `--strict` semantics beyond making the exit code
  actually follow the promise.

## Quality Gates

- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace -q`
- [ ] Help texts, README, and `crates/hyalo-cli/templates/rule-knowledgebase.md`
      updated for any changed behavior (especially the exit-code contract
      and `views run`)
- [ ] Dogfood the merged branch on own KB + MDN + docs before closing

## References

- [[dogfood-results/dogfood-v0150-iter132-followup]] — source report
- [[iterations/iteration-132-dogfood-v0150-iter131-fixes]] — previous
  follow-up; BUG-1, BUG-2, UX-1 here close gaps it left open
- [[iterations/iteration-129-tidy-report-followup]] — introduced
  `lint --strict`; BUG-3 restores its exit-code promise
- [[iterations/iteration-126-markdown-linter]] — base linter
  implementation; HYALO001 lives here
