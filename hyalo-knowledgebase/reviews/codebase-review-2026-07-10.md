---
title: Full-codebase review 2026-07-10 — dogfood + parallel deep review + docs audit
type: review
date: 2026-07-10
status: active
tags: [review, security, correctness, dogfooding, docs]
related:
  - "[[reviews/codebase-review-2026-06-11]]"
  - "[[dogfood-results/dogfood-v0160-iter157-159-pr186]]"
---

# Full-codebase review — hyalo 0.16.0 (2026-07-10)

Combined output of a dogfood session (own KB, MDN 14,375 files, GitHub Docs
3,710 files), two parallel deep code reviews (hyalo-cli + xtask;
hyalo-core + hyalo-mdlint), and a docs-vs-implementation audit that executed
every documented example against the binary. All findings below were verified
against source or reproduced live against `hyalo 0.16.0 (3ca89718)`.

## Findings by severity

| Severity | Count | Headline |
|---|---:|---|
| CRITICAL | 1 | `lint-rules set` panics on malformed `.hyalo.toml` |
| HIGH | 11 | `links fix --apply` frontmatter no-op; batch-mv rollback gap; delimiter drift siblings; index link-graph staleness on mv; CJK autofix drops; docs H1/H2 |
| MEDIUM | ~24 | flag-dropping hints, body-relative lint lines, fuzzy threshold, non-atomic init writes, template-matcher DoS, latent unicode panic, docs M1–M6, … |
| LOW | ~15 | ASCII-only `--section`/case-index folding, latent unwraps, terminology drift, … |

## CRITICAL

### `hyalo lint-rules set <RULE> --severity/--enabled` panics on malformed `.hyalo.toml`

`crates/hyalo-cli/src/commands/lint_rules.rs:222-226` and `:263-267`. The
guard `if doc.get("lint").is_none()` only covers "key absent"; with a
hand-edited scalar (`lint = "oops"`), `doc["lint"]["rules"]` blind-indexes via
`toml_edit`'s `IndexMut` and panics ("index not found", SIGABRT/exit 134).
Reproduced live. `types.rs` handles the identical scenario cleanly via
`.as_table_mut().context(...)?` — port that pattern and add a regression test
seeded with `lint = "oops"`.

## HIGH

### H-A: `links fix --apply` silently no-ops on frontmatter wikilinks while reporting them applied

Found by dogfood. `build_replacements_for_file`
(`crates/hyalo-core/src/link_fix.rs:919`) walks the body only (skips
frontmatter by design), so frontmatter FixPlans yield no Replacement — but
the CLI reports the pre-write FixPlan list as `Applied: yes`. On the own KB,
13 of 15 "applied" fixes were `related:` frontmatter links: all silent
no-ops, and they reappear as fixable on the next run, so an agent fix-loop
never converges. The detail list also dedupes by (source, old, new), hiding
the frontmatter/body split. `mv` already rewrites frontmatter links correctly
(`plan_frontmatter_wikilink_rewrites`) — `links fix` needs the same write
path. Full repro in
[[dogfood-results/dogfood-v0160-iter157-159-pr186]] BUG-1.

### H-B: `mv --apply` batch mode doesn't roll back partially-applied link rewrites on failure

`crates/hyalo-cli/src/commands/mv.rs:586-599`. `execute_batch_mv` renames
files, then `link_rewrite::execute_plans` writes each rewrite plan
independently. If plan N of M fails (e.g. concurrent edit trips the mtime
guard), the handler rolls back the renames but not the rewrites already
written for plans 1..N-1 — other files now hold wikilinks to paths that were
renamed back. Fix: snapshot pre-rewrite content and restore symmetrically
with the rename rollback.

### H-C: `clap_complete::generate()` bypasses the broken-pipe panic matcher (worst on Windows)

`crates/hyalo-cli/src/run.rs:612`. clap_complete's generators panic with
"failed to write completion file", which doesn't match
`is_broken_pipe_panic`'s expected `println!` prefix. On Unix SIGPIPE usually
masks it; on Windows the panic hook is the only defense, so
`hyalo completion powershell | Select-Object -First 5` shows a backtrace
instead of a quiet exit. Broaden the matcher or buffer through `println!`.

### H-D: documented examples that fail outright (docs audit)

- `hyalo lint --fix-rule HYALO001` (without `--fix`) is a clap usage error
  (exit 2) but appears in **README.md:99**, the binary's own `lint --help`
  EXAMPLES, and **root CLAUDE.md:25**.
- The pi skill template ships
  `task toggle … --section "Tasks" --all`
  (`crates/hyalo-cli/templates/skill-hyalo-pi.md:169`), but `--section` and
  `--all` are mutually exclusive — an LLM consuming the skill will emit a
  failing command.

