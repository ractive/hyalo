---
name: okf
user_invocable: false
description: >
  Author and maintain Open Knowledge Format (OKF) bundles with hyalo. Use this skill
  whenever you are producing or editing an OKF knowledge bundle — a directory of Markdown
  "concept" files with YAML frontmatter, distributed as a git repo or tarball. Trigger it
  when: scaffolding a new OKF concept, validating a bundle against the spec, browsing
  concepts by their `type`, wiring cross-links between concepts, or maintaining the
  reserved `index.md` / `log.md` files. Even if the user does not say "OKF" by name, use
  this skill when the task involves a knowledge bundle whose files carry a `type`
  frontmatter key, `# Schema` / `# Citations` sections, or a bundle-root `index.md`
  with an `okf_version` key. hyalo owns the deterministic frontmatter/link mechanics; the
  LLM does only the semantic work (prose, titles, tags, crawl judgment).
---

# OKF (Open Knowledge Format) — Authoring & Maintenance with hyalo

OKF is **a directory of Markdown files with YAML frontmatter**, distributed as a git repo
or tarball. It is a *format, not a platform* — no proprietary account or SDK is ever
required. hyalo is a natural, vendor-neutral OKF authoring/validation CLI: it owns every
deterministic mechanic (parse frontmatter, enforce `type`, check links, scaffold, browse),
leaving the LLM to do only the semantic work.

## Concept model (spec essentials)

- **Knowledge Bundle** = a self-contained directory tree. The unit of distribution.
- **Concept** = one `.md` document. The **Concept ID** is its path minus `.md`
  (`tables/users.md` → `tables/users`).
- **Frontmatter is YAML with exactly one required field: `type`** (an open string —
  unknown types MUST be tolerated). Recommended optional fields: `title`, `description`,
  `resource` (a canonical URI), `tags` (a list), `timestamp` (RFC 3339, offset-aware,
  e.g. `2026-05-28T22:44:47+00:00` or `...Z`).
- **Relationships** = plain Markdown links (untyped edges; the edge's meaning lives in the
  prose). Hierarchy is implicit from the directory layout.
- **Provenance** = a `# Citations` section plus git history. Conventional headings (SHOULD):
  `# Schema`, `# Examples`, `# Citations`.
- **Reserved files** `index.md` and `log.md` are **frontmatter-free by design**, with one
  exception: the *bundle-root* `index.md` MAY carry a single `okf_version: "0.1"` key and
  nothing else — every other `index.md`/`log.md` (including nested ones) has no frontmatter.
  - `index.md` = a pure Markdown link list: `* [Title](path) - description`.
  - `log.md` = a date-grouped chronological history (newest first, `YYYY-MM-DD` headings).
- **Permissive consumption:** consumers MUST NOT reject on missing optional fields, unknown
  `type` values, unknown extra keys, **broken cross-links**, or a missing `index.md`.

## The deterministic-vs-LLM split

Keep these separate. Never make the LLM reinvent the mechanical layer:

- **LLM-only** (you do this): prose bodies, semantic `title` / `description` / `tags`,
  crawl and augmentation judgment.
- **Ground-truth-only** (comes from the source system, e.g. BigQuery): `type`, `resource`.
- **Deterministic mechanics** (hyalo does this): validate frontmatter, scaffold new
  concepts, browse by type, check links, move files without breaking links.

## Reserved-file rules

- `index.md` and `log.md` carry **no `type`** — the OKF profile's
  `[schema] exempt = ["**/index.md", "**/log.md"]` exempts them from the required-`type`
  check. Do **not** add a `type` to them.
- The bundle-root `index.md` may carry a lone `okf_version: "0.1"` key — nothing else.
- `index.md` bodies are link lists; `log.md` bodies are dated changelog entries. These are
  *derived* data — **regenerate them with `hyalo okf index` / `hyalo okf log`** rather than
  hand-editing (see "Maintaining reserved files" below).

## Link forms

- Cross-links between concepts are plain Markdown links. **Bundle-absolute links
  (`/tables/x.md`, the spec-recommended §5 form) are preferred** — they stay stable when a
  document moves. The OKF profile sets `site_prefix = ""` so a leading `/` resolves from the
  bundle root.
