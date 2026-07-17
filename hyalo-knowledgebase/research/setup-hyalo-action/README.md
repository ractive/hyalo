# setup-hyalo

A composite GitHub Action that installs the prebuilt [`hyalo`](https://github.com/ractive/hyalo)
CLI on a runner in seconds and puts it on `PATH`. Use it to add a `hyalo lint`
PR check to any repository — or to give a [`claude-code-action`](https://github.com/anthropics/claude-code-action)
agent the `hyalo` binary so it can query and fix your markdown knowledgebase.

No compilation, no Node/Python — the action is pure `bash` + `curl` and downloads
a release archive that already matches the runner platform.

## Usage

```yaml
name: Lint knowledgebase
on:
  pull_request:
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: ractive/setup-hyalo@v1
      - run: hyalo lint --strict --format github
```

`--format github` renders each violation as an inline annotation on the PR diff.
The lint exits non-zero on errors, so the check fails when your vault has schema
or markdown violations.

## Inputs

| Input          | Default                | Description                                                                 |
| -------------- | ---------------------- | --------------------------------------------------------------------------- |
| `version`      | `latest`               | Release to install. `latest` tracks the newest release; pin a tag like `v0.17.0`. |
| `github-token` | `${{ github.token }}`  | Token for the release-asset API (raises the anonymous rate limit).          |

## Outputs

| Output    | Description                                                     |
| --------- | ------------------------------------------------------------- |
| `version` | The resolved version tag that was installed (e.g. `v0.17.0`). |

```yaml
      - uses: ractive/setup-hyalo@v1
        id: hyalo
        with:
          version: v0.17.0
      - run: echo "installed ${{ steps.hyalo.outputs.version }}"
```

## Supported runners

| OS      | Architecture      | Release target                | Notes                                    |
| ------- | ----------------- | ----------------------------- | ---------------------------------------- |
| Linux   | x86_64 / aarch64  | `*-unknown-linux-gnu`         | `ubuntu-latest`, arm runners             |
| macOS   | aarch64 (arm64)   | `aarch64-apple-darwin`        | `macos-14`+; x86_64 macOS is unsupported |
| Windows | x86_64 / aarch64  | `*-pc-windows-msvc` (`.zip`)  | steps run in `bash` (preinstalled)       |

hyalo does not publish an x86_64 macOS binary. On an Intel macOS runner the
action fails with a clear message — use an arm64 runner (`macos-14` or newer) or
`cargo install hyalo-cli` instead.

## Caching

The extracted binary is cached under `RUNNER_TOOL_CACHE`, keyed by version +
platform. Repeat runs on the same version skip the download.

## Pinning

For a floating major tag, `@v1` gives you the latest compatible action. To pin
exactly, reference a full commit SHA:

```yaml
      - uses: ractive/setup-hyalo@<full-sha>  # v1.0.0
```

Pinning the **action** (`@v1` / SHA) is independent of pinning the **binary**
(`version:` input). Pin both for fully reproducible CI.

## Versioning & release protocol

- The action is versioned independently of the hyalo binary (the
  `dtolnay/rust-toolchain` pattern), so the binary can release without retagging
  the action.
- Releases carry a full `vMAJOR.MINOR.PATCH` tag **and** a moving `vMAJOR` tag.
- To cut a release: tag `vX.Y.Z`, then move the floating major tag to it:

  ```sh
  git tag v1.0.0
  git tag -f v1
  git push origin v1.0.0
  git push -f origin v1
  ```

- Only retag the floating major tag for backwards-compatible changes; a breaking
  change bumps the major tag (`v2`).
- **hyalo release smoke test (manual):** when hyalo cuts a new release, run this
  action's `smoke` workflow (`workflow_dispatch`) with `version:` set to the new
  tag to confirm the new archive installs on all three OSes before announcing it.
  Automating this into the hyalo release pipeline is deferred — see the
  `ractive/release-workflows` change protocol.

## License

MIT — see [LICENSE](LICENSE).
