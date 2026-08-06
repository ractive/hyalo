---
title: Full-codebase review 2026-08-06 — Opus 5 deep review (write path, deps, consistency)
type: review
date: 2026-08-06
status: active
related:
  - "[[reviews/codebase-review-2026-07-10]]"
  - "[[reviews/link-handling-review-2026-07-18]]"
---

# Full-codebase review 2026-08-06 — Opus 5

Deep review of hyalo v0.20.0 (129k LOC, 4 crates) run with Opus 5.
Round 1 covered correctness / safety / dependencies. Round 2 (below) covers
CLI surface consistency: help text, hints, command names, flags.

Baseline state at review time — all green, so every finding below is
something the existing gates do **not** catch:

- `cargo clippy --workspace --all-targets -- -D warnings` — clean (full
  pedantic group enabled)
- `cargo test --workspace -q` — all pass
- edition 2024 throughout; only two `unsafe` blocks, both explicit-block
  form with `// SAFETY:` comments

## Assessment

The hard parts are right. `apply_body_fixes` conflict resolution, the
batch-`mv` rollback (DEC-056 / L-11), `line_col_to_byte`'s byte-vs-char
column discipline, and the snapshot loader's SEC-1/2/3 + MED-1 validation
all hold up under adversarial reading and carry comments explaining *why*.

The findings concentrate in one place: **the file-write layer**.
`fs_util::atomic_write` is 30 lines that every mutation path funnels
through, and two defects there were confirmed by experiment.

## Round 1 findings

### R1-1 (critical) — mutating a symlinked note destroys the link and discards the edit

`crates/hyalo-core/src/fs_util.rs:41`

`tmp.persist(path)` is a `rename(2)` onto `path`. When `path` is a symlink,
rename replaces **the symlink itself**, not its target. Reproduced:

```text
vault/alias.md -> sub/real.md          # symlink, 11 bytes
$ hyalo task toggle alias.md --line 5
  -> {"status": "x", "text": "task one"}   # reports success, exit 0

vault/alias.md                          # now a REGULAR FILE, 33 bytes
vault/sub/real.md                       # UNCHANGED - still "- [ ] task one"
```

The edit went to a new divergent copy; the real note was never touched; the
exit code was 0. Affects every mutation path — `set`, `remove`, `append`,
`task`, `lint --fix`, `mv`, `okf`, `changelog`, `link_rewrite` all call
`atomic_write`.

The vault-boundary check *does* correctly reject symlinks pointing outside
the vault (verified: errors with `file resolves outside vault boundary`).
It is the intra-vault case that is unguarded — and intra-vault aliasing is
exactly what Obsidian users do. This repo does it too
(`.claude/rules/knowledgebase.md` is symlinked to the crate `templates/`).

Fix: resolve `path` through `read_link`/`canonicalize` before choosing the
rename target so the write lands on the link's target, **or** detect
`symlink_metadata(path)?.file_type().is_symlink()` and refuse with a clear
error. Either is defensible; silently converting the link is not. No test
covers symlinked targets.

### R1-2 (critical) — `atomic_write` never fsyncs, so its own doc comment is false

`crates/hyalo-core/src/fs_util.rs:17-51`

The doc comment claims "a crash mid-write never leaves a truncated or
corrupted file". True for a *process* crash; false for system crash or
power loss. `write_all` only reaches the page cache and
`NamedTempFile::persist` renames without syncing, so the rename metadata
can reach disk before the data blocks — leaving a zero-length or garbage
file where a good file used to be. `hyalo lint --fix` over a vault turns
one power loss into hundreds of at-risk files.

Fix: `tmp.as_file().sync_all()` before `persist`; on Unix also fsync the
parent directory afterwards so the rename itself is durable. Then the doc
comment is accurate.

Same gap exists in `write_snapshot` (`index.rs:919`) but is **benign**
there — a torn snapshot fails `rmp_serde::from_slice` and falls back to a
disk scan, which is self-healing. Do not "fix" the index path as if it were
equally urgent.

### R1-3 (critical) — dead `pub` task mutators missing the mtime guard

`crates/hyalo-core/src/tasks.rs:481` (`toggle_task`), `:521` (`set_task_status`)

