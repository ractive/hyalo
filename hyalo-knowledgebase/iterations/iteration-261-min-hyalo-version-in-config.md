---
type: iteration
title: "Iteration 261 — min_hyalo_version: vaults declare the oldest hyalo allowed to run against them"
date: 2026-09-01
status: planned
tags:
  - iteration
  - config
  - compatibility
  - dx
branch: iter-261/min-hyalo-version
related:
  - "[[backlog/min-hyalo-version-in-config]]"
---

# Iteration 261 — `min_hyalo_version`: vaults declare the oldest hyalo allowed to run against them

## Goal

Implement [[backlog/min-hyalo-version-in-config]]: a top-level
`min_hyalo_version = "0.21.0"` key in `.hyalo.toml`. A binary older than the
declared minimum refuses to run any vault command (exit 1, message naming both
versions), because the dangerous failure is the *silent* one — `find --property
sources.ref=…` on a pre-0.21 binary returns zero matches and an agent concludes
"nobody cites this source". Read the backlog item first; this plan only fixes
the implementation shape and the three points where it departs from the
proposal.

**Do NOT release; release is a separate user-gated step.**

## Context (verified at `370acdfb`)

- Config struct: `ConfigFile`, `crates/hyalo-cli/src/config.rs:217`, has
  `#[serde(deny_unknown_fields)]`. Loader `load_config_from` at `config.rs:882`;
  a TOML parse error becomes `ResolvedDefaults.malformed` (`config.rs:903-911`),
  the file degrades to built-in defaults (`dir` salvaged), the warning is
  printed by `emit_config_diagnostics` (`config.rs:1559`), and mutating
  commands are refused at `run.rs:1090` (iter-201, M-2).
- Single choke point: `run_inner`, `crates/hyalo-cli/src/run.rs:597`; config
  is loaded once at `run.rs:605` before clap parsing. The post-parse validation
  block `run.rs:1045-1100` (`resolve_effective` → diagnostics → `dir` boundary
  refusal → malformed-and-writes refusal) is where the gate goes — **before**
  the `writes()` check so reads are refused too.
- Commands that return before that block and therefore never see the gate:
  `Init` (`run.rs:902`), `Deinit` (`:914`), `Completion` (`:919`), `Config`
  (`:925`); `--version`/`--help` return during parse. `config` must stay exempt
  by design (`config.rs:376` — "it exists to surface exactly this").
- Running version: `env!("CARGO_PKG_VERSION")` via `build_version_string`,
  `crates/hyalo-cli/src/cli/args.rs:205`. The build SHA/date are display-only
  suffixes, not part of `CARGO_PKG_VERSION`.
- `semver` is **not** a direct dependency (only transitive via wasm tooling in
  `Cargo.lock:1412`). Adding `semver = "1"` to `crates/hyalo-cli/Cargo.toml` is
  a new direct dependency — check `deny.toml` passes.
- `hyalo config` report: `ConfigReport` struct `crates/hyalo-cli/src/commands/config.rs:20`,
  populated at `:124`, JSON envelope `:227` (`results.malformed` /
  `results.parse_error` at `:239`), text `format!` at `:401`. Four spots to
  touch. `hyalo config` does not print the binary version today.
- Config-level lint findings already exist as a `.hyalo.toml` pseudo-file
  result (`commands/lint/config_checks.rs`, wired at `commands/lint/run.rs:426`).
  Highest rule id is HYALO007.
- Docs that enumerate config keys: `docs/configuration.md:7-35` (canonical),
  `README.md:280-292` (abbreviated, points at the doc), `CLAUDE.md:21` and
  `crates/hyalo-cli/templates/rule-knowledgebase.md:19` (both describe what
  `hyalo config` reports), `crates/hyalo-cli/templates/skill-hyalo.md`
  (config mentions at `:306`, `:380`, `:601`).
- e2e: single target `tests/e2e/mod.rs`; helpers in `tests/e2e/common/mod.rs`
  (`hyalo()`, `write_md`, `typed_results`); `.hyalo.toml` is written with
  `fs::write` (see `build_project`, `tests/e2e/config_trust.rs:22`). The
  exit-1-with-JSON-error pattern is `config_trust.rs:~440-495`; the version
  string shape test is `tests/e2e/version.rs:16`.

### The version-skew property this key inherits

Iteration 195a recorded: "a new config key is warned-and-ignored by the
*released* hyalo that lint-kb CI installs." With `deny_unknown_fields` that is
more than a warning today: a binary that predates this iteration treats
`min_hyalo_version` as an unknown key, marks the whole file **malformed**,
runs reads on built-in defaults (no schema, `dir` salvaged) and refuses
writes. So on pre-feature binaries the key is *accidentally loud* (stderr
warning on every run, writes blocked) but reads still run schema-less. Full
protection — including reads — exists only from the release that ships this
iteration onward. State this plainly in `docs/configuration.md`, and **do not
add the key to this repository's own `.hyalo.toml` in this PR**: the `lint-kb`
CI job runs the latest *release* via `setup-hyalo` and would see a malformed
config. Adding it here is a follow-up after the next release.

