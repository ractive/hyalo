# Changelog

## Unreleased

### Added

- **iter-145**: `task toggle` and `task set` now accept
  `--files-from <file|->` and `--glob <pattern>` via the unified input
  resolver. Multi-file selection flattens all per-file task results into a
  single array in the standard
  `{"results": [...], "total": N, "hints": [...]}` envelope.
- **iter-145**: `task read`, `read`, and `backlinks` now accept
  `--files-from` (resolved to a single file, consistent with their
  single-file policy). `--glob` is explicitly rejected with a clear error
  for these commands.

### Changed

- **iter-146**: `hyalo --version` now includes the git short-sha and commit
  date — e.g. `hyalo 0.16.0 (abc123def456 2026-05-26)`. A `+dirty` suffix is
  appended when the working tree had uncommitted changes at build time.
  Builds without a `.git` directory (crates.io tarball, offline) fall back
  silently to the bare `hyalo <semver>` form. Set
  `CARGO_HYALO_FORCE_NO_GIT=1` to force the bare form; CI can pre-supply
  `GIT_COMMIT` + `GIT_COMMIT_DATE` to skip the shell-out.

- **iter-145**: Unified file-input resolver (`commands/inputs.rs`) replaces
  three separate seams: `resolve_files_from_for_command`, `collect_files`,
  and `resolve_single_file`. All `<FILE>`/`--file` commands now go through
  the single `resolve_inputs` entry point with a per-command
  `ResolutionPolicy` that captures single-vs-multi semantics.

- **iter-144**: Index-suggestion hints. Two new automatic hints surface
  `hyalo create-index` when no snapshot index is active:
  - **Slow-query hint** — fires on `find`, `lint`, `backlinks`,
    `properties summary`, `tags summary`, `summary`, and `read` when the
    command takes longer than 500 ms. Suppressed by `--quiet` or when
    `--index`/`--index-file` is already in use.
  - **Large-vault summary hint** — fires from `hyalo summary` when the
    vault contains more than 500 files and no index is active.
  Both hints count toward the existing `MAX_HINTS` cap and are suppressed
  by `--no-hints` like all other hints.

### Changed

- **iter-143**: New `hyalo lint` hint — when SCHEMA violations land on a file
  with a declared `type:`, `hyalo types show <T>` is surfaced as the
  next-step. Generic across all SCHEMA failure modes (`required`, `pattern`,
  `item_pattern`, `required_sections`, type-mismatch). Suppressed when
  `--rule SCHEMA` or `--rule-prefix HYALO` is already active. Capped at 2
  distinct types per invocation.
- **iter-143**: `hyalo types show <T>` now suggests `hyalo new --type <T>`
  when the type declares any `required` properties.
- **iter-143**: `--files-from` callers (any command that accepts it) get
  counter-aware advice hints: `<N> input path(s) did not exist on disk` and
  `<N> input path(s) were outside the vault`. Prepended so the `MAX_HINTS`
  cap doesn't crowd them out behind generic next-step hints.

### Fixed

