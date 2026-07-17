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

1. **Timezone-aware `timestamp` is rejected — a real incompatibility.** `is_datetime` (`crates/hyalo-core/src/frontmatter/types.rs:42`) hard-requires exactly 19 chars `YYYY-MM-DDThh:mm:ss`. OKF's real timestamps carry an offset (`2026-05-28T22:44:47+00:00`, 25 chars). hyalo's `datetime` constraint + `HYALO004` would flag **every** real OKF concept. Needs an RFC 3339 / offset-aware form (proposed `datetime-tz`, keeping naive `datetime` intact). **DONE (iter-163):** `datetime-tz` constraint added (accepts `Z` and `±hh:mm`, quoted and unquoted YAML), disjoint from naive `datetime`; `HYALO004` validates both against their grammar; wired through `hyalo types set --property-type K=datetime-tz` and `types show`. See [[iteration-163-okf-frontmatter-foundations]].
2. **No reserved-file exemption.** With `type` required globally, every `index.md`/`log.md` is flagged for missing `type` — but the spec *mandates* they have none. `lint.ignore` is a coarse path list; there's no glob-scoped "exempt `**/index.md`, `**/log.md` from required-type", no root-`index.md` `okf_version` allowance. Most important integration caveat. **DONE (iter-163):** `[schema] exempt = ["**/index.md", "**/log.md"]` binds matching files to no schema (skips missing-`type`, required-property, undeclared-property, required-sections; honored in both `hyalo lint` and validate-on-write); the bundle-root `index.md` may carry a lone `okf_version` key (scoped to `rel_path == "index.md"`); globs are vault-relative and cross-platform. See [[iteration-163-okf-frontmatter-foundations]].
3. **No OKF init profile.** `init` is hardcoded to Claude/pi skills (`crates/hyalo-cli/src/commands/init.rs`); no path to scaffold an OKF-shaped `.hyalo.toml`. NB: the flag must be `--profile okf`, not `--format=okf` — `--format` is the global output flag and collides. **DONE (iter-164):** `hyalo init --profile okf` deep-merges an embedded declarative TOML fragment (`templates/profile-okf.toml`) into `.hyalo.toml` — `required=["type"]`, recommended props (`timestamp:datetime-tz`, `resource` URL pattern), `exempt=["**/index.md","**/log.md"]`, `site_prefix=""`, `validate_on_write=true`, seeded `[schema.types.*]`. Profiles are data-driven (`crates/hyalo-cli/src/commands/profiles.rs`), composable (deep-merge upserts only their own keys), and idempotent on re-run. [[iteration-164-okf-init-profile-and-skill]].
4. **No `index.md` / `log.md` generators.** These are *derived* data; maintaining them by hand is exactly the tedium hyalo should kill. `hyalo okf index` / `hyalo okf log` have no equivalent anywhere in the OKF ecosystem — highest-leverage, unique. [[iteration-165-okf-index-and-log-generators]].
5. **No conformance profile.** Spec §9 = 3 rules; no `hyalo lint` ruleset encodes it, and broken-link checks must be *warn* not *error* to stay spec-compliant. [[iteration-166-okf-conformance-lint]].
6. **No bundled OKF skill.** `init --claude` installs `hyalo`/`hyalo-tidy` skills; no OKF-authoring/producer skill. **DONE (iter-164):** `templates/skill-hyalo-okf.md` embedded in the binary; `hyalo init --profile okf --claude` installs it at `.claude/skills/okf/SKILL.md`. Teaches the OKF concept model, reserved-file rules, link forms, the deterministic-vs-LLM split, and only real, current hyalo commands (`find --property type=`, `new --type`, `lint --strict`, `set`/`append`, `mv`, `find --broken-links`) — deferring `okf index`/`log`/`lint --profile` to iters 165–166. [[iteration-164-okf-init-profile-and-skill]].
7. **No citation-aware linting.** hyalo has only generic markdown link rules (MD011/034/042/…) + internal broken-link repair (`hyalo links`); nothing understands the `# Citations` convention. Missing: *presence* (warn when a claim-bearing concept has no `# Citations`), *resolvability* (bundle-relative / `references/` citation links resolve), *non-regression* (don't shrink a doc's citation count — the reference-agent's own augmentation guard). External URL reachability stays out (determinism/offline). A convention (SHOULD), so warn-level advisory in the okf profile, not §9 conformance. Nuance: §8 says citations are *numbered*, but all official sample bundles use `-` bullets — accept both. [[iteration-166-okf-conformance-lint]].
8. **Bundle-absolute link edge case.** SPEC §5 *recommends* leading-`/` bundle-root links ("stable when documents are moved"). `strip_site_prefix` (`crates/hyalo-core/src/link_graph.rs:569`) already falls back to vault-root resolution on prefix mismatch, so these mostly work today — but the auto-derived `site_prefix` (vault dirname) mis-strips when the bundle dir shares a name with a top-level subdir (bundle root named `tables/` breaks `/tables/x.md`). The OKF profile must pin `site_prefix`; `--site-prefix ""` currently means "disable resolution", which is also wrong for OKF. **DONE (iter-163):** confirmed `site_prefix = ""` (→ `None`) is exactly right — it strips only the leading `/`, so `/tables/x.md` resolves to `tables/x.md` from bundle root and the dirname-collision mis-strip is avoided (an active prefix matching a subdir name would break it). Documented + tested; the OKF init profile (iter-164) sets `site_prefix = ""`. See [[iteration-163-okf-frontmatter-foundations]].

## Proposed shape

An **"okf" profile + a small `hyalo okf` subcommand group** — not a plugin architecture (overkill for a v0.1 draft):

- **`datetime-tz` constraint type** — RFC 3339 with offset (`+00:00` and `Z`, quoted and unquoted YAML); naive `datetime` unchanged.
- **Reserved-file exemption** — schema-level `exempt = ["**/index.md", "**/log.md"]` honored by validation + lint, plus root-`index.md` `okf_version` allowance.
- **Bundle-absolute link resolution** — spec-recommended `/x.md` form resolves from bundle root regardless of dirname (site_prefix pinning; see gap #8).
- **`hyalo init --profile okf`** — writes an OKF `.hyalo.toml` (base schema: `required=["type"]`, recommended props declared with `timestamp:datetime-tz`, `resource` URL pattern; reserved-file exemptions; site_prefix pin; broken-links = warn) and, with `--claude`, an `okf` skill.
- **`hyalo okf index`** — deterministic `index.md` (re)generation from `title`/`description`, grouped by `type`.
- **`hyalo okf log [TARGET]`** — prepend a dated entry to the `log.md` at any hierarchy level (§7 scope semantics).
- **Frontmatter key-order normalization** to `type, resource, title, description, tags, timestamp`, plus tz-aware `timestamp` auto-stamp on write.
- **`hyalo lint --profile okf`** — encodes SPEC §9; citation rules (advisory); optional augmentation guards (don't drop `# Schema`/`# Citations`).
- **Bundled `okf` skill** — teaches an agent OKF conventions + which hyalo commands to use for the deterministic layer.

### CLI design decisions (2026-07-16 second-pass review)

1. **`--profile`, not `--format`.** `hyalo init --format=okf` is impossible: `--format` is the global output flag (`json|text`) and propagates into subcommands (verified: `error: invalid value 'okf'`). `--profile okf` on both `init` and `lint` makes "profile" one coherent concept. OKF is deliberately the *first* profile, not the only one — see [[profile-candidates-beyond-okf]] for the roadmap (`skills`, `madr`, `changelog`, …).
2. **Keep the `hyalo okf` group** (vs a bare `hyalo index`): "index" already means the snapshot index (`create-index`/`drop-index`); an unqualified `index` command would collide. `okf index`/`okf log` are self-documenting.
3. **Generators default to dry-run + `--apply`**, matching the `links fix`/`links auto` house convention for bulk mutations.
3a. **No `okf lint` (considered 2026-07-16).** Validation stays centralized in `hyalo lint`: its flag surface (`--fix`, `--rule`, `--strict`, `--files-from -`) must compose with profiles, per-profile lint subcommands would fork it ×N profiles, and the `okf` group exists only for OKF-specific *artifact generators* — validation is the shared engine with a different ruleset. Semantics: `lint --profile okf` is the *ephemeral overlay* (CI, third-party bundles); after `init --profile okf` the rules live in `.hyalo.toml` and **plain `hyalo lint` suffices**. Discoverability via hints: `hyalo okf` commands emit `-> hyalo lint --profile okf`.
4. **Link forms:** SPEC §5 *recommends* bundle-absolute `/x.md` (stable under moves) — the "never leading `/`" rule seen in the reference agent's prompts is that agent's internal convention, **not** the spec. Official sample bundles use relative links in `index.md`; the blog's concept example uses absolute. hyalo must resolve both; generated indexes follow the samples (relative).
5. **Citations:** SPEC §8 says numbered; every official sample bundle uses `-` bullets. Lint accepts both.

## Docs-sync obligation

Per house rule, every iteration lands help text + `README.md` + this KB + the bundled skill **in the same PR** as the code.

## Sources

- Blog: <https://cloud.google.com/blog/products/data-analytics/how-the-open-knowledge-format-can-improve-data-sharing/>
- Spec: <https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md>
- Repo root: <https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf>
- Sample bundles: <https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf/bundles>
- Reference agent: <https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf/src/reference_agent>
- ga4 recipe sample: <https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf/samples/ga4_merch_store>
