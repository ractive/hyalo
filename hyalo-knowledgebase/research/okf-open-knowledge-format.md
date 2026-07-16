---
title: "OKF (Open Knowledge Format) — Fit, Gaps & Plan for hyalo"
type: research
date: 2026-07-16
status: active
tags: [research, okf, knowledge-management, interop, schema, architecture]
source: "https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md"
related: [research/karpathy-llm-wiki.md]
---

# OKF (Open Knowledge Format) — Fit, Gaps & Plan for hyalo

## TL;DR

OKF is **a directory of Markdown files with YAML frontmatter**, distributed as a git repo/tarball — hyalo's exact substrate. The ecosystem currently ships **no official validator/linter** (only a Gemini+BigQuery reference producer and a static HTML visualizer). hyalo can become the vendor-neutral **OKF authoring, validation, and maintenance CLI**, and — paired with a bundled skill — a **Claude-native OKF producer** that owns all the deterministic frontmatter/index/link mechanics while the LLM does only the semantic work.

Verdict: **strong fit, not a stretch.** Scope agreed: **Full OKF CLI**. Plan: iterations [[iteration-163-okf-frontmatter-foundations]], [[iteration-164-okf-init-profile-and-skill]], [[iteration-165-okf-index-and-log-generators]], [[iteration-166-okf-conformance-lint]].

## What OKF is

- **Origin:** Google Cloud (Knowledge Catalog team; authors Sam McVeety, Amir Hormati). Version **0.1 (Draft)**. Formalizes Karpathy's "LLM wiki" pattern (see [[karpathy-llm-wiki]]). Explicitly *"Format, not platform"* — never requires a proprietary account or SDK to read/write/serve. Distinct from Google Data Commons.
- **Blog:** <https://cloud.google.com/blog/products/data-analytics/how-the-open-knowledge-format-can-improve-data-sharing/>
- **Spec:** <https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md>
- **Sample bundles:** `okf/bundles/{crypto_bitcoin,ga4,stackoverflow}/`

### The format (spec essentials)

- **Knowledge Bundle** = self-contained directory tree; the unit of distribution (git repo recommended; tarball/zip/subdir allowed).
- **Concept** = one `.md` document. **Concept ID** = file path minus `.md` (`tables/users.md` → `tables/users`).
- **Frontmatter is YAML. Exactly one required field: `type`** (open string; unknown types MUST be tolerated). Recommended: `title`, `description`, `resource` (canonical URI), `tags` (list), `timestamp` (ISO 8601).
- **Relationships** = plain Markdown links (untyped edges; type conveyed by prose). Hierarchy is implicit from directories.
- **Provenance** = `# Citations` sections + git history. Conventional headings (SHOULD): `# Schema`, `# Examples`, `# Citations`.
- **Reserved files** `index.md` and `log.md` are **frontmatter-free by design**. The *bundle-root* `index.md` MAY carry a single `okf_version: "0.1"` key and nothing else.
  - `index.md` = pure Markdown link list: `* [Title](path) - description`.
  - `log.md` = date-grouped chronological history (newest first, `YYYY-MM-DD` headings, bold action words).
- **Conformance (§9)** — a bundle conforms to v0.1 iff:
  1. every non-reserved `.md` has a parseable YAML frontmatter block;
  2. every such block has a non-empty `type`;
  3. reserved files follow their §6/§7 structure when present.
- **Permissive consumption:** consumers MUST NOT reject on missing optional fields, unknown `type` values, unknown extra keys, **broken cross-links**, or missing `index.md`.

### Example concept (`crypto_bitcoin/tables/blocks.md`)

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

## The reference agent (why hyalo fits the *producer* too)

`okf/src/reference_agent` is Google's **proof-of-concept producer** — a two-pass, tool-calling **Gemini + BigQuery** agent (Google ADK, `gemini-flash-latest`):

1. **BQ pass** — per concept: read BigQuery schema/sample rows → LLM composes one OKF doc (prose + `# Schema` + `# Common query patterns` SQL + `# Citations`) → `write_concept_doc`.
2. **Web pass** (if `seeds.txt` given) — LLM crawls from seed doc URLs, augmenting existing docs or minting `references/*.md`.
3. **Index pass** — `regenerate_indexes` deterministically rewrites every `index.md`.

A **seed** = a starting doc URL for the web crawl. The `samples/ga4_merch_store/` dir is a *recipe* (`README.md` + `seeds.txt`) producing the committed `okf/bundles/ga4/`.

**Key finding:** to make LLM output valid OKF, Google hand-rolled an entire Python `bundle/` package (`document.py`, `index.py`, `paths.py`, `synthesizer.py`) doing work that is **pure frontmatter/markdown mechanics — hyalo's wheelhouse**, not LLM work:

| `reference_agent` (Python, by hand) | hyalo equivalent |
|---|---|
| parse `---`/YAML, enforce required keys (`type,title,description,timestamp`), reorder keys | frontmatter parse + schema `required` + key-order normalizer (new) |
| auto-stamp tz-aware `timestamp`; union `tags` on update | `hyalo set`/`append` + timestamp helper |
| `regenerate_indexes` (walk dirs → `* [title](link) - description`) | **`hyalo okf index`** (new — deterministic, no LLM) |
| augmentation guard: refuse writes that drop `# Schema`/`# Citations` | lint/validation rule |
| cross-link rules (only known ids, relative paths) | `hyalo links`/`backlinks` graph checks |

