# hyalo

[![crates.io](https://img.shields.io/crates/v/hyalo-cli?logo=rust)](https://crates.io/crates/hyalo-cli)
[![GitHub release](https://img.shields.io/github/v/release/ractive/hyalo?logo=github)](https://github.com/ractive/hyalo/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](#license)

**A structured CLI for markdown knowledgebases — built for humans and AI agents.**

If you maintain an [Obsidian](https://obsidian.md/) vault, a Zettelkasten, documentation site, or any folder of `.md` files with YAML frontmatter, you've probably hit the limits of `grep` and manual editing. Hyalo gives you a fast, structured way to search, filter, and bulk-edit your markdown files from the command line.

Hyalo does not define how you organize your notes. It works with the structure you already have — frontmatter properties, tags, `[[wikilinks]]`, markdown links, task checkboxes — and gives you powerful tools to query and maintain it at scale.

### The LLM Wiki pattern

Andrej Karpathy popularized the idea of an [LLM-maintained wiki](https://x.com/karpathy/status/1908527375407042770): instead of asking an LLM the same questions repeatedly, you have it build and maintain a persistent, structured knowledgebase that compounds over time. Every source ingested, every question answered adds to the wiki rather than vanishing with the conversation.

Hyalo is the tooling layer that makes this practical. An LLM agent can use `hyalo find` to search across thousands of notes by metadata, full-text, or regex. It can use `hyalo set` to bulk-update frontmatter, `hyalo mv` to reorganize files while keeping all links intact, and `hyalo lint` to enforce schema consistency — all without ever touching raw files or guessing at YAML syntax.

### What it does

| | |
|---|---|
| **Search** | Full-text search with BM25 ranking, regex, frontmatter filters, tag/section/task queries |
| **Mutate** | Set, remove, or append to properties and tags — one file or hundreds at once |
| **Move** | Rename or reorganize files; hyalo rewrites all `[[wikilinks]]` and `[markdown](links)` across the vault |
| **Fix links** | Detect broken links and auto-repair them with fuzzy matching |
| **Validate** | Lint frontmatter against type schemas, auto-fix defaults, typos, and date formats |
| **Overview** | Property/tag distributions, task counts, orphan files, link health at a glance |

### Why hyalo?

- **Fast.** Parallel scanning, streaming I/O, optional snapshot index. Handles 10,000+ file vaults in under a second.
- **Structured output.** TTY-aware: compact `text` for terminals, `json` when piped — with built-in `--jq` support. Easy to pipe into scripts, CI, or AI agents.
- **AI-agent friendly.** Designed as a tool for [Claude Code](https://claude.ai/claude-code) and other LLM coding agents. One command sets up the integration: `hyalo init --claude`.
- **Safe mutations.** Dry-run mode on all write operations. Preview before committing changes.
- **Cross-platform.** Works on macOS, Linux, and Windows. No runtime dependencies.

## Quick start

```sh
# Initialize: point hyalo at the folder that contains your .md files with the --dir flag.
# This is typically a subfolder like docs/, wiki/, or knowledgebase/.
# Omit --dir if the project root itself is the knowledgebase.
hyalo init --dir docs

# Inspect the effective configuration (effective dir, config path, hints, format, site_prefix)
hyalo config                             # text by default on a terminal
hyalo config --format json               # structured output for scripting / LLM agents

# Get a bird's-eye view (output format auto-detected: text on terminals, json when piped)
# Includes the resolved `kb dir` as the first line.
hyalo summary

# Full-text search (BM25 ranked, with boolean operators)
hyalo find "retry backoff"
hyalo find "retry OR timeout -deprecated"

# Filter by frontmatter
hyalo find --property status=draft --tag research

# Bulk-update metadata
hyalo set --property status=reviewed --where-tag research

# Move a single file — all inbound/outbound links and self-links are updated
hyalo mv --file old/path.md --to archive/path.md

# Batch move: preview files that would move (dry-run by default)
hyalo mv --glob 'iterations/*.md' --property status=completed --to iterations/done/

# Batch move: commit changes with --apply (builds link graph once for all files)
hyalo mv --glob 'iterations/*.md' --property status=completed --to iterations/done/ --apply

# Ambiguous bare wikilinks ([[stem]] matching multiple files) are skipped by default;
# pass --allow-ambiguous to rewrite them anyway based on stem matching
hyalo mv --file old.md --to new.md --allow-ambiguous

# Detect and fix broken links
hyalo links fix --apply

# Auto-link: scan body text for unlinked mentions of known page titles
# and convert them to [[wikilinks]]. Detects titles from filenames,
# frontmatter `title`, and `aliases`. Skips code blocks, existing links,
# headings, and comment fences.
hyalo links auto                     # dry-run preview
hyalo links auto --apply             # write changes
hyalo links auto --first-only --apply  # only first mention per target
hyalo links auto --exclude-title API --exclude-target-glob 'templates/*' --apply
hyalo links auto --file notes/todo.md --apply   # single-file mode

# Lint frontmatter against your schema and markdown body against
# bundled rules (MD001..MD059 from mdbook-lint plus two HYALO native
# cross-cutting rules). `--fix` applies autofixes for both passes,
# writes atomically, and converges in a single run (a second `--fix`
# changes nothing). HYALO002 fires only when any schema declares
# `status` as an enum containing "completed" — this includes the
# default schema as well as any [schema.types.*].
hyalo lint                              # full vault, summary mode
hyalo lint --rule MD013 --detailed      # drill into a single rule
hyalo lint --rule-prefix HYALO          # only HYALO native rules
hyalo lint --strict                     # promote missing-type and undeclared-property warnings to errors
hyalo lint --fix --dry-run              # preview autofixes
hyalo lint --fix                        # apply autofixes
hyalo lint --fix-rule HYALO001          # only autofix one rule

# Manage which rules are enabled and their severity (writes to .hyalo.toml).
hyalo lint-rules list
hyalo lint-rules show MD013
hyalo lint-rules set MD013 --enabled false
hyalo lint-rules set HYALO001 --severity error
hyalo lint-rules remove MD013           # revert to default

# Schema-driven file creation: scaffold a new file from a type schema.
# Add --index (or --index-file PATH) to also insert the entry into an existing
# snapshot index so subsequent --index queries see the new file immediately.
hyalo new --type iteration --file iterations/iter-99-example.md
```

Every write command supports `--dry-run` to preview changes before applying them.

### Agent loop: new → edit → lint

`hyalo new` creates a skeleton file with `TBD` placeholders that are intentionally
invalid — they will fail `hyalo lint`. This is by design. The loop is:

1. `hyalo new --type <name> --file <path>` — scaffold the skeleton
2. Edit the file to fill in the real values
3. `hyalo lint --file <path>` — see which placeholders still violate the schema

The lint output tells you exactly what to fix, field by field.

Run `hyalo --help` or `hyalo <command> --help` for the full reference.

## Claude Code integration

```sh
hyalo init --claude
```

This installs two [skills](https://docs.anthropic.com/en/docs/claude-code/skills) and a [rule](https://docs.anthropic.com/en/docs/claude-code/settings#rules) that teach Claude Code to use hyalo instead of raw `Read`/`Edit`/`Grep`/`Glob` when working with your vault:

**`hyalo` skill** — Auto-triggered whenever Claude touches markdown files in your vault. It uses `hyalo find`, `hyalo set`, `hyalo mv`, etc. for structured access to frontmatter, tags, links, and tasks.

**`hyalo-tidy` skill** (`/hyalo-tidy`) — A five-phase knowledgebase consolidation. Think of it as a librarian doing a periodic shelf-read: it orients with `hyalo summary`, gathers recent signal from git history, detects structural issues (broken links, orphan files, stale statuses, missing metadata), applies conservative fixes, and reports a health dashboard. Run it periodically to keep your vault clean.

**`knowledgebase` rule** — Scoped to `<your-vault>/**`. Reminds Claude to prefer hyalo CLI commands over built-in file tools whenever it touches vault files.

All artifacts are idempotent — re-running `hyalo init --claude` updates to the latest versions. `hyalo deinit` removes everything cleanly.

## Installation

### Homebrew (macOS & Linux)

```sh
brew install ractive/tap/hyalo
```

Covers macOS (Apple Silicon) and Linux (x86_64 and ARM64). The Linux binaries are statically linked against musl, so they have no glibc dependency.

### apt (Debian & Ubuntu)

```sh
curl -sLf 'https://dl.cloudsmith.io/public/ractive/hyalo/cfg/setup/bash.deb.sh' | sudo bash
sudo apt install hyalo
```

The setup script registers the [Cloudsmith](https://cloudsmith.io/~ractive/repos/hyalo)-hosted apt repository; `apt install` then pulls hyalo and picks up future updates through `apt upgrade`. Shell completions are installed system-wide automatically.

### dnf / yum / zypper (Fedora, RHEL & openSUSE)

```sh
curl -sLf 'https://dl.cloudsmith.io/public/ractive/hyalo/cfg/setup/bash.rpm.sh' | sudo bash
sudo dnf install hyalo    # or: yum install hyalo / zypper install hyalo
```

Registers the Cloudsmith-hosted rpm repository. Shell completions are installed system-wide automatically.

### Scoop (Windows)

```powershell
scoop bucket add ractive https://github.com/ractive/scoop-bucket
scoop install hyalo
```

### winget (Windows)

```powershell
winget install ractive.hyalo
```

### Cargo (from crates.io)

```sh
cargo install hyalo-cli    # installs the `hyalo` binary
```

### Manual download

Every [GitHub Release](https://github.com/ractive/hyalo/releases) publishes:

- **Archives** named `hyalo-v<version>-<target>.{tar.gz,zip}` for Linux (x86_64/ARM64, glibc and musl), macOS (Apple Silicon), and Windows (x86_64/ARM64). Each archive contains the binary, `LICENSE`, `README.md`, and a `completions/` directory with bash/zsh/fish scripts.
- **Standalone `.deb` and `.rpm` packages** for users who prefer to install a single downloaded file directly (they install completions system-wide, same as the apt/dnf repos above).
- **CycloneDX SBOMs** (`*.cdx.json`) and GitHub build-provenance attestations for the native builds. Verify an artifact with:

  ```sh
  gh attestation verify hyalo-v0.17.0-aarch64-apple-darwin.tar.gz --owner ractive
  ```

A `SHA256SUMS` file with checksums for every asset is attached to each release.

> **Intel Mac users:** Homebrew and the prebuilt macOS archive target Apple Silicon only. Use `cargo install hyalo-cli` above.

### Shell completions

The system packages (apt/dnf and the standalone `.deb`/`.rpm`) install completions automatically. For the Homebrew, Scoop, cargo, or tarball routes, either copy the scripts from the archive's `completions/` directory or generate them on the fly:

```sh
hyalo completion bash > ~/.local/share/bash-completion/completions/hyalo
hyalo completion zsh  > ~/.local/share/zsh/site-functions/_hyalo
hyalo completion fish > ~/.config/fish/completions/hyalo.fish
```

`hyalo completion --help` lists every supported shell (also elvish and powershell).

## Configuration

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

Supported property types: `string` (with optional `pattern`), `date` (`YYYY-MM-DD`), `datetime` (naive local ISO 8601 — `YYYY-MM-DDThh:mm:ss`, no timezone), `number`, `boolean`, `list`, `enum` (with `values`), and `string-list` (with optional `item_pattern`).

A required property whose value is YAML null (`tags: ~`) or an empty array (`tags: []`) is reported as `required property "tags" must not be empty` — vacuous values are treated as semantically equivalent to absent. This fires regardless of declared constraint type, so `required = ["tags"]` (with `tags` typed as `list`) is the idiomatic way to enforce non-empty tags; there's no separate `min_items` knob. Atomic-typed required properties (`string`, `date`, `number`, ...) only need to be present — an empty string or zero still satisfies them.

See `hyalo types --help` for managing schemas from the CLI, and `hyalo lint` to validate your vault against them.

### CWD-aware behaviour

When you run hyalo from a directory that has a `.hyalo.toml`, it becomes _context-aware_:

- **`hyalo --help`** prepends a short banner confirming which vault `dir` is active — useful when working from shell history or AI agent loops. Banner emojis (`ℹ️ `/`⚠️`) are TTY-gated: piped output is plain text.
- **`hyalo --version`** appends `(kb dir: <dir>)` so the resolved directory is visible at a glance. The base version string also includes the git short-sha and commit date when hyalo was built from a checkout — e.g. `hyalo 0.16.0 (abc123def456 2026-05-26)`. A `+dirty` suffix marks builds made with uncommitted changes. Set `CARGO_HYALO_FORCE_NO_GIT=1` at build time to force the bare semver form.
- **`hyalo summary`** includes the resolved `kb dir:` as its first output line. The `--format json` envelope exposes the same value as a top-level `dir` field alongside `total`, `tags`, `properties`, etc.
- **`hyalo config`** prints the full resolved configuration — handy for debugging `.hyalo.toml` resolution or feeding config into an LLM context.
- Running from _inside_ the vault directory emits a warning banner suggesting you `cd ..` to the project root so hyalo can find `.hyalo.toml`.
- Passing `--dir <path>` when it already matches `.hyalo.toml` emits a one-time `note:` that `--dir` is redundant.

### `--file` path semantics

Subcommands that accept `--file <path>` (`find`, `set`, `backlinks`, `read`, `links`, `mv`) accept either a vault-relative path or an absolute path that points _inside_ the configured vault. Absolute in-vault paths are canonicalised to the vault-relative form (with a one-time stderr warning, since pasting absolute paths is usually an LLM accident). An absolute path that resolves _outside_ the vault exits non-zero with `error: file resolves outside vault boundary` rather than silently returning an empty result set.

### Saved views

Name a filter set once, recall it everywhere:

```sh
hyalo views set drafts --property status=draft
hyalo find --view drafts                          # recall
hyalo find --view drafts --tag rust               # extend with additional filters
```

### CI diff-aware lint

`--files-from <PATH>` (or `-` for stdin) scopes any command to a caller-supplied file list,
bypassing the directory walk entirely. This is ideal for linting only changed files in CI:

```sh
# Lint only the markdown files touched on this branch
git diff --name-only origin/main -- '**/*.md' | hyalo lint --files-from -

# Non-.md paths (build artifacts, source files) are silently skipped —
# no need to pre-filter git diff output. Repo-relative paths that start with
# the vault directory prefix (e.g. `hyalo-knowledgebase/notes/foo.md` when
# the vault is configured as `dir = "hyalo-knowledgebase"`) are stripped
# automatically — so the recipe above works whether the vault is the repo
# root or a subdirectory.
# Counters in the JSON envelope show what was skipped (under `.results`):
git diff --name-only origin/main | hyalo lint --files-from - --format json \
  | jq '{missing: .results.files_missing, non_md: .results.files_skipped_non_md}'
```

`--files-from` is available on `find`, `lint`, `mv`, `set`, `remove`, `append`,
`task toggle`, `task set`, `task read`, `read`, and `backlinks`.
It is mutually exclusive with `--glob` and `--file`.

`--glob` is accepted on all file-taking commands except `read`, `backlinks`,
and `task read` (which are single-file commands and will return an error if
`--glob` is used).

### Snapshot index

For workflows that run many queries in a short window (CI, automation, LLM tool loops):

```sh
hyalo create-index          # one scan → .hyalo-index
hyalo find --index ...      # instant queries, no disk scan
hyalo drop-index            # clean up
```

Mutations with `--index` patch the index in-place, keeping it current for subsequent queries.

hyalo surfaces the index recommendation automatically: if a command takes longer than 500 ms or `hyalo summary` reports more than 500 files, a hint appears suggesting `hyalo create-index`. Both hints are suppressed when `--index`/`--index-file` is already in use, when `--quiet` is set, or when `--no-hints` is passed.

## Building from source

```sh
cargo build --release
```

## Releasing

1. Bump the workspace version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Cut the release: `gh release create vX.Y.Z --generate-notes`

Publishing the release triggers [`release.yml`](.github/workflows/release.yml), a thin caller for the shared reusable pipeline in [ractive/release-workflows](https://github.com/ractive/release-workflows). From a single tag, it:

- builds and tests seven targets (Linux x86_64/ARM64 in both glibc and musl, macOS Apple Silicon, Windows x86_64/ARM64);
- packages versioned archives, plus `.deb`/`.rpm` packages, and publishes them to the hosted apt/yum repos at Cloudsmith;
- publishes the crates to crates.io (with retry) and updates the Homebrew tap, Scoop bucket, and winget manifest;
- emits CycloneDX SBOMs and GitHub build-provenance attestations for the native builds.

Rehearse the whole thing without publishing anything via `gh workflow run release.yml` — a `workflow_dispatch` run builds and packages every target as a full dry run. If a downstream step needs to be re-run after a release, [`publish-crates.yml`](.github/workflows/publish-crates.yml) re-publishes to crates.io and [`cloudsmith-republish.yml`](.github/workflows/cloudsmith-republish.yml) backfills the Cloudsmith repos.

## Package repository hosting

[![OSS hosting by Cloudsmith](https://img.shields.io/badge/OSS%20hosting%20by-cloudsmith-blue?logo=cloudsmith&style=flat-square)](https://cloudsmith.com)

Package repository hosting is graciously provided by [Cloudsmith](https://cloudsmith.com).
Cloudsmith is the only fully hosted, cloud-native, universal package management solution, that
enables your organization to create, store and share packages in any format, to any place, with total
confidence.

## License

MIT — this repository contains code generated in whole or in part by AI systems under human supervision. See [AI_NOTICE](AI_NOTICE) for details.

> "Hyalo" — from [hyaloclastite](https://en.wikipedia.org/wiki/Hyaloclastite) — is a volcanic glass, just like obsidian.
