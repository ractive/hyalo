---
title: "Iteration 222 — security & robustness batch (jq limits, UTF-8 lint, managed-region + symlink integrity, Windows paths, error leaks)"
type: iteration
date: 2026-08-23
status: planned
branch: iter-222/security-robustness-batch
tags: [iteration, security, lint, write-path, cross-platform]
related:
  - "[[reviews/adversarial-review-2026-08-23]]"
  - "[[reviews/deep-analysis-2-2026-08-23]]"
  - "[[reviews/deep-analysis-3-2026-08-23]]"
  - "[[iterations/iteration-202-boundary-completion]]"
---

# Iteration 222 — security & robustness batch

## Goal

Clear the MEDIUM/LOW security findings and the ADVISORY items from
[[reviews/adversarial-review-2026-08-23]] that don't belong to the H-1
config-dir iteration: the invalid-UTF-8 lint abort, the `create-index -o`
symlink-policy divergence, the Windows drive-relative/ADS gap, and the
parser-internal error leaks. Also carries the HIGH `--jq` resource-limit
finding and the `init --claude` managed-region corruption from
[[reviews/deep-analysis-3-2026-08-23]] (F3-1, F3-2) — both are
robustness/write-path integrity, same threat surface. F3-1 is the
highest-severity item in this batch.

## Context

- **F3-1 (HIGH, VERIFIED)** — `crates/hyalo-cli/src/output.rs:790`: `--jq`
  is user/agent-supplied input evaluated with the ONLY guard being a 10 MiB
  output cap (`JQ_OUTPUT_CAP`). Two escapes, both reproduced on the release
  binary: (1) infinite CPU spin with no output — `hyalo find --jq 'def f: f;
  f'` hangs forever (the cap only fires when a value is emitted); (2)
  unbounded intermediate allocation — `hyalo find --jq '[range(3e8)] |
  length'` uses 4.8 GB RSS to print one number (the intermediate array is
  never counted). Not untrusted-vault content — the filter comes from the
  user/agent — so severity rests on the DoS-yourself agent-loop scenario
  (CLAUDE.md tells agents to build `--jq` programs; a wrong-but-plausible
  filter wedges or OOMs the machine).
- **F3-2 (MEDIUM, VERIFIED)** — `crates/hyalo-cli/src/commands/init.rs:926-927`:
  `upsert_managed_section` finds the FIRST `<!-- hyalo:end -->` anywhere in
  the file, including one appearing before the start marker (e.g. a stray
  marker mention in user prose). When `end_idx < start_idx` the `s < e`
  guard fails and it APPENDS a second managed section instead of replacing;
  `deinit`'s `strip_managed_section` then strips the original and orphans the
  appended one — silent corruption of the CLAUDE.md that steers agents. The
  sibling `strip_managed_section` (init.rs:680-689) and the shared
  `managed_region.rs` (`Markers::splice`, fixed for OKF/MADR in iter-165/166)
  already anchor END strictly after START; the CLAUDE.md upsert never got the
  fix. This is an argument for report #2's ARCH consolidation (route CLAUDE.md
  editing through `managed_region::Markers`, delete the hand-rolled copy).
- **M-1 (MEDIUM, VERIFIED)** — `crates/hyalo-cli/src/commands/lint.rs:2226`:
  `std::fs::read_to_string(full_path).with_context(...)?` propagates out of
  the per-file loop, so one invalid-UTF-8 file aborts the whole `lint` /
  `lint --fix` run (exit 2). Other commands already skip+warn per file
  (`scanner/mod.rs:99-104`). One corrupt file disables all autofix.
- **L-1 (LOW, VERIFIED)** — `crates/hyalo-core/src/index.rs:925-929`:
  `write_snapshot` uses raw `NamedTempFile::new_in` + `persist`, bypassing
  `fs_util`'s `resolve_write_target` and so replacing a symlinked
  `.hyalo-index` instead of following it (DEC-062 says the target is
  replaced, the symlink stays). Safety-neutral but a silent policy
  divergence: the `-o` path and the frontmatter write path give different
  answers for the same input.
- **M-2 (MEDIUM, SUSPECTED — needs Windows)** — `index.rs:659-676`
  (SEC-1) and the lexical `resolve_file` gate accept `C:foo`
  (drive-relative: `is_absolute()` is false, no `ParentDir` component) and
  `a.md:stream` (NTFS ADS is lexically in-vault). The `Component::Prefix`
  match only fires for absolute prefixed paths.