**Separation of concerns:**
- **LLM-only** (hyalo stays out): prose bodies, semantic `title`/`description`/`tags`, crawl judgment.
- **Cloud-data-only** (hyalo stays out): reading BigQuery ground-truth (`type`/`resource` originate here).
- **Deterministic mechanics** (hyalo owns): validate, scaffold, index, stamp, key-order, link-check.

So hyalo + a bundled `okf` skill = a **vendor-neutral, Claude-native OKF producer** where the agent never reinvents `bundle/`. This mirrors [[karpathy-llm-wiki]]'s conclusion: *hyalo provides the substrate; the LLM orchestrates.*

## What already works in hyalo

| OKF need | hyalo today |
|---|---|
| `type` required on every concept | `[schema.default] required = ["type"]` + `validate_on_write` / `lint --strict` |
| Browse by type | `hyalo find --property type="BigQuery Table"` |
| tags as first-class list | native list frontmatter + `hyalo tags` |
| `# Schema`/`# Examples`/`# Citations` addressing | `--section` substring heading match |
| Body/link search, broken-link detection | `hyalo find`, `hyalo links`, `hyalo backlinks` |
| Move without breaking links | `hyalo mv` (rewrites links vault-wide) |
| Open `type` string (no enum needed) | free-string `required` field |

**Non-gaps:** nested frontmatter objects (hyalo's known weakness) — OKF needs none; all fields are scalar/list.

## Gaps (ranked)

1. **Timezone-aware `timestamp` is rejected — a real incompatibility.** `is_datetime` (`crates/hyalo-core/src/frontmatter/types.rs:42`) hard-requires exactly 19 chars `YYYY-MM-DDThh:mm:ss`. OKF's real timestamps carry an offset (`2026-05-28T22:44:47+00:00`, 25 chars). hyalo's `datetime` constraint + `HYALO004` would flag **every** real OKF concept. Needs an RFC 3339 / offset-aware form (proposed `datetime-tz`, keeping naive `datetime` intact). Fixed in [[iteration-163-okf-frontmatter-foundations]].
2. **No reserved-file exemption.** With `type` required globally, every `index.md`/`log.md` is flagged for missing `type` — but the spec *mandates* they have none. `lint.ignore` is a coarse path list; there's no glob-scoped "exempt `**/index.md`, `**/log.md` from required-type", no root-`index.md` `okf_version` allowance. Most important integration caveat. [[iteration-163-okf-frontmatter-foundations]].
3. **`hyalo init --format=okf` doesn't exist.** `init` is hardcoded to Claude/pi skills (`crates/hyalo-cli/src/commands/init.rs`); no path to scaffold an OKF-shaped `.hyalo.toml`. [[iteration-164-okf-init-profile-and-skill]].
4. **No `index.md` / `log.md` generators.** These are *derived* data; maintaining them by hand is exactly the tedium hyalo should kill. `hyalo okf index` / `hyalo okf log` have no equivalent anywhere in the OKF ecosystem — highest-leverage, unique. [[iteration-165-okf-index-and-log-generators]].
5. **No conformance profile.** Spec §9 = 3 rules; no `hyalo lint` ruleset encodes it, and broken-link checks must be *warn* not *error* to stay spec-compliant. [[iteration-166-okf-conformance-lint]].
6. **No bundled OKF skill.** `init --claude` installs `hyalo`/`hyalo-tidy` skills; no OKF-authoring/producer skill. [[iteration-164-okf-init-profile-and-skill]].

## Proposed shape

An **"okf" profile + a small `hyalo okf` subcommand group** — not a plugin architecture (overkill for a v0.1 draft):

- **`datetime-tz` constraint type** — RFC 3339 with offset; naive `datetime` unchanged.
- **Reserved-file exemption** — schema-level `exempt = ["**/index.md", "**/log.md"]` honored by validation + lint, plus root-`index.md` `okf_version` allowance.
- **`hyalo init --format=okf`** — writes an OKF `.hyalo.toml` (base schema: `required=["type"]`, recommended props declared with `timestamp:datetime-tz`, `resource` URL pattern; reserved-file exemptions; broken-links = warn) and, with `--claude`, an `okf` skill.
- **`hyalo okf index`** — deterministic `index.md` (re)generation from `title`/`description`, grouped by `type`.
- **`hyalo okf log`** — prepend a dated `log.md` entry.
- **Frontmatter key-order normalization** to `type, resource, title, description, tags, timestamp`, plus tz-aware `timestamp` auto-stamp on write.
- **`hyalo lint --profile okf`** — encodes SPEC §9; optional augmentation guards (don't drop `# Schema`/`# Citations`).
- **Bundled `okf` skill** — teaches an agent OKF conventions + which hyalo commands to use for the deterministic layer.

## Docs-sync obligation

Per house rule, every iteration lands help text + `README.md` + this KB + the bundled skill **in the same PR** as the code.

## Sources

- Blog: <https://cloud.google.com/blog/products/data-analytics/how-the-open-knowledge-format-can-improve-data-sharing/>
- Spec: <https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md>
- Repo root: <https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf>
- Sample bundles: <https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf/bundles>
- Reference agent: <https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf/src/reference_agent>
- ga4 recipe sample: <https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf/samples/ga4_merch_store>
