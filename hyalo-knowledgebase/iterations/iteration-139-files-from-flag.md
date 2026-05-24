---
title: Iteration 139 — `--files-from <path|->` for VCS-agnostic diff-aware scoping
type: iteration
date: 2026-05-24
status: completed
branch: iter-139/files-from-flag
tags:
  - iteration
  - cli
  - lint
  - consumer-tooling
related:
  - "[[research/ff-rdp-discipline-consumer-notes]]"
  - "[[iteration-138-schema-extensions-and-new-command]]"
---

## Goal

Add `--files-from <path|->` to every command that already accepts
`--glob`. The caller supplies a list of file paths (one per line, stdin
sentinel `-`), and the command operates on that exact set — bypassing
the directory walk entirely.

This unblocks "diff-aware lint in CI" without coupling hyalo to git
(or any VCS). Callers compute the file set via whatever tool fits —
`git diff --name-only`, `hg status`, `make .changed`, a script — and
pipe the result into hyalo.

```sh
git diff --name-only origin/main -- 'kb/**/*.md' | hyalo lint --files-from -
```

ff-rdp's `iter-73` discipline tooling will use this to scope plan
linting to "only the plans touched on this branch". General-purpose
beyond ff-rdp — any consumer with a "what changed" tool can scope any
hyalo command the same way.

## Steps

### Flag plumbing

- [x] New global / per-subcommand `--files-from <PATH>` flag, where
      `PATH = -` reads from stdin.
- [x] Add to every command that currently accepts `--glob`: `find`,
      `lint`, `mv`, `set`, `remove`, `append`, `task toggle`,
      `task set`. (Skip `summary`, `properties summary`, `tags summary`,
      `properties rename`, `tags rename` — these are whole-vault by
      design.)
- [x] Mutually exclusive with `--glob` AND `--file` via clap
      `conflicts_with_all`. Clear error if two sources are given.

### Input parsing

- [x] Read source: file path or stdin (`-`).
- [x] One path per line. UTF-8 only — bail with a clear error on
      invalid UTF-8.
- [x] Strip leading `./` from each line.
- [x] Skip empty lines silently (common when piping from `git diff`).
- [x] Skip lines starting with `#`? **No** — keep it simple. Lines are
      literal paths, no comments.
- [x] No globs in line entries (`**/*.md` is literal text, not
      expanded). If you want globs, use `--glob`.

### Path resolution

- [x] **Absolute paths**: accepted, rewritten via
      `discovery::strip_absolute_vault_prefix`. Paths that lie outside
      the vault tree are warn-and-skipped (same as missing files).
- [x] **Relative paths**: treated as vault-relative (forward-slash
      form). Reject `..` traversal — same path-safety rules as
      everywhere else.
- [x] **Path normalization**: backslashes → forward slashes on input
      (Windows-friendly).

### Filtering

- [x] **Filter to `.md` only**, silently skip everything else
      (directories, `.toml`, `.txt`, etc.). Real-world `git diff` output
      is broader than the caller wants for hyalo specifically.
- [x] **Missing files**: warn-and-skip. Count in the JSON envelope as
      `files_missing: N`. Common when piping from `git diff` that
      includes deletions; the script shouldn't have to filter to
      `--diff-filter=AMR`.
- [x] Track a sibling count: `files_skipped_non_md: N` and
      `files_skipped_outside_vault: N` in the JSON envelope.

### Index interaction

- [x] **When `--index` (or `--index-file`) is given**: read from the
      snapshot for every path in the input list. Paths not present in
      the index get treated the same as missing-on-disk
      (`files_missing`). **Do not fall back to disk-rescan.** The
      contract for `--index` is "snapshot is the source of truth";
      `--files-from` must not weaken that.
- [x] **Without `--index`**: read from disk for each path. No
      directory walk happens — the input list IS the work set.
- [x] Performance: this should be measurably faster than `--glob` on
      large vaults when the file count is small (CI use case).

### Empty input

- [x] `--files-from -` with zero lines (or only empty lines):
      exit 0 with the standard envelope and `total: 0`.
- [x] Hint: "no files in input — nothing to do".

### Hints

- [x] `HintSource::FilesFrom` variant on success: suggest running the
      same command without `--files-from` to operate on the whole
      vault, when relevant.
- [x] Existing hints that suggest "scan all files" or "use `--glob`"
      pick up an alternative: "or pass `--files-from -` if you already
      have a file list (e.g. `git diff --name-only`)".

### Docs + UX surfaces

- [x] `--files-from` documented in the flag help text for every
      command that accepts it.
- [x] `README.md`: add a one-paragraph "CI diff-aware lint" example
      showing the git-diff pipeline.
- [x] `crates/hyalo-cli/src/cli/help.rs` example list: add a
      `--files-from` example to the long-help sections for `find` and
      `lint`.
- [x] `crates/hyalo-cli/templates/rule-knowledgebase.md`: short note
      that `--files-from` is available when the caller has a file list.
