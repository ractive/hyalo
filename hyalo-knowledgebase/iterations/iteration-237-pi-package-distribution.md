---
title: "Iteration 237 — pi install package distribution: decouple extension updates from hyalo releases"
type: iteration
date: 2026-08-25
status: planned
branch: iter-237/pi-package-distribution
tags:
  - iteration
  - pi
  - distribution
  - extension
related:
  - "[[iterations/iteration-236-typed-pi-tools]]"
  - "[[decision-log]]"
---

# Iteration 237 — pi install package distribution: decouple extension updates from hyalo releases

## Goal

Publish the pi extension + skills as an installable pi package so `pi update`
delivers extension fixes independently of hyalo releases. Today the only
distribution channel is `hyalo init --pi`, which copies a template embedded
in the hyalo binary — every user of the old broken extension is stuck until
they both upgrade hyalo *and* re-run `hyalo init --pi`. The 2026-08-24
dogfood demonstrated the failure mode live: the demo pane ran a broken
extension for months because no mechanism existed to push the fix.

## Context

pi supports package installation from git sources (`pi install
git:github.com/ractive/hyalo`, then `pi update` / `pi update-packages`
refreshes). The repo already ships the required shape in
`crates/hyalo-cli/templates/package.json` (written to `.pi/package.json` by
`hyalo init --pi`): `pi.extensions`, `pi.skills` entries. What is missing is
a **top-level package layout** — pi installs point at a repo (or subpath),
and today the templates live under `crates/hyalo-cli/templates/` with the
extension as a single `.ts` file that is `include_str!`-ed into the binary.

Design decisions (from the session discussion, 2026-08-24):

1. **Single source of truth stays in-repo.** A new top-level `pi-package/`
   directory (extension `extensions/hyalo.ts`, skills `skills/hyalo/`,
   `skills/hyalo-tidy/`, `package.json`) is the canonical package; the
   `include_str!` template switches to `include_str!("../../../pi-package/...")`
   or a build step verifies the two copies are identical. No dual
   maintenance.
2. **The extension shells out to the installed `hyalo` binary** — it must
   stay compatible with *released* hyalo versions, not just main. The
   drift guard's layer-1 type-check runs against installed pi; add an
   equivalent compatibility note: extension release notes must state the
   minimum hyalo version (e.g. `[pi] session_summary` needs ≥ the 0.21
   release).
3. **`hyalo init --pi` keeps working and becomes an installer of last
   resort** — vendored copy for users who don't want a git dependency. It
   should print a hint suggesting `pi install git:github.com/ractive/hyalo`
   for auto-updates.
4. **Versioning**: the package carries its own `version` in `package.json`,
   bumped on every extension/skill change, with a CHANGELOG entry. pi's
   update mechanism handles fetching; we handle semver honesty.

Open questions to resolve at implementation time (not blocking the plan):

- Git source pinning: does the repo want a `pi` branch/tag strategy for
  package releases, or is main acceptable? (Check what `pi install
  git:...` actually pins to — commit, tag, or branch.)
- Whether `pi-package/` or a subpath install (`pi install
  git:github.com/ractive/hyalo#path=crates/hyalo-cli/templates`) is the
  better shape; the top-level directory is the assumption until proven
  otherwise.

## Tasks

- [ ] Verify pi's package-install mechanics against the installed version
      (`pi install --help`, docs `/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent/docs/`):
      what a git source resolves to (commit/tag/branch), where packages are
      stored, how `pi update` refreshes them, and whether subpath installs
      exist. Record findings in this file (amend the Open questions).
- [ ] Create top-level `pi-package/` layout:
      `pi-package/package.json` (version 0.1.0, pi manifest),
      `pi-package/extensions/hyalo.ts`, `pi-package/skills/hyalo/SKILL.md`,
      `pi-package/skills/hyalo-tidy/SKILL.md` — seeded as copies of the
      current templates.
- [ ] De-duplicate: switch `PI_EXTENSION_CONTENT`/skill `include_str!`
      sources in `crates/hyalo-cli/src/commands/init.rs` to the
      `pi-package/` files, or (if path-escaping `include_str!` is cleaner
      avoided) add a CI/test check that template files and package files
      are byte-identical. Choose one mechanism; document why in code.
- [ ] `hyalo init --pi` update path: after installing the vendored copy,
      print the `pi install git:github.com/ractive/hyalo` hint (one line,
      only when `.pi/` is being created, not on every re-run).
- [ ] End-to-end verify in a scratch checkout: `pi install
      git:github.com/ractive/hyalo` (or local path equivalent), confirm the
      `hyalo` tool registers, the lint guardrail fires, and `pi list` shows
      the package; then `pi update` picks up a pushed change.
- [ ] Extend `pi-extension-e2e.sh`: when `pi-package/` exists, type-check
      the package copy too (cheap: same layer-1 invocation on a second path).
- [ ] Docs: README section "Install the pi integration" (package install as
      primary, `hyalo init --pi` as vendored fallback); CHANGELOG entry;
      minimum-hyalo-version note in the package README.
- [ ] Decision log entry: distribution model decision (package-first,
  vendored fallback, versioning policy).

## Acceptance criteria

- [ ] A machine with pi but no hyalo repo checkout can install the
      integration via `pi install git:github.com/ractive/hyalo` and gets a
      working `hyalo` tool + skills (given a `hyalo` binary on PATH)
- [ ] Pushing an extension change to the package and running `pi update`
      delivers it — verified with a trivial marker change (e.g. a version
      bump in the tool description)
- [ ] `hyalo init --pi` output is unchanged except the one-line package
      install hint, and the vendored copy it writes is byte-identical to
      the package copy
- [ ] No template/package drift is possible in CI: either single-source
      `include_str!` or a byte-identity test gates it
- [ ] fmt / clippy / test green; `just pi-extension` green against both
      the package copy and the template copy

## Non-goals

- npm registry publishing (git source is sufficient until someone asks for
  a registry)
- Auto-updating the *hyalo binary* via pi (pi updates the extension/skills;
  hyalo stays Homebrew/cargo-install as-is)
- Making the extension work without a `hyalo` binary on PATH (it is a CLI
  wrapper by design)
- Divergent package/template contents (the whole point is one source)

## Out of scope / carry-over candidates

- Publishing the package version to a release tag strategy (decide after
  first real update cycle)
- A `hyalo doctor`-style check that reports extension/hyalo version
  compatibility drift (e.g. `[pi]` unknown → old hyalo binary)
