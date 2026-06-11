---
title: Full-codebase review 2026-06-11 — architecture, Rust, security, deep logic
type: review
date: 2026-06-11
tags:
  - review
  - security
  - rust
  - audit
status: completed
---

# Full-codebase review — hyalo 0.16.0 (2026-06-11)

Read-only audit of the whole workspace (`hyalo-core`, `hyalo-cli`, `hyalo-mdlint`,
`xtask`; ~98k lines). Run as a fan-out of 87 agents: one explorer per crate, ~19
finder dimensions (Rust best practices, security, deep logic), and an adversarial
verifier per finding. Every finding below was re-checked by an independent agent
that re-read the cited code or reproduced it via the CLI; 10 of 63 raw findings were
dropped as mischaracterizations and are not listed. The two CRITICALs and the
lint-`--fix` data-loss items were additionally reproduced first-hand during synthesis.

## Summary

The codebase is in good shape overall. Prior audits (iter-92, iter-122, iter-125)
hold: the YAML parser budget, BM25 `doc_id` bounds checks, MessagePack size caps,
regex `size_limit`, `atomic_write` + mtime guards on `set`/`remove`/`append`/`task`/
`mv`/`links`, and `ensure_within_vault` on read paths are all present and working.
The findings cluster in three places that escaped that lineage: **(1)** the
frontmatter open-delimiter check disagrees between the read path and the write path,
so a BOM or a leading space silently duplicates the frontmatter block and demotes the
original to body (data loss); **(2)** `hyalo-mdlint`'s line/column-to-byte conversion
is wrong for byte-vs-char columns and for whole-file rules, so `lint --fix` injects
blank lines, never converges, silently skips non-ASCII lines, and (in one path)
reverts a frontmatter fix it just made via a non-atomic write; **(3)** `lint` and
`read` bypass the scanner's size cap and read whole files into memory, and `mv` never
applies `ensure_within_vault` so a symlinked destination escapes the vault.

### Findings by severity

| Severity | Count | Headline items |
|----------|-------|----------------|
| Critical | 2 | BOM / leading-space frontmatter corruption on `set`/`remove`/`append` |
| High | 13 | mv symlink escape; lint/read no size cap; lint `--fix` non-atomic + reverts frontmatter; 3× mdlint fix bugs; fenced `--line` toggle; single-file `mv` frontmatter links; JSON error envelope; BM25 mode divergence; `--index` link-graph corruption |
| Medium | 14 | lint `--fix` not idempotent; MD009 CRLF; dry-run divergence; schema severity mislabel; `read` terminal-escape; no size cap on `task`/`set`; mtime guard missing in lint fix; 3 delimiter predicates; help-vs-behavior mismatches |
| Low | 19 | snapshot has no format version; non-atomic `.hyalo.toml` writes; schema regex no size_limit; deserialization peak-memory amplification; dead code; help/behaviour nits; exit-code/escape nits |
| Nit | 5 | mdlint per-call allocations; sort-fallback alloc; dead config field |
| **Total** | **53** | |

---

## Phase 0 — Baseline gates

All green; no finding.

- `cargo build --release` — succeeds.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace -q` — all pass, 0 failures (incl. doctests).

---

## Phase 1 — Architecture & coherence

Layering is clean: `hyalo-core` is the library (discovery, scanner, frontmatter,
index, links, filter, bm25, schema), `hyalo-cli` is the command layer, `hyalo-mdlint`
hosts lint rules, `xtask` is dev tooling. Core logic does not leak into the CLI in any
material way. The coherence dimension produced 15 raw findings; 7 confirmed, 8 dropped
(the "1600-line dispatch match", "duplicate case-index helpers", and "scattered BUG
comments" were judged stylistic non-defects on re-read). The confirmed structural
items are all low/nit:

- **LOW — Snapshot index has no format version.** `crates/hyalo-core/src/index.rs:259`
  (`SnapshotHeader`) carries no `format_version`/`schema_version`. A future change to
  `IndexEntry`/`LinkGraph` would let an old snapshot deserialize into wrong-but-valid
  data instead of being rejected. Add a version field and reject mismatches (fall back
  to disk scan).
- **LOW — `ResolutionPolicy::Multi` is dead in production.** `crates/hyalo-cli/src/commands/inputs.rs:44`
  is `#[allow(dead_code)]`, only used in tests; dispatch only ever uses `Single`/`SingleOrMany`.
