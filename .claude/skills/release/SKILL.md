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
- Every `hyalo` invocation below means `target/release/hyalo` (build it
  first if stale). The bare `hyalo` on PATH is the previously *installed*
  release — it predates the version being cut and will reject new config
  keys/flags (v0.18.0 example: it choked on `[changelog]` in `.hyalo.toml`
  and didn't know `lint --profile`).

## 1. Preconditions

Decide the version from the `[Unreleased]` contents (pre-1.0 convention
here: minor bump for features/breaking, patch for fix-only), then run the
read-only preflight — it checks branch/clean/sync, the 3-spot version
match, that the tag doesn't exist, the changelog state (pre- vs
post-rotation), changelog-profile lint, and gh auth:

```bash
.claude/skills/release/scripts/release-preflight.sh check X.Y.Z
```

Fix every FAIL before proceeding. Additionally verify the latest merges are
green (`gh run list --branch main --limit 5` / `gh pr checks <last-PR>`) —
the script cannot judge CI health.

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
target/release/hyalo lint --dir . CHANGELOG.md --profile changelog   # root file is outside the vault → needs --dir .
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
.claude/skills/release/scripts/release-preflight.sh notes X.Y.Z > /tmp/relnotes.md
gh release create vX.Y.Z --notes-file /tmp/relnotes.md --generate-notes
```

The `notes` mode fails loudly (exit 1) if the `[X.Y.Z]` section is missing
or empty — never publish with an empty notes file. (If `hyalo changelog
show <version>` exists by now, prefer it.)

## 6. Watch the pipeline and verify

- `gh run list --workflow release.yml --limit 1` then `gh run watch <id>` —
  all jobs green (multi-OS builds, musl statics, deb/rpm → Cloudsmith,
  Homebrew, winget, SBOMs, provenance, SHA256SUMS).
- `gh release view vX.Y.Z` — assets present, notes render with the curated
  section on top.
- **Publish crates — manual dispatch required.** `publish-crates.yml` is
  `workflow_dispatch`-only; nothing triggers it automatically. Once the
  release pipeline is green:
  `gh workflow run publish-crates.yml -f ref=vX.Y.Z`, find the run id with
  `gh run list --workflow publish-crates.yml --limit 1`, then
  `gh run watch <id> --exit-status`. If it fails half-published, it has a
  manual escape hatch — see that workflow. The release is not done until
  this run is green.
- Pipeline internals live in `ractive/release-workflows`; failures inside
  reusable jobs are usually fixed there, not here.

## 7. Post-release follow-ups

- Downstream consumers pinned to "next release" features (e.g. the
  `ractive/setup-hyalo` action's smoke fixtures) can now be unblocked —
  check for release-blocked TODOs before closing out.
- Do NOT bump the workspace version for the next cycle automatically; that
  happens when the next cycle actually starts.
