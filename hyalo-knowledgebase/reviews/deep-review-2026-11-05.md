---
title: "Deep review 2026-11-05 — CLI and core code reading"
type: research
date: 2026-11-05
status: active
tags: [dogfooding, review]
related:
  - "[[iterations/iteration-321-content-safe-autofix]]"
---

# Deep review: hyalo-cli, hyalo-mdlint, xtask at main d1801b5a

Method: code reading plus the release binary on small vaults under scratch vaults (v1..v5). Nothing built or run through cargo.

## Findings

### L-1 HIGH  apply_body_fixes corrupts text when an insertion and a replacement share a start offset
crates/hyalo-cli/src/commands/lint/fix.rs:34-106 (conflict test at :56, ordering at :85)

The overlap test is `start < we && end > ws`. A zero-width insert at P and a replacement [P, Q) do not "overlap" under it, so both win. Buffer mutation then sorts by `Reverse(start)` only. A stable sort keeps priority order on ties, and MD022 (insert) comes before MD023 (replace). The inserts land first, and the replacement range [P, Q) then covers the inserted bytes plus the original text minus its tail. The tail survives as stray text after the replacement.

The comment at :95 says re-validating with `get` "enforces" the reasoning, but `get` only checks bounds and char boundaries, not content.

Repro (v1/c.md, body `# Wide\n ## Indented\ntext\n`):
- Fix ranges: MD022 inserts "\n" at 7 (twice) and at 20; MD023 replaces [7,20) with "## Indented\n".
- `hyalo lint --fix` produces `# Wide`, blank, `## Indented`, blank, then a stray line `d`, blank, `text`.
- It reported "fixed 8 · conflicts 0". A longer indented heading leaves a longer fragment.

Fix: order ties by `(start desc, end desc)`, so the replacement applies before the insert at its start. Better, build the output in one forward pass over non-overlapping edits sorted by start. Treat an insert at a replace range's start as a conflict. Add a unit test with an insert and a replace at the same start. The multi-pass re-lint does not catch this, because the result is valid markdown.

### L-2 HIGH  MD022/MD023/MD031 fixers ignore containers: blockquote and list-item structure is rewritten
Source: crates/hyalo-mdlint/src/engine.rs:~800-915. The `BodySpans` filter knows fences, indented code and HTML comments only. The fix text comes from the upstream mdbook-lint rules, which are container-blind.

Repro (v1/d.md):
- `> quote\n> ```sh\n> ls\n> ```\n> after` gets bare blank lines inserted. One blockquote becomes three.
- `1. step\n   # not heading?` is a heading inside the list item. MD023 de-indents it to column 0, which ends the list item and moves the heading out of it.
- MD022 and MD031 insert the blank line without the `> ` or indent prefix.

Fix: either suppress a fix whose line sits in a blockquote or list container (the span scanner has the information), or have the fixer re-emit the container prefix. Re-check that the fix does not change the parsed block structure.

### L-3 MEDIUM  Likely shared root cause for the known MD034/MD011/MD022/MD023 reports, plus another instance
engine.rs lint_body. Prose-versus-code is decided in one place (`BodySpans::line_is_code` and `line_is_html_comment`), but stock rules are suppressed per rule by a growing special-case list: MD011 regex, MD018 tag, MD031 fence, MD034 link markup, MD042 image. HTML blocks (`<pre>`, `<code>`, attributes), inline code, math and macro arguments are not modelled at all. Every new false positive becomes another `if rule_id == ...` branch.

New instance, not in your list: MD034 rewrites a URL inside a `$$ ... $$` math block (`a = https://example.com/m` becomes `a = <https://example.com/m>`; v1/a.md).

Fix: give `BodySpans` a "protected inline/block ranges" model (HTML raw-text elements, math, inline code, HTML tag attributes) and apply `fix_intersects_protected` plus a column-level check to every non-code-aware rule. That replaces the per-rule branches.

### L-4 MEDIUM  MD010 fixer rewrites tabs inside fenced code
engine.rs:~1100 (`CODE_BLOCK_AWARE_RULE_IDS` contains MD010).