### H-E: iteration-158 fixes not ported to sibling code paths (hyalo-core)

The core reviewer's dominant pattern: fixes applied to the canonical path but
missed in 2–4 hand-rolled duplicates.

- **BOM/delimiter drift** — `link_fix.rs:946,951`, `link_rewrite.rs:459,464`
  (+2 more), `auto_link.rs` use raw `line.trim() == "---"` instead of the
  BOM-aware `frontmatter::is_opening_delimiter`. A BOM-prefixed file with a
  wikilink in frontmatter: `in_frontmatter` never activates, `mv`/`links fix`
  silently mishandle that link. The comment claiming this "matches scanner
  behaviour" is stale.
- **Closing-delimiter disagreement** — `set`/`skip_frontmatter` accept an
  indented `  ---` closer (lenient `trim()`), while `find`'s body-search path
  (`find_closing_delimiter`) requires column 0: `set` and `find` silently
  disagree on the same file. Unify via a symmetric `is_closing_delimiter`.
- **Comment-fence check unguarded by code-fence state** —
  `link_fix.rs:960-972` toggles the `%%` fence before `fence.process_line`;
  `link_rewrite.rs` and `auto_link.rs` both guard correctly and have tests. A
  code fence documenting `%%` syntax silently disables fixing for the rest of
  the file.

### H-F: `mv --index` leaves stale/dangling link-graph entries

`link_graph.rs:424-456` (`rename_path` at 269-312), `index.rs::refresh_entry`
(490-503). Bare-wikilink-form keys are never renamed and rewritten links are
never inserted; `--index` and live-scan backlinks diverge permanently until a
full rebuild. Fix: on rename, replace the file's edges wholesale
(remove-by-source + reinsert from rescanned `FileLinks`).

### H-G: case-sensitive orphan/dead-end detection

`link_graph.rs:177-193` vs. `205-221`: backlink resolution is
case-insensitive but `all_targets`/`all_sources` return raw-case keys, so a
file linked only via `[[Web/Foo]]` is wrongly reported as an orphan of
`web/foo.md`. Thread `case_index` through orphan detection.

### H-H: `task toggle/set --section` skips nested-subsection tasks

`crates/hyalo-cli/src/commands/tasks.rs:20` (`resolve_task_lines`) uses the
flat "last-seen-heading" tag, while `read --section` uses
`heading::build_section_scope` (inclusive subsections). **Verified live**: a
`### Subtask` checkbox under `## Tasks` is silently skipped by
`task toggle --section Tasks` but included by `read --section Tasks`. Reuse
`build_section_scope` in `resolve_task_lines`.

### H-I: fix-column unit misclassification drops autofixes for 6 rules on multibyte lines

`crates/hyalo-mdlint/src/engine.rs:470-472`: `rule_uses_byte_columns`
whitelists only MD009/HYALO001, but MD001/MD018/MD019/MD022/MD023/MD031 also
emit byte-based columns (hand-traced against vendored rulesets). On a CJK
heading, the char-walk never reaches the byte column, `convert_fix` silently
drops the fix via `?` — violation reported, `--fix` no-ops forever. Extend
the whitelist with CJK regression tests analogous to the MD009 one.

## MEDIUM

### Behavior (dogfood)

- **Hints drop behavior-changing flags.** `hints_for_links_auto`
  (`crates/hyalo-cli/src/hints.rs:1520`) rebuilds the apply command
  preserving `--min-length`/`--file`/`--exclude-title`/`--glob` but not
  `first_only` or `exclude_target_glob` (PR #186 extended `args.rs` without
  extending the hint builder). Pasting the hint applied 3 links where the
  dry run promised 1. Same class: the `drop-index` hint after
  `create-index --index-file <custom path>` omits the path and targets the
  in-vault index instead (this actually deleted the wrong index during the
  session).
- **`lint` MD-rule line numbers are body-relative** — off by exactly the
  frontmatter line count (verified on two corpora); SCHEMA/HYALO findings
  use a different convention (`line 1`). `--fix` edits the correct lines;
  reporting only. Dogfood BUG-5.
- **Fuzzy link-fix default threshold accepts wrong matches.** Shared
  `iterations/done/iteration-` prefixes inflate Jaro-Winkler: a never-existed
  target matched an unrelated file at 0.896. On real data ShortestPath went
  15/15 correct, FuzzyMatch 0/2. Score the basename/slug, and/or add
  `--strategy`. Dogfood BUG-3.
- **Stale snapshot index diverges silently.** Mutations without `--index`
  don't patch an existing in-vault `.hyalo-index`; later `--index` queries
  return pre-edit values with no staleness signal. Dogfood BUG-4.
- **Triple-reporting of one bad datetime value** (firefox F-3, still open):
  SCHEMA error + HYALO003 (date heuristic, wrong rule for a datetime-typed
  property) + HYALO004 for the same property/value.

