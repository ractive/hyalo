# Staged `ractive/setup-hyalo` repo

This directory is the **complete, ready-to-push tree** for the standalone
`ractive/setup-hyalo` GitHub Action (see [[decision-log#DEC-051]] and
[[iterations/iteration-171-setup-hyalo-action]]).

It lives under `research/` (a lint-ignored path) because the automated iteration
run was **not permitted to create a new public GitHub repository** — that action
requires human authorization in the web UI.

## To publish

```sh
cd hyalo-knowledgebase/research/setup-hyalo-action
git init && git add -A && git commit -m "feat: setup-hyalo composite action"
gh repo create ractive/setup-hyalo --public --source=. --push
git tag v1.0.0 && git tag v1
git push origin v1.0.0 && git push origin v1
```

Then flip hyalo's own `.github/workflows/ci.yml` `lint-kb` job from
build-from-source to `uses: ractive/setup-hyalo@v1` and run the action's `smoke`
workflow to confirm the matrix is green on ubuntu/macos/windows.

The install logic in `action.yml` was validated end-to-end on macOS bash 3.2
(latest + pinned + warm-cache paths, and input-format rejection).
