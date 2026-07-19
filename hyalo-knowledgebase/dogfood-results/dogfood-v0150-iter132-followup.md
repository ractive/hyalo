---
title: "Dogfood v0.15.0 — iter-132 follow-up (intensive linter pass)"
type: research
date: 2026-05-10
status: active
tags: [dogfooding, lint, ux, mv, links, performance, exit-code]
related:
  - "[[dogfood-results/dogfood-v0150-iter131-followup]]"
  - "[[dogfood-results/dogfood-v0150-iter132]]"
  - "[[iterations/done/iteration-132-dogfood-v0150-iter131-fixes]]"
  - "[[iterations/done/iteration-126-markdown-linter]]"
---

# Dogfood v0.15.0 — iter-132 follow-up

Binary: `hyalo 0.15.0 (kb dir: hyalo-knowledgebase)` built from `main` after the
iter-132 merge (95a7672). KBs exercised:

- Own KB (`hyalo-knowledgebase/`, 267 files) — primary
- GitHub Docs (`/Users/james/devel/docs/content/`, 3 640 files) — refreshed via `git pull`
- MDN Web Docs (`/Users/james/devel/mdn/files/en-us/`, 14 375 files) — snapshot-indexed
- Synthetic scratch KB (`/tmp/hyalo-dogfood-iter132/scratch`) for `mv`, `set`, `lint` repros

Focus per the request: exercise every iter-132 fix, re-verify prior fixes,
push the markdown linter hard, and call out anything awkward.

## New Feature Verification (iter-132)

### BUG-A: `hyalo mv` rewrites `[[wikilinks]]` — MOSTLY FIXED

Repro from prior report now rewrites every form:

```text
$ hyalo --dir . mv b.md --to sub/b.md --format text
Moved b.md → sub/b.md
  a.md: [B](b.md) → [B](sub/b.md),
        [[b]] → [[sub/b]], [[b|alias]] → [[sub/b|alias]],
        [[B]] → [[sub/b]], [[b#sec]] → [[sub/b#sec]],
        [[b#sec|aliased]] → [[sub/b#sec|aliased]]
```

`find --broken-links` reports 0 after move; `backlinks sub/b.md` lists every
rewritten reference. Plain `[[b]]`, alias `[[b|x]]`, case-mismatch `[[B]]`,
section `[[b#sec]]`, alias+section all rewrite correctly.

**Partial gap — see BUG-1 below**: the relative-path form `[[./b]]` is not
rewritten. The iter-132 plan explicitly lists this case as an AC.

### BUG-B: `set --property date=<garbage>` note + lint rule — WORKING (with one shape-only gap)

`hyalo set c.md --property "date=not-a-date"` now emits:

```text
note: value "not-a-date" is not a valid ISO 8601 date (YYYY-MM-DD);
the property will sort lexicographically rather than chronologically
```

Same for `created=2026` (year-only), `updated=tomorrow` (prose). `date=2026-05-10`
is silent (as intended). `lint` surfaces a new `HYALO003 date-format` rule that
fires on the offending files. `lint-rules show HYALO003` reports
`default_enabled: true / default_severity: warn`, exactly as planned.

**Gap — see BUG-2 below**: `modified=2026-13-50` (shape-valid YYYY-MM-DD but
invalid month/day) is NOT flagged. The validator appears to be a regex on shape,
not a real `chrono::NaiveDate::parse`.

### BUG-C: `find --tag/--property <typo>` suggestions — WORKING

```text
$ hyalo find --tag iteraton
warning: no files matched --tag "iteraton"; did you mean: iteration?
$ hyalo find --property stauts=completed
warning: no files matched --property "stauts"; did you mean: status?
$ hyalo find --tag xyzzy        # far miss
No results                                       # no false-positive hint
```

Threshold tuning looks sane. Both `--tag` and `--property name=…` paths fire.

### UX-A: `create-index` outside-vault hint on text output — WORKING

