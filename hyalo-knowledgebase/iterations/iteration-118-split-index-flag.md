---
title: Iteration 118 — Split --index into boolean flag + --index-file path
type: iteration
date: 2026-04-15
status: completed
branch: iter-118/split-index-flag
tags:
  - iteration
  - cli
  - ergonomics
related:
  - "[[iterations/iteration-105-summary-redesign]]"
  - "[[iterations/done/iteration-47-snapshot-index]]"
---

# Iteration 118 — Split `--index` into boolean flag + `--index-file` path

## Goal

Replace the current `--index [=PATH]` hybrid flag with two orthogonal flags
whose responsibilities don't overlap:

| Flag | Meaning |
|---|---|
| `--index` | Boolean: use the snapshot index at `.hyalo-index` in the vault dir |
| `--index-file PATH` / `--index-file=PATH` | Use the index at PATH (implies `--index`) |

This eliminates the entire class of ambiguity bugs around value consumption
and makes the 99%/1% split in real-world usage explicit in the API surface.

**This is a hard break — no migration path.** `--index=PATH` and `--index PATH`
are removed entirely; there is no deprecation shim. Users who pass a value to
`--index` get clap's standard "unexpected value" error. The release notes
and CHANGELOG call this out. The blast radius is small: five internal test
call sites, a handful of retrospective dogfood notes, two skill templates in
the repo, and the two installed skills (hyalo, hyalo-tidy).

## The issue

`crates/hyalo-cli/src/cli/args.rs:156` currently defines `--index` as a
hybrid `Option<PathBuf>` with `num_args = 0..=1`,
`default_missing_value = ".hyalo-index"`, and `require_equals = true`
(added in commit `b20d1f9` during iter-105). The three-paragraph
disambiguation note at args.rs:137–142 exists entirely because of this
design.

### How it fails today

1. **Silent misses.** `hyalo find --index kb/.hyalo-index` (space form)
   uses the default `.hyalo-index` and passes `kb/.hyalo-index` as the
   BM25 query — no warning, no error, just wrong results and a full disk
   scan instead of the promised index lookup.

2. **Loud failures for positional-FILE subcommands.** `hyalo lint
   --index .hyalo-index` errors with *"file not found: .hyalo-index,
   did you mean .hyalo-index.md?"* because `.hyalo-index` lands in
   `Lint`'s positional FILE slot. Same symptom for `hyalo links fix`.

3. **The `hyalo-tidy` skill uses the broken space form in all 27
   occurrences** (both the repo template at
   `crates/hyalo-cli/templates/skill-hyalo-tidy.md` and the installed
   copy at `.claude/skills/hyalo-tidy/SKILL.md`) — so *none* of the
   skill's tidy queries have actually been using the snapshot index
   despite advertising "minimize disk scans" as a core tenet. Ditto the
   nine occurrences in `skill-hyalo.md` / `.claude/skills/hyalo/SKILL.md`.

### Why dropping `require_equals` alone is worse

An earlier draft of this iteration proposed simply removing
`require_equals = true`. That makes `--index PATH` work but reintroduces
a different ambiguity: clap greedily consumes the next token as the
value, so `hyalo lint --index file.md` turns `file.md` into the *index*
path, not the FILE positional. For commands with required positionals
(`set`, `remove`, `mv`, `append`, `read`, `task`, `backlinks`, `views
set`, `types show`, etc.) clap then errors "missing required argument"
— visible but confusing. The hybrid shape keeps biting no matter which
constraint you pick.

The split design sidesteps the entire problem: `--index` takes no value
and cannot swallow anything; `--index-file` takes a required value and
behaves like every other `PATH`-valued flag.

## Design

### Scope the flag to subcommands that actually consume an index

Today `--index` is `global = true`, which means clap surfaces it on
*every* subcommand's `--help` output — including `hyalo create-index`
and `hyalo drop-index`, where "use the snapshot index" makes zero
sense: `create-index` is writing the index, `drop-index` is deleting
it. The flag is also noise on `hyalo init`, `hyalo completion`,
`hyalo views list/set/remove`, and `hyalo types list/show/remove/set`,
none of which touch the snapshot index.