Both are `pub` in a published crate with **zero callers** in the workspace —
the CLI only uses the plural `toggle_tasks` / `set_tasks_status`. The plural
forms take `read_mtime` up front and `check_mtime` immediately before
writing (`:597`, `:637`). The singular forms do neither: stat, read, mutate,
`atomic_write`, no concurrent-modification check. Stale copies that missed
the hardening pass.

Delete them, or add the guard. An unguarded, untested, uncalled clobber path
in `hyalo-core`'s public API invites a future caller to pick the wrong one.

### R1-4 — read-only commands write into the user's vault, every invocation, uncached

`crates/hyalo-core/src/case_index.rs:188-242`, `:253`

`CaseInsensitiveMode::Auto` is the default (`config.rs:273`) and
`mode_enabled` resolves it via `probe_case_insensitive`, which creates a
real file in the vault root, stats its uppercase variant, and unlinks it.
Verified — `hyalo find` bumps the vault directory mtime:

```text
1786047046  /tmp/hyalo-probe
$ hyalo find --dir /tmp/hyalo-probe    # read-only command
1786047047  /tmp/hyalo-probe
```

Consequences:

- `find` / `summary` / `tags` are not read-only against the filesystem
- directory watchers (Obsidian sync, Dropbox, fswatch rebuilds) see churn
  on every invocation
- SIGKILL between `open(create_new)` (`:221`) and `remove_file` (`:235`)
  orphans a `.hyalo-case-probe-<hex>` in the vault with **no cleanup path**
  — unlike stale index files, which have `find_stale_indexes`. The name is
  dot-prefixed so discovery skips it: `hyalo find` cannot see the litter it
  left, but `git status` will
- no caching — `mode_enabled` is called at seven sites
  (`dispatch.rs:122,1351,1386,1978,2404`, `set.rs:385`, `types.rs:549`), so
  one command can probe repeatedly, three syscalls each, painful over
  NFS/SMB
- the fallback is **semantic, not cosmetic**: on a read-only
  case-insensitive mount the probe fails and case-insensitive link
  resolution silently turns off

Fix (stack both): cache per-run in a `OnceLock` keyed by directory, and
prefer a probe that does not write at all — stat the vault directory itself
under a case-flipped final component, or stat an already-discovered file
under a flipped name.

### R1-5 — `mdbook-lint-core` drags in the entire `mdbook` crate, which it never uses

`crates/hyalo-mdlint/Cargo.toml:19`

