---
title: Iteration 126 — Markdown linter (mdbook-lint-core embed + HYALO native rules)
type: iteration
date: 2026-05-04
status: completed
tags:
  - iteration
  - lint
  - feature
  - ux
  - llm
branch: iter-126/markdown-linter
---

## Goal

Extend `hyalo lint` from frontmatter-only validation to a full markdown rule engine. Embed `mdbook-lint-core` for stock markdownlint coverage (MD001..MD059) and add three hyalo-native cross-cutting rules that span frontmatter and body — invariants no other tool can check because no other tool has hyalo's parsed model in hand.

The headline value is the **cross-cutting rules**: stock rules ride along as a freebie. Output is shaped for AI agents — per-rule caps, summary mode, hint chains.

A new `hyalo lint-rules` command manages the rule catalog from the CLI (mirroring `hyalo types` / `hyalo views`) so agents don't hand-edit `.hyalo.toml`.

## Design

### Crate organization

New crate `crates/hyalo-mdlint`:
- Wraps `mdbook-lint-core` + `mdbook-lint-rulesets`
- Implements the three HYALO native rules as a `RuleProvider`
- Owns the severity-override table and default-on/off curation
- Re-exports a `LintEngine` factory and a violation type adapted to hyalo's diagnostic shape
- Brings new direct deps: `mdbook-lint-core = "0.14"`, `mdbook-lint-rulesets = "0.14"`, `comrak = "0.21"` (required for the `Rule` trait signature)

The CLI's existing `lint` subcommand grows a body pass that calls into `hyalo-mdlint` after the frontmatter pass.

### Three HYALO native rules

| ID | Rule | Severity | Autofix | Activation |
|---|---|---|---|---|
| HYALO001 | Bare `[]` should be `- [ ]` | error | ✅ | Always |
| HYALO002 | Frontmatter `title` ↔ first H1 agreement | warn | ❌ | When both present and disagree; mode `match \| either \| off`, default `either` |
| HYALO003 | `status: completed` requires all task checkboxes ticked | error | ❌ | Only if `.hyalo.toml` schema declares `status` with enum value `completed` |

HYALO001 is purely line-based (no AST). HYALO002 reads frontmatter (already parsed by hyalo-core) plus the AST's first heading. HYALO003 reads frontmatter `status` plus the existing task model.

### Curated default-on stock rules

Default-on (cheap, autofixable, structural, low false-positive):
MD001, MD009, MD010, MD011, MD012, MD018, MD019, MD022, MD023, MD031, MD034, MD040, MD042, MD047

Default-off (noisy, opinionated, or stylistic):
MD003, MD004, MD007, MD013, MD024, MD025, MD026, MD029, MD033, MD036, MD041, MD046

Other stock rules: leave at upstream default. Users override via `hyalo lint-rules set <ID> --enabled true|false`.

### Severity model

Severity is assigned by hyalo, not by mdbook-lint-core (the upstream crate has no config-level severity override). Per-rule severity table in `hyalo-mdlint`:
- Bugs that break rendering or violate semantic invariants → `error`
- Stylistic / opinionated → `warn`

Violations are post-processed in the `hyalo-mdlint` wrapper after collection from the engine: severity is rewritten according to the table, with user overrides applied last.

### Config schema in `.hyalo.toml`

```toml
[lint]
max_violations_per_rule = 3        # per-rule output cap (text + JSON)
max_files = 50                     # worst-offender file cap

[lint.rules]
# Scalar shorthand: bool toggles enabled
MD013 = false
MD024 = false
HYALO002 = false

# Long form: table for severity / mode
[lint.rules.HYALO002]
enabled = true
severity = "warn"
mode = "match"

[lint.rules.MD013]
enabled = true
severity = "error"
```

`[schema]` (frontmatter rules) stays untouched. Per-rule arg pass-through to mdbook-lint-core (e.g., MD013 `line_length=120`) is **deferred** — toml 0.5 vs 1.x version mismatch makes plumbing non-trivial. Stock rules use upstream defaults in v1.

### `hyalo lint` CLI surface

Existing `lint` semantics preserved (frontmatter pass first, exit 1 on errors). New flags:

```
hyalo lint                          # all enabled rules, summary output
hyalo lint --detailed               # full per-violation output
hyalo lint --rule MD013             # restrict to one rule
hyalo lint --rule-prefix HYALO      # restrict to a prefix
hyalo lint --max-per-rule N         # override per-rule cap (0 = unlimited)
hyalo lint --fix                    # apply autofixes (frontmatter + body)
hyalo lint --fix-rule HYALO001      # only autofix specified rules (repeatable)
hyalo lint --fix --dry-run          # preview without writing
hyalo lint --file <FILE>            # narrow to one file (existing)
hyalo lint --glob <PATTERN>         # narrow by glob (existing)
```

