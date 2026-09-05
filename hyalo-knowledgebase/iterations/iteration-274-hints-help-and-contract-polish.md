---
type: iteration
title: "Iteration 274 — Hints, help and contract polish sweep: every remaining LOW bug and UX item from the post-batch dogfood"
date: 2026-09-05
status: completed
tags:
  - iteration
  - hints
  - help
  - find
  - lint
  - config
  - dogfooding
branch: iter-274/hints-help-and-contract-polish
priority: 4
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-261-270]]"
  - "[[iterations/iteration-267-help-hints-text-polish]]"
  - "[[decision-log]]"
---

# Iteration 274 — Hints, help and contract polish sweep

## Goal

Every remaining LOW bug and UX item from [[dogfood-results/dogfood-v0220-post-batch-261-270]]
that is not claimed by iterations 271–273, in one sweep shaped like iteration 267. Groups 5 of
the report's recommendations plus the mutation/config odds and ends. Items are grouped by the
code they touch; most are one-function fixes with a fixture in the report.

Rules for this sweep: work the groups in order; **no new CLI flags** (an item that turns out
to need one closes as won't-do with a line in the Outcome, not a flag); time-box — anything
still open when the gates are green goes to `backlog/` with the report's repro, not to a new
iteration plan.

## HINTS — hints that mislead or cost more than they save

- [x] BUG-22: `summary`'s site-prefix hint names `[site] prefix`; the real key is top-level
      `site_prefix`. Fix the text; add the hint to the recipe-execution gate below.
- [x] BUG-17 / COH-17: the zero-result hint says "No file has a `status` property" when files
      have the key but not the value; distinguish key-absent from value-absent and list the
      existing values (kepano: 5 files, `[[Published]] (3), [[Active]] (1), null (1)`). Print
      the notice before the hints in text mode (COH-17, stream ordering).
- [x] UX-2: hints on an indexed run thread `--index` / `--index-file <path>` the way they
      thread `--dir` and `--format` (MDN: 0.14 s vs 1.2–1.4 s to follow a hint), including
      `summary`'s `find --orphan` / `find --broken-links` hints.
- [x] UX-3: `links fix` prints the "site_prefix stripped 0 of N site-absolute links" warning
      *before* fuzzy scoring and skips scoring when it fires (MDN: 28.7 s → seconds); the
      `find --broken-links` text hint that suggests `links fix` does not fire when every broken
      link is site-absolute.
- [x] UX-11: the stop-list stderr note caps the `--exclude-title` list at the 5 it says it
      shows, and says that CLI flags *add to* the built-ins while `[links.auto] exclude_titles`
      replaces them.
- [x] BUG-23: `[links.auto] exclude_titles = []` either switches the built-in stop-list off (as
      DEC-286's "replaces entirely" implies) or the note says "a non-empty list"; pick the
      former and pin it.
- [x] UX-25: the outside-vault refusal on `create-index --index-file` no longer suggests the
      unsupported `--index --index-file` combination; the clap error for that combination says
      "cannot be combined", not "unexpected argument".

## HELP — help text that contradicts behaviour

- [x] UX-7: `types set/show/list --help` leak 12-space doc-comment indentation; fix the
      `#[command(long_about)]` sources and add the check to `check-help-drift`.
- [x] UX-8: `changelog add --type` did-you-mean points at `--property type=…`, which the
      command lacks; the tip must consult the subcommand's real flags (`--category`).
- [x] BUG-27: the `find` text footer lists `score`, which `--fields score` rejects; drop it
      from the footer like `title_source` (DEC-275 round-trip promise).
- [x] UX-9: `find --help` documents that `--file` with a missing path is an error while
      `--files-from` counts it and an empty `--files-from` list is a silent 0; make the empty
      list a `-q`-proof warning.
- [x] Wishes from the report, documented rather than built: `tags rename` and `hyalo tags`
      cover frontmatter tags only (inline `#body/tags` are out); `find --property type=X` tests
      the raw value while binding normalises (`type~=` idiom); `--task todo --fields tasks`
      returns all tasks (the filter selects files). One sentence each in the relevant `--help`.
- [x] BUG-29 + gate: replace `IN("external","attachment")` with `(.kind == "external" or
      .kind == "attachment")` in `templates/rule-knowledgebase.md`, `templates/skill-hyalo.md`
      and `.claude/CLAUDE.md`; add an xtask gate (or extend `check-bundled-skills`) that
      extracts every `--jq '…'` from the shipped templates and executes it against the own KB,
      failing on a jq error.

## CONTRACT — exit codes and JSON envelopes

- [x] UX-1: record the exit-code taxonomy as a DEC amending DEC-276: 0 ok; 1 every hyalo-own
      user error; 2 clap usage errors and internal errors. Make `find a b` (hyalo's own
      did-you-mean-quotes error) exit 1 to match `--sort nope`.
- [x] BUG-25: user errors that emit plain text and exit 2 under `--format json` — bad `--glob`,
      unreadable `--files-from` file, `create-index --output` into a missing directory,
      `init --profile nope` — emit the JSON envelope and exit 1.
- [x] UX-13: `lint --fix` JSON gains `rules_fixed: {rule: n}` next to `rules_fired`, and text
      mode prints one line when `files_truncated` is true (`pass --limit 0 for the full list`).
- [x] UX-14: `lint-rules list` reports the effective `enabled: true`, never `null`.
- [x] UX-17: `task toggle` JSON `text` strips the trailing `\r` on CRLF files.
- [x] UX-20: `okf index --dry-run` exits 0 like every other dry run (amend the iteration-176
      choice in the changelog entry).
- [x] UX-12: `hyalo config` reports `dir_out_of_bounds` for an absolute or escaping `dir`
      instead of `"."`.
- [x] UX-18: `deinit --dir <nonexistent>` exits 1.

## FIND — filter and sort polish

- [x] UX-21: an empty comparison operand (`'a='`, `'a>='`), a double `=` (`'a=b=c'`), and an
      index/dot path on a non-map (`'title[0]=1'`, `'title.b.c=1'`) are rejected with exit 1
      like `'=b'` (DEC-276 family), not silently matched against nothing.
- [x] UX-4: the mixed-type sort warning is computed over the sorted set, not the shown slice
      (`--limit 2` must warn when `--limit 0` does).
- [x] UX-15: `--sort title` collates case-insensitively (DEC-273 fixed direction, not
      collation); `{%`-prefixed and lowercase titles sort among their peers.
- [x] UX-16: an H1 fallback title strips `<!-- … -->` comments.
- [x] UX-22: `links fix` does not warn "site_prefix stripped 0 of 1" for a `[[/a]]` that
      resolved.
- [x] UX-23 (`title: 1e3` → `"1000.0"`) and UX-24 (`set --tag` on a nested list flips to block
      style): record as accepted YAML-1.2 behaviour in the Outcome, no change.

## LINT — schema and rule selection

- [x] UX-5: `--rule SCHEMA` / `--rule-prefix SCHEMA` select the schema pass; `lint-rules list`
      shows a `SCHEMA` row (not configurable, `autofixable: false`).
- [x] UX-6: an empty required typed placeholder (DEC-285) reports one error (`must not be
      empty`), not also `expected number, got null`.
- [x] UX-19: `types set` rejects a type name containing whitespace or TOML-quoting characters,
      and prints one line when it sets `validate_on_write = true` as a side effect.
- [x] BUG-19: `[links] case_insensitive = "false"` returns the canonical on-disk path or
      refuses the case-different link; `hyalo config` prints `links.case_insensitive`; the
      docs say the literal probe is filesystem-dependent if exact-match cannot be guaranteed.
- [x] UX-10: `links fix --apply-fuzzy` text says "N candidates, M at or above <floor>" instead
      of "applied: N".

## Shared closing tasks

- [x] Changelog entries via `hyalo changelog add` (one per group, listing the items).
- [x] The exit-code DEC recorded in [[decision-log]].
- [x] Every unfinished item moved to `backlog/` with its repro (not a new plan).
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*` gates
      including the new recipe-execution gate (invoke as
      `CARGO_MANIFEST_DIR=<repo>/crates/xtask ./target/debug/xtask <gate>`).

## Acceptance criteria

- [x] Every item above is ticked, or has a backlog file, or is closed won't-do with a reason
      in the Outcome — no silent drops.
- [x] The shipped `--jq` recipes all execute against the own KB and an xtask gate proves it.
- [x] `find a b` exits 1; the four BUG-25 paths emit JSON and exit 1; the exit-code DEC exists.
- [x] MDN `links fix --dry-run` (read-only) finishes in seconds with the site-prefix warning
      first; indexed hints carry `--index-file`.
- [x] `hyalo lint --rule SCHEMA` works; `new` + `lint` on a typed placeholder reports one error
      per field.
- [x] Help-vs-behaviour mismatches listed in the report's "Help-vs-behaviour" notes are gone;
      `check-help-drift` covers the indentation leak.
- [x] Gates green; changelog; DEC.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-261-270]] — BUG-17, 19, 22, 23, 25, 27, 29; UX-1–25
- [[iterations/iteration-267-help-hints-text-polish]] — the previous sweep of this shape
- [[decision-log]] — DEC-273, DEC-275, DEC-276, DEC-285, DEC-286

## Outcome

Every item is closed. What shipped, and where a decision differs from the plan:

### Shipped as planned

**HINTS** — BUG-22 (`site_prefix`, not `[site] prefix`), BUG-17 (zero-result hints
separate key-absent from value-absent and name the values with counts; a
`status:` whose values are all `[[Wikilink]]` lists is no longer reported as a
missing property), UX-2 (`--index` / `--index-file` threaded into every hint
whose command accepts the flag, decided from clap's own tree), UX-3 (the
`site_prefix` diagnostic prints before scoring, and the site-absolute links are
excluded from fuzzy scoring once 500+ of them resolve nowhere — see below),
UX-11 + BUG-23 (flag list capped at the five the note says it shows; the note
states that flags ADD while `[links.auto] exclude_titles` REPLACES, and an
explicit `exclude_titles = []` now switches the built-in stop-list off).

**HELP** — UX-7 (`types` `long_about` indentation stripped, plus
`check-help-drift` gate 3f so it cannot come back), UX-8 (the unknown-flag tip
consults the invoked subcommand's real flags and names them when it has no
`--property`), BUG-27 (`score`/`matches` dropped from the `fields:` footer),
UX-9 (an empty `--files-from` list is a `-q`-proof warning; both `--files-from`
help texts document the missing-path contract), the three documentation wishes
(frontmatter-only tag scope, `type~=` vs raw `type=`, `--task` selects files not
tasks), BUG-29 (the `IN(...)` recipes replaced in all three shipped documents,
plus the new `xtask check-jq-recipes` gate that executes all 35 shipped `--jq`
recipes against this vault).

**CONTRACT** — UX-1 + BUG-25 as DEC-307, UX-13, UX-14, UX-17, UX-18, UX-20.

**FIND** — UX-21 (empty operand, double `=`, empty name), UX-4, UX-15, UX-16,
UX-22.

**LINT** — UX-5 (`--rule SCHEMA` / `--rule-prefix SCHEMA`, plus the
non-configurable `SCHEMA` row in `lint-rules list`/`show`), UX-6, UX-19,
BUG-19, UX-10.

### Decided differently, with the reason

- **UX-3 is volume-gated.** Skipping fuzzy scoring for *every* site-absolute
  broken link cost a real capability: a handful of them in an ordinary vault are
  usually genuine relocations a basename fallback repairs, and an e2e test
  proved it. The skip now needs both the `site_prefix` diagnostic AND 500+
  site-absolute broken links — the same threshold `summary`'s site-URL
  diagnostic uses. Below it the scoring pass is free; above it, it is the
  28.7 s MDN case the report measured.
- **UX-11 reverses dogfood L-12.** L-12 deliberately left the `--exclude-title`
  list uncapped so one paste-back silenced the note. A note that says "showing
  the 5 noisiest of 40" and then prints 40 flags contradicts its own sentence,
  and it re-fires on the next batch anyway. The cap wins.
- **UX-19's whitespace half was already done, minus spaces.** Quotes, tabs and
  every other TOML-quoting character are rejected; *interior spaces* stay legal
  because iteration-266's BUG-4 deliberately allowed them (a quoted TOML key
  round-trips and is a valid frontmatter `type:`). Only the
  `validate_on_write` side-effect line was missing, and it now prints.
- **UX-21's third case is a hint, not a parse error.** Whether `title.b.c=1`
  can match depends on what `title` holds in *this* vault, which the parser
  cannot know. The zero-result hint now says
  ``​`title` holds a scalar in all N files that set it, so the path `title.b.c`
  can never match``, which is the actionable half. Rejecting the syntax outright
  would also reject a legitimate path into a vault that does nest.
- **BUG-19 resolves to "canonical path + documented".** With
  `case_insensitive = "false"` hyalo already returns the canonical on-disk path;
  the link still resolves on a case-insensitive volume because the *literal*
  probe belongs to the filesystem, not to hyalo. Refusing it would mean
  stat-ing every path twice to second-guess the OS. `hyalo config` now reports
  `links.case_insensitive`, and both shipped documents state that exact-match
  resolution is guaranteed only on a case-sensitive filesystem.
- **UX-12 needed no change.** `hyalo config` already reports
  `dir_out_of_bounds: true` plus `dir_out_of_bounds_reason` quoting the rejected
  value, in both text and JSON; `dir` correctly shows the *effective* directory
  (the built-in default), which the text output labels as such.
- **COH-17's stream-ordering half was already done** in iteration 267: the
  empty-state notice is written to stderr before the hints reach stdout.
- **UX-23 and UX-24 are accepted YAML 1.2 behaviour, no change.** `title: 1e3`
  is a float in YAML 1.2 and round-trips as `1000.0`; `set --tag` on a nested
  list emitting block style is `serde_yaml`'s canonical form for a nested
  sequence. Quote the value (`title: "1e3"`) if the literal text matters.

### Nothing went to `backlog/`

Every listed item is either shipped or closed above with a reason.