- **iter-143**: `--index --files-from` now consults the snapshot for
  membership instead of falling through to `is_file()`. Paths that exist on
  disk but are absent from the snapshot count as `files_missing` —
  consistent with the `--index` contract ("snapshot is the source of
  truth"). Closes the deferred item from iter-139.

- **NEW-1**: `item_pattern` lint validation now reports every offending item in a
  `string-list` property (with its index) instead of short-circuiting after the first.
  Same fix for the per-item "expected string, got <kind>" branch.
- **NEW-2**: `--files-from` now strips the full configured `--dir` prefix (multi-segment
  paths like `files/en-us/x.md` with `--dir files/en-us`), not just the last component.
  Forward-slash normalisation handles Windows-flavoured input. Vault-relative literal
  paths still win over strip-and-retry. The all-missing stderr hint quotes the actual
  configured `dir`.
- **NEW-3**: `hyalo new --help` no longer claims it errors when the parent directory is
  missing (iter-140 BUG-4 made it `create_dir_all`). Help text scrubbed.
- **NEW-4**: `--files-from` trims leading/trailing whitespace per line before resolving,
  so `printf '  edge.md\n'` no longer reports the path as missing.
- **NEW-5**: `create-index` accepts `--index-file PATH` as a synonym for `-o/--output`.
  Conflicting values (`-o A --index-file B`) produce a clear error. The stale-index
  warning no longer fires when output was redirected away from the default location.
- **NEW-6**: `--files-from` input is deduplicated by resolved vault-relative path,
  preserving first-seen order (uses `IndexSet`). Pipelines like `git log --name-only`
  no longer cause `lint` to re-lint or `find` to return duplicates.
- **BUG-1**: `required_sections` schema enforcement was dead code in the grouped lint
  path (`lint_one_file_extended`). It now calls `validate_required_sections` and reports
  missing or out-of-order sections as `SCHEMA` errors.
- **BUG-2**: `--files-from` now strips the vault-dir basename prefix from repo-relative
  paths (e.g. `kb/notes/foo.md` with `--dir kb` resolves to `notes/foo.md`). Emits a
  hint to stderr when every entry was missing.
- **BUG-3**: Canonical TOML key for required body sections is now `required_sections`
  (snake_case). The old `required-sections` (kebab) is accepted as a deprecated alias and
  emits a warning on load.
- **BUG-4**: `hyalo new` now creates parent directories automatically (`create_dir_all`)
  instead of returning an error when they are missing.
- **BUG-5**: `hyalo new` scaffold no longer emits a double trailing newline; output ends
  with exactly one `\n`, eliminating MD047 false positives.
- **BUG-6/7**: `--files-from` counters (`files_missing`, `files_skipped_non_md`,
  `files_skipped_outside_vault`) are now under `.results` in the JSON envelope. For `lint`
  (results is an object) they are inserted directly; for `find` (results was a bare array)
  the array is promoted to `{"files": [...], "files_missing": N, ...}`.

### Added

- **Quality-gate xtask** (`cargo run -p xtask -- check-ac-fidelity | check-feature-fanout | check-help-drift`): three PR-time guards that catch partial implementations (AC-fidelity), cross-command flag inconsistency (feature-fanout matrix), and help-text drift before merge. Wired into a new `quality-gates.yml` CI workflow.

- **`EXAMPLES:` blocks on every subcommand `--help`** (`find`, `set`, `task`, `summary`,
  `read`, `links`, `create-index`, `types`, `properties`, `tags`, `backlinks`, `remove`,
  `append`, `views`, `init`, `lint-rules`) — LLM-ergonomics fix so agents don't need to
  escalate to top-level `hyalo help` to find idiomatic patterns. An integration test
  guards against future regressions.
- **`--files-from <PATH>`** flag on `find`, `lint`, `mv`, `set`, `remove`, and `append`:
  supply a newline-separated list of file paths (or `-` to read from stdin) and the command
  operates on exactly that set, bypassing the directory walk. Non-`.md` paths, paths outside
  the vault, and missing files are silently skipped; counters appear in the JSON envelope as
  `files_missing`, `files_skipped_non_md`, and `files_skipped_outside_vault`. Enables
  diff-aware CI workflows: `git diff --name-only origin/main | hyalo lint --files-from -`.
  Mutually exclusive with `--glob` and `--file`.
- **`item_pattern`** on `string-list` properties: per-item regex validation
  at `hyalo lint` time. Declare `type = "string-list"` and `item_pattern = "^..."` in
  `[schema.types.X.properties.Y]`. Each list item is matched against the regex;
  violations include the item index and pattern.
- **`required-sections`** on type schemas: declares the body outline a document
  of this type must contain. Entries are `"## Heading"` strings (level encoded by
  hash count); order-significant; extras are silently allowed. Enforced by `hyalo lint`.
- **`hyalo new --type <name> --file <vault-relative-path>`**: schema-driven file
  scaffolder that emits a placeholder skeleton (required frontmatter + required
  sections, all values `TBD` / type-appropriate empties). Designed to produce a
  file that fails lint — the lint loop is the agent feedback mechanism.

## 0.16.0 — 2026-05-23

### Breaking changes

- The hybrid `--index [=PATH]` flag has been split into two orthogonal flags:
  - `--index` is now a pure boolean; no value accepted.
  - `--index-file <PATH>` specifies an explicit index file and implies `--index`.

  Migration:

      hyalo find --index=./my.idx
      hyalo find --index-file=./my.idx

  `--index` and `--index-file` are **no longer global** — they appear only on
  subcommands that actually consume the snapshot index (`find`, `summary`,
  `tags summary/rename`, `properties summary/rename`, `backlinks`, `lint`,
  `links fix`, `read`, `set`, `remove`, `append`, `mv`, `task *`). They no
  longer appear on `create-index`, `drop-index`, `init`, `completion`,
  `views *`, or `types *`.

- `properties rename` and `tags rename` JSON output now uses `skipped_count`
  (integer) instead of `skipped` (array). This is a breaking change for
  consumers that parse the JSON output.

### Added

- `properties rename --dry-run` and `tags rename --dry-run` — preview which
  files would be modified without writing to disk.
- `find --fields outline` — alias for `--fields sections`.
- `--stemmer` / `--language` now accepts ISO 639-1 two-letter codes (e.g.
  `en`, `de`, `fr`) in addition to full language names.
- `create-index` output now notes when replacing an existing index file.
- `lint` hints now suggest adding unfixable files (e.g. unclosed frontmatter)
  to `[lint] ignore` in `.hyalo.toml` instead of only showing "See defined
  type schemas".

- **Case-insensitive link resolution.** Wikilinks and markdown links now
  resolve even when the target file's path differs in case (e.g.
  `[[api/fetch]]` matches `API/Fetch.md`). Controlled via `.hyalo.toml`:
  `[links] case_insensitive = "auto"` (default), `true`, or `false`.
  `"auto"` enables it on case-insensitive filesystems (macOS, Windows).