Repro: a ```make fence containing `\techo hi` is changed to 4 spaces (v1/a.md). That breaks Makefile, TSV, Go and other tab-significant samples. The doc comment calls this deliberate, but the fixer should not rewrite code fences. Keep the diagnostic and drop the `fix` inside fenced or indented code.

### L-5 MEDIUM  An invalid `severity` in `[lint.rules.X]` is silently treated as warn
engine.rs:~750 (`effective_severity`), and three duplicated copies in `lint_frontmatter_hyalo003/004/007` (`_ => Warn`).

Repro: `[lint.rules.MD009] severity = "eror"` gives warn, exit 0, no message. `hyalo config` reports malformed:false, and `lint-rules show` reports effective_severity warn. A CI gate the user thinks is an error never fails. `lint-rules set` validates; hand-edited config does not.

Fix: validate in config load (refuse like an unknown key). Fold the three duplicated resolvers into one helper. Those copies have already diverged, since `--strict` handling is only in the frontmatter ones.

### L-6 MEDIUM  `--jq` drops leading empty-string outputs
crates/hyalo-cli/src/output/jq.rs (`execute_jq_filter`): `if !out.is_empty() { out.push('\n') }`.

Repro: `hyalo find --limit 1 --jq '"", "", "a", "b"'` prints `a\nb`, not `\n\na\nb`. `"a","","b"` is fine. A filter like `.results[] | .properties.status // ""` loses the lines for leading files. That misaligns line-per-file output for downstream `paste` or `while read`. The byte accounting has the same flaw.

Fix: track `first` as a bool instead of testing `out.is_empty()`.

### L-7 MEDIUM  `deinit` destroys user content with no preview
crates/hyalo-cli/src/commands/init.rs (deinit). `deinit` has no `--dry-run`, which `init` also lacks.

Repro (v4): after `init`, append `[schema.types.note]` and `[views.todo]` to `.hyalo.toml`, append text to `.claude/skills/hyalo/SKILL.md` and `.claude/rules/knowledgebase.md`, then run `deinit`. All three files are deleted, including the user-authored schema and views, with no hash or marker check. Only the CLAUDE.md block and the jev assets are protected by markers or a manifest. `.claude/skills/hyalo/extra.md` was kept.

Fix: add `--dry-run`. Refuse or skip files that differ from the shipped template, unless forced. Keep `.hyalo.toml` when it carries more than `dir`.

### L-8 MEDIUM  xtask `workspace_root()` fails open to the current directory
crates/xtask/src/workspace.rs:10-35.

It uses `unwrap_or_else(".")` when `CARGO_MANIFEST_DIR` is unset (running `target/debug/xtask` directly, as in your memory note). If the walk finds nothing, it falls back to "." without error. It also matches any `Cargo.toml` that contains the substring `[workspace]`, including in a comment or a `[workspace.dependencies]`-only crate.

A gate run from the wrong directory then reads the wrong tree or an empty one. Some gates error on a missing path, but gates that tolerate `NotFound` do not (pi_package_sync prints "skipping", bundled_skills skips the pi dir).

Fix: error when no workspace is found, and parse the TOML for a `[workspace]` table.

### L-9 MEDIUM  check-pi-runtime trusts whatever `node_modules` exists
crates/xtask/src/pi_runtime.rs:10-13. This is the likely cause of your "passed locally through a symlink, failed in CI".

The precondition is `node_modules/esbuild` `.is_dir()`, which follows symlinks. A `node_modules` symlinked from another checkout or worktree, or one that is stale versus `package-lock.json`, produces a bundle for different dependency versions, and the byte comparison then reflects that tree rather than the lockfile. `node` is taken from PATH with no version pin, and build.mjs also embeds the detect-libc banner read from node_modules.

Fix: reject symlinked `node_modules` (`symlink_metadata`), and verify the installed tree matches the lock (`npm ls --all` exit status, or compare `node_modules/.package-lock.json` to `package-lock.json`). Optionally print esbuild and typescript versions in the gate.

### L-10 LOW  check-mutation-journal has a stale hardcoded list and a substring test
crates/xtask/src/mutation_journal.rs:27-50 and :124.

- `MUTATING_MODULES` omits changelog, madr, okf, types, views, lint_rules, init and deinit. They are all writers; today changelog, madr, okf and types reference the journal. If one stopped, the gate would stay green.
- `content.contains("MutationJournal")` passes on a comment.
- `SANCTIONED` entry "src/run.rs" can never match. `short` is stripped of the `src/` prefix, so it would be "run.rs". It is dead today because run.rs has no forbidden token, but the exemption does not work.
- The submodule scan reads only the top-level `.rs` files of `commands/lint/`.

Fix: derive the list from modules that call the atomic-write API, or assert the list against a scan. Search for a code reference rather than the substring.

