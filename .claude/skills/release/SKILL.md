---
name: release
description: Cut a hyalo release end-to-end — verify/bump the workspace version, rotate the changelog with hyalo itself, sync the winget fork, publish the GitHub release with curated + auto-generated notes, and watch the pipeline. Use this skill whenever the user wants to release a new hyalo version, cut/tag/publish vX.Y.Z, says /release, or says things like "ship 0.19.0", "cut the release", or "publish the release".
---

# Release hyalo

Cuts a release `vX.Y.Z` from `main`. The GitHub Release *publish* event is
what triggers the build/package pipeline (`.github/workflows/release.yml`,
shared via `ractive/release-workflows`) — so the release is created last,
after everything on main is final.

**Hard rules**

- NEVER create git tags manually. `gh release create` makes the tag.
- CI enforces tag == workspace `Cargo.toml` version; mismatch fails the
  pipeline after the tag exists — check the version BEFORE releasing.
- Everything lands on `main` before the release is created. The mechanical
  release commits (version bump, changelog rotation) go directly on main —
  a release is a synchronous maintainer act; don't spin up a PR for them.

## 1. Preconditions

- On `main`, clean tree, synced: `git switch main && git pull --ff-only`.
- Latest merges are green: `gh run list --branch main --limit 5` and/or
  `gh pr checks <last-PR>` — every quality gate passed.
- `CHANGELOG.md` has a non-empty `## [Unreleased]` section (that's what
  ships). If it's empty, stop — nothing to release.
- Decide the version from the `[Unreleased]` contents (pre-1.0 convention
  here: minor bump for features/breaking, patch for fix-only).

## 2. Verify or bump the workspace version

Check the root `Cargo.toml`. The version appears in **three places** that
must all match the target:

- `[workspace.package] version = "X.Y.Z"`
- `hyalo-core = { path = ..., version = "X.Y.Z" }`
- `hyalo-mdlint = { path = ..., version = "X.Y.Z" }`

Often the bump already happened during the dev cycle (e.g. 0.18.0 was bumped
in the changelog-conversion PR long before release day) — then just verify.
If a bump is needed: edit the three fields, run `cargo build --release`
(refreshes `Cargo.lock` — commit it too), run the full gates
(`cargo fmt` → `cargo clippy --workspace --all-targets -- -D warnings` →
`cargo test --workspace -q`), commit on main:
`chore(release): bump version to X.Y.Z`.

## 3. Rotate the changelog (dogfood it)

```bash
target/release/hyalo changelog release X.Y.Z --apply
hyalo lint --dir . CHANGELOG.md --profile changelog   # root file is outside the vault → needs --dir .
```

This moves `[Unreleased]` into a dated `## [X.Y.Z]` section and rewrites the
footer compare-links. Lint must report 0 errors. Commit + push on main:
`docs(changelog): release X.Y.Z`.

## 4. Sync the winget fork

```bash
gh repo sync ractive/winget-pkgs
```

A stale fork makes the pipeline's winget submission fail with a misleading
`CreateRef` error — sync every time, before the release exists.

## 5. Create the release (curated notes + auto-notes)

Extract the freshly rotated section as the human-curated top of the release
notes, then create the release. GitHub prepends `--notes-file` to the
`--generate-notes` PR list (grouping per `.github/release.yml` labels;
unlabeled PRs land under "Other Changes").

```bash
sed -n '/^## \[X.Y.Z\]/,/^## \[/p' CHANGELOG.md | sed '$d' > /tmp/relnotes.md
test -s /tmp/relnotes.md   # MUST be non-empty — abort if the sed matched nothing
gh release create vX.Y.Z --notes-file /tmp/relnotes.md --generate-notes
```

(If `hyalo changelog show <version>` exists by now, use it instead of sed.)

## 6. Watch the pipeline and verify

- `gh run list --workflow release.yml --limit 1` then `gh run watch <id>` —
  all jobs green (multi-OS builds, musl statics, deb/rpm → Cloudsmith,
  Homebrew, winget, SBOMs, provenance, SHA256SUMS).
- `gh release view vX.Y.Z` — assets present, notes render with the curated
  section on top.
- Crates publishing runs separately (`publish-crates.yml`); if it fails
  half-published, it has a manual escape hatch — see that workflow.
- Pipeline internals live in `ractive/release-workflows`; failures inside
  reusable jobs are usually fixed there, not here.

## 7. Post-release follow-ups

- Downstream consumers pinned to "next release" features (e.g. the
  `ractive/setup-hyalo` action's smoke fixtures) can now be unblocked —
  check for release-blocked TODOs before closing out.
- Do NOT bump the workspace version for the next cycle automatically; that
  happens when the next cycle actually starts.
