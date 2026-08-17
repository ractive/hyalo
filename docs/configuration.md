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
