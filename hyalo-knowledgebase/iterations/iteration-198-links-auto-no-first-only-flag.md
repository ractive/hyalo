---
type: iteration
title: Iteration 198 — --no-first-only counter-flag for links auto
date: 2026-08-17
status: planned
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

**This is a NEW plan filed by the iter-195a review/merge sweep on
2026-08-18, not yet scoped or committed to.** Re-check whether a real user
has actually hit this gap before implementing — DEC-067 judged the existing
workarounds (narrower `--file`/`--glob` scope, or a temporary config edit)
adequate absent evidence otherwise.

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

- [ ] Re-evaluate demand: has any user actually hit this gap since iter-195a
      shipped? If not, consider `wont-do` with evidence.
- [ ] If proceeding: design the flag (`--no-first-only`, or a tri-state
      `--first-only=<true|false>`) and its precedence over
      `[links.auto] first_only`.
- [ ] Unit tests for the new override in `AutoFilters::effective_first_only`
      (or its replacement).
- [ ] e2e coverage.
- [ ] Docs: `links auto --help`, `docs/configuration.md`.

## Acceptance criteria

- [ ] TBD once the re-evaluation task above concludes proceed vs. wont-do.

## Non-goals

- Changing the OR semantics for any other `[links.auto]` key — this is
  scoped to `first_only` only.
