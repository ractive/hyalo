---
title: "Profile candidates beyond OKF — standards hyalo could support via --profile"
type: research
date: 2026-07-16
status: active
tags: [research, profiles, okf, standards, interop, schema]
related: [research/okf-open-knowledge-format.md, research/obsidian-properties.md]
---

# Profile candidates beyond OKF

Survey of markdown standards/conventions that could become additional `hyalo init|lint --profile <name>` values after OKF ([[okf-open-knowledge-format]]). A profile = a canned `.hyalo.toml` (type schemas + lint config + exemptions) plus optionally scaffolding, generators, and a bundled skill. Two research sweeps: SSG/git-CMS ecosystems and KM/docs-methodology standards (2026-07-16, primary docs fetched, GitHub stars checked).

## TL;DR — decided roadmap (2026-07-16)

Sequence approved: **okf → madr → skills → changelog**, one iteration each, with a mandatory learnings-propagation retrospective at the end of every iteration (review + update the remaining iteration plans before starting the next).

1. **`okf`** — [[iteration-163-okf-frontmatter-foundations]] … [[iteration-166-okf-conformance-lint]]
2. **`madr`** — [[iteration-167-madr-profile]] (before skills: cheapest, and first proof the profile machinery is data-driven)
3. **`skills`** — [[iteration-168-skills-profile]] (adds generic max-length / dirname-coupling / line-budget rule kinds)
4. **`changelog`** — [[iteration-169-changelog-profile]] (generalizes the heading-grammar lint mode)
5. **Obsidian `types.json` import** — a bridge feature, one-way and modest (see caveats below; prior finding in [[obsidian-properties]]) — not scheduled
6. SSG profiles (`hugo`, `docusaurus`, `jekyll`, `mkdocs-material`, `starlight`) — big markets, later wave or community-contributed — not scheduled

## Tier A — agent-ecosystem adjacency (do these next)

### `skills` — Agent Skills (SKILL.md)

- **Format:** exactly hyalo's model (dir of `SKILL.md` + YAML frontmatter). Formal spec: <https://agentskills.io/specification>.
- **Hard constraints:** `name` required, 1–64 chars, `^[a-z0-9]+(-[a-z0-9]+)*$`, **must equal parent directory name**, reserved words (anthropic/claude) forbidden; `description` required 1–1024 chars; optional `license`, `compatibility` (≤500), `metadata`, `allowed-tools`. Body recommended <500 lines.
- **Profile enforces/scaffolds:** full schema, name↔dirname rule (new rule kind), line budget, `hyalo new skill <name>` scaffolding a compliant dir.
- **Why #1:** hard numeric constraints made for schema linting; hyalo would be the CI-friendly Rust validator for skill collections; dogfoodable on `.claude/skills/` in this repo. Adoption very high and rising.

### `madr` — Markdown Architecture Decision Records

- **Format:** MADR 4.0.0 has optional-but-typed YAML frontmatter: `status` enum (`proposed|rejected|accepted|deprecated` + `superseded by ADR-0123` pattern), `date`, `decision-makers`/`consulted`/`informed` lists. 3.x variant renames (`deciders`, `## Validation`); Nygard/adr-tools = headings only (no frontmatter). Spec: <https://adr.github.io/madr/>.
- **Profile enforces/scaffolds/generates:** filename `^\d{4}-[a-z0-9-]+\.md$` with auto-next-number (hyalo `filename-template` `{n}` already exists), 3 required sections, status enum + supersede pattern, generated TOC/status dashboard (parity with `adr generate toc`).
- **Why:** the classic "typed docs with lifecycle" — identical shape to hyalo's own iteration files. adr-tools 5.6k★, ADR collections 16.5k★.

### `changelog` — Keep a Changelog 1.1.0

