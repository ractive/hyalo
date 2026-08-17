---
type: iteration
title: Iteration 198 — --no-first-only counter-flag for links auto
date: 2026-08-17
status: in-progress
tags:
  - iteration
  - links
  - auto-link
branch: iter-198/links-auto-no-first-only-flag
related:
  - "[[iterations/iteration-195a-auto-link-config-exclusions]]"
---

# Iteration 198 — --no-first-only counter-flag for links auto

## Goal

Carried over from DEC-067 (decision log, 2026-08-18): [[iterations/iteration-195a-auto-link-config-exclusions]]
added `[links.auto] first_only`, which lets `--first-only` be persisted so
every `hyalo links auto` run behaves as if the flag were passed. DEC-067
"also considered and deferred" a `--no-first-only` counter-flag, so a vault
with `first_only = true` in its config could get a single all-mentions run
without editing `.hyalo.toml`. It was out of iter-195a's fixed three-key
scope. The decision log's own text: "File a backlog item if a real user hits
it" — i.e. this was NOT judged urgent; it was deferred pending real demand.

**Filed by the iter-195a review/merge sweep on 2026-08-18 with the
re-evaluation gate below as its first task.** That gate has since concluded
**proceed** — see "Re-evaluation outcome" and [[decision-log#DEC-068]].

## Context

- `first_only` merge semantics (iter-195a,
  `crates/hyalo-cli/src/commands/links.rs` `AutoFilters::effective_first_only`):
  `cli_first_only || config_first_only` — OR-only, so there is currently no
  way to force `first_only = false` for one run when the config sets it
  `true`.
- CLI flag surface: `--first-only` is a `bool` flag in
  `crates/hyalo-cli/src/cli/args.rs` (`LinksAction::Auto`). A `--no-first-only`
  counter-flag would need its own clap arg (or an override enum) since clap
  bool flags don't have a built-in "force off" companion by default.
- Workarounds already available and documented (`docs/configuration.md`,
  added by iter-195a): narrow the run's scope with `--file`/`--glob`, or edit
  `.hyalo.toml` temporarily.

## Tasks

- [x] Re-evaluate demand: has any user actually hit this gap since iter-195a
      shipped? If not, consider `wont-do` with evidence.
- [x] If proceeding: design the flag (`--no-first-only`, or a tri-state
      `--first-only=<true|false>`) and its precedence over
      `[links.auto] first_only`.
- [x] Unit tests for the new override in `AutoFilters::effective_first_only`
      (or its replacement).
- [x] e2e coverage.
- [x] Docs: `links auto --help`, `docs/configuration.md`.

## Re-evaluation outcome (2026-08-18): PROCEED

No external user report surfaced since iter-195a shipped. The evidence that
tipped it was internal instead: `warn_common_titles` — the other boolean in
`[links.auto]` — already has `--no-warn-common-titles`, leaving `first_only`
the only setting in the section a single run cannot opt out of. Both
documented workarounds are poor substitutes: narrowing `--file`/`--glob`
changes *what is scanned* rather than *how it is linked*, and a temporary
`.hyalo.toml` edit mutates shared vault state that a killed run leaves behind.
The change is ~10 lines of merge logic on an existing flag surface, so the
cost side of "defer pending demand" was near zero. Recorded as
[[decision-log#DEC-068]].

## Acceptance criteria

- [x] `hyalo links auto --no-first-only` links every mention in a vault whose
      `.hyalo.toml` sets `[links.auto] first_only = true`, dry-run and
      `--apply` alike.
- [x] `--no-first-only` is a no-op when `first_only` is not enabled.
- [x] `--first-only --no-first-only` together is a clap conflict error, not a
      silent precedence rule; `effective_first_only` still tie-breaks to off.
- [x] Unit tests cover all `--no-first-only` × config combinations; e2e tests
      cover override, `--apply`, no-op, and conflict.
- [x] `links auto --help`, the COMMAND REFERENCE synopsis, `docs/configuration.md`,
      `CHANGELOG.md`, and the bundled knowledgebase rule template all describe
      the flag.

## Non-goals

- Changing the OR semantics for any other `[links.auto]` key — this is
  scoped to `first_only` only.
