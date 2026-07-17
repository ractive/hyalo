---
name: skills
user_invocable: false
description: >
  Author and validate Agent Skills (SKILL.md) with hyalo. Use this skill whenever you are
  creating or editing a skill — a `<name>/SKILL.md` directory whose frontmatter carries a
  `name` slug and a `description`, per the Agent Skills specification
  (https://agentskills.io/specification). Trigger it when: scaffolding a new skill,
  validating a directory of skills against the spec (name regex/length, reserved words,
  name↔directory coupling, description length, body line budget), or checking a skill
  collection in CI. Even if the user does not say "Agent Skills" or "SKILL.md" by name, use
  this skill when the task involves `SKILL.md` files with `name`/`description` frontmatter.
---

# Agent Skills (SKILL.md) — Authoring with hyalo

Agent Skills is a convention for packaging agent capabilities as a directory
`<skill-name>/SKILL.md`. The frontmatter has the hardest machine-checkable constraints of
any format hyalo supports, so hyalo acts as a CI-friendly validator: it owns the
deterministic mechanics (scaffolding, frontmatter validation, the advisory rules); the LLM
does the semantic work (the instructions the skill contains).

## Model

- **Skill** = one directory whose entry point is `SKILL.md`. The directory name IS the
  skill's identity: `my-skill/SKILL.md` must have `name: my-skill`.
- **`name`** (required): a lowercase slug matching `^[a-z0-9]+(-[a-z0-9]+)*$`, 1–64 chars,
  no leading/trailing/consecutive hyphens. Must NOT be a reserved word (`anthropic`,
  `claude`). Must equal the parent directory name.
- **`description`** (required): 1–1024 characters, plain text, no XML/HTML tags (it is
  injected verbatim into a system prompt).
- **Optional-but-typed**: `license`, `compatibility` (≤500 chars), `allowed-tools` (a list
  of tool names). `metadata` is a free-form map — hyalo treats it as opaque and does not
  type it; validate its shape in review.
- **Body budget**: keep the SKILL.md body under 500 lines. Move long reference material
  into companion directories (`references/`, `scripts/`, `assets/`) — those are a
  convention, created by you, not by hyalo.

## Path binding

The `skill` schema is bound to `**/SKILL.md` via `[[schema.bind]]`, so any `SKILL.md`
anywhere in the tree is validated as a skill without needing explicit `type: skill`
frontmatter. Explicit frontmatter always wins.

## Validate

`hyalo lint --profile skills` runs the schema pass plus the Agent Skills advisory rules:

- `SKILL-RESERVED-NAME` (**error**) — `name` is `anthropic` / `claude`.
- `SKILL-NAME-DIRNAME` (warn) — `name` does not equal the parent directory.
- `SKILL-LINE-BUDGET` (warn) — the body exceeds 500 lines.

The hard `name` (regex/length) and `description` (length, no tags) constraints are enforced
by the schema itself. On a vault initialised with `hyalo init --profile skills` (which sets
`[lint] profile = "skills"`), plain `hyalo lint` runs these rules too.

## Scaffold

Create a new skill with the generic `hyalo new`:

```sh
hyalo new --type skill --file my-skill/SKILL.md
```

This writes `my-skill/SKILL.md` with compliant `name` / `description` placeholders. Fill in
the real `name` (it must equal `my-skill`) and a concise `description`, then run
`hyalo lint --profile skills` to confirm the skill is clean.