- [x] CHANGELOG `Unreleased` entry under Added.
- [x] Decision-log: add DEC-044 capturing "VCS-agnostic scoping via
      `--files-from` instead of a git-integrated `--since` flag", with
      reference to [[research/ff-rdp-discipline-consumer-notes#B]].

## Tasks

- [x] Add `--files-from` flag to every applicable subcommand
- [x] Implement input parsing (file + stdin, UTF-8, line splitting)
- [x] Implement path resolution (absolute via `strip_absolute_vault_prefix`,
      vault-relative, normalization)
- [x] Implement filtering (`.md` only, missing files, outside-vault)
- [x] Wire envelope counters: `files_missing`, `files_skipped_non_md`,
      `files_skipped_outside_vault`
- [x] Wire `--index` interaction (snapshot-only resolution; no disk
      fallback)
- [x] Wire mutual-exclusion errors with `--glob` and `--file`
- [x] `HintSource::FilesFrom` + per-command hint updates
- [x] Update help texts on every affected subcommand
- [x] Update README.md with the CI diff-aware lint example
- [x] Update `cli/help.rs` long-help examples
- [x] Update `templates/rule-knowledgebase.md`
- [x] CHANGELOG `Unreleased` entry
- [x] Decision-log DEC-044
- [x] Unit tests: input parsing edge cases (empty, comments-not-stripped,
      whitespace, trailing newline, UTF-8 BOM, CRLF)
- [x] Unit tests: path resolution (absolute in-vault, absolute
      out-of-vault, relative, `..` rejection)
- [x] E2E tests for `find` and `lint` with `--files-from`: happy path,
      empty input, mixed valid + missing + non-md + out-of-vault
      paths, mutual-exclusion errors, `--index --files-from` combo
- [x] Cross-platform CI verification (macOS + Ubuntu + Windows)

## Acceptance criteria

- [x] Every command in scope accepts `--files-from` with consistent
      semantics
- [x] `git diff --name-only origin/main | hyalo lint --files-from -`
      lints only the changed `.md` files; non-`.md` and deleted files
      surface as counters in the envelope, not as errors
- [x] `--index --files-from -` reads exclusively from the snapshot;
      paths missing from the snapshot count as `files_missing`
- [x] `--files-from` + `--glob` or `--files-from` + `--file` fails with
      a clear "pick one" error
- [x] Absolute paths inside the vault are rewritten to vault-relative;
      outside-vault absolute paths skip with the outside-vault counter
- [x] Empty input → exit 0
- [x] README + help + rule template + CHANGELOG + decision-log all
      updated in the same PR
- [x] All three CI platforms green
- [x] `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`
      green

## Design notes

- **VCS-agnostic by design.** No git dependency, no `--since` flag, no
  built-in diff. The caller picks their tool. Hyalo just accepts a
  flat file list.
- **`--files-from` is strictly a *source* of paths**, equivalent to
  `--glob` and `--file` in that role. All three feed the same
  downstream path-handling pipeline. Mutual exclusion keeps the source
  of truth obvious.
- **`--index` semantics preserved.** When `--index` is given, the
  snapshot is the source of truth — `--files-from` filters into it,
  never out of it. A path that's in `--files-from` but not in the
  index is treated as a missing file. This matches what `--index`
  already means and avoids a hidden disk-rescan fallback.
- **Filtering is silent for non-`.md` and outside-vault**, with
  counters in the envelope. CI scripts piping from `git diff` produce
  lots of irrelevant entries; a hard error would force every script
  to wrap with `grep -E '\.md$'` and `--diff-filter`, which is the
  exact ergonomic problem `--files-from` is meant to solve.
- **No `--files-from0` (NUL-separated) initially.** Newline-separated
  covers 99% of cases. Add later if anyone has a `.md` filename with a
  literal newline (vanishingly rare and likely a bug).

## Out of scope

- `--since <git-ref>` flag with built-in git integration. Explicitly
  rejected — see consumer notes for the discussion.
- `hyalo diff <revA>..<revB>` as a first-class command. `git diff` +
  `--files-from` covers the case.
- Globs in `--files-from` line entries. Use `--glob` if you want globs.
- Inline comments / `#` line prefixes in the input. Keep parsing
  trivial.
- Combining `--files-from` with `--glob` (intersection or union).
  Confusing; reject as mutually exclusive.
- Auto-detecting whether stdin is a TTY and warning if `-` is used
  interactively. Not worth the surface; the caller knows what they
  piped.

## References

- [[research/ff-rdp-discipline-consumer-notes#B]] — the consumer
  feature where `--files-from` was proposed and James's reasoning for
  preferring it over a git-coupled `--since` flag
- [[iteration-138-schema-extensions-and-new-command]] — the
  schema-side companion iteration; together iter-138 + iter-139 cover
  the ff-rdp wishlist with the smallest possible hyalo surface
