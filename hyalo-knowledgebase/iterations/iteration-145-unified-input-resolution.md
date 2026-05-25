---
title: Iteration 145 — Unified input resolution + `--files-from` for task subcommands
type: iteration
date: 2026-05-25
status: planned
branch: iter-145/unified-input-resolution
tags:
  - iteration
  - tasks
  - files-from
  - refactor
related:
  - "[[iteration-139-files-from-flag]]"
  - "[[iteration-143-hint-and-files-from-polish]]"
---

## Goal

Collapse the three separate file-selection seams into a single resolver
and use it everywhere. Side-effect: `hyalo task toggle`, `task set`,
`task read`, `read`, and `backlinks` gain `--files-from` and `--glob`
for free, closing the iter-139 leftover.

## Background

Today, file selection lives in three places:

1. **`resolve_files_from_for_command()`** — `crates/hyalo-cli/src/run.rs:208-355`.
   Pre-dispatch translator for `--files-from`. ~10 per-command match
   arms, each ~10-15 lines: call `files_from::resolve`, mutate `file` /
   `glob` vectors, clear competing sources. ~150 lines of boilerplate.
2. **`collect_files()` + `resolve_index()`** — `crates/hyalo-cli/src/commands/mod.rs:74-218`.
   Resolves `--file` + `--glob` to a `Vec<PathBuf>` for multi-file
   commands. Well-factored, used by `find`, `lint`, `set`, `remove`,
   `append`, `links`.
3. **`resolve_single_file()`** — `crates/hyalo-cli/src/cli/args.rs:83-93`.
   Trivial single-file pick. Used by `read`, `backlinks`, `task read`,
   `task toggle`, `task set`, single-file `mv`.

The split is what blocks task subcommands from accepting `--files-from`
or `--glob`. The right fix is one resolver everywhere, not a fourth
arm in the per-command match.

## Design

### `InputSelection` (new)

```rust
// crates/hyalo-cli/src/cli/inputs.rs (new module)
#[derive(Debug, Default, Clone, clap::Args)]
pub(crate) struct InputSelection {
    /// Bare positional file argument (single).
    #[arg(value_name = "FILE")]
    pub file_positional: Option<String>,

    /// Explicit file paths (repeatable). Conflicts with positional.
    #[arg(long = "file", value_name = "PATH")]
    pub file: Vec<String>,

    /// Glob patterns to match (repeatable).
    #[arg(long = "glob", value_name = "PATTERN")]
    pub glob: Vec<String>,

    /// Read file list from a path or `-` for stdin.
    #[arg(long = "files-from", value_name = "PATH|-")]
    pub files_from: Option<String>,
}
```

Every command that takes file inputs replaces its current
positional/`--file`/`--glob`/`--files-from` fields with
`#[clap(flatten)] selection: InputSelection`.

### `resolve_inputs` (new)

```rust
// crates/hyalo-cli/src/commands/inputs.rs (new module)
pub(crate) struct ResolvedInputs {
    pub files: Vec<PathBuf>,
    pub counters: Option<FilesFromCounterSummary>,
}

pub(crate) fn resolve_inputs(
    selection: &InputSelection,
    dir: &Path,
    configured_dir: &str,
    snapshot_index: Option<&SnapshotIndex>,
    policy: ResolutionPolicy,
) -> Result<ResolvedInputs> { ... }
```

`ResolutionPolicy` is a small enum capturing the per-command semantics
the current code already encodes implicitly:

- `Multi { require_nonempty: bool }` — find/lint/set/etc.
- `Single { allow_glob: bool }` — read/backlinks/task-read.
- `SingleOrMany` — for commands that *can* take many but currently
  default to one (used by the migrated task toggle/set).

Returns `ResolvedInputs` with the unified counters that already flow
into the JSON envelope via existing `files_from_hints`.

### Dispatch

Every command arm in `dispatch.rs` collapses to:

```rust
let ResolvedInputs { files, counters } =
    resolve_inputs(&args.selection, dir, &cfg_dir, snapshot_index.as_ref(), policy)?;
```

`resolve_files_from_for_command()` deletes. `collect_files()` becomes a
private implementation detail of `resolve_inputs`. `resolve_single_file()`
deletes (or stays as a private helper called by `resolve_inputs` in the
`Single` policy branch).

### Counter envelope

`FilesFromCounters` already exists and is already merged into the
JSON envelope at the run.rs seam. After this refactor, the merge
point moves to a single location keyed on `ResolvedInputs.counters`.
No new envelope fields.

### Backwards compatibility

Every existing CLI invocation continues to work — `InputSelection`
is a flatten, not a structural CLI change. The only user-visible
deltas are:

- `task toggle`, `task set`, `task read`, `read`, `backlinks` gain
  `--files-from` and `--glob`.
- Help text for those commands grows the new flags.

