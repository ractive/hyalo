---
title: Feature gaps & ideas from the 2026-07-10 dogfood + review session
type: research
date: 2026-07-10
status: active
tags: [dogfooding, feature-ideas, ux, ai-friendliness]
related:
  - "[[dogfood-results/dogfood-v0160-iter157-159-pr186]]"
  - "[[reviews/codebase-review-2026-07-10]]"
---

# Feature gaps & ideas — 2026-07-10 session

Every idea below is grounded in a concrete moment from this session where the
tool made me (an LLM agent, the primary audience) work harder than necessary.
Ranked by expected value.

## Tier 1 — directly unblocks agent workflows

### 1. MCP server mode (`hyalo serve --mcp`)

The biggest structural idea. Everything hyalo has built for agents — hints,
OUTPUT contracts, structured JSON errors, COMMON MISTAKES — is compensation
for living on the wrong side of a shell boundary. An MCP stdio server exposing
`find`/`read`/`set`/`task`/`links`/`lint` as tools would make all of it
native: help texts become tool schemas (machine-validated, no flag guessing —
the `mv --to` and `--section/--all` stumbles this session simply cannot
happen), hints become structured follow-up suggestions, and the snapshot
index loads **once per session instead of once per invocation** — on MDN
that's 0.4 s × every call, and it also dissolves the stale-index problem
(BUG-4) since the server owns the index lifecycle. `hyalo init --claude`
would register the server in `.mcp.json`. All existing CLI plumbing (dispatch
→ JSON envelope) is already shaped like tool-call responses.

### 2. Nested-map property access (`--property versions.fpt=*`)

GitHub Docs (3,710 files) keeps its most important metadata in nested maps
(`versions: {fpt: '*', ghes: '>=3.8'}`) and hyalo can only test existence or
regex over a serialized blob. Dotted paths in `--property`, `--fields`
display, and `set` (`hyalo set f.md --property versions.ghes='>=3.9'`) would
make hyalo genuinely usable on Docusaurus/GitHub-Docs-style trees. The filter
grammar already parses `K(op)V`; extend K to a path.

### 3. `hyalo undo` — journal of last mutation batch

Agents make mistakes; this session alone had `links fix --apply` writing a
wrong fuzzy match candidate at 0.896 confidence (only a threshold flag saved
it). Every mutating command already computes a full RewritePlan/atomic-write
set — persist the pre-images to `.hyalo/journal/<timestamp>/` (last N
batches) and add `hyalo undo [--list]`. Cheap to build on the existing
write-path choke point, and it converts every risky `--apply` into a safe
experiment. Git isn't always there (external KBs) and agent loops shouldn't
need `git checkout` as their safety net.

### 4. Section-level body mutation (`hyalo section append/replace`)

The one workflow where an agent must still fall back to raw file editing is
prose: append a bullet to `## Decisions`, add a row under `## Findings`.
`task toggle` already proves the section-scoped body-write machinery exists
(section_scanner + atomic write). `hyalo section append <file> --section
"Findings" --text "- new insight"` (+ `--replace`) would complete the
"never Edit directly" promise the project makes in its own CLAUDE.md. It
also makes body edits schema-aware (`required-sections` can validate).

## Tier 2 — sharpens existing features

### 5. `links fix --strategy` + prefix-aware fuzzy scoring

On real data this session: ShortestPath 15/15 correct, FuzzyMatch 0/2 wrong
(shared `iterations/done/iteration-` prefix inflates Jaro-Winkler). Let
`--apply` be restricted (`--strategy shortest-path,case`) and score fuzzy
candidates on the differing tail (basename/slug), not the whole path. Related
bug: BUG-3 in the dogfood report.

### 6. Index freshness: warn or auto-patch

Snapshot index silently serves stale results after any mutation that didn't
pass `--index` (BUG-4). Two cheap layers: (a) mutations auto-patch an
existing in-vault `.hyalo-index` even without the flag (the code for patching
already exists behind the flag); (b) read commands compare index build time
vs. the newest file mtime in the result set and print a one-line staleness
warning with the rebuild hint.

### 7. `lint --by-rule` triage table

First `lint` on a stock corpus is still a wall (7,399 warnings on firefox,
per the last report). A one-line-per-rule count table (exactly what I built
by hand with `--jq` + `sort | uniq -c`) plus the existing "tune MD010" hint
would make the first-run experience self-explanatory. (F-6 follow-up.)

### 8. `hyalo new` property overrides + auto-numbering

`new --type note --file x.md --property title="My Note"` saves the
scaffold→set round-trip in the agent fill-in loop that `new --help` itself
documents. Bonus: with the iteration `filename-template`
(`iterations/iteration-{n}-{slug}.md`) already in the schema,
`hyalo new --type iteration --slug robustness` could
compute `{n}` (max existing + 1) and the path itself — today the agent
duplicates that logic every iteration.

### 9. `hyalo stats` — native group-by

`--jq '.results | map(.properties.status) | group_by(.) | …'` works but is
write-only jq golf. `hyalo stats --by status --tag iteration` (counts, with
`--by property:X/tag/type/directory`) covers the most common dashboard need
in one memorable flag, and `properties` already computes per-property type
inference so the aggregation plumbing exists.

## Tier 3 — worth a backlog entry

### 10. `hyalo doctor`

One command bundling what a maintainer runs serially today: broken links,
orphans, mixed-type properties (`priority: mixed (87 text, 6 number)` was a
real find this session), undeclared types in use (`type: review` exists in 2
files but not in schema — also a real find), stale index, schema violations.
Exit non-zero for CI; each finding carries its fix-command hint.

### 11. Link graph export (`hyalo links graph --format dot|mermaid|json`)

The link graph is already built for backlinks/orphans; serializing it would
enable visualization and "what's reachable from X" analysis. Mermaid output
could be pasted straight into any markdown doc — including iteration plans.

### 12. `hyalo diff <file>` — semantic frontmatter diff

Show property-level changes (added/removed/retyped) between working tree and
git HEAD, or between two files. Agents reviewing their own mutations (and
`--dry-run` outputs) would get a stable, structured view instead of unified
text diff over YAML.

### 13. Machine-readable CLI self-description (`hyalo schema --commands`)

Emit the full clap tree (commands, flags, value types, defaults, examples) as
JSON. An agent could load the entire interface in one call instead of 22
`--help` invocations (which is exactly what the docs-audit agent had to do
today) — and the xtask `help_drift` gate could diff prose docs against it
mechanically, catching the M1-class drift (`default: json`) that slipped
through.

### 14. Per-KB "noise profile" bootstrap (`hyalo lint --init-profile`)

Pointing hyalo at a foreign corpus (firefox, GitHub Docs) always starts with
the same ritual: run lint, eyeball the top rules, `lint-rules set MDxxx
--enabled false` a few times. A single command that runs the by-rule
histogram and writes a commented `[lint.rules]` block disabling the top-N
noisiest rules (with counts as comments) would make "adopt hyalo on an
existing tree" a one-command experience.

## Non-features deliberately not proposed

- Watch mode/daemon for auto-reindex: the MCP server (idea 1) subsumes it
  with a cleaner lifecycle.
- A TUI: the audience is agents; investment belongs in the JSON/MCP surface.
- Frontmatter encryption/secrets: out of scope for a knowledgebase tool;
  CLAUDE.md security rules already cover the repo level.