### L-11 LOW  `.expect()` in non-test code (project rule)
commands/append.rs:400, remove.rs:371 and :392, set.rs:724 and :744, mod.rs:441, find/mod.rs:1343 (`.unwrap()`), types.rs:556 and :566, mdlint schema.rs:323. All have documented invariants, but the repo rule is "no expect outside tests". The `serde_json::to_value(&result).expect(...)` five-pack could be a single helper returning `Result`.

### L-12 LOW  Hint for `set` with no target omits the positional form
`hyalo set --property k=1` prints "set requires --file or --glob", but a positional FILE is accepted.

## Reviewed and found sound
- lint_body suppression of directives and unterminated fences, and `convert_fix` bounds checks (`get(start..end)`).
- `apply_body_fixes` bounds handling. The panic risk is covered; only the tie-order bug in L-1 remains.
- config_write.rs: capture/publish with an unchanged short-circuit, atomic replace, finalisation error mapping.
- `mv` dry-run versus apply (same `replacements`), same-path and collision errors, and `--on-conflict skip`.
- `lint-rules set` validation of rule id and severity, and dry-run.
- Clap conflicts for `--file`/`--glob`/`--files-from` on set, find and lint (rejected with exit 2, not ignored).
- Exit codes spot-checked: clap errors 2, user errors 1, jq syntax error 1, jq runtime error 2 (decided in DEC-378).
- `--jq` size and value caps and truncated diagnostics.
- xtask typed_output (syn-based, cfg-aware), pi_package_sync symlink rejection and reverse-orphan check, codex_package (BTreeSet, deterministic), ts_types (sorted walk).
- task toggle/set on fences and comments. `tags rename` scope matches its --help.

## Not reached
- links auto/fix internals (known issues), okf/madr/changelog writers, views/types config editing, `new`.
- run.rs/dispatch.rs, hint generation, broken-pipe code paths.
- Windows-specific path handling (read only; nothing exercised).
- npm_package.rs generation determinism (only grepped for HashMap/time; none found), bench_scale, jq_recipes, help_drift gate logic.
- `.hyalo.toml` discovery edge cases beyond a single config.
- HYALO001 fixer beyond a few shapes. `> [] x` and `1. [] x` are left alone, which is conservative; a lazy-continuation `  [] x` after a quote becomes a list item.

# Second pass (L-13 onward)

### L-13 MEDIUM  `links auto --apply` links inside HTML element text and `$$` math
Same root cause as L-3 (no HTML or math model in the span scan).
Repro (v6/beta.md): `<a href="x">Alpha Note</a>` becomes `<a href="x">[[alpha|Alpha Note]]</a>`, and `$$Alpha Note$$` becomes `$$[[alpha|Alpha Note]]$$`. Fences, inline code and existing links were left alone. Whatever fix L-3 adds should be shared with the auto-linker.

### L-14 MEDIUM  `hyalo new --file` mangles the path and accepts non-note names
crates/hyalo-cli/src/commands/new.rs:103,161 (also okf.rs:1274, changelog.rs:56,123, config.rs:744,801, files_from.rs:227).
- `new --type note --file 'sub\w.md'` on macOS creates `sub/w.md`, while `read` and `mv` keep the backslash as a filename byte. This contradicts DEC-363, which says `\` is a separator only on Windows.
- `--file sub/n` creates an extensionless `sub/n` that no hyalo command will ever see.
- `--file .md` creates a file literally named `.md`.
- `--file x.MD` is created with an uppercase extension.

Fix: apply `\`→`/` only under `cfg(windows)`, require a `.md` stem, and use one path-normaliser for every command argument that names a file.

### L-15 MEDIUM  Mutation-failure envelope has a hard-coded wrong hint and wrong text rendering
crates/hyalo-cli/src/output.rs:118 (`with_apply_report`).
Every failed mutation (set, mv, new, ...) gets the hint "task toggle is not safe to retry blindly". Repro: `hyalo new --type note --file sub/ --format json` yields `state: failed_before_commit` plus that hint. In text mode the same failure prints `Effects:  (committed; inspect before retrying)` although nothing was committed, so the user is told to look for writes that never happened.
Fix: derive the hint from the command and from whether any path is committed; keep the text and JSON states consistent.

### L-16 MEDIUM  `changelog release/add` accept input that breaks the changelog grammar
crates/hyalo-cli/src/commands/changelog.rs.
- `release 0.5.0 --date 2026-13-45 --apply` writes `## [0.5.0] - 2026-13-45`.
- `release 0.5.0` is accepted while `[1.0.0]` already exists. It inserts above it and `lint --profile changelog` then errors on VERSION-ORDER.
- The footer link is written as `[0.5.0]: TBD` when no compare URL can be derived.
- `add --message $'ok\n## [9.9.9] - 2026-02-02\n[Unreleased]: http://evil'` writes the extra lines as 2-space-indented continuation lines. They are then a heading (up to 3 spaces of indent are allowed) and a link reference definition. `lint --fix` MD023 would dedent the heading into a real `## [9.9.9]` section, a chain across two commands.
Fix: validate the date as a calendar date, refuse a version at or below the current top release, reject `TBD` footers or warn, and reject (or escape) a multi-line message whose later lines start with `#`, `[x]:` or a list marker.

