---
type: iteration
title: "Iteration 288 — First-class Codex integration via hyalo init --codex"
date: 2026-09-06
status: completed
tags: [iteration, codex, integration, skills, cli]
branch: iter-288/codex-integration
related:
  - "[[iterations/iteration-159-pi-integration]]"
  - "[[iterations/iteration-236-typed-pi-tools]]"
  - "[[iterations/iteration-237-pi-package-distribution]]"
  - "[[iterations/iteration-239-pi-install-verification]]"
  - "[[iterations/iteration-257-init-deinit-dir-scope-and-json-envelope]]"
  - "[[iterations/iteration-168-skills-profile]]"
  - "[[decision-log]]"
---

# Iteration 288 — First-class Codex integration via hyalo init --codex

## Goal

Run `hyalo init --codex` in a project and make Codex discover and use Hyalo for
knowledgebase search, reading, metadata changes, tasks, and maintenance. Install
project skills and a small managed `AGENTS.md` section, with the same configuration,
update, and removal workflow as the existing integrations. The executable remains
`hyalo`; the request's `hylo` spelling is treated as a typo, not a new alias.

This iteration delivers both project integration and a skills-only plugin, sharing
the same workflows. There is no dependency on the planned npm distribution or
TypeScript API iterations. Public directory submission remains a follow-up.

## Existing integrations

Findings from the current code and repository documentation, checked 2026-09-06:

| Surface | What ships today | Implication for Codex |
| --- | --- | --- |
| `init --claude` | Two skills, `.claude/rules/knowledgebase.md`, a managed block in `.claude/CLAUDE.md`, and skills for an explicitly selected profile | Match the workflow using Codex's project discovery locations |
| `init --pi` | Two skills, `.pi/extensions/hyalo.ts`, `.pi/lib/hyalo-api.{js,d.ts}`, and `.pi/package.json` | Reuse CLI workflows; the extension itself depends on Pi APIs |
| Pi package | Root `package.json` points into canonical `pi-package/`; five tools, session setup, and a post-write lint hook | Package installation and live loading need separate verification |
| Claude plugin proposal | `backlog/done/claude-plugin-distribution.md` still has unchecked acceptance criteria; no plugin manifest ships in this checkout | Do not assume that a working Claude plugin can simply be converted |

Implementation centres on `crates/hyalo-cli/src/commands/init.rs`, with flag parsing
in `src/cli/args.rs` and dispatch in `src/run.rs`. `Scope` already decides where
configuration and integration files belong; `Report` already exposes actions in
text and JSON. Reuse both contracts from
[[iterations/iteration-257-init-deinit-dir-scope-and-json-envelope]].

Pi's canonical package is mirrored under `crates/hyalo-cli/templates/pi/`, checked
by `check-pi-package-sync`. The mirror exists because `cargo package` cannot build
an `include_str!` that reaches outside the published crate. Preserve that lesson
when choosing Codex template locations. The live-install failure and fix in
[[iterations/iteration-239-pi-install-verification]] also show why merely checking
that generated files exist is insufficient.

## Integration choice

