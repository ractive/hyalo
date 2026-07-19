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

Gate every change on a clean vault — the [`setup-hyalo`](https://github.com/ractive/setup-hyalo) action installs the prebuilt binary in seconds, so a full-vault gate is two steps:

```yaml
# .github/workflows/lint-kb.yml
name: Lint knowledgebase
on: [push, pull_request]
jobs:
  lint-kb:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: ractive/setup-hyalo@v1   # installs hyalo, adds it to PATH
      - run: hyalo lint --strict --format github
```

Findings render as inline annotations on the PR diff, and `--strict` makes broken links fail the job. The diff-aware pull-request variant, the OKF drift check, and an `@claude` review setup (claude-code-action with the full hyalo toolbox) are covered in [docs/ci.md](docs/ci.md).

## Configuration

`hyalo init` creates a `.hyalo.toml` in your project root. All fields are optional — CLI flags always take precedence:

```toml
dir = "./my-vault"        # vault directory (default: ".")

[schema.types.iteration]
required = ["title", "date", "status", "tags"]
filename-template = "iterations/iteration-{n}-{slug}.md"
```

Schemas can type and require frontmatter properties, bind types to path patterns, and drive `hyalo new` scaffolding and `hyalo lint` validation. The full reference — config keys, saved views, CWD-aware behaviour, the snapshot index — lives in [docs/configuration.md](docs/configuration.md).

## Building from source

```sh
cargo build --release
```

Maintainer docs — the release process and package-repository hosting — live in [docs/releasing.md](docs/releasing.md).

## License

MIT — this repository contains code generated in whole or in part by AI systems under human supervision. See [AI_NOTICE](AI_NOTICE) for details.

> "Hyalo" — from [hyaloclastite](https://en.wikipedia.org/wiki/Hyaloclastite) — is a volcanic glass, just like obsidian.