- **ADVISORY** — (a) `anyhow` RUSTSEC-2026-0190 (`downcast_mut` unsoundness)
  is not in `deny.toml`'s ignore list; if `cargo deny check` gates CI it may
  fail, and if not the advisory is untriaged. (b) YAML alias-bomb rejection
  leaks `budget breached: Anchors { anchors: 1 }` (saphyr-internal); same
  family as the NEW-8 `ScalarBytes {…}` leak that
  [[iterations/iteration-219-list-splice-and-write-polish]] is already
  wrapping — coordinate so the mapping lives in one place. (c) case-probe
  files (`case_index.rs:420-437`) are created/deleted inside the vault,
  pinging file watchers and racing `git status`.

## Tasks

- [ ] F3-1 (HIGH): bound `--jq` evaluation. (a) run the jaq iterator on a
      thread with a wall-clock deadline (check an `Instant` every N steps and
      bail); (b) add a step/allocation ceiling — cap total emitted value count
      and per-value serialized size, and/or a global step counter — so an
      infinite/huge intermediate is stopped before OOM, not just at output;
      (c) document the limits in `--jq --help`. Tests (per report #2's
      note): `assert_cmd` with `.timeout()` asserting the command ERRORS
      rather than hanging on `def f: f; f` and on `[range(3e8)]`
- [ ] F3-2: fix `upsert_managed_section` to anchor the END marker strictly
      after START (like `strip_managed_section` already does), so a stray
      marker mention in prose can't cause a duplicate append. PREFERRED:
      route CLAUDE.md section editing through `managed_region::Markers` and
      delete the two hand-rolled line-scanners (coordinates with report #2
      ARCH consolidation). Test: the review's exact repro (stray
      `<!-- hyalo:end -->` before the real section) round-trips through
      `init --claude` / `deinit` with no duplication or orphan
- [ ] M-1: catch the UTF-8 (and read) error per file in the lint loop, emit
      one diagnostic per offending file, continue; still exit non-zero at
      the end when strictness requires it. Mirror `scanner/mod.rs`'s
      skip+warn. Test: a vault with one invalid-UTF-8 file lints the rest
      and `--fix` still fixes the rest
- [ ] L-1: route `write_snapshot` through `fs_util::atomic_write`/
      `atomic_write_within` (or `resolve_write_target` + the boundary check)
      so index writes share the one symlink policy — OR amend DEC-062 to say
      index writes replace links and document why. Test: `create-index -o`
      onto a symlink follows it (or the documented decision)
- [ ] M-2: on Windows, reject any rel path with a `Prefix` component or a
      colon in the final component, in BOTH the SEC-1 snapshot check and the
      lexical `resolve_file` gate. Gate the tests behind `#[cfg(windows)]`
      (and, if feasible, a lexical unit test that runs everywhere by
      constructing the components directly)
- [ ] ADVISORY-a: triage `anyhow` RUSTSEC-2026-0190 — bump anyhow if a fixed
      version exists, else add it to `deny.toml` with a written rationale.
      Confirm whether `cargo deny check` is actually a CI gate and record it
- [ ] ADVISORY-b: map the saphyr `Anchors {…}` (and any sibling budget
      structs) to a human message; reuse the wrapper
      [[iterations/iteration-219-list-splice-and-write-polish]] adds for
      `ScalarBytes` rather than duplicating it
- [ ] ADVISORY-c: move the case-insensitivity probe to a temp dir on the
      same filesystem (or a hidden `.hyalo`-prefixed transient) so it does
      not create/delete files in the user's vault; verify no residual file
      on either branch, and that the detection result is unchanged

## Acceptance criteria

- [ ] `--jq 'def f: f; f'` and `--jq '[range(3e8)] | length'` both error
      within a bounded time/memory instead of hanging or OOMing; the limits
      are documented in `--jq --help`
- [ ] The F3-2 stray-marker repro round-trips through `init --claude` /
      `deinit` with exactly one managed section and no orphan
- [ ] One invalid-UTF-8 file no longer aborts `lint` or `lint --fix`; the
      rest of the vault is linted/fixed and the bad file is reported once
- [ ] `create-index -o` onto a symlink follows one consistent, tested policy
      shared with the frontmatter write path (or DEC-062 is amended)
- [ ] Windows drive-relative and ADS rel paths are rejected by both gates
      (tested under `#[cfg(windows)]`)
- [ ] No user-facing error contains a saphyr/parser-internal debug struct
- [ ] `cargo deny check` is green with a documented disposition for
      RUSTSEC-2026-0190

## Non-goals

- The H-1 config-dir escape ([[iterations/iteration-221-config-dir-boundary]])
- Concurrency/crash-recovery tests for the write path
  ([[iterations/iteration-224-test-quality-hardening]])
