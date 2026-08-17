# Configuration reference

> Part of the [hyalo](../README.md) documentation.

`hyalo init` creates a `.hyalo.toml` in your project root. All fields are optional — CLI flags always take precedence.

```toml
dir = "./my-vault"        # vault directory (default: ".")
format = "text"           # output format: "json" or "text" (default: TTY-aware — text on terminals, json when piped)
hints = false             # drill-down command hints (default: true)
default_limit = 100       # max results for list commands (default: 50; 0 = unlimited)

[links]
frontmatter_properties = ["related", "depends-on"]   # list properties that contribute to the link graph
case_insensitive = "auto"                             # "auto", "true", or "false"

[links.auto]
exclude_titles = ["permissions", "README"]            # titles hyalo links auto never links
exclude_target_globs = ["templates/*"]                # pages hyalo links auto never links to
first_only = true                                     # link only the first mention per file
warn_common_titles = true                             # note when a candidate title is a common word

[schema.default]
required = ["title"]

[schema.types.iteration]
required = ["title", "date", "status", "tags"]
filename-template = "iterations/iteration-{n}-{slug}.md"

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed", "superseded"]
```

## Case-insensitive link resolution

`[links] case_insensitive` controls whether a link whose target differs only in
casing (`[[Foo]]` → `foo.md`) still resolves:

| Value | Behaviour |
| --- | --- |
| `"auto"` (default) | Detect the filesystem's case behaviour once per run and follow it |
| `"true"` | Always resolve case-insensitively |
| `"false"` | Never resolve case-insensitively — casing must match exactly |

Under `"auto"`, hyalo detects case behaviour with **stat calls only**: it looks
up an existing vault entry (or the vault directory itself) under a
case-flipped name and checks whether it lands on the same object. Read-only
commands therefore write nothing into the vault — no probe file, no directory
mtime change.

The one case that still needs a write is a vault that offers no usable
candidate at all: an empty directory whose own path has no ASCII letters. Then
hyalo falls back to creating and deleting a short-lived `.hyalo-case-probe-*`
file in the vault root. **If that write fails — a read-only mount, for example
— case-insensitive resolution silently turns off** and links are resolved
case-sensitively, which can report casing-mismatched links as broken. Set
`case_insensitive = "true"` explicitly when working against a read-only vault
on a case-insensitive filesystem.

If a process is killed between creating and deleting a fallback probe file,
the orphan is dot-prefixed and invisible to `hyalo find`; the next
`hyalo create-index` run sweeps `.hyalo-case-probe-*` files older than a
minute from the vault root.

## Persistent auto-link exclusions

`hyalo links auto` links unlinked mentions of page titles. On a vault whose
titles double as common words (`permissions`, `index`, `README`) most
candidates are noise, and the flags that suppress them had to be retyped on
every invocation. `[links.auto]` persists them per vault:

```toml
[links.auto]
exclude_titles = ["permissions", "README", "index"]   # like --exclude-title (case-insensitive)
exclude_target_globs = ["templates/*"]                # like --exclude-target-glob
first_only = true                                     # like --first-only
warn_common_titles = false                            # opt out of the common-word note
```

Merge rules with the CLI flags:

| Setting | With flags |
| --- | --- |
| `exclude_titles` | **Unioned** with `--exclude-title` — the flag extends the config list, never replaces it |
| `exclude_target_globs` | **Unioned** with `--exclude-target-glob` |
| `first_only` | `--first-only` turns it on for a single run; the config turns it on for every run; `--no-first-only` turns it off for a single run |
| `warn_common_titles` | On by default; `--no-warn-common-titles` turns it off for a single run, the config for every run |

`first_only` is the one key with a counter-flag: `--no-first-only` forces
first-mention-only **off** for a single run, so a vault that persists
`first_only = true` can still get a one-off all-mentions pass without editing
`.hyalo.toml`. It conflicts with `--first-only` — passing both is an error
rather than a silent precedence puzzle — and is a no-op when the config does
not enable `first_only`. The list keys have no counter-flag: they are unioned,
and narrowing a run's scope with `--file`/`--glob` covers the same need.

Because config exclusions apply silently, a run whose candidates they removed
reports how many: `config_excluded` in the JSON envelope (omitted when zero,
like `links.out_of_vault`) and an `Excluded by [links.auto] config: N titles`
line in text output. `hyalo config` prints the effective settings as
`links.auto.exclude_titles` / `links.auto.exclude_target_globs` /
`links.auto.first_only` / `links.auto.warn_common_titles` (and
`results.links_auto` in JSON).