### L-17 LOW  check-jq-recipes `MUTATING_SUBCOMMANDS` misses writers (latent)
crates/xtask/src/jq_recipes.rs:31-41, :294-298, :338-363.
The list lacks `tags`, `properties`, `task`, `types`, `views`, `lint-rules`, and `lint --fix` (which uses no `--apply`). Only `--apply` and the listed names are policed, and recipes then run against the repo root. `subcommand_of` also returns None when a global flag precedes the subcommand (`hyalo --dir kb set ...`). No shipped document has such a recipe today (grepped README, docs, templates, .claude/CLAUDE.md), but a future `tags rename ... --jq` would be executed against the real knowledgebase by the gate.
Fix: reverse the logic. Run recipes only for an allow-list of read-only subcommands, or always append `--dry-run` where the subcommand accepts it and fail otherwise.

### L-18 LOW  check-help-drift coverage hole and weak help capture
crates/xtask/src/help_drift.rs:60-92 and :133-148.
- `SUBCOMMANDS` is hand-maintained. It omits `deinit`, `drop-index` and `completions`, and every nested subcommand (`links fix|auto`, `tags rename`, `properties rename`, `views set|remove|run`, `types set|...`, `lint-rules set`, `okf index|log`, `changelog add|release`, `madr toc`). Their EXAMPLES and short-help ceilings are unchecked, and nothing compares the list with the clap tree.
- `help_text` ignores the exit status and returns stderr when stdout is empty, so a build failure's compiler output counts as "help". 3a still fails on it, but 3b/3g/3h-style content checks over that text are vacuous.
Fix: enumerate subcommands from `hyalo --help` recursively, and require a zero exit status.

### L-19 LOW  Windows-reserved and stream names are accepted by `new` and `mv --to`
No reserved-name handling exists in hyalo-cli (grep for CON/NUL/reserved/':' found none). macOS accepted `CON.md`, `nul.md`, `a:b.md`. On Windows `a:b.md` is an NTFS alternate data stream on file `a`, and `CON.md`/`NUL.md` are device names. Trailing dot or space is silently stripped by Win32. I could not run this on Windows, so it is code-read only. Fix: reject these in the shared vault-relative path validator under `cfg(windows)`.

### L-20 LOW  `.hyalo.toml` `dir` rewrites `\` to `/` on every OS
config.rs:744,801 and `validate_project_local_dir`. Off Windows, a directory literally named `a\b` is unreachable via `dir`. Same DEC-363 inconsistency as L-14. Gate the replace on `cfg(windows)`.

### Reviewed in the second pass and found sound
- `links fix`: dry-run and `--apply` produced the same edits. Fences and inline code were untouched. Frontmatter links were rewritten. The ambiguous `[[dup]]` was left alone. Fuzzy needs `--apply-fuzzy`.
- `madr toc`: refuses malformed or duplicate markers, and escapes `|` in titles.
- `okf index|log`: managed-region preservation is sound. Link destinations with spaces are wrapped in `<...>`.
- `changelog` and `set/append/task/tags rename/remove/lint --fix` all preserve CRLF.
- `views set/remove` validate names. `types set` rejects dotted names and unknown property types, and preserves comments.
- Broken pipe: a mutating `set` with a closed stdout writes all files, reports them committed and exits 141. Hint quoting for names with `'`, `$`, `&`, `()` is correct POSIX quoting.
- Config discovery: nearest ancestor whose vault contains the cwd, the "parent directory" note, `--dir` switch notice, malformed config only warns on reads, and absolute or escaping `dir` is refused (`C:foo` handled via the Prefix arm).
- npm_package: BTree-ordered or sorted outputs, no timestamps or HashMap iteration found. `query_npm_registry` classifies E401/E403 as failures.

### Still not reached
- run.rs and dispatch.rs line by line (only exercised through the binary), hint generation internals beyond quoting and the known dead ends.
- `links auto` code-span detection across list items (known), `okf` bundle edge cases (reserved-file collisions), `types set --default` coercions.
- bench_scale budgets, npm_package generation beyond the grep, and Windows behaviour beyond reading the code (no Windows host).