- **LOW — `HintContext` has 30+ fields** (`crates/hyalo-cli/src/hints.rs:81`) coupling
  the hints module to every command shape; brittle as commands are added.
- **LOW — `xtask` hardcodes the subcommand list** (`crates/xtask/src/help_drift.rs:19`);
  a new command added to `args.rs` is silently skipped by the help-drift check.
- **LOW — Snapshot validation failure is invisible to scripts** (`crates/hyalo-cli/src/run.rs:1196`):
  a `vault_dir`/`site_prefix` mismatch warns to stderr and falls back to disk scan but
  still exits 0, so a machine consumer cannot tell the index was ignored.
- **NIT — `ConfigFile::views` is read via raw TOML, not the typed config** (`crates/hyalo-cli/src/config.rs:83`).

Cross-cutting coherence is generally consistent (shared `mutation.rs`, `output.rs`,
`format_error`), with one real exception that produces several high/medium findings:
**error output does not uniformly honour `--format json`** — see L-CLI-1/L-CLI-2 and
M-CLI-1 in Phase 4. That is the single biggest coherence gap.

---

## Phase 2 — Rust best practices

No `unwrap`/`expect` violations in production code (the audit confirmed iter-92's fixes
hold; the one `unsafe` block at `index.rs:855` has a proper `// SAFETY:` comment).
Edition 2024 compliance is clean. Findings are all low/nit performance items:

- **LOW — Unconditional clone in link-graph hot path.** `crates/hyalo-core/src/link_graph.rs:416`
  clones `BacklinkEntry` even when `resolved_key` is `None` and the clone is unused.
  Runs per link per file during graph construction.
- **LOW — Duplicate posting-list intersection in BM25 phrase matching.**
  `crates/hyalo-core/src/bm25.rs:693` recomputes `docs_with_all_terms(terms)` already
  computed at line 680; reuse the intermediate.
- **LOW — Copy-pasted multi-file task dispatch.** `crates/hyalo-cli/src/dispatch.rs:911`
  (`TaskAction::Toggle`) and ~1019 (`TaskAction::Set`) are near-identical loops; extract
  a shared helper.
- **NIT — Sort fallback stringifies both sides** (`crates/hyalo-core/src/filter/sort.rs:117`);
  `Cow` could avoid one alloc (cold path).
- **NIT — mdlint recreates `Document` up to 3× per file** (`crates/hyalo-mdlint/src/engine.rs:380,409,431`)
  and rebuilds the 17-entry severity/default maps on every `lint_body` call
  (`engine.rs:327`). Build once per engine. (Note: the verifier confirmed the AST is
  *not* shared across stock vs HYALO rules, but the rebuild cost is real.)

---

## Phase 3 — Security

Most of the attack surface is well-hardened and was verified clean:

- **YAML frontmatter (serde-saphyr):** tight `Budget` (`max_aliases:0`, `max_depth:20`,
  `max_total_scalar_bytes:8192`, `DuplicateKeyPolicy::Error`, strict booleans) plus
  64 KiB / 2000-line pre-read caps. Alias bombs and deep nesting are rejected.
- **Regex (find -e, `~=`, `--title`, `--section`):** every user-facing regex compiles
  with `size_limit(1<<20)`; `a{999999999}` and giant alternations are rejected cleanly;
  the `regex` crate is backtracking-free. (One inconsistency below.)
