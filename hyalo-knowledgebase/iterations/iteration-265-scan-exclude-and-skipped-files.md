---
type: iteration
title: Iteration 265 — Vault-wide scan exclude and skipped-file reporting
date: 2026-09-03
status: planned
tags:
  - iteration
  - config
  - scan
  - index
  - dogfooding
branch: iter-265/scan-exclude-and-skipped-files
priority: 5
related:
  - "[[dogfood-results/dogfood-v0220-obsidian-vaults]]"
---

# Iteration 265 — Vault-wide scan exclude and skipped-file reporting

## Goal

Every command that scans a vault handles unparsable or excluded files its own
way, and the report [[dogfood-results/dogfood-v0220-obsidian-vaults]] shows
the cost. On kepano-obsidian, 28 Templater templates with `{{date}}` in their
frontmatter make `summary`, `find`, `tags`, `properties`, `lint`, `mv` and
`views` each print 251 stderr lines of `serde_yaml` excerpts (UX-1), while
`summary` reports `Files: 75` and never says 28 were skipped (UX-2). The only
exclusion knobs are per-feature (`[lint] ignore`, `[okf] ignore`,
`[schema] exempt`); Obsidian's "Excluded files" has no analogue. Around the
index, `links auto --index` aborts on one unparsable file instead of skipping
it (BUG-8), `create-index` counts an invalid-UTF-8 file in BM25 statistics
that the disk scan excludes (BUG-14), and the stale-index check exists on some
commands and not others (UX-7). Finally a malformed `.hyalo.toml` drops
`[lint] ignore` and schemas with a single warning, so `lint` in CI passes
files it should not have checked (BUG-19).

This iteration adds a `[scan] exclude = ["Templates/**"]` section honoured by
every command, collapses per-file parse diagnostics into one summary line,
makes `summary` report skipped and excluded counts, and closes the three
index-parity gaps. Constraint: **no new CLI flags** from dogfood pressure
(project rule): verbosity uses the existing `-q`, `RUST_LOG` and a config key;
exclusion is config only. Out of scope: MD-rule autofix behaviour
([[iterations/iteration-263-lint-autofix-obsidian-safety]]) and the
lint-ignore override for explicitly named files
([[iterations/iteration-267-help-hints-text-polish]]).

## Tasks

### SCAN-1: `[scan] exclude` honoured everywhere

- [ ] DEC-277 (tentative): a new `[scan]` section in `.hyalo.toml` with
      `exclude = [<glob>, …]`, relative to the vault dir, applied at file
      discovery so excluded files are invisible to every command (`find`,
      `summary`, `tags`, `properties`, `lint`, `links *`, `mv` link graph,
      `backlinks`, `create-index`, `views`, `types`, `okf`, `madr`). Define
      precedence against `[lint] ignore`, `[okf] ignore` and `[schema] exempt`
      (they stay, narrower in scope) and against an explicit `--file` /
      `--files-from` target (proposal: an explicitly named excluded file is
      refused with a message naming the glob, the same policy DEC-284 in
      iteration 267 picks for `[lint] ignore`).
- [ ] hyalo-core discovery: apply the globs once in the walker, reusing the
      `[lint] ignore` glob matcher; `--index` paths must also drop excluded
      files, and `create-index` must not index them (index/disk parity test).
- [ ] `hyalo config` text and JSON print the effective `[scan] exclude`
      (`results.scan.exclude`, empty list by default).
- [ ] Unit tests on the walker; e2e in `crates/hyalo-cli/tests/e2e`: a vault
      with `Templates/**` excluded where `find --count`, `summary`, `tags`,
      `lint --count` and `create-index` `files_indexed` all agree, and a
      `--file Templates/x.md` request is refused with the glob named.
- [ ] Docs: `hyalo config -h/--help`, the `.hyalo.toml` reference page under
      `hyalo-knowledgebase/`, `.claude/skills/hyalo/SKILL.md` config
      paragraph, `.claude/CLAUDE.md`, changelog.

### SCAN-2: collapse unparsable-frontmatter diagnostics (UX-1, UX-2)

- [ ] DEC-278 (tentative): per-file YAML diagnostics are collected, not
      streamed. At the end of a run stderr shows one line, `warning: skipped N
      files with unparsable frontmatter (run hyalo lint --rule HYALO005 for
      details)`; `-q` silences it; the full multi-line excerpts appear only
      under `RUST_LOG=hyalo=debug` or `[scan] verbose_skips = true`. `lint`
      itself keeps reporting each file as HYALO005 since that is its job.
- [ ] hyalo-core: route the parse failure through a `SkippedFile { path,
      reason }` collector on the scan result instead of `eprintln!` at the
      call site; hyalo-cli prints the summary line once per process.
- [ ] `summary`: add `results.files.skipped` (unparsable), `results.files.
      excluded` (by `[scan] exclude`) and per-directory skipped counts; text
      mode prints `Files: 75 (28 skipped, 0 excluded)` and a hint to
      `hyalo lint --rule HYALO005`.
- [ ] e2e: a vault with two `{{date}}` templates yields exactly one warning
      line on `find`, `summary`, `tags`, `properties` and `mv --dry-run`;
      `-q` yields none; `summary --format json` carries the counts.
- [ ] Docs: `summary -h/--help` result keys (also closes the HELP-14 part
      about undocumented keys for the new ones), skill file, changelog.

### INDEX-1: index/disk parity on skipped files (BUG-8, BUG-14)

