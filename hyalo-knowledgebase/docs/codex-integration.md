---
type: docs
title: Codex integration
date: 2026-09-06
status: active
---

# Codex integration

Hyalo provides two installation routes backed by the same skills: project files
created by `hyalo init --codex`, and a reusable Codex plugin. Both invoke the Hyalo
CLI. Install a Hyalo build containing [[iterations/iteration-288-codex-integration]]
for the new init flags; the skills' core query workflows use the 0.22 CLI surface.

## Project installation

```sh
hyalo init --codex
hyalo init --codex --profile okf
hyalo init --claude --pi --codex
```

The installer reads the existing `.hyalo.toml` vault selection and writes a short
managed block in `AGENTS.md`. Project skills live under `.agents/skills/`:
`hyalo`, `hyalo-tidy`, and workflows for every active known profile. Run commands
from the project or its descendants so Hyalo discovers the configuration.

The main skill supports automatic selection. Tidy is explicitly selected, with an
audit-only starter prompt. A requested audit does not authorize repairs. Skills
request lint after edits; no automatic write hook is installed.

Re-run init after upgrading Hyalo. Generated files carry a managed marker and are
replaced on update. Keep personal guidance outside them. An unmarked file at an
installation path causes a conflict before writes; symlinks and malformed managed
markers are refused. A root `AGENTS.override.md` produces a warning because it can
shadow the generated guidance; the installer leaves it intact.

## Plugin installation and coexistence

From a checkout of Hyalo containing the integration:

```sh
codex plugin marketplace add .
codex plugin add hyalo@hyalo
hyalo init --codex --codex-plugin
```

The marketplace manifest points to `plugins/hyalo`. The plugin bundles the two main
workflows and all four profiles, each with a focused description. It requires a
Hyalo executable on PATH and does not bundle a binary, MCP server, or hooks.

Use `--codex-plugin` when configuring each project for the installed plugin. This
removes installer-owned project skill copies and retains project-specific guidance
in `AGENTS.md`. It does not inspect or change user-level plugin configuration.
Keep this flag on subsequent init runs. To switch back to project skills, disable
or remove the plugin in Codex and run `hyalo init --codex`.

Start a new session after setup. Explicit skill selection uses `$hyalo` or
`$hyalo-tidy` for project skills; select the corresponding namespaced entry when
using plugin skills. A nested session inside a Git repository should still
discover root skills and guidance. A standalone directory should be opened as the
workspace root. Plugin availability depends on the client; standalone project
skills provide the IDE route.

## Updates and removal

For a local marketplace checkout, update the checkout, then reinstall with
`codex plugin add hyalo@hyalo` and start a new session. `.codex-plugin/plugin.json`
carries the Hyalo workspace version and is bumped with every release
([[decision-log#DEC-340: The Codex plugin manifest follows the workspace version (2026-10-04)|DEC-340]]).
Do not assume that replacing files refreshes an already-running session.

`hyalo deinit` removes configuration and all managed project integrations, including
Codex. It retains unrelated skills, user-added companion files, and instruction
text outside the managed block. It does not uninstall a globally installed plugin.
Use `codex plugin remove --help` for the installed client's plugin removal command.

## Maintaining the assets

Canonical Codex skills live in `plugins/hyalo/skills/`. Run
`just sync-codex-package` after editing them to refresh the byte-identical embedded
copies under `crates/hyalo-cli/templates/codex/skills/`. Both directions are checked:
missing copies, changed files, and orphaned embedded assets fail CI.

`cargo run -p xtask -- check-codex-package` also checks UI metadata and the marketplace
path. `check-bundled-skills` lints skills in their actual installed `.agents/skills`
location. The crate embeds only files within its own package, so crates.io builds
do not depend on the repository's plugin directory.

The `hyalo-tidy` skill carries two optional Jev assets, `scripts/jev.mjs` (the
helper bundle built from `npm/jev`) and `references/jev.md`. They are used only when
the user explicitly asks for Jev in a tidy session; the helper has no write path and
contacts the network only through `ask --allow-network` with `TYPESAFE_API_KEY`
([[decision-log#DEC-345: Jev assistance for hyalo-tidy is opt-in, read-only and receipt-owned (2026-10-04)|DEC-345]]).
`hyalo init --codex` installs both next to the tidy skill and records their exact
content in `.agents/skills/hyalo-tidy/.hyalo-jev-assets.json`; an upgrade replaces
only bytes that receipt owns, a user-modified file is preserved, and `hyalo deinit`
removes only receipt-owned files. The crate embeds these assets from
`crates/hyalo-cli/templates/jev/` alone, so `sync-codex-package` and
`check-codex-package` skip `hyalo-tidy/{scripts,references}`; run
`cargo run -p xtask -- sync-jev-assets` after changing `npm/jev` or the canonical
reference, and `check-jev-assets` verifies every shipped copy
([[decision-log#DEC-344: The crate embeds the Jev assets from templates/jev only (2026-10-04)|DEC-344]]).

Validate actual loading using a fresh Codex session and a scratch vault. Check a
natural-language search, a requested metadata change followed by lint, and an
audit-only tidy run. Compare the executed commands and resulting files. Installation
and static lint alone do not establish model behaviour. Record client versions and
unavailable surfaces in the iteration results.

## References

- [OpenAI: Build skills](https://learn.chatgpt.com/docs/build-skills)
- [OpenAI: Build plugins](https://learn.chatgpt.com/docs/build-plugins)
- [[iterations/iteration-288-codex-integration]]
