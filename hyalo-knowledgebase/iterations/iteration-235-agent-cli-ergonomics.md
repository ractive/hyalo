---
title: "Iteration 235 — agent-facing CLI ergonomics: find --filenames-only, --iteration addressing, boundary-error hints"
type: iteration
date: 2026-08-24
status: in-progress
branch: iter-235/agent-cli-ergonomics
tags:
  - iteration
  - cli
  - agent-ergonomics
  - find
related:
  - "[[research/agent-ergonomics-ralph-loop-port-2026-08-24]]"
  - "[[research/cli-structured-output-patterns]]"
  - "[[research/results-json-shape-inventory]]"
---

# Iteration 235 — agent-facing CLI ergonomics: find --filenames-only, --iteration addressing, boundary-error hints

## Goal

Close the three highest-friction findings from the ralph-loop port dogfood
([[research/agent-ergonomics-ralph-loop-port-2026-08-24]]): give `find` a
compact filename-only projection, give iteration-typed vaults natural-key
addressing (`--iteration N`), and make the vault-boundary error self-healing.
These are the three findings where consumer skills (two generations of
ralph-loop, both agent-authored) documented workarounds instead of the tool
providing the feature.

## Context

hyalo's primary consumers are coding agents. The dogfood session porting
ralph-loop (Claude Code → pi + cmux) hit the same three traps the original
skill's comments had already documented:

1. `--format text` is multi-line per file; agents need `--jq '.results[].file'`
   every time, and the ralph-loop skill carries **three warnings** against
   tool defaults (`--format text`, `--property 'title~='`, multi-line parsing).
2. Iteration plans are named `iteration-206-<slug>.md` with frontmatter title
   `"Iteration 206 — ..."`, so "find iteration 206" via title matching fails
   silently; the correct query is a filename glob, discovered only by failure.
3. `hyalo set /tmp/foo.md ...` → `"file resolves outside vault boundary"` —
   correct rejection, but no hint of the vault root or the fix, so an agent's
   next attempt is usually another wrong absolute path.

Note the mutation side already has filter-batched addressing
(`set --glob ... --where-property ...`); only the read side lags.

Design decision (from review discussion): the filename projection is a
**find-local flag** (`--filenames-only`, grep `-l` precedent), NOT a new
`--format` value — `--format` is a cross-cutting text/json concern and
paths-only output only makes sense for result-list commands.

## Tasks

- [x] Add `--filenames-only` flag to `hyalo find` (FindFilters in
      `crates/hyalo-cli/src/cli/args.rs`, rendering in
      `crates/hyalo-cli/src/commands/find/mod.rs`):
      - Prints one raw file path per line (no JSON quoting, no count, no hints)
      - Zero results → empty output, exit 0
      - Conflicts with `--jq`, `--format json`, and `--count` (mutually
        exclusive projections; clap `conflicts_with_all`, exit 2 on misuse)
      - Works in combination with every existing filter (positional pattern,
        `--property`, `--glob`, `--tag`, `--file`, `--files-from`, `--broken-links`,
        `--orphan`, `--strict`, saved `--view`)
      - `--strict` still flips the exit code (1 when results exist) — that
        combination is the CI-gate + filename-list use case
      - Serialize in views (`FindFilters` is `serde::Serialize`; field must
        round-trip like other output-shaping flags or be explicitly skipped
        with a documented reason)
- [x] Add `--iteration <ID>` filter to `hyalo find` and accept it on
      `hyalo set` as the file selector (replacing glob math):
      - `ID` matches the natural key: bare integer or integer+letter suffix
        (`206`, `16b`, `01`) — same grammar ralph-loop/preflight use
      - Resolution: match files by the type schema's `filename_template`
        (`iterations/iteration-{n}-{slug}.md` — machinery already exists at
        `crates/hyalo-cli/src/dispatch.rs:1779` for `--type` → template → glob)
      - Selecting the *type* comes from the template lookup (iterate over
        types with a filename template; `--iteration` without a matching
        template config → clear error naming the configured templates)
      - Non-unique match (multiple files, e.g. bare `16` when `16b` also
        exists as a separate file): in `find`, return all matches (it's a
        filter); in `set`, error listing the candidates unless exactly one
      - `set --iteration 206 --property status=completed` must work end-to-end
