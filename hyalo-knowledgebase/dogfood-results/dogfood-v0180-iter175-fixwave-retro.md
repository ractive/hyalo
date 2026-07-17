---
title: "Iter-175 retrospective — v0.18.0 profile fix-wave (RB-4/RB-5/UX-A + mediums)"
type: research
date: 2026-07-17
status: active
tags:
  - dogfooding
  - iter-175
  - profiles
  - retrospective
related:
  - "[[dogfood-v0180-okf-profiles-pre-release]]"
---

# Iter-175 retrospective — v0.18.0 profile fix-wave

Fix-wave iteration closing the pre-release blockers and sharp edges from
[[dogfood-v0180-okf-profiles-pre-release]] so v0.18.0 is releasable.

## What shipped

### Release blockers

- **RB-4 — `changelog add` placement.** `insert_entry` now bounds the
  `[Unreleased]` section at the footer link-reference block (not just the next
  `## ` heading), so a new `### Category` lands inside the section even when
  `[Unreleased]` is the last section. Output stays MD047-clean (one trailing
  newline). `changelog add`/`release` also route through a resolved changelog
  path (see task 4) and their `file` field reports the file's basename.
- **RB-5 — bundled skills self-conformance.** The `skills` skill description
  dropped the literal `<`. New `check-bundled-skills` xtask gate lints every
  `crates/hyalo-cli/templates/skill-*.md` as installed (`.claude/skills/<name>/SKILL.md`)
  under the skills profile and fails CI on any error-severity finding; wired into
  `quality-gates.yml`. Also trimmed the pi skill description to ≤1024 chars to
  pass the gate.

### High-impact UX

- **UX-A — reach `.claude/skills/`.** New `[scan] include = ["glob", …]` config
  key (`hyalo-core::discovery`): a process-global glob set, installed once at CLI
  startup, that re-admits specific hidden dot-subtrees to the vault walker via
  `filter_entry`. `.git` is always hard-excluded. Honored by every command that
  discovers files. The skills profile ships `[scan] include = [".claude/skills/**"]`,
  and an ephemeral `--profile skills` run installs it too (via
  `config::overlay_scan_include`).

### Mediums bundle

- **Root `CHANGELOG.md` reachability.** `[changelog] path`, resolved relative to
  the config-file directory (validated against config-dir escape). `changelog
  add`/`release` and `lint --profile changelog` (which injects the resolved file
  into the lint set when it lives outside the vault dir) all reach a repo-root
  changelog. `init --profile changelog --dir <sub>` auto-writes the key when a
  root file exists.
- **Neutral OKF profile.** Removed the `BigQuery Table`/`BigQuery Dataset`/
  `Reference` example types from `profile-okf.toml`; documented "Adding domain
  types" in the okf skill.
- **Scaffold fidelity.** `hyalo new` now (a) applies `[schema.types.<t>.defaults]`
  (incl. `$today`) for defaulted non-required properties, and (b) omits the
  explicit `type:` key when the target path is covered by a `[[schema.bind]]` for
  that type (fixes the non-spec `type: skill` in SKILL.md scaffolds and empty
  Status/Date in `madr toc`).
- **`madr toc` type filter.** Only files whose effective type is `adr` (explicit
  or bound) enter the TOC when the madr schema is active; a plain vault keeps the
  historical "every .md" behavior.
- **`types set default` rejection.** `default` is reserved for `[schema.default]`;
  `types set default` is now a user error pointing at that table.
- **Small fixes.** `init --help` lists the `changelog` profile; the `--claude`
  CLAUDE.md managed section gains one pointer line per active profile; redundant
  `lint --profile <p>` hints are suppressed when the vault already activates `<p>`
  (`profile_lint_hint`); `hyalo config` reports the effective dir under a `--dir`
  override.

## Notes / follow-ups

- UX-B (skip counters in text/github output) was addressed in iter-174, not here.
- The `[scan] include` glob→directory-descent uses the glob's literal prefix
  (`glob_dir_prefix`) to decide which hidden dirs to enter, then the full glob to
  admit files. Deeply-nested include patterns with metacharacters early in the
  path fall back to descending from the first literal segment.
- `[changelog] path` deeper than one level is left to the user; init only
  auto-writes for the common one-level docs-subdir layout.