### Code (deep review)

- **`init`/`deinit` use plain `fs::write`, not `atomic_write`** (~11 call
  sites in init.rs), including the read-modify-write upsert of a user-editable
  `.claude/CLAUDE.md` — a crash mid-write truncates it.
- **`task toggle --all` re-saves the snapshot index once per task**
  (`tasks.rs:236-239, 342-345`; `patch_index` at 377-417) instead of batching
  like `set`/`mv` do via `save_index_if_dirty` — 200 toggles = 200 full index
  serializations.
- **`types set --default` vault-wide propagation** writes many files with no
  per-file mtime guard (unlike the lint fix pipeline).
- **`--section` matching is ASCII-only case-insensitive**
  (`hyalo-core/src/heading.rs:168`): `--section CAFÉ` misses `## Café Notes`
  (verified live); `--title` does full Unicode lowering. The
  `debug_assert!(needle.is_ascii())` is compiled out in release.
- **Three latent `unwrap()`s in production code** (`set.rs:419`,
  `remove.rs:286`, `append.rs:299`) — currently unreachable but violate the
  project ban; identical pattern, `mutation.rs` extraction candidate.
- **`.hyalo.toml` writes in lint_rules.rs/types.rs are non-atomic** (direct
  `fs::write`, no mtime guard) unlike markdown frontmatter writes.
- **`plan_batch_mv` redundant re-read** (`link_rewrite.rs:1079`): second
  `read_to_string` of a file already in memory with nothing invalidating the
  first read — wasted I/O on large batches, and misleading about what it
  guards.
- **xtask `feature_fanout.rs:103`** flag-presence check is a substring match
  (`--dir` would match `--dir-foo` or prose mentions).

### Code (hyalo-core deep review)

- **Latent unicode panic in `is_self_link`'s `strip_md`**
  (`link_fix.rs:706-714`): `s[s.len()-3..]` panics mid-codepoint (verified in
  isolation on `"a🎉"`). **Reachability check by team-lead**: all four call
  sites pass discovered `.md` file paths, whose ASCII suffix keeps the slice
  boundary-safe — two live repro attempts (emoji filename, emoji link target)
  did not crash. Downgraded from the reviewer's CRITICAL to a latent panic:
  one refactor away from a crash, and the sibling
  `strip_wikilink_md_suffix` already does it safely with a regression test.
  Fix with `Path::extension()` or an `is_char_boundary` guard.
- **Filename-template matcher has exponential worst case**
  (`filename_template.rs:149`): 12+ adjacent `{slug}` placeholders hung >12 s
  (recursive backtracking, no memoization); `.hyalo.toml` loads untrusted and
  `lint --fix` calls it per file — a real DoS vector on shared KBs. Memoize
  or cap.
- **Index snapshot validation gaps**: duplicate `rel_path` entries desync
  `entries()` from `path_index` (double-counted tags in summary); size/count
  caps checked only after full MessagePack materialization; BM25 postings
  order not validated on load (crafted snapshot silently corrupts phrase
  search). All three fixable inside `index.rs`/`bm25.rs` validation.
- **Phrase-clause rejection is document-scoped** (`bm25.rs:644-725`): a doc
  failing one phrase clause is excluded from the whole OR result set even if
  another clause matches it.
- **TOCTOU stat-then-read across readers** (`scanner/mod.rs:48-80` +
  callers): nothing bounds the actual read; use `Read::take(MAX_FILE_SIZE)`.
- **`atomic_write` has no Windows rename retry** (`fs_util.rs:17-51`):
  transient sharing violations from AV/indexers fail the whole mutation.
- **`!key=value` silently mis-parses** (`filter/parse.rs:86`) as equality on
  a literal property named `!key` → silent zero results instead of a parse
  error.
- **Merged type schema recomputed per file** during lint
  (`lint.rs:520` → `schema.rs:62`): O(files × schema) instead of
  O(types × schema) on large KBs.
- Observations: `is_external` misses `ftp:`/`tel:`/protocol-relative URLs
  (fuzzy-matched as broken internal links); case-index folding is ASCII-only
  (Turkish İ/ı, Cyrillic); Setext headings invisible to `--section`
  (undocumented); dead `pub` API `remove_entry`/`insert_entry` with stale
  doc comments; one provably-safe `.expect()` in `scanner/strip.rs:60`;
  `plan_batch_mv` has zero direct unit tests (only e2e coverage).

### Docs (audit — executed against the binary)

- **M1**: top-level help "Global flags" box says `--format` default is
  `json` (`cli/help.rs:108`); actual is TTY-aware, contradicting the long
  help in the same output.
- **M2**: `config --format json` emits 7 fields; help/README/CLAUDE.md
  enumerate 6 (`raw_contents` undocumented).