Clap does not provide a clean way to exclude a `global = true` flag
from specific subcommands. The right fix is to stop making the index
flags global and attach them only where they apply, via a shared
`#[command(flatten)]` struct to avoid duplication:

```rust
/// Shared `--index` / `--index-file` flags for commands that can read
/// or mutate a snapshot index. Flatten into the subcommand's Args
/// struct rather than declaring these as globals on `Cli`.
#[derive(clap::Args, Debug, Default)]
pub struct IndexFlags {
    /// Use the snapshot index at .hyalo-index in the vault directory.
    ///
    /// Read-only commands (find, summary, tags summary, properties
    /// summary, backlinks) use the index to skip disk scans entirely.
    ///
    /// Mutation commands still read and write individual files on
    /// disk, but when the index is active they also patch the index
    /// entry in-place after each mutation — keeping it current for
    /// subsequent queries. This is safe as long as no external tool
    /// modifies vault files while the index is active.
    ///
    /// If the index file is incompatible (e.g. after a hyalo upgrade)
    /// hyalo falls back to a full disk scan automatically.
    ///
    /// For a non-default index path, use --index-file PATH (which
    /// implies --index).
    #[arg(long)]
    pub index: bool,

    /// Use the snapshot index at PATH. Implies --index.
    ///
    /// Relative paths are resolved against the current working
    /// directory (not the vault dir). Absolute paths are used as-is.
    #[arg(long, value_name = "PATH")]
    pub index_file: Option<PathBuf>,
}

impl IndexFlags {
    /// Returns the path of the snapshot index to use, or None if no
    /// index was requested. `--index-file` wins over `--index`; bare
    /// `--index` resolves to `.hyalo-index` inside the vault directory.
    pub fn effective_index_path(&self, vault_dir: &Path) -> Option<PathBuf> {
        if let Some(path) = self.index_file.as_ref() {
            return Some(path.clone());
        }
        if self.index {
            return Some(vault_dir.join(".hyalo-index"));
        }
        None
    }
}
```

### Subcommand index-flag matrix

| Subcommand | Index-aware? | Flags present |
|---|---|---|
| `find` | yes (read) | ✅ |
| `summary` | yes (read) | ✅ |
| `tags` (summary, rename) | yes | ✅ |
| `properties` (summary, rename) | yes | ✅ |
| `backlinks` | yes (read) | ✅ |
| `lint` | yes (read) | ✅ |
| `links` (fix, etc.) | yes (read) | ✅ |
| `read` | yes (read) | ✅ |
| `set` | yes (read + patch-in-place) | ✅ |
| `remove` | yes (read + patch-in-place) | ✅ |
| `append` | yes (read + patch-in-place) | ✅ |
| `mv` | yes (read + patch-in-place) | ✅ |
| `task` | yes (read + patch-in-place) | ✅ |
| `views` (list/set/remove) | no | ❌ |
| `types` (list/show/remove/set) | no | ❌ |
| `create-index` | no (it writes the index) | ❌ |
| `drop-index` | no (it deletes the index) | ❌ |
| `init` | no | ❌ |
| `completion` | no | ❌ |

Every row marked ✅ flattens `IndexFlags` into its Args struct. Rows
marked ❌ do not, so `--help` no longer advertises `--index` /
`--index-file` on commands that can't use them.

### Alternative approaches considered