### Output shape

**Default text output (summary mode):**
```
HYALO001 backlog/note.md: 12
MD013 docs/foo.md: 847
MD009 docs/bar.md: 5
HYALO003 iterations/iter-126.md: 1
Total: 865 violations across 4 rules in 3 files (3 autofixable rules)

  -> hyalo lint --rule MD013 --detailed             # see line-length violations
  -> hyalo lint --file docs/foo.md --detailed       # drill into worst-offender file
  -> hyalo lint --fix --dry-run                     # preview 17 autofixable changes
  -> hyalo lint --fix                               # apply autofixable fixes
  -> hyalo find --task todo --file iter-126.md      # HYALO003: see open tasks
```

**JSON envelope:**
```json
{"results": {"files": [
   {"file": "docs/foo.md",
    "rule_groups": [
      {"rule": "MD013", "count": 847, "shown": 3, "truncated": true,
       "severity": "warn", "violations": [...first 3...]},
      {"rule": "HYALO001", "count": 12, "shown": 3, "truncated": true,
       "severity": "error", "violations": [...]}]}],
 "total": 865, "rules_fired": 4, "files_with_violations": 3,
 "files_truncated": false},
 "hints": [...]}
```

**Under `--fix`:**
```json
{"results": {"files": [
   {"file": "foo.md",
    "fixed_groups":     [{"rule": "MD009", "count": 4}],
    "remaining_groups": [{"rule": "MD013", "count": 8, "shown": 3, ...}],
    "conflicts":        [{"rule": "MD047", "reason": "range overlap with MD009"}]}],
 "total_fixed": 12, "total_remaining": 8, "total_conflicts": 1}}
```

The existing flat `violations: [...]` shape under `hyalo lint` JSON is **broken** in favor of `rule_groups`. Acceptable given the small installed base.

### Autofix mechanics

Two passes, byte-disjoint by construction:
1. **Frontmatter pass** (existing). Operates on bytes 0..end-of-`---\n`.
2. **Body pass** (new). Operates on bytes after frontmatter. Collects all `Fix { start, end, replacement }` ranges from violations, sorts by `(start, end, rule_id)`, applies greedily top-to-bottom; any fix overlapping a previously-applied range is dropped and reported as a conflict.

After fix, the file is written atomically and `patch_index_for_modified_files` (existing infra in `dispatch.rs:142`) is called to keep the snapshot index current. `--fix` does not implicitly re-lint; the user reruns to verify.

### Performance & scan strategy