# hyalo-core deep review (main d1801b5a, binary hyalo 0.25.1 d1801b5a2960)

Method: read-only. Code reading of fs_util, rooted, frontmatter/splice, case_index, index load path, link_rewrite apply path, discovery::resolve_target_inner, body_syntax/strip. Behaviour checks with the release binary on small vaults under scratch vaults (v1..v12, p*, s*). No cargo run.

## Findings

### C-1 HIGH (wrong results) - partial-path wikilinks never resolve
Where: crates/hyalo-core/src/discovery.rs:2859 `resolve_target_inner` (exact path, case index, `.md`, `/index.md`, then bare-stem only); link_resolve.rs `resolve_stem` / case_index `lookup_stem*` are stem-only.
Scenario (v11): `a/x/note.md` and `b/y/note.md`; body `[[x/note]] [[y/note]]`. `find --fields links` gives `path: None` (broken) for both; `[[a/x/note]]` resolves. `links fix` offers only fuzzy candidates below the floor. `[[x/note]]` is exactly what Obsidian writes to disambiguate a duplicated stem (Obsidian matches any path suffix of the link). Consequences: false "broken" in find/summary/HYALO006; such links are not graph edges, so `backlinks` omits them and `mv` neither rewrites nor reports them when the target moves. The bare-stem ambiguity code (DEC-374) knows candidates but a multi-segment target never reaches it.
Fix: after the exact/index attempts, for a target with a `/`, look up the case-index for files whose path ends with `/<target>.md` (or `<target>/index.md`); unique -> hit, multiple -> Ambiguous. Do it in the one shared resolver so read, mv, links fix agree. Not found in any DEC or backlog item (backlog/done/shortest-path-link-resolution.md only covers bare stems).

### C-2 MEDIUM (quadratic CPU) - unbalanced backticks in a long paragraph
Where: body_syntax.rs:419 `crate::scanner::code_run_exists(crate::scanner::block_lookahead(rest), run)`; scanner/strip.rs:94 `block_lookahead` (scans every line of the rest of the block just to find its end, before the cheap `code_run_exists`); same pattern in `strip_inline_code_stateful` callers.
Scenario: each backtick run with no closer on its own line calls block_lookahead, O(rest-of-block). A block with no blank line and one stray backtick (or LaTeX-style ``quote'') per line is O(n^2). Measured (`hyalo summary`): 7.5k lines 0.33s, 15k 1.28s, 30k 5.1s; a 3.2MB file of 60k such lines: summary 22s, create-index 22s, lint 72s on a 5MB vault. MAX_FILE_SIZE is 100MB, so a hostile or generated note can stall every command that scans the vault (rayon does not help, one file).
Fix: compute the block end once per block (monotone cursor in the line loop, reset at is_block_boundary) and, per run length, memoise "does a run of exactly N backticks occur after this offset within the block" (e.g. a map N -> last offset, built in one pass), or scan forward line by line from the current line and stop at the first boundary or hit (early exit on hit makes the common case O(1)).

### C-3 MEDIUM (crash) - non-UTF-8 argv panics
Where: crates/hyalo-cli/src/run.rs:727 and :791 (`std::env::args()`; outside hyalo-core but reached by every invocation).
Scenario: `hyalo find $'\xff'`, `hyalo set --file $'bad\xff.md'`, or a shell glob expanding to a Latin-1 filename on Linux: panics with "called Result::unwrap() on an Err value: "\xFF"" in std::env, exit 134 instead of a JSON error envelope (breaks the 0/1/2 exit contract). Reproduced on macOS with `hyalo find $'\xff'` and `hyalo summary $'--dir=\xff'`.
Fix: use `args_os()` and `to_string_lossy()` for the pre-scan (quiet detection, raw_args); let clap handle OsString and report a normal error.

### C-4 MEDIUM - every newly created file gets mode 0600, ignoring umask
Where: rooted.rs:424-434 `NewEntry::create` (NamedTempFile::new_in then persist_noclobber, no set_permissions); fs_util.rs `atomic_write` for a not-yet-existing destination (existing_perms is None); fs_util.rs test `atomic_write_new_file_uses_platform_default` (:398) deliberately asserts nothing about the mode.
Scenario (v1, umask 022): `hyalo create-index` -> `.hyalo-index -rw-------`; `hyalo init` -> `.hyalo.toml -rw-------`; `hyalo new --type note --file n1.md` -> `-rw-------`. Notes created in a shared/group vault, a container volume or a web-served docs tree are unreadable to the other users/processes, while hyalo-written replacements of existing files keep their mode, so the vault ends up with inconsistent modes.
Fix: for a new entry chmod the temp file to `0o666 & !umask` (read umask once via a probe-free approach: create with OpenOptions + mode(0o666) and let the kernel apply umask, or use `tempfile::Builder::permissions`). Add a test that asserts mode == 0o644 under umask 022.