- To move or rename a concept and rewrite every inbound link vault-wide, use
  `hyalo mv old.md --to new.md` — never a raw `mv` (it would orphan the links).

## The exact hyalo commands

Use these — do not fall back to Read/Grep/Glob for OKF bundle work.

```bash
# Scaffold a new concept from the schema (fills TBD placeholders), then see what to fill in:
hyalo new --type "BigQuery Table" --file tables/blocks.md
hyalo lint --file tables/blocks.md

# Browse concepts by their OKF `type`:
hyalo find --property type="BigQuery Table"
hyalo find --property type=Reference --tag bitcoin

# Read a specific conventional section of a concept:
hyalo read tables/blocks.md --section "# Schema"
hyalo read tables/blocks.md --section "# Citations"

# Validate the whole bundle against the OKF profile schema.
# --strict promotes missing-`type` and undeclared-property warnings to errors,
# encoding the spec §9 conformance rules (every non-reserved concept has a `type`):
hyalo lint --strict

# Set / update frontmatter deterministically (never hand-edit the YAML block):
hyalo set tables/blocks.md --property title="Bitcoin Blocks Table"
hyalo set tables/blocks.md --tag bigquery

# Find broken cross-links (advisory only — the spec forbids rejecting on them):
hyalo find --broken-links

# Move a concept and rewrite inbound links across the bundle:
hyalo mv tables/blocks.md --to tables/bitcoin_blocks.md
```

## Maintaining reserved files (`index.md` / `log.md`)

`index.md` and `log.md` are *derived* — never hand-edit them. hyalo regenerates both
deterministically (no LLM). Run these after adding, editing, moving, or removing concepts:

```bash
# Regenerate every directory's index.md from concept frontmatter.
# Concepts are grouped by `type`; entries are `* [title](relative-link) - description`
# (title falls back to the filename; description optional); subdirectories are listed too.
hyalo okf index --dry-run          # preview (default); exits non-zero if any index.md is stale
hyalo okf index --apply            # write the regenerated index.md files
hyalo okf index tables --apply     # scope to a single subtree

# Prepend a dated entry to a log.md (newest first, under a YYYY-MM-DD heading).
# TARGET selects the log: a directory -> TARGET/log.md, a log.md path, or omitted -> bundle root.
hyalo okf log --message "Added the blocks table" --apply
hyalo okf log tables --action Update --message "Refreshed the schema section" --apply
```

Key rules for these generators:

- **`okf index` owns only a managed region** delimited by `<!-- okf:index:begin -->` /
  `<!-- okf:index:end -->`. Prose you write outside those markers is preserved — put any
  bundle overview above the begin marker.
- It **preserves the bundle-root `index.md`'s `okf_version`** key and never adds frontmatter
  to nested `index.md`/`log.md` files.
- It is **idempotent** (running `--apply` twice changes nothing) and **CI-friendly**:
  `hyalo okf index --dry-run` exits non-zero when the committed `index.md` files are stale.
- Both default to `--dry-run`; pass `--apply` to write. Do the concept edits first (with
  `hyalo set` / `hyalo new` / `hyalo mv`), then regenerate the reserved files.

## Setting up an OKF bundle

```bash
hyalo init --profile okf            # writes an OKF-ready .hyalo.toml
hyalo init --profile okf --claude   # also installs this skill for Claude Code
```

The OKF profile's `.hyalo.toml` sets: `[schema.default] required = ["type"]`, the
recommended props (`title`, `description`, `resource` with a URL pattern, `tags`,
`timestamp: datetime-tz`), `exempt = ["**/index.md", "**/log.md"]`, `site_prefix = ""`
for bundle-root links, and `validate_on_write = true` so authoring stays conformant.

## Example concept

```markdown
---
type: BigQuery Table
resource: https://bigquery.googleapis.com/v2/projects/bigquery-public-data/datasets/crypto_bitcoin/tables/blocks
title: Bitcoin Blocks Table
description: Details about the Bitcoin Blocks BigQuery table, including its schema.
tags:
- bitcoin
- bigquery
timestamp: '2026-05-28T22:43:59+00:00'
---

# Schema

| Field | Type |
| --- | --- |
| hash | hex_string |

# Citations
- https://github.com/blockchain-etl/bitcoin-etl
```