- Full-vault default; `--file` / `--glob` narrow scope (existing semantics).
- Snapshot index does **not** accelerate body lint (body bytes aren't indexed). Documented in `lint --help`.
- One read per file into a `String`; AST parsed lazily only if any enabled rule needs tree access. Line-based rules iterate `doc.lines` directly.
- File-level parallelism via `rayon::par_iter` (rayon already in workspace deps).
- Each worker holds one file's content + AST; results merged at the end.

### `hyalo lint-rules` management CLI

Mirrors `hyalo types` / `hyalo views` shape. New top-level command, edits `[lint.rules]` in `.hyalo.toml` via `toml_edit` (preserves comments and formatting).

```
hyalo lint-rules                                    # default = list
hyalo lint-rules list [--enabled-only|--disabled-only] [--rule-prefix HYALO]
hyalo lint-rules show <RULE_ID>                     # description, default, source
hyalo lint-rules set <RULE_ID> [--enabled BOOL] [--severity warn|error] [--dry-run]
hyalo lint-rules remove <RULE_ID> [--dry-run]       # revert to default
```

`set` writes scalar form `MD013 = false` when only `--enabled` is given; promotes to table form `[lint.rules.MD013]` when `--severity` is also given. `remove` deletes the override entry.

Validation: rule IDs are checked against the running engine's `available_rules()` catalog. Typos error out immediately rather than silently creating dead config. Rule descriptions sourced from `Rule::description()` so the catalog is always in sync.

### Hints

Generic-by-default with per-rule chains for HYALO001/002/003. `MAX_HINTS = 5` (existing convention). Conditional emission (`if count > 0 && hints.len() < MAX_HINTS`).

- Worst-offender drill-down: `→ hyalo lint --rule <X> --detailed`
- Worst-offender file drill-down: `→ hyalo lint --file <Y> --detailed`
- If autofixable violations exist: `→ hyalo lint --fix --dry-run` then `→ hyalo lint --fix`
- Per-rule chains:
  - HYALO001 → `→ hyalo lint --rule HYALO001 --fix`
  - HYALO002 → `→ hyalo read --file <FILE> --lines 1:5  # compare title vs H1`
  - HYALO003 → `→ hyalo find --task todo --file <FILE>  # see open tasks`
- `--fix` rerun hint emitted only when `conflicts > 0`

## Risks

- **Binary size +~3 MB** (release): mdbook-lint-core pulls mdbook 0.4, handlebars, syntect, chrono, comrak transitively (~24 new crates). Acceptable for the audience; documented.
- **toml version mismatch**: mdbook-lint-core uses toml 0.5; hyalo uses toml 1.x. Per-rule arg pass-through deferred to a follow-up if anyone asks.
- **Curated default-on set is a guess** until dogfooded across multiple KBs. Worst case: flip 1–2 rules in v0.15.x patch release after dogfooding feedback.
- **HYALO002's `mode: either` default** may surprise KBs that intentionally keep title and H1 different. Mitigated by `hyalo lint-rules set HYALO002 --enabled false`.
- **`rule_groups` JSON shape** is breaking for anyone scripting against the existing flat `violations: [...]`. Small installed base, acceptable.
- **Stock rule severity is hyalo-controlled**, not upstream-controlled. If mdbook-lint-rulesets adds new rules in a minor version, they land at upstream-default severity and default-off enabled state until we curate them. Document this and pin minor versions.

## Tasks

### Crate scaffolding

- [x] Create `crates/hyalo-mdlint` with `mdbook-lint-core`, `mdbook-lint-rulesets`, `comrak` direct deps; add to workspace `members`
- [x] Add `hyalo-mdlint = { path = "...", version = "0.15.0" }` to workspace deps
- [x] Add `hyalo-mdlint` as a dep of `hyalo-cli`
- [x] Define internal `Diagnostic` type that adapts mdbook-lint-core's `Violation` to hyalo's existing diagnostic shape

### Engine wiring

- [x] Implement `HyaloRuleProvider` registering the three HYALO rules
- [x] Build engine factory: `PluginRegistry::new()` + `StandardRuleProvider` + `HyaloRuleProvider` → `LintEngine`
- [x] Severity-override table baked in as static `HashMap<&'static str, Severity>`
- [x] Default-on/off curation table baked in as static `HashSet<&'static str>`
- [x] Post-process violations: apply severity overrides + filter by enabled set + load user overrides from `[lint.rules]`

### HYALO native rules

- [x] HYALO001 bare `[]` → `- [ ]` with autofix; line-based, no AST
- [x] HYALO002 title↔H1 with mode config (`match` | `either` | `off`); reads frontmatter from `Document` + first heading from AST
- [x] HYALO003 status↔tasks; reads frontmatter `status`, walks tasks; only fires when schema declares `status` with `completed` enum (no-op otherwise)
- [x] Unit tests for each rule (positive + negative cases, autofix idempotency for HYALO001)

### `hyalo lint` extension

- [x] Body-pass orchestration: read file once, lazy AST, run engine, post-process violations
- [x] Group violations by `(file, rule_id)` into `rule_groups`; per-rule cap (default 3)
- [x] Sort files by total-violations desc; cap files (default 50)
- [x] New flags: `--detailed`, `--rule`, `--rule-prefix`, `--max-per-rule`, `--fix-rule`
- [x] Update existing `--fix` to run frontmatter pass first then body pass
- [x] Body autofix engine: collect fixes, sort `(start, end, rule_id)`, apply greedy top-to-bottom, defer overlaps as `conflicts`
- [x] Wire `patch_index_for_modified_files` after `--fix` writes
- [x] File-level parallelism via `rayon::par_iter` over the file list
- [x] Update help text for `hyalo lint` to document new flags + index-non-acceleration note

### JSON envelope

- [x] Replace flat `violations: [...]` with `rule_groups: [...]` in lint output
- [x] Add `fixed_groups`, `remaining_groups`, `conflicts` under `--fix`
- [x] Top-level totals: `total`, `rules_fired`, `files_with_violations`, `files_truncated`

### Hint generation

- [x] Generic worst-offender drill-down hints (`--rule X --detailed`, `--file Y --detailed`)
- [x] Autofix hints (`--fix --dry-run`, `--fix`)
- [x] Per-rule chains for HYALO001/002/003
- [x] Conflict-only `--fix` rerun hint
- [x] Cap at `MAX_HINTS = 5`, conditional emission pattern

### `hyalo lint-rules` command

- [x] Add subcommand to `hyalo-cli` clap surface
- [x] `list` — read engine catalog, merge with `[lint.rules]` overrides; filter flags `--enabled-only`, `--disabled-only`, `--rule-prefix`
- [x] `show <RULE_ID>` — description, default severity, default enabled, autofixable, source crate, current effective config
- [x] `set <RULE_ID> [--enabled BOOL] [--severity SEV] [--dry-run]` — upsert into `[lint.rules]`; scalar form when only `--enabled`, table form when `--severity` also set
- [x] `remove <RULE_ID> [--dry-run]` — delete override
- [x] Validation: reject unknown rule IDs against `engine.available_rules()`
- [x] toml_edit-based mutations preserve comments / formatting
- [x] Hints: drill-down to `lint --rule X` and `lint-rules show X`

### `.hyalo.toml` config plumbing

- [x] Parse `[lint] max_violations_per_rule`, `max_files` into a `LintConfig` struct
- [x] Parse `[lint.rules]` into `HashMap<String, RuleOverride>` (scalar + table form)
- [x] Parse `[lint.rules.HYALO002] mode` for HYALO002's mode config
- [x] Validate unknown rule IDs in config at startup; warn (don't error) for forward-compat

### E2E tests

- [x] `hyalo lint` on a fixture vault produces summary text + JSON `rule_groups`
- [x] `hyalo lint --rule HYALO001 --detailed` returns full per-violation output
- [x] `hyalo lint --max-per-rule 0` returns unbounded per-rule
- [x] `hyalo lint --fix --dry-run` shows expected fixes; no files mutated
- [x] `hyalo lint --fix` mutates files atomically; index patched (verify via `find --index --task todo` after a HYALO001 fix)
- [x] `hyalo lint --fix-rule MD009` only fixes MD009
- [x] Conflict scenario: two rules emit overlapping fix ranges; one applied, other deferred
- [x] `hyalo lint-rules list` includes both stock and HYALO rules
- [x] `hyalo lint-rules show MD013` returns description from upstream `Rule::description()`
- [x] `hyalo lint-rules set MD013 --enabled true` writes scalar form to `[lint.rules]`
- [x] `hyalo lint-rules set MD013 --severity error` promotes to table form
- [x] `hyalo lint-rules set MD9999` errors with "no such rule"
- [x] `hyalo lint-rules remove MD013` deletes the override; subsequent `list` shows default state
- [x] HYALO003 no-op when schema doesn't declare `status` enum
- [x] HYALO003 fires on a file with `status: completed` and an open task

### Docs & integrations

- [x] Update `crates/hyalo-cli/templates/rule-knowledgebase.md` with `lint` and `lint-rules` examples
- [x] Update `CLAUDE.md` (project) with new commands
- [x] Update README with the new lint capabilities
- [x] Update `hyalo-knowledgebase/decision-log.md` with the framing-(B) choice and severity model rationale

### Quality gates (per CLAUDE.md, in order, must pass)

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace -q`

## Acceptance criteria

- [x] `hyalo lint` runs frontmatter + MD + HYALO passes in one invocation; existing frontmatter behavior unchanged for vaults with no `[lint]` config
- [x] Three HYALO rules implemented and tested (HYALO001 with autofix, HYALO002 with mode config, HYALO003 schema-conditional)
- [x] Curated default-on stock rule set (~14 rules) active without user config; remainder default-off
- [x] Output capped at 3 violations per rule and 50 files by default; configurable via `[lint]` and `--max-per-rule`
- [x] `--fix` applies frontmatter and body fixes; conflicts deferred and reported; index patched after writes
- [x] `hyalo lint-rules` list/show/set/remove all work with `--dry-run` support on mutations
- [x] `hyalo lint-rules set` rejects unknown rule IDs
- [x] Hints for HYALO001/002/003 chain into the relevant follow-up commands
- [x] All quality gates pass

## Out of scope (deferred to follow-up iterations)

- Per-rule arg pass-through to mdbook-lint-core (toml version translation layer)
- Global `[lint]` config management CLI (`hyalo lint-config`)
- `CollectionRule` support for cross-document rules (orphans/dead-ends already covered by `find`)
- Custom rule plugins beyond the three HYALO native rules
- LSP-style integration (rumdl-style) — too heavy for this iteration
