---
title: "Path-bound schemas — binding types/profiles to globs and files"
type: research
date: 2026-07-16
status: active
tags: [research, profiles, schema, globs, lint, design]
related: [research/profile-candidates-beyond-okf.md, research/okf-open-knowledge-format.md]
---

# Path-bound schemas

Can schema types / profiles be bound to paths — `engineering/architecture/adrs/**` → madr, `CHANGELOG.md` → changelog? Codebase research 2026-07-16 (all citations verified).

## What exists today

- **Type dispatch is exclusively `type:` frontmatter.** Lint: `crates/hyalo-cli/src/commands/lint.rs:515`; `validate_on_write`: `set.rs:335`; `new --type` is explicit. No `type:` → `[schema.default]` applies.
- **One embryonic path→type precedent already exists:** `infer_type_from_path()` (`lint.rs:776`) — during `lint --fix`, a file *without* `type:` is matched against every type's `filename-template`; an unambiguous single match auto-inserts the type. `FilenameTemplate` has `.to_glob()` (`filename_template.rs:114`). Path-bound schemas extend this seam.
- **Glob infra is centralized on `globset`** (literal_separator, backslash_escape, `**`, Windows paths normalized to `/`): discovery `match_globs()` (`discovery.rs:405`), find/views (`filter_index.rs:14`, `!` negation), `lint.ignore` (`dispatch.rs:1695`).
- **Lint sees `(full_path, rel_path)` per file** — the path is available exactly where validation resolves the schema.
- **Exactly one `.hyalo.toml`** — nested configs are detected and *ignored* with a warning (`config.rs:250`). Any binding must live in the flat root config.
- **No per-path anything else**: no `[schema] exempt` yet (planned, iter-163), no per-path lint-rule scoping, no per-dir schema overrides.

## Options considered

| | 1. `match=[]` globs on each type | 2. `[schema.bind]` ordered path→type map | 3. Per-path profile overlays |
|---|---|---|---|
| Shape | scattered per-type | centralized, audit-friendly | separate override contexts |
| Conflicts | ambiguous match → silent no-infer | first match wins (predictable) | multi-profile merge complexity |
| Effort | ~200 LoC | ~150 LoC | ~250 LoC |

## Decision: `[schema.bind]` (option 2), with two sharpenings

```toml
[schema.bind]
"engineering/architecture/adrs/**" = "adr"
"**/SKILL.md" = "skill"
"CHANGELOG.md" = "changelog"
```

Ordered, first match wins; compiled to a `GlobSet` at schema-load time; unknown target type → config warning.

1. **Binding assigns the effective schema even when `type:` frontmatter is absent or impossible.** It is *not* merely `--fix`-time inference: `CHANGELOG.md` (no frontmatter, ever) must get its rules purely by path. Precedence: explicit `type:` frontmatter always wins; frontmatter↔binding mismatch (a `type: note` file inside `adrs/`) → **warn-level lint**, a genuinely useful diagnostic. Inference-on-fix (`lint --fix` inserting `type:`) keeps working and consults bindings after `filename-template`.
2. **Profiles must be composable.** madr and changelog bindings coexist in one vault, so `hyalo init --profile <p>` is additive — each run upserts that profile's fragment (types + bind entries + lint config) into the one `.hyalo.toml`. No "one profile per vault".

**Relationship to `[schema] exempt` (iter-163):** exempt is "no schema requirements here" — logically bind-to-nothing. Keep exempt as planned (163 ships before bind exists); once bind lands, consider `"**/index.md" = "none"` sugar and document the overlap. Don't block 163 on this.

**External prior art** (from [[profile-candidates-beyond-okf]]): Jekyll `defaults: scope: {path, type}`, Decap folder collections, Hugo `cascade._target.path`, ESLint `overrides.files` — first-match/last-match ordered glob binding is the industry-standard shape.

## Scheduling

Follows this chain's pattern — capability ships with its first consumer:

- **[[iteration-167-madr-profile]]** implements `[schema.bind]` + binds the ADR directory (init `--profile madr` asks/derives the path, default `docs/decisions/**`)
- **[[iteration-168-skills-profile]]** consumes it (`"**/SKILL.md" = "skill"`) — resolves that iteration's open "filename-based dispatch" question
- **[[iteration-169-changelog-profile]]** consumes it (`"CHANGELOG.md" = "changelog"`, frontmatter-less by binding + exempt)
- **[[iteration-164-okf-init-profile-and-skill]]** must build profile fragments composable/upsertable so bind entries slot in later