### C-5 MEDIUM/LOW - first frontmatter write on a file without frontmatter moves the BOM and mixes line endings
Where: frontmatter/parse.rs:1242 `write_frontmatter*` (insert path); frame.rs:591 `no_frontmatter()` reports `has_bom: false` even when the file starts with U+FEFF.
Scenario (v12): `\xEF\xBB\xBF# My Title\r\ntext\r\n`; `hyalo set h.md --property k=v` yields `---\nk: v\n---\n\xEF\xBB\xBF# My Title\r\n...`: the BOM is no longer at the start of the file (it is now inside the first body line), and the file has LF frontmatter over a CRLF body. Files with a BOM and frontmatter are handled correctly (tested in mod.rs), so this is the one gap. Related read-side defect: BOM + `# Title` and no frontmatter is not recognised as an H1 (`title_source: filename`), because the heading scan sees `\u{feff}#`.
Fix: in the insert path detect a leading BOM, emit BOM + frontmatter + body-without-BOM, and use the dominant line ending of the body (first line ending) for the new block. Strip a leading BOM before heading detection.

### C-6 LOW (quadratic CPU) - single-line pathological link text
Where: links.rs:1143 `try_parse_markdown_link_at` -> `parse_destination` / `find_close_paren_after_destination` (:543) and `find_label_close_bracket` (:513) rescan to end of line for every `[`; wikilink path similar.
Measured: line of `[a](` x n: 20k repeats (80KB) 3.0s, 40k 12s; `[[` x n: 20k 0.87s, 40k 3.5s; `[` x n 0.23s/0.88s. Doubling input quadruples time. Only reachable with one very long line, so LOW, but it is the same class as C-2 and shares the 100MB cap. Fix: bound the scan (stop at the first unescaped `[` for the label; cache "no `)` after offset k" per line).

### C-7 LOW (latent panic) - byte-offset slicing without boundary check in the rewrite applier
Where: link_rewrite.rs:1518-1520 `apply_replacements_observed`: `end <= line.len() && line[pos..end] == *r.old_text`. `end <= len` is checked but `pos`/`end` char-boundary is not, so a Replacement whose offset was computed against a different text layout (any future producer, or a plan reused after the line changed) panics instead of being skipped. The code right below already treats mismatch as "stale, skip". Fix: `line.get(pos..end) == Some(r.old_text.as_str())`. Also allocates a new String per replacement (`format!`); use replace_range on a mutable String.

### C-8 LOW - post-publish failure reported as write failure; bulk path has no content fsync
(a) fs_util.rs:327-331: after `tmp.persist(dest)` succeeded, a failing `set_permissions(dest)` returns Err, so the caller reports the write as failed although the new content is already published (and the temp-file chmod at :309 already set the same mode, so the second call is redundant). Drop it, or make it non-fatal.
(b) rooted.rs `Durability::BulkRewrite` (DEC-317, >8 plans): no `sync_all` on the replacement before rename, then only the directory is fsynced. A directory fsync does not make the file's data durable, so a power loss after finish() can leave zero-length notes on filesystems without rename-replace data ordering (xfs, some network FS). Documented trade-off; flagging that `Durability::BulkRewrite` doc says "can lose published content", but `lint --fix` over a whole vault is exactly where that is costly. Consider syncing files in the worker (cost is parallelisable) and only deduplicating the directory fsync.