## Decisions to record as DEC-266 (departures from the backlog proposal)

1. **No lint rule.** The backlog asks `hyalo lint` to "additionally flag" a
   minimum above the running binary. That condition is exactly the one the
   gate refuses on, and `lint` loads the config like every other command, so it
   never gets far enough to report anything — a HYALO008 would be dead code.
   The diagnostic path is `hyalo config` (exempt from the gate), which gains
   the declared minimum, the running version and a satisfied flag. Do not
   add `hyalo config check` either (no new subcommand for what `hyalo config`
   already shows; standing no-new-CLI-surface rule in [[decision-log]]).
2. **Declared value must be a plain `MAJOR.MINOR.PATCH`.** Pre-release or
   build metadata in the *declared* value is a config error — a vault has no
   business pinning to a pre-release. The *running* version is compared with
   its pre-release/build parts stripped (`0.23.0-pre` satisfies `0.23.0`), as
   the backlog specifies.
3. **The message points at one stable upgrade location, not a guessed
   package-manager command.** hyalo installs via brew, cargo, winget,
   `setup-hyalo`, and release binaries; guessing prints the wrong command more
   often than the right one. Name the required and running versions, the
   config path, and the README install anchor.

## Tasks

### A. Parse and validate the key [0/3]

- [ ] Add `min_hyalo_version: Option<MinHyaloVersion>` to `ConfigFile`
      (`config.rs:217`) and carry it into `ResolvedDefaults`. Implement
      `MinHyaloVersion` as a newtype over `semver::Version` with
      `#[serde(try_from = "String")]`, rejecting values that fail to parse or
      that carry pre-release/build parts, so an invalid value surfaces through
      the existing malformed-config path with toml's key/line location and a
      message of the shape `min_hyalo_version = "…" is not a release version
      (expected MAJOR.MINOR.PATCH)`. Add `semver = "1"` to
      `crates/hyalo-cli/Cargo.toml`; run `cargo deny check` (or whatever
      `deny.toml` is wired to) and confirm it passes.
- [ ] Add a pure `fn version_satisfies(running: &str, min: &semver::Version) -> Result<bool>`
      (or equivalent) that strips pre-release/build from `running` before
      comparing. Unit tests: equal, lower, higher, patch/minor/major each,
      running pre-release equal to min → satisfied, running `0.22.0` vs min
      `0.22.1` → not satisfied.
- [ ] Unit tests for the deserializer: `"0.21.0"` ok; `"0.21"`, `"v0.21.0"`,
      `"0.21.0-rc1"`, `"0.21.0+build"`, `""`, non-string → error with the
      expected message; key absent → `None`.

### B. The gate [0/3]

- [ ] Insert the check in `run_inner`'s post-parse block (`run.rs:1045-1100`)
      **before** the `malformed && writes()` refusal, after
      `emit_config_diagnostics`. When the running version is below the
      declared minimum: return `AppError::User` (exit 1) through
      `crate::output::format_error` so `--format json` produces the same
      `{"error": …}` object as the malformed-config refusal. Message shape:
      `this vault requires hyalo >= 0.23.0 (<config-dir>/.hyalo.toml: min_hyalo_version); running 0.22.0`
      followed by `upgrade: see <README install URL>`, where the URL is
      `https://github.com/ractive/hyalo#install` — verify that anchor exists
      in README first.
- [ ] Confirm which commands bypass the gate and that the set is exactly
      `config`, `completion`, `--version`, `--help` plus `init`/`deinit`
      (which return before the block today). Decide `init`/`deinit`
      explicitly in DEC-266: recommendation is to leave them exempt — neither
      interprets vault content, and `deinit` on a too-old binary must remain
      possible to back out — but the decision must be written down, not
      inherited from control flow.
- [ ] Check the `--dir` case: the gate applies to whichever config
      `load_config` resolved (naming another tree switches to that tree's
      config, DEC-261/262). Add an e2e that `--dir <vault-with-min>` from
      outside the vault is refused and `--dir <vault-without-min>` from inside
      a vault with the key is not.

### C. `hyalo config` surfaces it [0/1]

- [ ] Extend `ConfigReport` (struct `:20`, collect `:124`, JSON `:227`, text
      `:401`) with the declared minimum (absent → `null`/omitted line), the
      running version (`CARGO_PKG_VERSION`, always present from now on), and
      a boolean satisfied flag. Text form on mismatch must be unmissable, e.g.
      `min_hyalo_version: 0.23.0  (running 0.22.0 — TOO OLD, every other
      command will refuse)`. JSON field names are the implementer's call but
      go through `check-command-reference`/help-drift gates and into the
      `CLAUDE.md`/`rule-knowledgebase.md` sentence that lists what `hyalo
      config` reports.