- [x] Improve the vault-boundary error (both sites:
      `crates/hyalo-cli/src/commands/mod.rs:294` and
      `crates/hyalo-cli/src/commands/find/mod.rs:211`):
      - Message includes the effective vault dir (absolute path) and the
        offending path
      - Hint text: "pass a path relative to <dir>, or cd to a parent of it"
        (the find/mod.rs site already passes a hint — align the shared
        error-format call in commands/mod.rs to include dir + hint)
- [x] Update `--help` long text for `find` (`--filenames-only`, `--iteration`)
      and `set` (`--iteration`) following the existing help conventions
      (usage examples that show the agent use case)
- [x] Define `--iteration` interaction with the other `set` file selectors
      (`--file`, `--glob`, `--tag`, `--type`): they are **competing selectors** —
      clap `conflicts_with` on all of them (exit 2 when combined). `--where-property`
      still composes (it filters *within* the selection) — document this distinction
      in `--help`
- [x] Tests:
      - e2e: `find --filenames-only` with filters, zero-result, conflict
        matrix (--jq / --count / --format json), --strict exit code
      - e2e: `find --iteration 206` and `set --iteration 206 ...` happy path;
        ambiguity error; missing-template error; letter-suffix and
        zero-padded IDs
      - e2e: boundary error message contains vault dir and hint (there are
        existing assertions on the current message text in
        `tests/e2e/mv.rs:1401` and `tests/e2e/symlinks.rs:145` — update them,
        don't just append)
      - unit: ID grammar parsing (bare, letter, zero-pad, invalid)
- [x] Docs: `--filenames-only` and `--iteration` in the README command
      reference; mention in CHANGELOG unreleased section

## Acceptance criteria

- [x] `hyalo find --property status=planned --filenames-only` prints exactly
      the pending iteration plan paths, one per line, no decoration — usable
      in `sort`, `xargs`, `while read` pipelines
- [x] `hyalo find --iteration 206 --filenames-only` prints exactly the
      iteration-206 plan path; `hyalo set --iteration 206 --property
      status=completed` updates it — both without any glob or `--jq`
- [x] `hyalo set /tmp/foo.md --property x=1` fails with an error containing
      the vault dir and a relative-path hint
- [x] The ralph-loop skill's three hyalo-usage warnings could be dropped:
      with `--filenames-only` and `--iteration`, none of the warned-against
      patterns are needed (manual follow-up after merge — the skill lives at
      `~/.pi/agent/skills/ralph-loop/`, outside this repo, so it is not part
      of this iteration's deliverables or CI)
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all green; existing boundary-message test
      assertions updated to the new message
- [x] No behavior change for existing invocations: no new flag → identical
      output byte-for-byte, **except the vault-boundary error text (Task 3)**,
      which changes for existing invocations by design (regression-tested on
      a snapshot of current find/set output in the e2e suite)

## Non-goals

- Porting the ralph-loop skill (`~/.pi/agent/skills/ralph-loop/`) to consume
  the new flags — it lives outside this repo, so this repo's CI and review
  cannot verify it. Manual follow-up after merge (one iteration = one branch
  = one PR; nothing in this repo lands after the merge)

- NUL-delimited `--filenames0` sibling (add later if `xargs -0` composability
  is actually requested by a consumer)
- `--iteration` on other subcommands (`read`, `task`, `links`, ...) — extend
  only if consumers need it; find/set covers the ralph-loop workflow
- Unifying the `properties` command's output shape with the find envelope
  (review finding #4 — separate iteration; existing research:
  [[research/results-json-shape-inventory]])
- Title-vs-filename matching normalization in `--property 'title~='`
  (obsoleted for iterations by `--iteration`; revisit only if it bites on
  non-iteration types)
- Any change to `--format text`'s human layout itself (it stays the default
  for humans; `--filenames-only` is the agent projection, not a replacement)

## Out of scope / carry-over candidates

- `--filenames0` (NUL-delimited variant)
- `--iteration` on `read`/`task`/`links`
- `properties` envelope unification (finding #4)
- `title~=` normalization for non-iteration types (finding #2 remainder)