```text
Error: output path is outside the vault boundary
  path: /tmp/hyalo-dogfood-iter132/test.idx
  hint: use --allow-outside-vault to override
```

JSON envelope unchanged. Hint is impossible to miss on text now.

### UX-B: `hyalo links` defaults to dry-run — WORKING

`hyalo links` and `hyalo links fix --dry-run` produce byte-identical output
(checked against own KB: 7 broken / 5 fixable). No subcommand-required error
anymore.

### UX-C: `--index-file` promoted to global — WORKING

```text
$ hyalo --index-file /tmp/own.idx find --limit 1   # works
$ hyalo --index-file /tmp/a.idx find --index-file /tmp/b.idx ...
   # subcommand wins, per spec
```

Confirmed across `find`, `lint`, `backlinks`. One caveat: passing
`--index-file` twice globally errors with the generic clap message
"cannot be used multiple times" rather than something more directed —
LOW noise.

### UX-D: `views <name>` shortcut — PARTIAL

iter-132 chose option B (introduce `views run <name>`) over option A
(`views <name>` bare). `hyalo views run open-tasks` works and forwards extra
flags correctly (`views run open-tasks --tag iteration`). However, the original
report's natural workflow `hyalo views open-tasks` (i.e. bare name) still
errors with `unrecognized subcommand 'open-tasks'` and offers no `did you mean
run open-tasks?` hint. This is the most common typo path the user will hit
after running `views list`.

### UX-E: `lint --strict` help text — WORKING

`hyalo lint --strict --help` now reads:

> Promote schema warnings to errors: "no 'type' property", "undeclared property
> in frontmatter", and date-format violations (HYALO003), causing lint to exit
> non-zero when those promotions fire.

Schema dependency is implicit ("no 'type' property"). Could be a touch more
explicit (e.g. "requires a `[types]` block in `.hyalo.toml`") but the
intent is now discoverable from the help. **Note**: the "exit non-zero" promise
turns out to be aspirational — see BUG-3.

### UX-F: `--sort path` and `--desc` aliases — WORKING

```text
$ hyalo find --sort path --limit 1   # OK
$ hyalo find --desc --limit 1         # OK
$ hyalo find --sort title --desc      # OK (combined)
```

Both aliases accepted everywhere `--sort file` / `--reverse` work.

## Bug Regression Testing (iter-131 follow-up + earlier)

| Bug from prior report | Status |
| --- | --- |
| BUG-1 — `find --file <abs-path-inside-vault>` returns results | STILL FIXED |
| BUG-2 — `lint-rules set --severity warn` removes override (no no-op section) | STILL FIXED |
| BUG-3 — `summary --format json` has top-level `dir` | STILL FIXED |
| UX-1 — `--dir` redundancy warning is single-prefixed | STILL FIXED |
| UX-2 — `--file <abs-path>` no longer silently empty | STILL FIXED |
| UX-3 — Banner emojis suppressed on piped output | STILL FIXED |

Older bugs (iter-127/130 era): `links fix` correctness, LLM misuse warnings,
CWD-aware help banner, `hyalo config` — all still functioning as documented.

## Bugs Found

### BUG-1: `hyalo mv` does not rewrite `[[./relative]]` wikilinks (MEDIUM)

The iter-132 plan AC explicitly lists `[[./b]]` (relative-path wikilink) as a
case to cover. In practice it survives `mv` untouched:

```text
$ cat a.md   # after mv b.md --to sub/b.md
See ... and [[./b]] and ...   # ← still points at old basename

$ hyalo --dir . backlinks sub/b.md
6 backlinks for "sub/b.md"     # ← [[./b]] not counted; the wikilink resolver
                                #   also can't resolve "./b" (shows as
                                #   "./b" (unresolved) in find --fields links)
```

