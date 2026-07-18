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

## OKF (Open Knowledge Format)

[OKF](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md) is a vendor-neutral format for **knowledge bundles**: a directory of Markdown "concept" files with YAML frontmatter, distributed as a git repo or tarball. Every concept has exactly one required field — `type` — plus recommended `title`, `description`, `resource`, `tags`, and an RFC 3339 `timestamp`. Reserved `index.md`/`log.md` files are frontmatter-free, except the bundle-root `index.md`, which may carry a single `okf_version` key and nothing else.

hyalo makes OKF a first-class target:

```sh
hyalo init --profile okf            # scaffold an OKF-ready .hyalo.toml
hyalo init --profile okf --claude   # also install the bundled `okf` skill
```

The `okf` profile merges a declarative config fragment into `.hyalo.toml`:

- `[schema.default] required = ["type"]` — the one OKF-mandated field.
- Recommended props declared with types: `title`/`description` (string), `resource` (URL-pattern string), `tags` (list), `timestamp` (`datetime-tz`, offset-aware).
- `[schema] exempt = ["**/index.md", "**/log.md"]` — reserved files skip the required-`type` check.
- `site_prefix = ""` — bundle-absolute links (`/tables/x.md`, the spec-recommended form) resolve from the bundle root.
- `validate_on_write = true` — authoring stays conformant.

