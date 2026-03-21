---
title: "--help-all flag for comprehensive LLM-friendly CLI reference"
type: backlog
date: 2026-03-21
status: idea
priority: medium
origin: dogfooding iteration-07
tags:
  - backlog
  - cli
  - llm
  - discoverability
---

# --help-all flag for comprehensive LLM-friendly CLI reference

## Problem

The standard `--help` flag shows bare flag and subcommand listings — enough for a human who already knows the tool, but insufficient for an LLM (or a new user) that needs to understand the full CLI surface to generate correct commands. Today an LLM agent has to call `--help` on every subcommand individually, parse each output, and still guess at common workflows and output formats. This wastes tokens and invites mistakes.

## Proposal

Add a `--help-all` flag to the root `hyalo` command that outputs a single, comprehensive reference covering:

- **All commands and subcommands with their flags** — the full tree, not just top-level.
- **Practical usage examples** showing common workflows, e.g.:
  ```sh
  # Find all files tagged 'rust'
  hyalo tag find rust

  # List properties of a specific file
  hyalo property list iterations/iteration-06-outline.md
  ```
- **Output format examples** — what JSON, text, and TSV output actually looks like for representative commands, so an LLM can parse the structure without trial and error.
- **Composability examples** — piping with `--jq`, using `--format tsv` with unix tools like `sort`, `cut`, `awk`.

```sh
hyalo --help-all
hyalo --help-all --format text   # plain text (default)
hyalo --help-all --format json   # machine-readable structured reference
```

## Notes

This is useful for both humans wanting a quick single-page reference and LLMs that need to understand the full CLI surface to generate correct commands. The output should be stable enough to include in system prompts or pipe into an LLM context window. Consider auto-generating it from clap metadata plus hand-written example annotations.
