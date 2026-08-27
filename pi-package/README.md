# hyalo pi package

This directory is the pi integration package for hyalo. It is the **single
source of truth** for the pi extension and skills — the hyalo binary embeds
these same files (via `include_str!`) and writes them to `.pi/` with
`hyalo init --pi` (the vendored fallback).

## Install

```sh
pi install git:github.com/ractive/hyalo
```

From **hyalo ≥ 0.21** on, pin the release tag matching your binary instead
(recommended — the extension's expected output shapes track the binary, so
a matched tag avoids extension/binary drift):

```sh
pi install git:github.com/ractive/hyalo@v0.21.0
```

(The tag form needs ≥ v0.21.0; earlier release tags predate the root package
manifest that git installs require.) This registers the `hyalo` extension
(generic + typed tools: `hyalo_find`,
`hyalo_read`, `hyalo_set`, `hyalo_task`, and a post-write lint guardrail) plus
the `hyalo` and `hyalo-tidy` skills. A main-HEAD install updates with
`pi update --extensions`; a pinned-tag install moves only when you re-pin
with a new `pi install git:…@vX.Y.Z` (pi reconciles pinned refs but never
moves them).

If you don't want a git dependency, run `hyalo init --pi` inside your vault
instead — it writes a vendored copy of these same files. The downside: the
vendored copy only changes when you upgrade the hyalo binary and re-run the
command.

## Requirements

- A `hyalo` binary on `PATH` (Homebrew, apt, cargo install, …). The extension
  is a CLI wrapper by design; there is no bundled logic.

## Compatibility

The extension shells out to the installed `hyalo` binary and must stay
compatible with *released* hyalo versions:

- The typed tools (`hyalo_find` etc.) require **hyalo ≥ 0.21** (the next
  release after 0.20); the `[pi]` config section requires hyalo ≥ 0.20.
- On older binaries the typed tools report hyalo errors; the generic `hyalo`
  tool still works.
