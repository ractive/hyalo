# Linting your vault in CI (GitHub Actions)

> Part of the [hyalo](../README.md) documentation.

Gate every change on a clean vault. The [`setup-hyalo`](https://github.com/ractive/setup-hyalo) action installs the prebuilt release binary in seconds — no compilation — so a full-vault gate is two steps:

```yaml
# .github/workflows/lint-kb.yml
name: Lint knowledgebase
on:
  push:
    branches: [main]
jobs:
  lint-kb-full:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: ractive/setup-hyalo@v1          # installs hyalo, adds it to PATH
      - run: hyalo lint --strict --format github
```

Pin the binary with `with: { version: v0.20.0 }` (the default `latest` tracks the newest release). Run the job from the repo root: `.hyalo.toml` supplies the vault `dir`, and annotation paths are emitted relative to the repository root so they land on the right file and line.

## Diff-aware linting on pull requests

On a **pull request**, use the diff-aware variant instead: pipe `git diff` into `--files-from -` to lint only the files the PR touches (non-markdown paths are skipped and vault-dir prefixes are stripped automatically — no pre-filtering needed). Check out with `fetch-depth: 0` so the merge-base is available:

```yaml
jobs:
  lint-kb:
    if: github.event_name == 'pull_request'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0                      # full history for the three-dot merge-base
      - uses: ractive/setup-hyalo@v1
      - run: |
          git fetch --quiet origin "${{ github.base_ref }}"
          git diff --name-only --diff-filter=d "origin/${{ github.base_ref }}...HEAD" \
            | hyalo lint --strict --format github --files-from -
```

Three-dot `origin/<base>...HEAD` diffs against the *merge-base* of the PR base and HEAD, so a stale branch stays scoped to the files it changed — not to base-tip drift it never touched. The diff-aware shape matters because GitHub registers at most **10 error + 10 warning inline annotations per step**: scoping to the PR's own files spends that budget on findings the author can act on, while the full-vault job (on `push` to main, where the cap is irrelevant) catches the cross-file regressions a diff can't see — a deleted note others link to, a schema change.

## How `--format github` behaves

`--format github` emits one [GitHub Actions workflow command](https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions) per violation — `::error` / `::warning` lines that render as inline annotations on the PR diff — in deterministic `(path, line, rule)` order, appending a `::notice::` with the true totals whenever the counts exceed GitHub's inline cap. `--strict` promotes warnings to errors — including `HYALO006` broken links — so the job fails on a broken link; unparseable frontmatter is always an error (`HYALO005`), so a green lint means the vault is genuinely clean. `--files-from -` works on most file-taking commands (`find`, `set`, `mv`, `task`, …), not just `lint`.

## OKF reserved-file drift check

For **OKF** vaults, add a reserved-file drift check — `hyalo okf index` is dry-run by default and exits non-zero when any `index.md` is stale:

```yaml
      - run: hyalo okf index   # dry-run; non-zero exit on drift
```

## `@claude` agent on GitHub (claude-code-action)

Because [`setup-hyalo`](https://github.com/ractive/setup-hyalo) puts the binary on
`PATH` before the agent runs, a [`claude-code-action`](https://github.com/anthropics/claude-code-action)
workflow can hand `@claude` the full hyalo toolbox — so an `@claude` mention on a
PR or issue can triage and *fix* lint findings (`hyalo lint --fix`, `hyalo set`,
`hyalo mv`) rather than just report them. Commit the hyalo skill first with
`hyalo init --claude` (add `--profile okf` for an OKF bundle) so the agent knows
to prefer the CLI over raw file edits.

```yaml
# .github/workflows/claude.yml
name: claude
on:
  issue_comment:
    types: [created]
jobs:
  claude:
    if: contains(github.event.comment.body, '@claude')
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
    steps:
      - uses: actions/checkout@v4
      - uses: ractive/setup-hyalo@v1          # hyalo on PATH before the agent starts
      - uses: anthropics/claude-code-action@v1
        with:
          anthropic_api_key: ${{ secrets.ANTHROPIC_API_KEY }}
          # Let the agent run any hyalo subcommand (lint/find/set/mv/task/...).
          allowed_tools: Bash(hyalo:*)
```

The committed skill routes the agent to commands like
`hyalo lint --strict --format github` (see findings), `hyalo lint --fix`
(auto-fix the fixable rules), and `hyalo set <file> --property status=done`
(targeted frontmatter edits). Fix-mode and read-only lint use different JSON
shapes (`remaining_groups` vs `rule_groups`); `--format github` renders both, so
`hyalo lint --fix --format github` still annotates any violation left unfixable
(e.g. a missing required property) after the auto-fix pass.

## Living reference

hyalo's own `lint-kb`/`lint-kb-full` jobs in [.github/workflows/ci.yml](../.github/workflows/ci.yml) are the living reference.