- **Path traversal on read/existing-write paths:** `resolve_file` rejects `..`,
  absolute, null-byte, and symlink-escape paths then `ensure_within_vault`s; there is a
  dedicated `resolve_file_rejects_symlink_escape` test.
- **No shell-out / env expansion** in shipped code (only build-time git in `build.rs`).
- **TOCTOU:** `atomic_write` is correct (temp + persist in same dir); the read-mtime →
  check-mtime → atomic-write pattern holds on `set`/`remove`/`append`/`task`/`mv`/`links`.

Confirmed findings:

### HIGH — `hyalo mv` escapes the vault through a symlinked destination directory
**File:** `crates/hyalo-cli/src/commands/mv.rs:738` (also `:531` batch)
`validate_target_single`/`validate_batch_target` reject literal `..`/absolute components
via `Path::components()` but never resolve symlinks or call `ensure_within_vault`. If a
directory component of the destination is an in-vault symlink pointing outside,
`dir.join(new_rel)` resolves outside and `create_dir_all` + `fs::rename` move the file
out (and can fabricate directory trees outside the vault). The read path and the
inbound-link-rewrite write path both do `ensure_within_vault`; `mv` was simply never
given the same treatment.
**Repro:** `d=$(mktemp -d); mkdir -p "$d/vault" "$d/outside"; printf -- '---\ntitle: src\n---\nbody\n' > "$d/vault/note.md"; ln -s ../outside "$d/vault/escapelink"; hyalo --dir "$d/vault" mv --file note.md --to 'escapelink/escaped.md'; ls -la "$d/outside"` → file lands outside the vault, exit 0, no warning.
**Fix:** canonicalize the destination's nearest existing ancestor and `ensure_within_vault` before any fs mutation, in both `execute_mv` and `execute_batch_mv`.