1. **Drop `require_equals` only, keep hybrid shape.** Rejected (see "Why
   dropping `require_equals` alone is worse" above). Shifts ambiguity
   from space-form-breaks-silently to space-form-eats-positional.

2. **Custom `ValueParser` that detects index paths.** Brittle, surprising,
   complicates help output. Rejected.

3. **Rename to `--use-index` (bool) + `--index-file` (path).** Forces a
   rename of a widely-documented flag for no practical gain; `--index`
   as-a-toggle reads fine.

4. **Keep `--index=PATH` as a deprecated alias for one release.** User
   explicitly vetoed. The flag's documented behavior is changing (boolean
   vs. optional-value), and a silent-maybe compatibility layer is worse
   UX than a loud error telling users to switch to `--index-file`.

5. **Environment variable fallback (`HYALO_INDEX_FILE`).** Orthogonal
   nice-to-have; can be added later. Out of scope for this iteration.

## Surface area to update (ALL of the following must be synced in this PR)

This is a user-facing breaking change, so every place that mentions or
demonstrates the old flag form must be audited and updated together. Do
not split across PRs — the rule in [[CLAUDE.md]] is docs and code ship
together.

### 1. Rust source (definition, dispatch, help text)

- `crates/hyalo-cli/src/cli/args.rs` — flag definition (lines 135–157)
  and every `long_about`/`help` string that mentions `--index`
- `crates/hyalo-cli/src/cli/help.rs` — 3 occurrences of `--index` in
  help synopsis text
- `crates/hyalo-cli/src/run.rs` — reads `cli.index`; switch to the helper
- `crates/hyalo-cli/src/hints.rs` — any hint output that suggests
  `--index=PATH`

### 2. Help text audit

Every subcommand's `long_about` that mentions `--index` must be reviewed
and adjusted. `cargo run -- --help` and `cargo run -- <each subcommand>
--help` output must be read end-to-end after the change and verified
for consistency. No `--index=PATH` or `--index[=PATH]` syntax may remain
in any help text.

### 3. Tests

- `crates/hyalo-cli/tests/e2e/index.rs` — five `--index={path}` call
  sites at lines 596, 633, 656, 1431–1439

### 4. Repository skill templates

- `crates/hyalo-cli/templates/skill-hyalo-tidy.md` — 27 occurrences
- `crates/hyalo-cli/templates/skill-hyalo.md` — 9 occurrences
- `crates/hyalo-cli/templates/rule-knowledgebase.md` — check for any

### 5. Installed skills (checked into `.claude/skills/` in this repo)

- `.claude/skills/hyalo-tidy/SKILL.md` — 27 occurrences
- `.claude/skills/hyalo/SKILL.md` — 9 occurrences
- Any other installed skill that references `--index` (grep wide)

These should stay in sync with the templates in §4. If `hyalo init` or
another command regenerates them, verify the regeneration produces the
new form; otherwise edit both in this PR.

### 6. README and top-level docs

- `README.md` — 7 occurrences
- Anything else at the repo root that documents the CLI surface

### 7. Knowledgebase

- `hyalo-knowledgebase/**` — retrospective dogfood notes mention
  `--index=` (iter-113, iter-114, iter-115 dogfood results, plus the
  done iter-47 snapshot-index file). Historical records should be
  annotated with a short "superseded by iter-118" note rather than
  rewritten — readers researching past dogfood rounds need to see what
  actually happened.

### 8. CHANGELOG

- Add a `### Breaking changes` section with a migration example:
  ```
  -  hyalo find --index=./my.idx
  +  hyalo find --index-file=./my.idx
  ```

### 9. Replacement guidance for `--index PATH` in the tidy skill

In the skill doc, the correct replacement for every
`hyalo <cmd> --index .hyalo-index` is just `hyalo <cmd> --index` — the
KB uses the default location everywhere. Do not mechanically rewrite to
`--index-file=.hyalo-index` since that adds noise with no value.

## Tasks

### Code changes

