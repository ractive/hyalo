---
title: "Agent-facing ergonomics review — pi + cmux port of ralph-loop (2026-08-24)"
type: research
date: 2026-08-24
status: active
tags: [dogfooding, agent-ergonomics, ralph-loop, cmux, pi]
related: ["[[iterations/iteration-206-links-perf-profiling]]", "[[research/cli-structured-output-patterns]]", "[[research/token-consumption-analysis]]"]
---

# Agent-facing ergonomics review — pi + cmux port of ralph-loop

## Context

Ported the Claude Code ralph-loop skill to pi (skill at `~/.pi/agent/skills/ralph-loop/`),
driving child agents in cmux panes via the modern cmux CLI (`wait-for`, `events`,
`pipe-pane`, `respawn-pane`, `new-pane --direction right`). The port and this review
came out of one long dogfooding session where hyalo was used as the iteration-plan
registry: `hyalo find` for plan discovery, `hyalo properties` for status reads,
`hyalo set` for status writes, `hyalo config` for schema inspection.

Everything below was hit live, not theorized. Severity is about how much agent
friction it caused.

## What worked well (keep doing this)

- **`find` with `--property` + `--jq` is the single best agent surface hyalo has.**
  `hyalo find --property status=planned --jq '.results[].file'` gives a clean,
  one-path-per-line list — exactly what an orchestrator needs to enumerate work.
  One call replaced five greps over frontmatter.
- **Consistent JSON envelope** across `find` / `config` / `set` means `--jq`
  composes predictably across subcommands. The `--jq` escape hatch is what makes
  the default output tolerable for agents.
- **Hints** (`->` drill-down commands in output) genuinely steered navigation
  without doc-reading. `hyalo set` printing a verify command hint after mutation
  is a nice touch.
- **Schema-driven frontmatter** (status enums, branch patterns) made preflight's
  completion-checking trivial and trustworthy: `status=completed` + merge-commit
  grep = two independent verification signals per iteration.
- **`hyalo set` idempotency + skip reporting** ("skipped if the stored value is
  already identical") is the right semantics for agent-driven state writes.

## Findings (friction), ranked

### 1. `--format text` is hostile to machine consumption (high)

The default (and only non-JSON) text format is multi-line per file: quoted path,
indented properties, sections, links. Agents scripting against it must reach for
`--jq` every time, and `--format text` output is *worse than useless* for piping —
it cannot be consumed with `cut`/`grep` reliably.

Evidence: the ported ralph-loop skill now carries **three separate warnings** not
to use `--format text` or `--property 'title~='` for machine consumption, ported
from the original Claude skill where the same traps were documented after real
failures. When a consumer skill must warn against a tool's defaults repeatedly,
the defaults are wrong for that consumer class.

**Proposal:** a find-specific projection flag, `--filenames-only` (grep `-l`
precedent): `hyalo find --property status=planned --filenames-only` prints
one raw file path per line. Not a new `--format` value — `--format` is a
cross-cutting text/json concern, and "paths only" only makes sense for
result-list commands; a find-local flag keeps the API honest. Should conflict
with `--jq`/`--format json` (pick one projection). Sibling flag `--filenames0`
(NUL-delimited) optional for `xargs -0` composability. Zero results → empty
output, exit 0.

Note the mutation side already solved this: `hyalo set --glob ... --where-property ...`
batches mutations with filters in one command — no `xargs` piping needed. Only
the read side (`find`) lacks the compact projection.

### 2. Title-vs-filename mismatch makes title search a trap (high)

Iteration files are `iteration-206-<slug>.md` but their frontmatter `title` is
`"Iteration 206 — ..."`. So the most natural lookup — "find the plan for
iteration 206" — **fails silently** with `--property 'title~=206'` style queries
(different dash, different phrasing) and the correct query is a filename glob:
`hyalo find --glob '**/iteration-206-*.md'`.

This trap was hit by the original skill author repeatedly (it's in the old
skill's comments) and by this port again this session. The skill now warns about
it in two places — that's a workaround living in consumer docs instead of a fix
in the tool.

**Proposals (either or both):**
- A dedicated lookup: `hyalo find --iteration 206` that resolves the
  filename convention (respects `filename-template` from the schema config).
- `title~=` matching that normalizes the iteration id out of titles, or at
  minimum documentation that filename glob is the canonical iteration lookup.

### 3. Vault-boundary error is opaque on absolute paths (medium)

`hyalo set /tmp/foo.md --property status=completed` → `"file resolves outside
vault boundary"`. Correct behavior (safety), but the error doesn't say what the
vault root *is* or how to fix the invocation. An agent's next guess is usually
another absolute path. Suggest including the effective `dir` in the error and a
hint like "pass paths relative to <dir> or run from within it".

### 4. `hyalo properties` output shape differs from `find` (low-medium)

`hyalo properties <file> | jq -r '.status'` works but returns a different shape
than `find`'s envelope; preflight needed its own jq incantation plus a grep
fallback. One more shape to memorize. (Related existing research:
[[research/results-json-shape-inventory]] and [[research/cli-structured-output-patterns]].)

### 5. No `--iteration` / natural-key addressing anywhere (medium, feature)

Everything is paths or globs. But this vault *has* a strong natural key for its
most important document type — the iteration number, encoded in both filename
template and branch pattern. `hyalo set --iteration 206 --property status=completed`
(plus `find --iteration 206`) would make the most common agent operation in this
repository a single unambiguous command with no glob math. This would also
benefit humans.

## ralph-loop integration notes

- ralph-loop now writes iteration status via `hyalo set <plan> --property
  status=completed` (knowledgebase = source of truth), with its own
  `state.json` retained only for run-scoped state (progress bar, resume,
  phase tracking). Two ledgers, but with disjoint responsibilities now.
- preflight's completion check already reads hyalo status — so hyalo writes
  feed back into the next run's preflight automatically. Nice loop closure.
- The `status` enum (`planned/in-progress/completed/...`) covers everything
  ralph-loop needs; no schema changes required.

## Suggested iteration candidates (in priority order)

1. `find --filenames-only` projection flag — unblocks finding #1.
2. Iteration natural-key addressing: `find --iteration N` / `set --iteration N`
   — unblocks findings #2 and #5.
3. Vault-boundary error message with effective dir + fix hint — finding #3.
4. (optional) Unify `properties` output shape with the standard envelope — #4.

Items 1+2 together would let ralph-loop drop *all* of its hyalo-usage warnings.

## Session provenance

Ported from `~/.claude/skills/ralph-loop` (v: state-machine + `claude-teams`
children) to `~/.pi/agent/skills/ralph-loop` (pi children in cmux panes,
`cmux wait-for` handshake instead of polling). Dry-run validated against
iteration-206 plan files with a fake agent binary. Model selection persisted
per-run in ralph state; iteration status persisted in this knowledgebase.