### HIGH — `lint` (read-only and `--fix`) reads whole files with no size cap, in parallel
**File:** `crates/hyalo-cli/src/commands/lint.rs:1872`
`lint_one_file_extended` calls `std::fs::read_to_string` with no `MAX_FILE_SIZE` gate,
dispatched via `files.par_iter()`, so peak RSS scales as `threads × file_size` — the
scanner's 100 MiB cap (`scanner/mod.rs:37`) is bypassed entirely. Measured: 8×200 MiB
files → ~1.68 GiB RSS; a single 591 MiB file is read with no warning.
**Fix:** stat-and-skip on `scanner::MAX_FILE_SIZE` before `read_to_string` (mirror the
scanner's stderr warning), or route through the streaming scanner.

### HIGH — `read` buffers the whole body into `Vec<String>` with no size or per-line cap
**File:** `crates/hyalo-cli/src/commands/read.rs:160`
`read_body_lines` streams with `BufReader` but pushes every line into a `Vec`, with no
`MAX_FILE_SIZE` gate and no `MAX_BODY_LINE_BYTES` analog. A newline-free 591 MiB blob →
~2.7 GiB RSS. The `find` hint output even suggests `hyalo read <file>` for files it just
skipped as oversized, steering users into the unbounded path.
**Fix:** stat-and-skip on `MAX_FILE_SIZE`; replace `reader.lines()` with the scanner's
`read_line_capped`.

### HIGH — `lint --fix` body write is non-atomic and reverts the frontmatter fix from the same run
**File:** `crates/hyalo-cli/src/commands/lint.rs:2183`
This is the only mutating path that uses non-atomic `std::fs::write` and has no mtime
guard. Two defects: **(1)** `content`/`body_start` are captured from the original bytes
at line ~1871; if a frontmatter fix is written first (atomic, line 2052), the body
branch then overwrites the whole file using the *stale* pre-fix frontmatter slice,
silently reverting the just-applied frontmatter fix (verified: a `type: note` file
missing a defaulted `status` plus a body MD009 violation ends with `status` absent).
**(2)** `std::fs::write` truncates in place — a crash mid-write corrupts the document.
**Fix:** reconstruct from post-frontmatter-fix on-disk content and route through
`fs_util::atomic_write`; ideally do a single combined frontmatter+body write.

### MEDIUM — Resource caps missing on more mutation paths
- `crates/hyalo-core/src/tasks.rs:437` — `toggle_task`/`set_task_status`/`toggle_tasks`/`set_tasks_status` all start with `std::fs::read_to_string` (no `MAX_FILE_SIZE`), then split + rebuild (~2× resident).
- `crates/hyalo-core/src/frontmatter/parse.rs:262` — `write_frontmatter` reads the full body with `read_to_end` (no cap) and copies it again before `atomic_write`.

### MEDIUM — `lint --fix` frontmatter write has no TOCTOU mtime guard
**File:** `crates/hyalo-cli/src/commands/lint.rs:2052` — the write is atomic but, unlike
`set.rs:415`/`append.rs:295`/`remove.rs:282`/`tasks.rs`, there is no `read_mtime`/`check_mtime`,
so a concurrent edit between the read (line ~1871) and write is silently lost.

### MEDIUM — `hyalo read` text output is not control-byte sanitized
**File:** `crates/hyalo-cli/src/output_pipeline.rs:175` (also `run.rs:561/571/599`)
iter-122 sanitized frontmatter values and filenames in *structured* text output, but
`read`'s `RawOutput` body (`commands/read.rs:339`) is `println!`'d raw. A `.md` body with
ANSI escapes injects them into the terminal. The post-iter-122 dogfood note already
flagged this class.

### LOW — Security nits
- **Deserialization peak-memory amplification** (`crates/hyalo-core/src/index.rs:502`):
  `rmp_serde::from_slice` fully materializes `entries`/`graph`/`bm25_index` *before* the
  iter-122 caps run, so a ≤512 MiB crafted `.hyalo-index` can transiently allocate
  ~2.6 GiB. The caps are post-deserialization; bound during decode or lower the file cap.
- **Schema regexes compiled without `size_limit`** (`crates/hyalo-cli/src/commands/lint.rs:1004` and `:1151`):
  every other regex site uses `size_limit(1<<20)`; the two schema-`pattern`/`item_pattern`
  sites use bare `Regex::new`, inconsistent and unbounded (project-config input, lower risk).
- **Non-atomic `.hyalo.toml` writes** (`crates/hyalo-cli/src/commands/lint_rules.rs:338,480`,
  `types.rs:578`, `views.rs:199`): read-modify-write config with truncating `std::fs::write`;
  a crash mid-write can lose all type schemas / rule overrides / views.
- **Single-file `mv` clobber window** (`crates/hyalo-cli/src/commands/mv.rs:738`):
  `target_path.exists()` (line ~644) is far from `fs::rename` (line 738); on Unix rename
  unconditionally clobbers a destination created in the window, and the mtime guard only
  protects the source.
- **Pre-pipeline error paths print unsanitized user args** (`crates/hyalo-cli/src/run.rs:340,347`):
  `AppError::User`/`Internal` are `eprintln!`'d without `sanitize_control_chars`, and some
  embed raw `--dir`/argument bytes.
- **Maintainer absolute paths in 9 tracked KB files** (e.g. `hyalo-knowledgebase/dogfood-results/dogfood-v0160-iter-144-147.md:19`):
  `/Users/james/devel/...` paths committed to the public repo — the same info-disclosure
  class iter-122 MED-2 cleaned up; new dogfood reports reintroduced it. Replace with placeholders.

---

## Phase 4 — Deep logic review

### CRITICAL — BOM-prefixed file is corrupted by `set`/`remove`/`append`
**File:** `crates/hyalo-core/src/frontmatter/parse.rs:375` (and `:462`)
A leading UTF-8 BOM defeats every delimiter check. `read_frontmatter_from_reader`
(`:462`, `line.trim() == "---"`) returns an empty property map; `find_body_offset`
(`:375`, `line.trim_end_matches(['\n','\r']) != "---"`) returns `body_offset = 0`. `set`
then inserts into the empty map and `write_frontmatter` **prepends a brand-new `---`
block on top of the entire original file** — the original `title`/`status` is demoted to
body content and is no longer queryable. Reports success (exit 0). Affects all three
commands; `remove` is doubly wrong (reports a property removed while it survives in the
buried block). The codebase already strips BOMs in `files_from.rs:43`.
**Repro (reproduced first-hand):**
```
d=$(mktemp -d); printf '\xEF\xBB\xBF---\ntitle: Note\nstatus: draft\n---\nBody.\n' > "$d/bom.md"
hyalo --dir "$d" set bom.md --property status=done
# file now begins '---\nstatus: done\n---\n\357\273\277---\ntitle: Note...': two frontmatter blocks
```
**Fix:** strip a leading BOM once at the start of all three entry points and reconcile
`find_body_offset` with `read_frontmatter_from_reader` so reader and writer agree.

### CRITICAL — Leading whitespace before `---` corrupts the file
**File:** `crates/hyalo-core/src/frontmatter/parse.rs:462`
Same root cause: the read path (`line.trim() == "---"`) accepts ` ---` (leading space)
and loads the real properties; `find_body_offset` (trailing-only trim) rejects it and
returns `body_offset = 0`. The write then duplicates the frontmatter into the body.
**Repro (reproduced first-hand):**
```
d=$(mktemp -d); printf ' ---\ntitle: Note\nstatus: draft\n---\nBody.\n' > "$d/lead.md"
hyalo --dir "$d" set lead.md --property status=published
# prepends '---\ntitle: Note\nstatus: published\n---' above the original ' ---' block
```
**Fix:** make `find_body_offset` use the identical opening-delimiter predicate as the
reader; the two must compute consistent offsets.

> Both criticals share one root cause: **three different opening-`---` predicates**
> (`extract_frontmatter` uses `starts_with("---")`, the reader uses `trim()`,
> `find_body_offset` uses trailing-only trim — `parse.rs:517/462/375`, logged separately
> as MEDIUM). Reconciling them fixes both criticals.

### HIGH — mdlint `line_col_to_byte` is wrong, breaking three `--fix` behaviours
The stock rules and HYALO001 emit **byte** columns (`line.len()`), but
`crates/hyalo-mdlint/src/engine.rs:474` advances `cur_col += 1` per **char**. Three
distinct confirmed bugs result (all reproduced first-hand):

- **MD009 autofix injects a spurious blank line** (`engine.rs:460`). End col `line.len()+1`
  maps to the byte offset *of the newline itself*, so the `[start,end)` range omits the
  `\n`; the replacement re-adds `\n` and the original survives. `only line   \nsecond` →
  `only line\n\nsecond`. Corrupts structure on every MD009 fix. (On CRLF files this also
  produces mixed line endings — logged as MEDIUM.)
- **Autofix silently dropped for any non-ASCII line** (`engine.rs:474`). For a line with
  multibyte UTF-8, byte-col > char-col, the loop walks past the line and returns `None`,
  so `convert_fix` drops the fix. The violation is reported but `--fix` does nothing —
  bare `[]` checkboxes / trailing spaces on any `café`/CJK/emoji line are never fixed.
- **MD047 never converges** (`crates/hyalo-cli/src/commands/lint.rs:2279`). MD047's fix
  is computed against the *full* document upstream but hyalo feeds it body-only text, so
  the byte range is wrong; each run reports `total_fixed=1` while the file stabilises at
  two trailing newlines that MD047 keeps re-flagging. The user can never get a clean lint.

**Fix:** make `line_col_to_byte` advance by `ch.len_utf8()` (treat columns as byte
positions), treat end col `len+1` as past-the-newline, and run whole-file rules (MD047)
against full content.

### HIGH — `task toggle/set --line N` mutates checkbox lines inside code fences
**File:** `crates/hyalo-core/src/tasks.rs:514` (and `:561`)
`toggle_tasks`/`set_tasks_status` resolve `--line N` by indexing `file_lines[line-1]` and
validating only with `detect_task_checkbox`; they never track fenced/`%%`-comment state.
Every *other* task path is fence-aware (`read_task` runs a `FenceTracker`; `--all`/`--section`
go through the scanner). So `task toggle f --line N` flips a `- [ ]` line inside a ```code```
fence, contradicting the documented "Skips … fenced code blocks" and the existing
`task_read_inside_code_block_exits_1` test (which guards the *read* path only).
**Repro (verified):** a `--line 10` pointing inside a fence → `task read` exits 1
(correct) but `task toggle` exits 0 and mutates the fenced line.
**Fix:** resolve valid task lines via the scanner pass and reject `--line` inside a
fence/comment with the existing "line N is not a task" error.

### HIGH — Single-file `mv` leaves dangling frontmatter wikilinks
**File:** `crates/hyalo-core/src/link_rewrite.rs:184`
`plan_mv` (used by `hyalo mv --file X --to Y`) calls `plan_inbound_rewrites`, which skips
the whole frontmatter block; the batch path uses `plan_inbound_rewrites_with_fm`, which
rewrites wikilinks in frontmatter link properties (`related`, `depends-on`, `supersedes`,
`superseded-by`). So a single-file move leaves every inbound frontmatter wikilink pointing
at the old (now-missing) path — links hyalo's own `backlinks` tracks. The project's own
iteration files use `related:` heavily, so dogfooding `mv` corrupts cross-references.
**Fix:** make `plan_mv` call `plan_inbound_rewrites_with_fm`; add a single-file e2e test
mirroring the batch `t12_frontmatter_wikilink_rewrite`.

### HIGH — `find` argument-validation errors are not JSON under `--format json`
**File:** `crates/hyalo-cli/src/dispatch.rs:481` (also 474/489/496/508/514)
`find`'s sub-parser errors return `UserError(format!("Error: {e}"))` — raw text — and the
pipeline only re-formats for `--format text`, so under JSON (and default piped mode,
which auto-selects JSON) a plain line goes to stderr. This breaks the documented
"All JSON is wrapped in a consistent envelope … `{\"error\": …}`" contract. Sibling
commands (e.g. `set --tag` via `format_error`) emit valid JSON for the identical error,
proving it's an inconsistency. Affects `--task`/`--fields`/`--sort`/`--section`/tag validation.
**Repro (verified):** `hyalo --dir kb find --task '???' --format json 2>err; jq . err` → parse error.

### HIGH — Same JSON-envelope bug across mutation/lint/mv/links dispatch
**File:** `crates/hyalo-cli/src/dispatch.rs:1121` (also 1159/1194/1287/1295/1318/1831/1855/1861/1868/1874/1885)
The same `UserError(format!("Error: …"))` anti-pattern repeats for `set`/`remove`/`append`
`--where-property` parsing, `lint` `--type`/filter parsing, `find`/`mv` tag validation,
and `mv`/`links` where-filter parsing. **Fix:** route every site through
`crate::output::format_error(format, …)`; add an e2e test asserting JSON validity on
invalid-filter error paths.

### HIGH — `set`/`append`/`remove`/`lint --fix` with `--index` corrupt the persisted link graph
**File:** `crates/hyalo-cli/src/commands/mutation.rs:32`
`update_index_entry` patches only `properties`/`tags`/`modified`; it never re-scans
wikilinks, so neither `entry.links` nor the snapshot `LinkGraph` is updated. Frontmatter
link properties (`related`, etc.) *do* feed the graph (`link_graph.rs:424`). So
`hyalo set --file X --property related='[[foo]]' --index` writes the file and re-saves
the snapshot, but the persisted backlink graph stays pre-mutation until a full
`create-index`. Violates the `IndexFlags` contract ("keep the index current"). `mv` and
`links fix` handle this correctly via re-scan.
**Repro (verified):** after the mutation, `backlinks foo.md --index` returns `[]` while
the live scan returns the correct backlink.
**Fix:** route `update_index_entry` through `refresh_entry` plus a link-graph update
(rebuild outbound edges), mirroring `rename_index_entry`.

### HIGH — BM25 ranking diverges between `--index` and default paths under a metadata filter
**File:** `crates/hyalo-cli/src/commands/find/mod.rs:450-465` vs `:318-341`
The default path builds a fresh BM25 corpus from only the metadata-passing candidates, so
IDF uses `N = candidates`; the persisted-index path computes IDF over the full vault then
intersects. With a `--property`/`--tag` filter + body query, the two paths produce
different scores and rankings; the default path silently degrades relevance (IDF over the
filtered subset is non-standard). Without a metadata filter they agree.
**Fix:** seed the candidate-only corpus with full-vault corpus statistics (N, df, avgdl),
or always build BM25 over all scoped entries and intersect after scoring.

### Medium logic findings
- **`lint --fix` is not idempotent** (`crates/hyalo-cli/src/commands/lint.rs:2146`).
  Fixes are computed once from the original body and applied as a batch; the engine never
  re-lints. Reproduced first-hand: a trailing-space line followed by a blank needs **3
  passes** to converge (MD009 injects a blank → MD012 fires next pass → fixed → clean).
  Root cause is the MD009 spurious-blank bug above.
- **Fix conflict resolution ignores severity** (`lint.rs:2239`). Overlapping fixes are
  applied by start-offset, first-wins; a Warn cosmetic fix (MD009) can beat an Error
  structural fix (HYALO001 `[]`→`- [ ]`) on the same line. Add a severity tiebreak.
- **`--dry-run` diverges from the real write for a fenced `--line` target**
  (`crates/hyalo-cli/src/commands/tasks.rs:185` vs `:235`): dry-run uses the fence-aware
  `find_task_lines` while the real toggle uses the fence-unaware `toggle_tasks`, so they
  give opposite answers for a `--line` inside a fence. (Resolved once the HIGH fence bug is fixed.)
- **Mixed-severity SCHEMA lint group reports the first violation's severity, not the max**
  (`crates/hyalo-cli/src/commands/lint.rs:1741`): a genuine schema error can be labelled
  `warn` in JSON/text output depending on violation order. Use the group max.
- **CRLF frontmatter silently converted to LF** (`crates/hyalo-core/src/frontmatter/parse.rs:285`):
  `write_frontmatter` re-serializes with LF and `b"---\n"` while the body keeps CRLF →
  a mixed-line-ending file (Windows-compat regression class).
- **MD009 autofix produces mixed line endings on CRLF files** (`crates/hyalo-mdlint/src/engine.rs:460`)
  — the `\n`-readded replacement next to a surviving `\r\n` yields `\n\r\n`.
- **`find --help` contradicts behaviour:** `--property title~=` is documented as
  "only searches frontmatter" but matches the H1-derived title too
  (`crates/hyalo-cli/src/cli/args.rs:439` vs `commands/find/mod.rs:536`).
- **Three divergent opening-delimiter predicates** (`crates/hyalo-core/src/frontmatter/parse.rs:517/462/375`)
  — the structural root cause of both criticals.

### Low / nit logic findings
- **`append` to a mapping property returns exit 2 (internal) for a user error**
  (`crates/hyalo-cli/src/commands/append.rs:103`) — should be a user-error exit code.
- **`find --help` calls `=~` a "common mistake / Wrong"** but `parse.rs:104` fully
  supports it as a regex alias with passing tests (`crates/hyalo-cli/src/cli/args.rs:438`).
- **`--sort score` works but is omitted from the `--sort` help enumeration**
  (`crates/hyalo-cli/src/cli/args.rs:294` vs `crates/hyalo-core/src/filter/sort.rs:32`).
- **YAML comments are destroyed on any mutation** (`crates/hyalo-core/src/frontmatter/parse.rs:269`)
  — inherent to the serde round-trip through `IndexMap<String, Value>`; document the
  limitation or move to a comment-preserving editor (`toml_edit`-style) for YAML. (LOW
  because it's lossy-but-not-corrupting and widely understood.)
- **iter-157 stem-map behaviour change** (`crates/hyalo-cli/src/dispatch.rs:62`):
  `build_case_index_from_snapshot` seeds the stem map from snapshot entries instead of a
  fresh disk walk, so a bare wikilink `[[deep]]` to a subdir file created after the
  snapshot is reported broken under `--index` (was resolved by the pre-iter-157 live
  walk). Consistent with the documented "`--index` trusts the snapshot" model, but a
  behaviour change worth a doc note.

> **Verified clean (not bugs):** block scalars, unicode in keys/values, key order, and
> quote-style normalization all round-trip correctly on `set`/`remove`/`append`;
> multiple `--property` is AND; `--tag` prefix matching is correct; `detect_task_checkbox`
> correctly rejects `[]` and accepts `[ ]`/`[x]`/`[X]`/`[-]`/`[?]`; `--section` substring
> matching is by-design (iter-36 decision); aliased/anchored/path-form wikilinks rewrite
> correctly; required-property validation and `--strict` promotion compose correctly;
> `--dry-run` is byte-faithful except where it faithfully inherits a broken apply;
> exit-code and path-traversal-rejection for the common cases are solid.

---

## Phase 5 — Verification method

Each finding was produced by a finder agent (schema-forced output with `file:line`,
severity, repro) and then handed to an independent adversarial verifier instructed to
refute it unless it could re-read the cited code or reproduce it via the binary against a
throwaway temp copy (the real KB was never mutated). 63 raw → 53 confirmed; the 10 dropped
were mischaracterizations (e.g. "1600-line dispatch match", "duplicate case-index helpers",
"HYALO rules parse AST twice") or self-declared non-issues. During synthesis the two
CRITICALs, the MD009 spurious-blank bug, and the non-idempotency cascade were reproduced
first-hand from the release binary.

---

## TLDR — top 5 to fix first

1. **Frontmatter delimiter disagreement → silent data loss** (both CRITICALs).
   Reconcile the three opening-`---` predicates and strip a leading BOM in
   `frontmatter/parse.rs`. A BOM or a leading space makes `set`/`remove`/`append`
   duplicate the frontmatter block and bury the original — reported as success.
2. **mdlint `line_col_to_byte` byte-vs-char bug.** One fix (advance by `len_utf8`,
   treat end col as past-the-newline) clears three HIGH `--fix` bugs: spurious blank
   lines (MD009), non-ASCII lines never fixed, and MD047 never converging.
3. **`lint --fix` body write is non-atomic and reverts the frontmatter fix.** Route
   through `atomic_write`, add the mtime guard, and reconstruct from post-fix content
   (`lint.rs:2052/2183`). This is the only mutation path missing the iter-122 guarantees.
4. **`mv` symlink escape + no size cap on `lint`/`read`.** Apply `ensure_within_vault`
   to mv destinations (`mv.rs`); stat-and-skip on `MAX_FILE_SIZE` in `lint`/`read`
   before reading whole files into memory.
5. **JSON error-envelope inconsistency.** Route all `dispatch.rs` `UserError(format!())`
   sites through `format_error` so `--format json` (and default piped mode) always emits
   valid JSON — scripts currently get plain text on any invalid filter.