### D. Tests [0/2]

- [ ] e2e module `tests/e2e/min_hyalo_version.rs`: vault with
      `min_hyalo_version = "99.0.0"` → `find`, `lint`, `set` each exit 1, the
      stderr/JSON error names `99.0.0`, the running version and the install
      anchor, and the target file of `set` is byte-identical afterwards;
      `hyalo config` still exits 0 and reports the mismatch in both formats;
      `--version` still exits 0. Vault with `min_hyalo_version = "0.1.0"` and
      vault with the key absent → `find`/`set` behave as today. Invalid value
      (`"0.21"`) → the malformed-config warning names `min_hyalo_version`,
      reads run, `set` is refused (existing behaviour, now covered for this
      key). Derive the "equal" case from `env!("CARGO_PKG_VERSION")` at test
      time so it does not rot on the next version bump.
- [ ] Run the existing `config_trust.rs`, `hyalo_config.rs`, `version.rs`
      suites unchanged — the new field must not alter any existing envelope
      or text line when the key is absent (the running-version line/field in
      `hyalo config` is the one intentional addition; update those tests'
      expectations for it, nothing else).

### E. Docs, in the same PR [0/3]

- [ ] `docs/configuration.md`: add `min_hyalo_version` to the top-level block
      (`:7-11`) with a one-line comment, plus a short section: semantics
      (refuse-not-warn and why), value format, what is exempt, and the
      version-skew paragraph above (protection starts at the release that
      ships it; pre-feature binaries see a malformed config instead). Mention
      it next to the sequence-descending dot-path example that motivated it,
      if that example lives in the schema/`find` docs.
- [ ] `CLAUDE.md:21` and `crates/hyalo-cli/templates/rule-knowledgebase.md:19`:
      extend the "what `hyalo config` reports" sentence. `skill-hyalo.md`
      only where it already enumerates config keys. README only if its
      existing prose becomes wrong — README is not a feature list
      ([[decision-log]] README rule); the abbreviated block at `:280-292`
      points at `docs/configuration.md`, which is enough.
- [ ] `CHANGELOG.md` `[Unreleased]` entry under Added; DEC-266 in
      `decision-log.md` covering the three decisions above and the
      `init`/`deinit` call; **do not** touch this repo's `.hyalo.toml`.

### F. Close out [0/2]

- [ ] `hyalo mv backlog/min-hyalo-version-in-config.md backlog/done/min-hyalo-version-in-config.md`
      and `hyalo set backlog/done/min-hyalo-version-in-config.md --property status=completed`;
      tick its acceptance checkboxes with `hyalo task toggle`. Fix the
      `related:` link in this file if `mv` does not rewrite it.
- [ ] Record outcomes below: the final message text, the exempt-command set,
      and the measured overhead of the check on `find --limit 1` against the
      MDN vault (expected: unmeasurable — one semver parse per run — but say
      so with a number, the 260 floor is 151 ms).

## Acceptance criteria

- [ ] `min_hyalo_version` is parsed from `.hyalo.toml`; optional; a
      non-release or unparsable value is a malformed-config error that names
      the key.
- [ ] Every command except `config`, `completion`, `--version`, `--help` (and
      `init`/`deinit` if DEC-266 keeps them exempt) exits 1 when the running
      version is lower; reads included.
- [ ] Missing key changes nothing; equal or higher version changes nothing;
      running pre-release/build parts are ignored in the comparison.
- [ ] The refusal names the required version, the running version, the
      config path and the install location, in text and as a JSON error
      object under `--format json`.
- [ ] `hyalo config` shows declared minimum, running version and the
      satisfied flag in both formats.
- [ ] `docs/configuration.md` documents the key including the version-skew
      caveat; `CLAUDE.md`/`rule-knowledgebase.md` config sentence updated;
      CHANGELOG and DEC-266 written; this repo's `.hyalo.toml` unchanged.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `cargo deny check`, all CI `xtask check-*`,
      `hyalo lint --strict`.

## Non-goals

- A lint rule for the mismatch (DEC-266 point 1) and a `hyalo config check`
  subcommand — rejected, not deferred.
- A *maximum* version or a per-feature capability list — the backlog does not
  ask for it and one floor is the whole requirement.
- Setting the key in this repository's `.hyalo.toml` — follow-up after the
  release that contains this iteration.
- Making pre-feature binaries understand the key — impossible by definition;
  documented instead.

## Links

- [[backlog/min-hyalo-version-in-config]]
- [[iterations/iteration-195a-auto-link-config-exclusions]] — the CI
  version-skew note this plan inherits
- [[iterations/iteration-201-config-trust]] — the malformed-config refusal
  the gate sits next to
- [[iterations/iteration-213-config-ux-polish]] — `hyalo config` envelope
