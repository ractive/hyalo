---
title: "links auto: persistent exclusions in .hyalo.toml"
type: backlog
date: 2026-07-04
tags:
  - backlog
  - links
  - auto-link
  - config
status: planned
priority: low
origin: external-user dogfood 2026-07-04 (third-party vault)
---

# links auto: persistent exclusions in .hyalo.toml

## Problem

On a vault whose page titles double as common English words, `hyalo links auto`
is dominated by noise: an external user's dry-run found 33 candidates of which
~94% were junk — 24× the prose word "permissions" (would spam-link a how-to
guide) and 7× "README" mentions that referred to the repo-root README but would
have linked to the vault's index page (actively wrong).

`--exclude-title` and `--first-only` make the feature usable, but they are
per-invocation CLI flags only. The working incantation for that vault —

```bash
hyalo links auto --exclude-title permissions --exclude-title README --exclude-title index --first-only
```

— has to be remembered and retyped every time. The `[links]` config section
currently supports only `case_insensitive`; there is no way to persist
auto-link preferences per vault.

## Proposal

Add a `[links.auto]` section to `.hyalo.toml`:

```toml
[links.auto]
exclude_titles = ["permissions", "README", "index"]
exclude_target_globs = ["templates/*"]
first_only = true
```

- CLI flags are additive for the list options and override for `first_only`.
- `hyalo links auto` output should mention when config exclusions are active
  (e.g. a `config_excluded` count) so a bare run stays explainable.
- Optional stretch: a warning when a candidate title is a very common English
  word (dictionary/stopword heuristic) suggesting it be excluded — the noise
  source here is inherent to titles like "permissions", not vault-specific.

## Prior art (existing exclusion mechanisms)

Per-invocation excludes are widespread; persistent ones have exactly one precedent:

| Mechanism | Commands | Persistent? |
|-----------|----------|-------------|
| `--glob '!pattern'` negation | find, lint, mv, set, remove, append (feature-matrix-enforced) | no |
| `-term` body-query negation | find (BM25) | no |
| `K!=V`, `!K` property filters | find, `--where-property` on mutations | no |
| `--exclude-title`, `--exclude-target-glob` | links auto | no |
| `--ignore-target <substring>` | links fix | no |
| `.gitignore` respected by discovery | all (file discovery) | yes (file-level) |
| `lint-rules set <ID> --enabled false` → `[lint]` in `.hyalo.toml` | lint | **yes — the config precedent** |

Design should follow the `lint-rules` precedent for persistence and the
`exclude_*` naming already used by `links auto` itself. Naming nit while in
the area: `links fix` calls the same concept `--ignore-target` — consider
aligning (alias, keep backwards compat).

## Acceptance criteria

- [ ] `[links.auto] exclude_titles` suppresses matches without any CLI flags
- [ ] CLI `--exclude-title` extends (not replaces) the config list
- [ ] `first_only = true` in config behaves like the flag; flag still wins per run
- [ ] `hyalo config` shows the effective `[links.auto]` settings
- [ ] Help text + README + [[schema-and-lint]]-adjacent docs updated in the same PR
