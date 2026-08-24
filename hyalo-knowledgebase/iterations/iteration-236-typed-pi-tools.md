---
title: "Iteration 236 — typed pi tools: structured find/read/set/task alongside the generic hyalo tool"
type: iteration
date: 2026-08-25
status: planned
branch: iter-236/typed-pi-tools
tags:
  - iteration
  - pi
  - agent-ergonomics
  - extension
related:
  - "[[iterations/iteration-235-agent-cli-ergonomics]]"
  - "[[research/agent-ergonomics-ralph-loop-port-2026-08-24]]"
---

# Iteration 236 — typed pi tools: structured find/read/set/task alongside the generic hyalo tool

## Goal

Replace the argv-assembly footgun with typed tools for the highest-frequency
hyalo operations. The pi extension today registers one generic `hyalo` tool
(`subcommand` + free-form `args[]`); the model assembles CLI argv itself,
and every dogfooding bug so far came from exactly that surface:

- duplicate `--format text` (extension default + model-supplied — PR #262)
- hallucinated `--status` flag (status is a property — PRs #264, #265)
- quoting/escaping of search terms inside `args[]`

Typed tools (`hyalo_find`, `hyalo_read`, `hyalo_set`, `hyalo_task`) give the
model structured parameters instead: no flag spelling, no quoting, no
default-injection collisions — the schema *is* the interface. The generic
`hyalo` tool stays as the escape hatch for everything else.

## Context

Shipped so far (this roadmap): extension repair + prompt presence (#257),
local drift guard `just pi-extension` (#258), post-write lint guardrail
(#259), opt-in session summary (#261), argv-collision and hallucination
patches (#262, #264, #265). Each patch taught the model *not* to make one
specific mistake; typed tools remove the mistake class.

Design decisions (from the session discussion, 2026-08-24):

1. **Typed tools for the ~80% cases only.** `find` (queries), `read`
   (one file, optional section), `set` (frontmatter mutation), `task`
   (toggle). Everything else — `summary`, `lint`, `backlinks`, `config`,
   `links`, `types`, … — keeps the generic tool. Each typed tool is a
   maintenance surface that can drift from CLI flags; the generic tool
   cannot break.
2. **A small typed surface beats a complete one.** Only parameters with
   stable, high-frequency value: `query`, `property` (single `K=V` or list),
   `tag`, `taskStatus`, `count`, `limit` for find; `file`, `section` for
   read; `file`, `property`, `value` for set; `file`, `line` / `--all` for
   task. Exotic flags (`--view`, `--files-from`, `--index-file`, `--jq`)
   stay generic-tool territory.
3. **Shared execution core.** All typed tools build argv and route through
   the *same* `pi.exec` + error-rendering path as the generic tool (one
   `runHyalo(pi, argv)` helper). No behavioral divergence in timeouts,
   signals, or error formatting.
4. **Guidelines shift, not grow.** `promptGuidelines` stays ≤ 3 bullets:
   prefer the typed tools, fall back to the generic tool for anything not
   covered, follow `->` hints. The per-property-flag teaching from #264 can
   be dropped from the tool schema once `hyalo_find.property` makes it moot.

## Tasks

- [ ] Extract a shared `runHyalo(pi, argv, signal)` helper in the template
      (`pi.exec` + exit-code/error rendering + `details: undefined`), and
      refactor the generic tool's `execute` onto it. Pure refactor; behavior
      byte-identical.
- [ ] Implement `hyalo_find` tool: params `query?`, `property?`
      (array of `K=V` filter strings), `tag?`, `glob?`, `taskStatus?`
      (`todo|done|any`), `countOnly?` (→ `--count`), `limit?`; always
      `--format text`; composes nothing else.
- [ ] Implement `hyalo_read` tool: params `file`, `section?`; text output.
- [ ] Implement `hyalo_set` tool: params `file`, `property` (single `K=V`),
      `tag?` (optional add-tag); text output; honors the existing lint
      guardrail automatically (it fires on tool_result regardless of which
      tool wrote).
- [ ] Implement `hyalo_task` tool: params `file`, `mode` (`all|section|line`),
      `section?`, `lines?` (array of ints); dispatches to `task toggle`.
- [ ] Rework `promptSnippet`/`promptGuidelines`/tool descriptions: typed
      tools listed as primary, generic `hyalo` as fallback; drop the
      "no --status flag" bullet if `hyalo_find.property` makes it redundant
      (verify with a live hallucination probe before deleting).
- [ ] Extend `pi-extension-e2e.sh` (layer 4): one forced call per typed tool
      (`--no-builtin-tools`, asserting non-empty structured output), keeping
      the whole guard under ~2 minutes.
- [ ] Update `skill-hyalo-pi.md` template: teach the typed tools first,
      generic tool as escape hatch; keep `--format text` guidance only for
      the generic tool.
- [ ] Live dogfood verification (hyalo-demo worktree): the three historical
      failure scenarios (status filter, search with quotes, set property)
      must each succeed through a typed tool with no clap error and no
      bash fallback.
- [ ] Docs: `docs/configuration.md` `[pi]` section gains a sentence naming
      the typed tools; CHANGELOG unreleased entry.

## Acceptance criteria

- [ ] A fresh pi session with the extension loaded answers "find all
      planned iterations", "read the decision log", "set iteration-236's
      status to in-progress", and "toggle all tasks in file X" using the
      typed tools — no generic-tool calls, no bash, no clap errors
- [ ] `hyalo find --property status=planned`-equivalent query via
      `hyalo_find` returns the same files as the CLI invocation (spot-check
      3 queries)
- [ ] `just pi-extension` covers every registered tool (generic + 4 typed +
      guardrail); all layers green; fmt/clippy/test green
- [ ] No change to the Rust crate — this iteration is template + guard
      script + docs only (`hyalo init --pi` picks it up on re-run)
- [ ] Template still type-checks against installed pi (guard layer 1) with
      the 4 new registrations

## Non-goals

- Typed tools for every subcommand (`summary`, `lint`, `backlinks`, `links`,
  `types`, `views`, …) — generic tool covers them; add one only after a
  dogfood session shows repeated model friction
- Custom `renderCall`/`renderResult` TUI rendering (roadmap item 6b —
  separate iteration; typed tools must land first so renderers have stable
  tool identities to key on)
- MCP server or any non-pi transport
- Changes to hyalo CLI flags themselves (the typed layer maps onto the
  existing CLI; flag design is [[iterations/iteration-235-agent-cli-ergonomics]])

## Out of scope / carry-over candidates

- `--jq` passthrough param on `hyalo_find` (if agents ask for it)
- A `hyalo_lint` typed tool (once a dogfood session shows the model linting
  via the generic tool often enough to justify it)