`mdbook` v0.4.52 accounts for **82 of hyalo's 168 transitive crates** —
roughly half the tree. It arrives via a hard, non-optional dependency in
`mdbook-lint-core`'s manifest. Grepping that crate's entire source for any
reference to the `mdbook` crate returns **nothing** — it is an unused
dependency upstream. (`mdbook-lint-rulesets`' `pub use
mdbook::MdBookRuleProvider` is its own internal module of that name, and
hyalo uses `StandardRuleProvider` anyway.)

What hyalo pays: `handlebars`, `pest` + `pest_derive` (a PEG generator
running at build time), `sha2`, `chrono` + `jiff`, `env_logger`, a second
full `clap`, `toml` **0.5.11**, and `opener` — a crate whose job is
launching the user's web browser, linked into a markdown frontmatter CLI.
It is also the sole reason `deny.toml:33-38` needs a scoped MPL-2.0
exception, and the source of the `toml` / `darling` / `derive_builder` /
`thiserror` duplicate versions.

The MPL question was clearly considered; the tree size was not. Highest-
leverage action in this review: upstream PR to
`joshrotenberg/mdbook-lint` making the `mdbook` dep optional or dropping
it. That one change halves the build, removes the license exception, and
clears most of the duplicate-version list.

### R1-6 — `apply_body_fixes` indexes with unvalidated offsets

`crates/hyalo-cli/src/commands/lint.rs:3087,3092`

`result[start..end]` and `replace_range(start..end, ..)` are guarded only by
`end > body.len()` (`:3067`). Nothing checks `start <= end` or that either
offset is a UTF-8 char boundary — both panic conditions, mid-`--fix` over a
user's vault.

Currently safe, but by a non-local argument: offsets trace back to
`line_col_to_byte`, which walks `char_indices()` and can only return
boundaries. The exception is `trim_md034_liquid` (`engine.rs:819-820`),
which derives `new_end = end - (inner.len() - cut)` by pure arithmetic on
the *replacement* string's length — valid only while upstream MD034's
replacement stays byte-identical to the source slice. `DiagFix` also has
public fields, so the invariant is not type-enforced.

Fix: `result.get(start..end)`, treat `None` as `Conflict`/skip, matching how
`convert_fix` already drops unconvertible fixes rather than trusting them.

### R1-7 — dead no-op loop with a self-contradicting comment

`crates/hyalo-cli/src/commands/lint.rs:1689-1694`

```rust
for (full_path, rel_path) in files {
    // ... "Actually the frontmatter pass writes inline - we need to patch for all modified."
    let _ = (full_path, rel_path); // covered by per_file above
}
```

Leftover scaffolding; the `let _ =` is what keeps clippy quiet. The comment
contradicts itself mid-sentence and reads as an unfinished TODO to anyone
auditing the `--fix` index-patching path. Delete it.

### R1-8 — ~40 `.unwrap()` calls contradict the project's own rule

`crates/hyalo-cli/src/commands/init.rs` (`:137,147,168,177,179,214,...`)

CLAUDE.md states "No `.unwrap()` / `.expect()` outside of tests". These are
all `writeln!(summary, ...)` into a `String`, where `fmt::Write` is
genuinely infallible — harmless, but they are the bulk of the rule's
violations and they train the eye to skim past `.unwrap()` in non-test
code. `let _ = writeln!(...)` costs nothing and keeps the rule
grep-scannable.

## Round 1 observations (not tracked as work)

- The `.md`-stripping idiom (`Path::extension()` says "md" -> slice
  `&s[..s.len()-3]`) appears at seven sites and is boundary-safe
  everywhere, since an ASCII `.md` tail guarantees the split point. One
  cosmetic wrinkle: `discovery.rs:1362` guards on `!target.contains('/')`
  but not `'\\'`, so on Windows a target like `note.md\` yields stem
  `note.` and misses the lookup. Wrong answer, not a panic, pathological
  input only.
- `ensure_within_vault` uses `Path::starts_with`, which is component-wise —
  `/vault2` correctly fails a `/vault` prefix check. Easy to get wrong with
  a string comparison; right here.
- The `--fix` convergence loop (`lint.rs:2612-2658`) is bounded by
  `MAX_BODY_FIX_PASSES` **and** breaks on no-progress. Both, not just one.
- `deny.toml`'s two RUSTSEC ignores (`bincode` 1.x, `yaml-rust`) both come
  through `comrak -> syntect`. Re-check after any `mdbook-lint` bump; they
  may fall out on their own.
- Edition 2024: no findings. Both `unsafe` blocks
  (`broken_pipe.rs:47`, `index.rs:970`) are explicit blocks with `// SAFETY:`
  comments, not implicit `unsafe fn` bodies. No bare `#[no_mangle]`, no
  `static mut` refs, no bare `extern`, no `dyn`-less trait objects.
  Let-chains used idiomatically rather than nested `if let`.

## Round 2 findings — CLI surface consistency

Help text, hints, command names, flags, and whether the code agrees with any
of them. Everything below was verified by running the binary, not by reading
help strings.

### R2-1 — 8 of 25 commands are missing from COMMAND REFERENCE

`hyalo --help` says "See COMMAND REFERENCE below for full syntax of each
command." It then omits: **`changelog`, `config`, `lint`, `lint-rules`,
`madr`, `new`, `okf`, `types`**.

That includes `lint` — arguably the flagship feature — and `config`, which
`.claude/CLAUDE.md` explicitly instructs agents to use. The
`Commands:` block lists 25; COMMAND REFERENCE covers 17.

### R2-2 — four mutually inconsistent enumerations of "list commands", none correct

Commands that actually emit `total` (verified):
**`find`, `tags summary`, `properties summary`, `backlinks`, `lint`,
`views list`, `types list`.**

What the help says, in four places:

| Location | Claims | Wrong how |
|---|---|---|
| OUTPUT paragraph | find, tags, properties, backlinks | missing lint, views, types |
| `--count` flag help | find, tags summary, properties summary, backlinks | missing lint, views, types |
| "Default output limits" | find, lint, tags summary, properties summary, backlinks | missing views, types |
| OUTPUT SHAPES | "find, tags summary, properties summary, backlinks; **omitted elsewhere**" | actively false — an exclusivity claim |

The `--count` *runtime error message* has a fifth list ("find, tags summary,
properties summary, backlinks, lint") which is closer but still wrong:
`hyalo views list --count` returns 7 and `hyalo types list --count` returns
6, so the error asserts an exclusivity that the code does not enforce.

One shared constant should generate all of these.

### R2-3 — the global-flags reference block contradicts the same help's Options block

```text
--format json|text      Output format (default: json, override via .hyalo.toml)
```

Two errors in one line:

- **default is not json.** Both the OUTPUT paragraph and the `--format`
  Options entry — in the same `--help` output — say "text" when stdout is a
  terminal, "json" when piped.
- **`github` is missing.** It is a real `--format` value (lint-only, emits
  GitHub Actions annotations) and appears in Options' possible-values.

The block also omits `--index-file`, which is a documented global option.

### R2-4 — subcommand verb vocabularies do not transfer between groups

| Group | Verbs |
|---|---|
| `properties`, `tags` | `summary`, `rename` |
| `types` | `list`, `show`, `set`, `remove` |
| `views` | `list`, `set`, `remove`, `run` |
| `lint-rules` | `list`, `show`, `set`, `remove` |

"Show me all of them" is `summary` in two groups and `list` in three.
Verified non-transferable:

```bash
$ hyalo tags list       -> error: unrecognized subcommand 'list'
$ hyalo types summary   -> error: unrecognized subcommand 'summary'
```

A user who learns `hyalo types list` will guess `hyalo tags list` and fail.
Cheapest fix that breaks nothing: add `list` as an alias on
`properties`/`tags` and `summary` as an alias on `types`/`views`/`lint-rules`.

### R2-5 — `mv` has opposite safety defaults depending on which selector you use

From `hyalo mv --help`, verbatim:

- SINGLE-FILE MODE: "Applied immediately unless `--dry-run` is passed."
- BATCH MODE: "Defaults to dry-run; pass `--apply` to commit changes."

Same command, inverted default, selected by whether you passed `--file` or
`--glob`/`--property`/`--tag`/`--type`. Both flags are accepted in both
modes as silent no-ops (verified: `--apply` in single-file mode writes, as
it would have anyway; `--dry-run` in batch mode previews, as it would have
anyway).

The failure mode: a user who learns the batch form (safe preview, `--apply`
to commit) reasonably expects `hyalo mv --file a.md --to b.md` to preview
too. It writes. `--apply` being *accepted* in single-file mode actively
reinforces the wrong mental model.

Related, the wider inconsistency across mutating commands: `links fix`
defaults to dry-run + `--apply`; `lint` uses `--fix` plus `--fix --dry-run`;
`mv` uses both conventions at once. Three vocabularies for one concept.

### R2-6 — `hyalo tags summary` emits a hint that errors

`hints.rs:1439` builds `["tags", "--limit", "0"]` — missing the `summary`
subcommand. See dogfood BUG-3 for the reproduction and for the 29-hint
executability sweep (28 pass). The existing test asserts
`show_all.cmd.contains("--limit 0")`, which the broken command satisfies;
there is no execution-based hint gate.

Note `properties summary` emits **no** "show all" hint at all, so the two
symmetric commands are asymmetric: one has a broken hint, the other is
missing it.

### R2-7 — documented syntax is not the recommended syntax for `read`/`backlinks`

COMMAND REFERENCE and every COOKBOOK example document only the flag form:

```text
hyalo read -f/--file F ...
hyalo backlinks -f/--file F [-n/--limit N]
```

Every hint the tool emits uses the positional form
(`hyalo read decision-log.md`, `hyalo backlinks decision-log.md`). Both work
(verified, exit 0). The positional form is the preferred one — it should
appear in the reference, not only in hints.

### R2-8 — `hyalo config` breaks the JSON envelope contract

Full detail in dogfood BUG-4: no `results`/`total` keys, `hints` is a bool
rather than an array, and `--jq` is silently ignored while `--count`
correctly errors. The help's "All JSON is wrapped in a consistent envelope"
has exactly one exception and it is the command agents are told to use for
debugging.

## Round 2 observations

- `create-index` uses `-o/--output` while `drop-index` uses `-p/--path` for
  the same file. Minor, but they are a matched pair.
- `find -n` is `--limit`; `summary -n` is `--recent`. Same short flag, two
  meanings.
- `links fix` / `links auto` — a verb and an adjective as sibling
  subcommands.
