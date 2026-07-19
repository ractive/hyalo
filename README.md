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

## Installation

### Homebrew (macOS & Linux)

```sh
brew trust --formula ractive/tap/hyalo   # Homebrew 6+: third-party taps need one-time trust
brew install ractive/tap/hyalo
```

Covers macOS (Apple Silicon) and Linux (x86_64 and ARM64). The Linux binaries are statically linked against musl, so they have no glibc dependency.

Homebrew 6 introduced [tap trust](https://docs.brew.sh/Tap-Trust): formulae
from third-party taps refuse to load until trusted. `brew trust --formula`
scopes the trust to just this formula; `brew trust ractive/tap` trusts the
whole tap instead.

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

### AUR (Arch Linux)

```sh
yay -S hyalo-bin    # or: paru -S hyalo-bin
```

Installs the prebuilt release binary (x86_64 and ARM64). Without an AUR
helper:

```sh
git clone https://aur.archlinux.org/hyalo-bin.git
cd hyalo-bin && makepkg -si
```

The [hyalo-bin](https://aur.archlinux.org/packages/hyalo-bin) package is
updated automatically on every release.

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
  gh attestation verify hyalo-v0.20.0-aarch64-apple-darwin.tar.gz --owner ractive
  ```

A `SHA256SUMS` file with checksums for every asset is attached to each release.

> **Intel Mac users:** Homebrew and the prebuilt macOS archive target Apple Silicon only. Use `cargo install hyalo-cli` above.

### Shell completions

The system packages (apt/dnf and the standalone `.deb`/`.rpm`) install completions automatically. For the Homebrew, Scoop, cargo, or tarball routes, either copy the scripts from the archive's `completions/` directory or generate them on the fly:

```sh
hyalo completions bash > ~/.local/share/bash-completion/completions/hyalo
hyalo completions zsh  > ~/.local/share/zsh/site-functions/_hyalo
hyalo completions fish > ~/.config/fish/completions/hyalo.fish
```

`hyalo completions --help` lists every supported shell (also elvish and powershell).

## Quick start

```sh
# Point hyalo at the folder that contains your .md files
# (omit --dir if the project root itself is the knowledgebase)
hyalo init --dir docs

# Bird's-eye view: file count, tags, properties, link health
hyalo summary

# Full-text search (BM25 ranked, boolean operators) and frontmatter filters
hyalo find "retry OR timeout -deprecated"
hyalo find --property status=draft --tag research

# Bulk-update metadata
hyalo set --property status=reviewed --where-tag research

# Move or rename — every [[wikilink]] and [markdown](link) across the vault is rewritten
hyalo mv old/path.md archive/path.md

# Detect and repair broken links
hyalo links fix --apply

# Convert unlinked mentions of known page titles into [[wikilinks]]
hyalo links auto --apply

# Validate frontmatter against your schema and the markdown body against bundled lint rules
hyalo lint
hyalo lint --fix     # apply autofixes

# Scaffold a new file from a type schema
hyalo new --type iteration --file iterations/iter-99-example.md
```

Every write command supports `--dry-run` to preview changes before applying them, and every command documents its flags: `hyalo <cmd> --help`.

### Agent loop: new → edit → lint

`hyalo new` creates a skeleton file with `TBD` placeholders that are intentionally
invalid — they will fail `hyalo lint`. This is by design. The loop is:

1. `hyalo new --type <name> --file <path>` — scaffold the skeleton
2. Edit the file to fill in the real values
3. `hyalo lint --file <path>` — see which placeholders still violate the schema

The lint output tells you exactly what to fix, field by field.

## Claude Code integration

```sh
hyalo init --claude
```

This installs two [skills](https://docs.anthropic.com/en/docs/claude-code/skills) and a [rule](https://docs.anthropic.com/en/docs/claude-code/settings#rules) that teach Claude Code to use hyalo instead of raw `Read`/`Edit`/`Grep`/`Glob` when working with your vault:

**`hyalo` skill** — Auto-triggered whenever Claude touches markdown files in your vault. It uses `hyalo find`, `hyalo set`, `hyalo mv`, etc. for structured access to frontmatter, tags, links, and tasks.

**`hyalo-tidy` skill** (`/hyalo-tidy`) — A five-phase knowledgebase consolidation. Think of it as a librarian doing a periodic shelf-read: it orients with `hyalo summary`, gathers recent signal from git history, detects structural issues (broken links, orphan files, stale statuses, missing metadata), applies conservative fixes, and reports a health dashboard. Run it periodically to keep your vault clean.

**`knowledgebase` rule** — Scoped to `<your-vault>/**`. Reminds Claude to prefer hyalo CLI commands over built-in file tools whenever it touches vault files.

All artifacts are idempotent — re-running `hyalo init --claude` updates to the latest versions. `hyalo deinit` removes everything cleanly.

## Profiles

Profiles are pre-packaged schema and lint configurations for popular markdown conventions. `hyalo init --profile <name>` merges a declarative fragment into `.hyalo.toml`; add `--claude` to also install a bundled Claude Code skill for the convention. Profiles are **composable** and **idempotent**: multiple `--profile` runs deep-merge without clobbering each other or your hand-written config, and `hyalo lint --profile <name>` works as an ephemeral overlay on any checkout (CI, a freshly cloned third-party bundle) with no config file at all.

```sh
hyalo init --profile okf         # Open Knowledge Format bundles
hyalo init --profile madr        # MADR architecture decision records
hyalo init --profile skills      # Agent Skills SKILL.md validation
hyalo init --profile changelog   # Keep a Changelog
```

| Profile | Scope | Binds | Key rules |
| --- | --- | --- | --- |
| `okf` | Whole bundle | `type`-required concepts; `index.md`/`log.md` reserved | Permissive (warn-only): citations, reserved-file structure, augmentation guard |
| `madr` | `docs/decisions/**` | `adr` schema, status lifecycle, `NNNN-slug.md` | `MADR-SUPERSEDE-RESOLVE`, `MADR-DUPLICATE-NUMBER` (warn) |
| `skills` | `**/SKILL.md` | `skill` schema (`name`/`description` bounds) | `SKILL-RESERVED-NAME` (error), name↔dir + line-budget (warn) |
| `changelog` | `CHANGELOG.md` | frontmatter-less `changelog` type | `CHANGELOG-*` grammar (mostly error), empty-section + link-ref (warn) |

**`okf` — [Open Knowledge Format](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md).** Vendor-neutral "knowledge bundles": a directory of Markdown concept files whose frontmatter requires exactly one field (`type`), with reserved frontmatter-free `index.md`/`log.md` files. The profile sets up the schema, reserved-file exemptions, and bundle-absolute links; `hyalo lint --profile okf` validates a bundle in the spec's permissive-consumption spirit — errors only where the spec demands them, warnings for everything else (broken cross-links, reserved-file drift, citations). The `hyalo okf index` and `hyalo okf log` generators maintain the reserved files deterministically: they rebuild each directory's `index.md` link list and prepend dated `log.md` entries inside managed marker regions, preserving hand-written prose around them — a file with malformed markers is never rewritten, so a generator can never delete your prose. Generators are dry-run by default and exit non-zero on drift, which doubles as a CI freshness check.

**`madr` — [MADR](https://adr.github.io/madr/) architecture decision records.** One decision per file under `docs/decisions/`, named `NNNN-slug.md`, with a status lifecycle. The profile binds an `adr` schema to that subtree (no `type:` frontmatter needed), `hyalo new --type adr` scaffolds a decision with the MADR-4 sections, and `hyalo madr toc` maintains a table-of-contents dashboard in `docs/decisions/README.md`. Advisory rules catch dangling `superseded by` references and duplicate ADR numbers.

**`skills` — [Agent Skills](https://agentskills.io/specification) SKILL.md validation.** The spec's frontmatter is unusually strict (slug-pattern `name`, length-bounded `description`), which makes hyalo a fast, CI-friendly validator for a whole skill collection: `hyalo lint --profile skills`. `hyalo new --type skill` scaffolds a compliant `SKILL.md`, and the profile re-admits the hidden `.claude/skills/**` directory so the canonical Claude Code skill location is reachable in place.

**`changelog` — [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).** Enforces the full `CHANGELOG.md` grammar (version and date ordering, the six change categories, footer link references) as `CHANGELOG-*` lint rules, and maintains the file deterministically: `hyalo changelog add` appends an entry under `[Unreleased]`, and `hyalo changelog release X.Y.Z` rotates it into a dated version section — hyalo's own releases are cut this way. Works with a repo-root `CHANGELOG.md` even when your vault lives in a subdirectory.

Each profile's full behaviour — schemas, generator edge cases, individual rules — is documented in its bundled skill (`hyalo init --profile <name> --claude`), in `hyalo lint-rules list`, and in `hyalo <cmd> --help`.

## Lint your vault in CI (GitHub Actions)

Gate every change on a clean vault. The [`setup-hyalo`](https://github.com/ractive/setup-hyalo) action installs the prebuilt release binary in seconds — no compilation — so a full-vault gate is two steps:

```yaml
# .github/workflows/lint-kb.yml
name: Lint knowledgebase
on:
  push:
    branches: [main]
jobs:
  lint-kb-full:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: ractive/setup-hyalo@v1          # installs hyalo, adds it to PATH
      - run: hyalo lint --strict --format github
```

Pin the binary with `with: { version: v0.20.0 }` (the default `latest` tracks the newest release). Run the job from the repo root: `.hyalo.toml` supplies the vault `dir`, and annotation paths are emitted relative to the repository root so they land on the right file and line.

On a **pull request**, use the diff-aware variant instead: pipe `git diff` into `--files-from -` to lint only the files the PR touches (non-markdown paths are skipped and vault-dir prefixes are stripped automatically — no pre-filtering needed). Check out with `fetch-depth: 0` so the merge-base is available:

```yaml
jobs:
  lint-kb:
    if: github.event_name == 'pull_request'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0                      # full history for the three-dot merge-base
      - uses: ractive/setup-hyalo@v1
      - run: |
          git fetch --quiet origin "${{ github.base_ref }}"
          git diff --name-only --diff-filter=d "origin/${{ github.base_ref }}...HEAD" \
            | hyalo lint --strict --format github --files-from -
```

Three-dot `origin/<base>...HEAD` diffs against the *merge-base* of the PR base and HEAD, so a stale branch stays scoped to the files it changed — not to base-tip drift it never touched. The diff-aware shape matters because GitHub registers at most **10 error + 10 warning inline annotations per step**: scoping to the PR's own files spends that budget on findings the author can act on, while the full-vault job (on `push` to main, where the cap is irrelevant) catches the cross-file regressions a diff can't see — a deleted note others link to, a schema change.

`--format github` emits one [GitHub Actions workflow command](https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions) per violation — `::error` / `::warning` lines that render as inline annotations on the PR diff — in deterministic `(path, line, rule)` order, appending a `::notice::` with the true totals whenever the counts exceed GitHub's inline cap. `--strict` promotes warnings to errors — including `HYALO006` broken links — so the job fails on a broken link; unparseable frontmatter is always an error (`HYALO005`), so a green lint means the vault is genuinely clean.

For **OKF** vaults, add a reserved-file drift check — `hyalo okf index` is dry-run by default and exits non-zero when any `index.md` is stale:

```yaml
      - run: hyalo okf index   # dry-run; non-zero exit on drift
```

### `@claude` agent on GitHub (claude-code-action)

Because [`setup-hyalo`](https://github.com/ractive/setup-hyalo) puts the binary on
`PATH` before the agent runs, a [`claude-code-action`](https://github.com/anthropics/claude-code-action)
workflow can hand `@claude` the full hyalo toolbox — so an `@claude` mention on a
PR or issue can triage and *fix* lint findings (`hyalo lint --fix`, `hyalo set`,
`hyalo mv`) rather than just report them. Commit the hyalo skill first with
`hyalo init --claude` (add `--profile okf` for an OKF bundle) so the agent knows
to prefer the CLI over raw file edits.

```yaml
# .github/workflows/claude.yml
name: claude
on:
  issue_comment:
    types: [created]
jobs:
  claude:
    if: contains(github.event.comment.body, '@claude')
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
    steps:
      - uses: actions/checkout@v4
      - uses: ractive/setup-hyalo@v1          # hyalo on PATH before the agent starts
      - uses: anthropics/claude-code-action@v1
        with:
          anthropic_api_key: ${{ secrets.ANTHROPIC_API_KEY }}
          # Let the agent run any hyalo subcommand (lint/find/set/mv/task/...).
          allowed_tools: Bash(hyalo:*)
```

The committed skill routes the agent to commands like
`hyalo lint --strict --format github` (see findings), `hyalo lint --fix`
(auto-fix the fixable rules), and `hyalo set <file> --property status=done`
(targeted frontmatter edits).

hyalo's own `lint-kb`/`lint-kb-full` jobs in [.github/workflows/ci.yml](.github/workflows/ci.yml) are the living reference.

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

Schemas support typed properties (`string`, `date`, `datetime`, `datetime-tz`, `number`, `boolean`, `list`, `enum`, `string-list` — with regex patterns, enum values, and length bounds), per-type filename templates, path-bound types (`[[schema.bind]]`) that apply a schema to a subtree without explicit `type:` frontmatter, and reserved-file exemptions (`[schema] exempt`). Manage schemas from the CLI with `hyalo types list|show|set`, validate with `hyalo lint`, and inspect the resolved configuration with `hyalo config`.

### Snapshot index

For workflows that run many queries in a short window (CI, automation, LLM tool loops):

```sh
hyalo create-index          # one scan → .hyalo-index
hyalo find --index ...      # instant queries, no disk scan
hyalo drop-index            # clean up
```

Mutations with `--index` patch the index in-place, keeping it current for subsequent queries — and hyalo suggests creating an index automatically once a vault grows past ~500 files.

## Building from source

```sh
cargo build --release
```

## Releasing

1. Bump the workspace version in `Cargo.toml`
2. Rotate the changelog with hyalo itself: `hyalo changelog release X.Y.Z --apply` (then replace the `TBD` footer link with the real compare URL)
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