### C-9 LOW - replacement drops owner/ACL/xattrs and breaks hardlinks
rooted.rs:stage_replacement and fs_util write_no_dir_sync copy only the mode. Every mutation creates a new inode: uid/gid (a root-run hyalo takes ownership of the user's note), POSIX ACLs, SELinux labels, xattrs and hardlink identity are lost. Acceptable for most vaults; at least document it, or copy owner when running as root.

### C-10 LOW (unverified, Windows only) - reserved device names are not rejected
No check for CON/PRN/AUX/NUL/COM1../LPT1.. (with or without extension) or trailing dot/space anywhere in the repo (grep found only `has_unsafe_windows_colon`, discovery.rs:1380). `hyalo new --file nul.md` or `mv --to aux.md` on Windows would open the device (data silently discarded, or mv hard_link fails after capture). Not reproducible here.
Fix: validate each component of any user-supplied destination in RelativeName::new on cfg(windows).

### C-11 LOW (maintenance hazard) - first-call-wins process globals
discovery.rs:27/44/279 (`SCAN_INCLUDE`, `SCAN_EXCLUDE`, `LINK_ALIASES` OnceLock) and bm25/tokenizer.rs:104 (`SEARCH_SETTINGS`): a second `set_*` with different values is silently ignored. The CLI sets them once, and `--dir` onto another vault switches config before the call (verified with cA/cB: exclude applies only to its own vault), so no user-visible bug today. Any embedding, multi-vault process or test run in the same process sees the first vault's config; the tests already work around this ("only one test per process could ever set"). A returned `bool`/Err from the setters when a different value is already installed would turn silent divergence into a loud one.

## Reviewed and found sound
- Frontmatter splicer: final re-parse gate (`map_eq`) makes every heuristic safe; BOM+CRLF frontmatter, empty block, `...` terminator, mixed endings (falls back with warning), no trailing newline all behave. Alias/deep/wide YAML bombs are rejected cheaply (d_* tests, <0.05s).
- Confinement: symlinked note, symlinked dir, dangling symlink dir, `mv`/`new` through them, `lint --fix` - nothing written outside the vault, outside file untouched.
- Snapshot loading: 1,650 random byte-flip/truncation runs of `find`/`summary`/`backlinks --index-file` on a mutated v8 snapshot: no panic, no hang, exit 0/1 only. rel_path validation (abs, `..`, NUL, Windows prefix/ADS) is complete; entry/edge caps exist (post-decode, but serde caps prealloc).
- Query parser: 20+ hostile queries (unbalanced parens, 5000-deep nesting, 20k ORs, 3000 prefixes, huge slop, invalid UTF-8 in pattern) never panic; only argv-level C-3.
- mv rewriting with BOM/CRLF/multibyte/table cells/frontmatter links: byte-exact, line endings preserved.
- Atomic write ordering (fsync temp, rename, dir fsync) in PerFile/PerDirectory is correct.

## Not reached
link_fix.rs (4.6k lines), auto_link.rs, link_graph.rs, catalog.rs, schema.rs, anchor_fix.rs, filter/, tasks.rs, ignore_classify.rs (only skimmed), bm25 scoring math and tokenizer internals (black-box only), index.rs incremental reuse/tree_moved logic, case_index hardlink false-positive in the stat probe (speculative: `README.md`/`readme.md` hardlinked on a case-sensitive FS would read as case-insensitive), Windows and Linux behaviour (macOS only), dead-code / pub-surface audit, duplicate-logic drift between link_rewrite and link_fix beyond the resolver.

---
# Part 2 (second pass: link_fix/auto_link/anchor_fix/link_graph/catalog, index reuse, schema/filter/tasks, bm25, pub audit)

### C-12 MEDIUM (wrong results) - wikilink `../x` is classed out_of_vault although it names an in-vault note
Where: catalog.rs `resolve_with_explicit_paths` (candidate building): only `kind == Markdown || raw.starts_with("./")` is normalised against the source directory; a wikilink starting with `../` is passed raw to `normalized_target_escapes_vault`, which sees the leading `..` and returns `Resolution::External`.
Scenario (v13): `sub/src.md` containing `[[../t]]`, `[[../other/o]]`, `![[../t.md]]`; `t.md` and `other/o.md` exist. `find --broken-links` shows `out_of_vault: true`, `summary.links.broken: 0, out_of_vault: 2`, `links fix` lists them under `out_of_vault_links`, `backlinks t.md` is empty. The Markdown spelling `[x](../t.md)` in `sub/m.md` resolves to `t.md` and `mv t.md --to t9.md --dry-run` rewrites it but leaves `[[../t.md]]` untouched. So the link is never broken, never a graph edge, and never rewritten: it silently rots after any move. (Assumes Obsidian's relative wikilinks; `./` already gets source-relative treatment here, so `../` is the same rule with the dots missing.)
Fix: `raw.starts_with("./") || raw.starts_with("../")` in the catalog branch; then make normalize_link_target (below) agree.

### C-13 MEDIUM (maintenance hazard, root cause for the writer/reader drift) - three independent target-normalisation implementations
1. catalog.rs `resolve_with_explicit_paths` (used by mv/link_rewrite, link_graph, and by `resolve_link_from_source` when the case index is complete).
2. discovery.rs:1878 `normalize_link_target` + `resolve_target_inner` (:2859), used when the index is absent/incomplete and by `classify_link`, `links fix`: wikilinks are "resolved as-written", `./` is stripped (root-relative), `..` always rejected, `.md`/`/index.md` appended by its own code.
3. The write side re-implements the splitting itself: link_rewrite.rs:544 `split_target_fragment` splits on `#` only (catalog and discovery split on `#` and `?`); `percent_decode_path` is re-called at link_rewrite.rs:1110, 1265, 1425, 2153, 2344, link_write.rs:304, catalog.rs:105, link_fix.rs:1739/1767; `split_frontmatter_target` and `strip_wikilink_md_suffix` are separate again.
The same wikilink therefore gets different meanings: `[[./t]]` from `sub/` is `sub/t.md` via catalog but root `t.md` via the legacy path (discovery.rs strips `./` before probing, see the DEC-310 comment); `[[../t]]` is External in catalog and None in legacy; `?` is stripped on read but not by the writers' fragment split, which matches the known `[[note?]]` mv misses. DEC-374 (iter-319) unified only the ambiguity predicate. I could not make the legacy path run from the CLI (find/lint/links fix all use the catalog with a complete index), so the `./` divergence is code-reading only; C-12 is verified.
Fix direction: make `catalog::resolve` return the canonical split (path, fragment, query, alias) and have every writer take its spans from that one structure; delete normalize_link_target's wikilink arm.

### C-14 LOW - `- [ ]no space` is a task
tasks scanner (toggle and `find --fields tasks`): `- [ ]no space` is listed as an open task and `task toggle` rewrites it to `- [x]no space`; GFM and Obsidian require whitespace after `]`. `- [ ]` with nothing after is also a task (GFM: yes, so fine). Visible effect: prose such as `- [x]ample` counts as a done task.

### C-15 LOW - dead public API in hyalo-core (no non-test caller in the workspace)
anchor.rs `into_ids`, `explicit_anchor_ids_in_line`, `fragment_matches_explicit_anchor`; anchor_fix.rs `plan_anchor_fixes` (only the `_filtered` variant is used); bm25/tokenizer.rs `heading_body_lines`; body_syntax `line_is_indented_code`; case_index `alias_count`, `ambiguous_stem_key`; discovery `is_scan_excluded`, `scan_exclude_glob`, `reset_*_count`, `resolve_attachment_from_source` (pub, one internal caller); filename_template `matches_path`; frontmatter/frame.rs `opening_span`, `closing_span`; frontmatter/parse.rs `get_property`, `set_property`, `remove_property` (pre-splice mutation API: an unused way to write frontmatter that bypasses the splicer); link_rewrite `PartialExecuteReport::applied_paths`; scanner/frontmatter `into_props`; schema `from_raw_lossy`, `type_for`; warn `skipped_frontmatter_count`, `invalid_utf8_count`; fs_util `WritePhase` (deprecated, inert). Candidates for `pub(crate)` or deletion; the unused `set_property`/`remove_property` pair is the one worth removing to keep a single write path.

## Verified sound in pass 2
- `links fix --apply --apply-fuzzy` and `links auto --apply` on a CRLF file with table cell, no trailing newline, BOM-free frontmatter: line endings and final-byte state preserved; anchor suggestions are advisory and idempotent on re-run.
- `links auto` with Unicode case-folding hazards (U+0130, sharp s, KELVIN SIGN, ligatures, final sigma): offsets correct, no panic, no corruption.
- Incremental `create-index` reuse across config changes (`[search] language`, `code_blocks`, `[links] aliases/frontmatter`): `--index` and disk answers agree after a plain re-run; language is tracked per entry.
- BM25: empty files, frontmatter-only files, empty corpus - no NaN/panic; avgdl and doc-id validation exist on load; `as u32` casts are bounded by the 100MB file cap.
- `task toggle` on CRLF/fenced/nested/no-final-newline files keeps bytes exact and skips fenced tasks.

## Still not reached
link_fix.rs fuzzy scoring (link_score.rs) and relocation planner internals beyond the CLI surface; link_graph internals beyond its catalog call; `index::tree_moved` / staleness probe (not exercised; only create-index reuse via CLI); schema.rs validation and filter/ parser (only skimmed via CLI); tokenizer stemming internals; auto_link zone detection beyond the Unicode test; Windows/Linux behaviour; ignore_classify precedence port vs the `ignore` crate.