The profile is **vendor-neutral**: it ships **no example `[schema.types.*]`** (a real vault's concept `type` values are domain-specific). Declare your own domain types under `[schema.types."<name>"]` — quoted keys with spaces are supported — with recommended `# Schema` / `# Citations` `required_sections`. The bundled `okf` skill's "Adding domain types" section walks through it.

Profiles are **composable** and **idempotent**: multiple `--profile` runs coexist in one vault. The fragment is deep-merged, and array config keys **union** rather than clobber — each profile's `[schema] exempt` globs, `[[schema.bind]]` entries, `[schema.default] required` fields, and `[lint] profiles` markers all accumulate, so a later `init --profile` never shrinks an earlier one's config or your hand-added entries. Hand-written comments and key order survive the merge, re-running is byte-idempotent, and when a profile overwrites a differing **scalar** value it prints a `conflict: <key> "<old>" -> "<new>"` line to stderr — nothing is lost silently. With `--claude`, the bundled `okf` skill teaches Claude the OKF concept model, reserved-file rules, link forms, and the exact hyalo commands — hyalo owns the deterministic frontmatter/link mechanics while the LLM does the semantic work.

### Reserved-file generators (`okf index` / `okf log`)

OKF bundles keep two *derived* reserved files that are otherwise hand-maintained: `index.md` (a Markdown link list of the concepts in each directory) and `log.md` (a date-grouped changelog). hyalo regenerates both deterministically — no LLM, no cloud:

```bash
hyalo okf index --dry-run          # CI: exits non-zero if any index.md is stale
hyalo okf index --apply            # regenerate every directory's index.md
hyalo okf index tables --apply     # scope to a subtree
hyalo okf index --apply --replace  # overwrite a marker-less index.md (destructive)

hyalo okf log --message "Added blocks table" --apply
hyalo okf log tables --action Update --message "Refreshed schema" --apply
```

- **`okf index`** walks each directory, groups its child concepts by frontmatter `type` (untyped concepts fall under `Other`), and emits `* [title](relative-link) - description` lines (title falls back to the filename stem; description optional). Immediate subdirectories are listed under a `Subdirectories` group. The generated list lives inside a stable managed region delimited by `<!-- okf:index:begin -->` / `<!-- okf:index:end -->` markers, so any hand-written prose outside the markers is preserved verbatim across runs. The bundle-root `index.md`'s lone `okf_version` frontmatter key is kept. Links are relative and forward-slashed (cross-platform), and always **CommonMark-valid**: destinations with spaces are angle-bracket wrapped (`](<blocks table.md>)`), `[`/`]` in titles are escaped, and multi-line descriptions are collapsed to one line — so a spaced or unicode filename never renders as literal text on GitHub. Running `--apply` twice is a no-op (idempotent); `--dry-run` (the default) exits non-zero on drift, so it doubles as a **CI freshness check**.
  - **Non-destructive adopt:** an existing `index.md` that has *no* markers is **adopted** — its entire hand-written body is preserved and the managed region is appended after it (dry-run reports `adopt (preserving N existing lines)`). Only `--replace` overwrites such a file with a fresh managed index. On case-insensitive filesystems an existing `INDEX.md` is recognized as the reserved file and adopted by its on-disk casing.
  - **Malformed markers are never rewritten:** a file whose markers are **dangling** (a begin with no end, or an end with no begin), **reversed**, or **duplicated** is left byte-identical and reported (`skip`) with a stderr warning — the generator never splices across a broken marker, so it can never delete the hand prose that follows one (a former data-loss edge). Fix the markers by hand; the companion `OKF-INDEX-MARKERS` lint rule flags the same condition in CI. In `--dry-run` a malformed-marker file counts as drift.
  - **Robust apply:** a single impossible or unwritable target (e.g. a *directory* literally named `index.md`) is warned-and-skipped, and the run continues writing every other index — no partial mid-run abort. Dry-run detects such targets and reports `skip` instead of claiming `create`. The exit code reflects any write failure.
  - **Scoping & malformed files:** files matching an `[okf] ignore` glob in `.hyalo.toml` (e.g. `_template/**`, `test/fixture-vault/**`) are neither indexed nor generated into. A concept whose frontmatter cannot be parsed is skipped with a stderr warning (suppressed by `-q`/`--quiet`) and the run continues (a scoped run never fails on a bad file outside its scope). A nonexistent scope directory is rejected (exit 1) instead of vacuously passing a CI check.
- **`okf log`** prepends a dated entry under today's `YYYY-MM-DD` heading (newest first) to a scope-selectable `log.md`. The `TARGET` argument picks the log: a directory (`TARGET/log.md`), a `log.md` path (written directly), or omitted (the bundle-root `log.md`, per SPEC §7 directory-local scope). `--action Update` prefixes a bold action word (an empty `--action ""` is a user error, like an empty `--message`). A multi-line `--message` keeps the log structure valid — continuation lines are indented under the bullet, so a `## fake heading` inside the message can't break out into a real heading. The file is created (frontmatter-free) when absent, and `TARGET` is validated to stay inside the bundle; a nonexistent directory target is rejected consistently by both dry-run and apply (create it first).

Both generators default to `--dry-run` and mutate only with `--apply`, matching hyalo's `links fix` / `links auto` convention. Configure the generator scope with an `[okf]` section:

```toml
[okf]
ignore = ["_template/**", "test/fixture-vault/**"]  # skip these trees
```

### Validate an OKF bundle (`lint --profile okf`)

`hyalo lint --profile okf` checks a bundle against the OKF [SPEC §9](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md) conformance rules. It is an **ephemeral overlay** — it merges the same fragment `hyalo init --profile okf` writes, in memory, without touching `.hyalo.toml` — so it works on CI or a freshly cloned third-party bundle with no config at all:

```bash
hyalo lint --profile okf                 # validate the whole bundle
git diff --name-only origin/main | hyalo lint --profile okf --files-from -   # scope to a diff in CI
```

The profile honours OKF's **permissive-consumption** model — *warn, don't reject*:

- **Errors only** on the two things SPEC §9 makes hard requirements: a non-reserved `.md` with no parseable frontmatter block, or one whose block lacks a non-empty `type`. Reserved `index.md`/`log.md` are exempt (they are frontmatter-free by design).
- **Warns, never rejects,** on everything the spec says a consumer MUST NOT reject on: broken cross-links, reserved-file structure drift (`index.md` should be a link list; `log.md` should be date-grouped, newest-first), missing or malformed `# Citations`, and augmentation regressions (a `# Schema`/`# Citations` section emptied out).
- **Always accepts** unknown `type` values and extra frontmatter keys.

Advisory citation rules make the `# Citations` convention first-class: `OKF-CITATIONS-PRESENT` (a claim-bearing concept should cite), `OKF-CITATIONS-WELL-FORMED` (entries are links — both numbered `1.` and `-` bullet lists accepted), and `OKF-CITATIONS-RESOLVE` (bundle-relative / `references/…` links resolve on disk; external `http(s)` URLs are surfaced but not network-checked). Each OKF rule is individually toggleable with `hyalo lint-rules set OKF-… --enabled false` and appears in `hyalo lint-rules list`.

Because the overlay reuses the init fragment, a vault created with `hyalo init --profile okf` (which records `[lint] profiles = ["okf"]`) runs the same rules under a plain `hyalo lint` — `--profile okf` on such a vault is a no-op.

## MADR profile — Architecture Decision Records

[MADR](https://adr.github.io/madr/) (Markdown Architecture Decision Records, 4.0.0) is a lightweight convention for capturing architecture decisions as Markdown files. Each decision is one file under `docs/decisions/`, named `NNNN-slug.md`, with a `status` lifecycle (`proposed` → `accepted` / `rejected`, later `deprecated`, or `superseded by ADR-NNNN`).

```sh
hyalo init --profile madr            # scaffold an ADR-ready .hyalo.toml
hyalo init --profile madr --claude   # also install the bundled `madr` skill
```

The `madr` profile is **pure data over the same machinery as `okf`**, plus two generic mechanisms it is the first consumer of:

- **Path-bound schemas (`[[schema.bind]]`)** — the `adr` type is bound to `docs/decisions/**/*.md`, so the schema applies to *that subtree only*, inside any larger vault. Files there need no explicit `type: adr` frontmatter; the binding supplies it (explicit frontmatter always wins, and a `type:` that disagrees with the binding warns). **Bind = typing:** a bound file satisfies a `required = ["type"]` gate through its binding, so a spec-valid frontmatter-less `SKILL.md`, MADR ADR, or `CHANGELOG.md` lints clean without a hand-written `type:` key (its type-specific required properties are still enforced). Bindings are ordered and first-match-wins.
- **Zero-padded filename tokens (`{n:04}`)** — the `adr` `filename-template` is `docs/decisions/{n:04}-{slug}.md`, producing `0007-use-postgres.md`. The pad width is a rendering minimum, so `1-x.md` and `0002-x.md` are both still recognized as ADRs.

The `adr` type keeps MADR's light frontmatter (`status`, `date`, `decision-makers`/`consulted`/`informed` — the 3.x `deciders` spelling is accepted as an alias) optional-but-typed, and requires the MADR-4 short-template sections `## Context and Problem Statement`, `## Considered Options`, `## Decision Outcome`.

```sh
hyalo new --type adr --file docs/decisions/0007-use-postgres.md   # scaffold an ADR
hyalo lint                                                        # validate (profile recorded in config)
hyalo madr toc --apply                                            # regenerate docs/decisions/README.md
```

**`hyalo madr toc`** regenerates an ADR table of contents / status dashboard (number, title, status, date) into `docs/decisions/README.md`, inside a `<!-- madr:toc:begin -->` / `<!-- madr:toc:end -->` managed region (prose outside is preserved). A marker-less `README.md` is adopted non-destructively (its body is kept and the region appended; `--replace` overwrites). Like `okf index`, it defaults to `--dry-run` and exits non-zero on drift, so it doubles as a CI check.

Two advisory (warn-level) lint rules layer on top of the schema pass under `hyalo lint --profile madr`: **`MADR-SUPERSEDE-RESOLVE`** (a `status: superseded by ADR-0123` that points at a non-existent `0123-*.md` warns) and **`MADR-DUPLICATE-NUMBER`** (two ADRs sharing an `NNNN` prefix warn). Both are toggleable via `hyalo lint-rules set MADR-… --enabled false` and appear in `hyalo lint-rules list`.

## Agent Skills profile — SKILL.md validator

The [Agent Skills](https://agentskills.io/specification) spec packages an agent capability as a directory `<skill-name>/SKILL.md` whose frontmatter is unusually strict — which makes hyalo a CI-friendly Rust validator for a whole skill collection.

```sh
hyalo init --profile skills            # scaffold a skills-ready .hyalo.toml
hyalo init --profile skills --claude   # also install the bundled `skills` skill
```

The `skills` profile is **pure data over the same machinery as `okf`/`madr`**, and is the first consumer of a new generic constraint — **string length bounds (`min-length` / `max-length`)** on any `string` property. The `skill` type is path-bound to `**/SKILL.md` (any depth) and requires exactly two fields, both hard-validated by the schema pass:

- **`name`** — a lowercase slug (`^[a-z0-9]+(-[a-z0-9]+)*$`, no leading/trailing/consecutive hyphens), 1–64 characters (`min-length`/`max-length`).
- **`description`** — 1–1024 characters, with no `<…>` tags (it is injected verbatim into a system prompt).

Optional-but-typed fields: `license`, `compatibility` (≤500 chars), `allowed-tools` (a list), and the Claude Code extension `user_invocable` (boolean). The free-form `metadata` map is intentionally left untyped (hyalo treats nested objects as opaque). Companion directories (`scripts/`, `references/`, `assets/`) are a convention you create, not scaffolded.

```sh
hyalo new --type skill --file my-skill/SKILL.md   # scaffold a compliant SKILL.md
hyalo lint --profile skills                        # validate a directory of skills
```

Three advisory rules layer on top of the schema pass under `hyalo lint --profile skills`: **`SKILL-RESERVED-NAME`** (**error** — `name` is a reserved word `anthropic`/`claude`, which the slug pattern cannot express without look-around), **`SKILL-NAME-DIRNAME`** (warn — `name` must equal its parent directory), and **`SKILL-LINE-BUDGET`** (warn — the body should stay under 500 lines). All appear in `hyalo lint-rules list` and are toggleable via `hyalo lint-rules set SKILL-… --enabled false`.

**Reaching `.claude/skills/`.** The vault walker skips hidden dot-directories by default, so the canonical Claude Code skill location would be invisible. The `skills` profile therefore ships `[scan] include = [".claude/skills/**"]`, a general walker escape hatch that re-admits specific hidden subtrees (never `.git`, which is always excluded). With it, `hyalo find`/`lint` reach `.claude/skills/**/SKILL.md` in place — no relocation:

```toml
[scan]
include = [".claude/skills/**"]   # honored by every command that scans the vault
```

## Changelog profile — Keep a Changelog

[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) is a convention for a human-readable `CHANGELOG.md`: a `# Changelog` title, `## [Unreleased]` pinned above dated `## [X.Y.Z] - YYYY-MM-DD` version sections (newest first), `###` subsections limited to six change categories, and a footer of `[x.y.z]: <url>` link references. Unlike the other profiles, `CHANGELOG.md` is **frontmatter-free** — the whole grammar lives in the body.

```sh
hyalo init --profile changelog            # scaffold a changelog-ready .hyalo.toml
hyalo init --profile changelog --claude   # also install the bundled `changelog` skill
```

The `changelog` profile binds a frontmatter-less `changelog` type to the literal `CHANGELOG.md` (via `[[schema.bind]]`) and exempts it from the frontmatter rules; the grammar is enforced by the `CHANGELOG-*` body rules. It is the first consumer of a new generic **heading-grammar** capability (a declarative *sequence / level / text-pattern* engine, lifted from OKF's hand-rolled reserved-file checks), so later profiles (Nygard ADRs, Standard Readme) can declare a grammar rather than re-implement a scanner.

```sh
hyalo lint --profile changelog                          # validate CHANGELOG.md
git diff --name-only origin/main | hyalo lint --profile changelog --files-from -   # CI on a diff
```

The grammar is **stricter than the other profiles** (a malformed changelog is a real defect), so most rules default to **error**: `CHANGELOG-TITLE`, `CHANGELOG-VERSION-HEADING`, `CHANGELOG-CATEGORY`, `CHANGELOG-VERSION-ORDER` (versions strictly descending), `CHANGELOG-DATE-ORDER` (dates non-increasing), and `CHANGELOG-UNRELEASED-POSITION`. Two soft rules warn: `CHANGELOG-EMPTY-SECTION` and `CHANGELOG-LINK-REF` (every version heading needs a matching footer link ref and vice versa). All appear in `hyalo lint-rules list` and are toggleable via `hyalo lint-rules set CHANGELOG-… --enabled false`.

A release generator maintains the file deterministically:

```sh
hyalo changelog add --category Added --message "New export format" --apply   # append under Unreleased
hyalo changelog release 1.2.0 --dry-run                                      # CI: preview the rotation
hyalo changelog release 1.2.0 --apply                                        # rotate [Unreleased] → [1.2.0]
```

**`hyalo changelog release <X.Y.Z>`** rotates the accumulated `## [Unreleased]` content into a dated `## [X.Y.Z] - <date>` section (date defaults to today, override with `--date`), recreates an empty `[Unreleased]` above it, and appends a placeholder `[X.Y.Z]: TBD` footer link reference (replace `TBD` with the real compare/tag URL). It **refuses** to release a version that already exists. **`hyalo changelog add`** appends `- <message>` under the `### <category>` subsection of `[Unreleased]` — always **inside** the section, before the footer link-reference block, with a single trailing newline — creating the subsection if needed. Both default to `--dry-run` and exit non-zero on drift, so they double as CI checks.

**Repo-root `CHANGELOG.md` with a docs-subdir vault.** When your vault dir is a subdirectory (`dir = "docs"`) but `CHANGELOG.md` lives at the repo root, point the changelog commands at it with `[changelog] path` — resolved relative to the config file's directory (it may sit outside the vault dir, but never above the repo root):

```toml
dir = "docs"
[changelog]
path = "CHANGELOG.md"   # the repo-root file, beside .hyalo.toml
```

`hyalo init --profile changelog --dir docs` writes this key automatically when a root `CHANGELOG.md` already exists. With it, `hyalo changelog add`/`release` and `hyalo lint --profile changelog` all operate on the root file with no `--dir .` gymnastics.

### Profiles at a glance

| Profile | Scope | Binds | Key rules |
| --- | --- | --- | --- |
| `okf` | Whole bundle | `type`-required concepts; `index.md`/`log.md` reserved | Permissive (warn-only): citations, reserved-file structure, augmentation guard |
| `madr` | `docs/decisions/**` | `adr` schema, status lifecycle, `NNNN-slug.md` | `MADR-SUPERSEDE-RESOLVE`, `MADR-DUPLICATE-NUMBER` (warn) |
| `skills` | `**/SKILL.md` | `skill` schema (`name`/`description` bounds) | `SKILL-RESERVED-NAME` (error), name↔dir + line-budget (warn) |
| `changelog` | `CHANGELOG.md` | frontmatter-less `changelog` type | `CHANGELOG-*` grammar (mostly error), empty-section + link-ref (warn) |

Profiles are **composable** and **idempotent**: multiple `--profile` runs deep-merge into one `.hyalo.toml` without clobbering each other, `hyalo lint --profile <name>` is an ephemeral overlay that needs no committed config, and a vault initialised with a profile runs its rules under plain `hyalo lint`.

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
hyalo completions bash > ~/.local/share/bash-completion/completions/hyalo
hyalo completions zsh  > ~/.local/share/zsh/site-functions/_hyalo
hyalo completions fish > ~/.config/fish/completions/hyalo.fish
```

`hyalo completions --help` lists every supported shell (also elvish and powershell). `hyalo completion` (singular) remains as a backward-compatible alias.

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

Supported property types: `string` (with optional `pattern`), `date` (`YYYY-MM-DD`), `datetime` (naive local ISO 8601 — `YYYY-MM-DDThh:mm:ss`, no timezone), `datetime-tz` (timezone-aware RFC 3339 — `YYYY-MM-DDThh:mm:ss` plus a `Z` or `±hh:mm` offset, e.g. `2026-05-28T22:44:47+00:00`), `number`, `boolean`, `list`, `enum` (with `values`), and `string-list` (with optional `item_pattern`). `datetime` and `datetime-tz` are disjoint: a naive value never satisfies `datetime-tz`, and a tz-aware value never satisfies `datetime`.

A required property whose value is YAML null (`tags: ~`) or an empty array (`tags: []`) is reported as `required property "tags" must not be empty` — vacuous values are treated as semantically equivalent to absent. This fires regardless of declared constraint type, so `required = ["tags"]` (with `tags` typed as `list`) is the idiomatic way to enforce non-empty tags; there's no separate `min_items` knob. Atomic-typed required properties (`string`, `date`, `number`, ...) only need to be present — an empty string or zero still satisfies them.

**Exempt (reserved) files:** `[schema] exempt = ["**/index.md", "**/log.md"]` lists vault-relative globs for files that are bound to no schema. They skip the missing-`type` warning, required-property checks, and undeclared-property warnings — useful for reserved files such as an [Open Knowledge Format](https://openknowledge.foundation/) bundle's `index.md`/`log.md`. Glob matching is vault-relative and cross-platform (paths are normalized to forward slashes). For bundle-root absolute links (`/x/y.md` resolved from the vault root), set `site_prefix = ""` so only the leading `/` is stripped — this also avoids mis-stripping when a subdirectory shares its name with the vault directory.

Exempt-glob matching honors the vault's resolved `[links] case_insensitive` mode (see below): on a case-insensitive filesystem (the macOS/Windows default, or `case_insensitive = "true"`), a literally-named `INDEX.md` is exempted by `**/index.md` the same way `hyalo okf index` already treats `INDEX.md` as the reserved index file. On a case-sensitive filesystem (or `case_insensitive = "false"`), `INDEX.md` and `index.md` are distinct files and only the latter matches.

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

# Non-.md paths (build artifacts, source files) are skipped —
# no need to pre-filter git diff output. Repo-relative paths that start with
# the vault directory prefix (e.g. `hyalo-knowledgebase/notes/foo.md` when
# the vault is configured as `dir = "hyalo-knowledgebase"`) are stripped
# automatically — so the recipe above works whether the vault is the repo
# root or a subdirectory.
# Counters in the JSON envelope show what was skipped (under `.results`):
git diff --name-only origin/main | hyalo lint --files-from - --format json \
  | jq '{missing: .results.files_missing, non_md: .results.files_skipped_non_md}'
```

Dropped input paths are no longer JSON-only: `--format text` prints a
`note: N input paths missing, M non-markdown skipped` line on stderr and
`--format github` emits the same as a `::notice::` in the job log, so a
diff-scoped run always shows what it left out without piping through `jq`.
An explicitly named `--file` that is excluded by `[lint] ignore` likewise
prints a notice instead of silently reporting `0 files checked`.

**Parse errors fail the check.** A file whose frontmatter cannot be parsed
(invalid YAML, duplicate keys, an oversized scalar) is reported as an
error-severity **`HYALO005` / `frontmatter-parse-error`** violation and counts
toward `files_checked` — it can no longer silently vanish from the scan and
leave a green lint. The rule is listed in `hyalo lint-rules list` and its
severity is configurable via `[lint.rules.HYALO005]`, but no profile downgrades
it. A green `hyalo lint` in CI therefore means the vault is genuinely clean.

`--files-from` is available on `find`, `lint`, `mv`, `set`, `remove`, `append`,
`task toggle`, `task set`, `task read`, `read`, and `backlinks`.
It is mutually exclusive with `--glob` and `--file`.

`--glob` is accepted on all file-taking commands except `read`, `backlinks`,
and `task read` (which are single-file commands and will return an error if
`--glob` is used).

### GitHub PR annotations (`--format github`)

`hyalo lint --format github` emits one [GitHub Actions workflow command] per
violation, so findings render as **inline annotations on the PR diff** — no `jq`
glue required. Errors become `::error`, warnings become `::warning`, and a
one-line `N errors, M warnings in K files` summary is printed at the end so the
job log stays readable. The lint still exits non-zero on errors, failing the
check. `--format github` is lint-only; other subcommands reject it.
Annotations are **never truncated** under `--format github` — the per-rule and
per-file display caps are lifted so every finding lands on the PR, even past the
default 50-file cap.

Under `--fix --dry-run --format github`, violations that `--fix` *would* resolve
are rendered as `::notice` annotations with a `[fixable]` title prefix and the
summary switches to `N fixable, M remaining`, so a dry-run preview reads
distinctly from a plain lint run.

Drop this into a workflow to lint your whole vault on every PR. The
[`setup-hyalo`](https://github.com/ractive/setup-hyalo) action installs the
prebuilt binary in seconds — no compilation — so the whole check is two steps:

> **Note**: [`ractive/setup-hyalo@v1`](https://github.com/ractive/setup-hyalo)
> is published and installs the latest *release* binary. `--format github`
> is not in a release yet (it ships in the next release after v0.17.0), so
> until then run the lint step as plain `hyalo lint --strict` — same pass/fail
> behavior, just without inline PR annotations.

```yaml
# .github/workflows/lint-kb.yml
name: Lint knowledgebase
on:
  pull_request:
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: ractive/setup-hyalo@v1          # installs hyalo, adds it to PATH
      # Run from the repo root so annotation paths resolve against the workspace.
      # `.hyalo.toml` sets `dir = "..."`, so `hyalo lint` targets the vault.
      - run: hyalo lint --strict --format github
```

Pin the binary with `with: { version: v0.17.0 }`; the default `latest` tracks the
newest release. If you'd rather not depend on the action, install hyalo any other
way (Homebrew, `cargo-binstall`, `cargo install hyalo-cli --locked`, or a release
binary — see [Installation](#installation)) before the lint step.

Paths are emitted **relative to the repository root**: vault-relative paths are
prefixed with the vault dir's path relative to the current directory, so the job
**must run from the repo root** for annotations to land on the right file/line.

For a diff-aware variant that only annotates files touched on the branch, combine
it with `--files-from -`:

```yaml
      - run: |
          git fetch origin main --depth=1
          git diff --name-only origin/main -- '**/*.md' \
            | hyalo lint --strict --files-from - --format github
```

For **OKF** vaults, add an optional second step to catch reserved-file
(`index.md` / `log.md`) drift — `hyalo okf index` is dry-run by default and exits
non-zero when the on-disk artifacts are stale:

```yaml
      - name: Check OKF reserved-file drift
        run: hyalo okf index   # dry-run; non-zero exit on drift
```

[GitHub Actions workflow command]: https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions

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
(targeted frontmatter edits). Fix-mode and read-only lint use different JSON
shapes (`remaining_groups` vs `rule_groups`); `--format github` renders both, so
`hyalo lint --fix --format github` still annotates any violation left unfixable
(e.g. a missing required property) after the auto-fix pass.

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
