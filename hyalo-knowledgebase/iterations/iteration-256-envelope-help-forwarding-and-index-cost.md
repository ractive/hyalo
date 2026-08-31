---
type: iteration
title: "Iteration 256 — envelope unification, help forwarding, and --fields all index cost"
date: 2026-08-30
status: completed
tags:
  - iteration
  - dogfood-fixes
  - consistency
branch: iter-256/envelope-help-forwarding-index-cost
depends-on: "[[iterations/iteration-254-dogfood-v0220-help-and-shape-fixes]]"
---

# Iteration 256 — envelope unification, help forwarding, and `--fields all` index cost

## Goal

Carry-over sweep from [[iterations/iteration-254-dogfood-v0220-help-and-shape-fixes]]
(PR #296): three findings 254 explicitly named as Non-goals ("file a DEC or
its own iteration" / "decide separately" / "revisit after the next dogfood
round") plus one cosmetic reshuffle 254 deferred the same way. None of the
four touch result shape or short-help wording, so none belonged in 254's
scope; grouped here because each is a standalone decide-then-implement
task, not because they are related to each other.

## Tasks

### COH-9: "every mutating command reports dry_run and skipped_count" is false [2/2]

- [x] Read `dogfood-v0220-help-efficiency-and-find-shape.md`'s COH-9 section
      in full (`hyalo read dogfood-results/dogfood-v0220-help-efficiency-and-find-shape.md --section COH-9`)
      for the exact commands whose envelope omits `dry_run`/`skipped_count`
      despite `rule-knowledgebase.md` claiming it is universal. Enumerate
      every mutating command's actual envelope shape.
- [x] Decide: either make the envelope actually uniform (add the missing
      keys to the outliers) or soften `rule-knowledgebase.md`'s claim to
      name the exceptions. Record as a DEC either way — this is a
      documented-contract question, not a one-line fix. Update the bundled
      `SKILL.md`s and `CLAUDE.md` if the contract text changes.

### HELP-5: `hyalo help <cmd>` renders the full `--help`, not `-h` [1/1]

- [x] Decide whether `hyalo help <cmd>` should forward to the short `-h`
      page instead (agents default to `help <cmd>` out of habit and get the
      26 KB long form when they likely wanted the 2-3 KB short one). This is
      a clap `Subcommand::Help` behaviour change — confirm clap-derive
      supports intercepting it before committing to the approach; if not,
      document the `hyalo <cmd> -h` alternative more prominently instead
      (root `-h`'s "Everything else" line already added `hyalo help <cmd> =
      --help` in iter-251/254 — verify it's still accurate) and close this
      as won't-fix with a DEC explaining the clap constraint.

### FIND-8: `--fields all` costs ~20% wall time on an indexed vault [1/1]

- [x] Profile `find --fields all --index --limit 1` vs `find --index --limit 1`
      on a large indexed vault (reuse the `bench_scale` xtask harness —
      `cargo run -p xtask -- bench-scale` — or the MDN/GitHub-Docs vaults
      from the `dogfood` skill) to confirm the ~20% figure and find where
      the cost actually is (materialising `sections`/`links`/`tasks`/
      `backlinks`/`properties-typed` from the snapshot index vs. computing
      them fresh). Decide whether it's worth lazy-computing only the
      fields actually requested when reading from an index (the DEC-254
      exact-projection machinery this iteration depends on may make that
      easier now than when FIND-8 was first filed) or whether the cost is
      inherent and just needs documenting on `--fields all`'s help text.

### Root `-h` example set and command-group reshuffle (LOW) [1/1]

- [x] Revisit the top-level `-h` COMMANDS grouping and the five EXAMPLES
      lines chosen in iter-251/254 against a fresh dogfood pass — 254's
      Non-goals flagged this as "revisit after the next dogfood round",
      which this iteration's own carry-over sweep now is. If nothing reads
      wrong on a fresh read, close as no-op with a one-line note; don't
      force a change for its own sake.

## Acceptance criteria

- [x] COH-9 has a recorded DEC (either envelope made uniform, with tests, or
      the claim corrected in docs) and `rule-knowledgebase.md` matches
      reality either way.
- [x] HELP-5 has a recorded decision (forwarded, or won't-fix with the clap
      constraint documented) — not left ambiguous.
- [x] FIND-8's cost is measured on this iteration's code (not assumed from
      254's dogfood run) and either fixed or documented as inherent, with
      the number recorded.
- [x] The root `-h` reshuffle item is explicitly closed (changed or
      no-op'd), not silently dropped.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, all `xtask check-*`, `hyalo lint --strict`.

## Non-goals

- None of these four items are load-bearing for a release; if time is
  short, COH-9 and HELP-5 (both decision debt) take priority over FIND-8
  (a measured-but-tolerable perf cost) and the cosmetic reshuffle.

## Links

- [[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]
- [[iterations/iteration-254-dogfood-v0220-help-and-shape-fixes]]

## Outcomes

Four decisions recorded: [[decision-log#DEC-257]] (envelope),
[[decision-log#DEC-258]] (help forwarding), [[decision-log#DEC-259]] (FIND-8),
[[decision-log#DEC-260]] (root `-h`). Three of the four turned out to need
code, not just a written decision.

### COH-9 — enumerated, then half-unified

The claim was measured, not read off the source: every mutating command was
run against a scratch vault with `--format json` and its top-level `results`
keys recorded. 18 of 23 did not match the sentence. `dry_run` is now universal
on object-shaped results; `skipped_count` is documented as bulk-family-only;
`task toggle`/`task set` are named as the array-payload exception. See
DEC-257 for why the two halves resolved in opposite directions.

### HELP-5 — clap does not block it

The plan asked to confirm clap-derive can intercept `Subcommand::Help` before
committing to the approach. It can: `disable_help_subcommand` plus an argv
rewrite. `hyalo help find` 28 701 B → 2 992 B, and HELP-13 (no did-you-mean on
`help <typo>`) closes as a side effect.

The plan also asked to verify that root `-h`'s "Everything else" line still
says `hyalo help <cmd> = --help`. It did **not** — the line read
`Everything else:  hyalo --help  |  hyalo <cmd> -h`, with no mention of `help`
at all. Moot now that the equivalence runs the other way; the line reads
`hyalo <cmd> -h (== hyalo help <cmd>)  |  hyalo --help`.

### FIND-8 — the hypothesis on file was wrong

Measured on this iteration's code against MDN (14 399 files, 123 MB index),
best of 7, release build. The ~20% reproduced exactly (0.371 s → 0.448 s at
`--limit 1`), but it was not heavy fields being materialised before the limit:
per-field timing showed `sections`, `tasks`, `backlinks` and
`properties-typed` all free, and the entire delta sitting on `links`.
Instrumenting `maybe_case_index` put 62.4 ms of it in
`build_case_index_from_snapshot`, whose own doc comment claimed "microseconds".

Root cause was a quadratic dedupe in `CaseInsensitiveIndex::insert` — see
DEC-259. Fixed: 62.4 ms → 4.2 ms, and `--fields all --limit 1` is now 0.368 s
against a 0.371 s baseline. The cost is gone rather than documented.

Worth noting for future perf work: the remaining ~0.37 s floor on this vault
is snapshot load and deserialization of a 123 MB index, which dwarfs
everything `find` does afterwards. Any further `--fields` micro-optimisation
on an indexed vault is measuring noise until that is addressed.

### Root `-h` — no reshuffle, one factual fix

Grouping and examples stand (DEC-260). The third group's label claimed every
member writes `.hyalo.toml`; `create-index`/`drop-index` write an index and
`completions` writes nothing. Corrected.

## Dogfooding notes

- `hyalo init --dir <other-tree>` writes `.hyalo.toml` into the **current**
  directory with `dir = "<absolute path to other-tree>"` — a value the tool
  then refuses on every subsequent run ("an absolute path, which a
  project-local `.hyalo.toml` is not allowed to set"). During this iteration
  that silently replaced this repo's own 199-line `.hyalo.toml` with a
  one-line broken file. `init` should either write the path relative to the
  config's own directory or refuse to write a config it will not accept.
  Worse, `deinit` **also** operates on CWD while `--dir` points elsewhere: a
  single `hyalo --dir <temp-vault> deinit` from this repo deleted its
  `.hyalo.toml`, its `.claude/CLAUDE.md`, and the three `.claude` symlinks
  into `crates/hyalo-cli/templates/`. Nothing warned, and the summary listed
  the removals interleaved with a dozen `skipped … (not found)` lines, which
  reads like a no-op at a glance. `init`/`deinit` should either honour `--dir`
  for their target or refuse when `--dir` names a tree other than CWD.
- `hyalo find --property 'title~=/DEC-25/'` returns 0 results against a
  decision log whose DEC headings are `##` body headings, not titles — correct
  behaviour, but the natural query for "which DEC numbers are taken" is
  `hyalo find 'DEC-256'` or a body regex. A hint on a zero-result
  `title~=` query pointing at body search would have saved a round trip.
- `--format json` is ignored by `init` and `deinit`, which always print their
  text summary. They are outside the results envelope entirely (DEC-257 names
  them as such), but an agent piping their output gets no parseable result.
