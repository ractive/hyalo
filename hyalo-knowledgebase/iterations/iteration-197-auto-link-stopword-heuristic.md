---
type: iteration
title: Iteration 197 — stopword warning heuristic for links auto candidates
date: 2026-08-17
status: completed
tags:
  - iteration
  - links
  - auto-link
branch: iter-197/auto-link-stopword-heuristic
related:
  - "[[backlog/done/auto-link-config-exclusions]]"
  - "[[iterations/iteration-195a-auto-link-config-exclusions]]"
---

# Iteration 197 — stopword warning heuristic for links auto candidates

## Goal

Carried over from [[iterations/iteration-195a-auto-link-config-exclusions]]'s
non-goals: the "optional stretch" section of
[[backlog/done/auto-link-config-exclusions]] proposed warning when a `links
auto` candidate title is a very common English word (dictionary/stopword
heuristic), since the noise iter-195a's `[links.auto]` config now suppresses
is inherent to titles like "permissions" — a vault-specific exclusion list
still requires the user to notice the noise first. This iteration explores
whether a proactive warning is worth adding.

**Re-assessment outcome (2026-08-18): PROCEED, narrowed.** The plan asked
whether this is still wanted given that `[links.auto]` already gives the one
reporting user a durable fix. It is, for a reason the exclusion config cannot
address: `[links.auto]` is a *remedy* and the remedy only gets applied after
someone reads 33 candidates and works out which 31 are junk. The warning is
*discovery*, and it is the only part of the story that helps a first run. What
was dropped from the original stretch idea: no dictionary dependency, no
scoring of the title inventory, and no new report field — see the design
decisions below.

## Context

- Backlog stretch text (2026-07-04):  "warning when a candidate title is a
  very common English word (dictionary/stopword heuristic) suggesting it be
  excluded — the noise source here is inherent to titles like
  'permissions', not vault-specific."
- iter-195a shipped `[links.auto] exclude_titles` / `exclude_target_globs` /
  `first_only`, which lets a user silence the noise once they've seen it,
  but does nothing for a first-time `links auto` run before the user knows
  which titles are noisy.
- No stopword/dictionary dependency exists in the workspace today
  (`cargo tree` from the vault root before deciding on an approach) —
  CLAUDE.md requires "No polyglot tooling" and "New crates ... `hyalo-<domain>`"
  but says nothing against a small embedded static word list in Rust; a
  bundled ~200-word common-English list is likely cheaper and more
  predictable than a dependency.

## Tasks

- [x] Re-evaluate demand: has any other user hit the same noise pattern
      since iter-195a shipped `[links.auto]`? If not, consider dispositioning
      this as `wont-do` with evidence rather than implementing speculatively.
- [x] If proceeding: design the heuristic (bundled stopword list vs.
      length/frequency heuristic vs. something else) and where in the
      `links auto` dry-run report the warning surfaces.
- [x] Decide default-on vs. opt-in (a new `--warn-common-titles` flag or a
      `[links.auto]` key) — must not change existing dry-run/`--apply`
      output shape for vaults that don't opt in, to avoid a breaking change.
- [x] Unit + e2e coverage.
- [x] Docs: `links auto --help`, `docs/configuration.md` if a new config key
      is added.

## Design decisions

1. **Bundled static word list, no dependency.**
   `hyalo-core::common_words` holds ~780 high-frequency English words plus
   generic doc filenames (`readme`, `changelog`, `index`, `todo`, …) as a
   sorted `&[&str]`, queried by binary search. Regular plurals are normalised
   before lookup (`-ies`→`-y`, `-es`/`-s`→`-`), which is what makes the
   reported case ("permissions") match the stored `permission`. Non-ASCII
   titles are never classified — it is an English word list and pretending
   otherwise would be a lie dressed as a feature.
2. **Warn on emitted matches, not on the title inventory.** The check runs
   over the proposed replacements, so (a) a common-word title that produced no
   match is never mentioned, (b) the counts quoted are exactly the links being
   offered, and (c) excluding a title makes the note vanish — the heuristic is
   self-extinguishing and can never nag about something already handled.
   It also means zero extra file I/O and no second inventory pass.
3. **Default-on, stderr-only.** Opt-in was rejected: a warning you must first
   opt into cannot help a first run, which is the entire justification for
   building it. The compatibility requirement is satisfied instead by keeping
   the note off stdout — the report is byte-identical whether it fires or not
   (asserted by an e2e test), so no JSON consumer breaks. It rides the
   existing `warn::note` channel, so `-q` suppresses it and identical text
   dedups, like every other note.
4. **Two opt-outs, no opt-in flag.** `--no-warn-common-titles` for one run,
   `[links.auto] warn_common_titles = false` for every run. There is
   deliberately no `--warn-common-titles`: the default already does that.
5. **The note is one line and hands over the exact fix** — offenders sorted by
   match count (ties alphabetical, so output is deterministic), capped at 5
   named plus `+N more`, followed by ready-to-paste `--exclude-title` flags and
   a pointer at the persistent form.

## Acceptance criteria

- [x] `hyalo links auto` on a vault with a `permissions.md` page prints one
      `note:` on stderr naming `"permissions"` with its match count
- [x] The stdout report is byte-identical with the note enabled and disabled
- [x] Excluding the title (flag *or* `[links.auto] exclude_titles`)
      extinguishes the note
- [x] A vault with only domain-specific titles gets no note at all
- [x] `-q`, `--no-warn-common-titles`, and
      `[links.auto] warn_common_titles = false` each silence it
- [x] `hyalo config` reports `links.auto.warn_common_titles` in text and JSON
- [x] Unit coverage for the word list, the note text, and the flag/config
      merge; e2e coverage for every bullet above
- [x] `links auto --help`, `docs/configuration.md`, `CHANGELOG.md`, and the
      bundled knowledgebase rule template document the behaviour

## Non-goals

- Changing `[links.auto]`'s existing three keys (`exclude_titles`,
  `exclude_target_globs`, `first_only`) — this is additive only.
- Adding a `common_titles` field to the `links auto` envelope. Advisory text
  belongs on stderr; putting it in the report would be the breaking change the
  plan set out to avoid.
- Auto-excluding anything. The note never changes which links are proposed.