## Steps

### Phase A — Resolver

- [ ] Create `crates/hyalo-cli/src/cli/inputs.rs` with `InputSelection`.
- [ ] Create `crates/hyalo-cli/src/commands/inputs.rs` with
      `ResolvedInputs`, `ResolutionPolicy`, and `resolve_inputs`.
- [ ] Port `collect_files` logic into `resolve_inputs` (Multi policy).
- [ ] Port `resolve_single_file` logic into `resolve_inputs`
      (Single policy).
- [ ] Port the `--files-from` resolve step from
      `resolve_files_from_for_command` arms into `resolve_inputs`.
- [ ] Unit tests for each policy (empty, positional-only, file-only,
      glob-only, files-from-only, files-from + filter, missing, mixed).

### Phase B — Migrate commands

For each command, replace per-command fields with
`#[clap(flatten)] selection: InputSelection`, then collapse the
dispatch arm to a single `resolve_inputs` call:

- [ ] `find`
- [ ] `set` (properties)
- [ ] `remove` (properties)
- [ ] `append`
- [ ] `lint`
- [ ] `links auto` / `links fix`
- [ ] `backlinks`
- [ ] `read`
- [ ] `task read`
- [ ] `task toggle`
- [ ] `task set`
- [ ] `mv` (single-file mode)

### Phase C — Cleanup

- [ ] Delete `resolve_files_from_for_command` from `run.rs`.
- [ ] Delete `resolve_single_file` from `cli/args.rs` (or demote to
      private inside `inputs.rs`).
- [ ] Make `collect_files` private to the new `inputs` module.

### Phase D — Tests + docs

- [ ] E2E: `task toggle --files-from list.txt --section "Tasks"`.
- [ ] E2E: `task set --files-from - --status x --all` (stdin).
- [ ] E2E: `task read --glob 'iterations/*.md'` returns multi-file
      result (or rejects with a clear error if policy is Single —
      decided during impl).
- [ ] E2E: counter envelope matches existing `--files-from` commands.
- [ ] Existing e2e suite must pass unchanged — that's the regression
      net.
- [ ] CHANGELOG `Unreleased` → Added (new flags) + Changed (unified
      resolver).
- [ ] README: short note that `--files-from` / `--glob` now work on
      all file-taking commands.
- [ ] `xtask check-feature-fanout`: update `feature-matrix.toml` to
      require `--file`, `--glob`, `--files-from` on every command that
      now carries `InputSelection`. The gate enforces drift.
- [ ] Move `iteration-145-...md` → `iterations/done/` after merge.

## Tasks

- [ ] Phase A: resolver + unit tests
- [ ] Phase B: migrate all 12 commands
- [ ] Phase C: delete dead code
- [ ] Phase D: e2e tests, CHANGELOG, README, feature-matrix
- [ ] All quality gates green (fmt, clippy -D warnings, full test
      suite, xtask gates)

## Acceptance criteria

- [ ] `resolve_files_from_for_command` no longer exists; one
      resolver in `commands/inputs.rs` is the only file-selection
      entry point.
- [ ] `hyalo task toggle --files-from <list>` and `hyalo task set
      --files-from <list>` work end-to-end with counters in envelope.
- [ ] `hyalo read`, `hyalo backlinks`, `hyalo task *` accept `--glob`
      (or reject cleanly per policy).
- [ ] `xtask check-feature-fanout` enforces `--file` + `--glob` +
      `--files-from` presence across all file-taking commands.
- [ ] No regression in existing e2e suite.

## Design notes

- **One struct, one resolver.** The reason the current code is
  fragmented is that `--files-from` was bolted on later (iter-139)
  without revisiting `collect_files`. The fix is to make the
  resolver the single source of truth for "what files does this
  command operate on?"
- **Policy enum, not trait.** `ResolutionPolicy` is a small enum
  rather than a `FileProvider` trait — commands don't customize
  resolution, they pick from a fixed menu. Enum is shorter and
  easier to grep.
- **Property/tag filtering stays out.** Those filters operate on
  parsed frontmatter, not on file paths. They live correctly inside
  `find_commands::find()` and are not the resolver's job. A future
  iteration could let other commands narrow by frontmatter, but
  that's a separate refactor with its own design questions
  (which commands need it? how does it interact with `--index`?).
- **Counter envelope unchanged.** The user-visible JSON shape stays
  identical; only the internal plumbing collapses.

## Out of scope

- Moving property/tag filters into the resolver layer.
- A new filter flag (e.g. `--exclude-glob`) — feasible after this
  refactor, but not part of this iteration.
- Changing the `--index` / `--index-file` plumbing.

## References

- [[iteration-139-files-from-flag]] — original `--files-from`
- [[iteration-143-hint-and-files-from-polish]] — polish + envelope
  merge