- **Format:** no frontmatter — but the most deterministic heading grammar in markdown: `# Changelog` → `## [Unreleased]` → `## [X.Y.Z] - YYYY-MM-DD` newest-first, `###` limited to `Added|Changed|Deprecated|Removed|Fixed|Security`, `[YANKED]`, footer link refs matching every version. Spec: <https://keepachangelog.com/en/1.1.0/>.
- **Profile enforces/generates:** grammar, semver descending, date monotonicity, unknown/empty subsection lints, link-ref cross-check; "release" generator (rotate Unreleased → version) — structurally the planned `okf log` machinery.
- **Why:** arguably the widest-adopted structured-markdown convention anywhere; no Rust-native linter of note exists. Requires a **heading-grammar lint mode** (frontmatter-less profile — same capability the OKF reserved-file checks need for `log.md`).

### Obsidian (`types.json` importer + thin rules) — downgraded on review

- Obsidian standardizes property *types* (Text/List/Number/Checkbox/Date/Datetime; reserved `tags`/`aliases`/`cssclasses`), not required keys/lifecycles — see [[obsidian-properties]], whose "Global Type Assignment" finding is decisive: **`types.json` is a flat vault-global `property-name → type` map applied to any document** — no document types, no required fields, no enums/patterns, and the types are *UI rendering hints*, not validation constraints.
- **Structural fit:** hyalo *does* have a vault-global constraints layer (`[schema.default.properties.*]` in `crates/hyalo-core/src/schema.rs` — a full `TypeSchema` merged under per-type overrides), so a **one-way import** `types.json` → `[schema.default.properties]` is mechanically clean (text→string, checkbox→boolean, multitext→string-list, …).
- **Caveats:** the import is a semantic upgrade (hints become lint constraints — may flood real vaults with warnings; import should default to `severity=warn`), yields only loose types, and the **reverse direction is lossy** — hyalo legally types the same property differently per document type, which `types.json` cannot represent. An earlier draft of this note claimed a "near 1:1 round-trip"; that was wrong.
- Verdict: a small, optional `hyalo import obsidian` convenience + thin lints (reserved keys are lists, `tag`→`tags`), well below `skills`/`madr`/`changelog` in priority.

## Tier B — SSG/CMS profiles (large markets, later wave)

| Profile | Stars | YAML gate | What it enforces | Notes |
|---|---|---|---|---|
| `hugo` | 89k | partial (also TOML `+++`/JSON) | 6 date formats, `draft`, `build.*` enums, **`params` namespacing lint**, alias keys, `_index.md` vs `index.md` semantics | Archetypes ≈ hyalo scaffolds; cascade ≈ per-dir defaults. Biggest market; format fragmentation is the caveat |
| `docusaurus` | 65.6k | clean | tag/author **enums from `tags.yml`/`authors.yml`** (upstream `onInlineTags: throw`), blog `YYYY-MM-DD-slug` regex, deprecated-key lints, `_*` exemptions | Best adoption+validation-culture combo; zero format risk |
| `jekyll` | 51.6k | perfect (YAML mandated) | `_posts/YYYY-MM-DD-*` filename regex, datetime format, `published`, tags-string footgun, `_config.yml` `defaults:` importable | GH Pages long tail; most deterministic conventions |
| `mkdocs-material` | 27.1k | clean | blog `date` **required**, `status` enum, `tags_allowed`/`categories_allowed` mirrored as lints, `.authors.yml` enum | Upstream stops builds on these — hyalo is the pre-commit version. Friction: nested objects (`search.boost`) |
| `starlight`/`astro` | 8.9k/61.1k | clean in practice | Starlight: required `title`, `template`/badge enums; Astro blog: mirror official template schema | Source of truth is zod/TS — profile mirrors, can't import |

