# Staged `ractive/setup-hyalo` repo

> **PUBLISHED 2026-07-17** — the live repo at
> <https://github.com/ractive/setup-hyalo> (tags `v1.0.0`, `v1.0.1`, `v1`) is
> now the source of truth; this staged copy is a historical snapshot. Delta vs
> this tree: the smoke fixture lint runs plain `--strict` (no `--format
> github`) until a hyalo release > v0.17.0 ships the annotations format; this
> `PUBLISH.md` runbook was not pushed to the public repo. The 6-job smoke
> matrix is green. Flipping hyalo's `lint-kb` CI job to the action remains
> **blocked on that next release** — the job uses `--format github`, which the
> action-installed release binary doesn't have yet.

This directory is the **complete, ready-to-push tree** for the standalone
`ractive/setup-hyalo` GitHub Action (see [[decision-log#DEC-051]] and
[[iterations/iteration-171-setup-hyalo-action]]).

It lives under `research/` (a lint-ignored path) because the automated iteration
run was **not permitted to create a new public GitHub repository** — that action
requires human authorization in the web UI.

## To publish

```sh
cd hyalo-knowledgebase/research/setup-hyalo-action
git init && git add -A
git status   # review: only the staged action files, no .DS_Store/editor droppings
git commit -m "feat: setup-hyalo composite action"
gh repo create ractive/setup-hyalo --public --source=. --push
git tag v1.0.0 && git tag v1
git push origin v1.0.0 && git push origin v1
```

Then, **in this order**:

1. Wait for the action repo's `smoke` workflow to go green on all three OSes
   (ubuntu / macos-14 / windows). The Windows unzip path has only been exercised
   in CI — it was not part of the local end-to-end verification — so do not skip
   the matrix.
2. Only after the matrix is green, flip hyalo's own `.github/workflows/ci.yml`
   `lint-kb` job from build-from-source to `uses: ractive/setup-hyalo@v1`.

The install logic in `action.yml` was validated end-to-end on macOS bash 3.2
(latest + pinned + warm-cache paths, and input-format rejection).