- New lint rule `link-case-mismatch`: warns when a link resolves only via
  case-insensitive fallback, suggesting the canonical-case path.
- `links fix` now detects and offers to fix case-mismatched links.
- `task set --dry-run` — preview which tasks would be changed without
  modifying the file.
- Security: snapshot index (``.hyalo-index``) now validates entry paths on load
  — rejects traversal (``..``), absolute paths, and null bytes.
- Security: snapshot index files larger than 512 MB are rejected to prevent
  OOM from crafted files.

### Changed

- `--index` semantics: bare `--index` now unambiguously uses `.hyalo-index`
  in the vault directory. Use `--index-file <PATH>` for a non-default path.
- Removed three `unsafe { from_utf8_unchecked }` blocks in the scanner; the
  ASCII-only mutation paths now go through safe `String::from_utf8`. Only
  `unsafe` left in the codebase is `libc::kill(pid, 0)` for PID-liveness in
  the snapshot index. See [decision-log DEC-042] and
  `research/miri-unsafe-audit.md`.

### Fixed

- `hyalo backlinks <target.md>` now finds incoming short-form `[[basename]]`
  wikilinks that unambiguously resolve to the target — previously they were
  silently dropped while `find --fields links` resolved them correctly. The
  two commands now share resolver semantics. `find --orphan` / `--dead-end`
  inherit the fix.
- **Cross-platform link resolution.** Obsidian short-form bare wikilink
  resolution (`[[note]]` → `sub/note.md` when unique) now works on
  case-sensitive filesystems (Linux, Windows) even when
  `[links] case_insensitive` is off or auto-detects off. Previously the
  short-form stem fallback was incorrectly gated on case-insensitive mode.
- `links fix` reports a short-form wikilink whose stem casing differs from
  the on-disk filename as `LinkCaseMismatch` (was `ShortFormStemMismatch`).
  Same user intent — fix the casing — and now consistent across platforms.

### Internal

- Miri scaffolding: `justfile` recipes (`just miri`, `just miri-filter`,
  `just miri-all`) and `#[cfg(not(miri))]` gates around `rayon::par_iter`
  with serial fallback. Manual gate only, not in CI.