**Impact**: medium. Relative-path wikilinks are rare in pure Obsidian but
common when authors hand-type "this directory" links. Detection (via
`find --broken-links` after `mv`) still works, but the headline iter-132
acceptance criterion is unmet.

**Expected**: `[[./b]]` rewrites to `[[./sub/b]]` (or `[[sub/b]]`) like the
other shapes.

### BUG-2: HYALO003 date-format is shape-only, not calendar-valid (MEDIUM)

```text
$ hyalo set x.md --property "modified=2026-13-50"
modified=2026-13-50: 1/1 modified
  "x.md"
   # ← no note emitted

$ hyalo lint x.md --rule HYALO003
… (showing 0 of 0 files with issues)
```

Shape `YYYY-MM-DD` matches even when month=13 and day=50 are nonsense.
`hyalo find --sort modified` will silently sort `2026-13-50` after a
real `2026-12-31`. The note-on-write and lint rule both fire only when
the string fails the regex, not when it fails `chrono::NaiveDate::parse`.

**Expected**: parse via a real date parser; reject `2026-13-50`, `2026-02-30`,
etc.

### BUG-3: `hyalo lint` always exits 0 (HIGH for CI)

```text
$ hyalo lint                     # 134 files with errors+warnings
EXIT: 0
$ hyalo lint --strict            # promotes warnings → errors
EXIT: 0
$ hyalo lint --rule HYALO001     # 2 HYALO001 errors on own KB
EXIT: 0
```