Codex discovers project skills under `.agents/skills`, scanning from CWD toward the
repository root. Skills support explicit invocation and description-based matching;
optional `agents/openai.yaml` supplies display metadata and invocation policy.
These are documented local extension points, suitable for this installer.
([OpenAI: Build skills](https://learn.chatgpt.com/docs/build-skills))

`AGENTS.md` provides persistent project guidance. Codex prefers an
`AGENTS.override.md` in the same directory and composes instructions along the path
to CWD, so installation must account for overrides and nested launches.
([OpenAI: AGENTS.md](https://learn.chatgpt.com/docs/agent-configuration/agents-md))

Plugins can package skills without an MCP server, using
`.codex-plugin/plugin.json` and a skills directory. They are a viable distribution
layer, with installation and update mechanics beyond copying project files.
([OpenAI: Build plugins](https://learn.chatgpt.com/docs/build-plugins))

Chosen design: support project skills and a reusable skills-only plugin, execute
the existing Rust CLI through Codex's shell, and provide a concise project reminder.
A new MCP server, TypeScript adapter, or custom tool
transport would add another maintained API without being necessary for this goal.

## Generated files and behaviour

Paths below are relative to the existing resolved `Scope.root`:

```text
.hyalo.toml
AGENTS.md                         # upsert only the hyalo-managed section
.agents/skills/hyalo/
  SKILL.md
  agents/openai.yaml
.agents/skills/hyalo-tidy/
  SKILL.md
  agents/openai.yaml
.agents/skills/<profile-skill>/    # when the corresponding profile is active
  SKILL.md
```

`--codex`, `--claude`, and `--pi` compose independently. Plain `init` retains its
configuration-only behaviour. A vault below CWD keeps integration files at CWD;
an external `--dir` places them in that external tree, following existing scope
resolution. Preserve an existing configured vault when `--dir` is omitted: resolve
the effective `dir` from the retained configuration before rendering Codex content,
not from directory auto-detection. Quote paths safely, including spaces.

`init --codex --codex-plugin` writes project guidance and removes managed local
skill copies, for projects using the separately installed plugin. It never changes
global Codex configuration. Canonical assets live under `plugins/hyalo/skills/`;
crate-local copies are synchronized by a Rust xtask and checked in CI. The root
marketplace manifest points at `plugins/hyalo`. The plugin carries its own version.

The managed instruction block identifies the configured vault, points to the
installed skills, prefers Hyalo for supported structured operations, permits body
editing with normal editing tools, and requires linting changed markdown before
handoff. Include brief reminders for active profiles. Keep the block small; command
details belong in the skills and `hyalo <command> --help`.

Re-running `init --codex` refreshes installer-owned files. Mark generated Codex
assets as managed; document that custom guidance belongs outside those files.
Preserve an existing unmarked skill at a colliding path and report the conflict.
Use `<!-- hyalo:start -->` / `<!-- hyalo:end -->` for the instruction block,
preserving all surrounding content. Report a same-directory `AGENTS.override.md`
as a discovery warning; do not modify it automatically.

`deinit` retains its current all-integrations meaning and adds removal of managed
Codex files and the managed `AGENTS.md` block. Remove directories only when empty;
retain unrelated skills, user-added companion files, and instruction text. Neither
command edits global Codex settings, installs packages, or changes approvals.

## Tasks

- [x] Add `codex: bool` to `Commands::Init`, wire it through `run.rs` and the init
      entry points, and include it in the early-return condition. Extend help,
      examples, and `Report` path descriptions. Keep JSON advice inside the report
      rather than printing extra text on stdout.
- [x] Ship the plugin manifest and repository marketplace entry; verify installation,
      updates, and switching between local skills and plugin mode without duplicates.
- [x] Add embedded Codex assets under `crates/hyalo-cli/templates/codex/`, with no
      references outside the published crate. Adapt the Hyalo and tidy workflows:
      remove Claude-specific fields (`user_invocable`, `context: fork`,
      `disable-model-invocation`) and tool names; use accurate CLI examples and
      explicit text/JSON formats. Keep common command guidance reusable or covered
      by the existing drift checks so Codex does not become a stale third manual.
- [x] Give the main skill a focused knowledgebase description and allow implicit
      invocation. Make tidy explicitly invoked via its Codex policy, with a clear
      default prompt. Distinguish an audit request from authorization to repair;
      preserve dry-run/review steps for bulk changes. Include binary discovery,
      configuration inspection, vault-relative paths, limits, error handling, and
      post-edit linting. If the executable is missing, explain installation using
      supported release channels rather than downloading it silently.
- [x] Implement the installer and managed instruction block using `Scope` and
      `Report`. Check Codex destinations before writing: unowned collisions,
      malformed markers, and symlinks must not overwrite unrelated content or
      write outside the selected root. Return actionable diagnostics; filesystem
      failures retain the existing init/deinit error-envelope contract.
- [x] Install Codex-compatible profile skills for all active known profiles in
      the resulting configuration, including profiles enabled before Codex was
      added. Reuse `profiles::PROFILES` and common bodies where compatible; adapt
      host-specific wording and metadata explicitly. Refresh profile reminders
      on each run without changing profile merge semantics.
- [x] Extend the skills profile's `[scan] include` to reach `.agents/skills/**`,
      preserving `.claude/skills/**` and user scan settings. Update the associated
      profile guidance. Extend `check-bundled-skills` to discover Codex templates
      and validate the actual installed `.agents/skills` layout, not only copies
      placed in a Claude directory. Validate `openai.yaml` metadata separately.
- [x] Extend deinit to remove only the managed Codex assets and instruction block,
      including profile assets and metadata. Preserve unowned collisions and
      user-added files. Test repeated removal and removal scoped to an external
      vault without touching the invoking checkout.
- [x] Add Rust unit/e2e coverage in the existing init suites: fresh install,
      repeat/update, all flag combinations, existing custom config without
      `--dir`, relative/absolute/external paths and spaces, profile composition,
      JSON reporting, content preservation, collisions, malformed markers,
      symlinks, and deinit. Include unknown-profile failure before artifact writes.
- [x] Update README agent support and installation instructions, init/deinit help,
      affected bundled guidance, and CHANGELOG. Record the local-skills decision
      in [[decision-log]] during implementation. Explain that vendored skills update
      when Hyalo is upgraded and init is rerun; linting is a skill instruction in
      this version, not Pi's automatic post-write hook.
- [x] Verify actual use in a fresh Codex CLI session in a scratch Git project:
      ordinary search from the root and a nested vault directory, explicit tidy
      selection, a requested property change followed by lint, and an audit-only
      request. Inspect executed commands and the resulting diff; record versions.
- [x] Repeat the live workflow in the separate Codex desktop app, including fresh
      launches from the project root and a nested directory. Deferred at the
      partial merge on 2026-09-06; closed on the user's successful workflow report
      on 2026-09-08, with client version and evidence limits recorded below.
- [x] Run implementation gates: `cargo fmt`,
      `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, all `xtask check-*`, and `hyalo lint --strict`.
      Verify the CLI's package build contains every embedded Codex asset. Build
      the release binary and dogfood the new integration on a scratch copy of this
      knowledgebase, preserving the working checkout's own agent configuration.

## Acceptance criteria

- [x] `hyalo init --codex` installs the documented project artifacts offline,
      without requiring Codex to be installed to generate them.
- [x] Fresh Codex CLI sessions discover the skills and use the installed Hyalo CLI for
      representative knowledgebase work; versions and observed behaviour are recorded.
- [x] Repeat init refreshes managed content without duplicate instruction blocks,
      preserves unrelated content, and uses the configured vault consistently.
- [x] Codex works alone and alongside Claude/Pi; active profile skills are available
      and installed Codex skills pass Hyalo's skills-profile validation.
- [x] `deinit` removes the managed Codex integration while retaining user content;
      external-vault operations leave the invoking checkout untouched.
- [x] JSON init/deinit reports remain parseable and accurately name affected paths,
      actions, and any conflicts or discovery warnings.
- [x] Audit-only requests do not mutate notes; requested edits are followed by lint.
- [x] Rust, drift, package-build, and documentation gates pass.

## Implementation results

Implemented on `iter-288/codex-integration`, verified on macOS with Codex CLI
`0.153.4` on 2026-09-06. Project installation, plugin mode, profile workflows,
managed removal, documentation, and CI drift checks are implemented.

### Automated checks

- `cargo fmt`, strict workspace Clippy, and `cargo test --workspace -q` pass:
  4,868 tests passed and two doctests were ignored. This includes all 2,152 CLI
  end-to-end tests and the new Codex install/remove safety coverage.
- All implemented `xtask check-*` gates pass: feature fanout, help drift, command
  reference, 14 bundled skills, 12 synchronized Codex assets, Pi package parity,
  executable jq recipes, and the mutation-journal guard. The dead-primitives and
  TODO-annotations commands still report their existing unimplemented status.
- The jq gate exercised 42 recipes; its existing MADR TOC recipe was not
  exercisable because this vault has no `docs/decisions` directory.
- Strict knowledgebase lint exits successfully. The new iteration, integration
  documentation, and decision-log entry have no findings; the full vault retains
  four existing warnings about unchecked tasks in completed iterations 270, 271,
  277, and 281. `git diff --check` passes.

### Installation and discovery

- The official plugin validator and all six skill validators pass. The plugin
  manifest remains version `0.1.0`; no global user installation was changed.
- Installed the repository marketplace and `hyalo@hyalo` into an isolated scratch
  Codex profile. The CLI app-server's `skills/list` reports all six plugin skills,
  with display metadata and no loading errors.
- After switching a scratch Git project to `--codex-plugin`, fresh app-server
  discovery from both root and nested directories reports exactly six Hyalo
  skills, all owned by the plugin, with no duplicate project copies.
- Copied the marketplace into a scratch tree, applied the official cachebuster
  update helper there, and reinstalled. Codex loaded version
  `0.1.0+codex.20260906181244` from the updated cache in both launch locations.
- Built the release executable and ran project installation and a vault-relative
  query against a scratch copy of this knowledgebase. The working checkout's
  own project instructions and user-level Codex configuration were untouched.

### Packaged build

Cargo's normal offline package verification could not resolve the unpublished
workspace crates; workspace verification also hit Cargo's temporary-registry
`no hash listed for hyalo-core` error. As a separate verification, packaged the
workspace with `--no-verify`, extracted the three `.crate` archives, and built the
extracted CLI offline with dependency overrides pointing only to its extracted
core and markdown-lint siblings. That build succeeded and its executable installed
the Codex integration successfully. No repository plugin path was used by the
extracted build, confirming the embedded assets are self-contained.

### Review fixes and live CLI verification

Independent local review and Copilot feedback led to two verified fixes: generated
guidance now limits ancestor configuration discovery to the project root or inside
the configured vault, and managed-file rewrites preserve Unix permission bits,
including macOS set-ID bits. Regression tests cover both. Permission tests were
run outside the outer sandbox, which otherwise strips set-ID fixture bits.

An attended, fresh Codex CLI `0.153.4` session using `gpt-6-astra` ran in cmux
against a committed scratch Git fixture on 2026-09-06. Project skills were generated
by the reviewed implementation at `1607edc6`. The session read `AGENTS.md` and both
installed skills, selected Hyalo for the ordinary search, and used the explicitly
requested `$hyalo-tidy` for the audit. Most commands used the existing installed
`hyalo 0.22.0 (625c5c19510d 2026-09-05)`; one configuration check used the current
debug binary. This validates the new guidance with the installed compatible CLI.

- `hyalo find --property status=planned --format text` from the root and
  `notes/nested` returned the same single note. The fresh session launched at the
  root and executed the nested query with a changed command CWD; this was not a
  separate nested interactive launch.
- `hyalo set planned.md --property status=in-progress` was followed by
  `hyalo lint` and `hyalo lint planned.md`. Both reported zero errors and the
  fixture's expected broken-link warning.
- The audit ran `config`, `summary`, `lint`, `find --broken-links`, `find --orphan`,
  `types list`, note reads, and status queries. It identified the intentional
  `[[missing-reference]]` and orphan control note without repairing either.
- A before/after file-hash comparison around the audit reported no changed files.
  The final Git diff contained only the requested status replacement; the edited
  note's body and the control note were unchanged. `git diff --check` passed.

### Desktop verification — user confirmed (2026-09-08)

On 2026-09-06 the user accepted deferring the desktop workflow and requested
merging PR #331. The successful interactive CLI test in cmux was recorded
separately; its temporary pane has been closed.

On 2026-09-08, after receiving the desktop verification steps for fresh root and
nested-directory chats, ordinary search, a property update followed by lint, and
an explicitly invoked audit without changes, the user reported: "Everything is
working perfectly. The skills are discovered." The reported client is
**ChatGPT Version 26.901.51231**.

This closes the deferred desktop task on the basis of the user's manual workflow
confirmation. Skill discovery is explicitly confirmed. No desktop command
transcript or before/after diffs were supplied in this conversation, so this
record does not claim independent inspection of those artifacts.

### Reconciliation — iteration 287 companion assets (2026-09-07)

Iteration 287 adds the self-contained generated Pi API runtime and declaration
under `.pi/lib/`, outside the auto-discovered extensions directory. The
`init --pi` inventory above, crate-local mirror, sync gate, and deinit behavior
now include these two companion files. This does not change Codex integration
behavior or satisfy the deferred desktop verification task.

## Non-goals and follow-up

- No global install mode, CLI rename, custom Codex tool server, or changes to
  sandbox and approval policy.
- No automatic hook enforcement in this iteration. Consider it only after a
  separate, tested Codex hook design with clear parity and failure behaviour.
- Public directory submission and remote publication are separate from this
  iteration. Plugin packaging, local installation/update verification, and shared
  asset maintenance are in scope.
