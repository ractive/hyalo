---
type: iteration
title: "Iteration 274 — Hints, help and contract polish sweep: every remaining LOW bug and UX item from the post-batch dogfood"
date: 2026-09-05
status: planned
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

- [ ] BUG-22: `summary`'s site-prefix hint names `[site] prefix`; the real key is top-level
      `site_prefix`. Fix the text; add the hint to the recipe-execution gate below.
- [ ] BUG-17 / COH-17: the zero-result hint says "No file has a `status` property" when files
      have the key but not the value; distinguish key-absent from value-absent and list the
      existing values (kepano: 5 files, `[[Published]] (3), [[Active]] (1), null (1)`). Print
      the notice before the hints in text mode (COH-17, stream ordering).
- [ ] UX-2: hints on an indexed run thread `--index` / `--index-file <path>` the way they
      thread `--dir` and `--format` (MDN: 0.14 s vs 1.2–1.4 s to follow a hint), including
      `summary`'s `find --orphan` / `find --broken-links` hints.
- [ ] UX-3: `links fix` prints the "site_prefix stripped 0 of N site-absolute links" warning
      *before* fuzzy scoring and skips scoring when it fires (MDN: 28.7 s → seconds); the
      `find --broken-links` text hint that suggests `links fix` does not fire when every broken
      link is site-absolute.
- [ ] UX-11: the stop-list stderr note caps the `--exclude-title` list at the 5 it says it
      shows, and says that CLI flags *add to* the built-ins while `[links.auto] exclude_titles`
      replaces them.