- **M3**: `--files-from` silently changes `find`'s `results` from array to
  object, breaking the canonical `--jq '.results[].file'` recipe the help
  itself advertises (correct: `.results.files[].file`).
- **M4**: `find --help` COMMON MISTAKES declares `=~` wrong, but the parser
  accepts `=~` identically to `~=` (137/137 verified) — an LLM may "fix"
  working commands.
- **M5**: `init`/`deinit` accept but ignore `--format json` (always print
  human text).
- **M6**: the documented "frontmatter would exceed size budget" exit-1 error
  for `set`/`remove`/`append` is unreachable (8 KiB scalar read-cap fires
  first, file silently skipped, exit 0); only `hyalo new` can reach it.

## LOW

- `tasks` alias for `task` still missing (firefox F-4); `links --file` still
  rejected while peer commands accept it (F-5).
- Multi-line property values print raw newlines in text output — a value
  containing `\nmalicious: injected` renders as a fake sibling property
  (write path is safely quoted; display only). Dogfood BUG-6.
- `backlinks` help lists `label` unconditionally but it's
  `skip_serializing_if = None`; top-level OUTPUT SHAPES omits it.
- Invalid `--sort` error advertises `score`; the flag help doesn't list it.
- README omits `[search] language`/`--language`/`--stemmer` and `views run`;
  README "Releasing" invites a manual tag contrary to the
  `gh release create` convention; CHANGELOG's `0.16.0 — 2026-05-23` date
  predates ~2 months of work now under it.
- Terminology drift, one directory / four names: help says "vault" (9×,
  0× "knowledgebase"), runtime says "kb dir:", config says "dir:", README
  mixes vault×22/knowledgebase×7, root CLAUDE.md knowledgebase-only — while
  DEC-002 deliberately moved away from "vault". Help/README are the outliers.

## What's genuinely well done (both reviewers, independently)

- The **mtime+size TOCTOU fingerprint + temp-file+rename atomic-write
  pattern** is applied uniformly across every mutating command — reused,
  not reinvented. Held up under 20 concurrent writers in dogfood.
- **Broken-pipe handling** (PR #186) is architecturally sound: one install
  site, all output funneled through two print sites, verified across every
  command; the only gap is the clap_complete path (H-C).
- **Zero non-test `unwrap()`/`expect()`** across the entire read/query
  surface; the two exceptions in `find/mod.rs` are guarded and commented.
- The **byte/char-boundary autofix bug class stays fixed**: every
  autofixable rule's offsets go through `char_indices()`/`len_utf8()`,
  backed by CJK/emoji tests; `apply_body_fixes`' two-phase
  (severity-priority selection, descending-offset mutation) overlap handling
  is provably correct with explicit adjacency and out-of-bounds tests.
- **`mv`'s vault-escape guard** is enforced at plan-time and again before
  each fs op; link matching has deterministic ambiguity resolution.
- **d8285aa (`--first-only`)**: `resolve_existing_link_targets` correctly
  mirrors the scanner's zone-skipping, divergence documented; solid tests.
- The **`unsafe { libc::kill }` PID-liveness check** (`index.rs:938-988`) is
  the only unsafe block found and it's exemplary: guards `pid == 0`/overflow,
  distinguishes `ESRCH` from `EPERM` correctly, accurate SAFETY comment,
  clean non-Unix degradation.
- **BM25 core**: Lucene-style IDF provably positive, index/query tokenizer
  symmetry confirmed, char-based (unicode-safe), CRLF a non-issue.
- **Fence-awareness in task mutation** is correct at every entry point with
  atomic-batch-rejection tests — the one subsystem where byte/char handling
  was hand-traced and found flawless.
- Clean `cargo clippy --workspace --all-targets -- -D warnings`.

## Suggested fix order

1. CRITICAL lint-rules panic (small, pattern exists next door in types.rs).
2. H-A `links fix` frontmatter write path + honest Applied reporting.
3. H-E sibling-drift bundle (BOM delimiter ×5 call sites, closing-delimiter
   unification, comment-fence guard) — one iteration, shared helper extract.
4. H-H nested-subsection task toggle (reuse `build_section_scope`) and
   H-I mdlint byte-column whitelist — both small, both silently wrong today.
5. H-D doc/example fixes (three files + pi template; cheap, high LLM impact).
6. H-F/H-G link-graph index staleness + case-insensitive orphans.
7. MEDIUM hint flag-preservation (`first_only`, `exclude_target_glob`,
   `--index-file` in drop-index hint).
8. MEDIUM lint line-number offset (+ unify SCHEMA line convention),
   latent `strip_md` panic, template-matcher memoization.
9. The rest of the MEDIUMs opportunistically; batch with related files.
