# Changelog

## Unreleased

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

### Added

- **Case-insensitive link resolution.** Wikilinks and markdown links now
  resolve even when the target file's path differs in case (e.g.
  `[[api/fetch]]` matches `API/Fetch.md`). Controlled via `.hyalo.toml`:
  `[links] case_insensitive = "auto"` (default), `true`, or `false`.
  `"auto"` enables it on case-insensitive filesystems (macOS, Windows).
- New lint rule `link-case-mismatch`: warns when a link resolves only via
  case-insensitive fallback, suggesting the canonical-case path.
- `links fix` now detects and offers to fix case-mismatched links.

### Changed

- `--index` semantics: bare `--index` now unambiguously uses `.hyalo-index`
  in the vault directory. Use `--index-file <PATH>` for a non-default path.