- [x] Remove the current `pub index: Option<PathBuf>` global field from `Cli` in `crates/hyalo-cli/src/cli/args.rs`
- [x] Add the `IndexFlags` struct (derive `clap::Args`) with `pub index: bool` and `pub index_file: Option<PathBuf>` — **not** `global = true`
- [x] Add the `IndexFlags::effective_index_path(&self, vault_dir: &Path) -> Option<PathBuf>` helper (takes the vault dir from the parent `Cli` since the flag struct doesn't own `--dir`)
- [x] Flatten `IndexFlags` into every subcommand variant that consumes an index (see the matrix in the Design section): `Find`, `Summary`, `Tags*`, `Properties*`, `Backlinks`, `Lint`, `Links*`, `Read`, `Set`, `Remove`, `Append`, `Mv`, `Task*`
- [x] Do **not** flatten `IndexFlags` into `CreateIndex`, `DropIndex`, `Init`, `Completion`, `Views*`, or `Types*` — these subcommands must not advertise `--index` in their help output
- [x] Remove the three-paragraph disambiguation note on the old `--index` field; replace with the per-flag doc comments shown in "New surface"
- [x] Update every call site that reads `cli.index` (`run.rs`, dispatch, any command module) — read the flag from the subcommand's flattened `IndexFlags` instead of from `Cli`
- [x] Grep for `require_equals` in the crate; confirm no remaining flags depend on it for index-related parsing
- [x] Update `crates/hyalo-cli/src/cli/help.rs` and every `long_about` string that mentions `--index` — replace `--index[=PATH]` with the new two-flag pair
- [x] Update `crates/hyalo-cli/src/hints.rs` if it suggests `--index=PATH`

### Tests

- [x] Update the five `--index={path}` call sites in `crates/hyalo-cli/tests/e2e/index.rs` to use `--index-file={path}`
- [x] Add e2e test: `hyalo find --index` (bare boolean) uses the default `.hyalo-index`
- [x] Add e2e test: `hyalo find --index-file PATH` (space form) uses PATH
- [x] Add e2e test: `hyalo find --index-file=PATH` (equals form) uses PATH
- [x] Add e2e test: `hyalo lint --index file.md` — `file.md` is the positional FILE (not the index path); the index is active
- [x] Add e2e test: `hyalo find --index=garbage` errors cleanly (clap rejects a value on a boolean flag)
- [x] Add e2e test: `hyalo find --index-file` (no PATH) errors cleanly
- [x] Add e2e test: `hyalo find --index --index-file=PATH` — `--index-file` wins; verify the right file is read
- [x] Add e2e test: `hyalo create-index --index` errors (unknown flag) — proves the flag is no longer global
- [x] Add e2e test: `hyalo drop-index --index-file=foo` errors (unknown flag)
- [x] Add e2e test: `hyalo init --index` errors (unknown flag)

### Help-text audit (manual pass)

- [x] Run `cargo run -- --help` and every `cargo run -- <subcommand> --help` variant; read output end-to-end
- [x] Confirm no `--index=PATH`, `--index[=PATH]`, or "use the equals form" language remains in any help output
- [x] Update the long_about prose for every subcommand that referenced the old surface
- [x] **Verify `hyalo create-index --help` does NOT list `--index` or `--index-file`** (they made no sense there, previously surfaced via `global = true`)
- [x] **Verify `hyalo drop-index --help` does NOT list `--index` or `--index-file`**
- [x] Verify `hyalo init --help`, `hyalo completion --help`, `hyalo views --help` (+ list/set/remove), and `hyalo types --help` (+ list/show/remove/set) likewise omit `--index` / `--index-file`
- [x] Verify every subcommand marked ✅ in the Design matrix DOES list both flags

### Repository skill templates

- [x] Rewrite `crates/hyalo-cli/templates/skill-hyalo-tidy.md` — replace all 27 `--index .hyalo-index` with bare `--index`
- [x] Rewrite `crates/hyalo-cli/templates/skill-hyalo.md` — replace all 9 occurrences with the appropriate new form (bare `--index` for default-path examples, `--index-file=...` for explicit-path examples)
- [x] Audit `crates/hyalo-cli/templates/rule-knowledgebase.md` for any references

### Installed skills (checked into the repo)

- [x] Rewrite `.claude/skills/hyalo-tidy/SKILL.md` — mirror the template changes (27 occurrences)
- [x] Rewrite `.claude/skills/hyalo/SKILL.md` — mirror the template changes (9 occurrences)
- [x] Grep `.claude/skills/**/*.md` for any other `--index` references and update
- [x] Verify templates and installed copies stay in sync: if a regeneration command exists (`hyalo init` or similar), run it and diff; otherwise the two sets are edited in lockstep in this PR

### README and top-level docs

- [x] Rewrite all 7 `--index` occurrences in `README.md`
- [x] Grep the repo root (`*.md`) for any additional mentions

### Knowledgebase

- [x] Append a short "superseded by iter-118" note under the `--index=` mentions in:
  - `hyalo-knowledgebase/iterations/iteration-113-dogfood-v0120-fixes.md`
  - `hyalo-knowledgebase/iterations/iteration-115-dogfood-v0120-iter114-followup.md`
  - `hyalo-knowledgebase/iterations/done/iteration-47-snapshot-index.md`
  - `hyalo-knowledgebase/dogfood-results/*.md` (four files)
- [x] Do not rewrite the historical content; readers may need to see the prior surface

### CHANGELOG

- [x] Add a `### Breaking changes` entry with before/after migration example
- [x] Add a `### Changed` entry for the new `--index` boolean semantics and the new `--index-file` flag

### Quality gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace -q`
- [x] Build `target/release/hyalo`, then dogfood: run the tidy skill's Phase 2 and Phase 3 commands against `hyalo-knowledgebase/` with the new surface and confirm index usage (wall-clock should drop meaningfully vs. a `drop-index` baseline; if adding `--verbose` "using index: PATH" debug output is easy, include it for deterministic verification)

## Acceptance criteria

- [x] `--index` is a pure boolean flag; `--index-file PATH` / `--index-file=PATH` takes the explicit path
- [x] `hyalo lint --index file.md` parses cleanly: `file.md` is the lint target, `--index` is the toggle
- [x] `hyalo ... --index=anything` errors with clap's standard "unexpected value" message — no silent misses, no deprecation shim
- [x] The tidy skill's commands use the index (verified via dogfood timing or debug log)
- [x] No `require_equals` remains in the crate for index-related flags
- [x] `--index` and `--index-file` are **no longer global** — they appear only on subcommands that actually consume the snapshot index (see the Design matrix). `hyalo create-index --help` and `hyalo drop-index --help` do not list them.
- [x] Doc comment on `args.rs` drops the three-paragraph workaround note; help output is one paragraph per flag
- [x] **Every help text, README entry, skill template, installed skill, and CHANGELOG note is updated in the same PR** — nothing references the old `--index=PATH` surface except the historical knowledgebase notes, which are annotated rather than rewritten
- [x] Zero regressions in existing e2e tests; new tests cover both flags
- [x] No clippy warnings

## Out of scope

- Environment-variable fallback (`HYALO_INDEX_FILE`)
- Auto-detecting non-default index locations
- Changes to the snapshot index format or staleness detection
- Adding a dedicated "was the index actually used" metric to command output
  (nice-to-have; track separately if we want it)

## References

- Original `require_equals` rationale: commit `b20d1f9` during iter-105
- [[iterations/iteration-105-summary-redesign]]
- [[iterations/done/iteration-47-snapshot-index]] — the snapshot index itself
- `.claude/skills/hyalo-tidy/SKILL.md` — primary (broken) consumer of the space form
- `.claude/skills/hyalo/SKILL.md` — secondary consumer
- `crates/hyalo-cli/templates/skill-hyalo-tidy.md` / `skill-hyalo.md` — repo templates
- `crates/hyalo-cli/src/cli/args.rs:135–157` — current flag definition
- `crates/hyalo-cli/tests/e2e/index.rs:596, 633, 656, 1431–1439` — test call sites to update
- `README.md` — 7 user-facing mentions