This is a regression against the iter-129 promise ("`lint --strict` causes
lint to exit non-zero") and against the `lint --strict --help` text quoted
above. It also makes `hyalo lint` unusable as a CI gate, which is the
canonical use-case for the strict mode.

**Note**: the prior iter-131 follow-up did not measure exit codes, so this
may have been silently broken for some time. Repros on both own KB and the
synthetic scratch KB (where HYALO001 errors are unambiguous).

**Expected**: exit 1 when any `error`-severity finding is reported; exit 1
under `--strict` when any promoted warning fires.

### BUG-4: `lint-rules remove` leaves an empty `[lint]` table behind (LOW)

```text
$ hyalo lint-rules set MD013 --enabled true
$ hyalo lint-rules remove MD013
$ cat .hyalo.toml
[lint]

[lint.rules]
```

Cosmetic — but it means a `.hyalo.toml` that started clean now has a
no-op `[lint]` / `[lint.rules]` pair. Same family as the iter-131 BUG-2,
just one level up. Self-heal would be to drop empty subtables on
serialise.

### BUG-5: HYALO001 misses the `- [] task` form (MEDIUM)

```text
$ printf -- '- [] task\n' >> file.md
$ hyalo lint --rule HYALO001 file.md
4 files checked, no issues   # ← not flagged
```

The rule fires for bare `[] task` (no leading dash) but ignores the
`- []` shape, which is the much more common Obsidian / GitHub-checklist
typo. Recall is the issue, not precision.

**Expected**: also flag `- []` and `* []`, converting to `- [ ]` / `* [ ]`.

## UX Issues

### UX-1: `views <name>` (bare) still surprises after `views list` (LOW)

`views list` is the natural starting point; the obvious follow-up
`hyalo views open-tasks` errors with `unrecognized subcommand 'open-tasks'`
and offers no hint. iter-132 implemented `views run <name>` but did not
wire the bare-name path to either dispatch-with-hint or `views list`
output hints. Adding a single `did you mean 'views run open-tasks'?` to
the unknown-subcommand path would close the loop.

### UX-2: `find --view <typo>` is exact-match only (LOW)

```text
$ hyalo find --view planne
Error: unknown view 'planne'
  tip: run 'hyalo views list' to see available views
```

The BUG-C fuzzy-suggestion plumbing already exists for tags and properties.
Extending it to view names is one call away and avoids a `views list` round
trip.

### UX-3: `properties` reports `priority` as both `text` and `number` (LOW)

```text
priority	number	6 files
priority	text	84 files
```

This is real — some files have integers, some strings. But the listing
gives no indication this is a *type inconsistency*. A schema-less KB user
will likely treat the two lines as two distinct properties. Ideas: collapse
into one line "priority (mixed: 6 number, 84 text)", or add a
`HYALO00N mixed-property-types` lint rule.

### UX-4: `create-index -o <outside-vault>` hint appears twice in text output (LOW)

```text
Error: output path is outside the vault boundary
  path: /tmp/x.idx
  hint: use --allow-outside-vault to override
```

The text output is already structured (error, path, hint). The JSON envelope
duplicates the `hint` field. Minor — but a user comparing the two will
wonder which is canonical. Either keep the text format and drop the JSON
key, or vice versa.

### UX-5: `lint-rules show` prints `--dir is redundant` only sometimes (LOW)

`lint-rules show MD013 --dir .` from inside the project root prints the
redundancy note. `lint-rules set MD013 --enabled true --dir .` from the
same directory does not. Both should behave the same.

### UX-6: `find ""` (empty body pattern) errors but `find` (no pattern) works (LOW)

```text
$ hyalo find ""
body pattern must not be empty; omit the pattern to match all files
```

The error message is friendly and correct. But scripted callers building
the query string from variables will hit this. A `--allow-empty-pattern`
or accepting empty as "no pattern" would unblock them. (Workaround:
detect empty in caller and drop the arg.)

## What Worked Well

- **`mv` link rewrite** on the main shapes is genuinely a relief: no more
  silently broken wikilink graphs after a rename. The per-file `[[old]]` →
  `[[new]]` log is a nice touch and exactly what a user wants to see.
- **Fuzzy hints for `--tag` / `--property`** are silent on far misses and
  hit the obvious typos. The signal-to-noise ratio felt right across own
  KB, docs, and the synthetic scratch KB.
- **Date-format lint rule** is a smart addition for schema-less KBs.
  HYALO003 is on by default at `warn` severity, which is the right tradeoff.
- **Global `--index-file`** finally makes external-KB workflows feel
  symmetric to local-KB ones. Combined with iter-130's global `--dir`
  this is the right shape for the read-only commands.
- **Performance is rock-solid**: own KB cold `find --limit 1` is 35 ms;
  MDN with index loads 14 375 files in 712 ms; MDN BM25 `find "javascript"`
  in 738 ms; docs `lint --fix --dry-run` across 3 640 files in well under
  a second. No noticeable regression versus the iter-131 report
  (which had similar shape).
- **Hints stayed relevant**: even on edge cases (zero results, dry-run
  fix preview, lint with no findings) the drill-down suggestions
  pointed at the next sensible command.

## Performance

| Command (cwd, KB) | Time |
| --- | --- |
| `find --limit 1` (own, no index) | 0.035 s |
| `find "linter" --limit 3` (own, BM25) | 0.082 s |
| `summary` (own) | 0.036 s |
| `find --limit 1` (docs/content, 3640 files, no index) | 0.162 s |
| `find "actions" --limit 1` (docs, BM25, no index) | 1.050 s |
| `find "actions" --limit 1` (docs, BM25, with index) | 0.186 s |
| `summary` (docs) | 0.358 s |
| `create-index` (MDN, 14375 files) | 2.605 s |
| `find --limit 1` (MDN, with index) | 0.712 s |
| `find "javascript" --limit 1` (MDN, with index) | 0.738 s |
| `lint --rule HYALO001` (MDN, with index) | 0.994 s |
| `lint --fix --dry-run` (docs, 3640 files) | < 0.5 s wall |

All well within the iter-131 ranges. Index dramatically helps BM25 on docs
(5.6× speedup); on MDN the index makes any whole-vault command interactive.

## What Went Well / What Felt Missing or Awkward

### Went well

- The four headline iter-132 deliverables (BUG-A wikilink rewrite,
  BUG-B date note + lint rule, BUG-C fuzzy hints, UX-C global
  `--index-file`) hit their target use cases on the first try. The
  ergonomic wins from UX-A/B/F (hint visibility, default `links` action,
  sort aliases) are small individually but add up to a *much* smoother
  read-only workflow.
- Error messages remain a clear strength. Hyalo's "tip:" / "hint:" /
  "note:" vocabulary is consistent and skimmable, and the drill-down
  hints rarely steered me wrong.
- Performance discipline holds. The own KB never made me wait; even MDN
  felt interactive once indexed.

### Felt missing / awkward / non-ergonomic

1. **`hyalo lint` always exits 0 (BUG-3) is the single biggest miss.**
   "Errors that aren't error-exits" defeats the canonical CI use case.
   The `--strict --help` text directly promises non-zero exit. This needs
   to be the first thing fixed in the next iteration.
2. **HYALO003 is shape-only (BUG-2).** A user reading the rule description
   ("not a valid ISO 8601 date") will reasonably assume `2026-13-50` is
   caught; it isn't. Swap the regex for `chrono::NaiveDate::parse`.
3. **`[[./b]]` slip in `mv` (BUG-1).** The iter-132 plan explicitly listed
   this AC. The fix landed for everything else; this case slipped.
4. **`views <name>` bare-name path (UX-1).** The plan said "implement
   `views <name>` → `find --view <name>` (forwarding any extra flags)";
   the merged code implemented `views run <name>` instead. That's a
   defensible choice (avoids name collisions with future subcommands)
   but the natural-workflow gap from the prior dogfood report isn't
   closed. Either rename / alias to bare form, or surface a hint on the
   unknown-subcommand path.
5. **HYALO001 doesn't match `- []` (BUG-5).** The rule documentation
   says "bare `[]` should be written as `- [ ]`", which strongly implies
   `- []` is in scope. It isn't.
6. **`--view <typo>` doesn't get the fuzzy treatment (UX-2).** The
   infrastructure from BUG-C is right there.
7. **`properties` listing for split-typed keys (UX-3) is misleading.**
   A `mixed` annotation or a dedicated lint rule would catch the
   `priority: 5` vs `priority: "5"` foot-gun.
8. **Cosmetic .hyalo.toml junk after `lint-rules remove` (BUG-4).**
   Same family as the fix iter-131 already shipped for `--severity` —
   should self-heal at the `[lint]` table level too.
9. **`find ""` rejection (UX-6).** Friendly, but blocks variable-driven
   scripts. Accept empty as a synonym for "no pattern".
10. **`lint-rules` redundancy-note inconsistency (UX-5).** Same flag,
    different paths in the same command family, different behaviour.

Net: iter-132 successfully closes most of the iter-131 follow-up issues
and ships three genuinely good additions (fuzzy hints, HYALO003,
global `--index-file`). The lint exit-code regression is the one item
that warrants an immediate fix; everything else is polish.

## Recommendations (priority order)

1. **Fix `hyalo lint` exit codes** (BUG-3) — non-zero on `error`-severity
   findings, and on any `--strict` promotion.
2. **Real date parsing in HYALO003 + the `set` note** (BUG-2) — use
   `chrono::NaiveDate::parse_from_str("%Y-%m-%d")` so `2026-13-50` is
   caught.
3. **Cover `[[./relative]]` in `mv`** (BUG-1) — same resolver, one more
   case.
4. **`- []` in HYALO001** (BUG-5) — extend the regex.
5. **Fuzzy match for `--view`** (UX-2) and bare-name hint for
   `views <name>` (UX-1) — reuse BUG-C plumbing.
6. **Drop empty `[lint]` tables on serialise** (BUG-4).
7. **Mixed-type indication in `properties` listing** (UX-3) — or a new
   `HYALO00N mixed-property-types` lint rule.
8. **`find ""` accept-empty mode** (UX-6).
