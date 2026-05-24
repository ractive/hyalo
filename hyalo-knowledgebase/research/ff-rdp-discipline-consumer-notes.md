---
title: "ff-rdp ralph-loop discipline — consumer feature wishlist"
type: research
date: 2026-05-24
status: discussion
origin: ff-rdp post-iter-62 session (2026-05-24); discussion between James and Claude
tags:
  - feedback
  - consumer
  - lint
  - schema
  - templating
---

# ff-rdp ralph-loop discipline — consumer feature wishlist

## Context

The [ff-rdp](https://github.com/ractive/ff-rdp) project runs an LLM-driven
iteration loop (`ralph-loop`) that auto-implements numbered iteration
plans stored as markdown files with YAML frontmatter under
`kb/iterations/`. Each plan ships with required frontmatter
(`title`, `status`, `branch`, `dogfood_path`, …) and a required body
shape (`## Themes`, `## Tasks`, `## Acceptance Criteria`,
`## Design notes`, `## Out of scope`, `## References`). A separate
"iteration discipline" toolchain validates plans + branch diffs at PR
time.

Today that toolchain lives in two places that overlap with hyalo:

- **`crates/xtask/src/check_iteration_plan.rs`** — reimplements
  required-property enforcement, status-enum validation, and YAML
  parsing for plan frontmatter. Was added before the team noticed
  hyalo's `lint` + `types` surface could do this natively.
- **`tools/ralph-loop/scripts/*.sh`** — `ac-fidelity-check.sh`,
  `claims-vs-code.sh`, plus `run-iteration.sh` which `sed`-edits
  frontmatter to flip `status: planned` → `status: done` post-merge.

A planned iteration (`iter-73` in ff-rdp) will move the schema bits to
`[schema.types.iteration]` in `.hyalo.toml` and shrink the xtask to
just the one rule hyalo can't express today: *"if the plan body
mentions new `pub` items, `first_call_sites:` must be non-empty."*

This note captures what additional hyalo features would let ff-rdp
**delete even more** of its in-house discipline tooling — filtered by
James's feedback on what's reasonable for hyalo to absorb vs what
should stay project-side.

## Features worth considering (filtered)

These are the items James flagged as plausible. The earlier brainstorm
also included structured-object frontmatter, pluggable lint rules, a
status state machine, cross-file invariants, status-coherence autofix,
severity escalation, native AC semantics — all judged either too
domain-specific or too heavy for hyalo's scope.

### A. Per-item regex on string-list properties

**Problem.** `first_call_sites` is naturally a list of `{primitive,
site}` pairs. hyalo's schema doesn't model nested objects, and James
agrees stuffing nested objects into frontmatter is the wrong direction.

**Workaround already viable today.** Encode each pair as a single
string with a delimiter:

```yaml
first_call_sites:
  - "ff_rdp_cli::util::safe_write => crates/ff-rdp-cli/src/commands/screenshot.rs"
  - "ff_rdp_cli::util::safe_create => crates/ff-rdp-cli/src/commands/auto_consent.rs"
```

`hyalo set --property first_call_sites=[…]` works, the convention is
documented, the xtask parses each item with `split(" => ")`.

**What would help hyalo-side: a per-item regex validator on string
lists.** Strawman:

```toml
[schema.types.iteration.properties.first_call_sites]
type = "string-list"
item_pattern = '^[\w:]+ => crates/[^ ]+$'
```

Errors at lint time if any item violates the pattern. Strictly smaller
than nested-object support; reuses the existing pattern machinery
already wired for scalar string properties.

**Alternative considered.** Two parallel string-lists with a
`same_length_as` constraint (`first_call_primitives` +
`first_call_sites`). Schema-validatable but visually awkward (eye has
to hop between lists to pair them). Per-item regex on a single list is
cleaner.

### B. `--files-from <path|->` instead of `--since <git-ref>`

**Problem.** Many discipline checks want to lint *only what changed in
this branch* — full-vault lint produces noise on every PR. The obvious
ergonomic is `hyalo lint --since origin/main`.

**James's concern.** Pulling git into hyalo (even via `gix`) is
non-trivial weight and ties hyalo to a single VCS.

**Cleaner split.** Hyalo accepts `--files-from <path>` or
`--files-from -` (stdin) with one path per line. The caller computes
"what changed" using whatever tool fits — git, hg, fossil,
`make .changed`, a yaml manifest — and pipes the result:

```sh
git diff --name-only origin/main -- 'kb/**/*.md' | hyalo lint --files-from -
```

Hyalo stays VCS-agnostic; callers handle scope. As a bonus, this is
useful outside CI (a script that already knows which files to lint).
Minimal surface addition.

`hyalo diff <revA>..<revB>` (also discussed) becomes unnecessary —
`git diff` already shows raw markdown diffs, and structured-delta
views are a CI/reviewer concern.

### C. Required body sections in the schema

**Problem.** ff-rdp's iteration plans have a known body shape
(template under `kb/iterations/_template.md`). Nothing enforces the
sections actually exist on each plan; agents occasionally write a plan
that's missing `## Acceptance Criteria` or `## Out of scope` and the
PR ships before a human notices.

**Strawman.**

```toml
[schema.types.iteration]
required_sections = [
  "Themes",
  "Tasks",
  "Acceptance Criteria",
  "Design notes",
  "Out of scope",
  "References",
]
```

`hyalo lint` walks the body's H2 headings (level configurable) and
fails with a clear `missing section: Acceptance Criteria` on absence.
Optional `non_empty = true` per section if "header exists AND has body
content" matters. New native rule slot — call it HYALO003.

Few hundred lines on hyalo's side. Replaces a "did the template stay
intact?" check that today nobody runs.

### D. `hyalo new --type <name>` from a schema-declared template

**Problem.** Today the agent stamps a new plan by reading
`_template.md` and substituting placeholders by hand. Errors:
- forgets to fill `branch:` so the pattern check fails
- leaves `YYYY-MM-DD` in `date:`
- numbering / slug naming drifts from convention
- the filename and `branch:` value don't agree on the id

**Strawman.**

```toml
[schema.types.iteration]
template          = "_template.md"
filename_template = "iteration-{id}-{slug}.md"
```

```sh
$ hyalo new --type iteration --id 73 --slug hyalo-schema
created kb/iterations/iteration-73-hyalo-schema.md
```

Variable substitution: `{id}`, `{slug}`, `{date}` (auto `today`),
`{title}` (computed `Iteration {id}: {Slug Titlecased}`). Simple
`{var}` replacement — no templating-engine dependency (tera /
handlebars overkill).

Combines well with C: `hyalo new` produces a plan that already
satisfies the schema; `hyalo lint` keeps it that way on save; CI fails
when an edit drifts. The "agent forgot to fill `branch:`" failure mode
disappears at source.

## Summary table

| Feature | Hyalo-side cost | Consumer benefit | James's call |
|---|---|---|---|
| A. Per-item regex on string-lists | small | unlocks `first_call_sites`-style fields with no nested-object pain | tentative yes |
| B. `--files-from` | tiny | unblocks diff-aware lint without VCS coupling | yes |
| C. Required body sections | medium | catches template-drift at lint time | yes |
| D. `hyalo new --type` | medium | removes the most common agent-stamping error class | yes |

## Out of scope per James's filter

- Structured-object frontmatter (`first_call_sites: [{primitive,
  site}]`). Solved by A above without nested objects.
- Pluggable / custom lint rules (WASM, Lua, TOML rule descriptors).
  Too far.
- Status state machine (`[status_transitions]`). Too far.
- Cross-file invariants via `depends_on:` resolution. Too far.
- Schema-aware autofix for status coherence
  (`in-progress → done` when all tasks ticked). Workflow-specific.
- `hyalo diff` as a first-class command. Covered by `git diff` + B.
- Severity escalation by context. Too specific.
- Native AC / `## Acceptance Criteria` parsing. Too domain-specific.
- Bulk property rename. **Already exists** via `hyalo properties
  rename` — note for the ff-rdp side, not a hyalo gap.

## Consumer-side follow-up

In ff-rdp, `iter-73` will land *with today's hyalo capabilities only*
— `[schema.types.iteration]` + `hyalo lint` in CI + xtask shrink + use
of `hyalo set` for status flips. If hyalo later ships A / B / C / D,
ff-rdp can file follow-up iterations to drop the remaining xtask
surface and the `_template.md` substitution path.

## References

- Source conversation: ff-rdp post-iter-62 session, 2026-05-24, between
  James and Claude (Opus 4.7).
- ff-rdp iteration plan: `kb/iterations/iteration-73-hyalo-schema-for-iteration-plans.md`
  (in the ff-rdp repo).
- ff-rdp discipline rule reference: `CLAUDE.md` *"Iteration
  discipline"* section.
