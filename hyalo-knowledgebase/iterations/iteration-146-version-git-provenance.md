---
title: Iteration 146 — Embed git sha + date in `hyalo --version`
type: iteration
date: 2026-05-26
status: planned
branch: iter-146/version-git-provenance
tags:
  - iteration
  - version
  - build-info
  - dx
related: []
---

## Goal

Make `hyalo --version` output build provenance — short git sha + commit
date — so a reported version from a dogfood / bug report uniquely
identifies the binary. Current output is just `hyalo 0.16.0`, which is
ambiguous across the many commits accumulated under the unreleased
0.16.0 stamp.

Target shape: `hyalo 0.16.0 (abc123def456 2026-05-26)`. A `+dirty`
suffix on the sha when the working tree had uncommitted changes at
build time.

When git is unavailable (crates.io tarball, offline build, or the
escape-hatch env var), fall back silently to the bare
`CARGO_PKG_VERSION` — no panic, no warning.

This mirrors the approach taken in `ff-rdp` at commit `4c01f0d`
("iter-82"), where the same problem was solved with a build script
emitting `cargo:rustc-env` strings consumed via `env!` in the args
module.

## Design

### `crates/hyalo-cli/build.rs` (new)

A build script that:

1. Emits `cargo:rerun-if-changed` directives for `.git/HEAD` and
   `.git/refs/` (resolved via `git rev-parse --git-dir` so worktrees
   work). Plus `cargo:rerun-if-env-changed` for the override env vars.
2. Honors `CARGO_HYALO_FORCE_NO_GIT=1` — emits empty strings, used by
   tarball builds and the tarball-fallback unit test.
3. Honors `GIT_COMMIT` + `GIT_COMMIT_DATE` env vars — CI/CD path,
   skips the git shell-out. Useful for hermetic builds.
4. Otherwise shells out to `git rev-parse --short=12 HEAD` and
   `git show -s --format=%cs HEAD`, plus `git status --porcelain`
   for the dirty check. Emits empty strings if any of these fail
   (not a panic).

Output env vars: `HYALO_BUILD_VERSION_SHA`, `HYALO_BUILD_DATE`.
Pure stdlib — no new dependencies.

### `crates/hyalo-cli/src/cli/args.rs`

Add `build_version_string()` that reads the two env vars at compile
time via `env!`, plus `CARGO_PKG_VERSION`. Returns:

- `"{PKG} ({SHA} {DATE})"` when SHA is non-empty.
- bare `PKG` when SHA is empty (tarball / offline).

Memoized in a `OnceLock<String>` since `clap`'s `version =` attribute
needs a `&'static str`.

Wire it into the `#[command(...)]` macro at line 99: change
`version,` → `version = build_version_string(),`.

### Tests

- **Unit test** in `args.rs`:
  - `build_version_string_with_sha` — set up via a closure / direct
    string-format check (mirrors ff-rdp's pattern; the env! values
    are baked in at compile time so the test must work with whichever
    SHA the test binary was built with).
  - `build_version_string_returns_pkg_version_when_sha_empty` — the
    bare-PKG fallback path. Cleanest way is to exercise the format
    logic in a helper that takes `sha/date` as parameters, then
    have `build_version_string()` call it with the `env!`-ed values.
    That makes both paths testable without rebuilding.
- **E2E test** in `crates/hyalo-cli/tests/e2e/version.rs` (new):
  - `version_includes_git_sha_when_built_from_git`: run
    `hyalo --version`, assert output matches
    `^hyalo \d+\.\d+\.\d+ \([0-9a-f]{7,12} \d{4}-\d{2}-\d{2}(\+dirty)?\)$`
    when SHA segment is present; if empty (offline build), assert
    bare semver form.
  - `version_shows_in_short_flag`: same content via `-V`.

### Docs

- `README.md`: short note in a "Version info" or "Building from
  source" section that the version string includes the git sha
  when built from a checkout.
- `CHANGELOG.md` `Unreleased` → `### Changed`: "`hyalo --version`
  now includes the git short-sha and commit date."

### Out of scope

- A `hyalo doctor` command surfacing the long-version (ff-rdp has
  this; hyalo doesn't have a `doctor`). Not adding one in this iter.
- Embedding the date in any JSON envelope. The version string is
  the only consumer for now.
- Embedding the build target triple. Add later if a dogfood
  surfaces a need.

## Steps

- [ ] Create `crates/hyalo-cli/build.rs` with the git-provenance
      logic, including the env-var override paths and the
      `CARGO_HYALO_FORCE_NO_GIT` escape hatch.
- [ ] Add `build_version_string()` to `cli/args.rs` with a
      `format_version_string(pkg, sha, date)` helper that the unit
      tests can exercise directly.
- [ ] Wire `version = build_version_string()` into the `#[command]`
      macro.
- [ ] Unit tests in `args.rs` for both paths (sha present,
      sha empty).
- [ ] New `tests/e2e/version.rs` with the binary-level assertion.
      Register the file in `tests/e2e/main.rs`.
- [ ] README + CHANGELOG entries.
- [ ] Confirm `cargo build --release` followed by
      `target/release/hyalo --version` prints the new format on the
      developer's machine.
- [ ] Quality gates: `cargo fmt`, `cargo clippy --workspace
      --all-targets -- -D warnings`, `cargo test --workspace -q`.
- [ ] xtask gates: `check-ac-fidelity`, `check-feature-fanout`,
      `check-help-drift`.

## Tasks

- [ ] build.rs implementation
- [ ] args.rs version-string assembly
- [ ] Unit tests (sha + tarball paths)
- [ ] E2E test
- [ ] CHANGELOG + README
- [ ] Quality + xtask gates green

## Acceptance criteria

- [ ] `hyalo --version` outputs `hyalo <semver> (<sha12> <YYYY-MM-DD>)`
      when built from a git checkout
- [ ] `hyalo -V` matches `--version`
- [ ] A dirty working tree at build time appends `+dirty` to the sha
- [ ] `CARGO_HYALO_FORCE_NO_GIT=1` env var produces the bare semver
- [ ] Build does not fail when run outside a git repo
      (e.g. `cargo install` from crates.io tarball would-be path)
- [ ] No new runtime dependencies; no new build dependencies beyond
      stdlib (no `vergen`, no `built`)

## Design notes

- **Why a build script, not `vergen`?** `vergen` pulls in
  `time`/`chrono` + several other crates. The ff-rdp approach (90
  lines of stdlib `Command` calls) does exactly what we need with
  zero supply-chain footprint.
- **Why `%cs` (commit date) and not `%ci` (commit ISO timestamp)?**
  Day granularity is enough for "which build is this?" and produces
  a cleaner version string. Timestamps add noise.
- **`+dirty` matters.** A dogfood report against a dirty binary
  reflects code that nobody else can reproduce. The marker makes
  that visible at a glance.
- **No `unwrap` in build.rs.** Failures degrade to "no provenance"
  (empty env vars), never panic — a broken build script breaks
  every consumer, including `cargo install`.

## References

- ff-rdp commit `4c01f0d` ("iter-82") — the reference implementation
- `crates/ff-rdp-cli/build.rs` — copy-paste-modify source
- `crates/ff-rdp-cli/src/cli/args.rs:209-234` — `build_version_string`
  pattern