- [ ] `links auto --index`: the "index is missing N files, adding from disk"
      refresh path must use the same skip-and-warn behaviour as the disk
      scan; an unparsable file is counted in the SCAN-2 summary and named in
      the debug log, never propagated as an error.
- [ ] `create-index` full build: exclude invalid-UTF-8 files from BM25
      document count and average length exactly like the disk scan, and
      report them under `warnings` so `files_indexed` matches `summary`
      `Files:`.
- [ ] Parity test: build an index over a fixture containing one invalid-UTF-8
      file and one unparsable-frontmatter file; assert `find <term> --index`
      scores equal disk scores to 1e-6 and `links auto --index --dry-run`
      exits 0 with the same match count as the disk run.
- [ ] Changelog entries for both.

### INDEX-2: stale-index check alignment (UX-7)

- [ ] Enumerate which commands check `.hyalo-index` staleness today
      (`find` warns when the index is older than the vault mtime; `links auto`
      does a per-file (mtime, size) refresh; `set --index` warned and then
      refreshed itself) and record the matrix in a research note.
- [ ] DEC-280 (tentative): one policy. Proposal: every `--index` command does
      the cheap per-file stat refresh for the files it is about to touch or
      return (a `--file`-targeted `find` stats its targets), and prints the
      "index older than vault" warning only when it cannot refresh. Reject
      making full-vault refresh implicit on every read because it re-introduces
      the cost iteration 260 removed.
- [ ] Implement the policy in hyalo-core index loading; e2e: `find --index
      --file <just-appended file>` returns the new content or warns, never a
      silent stale snapshot; `set --index` no longer prints a warning for a
      staleness it fixes itself.
- [ ] Docs: `--index` long help paragraph shared by all commands, skill file,
      changelog.

### CONFIG-1: malformed `.hyalo.toml` is loud (BUG-19)

- [ ] DEC-279 (tentative): today a `.hyalo.toml` that fails to parse blocks
      mutating commands and lets reads continue on defaults with a warning.
      `lint` is a read but is used as a CI gate, and dropping `[lint] ignore`
      and schemas silently changes the verdict. Decide: `lint` (and
      `find --strict`, `views run`, any command whose exit code is a gate)
      exits 1 when the config is malformed; other reads keep the warning but
      print it on every invocation, `-q`-proof. Record which commands are
      "gates".
- [ ] Implement in hyalo-cli config loading; e2e: a `.hyalo.toml` with an
      unknown top-level key makes `hyalo lint` exit 1 with the parse error and
      `hyalo find --count` still answer with the warning.
- [ ] Docs: `hyalo config --help` malformed paragraph, `.claude/CLAUDE.md`
      config-discovery bullet, skill file, changelog.

## Acceptance criteria

- [ ] kepano-obsidian, cwd `../kepano-obsidian`, clean checkout, no config
      change: `hyalo summary --format text 2>err.txt; wc -l err.txt` → 1
      line (was 251) and it names 28 skipped files; `hyalo summary --format
      json --jq '.results.files | {total,skipped,excluded}'` → `total 75,
      skipped 28, excluded 0`; `hyalo find --limit 1 -q 2>&1 >/dev/null |
      wc -l` → 0.
- [ ] `../kepano-obsidian` with a scratch `.hyalo.toml` containing
      `[scan]\nexclude = ["Templates/**"]`: `hyalo find --count`, `hyalo lint
      --count`-scanned files, `hyalo tags --format json --jq
      '.results.files_scanned'` and `hyalo create-index --format json --jq
      '.results.files_indexed'` all report 51 (103 minus 52 under
      `Templates/`); `hyalo config --jq '.results.scan.exclude'` →
      `["Templates/**"]`; stderr has no frontmatter warning at all.
- [ ] Obsidian Hub, cwd `../obsidian-hub`: `hyalo create-index && hyalo links
      auto --index --dry-run --format json --jq '.results.matched'` exits 0
      and equals the disk run's `matched` (report: 18510).
- [ ] Scratch vault with `printf 'line ok\n\xff x\n' > bad.md`: `hyalo
      create-index --format json --jq '.results.warnings'` → 1; `hyalo find
      index --limit 3 --format json --jq '.results[0].score'` equals the
      `--index` value.
- [ ] Scratch vault: append a file, then `hyalo find --index --file new.md
      --fields lines` returns the current line count (not a snapshot) or a
      stale-index warning, per DEC-280.
- [ ] Scratch vault with `exclude = [...]` at the top level of `.hyalo.toml`:
      `hyalo lint; echo $?` → 1 with the parse error; `hyalo find --count`
      still prints a number and a `-q`-proof warning.
- [ ] Own KB: `hyalo summary` output gains the skipped/excluded counts (both
      0) and nothing else changes; `hyalo lint --strict` clean.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`, `hyalo lint --strict` on the KB,
      xtask help-drift check.
- [ ] Changelog entries via `hyalo changelog add`; DEC-277 through DEC-280
      recorded in [[decision-log]]; `.claude/skills/hyalo/SKILL.md` and
      `.claude/CLAUDE.md` document `[scan] exclude` and the summary line.

## Links

- [[dogfood-results/dogfood-v0220-obsidian-vaults]]
- [[iterations/iteration-260-lazy-bm25-snapshot-load]]
- [[iterations/iteration-263-lint-autofix-obsidian-safety]]
- [[iterations/iteration-267-help-hints-text-polish]]
- [[decision-log]]