- [ ] BUG-23: `[links.auto] exclude_titles = []` either switches the built-in stop-list off (as
      DEC-286's "replaces entirely" implies) or the note says "a non-empty list"; pick the
      former and pin it.
- [ ] UX-25: the outside-vault refusal on `create-index --index-file` no longer suggests the
      unsupported `--index --index-file` combination; the clap error for that combination says
      "cannot be combined", not "unexpected argument".

## HELP — help text that contradicts behaviour

- [ ] UX-7: `types set/show/list --help` leak 12-space doc-comment indentation; fix the
      `#[command(long_about)]` sources and add the check to `check-help-drift`.
- [ ] UX-8: `changelog add --type` did-you-mean points at `--property type=…`, which the
      command lacks; the tip must consult the subcommand's real flags (`--category`).
- [ ] BUG-27: the `find` text footer lists `score`, which `--fields score` rejects; drop it
      from the footer like `title_source` (DEC-275 round-trip promise).
- [ ] UX-9: `find --help` documents that `--file` with a missing path is an error while
      `--files-from` counts it and an empty `--files-from` list is a silent 0; make the empty
      list a `-q`-proof warning.
- [ ] Wishes from the report, documented rather than built: `tags rename` and `hyalo tags`
      cover frontmatter tags only (inline `#body/tags` are out); `find --property type=X` tests
      the raw value while binding normalises (`type~=` idiom); `--task todo --fields tasks`
      returns all tasks (the filter selects files). One sentence each in the relevant `--help`.
- [ ] BUG-29 + gate: replace `IN("external","attachment")` with `(.kind == "external" or
      .kind == "attachment")` in `templates/rule-knowledgebase.md`, `templates/skill-hyalo.md`
      and `.claude/CLAUDE.md`; add an xtask gate (or extend `check-bundled-skills`) that
      extracts every `--jq '…'` from the shipped templates and executes it against the own KB,
      failing on a jq error.

## CONTRACT — exit codes and JSON envelopes

- [ ] UX-1: record the exit-code taxonomy as a DEC amending DEC-276: 0 ok; 1 every hyalo-own
      user error; 2 clap usage errors and internal errors. Make `find a b` (hyalo's own
      did-you-mean-quotes error) exit 1 to match `--sort nope`.
- [ ] BUG-25: user errors that emit plain text and exit 2 under `--format json` — bad `--glob`,
      unreadable `--files-from` file, `create-index --output` into a missing directory,
      `init --profile nope` — emit the JSON envelope and exit 1.
- [ ] UX-13: `lint --fix` JSON gains `rules_fixed: {rule: n}` next to `rules_fired`, and text
      mode prints one line when `files_truncated` is true (`pass --limit 0 for the full list`).
- [ ] UX-14: `lint-rules list` reports the effective `enabled: true`, never `null`.
- [ ] UX-17: `task toggle` JSON `text` strips the trailing `\r` on CRLF files.
- [ ] UX-20: `okf index --dry-run` exits 0 like every other dry run (amend the iteration-176
      choice in the changelog entry).
- [ ] UX-12: `hyalo config` reports `dir_out_of_bounds` for an absolute or escaping `dir`
      instead of `"."`.
- [ ] UX-18: `deinit --dir <nonexistent>` exits 1.

## FIND — filter and sort polish

- [ ] UX-21: an empty comparison operand (`'a='`, `'a>='`), a double `=` (`'a=b=c'`), and an
      index/dot path on a non-map (`'title[0]=1'`, `'title.b.c=1'`) are rejected with exit 1
      like `'=b'` (DEC-276 family), not silently matched against nothing.
- [ ] UX-4: the mixed-type sort warning is computed over the sorted set, not the shown slice
      (`--limit 2` must warn when `--limit 0` does).
- [ ] UX-15: `--sort title` collates case-insensitively (DEC-273 fixed direction, not
      collation); `{%`-prefixed and lowercase titles sort among their peers.
- [ ] UX-16: an H1 fallback title strips `<!-- … -->` comments.
- [ ] UX-22: `links fix` does not warn "site_prefix stripped 0 of 1" for a `[[/a]]` that
      resolved.
- [ ] UX-23 (`title: 1e3` → `"1000.0"`) and UX-24 (`set --tag` on a nested list flips to block
      style): record as accepted YAML-1.2 behaviour in the Outcome, no change.

## LINT — schema and rule selection

- [ ] UX-5: `--rule SCHEMA` / `--rule-prefix SCHEMA` select the schema pass; `lint-rules list`
      shows a `SCHEMA` row (not configurable, `autofixable: false`).
- [ ] UX-6: an empty required typed placeholder (DEC-285) reports one error (`must not be
      empty`), not also `expected number, got null`.
- [ ] UX-19: `types set` rejects a type name containing whitespace or TOML-quoting characters,
      and prints one line when it sets `validate_on_write = true` as a side effect.
- [ ] BUG-19: `[links] case_insensitive = "false"` returns the canonical on-disk path or
      refuses the case-different link; `hyalo config` prints `links.case_insensitive`; the
      docs say the literal probe is filesystem-dependent if exact-match cannot be guaranteed.
- [ ] UX-10: `links fix --apply-fuzzy` text says "N candidates, M at or above <floor>" instead
      of "applied: N".

## Shared closing tasks

- [ ] Changelog entries via `hyalo changelog add` (one per group, listing the items).
- [ ] The exit-code DEC recorded in [[decision-log]].
- [ ] Every unfinished item moved to `backlog/` with its repro (not a new plan).
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*` gates
      including the new recipe-execution gate (invoke as
      `CARGO_MANIFEST_DIR=<repo>/crates/xtask ./target/debug/xtask <gate>`).

## Acceptance criteria

- [ ] Every item above is ticked, or has a backlog file, or is closed won't-do with a reason
      in the Outcome — no silent drops.
- [ ] The shipped `--jq` recipes all execute against the own KB and an xtask gate proves it.
- [ ] `find a b` exits 1; the four BUG-25 paths emit JSON and exit 1; the exit-code DEC exists.
- [ ] MDN `links fix --dry-run` (read-only) finishes in seconds with the site-prefix warning
      first; indexed hints carry `--index-file`.
- [ ] `hyalo lint --rule SCHEMA` works; `new` + `lint` on a typed placeholder reports one error
      per field.
- [ ] Help-vs-behaviour mismatches listed in the report's "Help-vs-behaviour" notes are gone;
      `check-help-drift` covers the indentation leak.
- [ ] Gates green; changelog; DEC.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-261-270]] — BUG-17, 19, 22, 23, 25, 27, 29; UX-1–25
- [[iterations/iteration-267-help-hints-text-polish]] — the previous sweep of this shape
- [[decision-log]] — DEC-273, DEC-275, DEC-276, DEC-285, DEC-286
