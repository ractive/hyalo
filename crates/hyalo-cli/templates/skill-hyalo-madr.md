---
name: madr
user_invocable: false
description: >
  Author and maintain Markdown Architecture Decision Records (MADR 4.0.0) with hyalo. Use
  this skill whenever you are creating or editing ADRs — Markdown files under a
  `docs/decisions/` directory that record an architecture decision with a status
  lifecycle (proposed / rejected / accepted / deprecated / superseded). Trigger it when:
  scaffolding a new ADR, validating ADRs against MADR conventions, checking a supersede
  reference, or regenerating the ADR table of contents. Even if the user does not say
  "MADR" or "ADR" by name, use this skill when the task involves numbered decision files
  (`NNNN-slug.md`) with a `status` frontmatter key.
---

# MADR (Markdown Architecture Decision Records) — Authoring with hyalo

MADR is a lightweight convention for capturing architecture decisions as Markdown files.
Each decision lives in its own file under `docs/decisions/`, named `NNNN-slug.md` where
`NNNN` is a zero-padded sequence number. hyalo owns the deterministic mechanics
(scaffolding, numbering, frontmatter validation, the TOC); the LLM does the semantic work
(the decision narrative, options, consequences).

## Model

- **ADR** = one `.md` file under `docs/decisions/`. Filename: `NNNN-slug.md`
  (e.g. `0007-use-postgres.md`).
- **Status lifecycle** (frontmatter `status`): `proposed` → `accepted` / `rejected`,
  later `deprecated`, or `superseded by ADR-NNNN` when a newer ADR replaces it.
- **Frontmatter** is optional-but-typed: `status`, `date`, `decision-makers` (MADR 4;
  `deciders` in 3.x), `consulted`, `informed`. hyalo validates present fields; nothing is
  hard-required because the path binding already types the file as an `adr`.
- **Required sections** (MADR 4 short template): `## Context and Problem Statement`,
  `## Considered Options`, `## Decision Outcome`.

## Path binding

The `adr` schema is bound to `docs/decisions/**/*.md` via `[[schema.bind]]`, so it applies
only to that subtree — an ADR directory can live inside any larger vault. Files there need
no explicit `type: adr` frontmatter; the binding supplies it.

## Commands

- **Scaffold**: `hyalo new --type adr --file docs/decisions/0007-use-postgres.md`
  (creates the file with `status: proposed`, `date: <today>`, and the required section
  skeleton).
- **Validate**: `hyalo lint` (or `hyalo lint --profile madr`) checks the status pattern,
  required sections, supersede references, and duplicate ADR numbers.
- **Table of contents**: `hyalo madr toc --apply` regenerates a `docs/decisions/README.md`
  index of all ADRs (number, title, status, date) inside a managed region. `--dry-run`
  (the default) exits non-zero on drift — use it in CI.

## MADR advisory lint rules

- `MADR-SUPERSEDE-RESOLVE` — `status: superseded by ADR-0123` warns when no
  `0123-*.md` exists in the ADR directory.
- `MADR-DUPLICATE-NUMBER` — warns when two ADR files share the same `NNNN` prefix.