**Importers, not profiles:** Decap CMS `config.yml` (fields map ~1:1: `required` default-true, `pattern` + custom message, `select.options`, datetime formats, slug templates) and Front Matter CMS `frontmatter.json` (uses hyalo's own `type:` dispatch convention). `hyalo import decap|frontmatter|obsidian` would onboard existing schemas mechanically.

**Deferred/rejected:** Zola (TOML `+++` primary — revisit if TOML frontmatter lands), Quarto/MyST (richest constraint vocabulary — max-length, cross-field, SPDX/ORCID — but `.qmd` + niche; mine for feature design), Eleventy/Hexo/VitePress (little to enforce), Logseq (`key:: value`, not YAML; moving to SQLite), AGENTS.md (explicitly schema-free), Diátaxis (officially anti-structure; at most a "Diátaxis-inspired" `type` enum), Dendron (great schema prior art, project in maintenance mode), Johnny.Decimal/PARA/Foam/Zettelkasten (no on-disk spec; Zettelkasten could be an opinionated `^\d{12}` ID profile), llms.txt (better as a hyalo *generator* than a lint target), cursor-rules/`.mdc` + Windsurf/Copilot rules (crisp schemas, blocked on non-`.md` extensions).

## Cross-cutting capability gaps (union of both sweeps)

Budget these into the profiles feature; several are shared across many profiles:

1. **Heading-grammar lint mode** (frontmatter-less structure specs) — changelog, standard-readme, Nygard ADR, OKF `index.md`/`log.md` structure checks (iter-166 already needs a first cut)
2. **Filename↔frontmatter coupling** — Jekyll/Docusaurus date prefixes, MADR numbering, skills name↔dirname
3. **Enum values from external file** — Docusaurus `tags.yml`/`authors.yml`, Material `tags_allowed`, `.authors.yml`
4. **String max-length + per-file line budgets** — skills (1024-char description, <500 lines), MyST title ≤500
5. **Cross-field constraints** — MyST `corresponding→email`, Windsurf `globs` iff `trigger: glob`
6. **Nested-object property typing** — Material `search.boost`, `date.created` (known hyalo gap; OKF doesn't need it, these do)
7. **Deprecated-key→replacement lint** — Docusaurus `author_*`, Quartz aliases, Obsidian `tag`→`tags`
8. **Frontmatter-forbidden rule** — OKF reserved files (planned, iter-163)
9. **TOML `+++` frontmatter** — unlocks Zola + full Hugo
10. **Non-`.md` extensions** — `.mdc` (Cursor), `.qmd` (Quarto)
11. **Schema importers** — `types.json`, Decap `config.yml`, `frontmatter.json`, Jekyll `defaults:`

## Design implication for iter-164

With 4+ profiles plausibly following OKF, the `--profile` implementation should be **data-driven from day one**: profiles as embedded declarative TOML fragments (schema + lint config + exemptions + template references), not per-profile Rust code paths — so `skills`/`madr`/`changelog` are additive data + a few new rule kinds, and a user-local `.hyalo/profiles/*.toml` becomes possible later. Noted in [[iteration-164-okf-init-profile-and-skill]].

## Sources

- Agent Skills spec: <https://agentskills.io/specification>
- MADR: <https://adr.github.io/madr/>
- Keep a Changelog: <https://keepachangelog.com/en/1.1.0/>
- Obsidian properties: <https://obsidian.md/help/properties> (see [[obsidian-properties]])
- Hugo front matter: <https://gohugo.io/content-management/front-matter/>
- Docusaurus docs/blog frontmatter, `tags.yml`: <https://docusaurus.io/docs/api/plugins>
- Jekyll front matter/defaults: <https://jekyllrb.com/docs/front-matter/>
- MkDocs Material meta/blog: <https://squidfunk.github.io/mkdocs-material/>
- Astro content collections / Starlight frontmatter: <https://docs.astro.build/en/guides/content-collections/>, <https://starlight.astro.build/reference/frontmatter/>
- Decap CMS widgets: <https://decapcms.org/docs/widgets/>
- Front Matter CMS: <https://frontmatter.codes/docs>
- Quarto scholarly frontmatter: <https://quarto.org/docs/authoring/front-matter.html>
- MyST frontmatter: <https://mystmd.org/guide/frontmatter>