### The common-word title note

Exclusions only help once you have noticed the noise. On a first run, `hyalo
links auto` also checks whether any proposed link came from a page whose title
is an ordinary English word or a generic doc filename (`permissions`, `index`,
`notes`, `README`) and, if so, prints one advisory line on stderr:

```text
note: 2 auto-link candidate titles are common English words and account for 31 of 33
proposed links: "permissions" (24×), "index" (7×). If those are prose mentions rather
than deliberate references, skip them with --exclude-title permissions
--exclude-title index — or persist them once under [links.auto] exclude_titles in
.hyalo.toml. Silence this note with --no-warn-common-titles.
```

Properties of the check, all deliberate:

- **stderr only.** The report on stdout is byte-identical with the note enabled
  or disabled, so no JSON consumer or diff-based workflow changes.
- **Self-extinguishing.** Only titles that actually produced matches in *this*
  run are named. Excluding them — by flag or config — removes the note.
- **Match-count honest.** The counts quoted are the links being offered, not an
  estimate over the title inventory.
- **Silenced by `-q`** like every other note, or permanently by
  `warn_common_titles = false`.
- **Bundled word list, no dependency.** The list lives in
  `hyalo-core::common_words` and covers high-frequency English words (plus
  regular plurals, so `permissions` matches the stored `permission`) and generic
  doc filenames. Titles shorter than 3 characters are never classified —
  `--min-length` already excludes them.

`hyalo links fix` has a similar per-invocation filter spelled
`--ignore-target <substring>`. It matches link *targets* by substring rather
than page titles or paths, so it is deliberately not part of `[links.auto]` and
keeps its own name.

## Schemas

Schemas support typed properties (`string`, `date`, `datetime`, `datetime-tz`, `number`, `boolean`, `list`, `enum`, `string-list` — with regex patterns, enum values, and length bounds), per-type filename templates, path-bound types (`[[schema.bind]]`) that apply a schema to a subtree without explicit `type:` frontmatter, and reserved-file exemptions (`[schema] exempt`). Manage schemas from the CLI with `hyalo types list|show|set`, validate with `hyalo lint`, and inspect the resolved configuration with `hyalo config`.

## Saved views

Name a filter set once, recall it everywhere:

```sh
hyalo views set drafts --property status=draft
hyalo find --view drafts                          # recall
hyalo find --view drafts --tag rust               # extend with additional filters
```

## CWD-aware behaviour

When you run hyalo from a directory that has a `.hyalo.toml`, it becomes _context-aware_:

- **`hyalo --help`** prepends a short banner confirming which vault `dir` is active — useful when working from shell history or AI agent loops. Banner emojis (`ℹ️ `/`⚠️`) are TTY-gated: piped output is plain text.
- **`hyalo --version`** appends `(kb dir: <dir>)` so the resolved directory is visible at a glance. The base version string also includes the git short-sha and commit date when hyalo was built from a checkout — e.g. `hyalo 0.20.0 (abc123def456 2026-05-26)`. A `+dirty` suffix marks builds made with uncommitted changes. Set `CARGO_HYALO_FORCE_NO_GIT=1` at build time to force the bare semver form.
- **`hyalo summary`** includes the resolved `kb dir:` as its first output line. The `--format json` envelope exposes the same value as a top-level `dir` field alongside `total`, `tags`, `properties`, etc.
- **`hyalo config`** prints the full resolved configuration — handy for debugging `.hyalo.toml` resolution or feeding config into an LLM context. `--format json` uses the standard envelope, so `hyalo config --jq '.results.dir'` works like it does everywhere else; the config's own hints switch is reported as `results.hints_enabled` so it never collides with the envelope's `hints` array. `dir` is also hoisted to the envelope root.
- Running from _inside_ the vault directory emits a warning banner suggesting you `cd ..` to the project root so hyalo can find `.hyalo.toml`.
- Passing `--dir <path>` when it already matches `.hyalo.toml` emits a one-time `note:` that `--dir` is redundant.

## Snapshot index

For workflows that run many queries in a short window (CI, automation, LLM tool loops):

```sh
hyalo create-index          # one scan → .hyalo-index
hyalo find --index ...      # instant queries, no disk scan
hyalo drop-index            # clean up
```

Mutations with `--index` patch the index in-place, keeping it current for subsequent queries — and hyalo suggests creating an index automatically once a vault grows past ~500 files.

Every command documents its flags and semantics in detail: `hyalo <cmd> --help`.
