---
title: "Enriched --help via clap's two-tier help system"
type: backlog
date: 2026-03-21
status: completed
priority: medium
origin: dogfooding iteration-07
tags:
  - backlog
  - cli
  - llm
  - discoverability
---

# Enriched --help via clap's two-tier help system

## Problem

The standard `-h` output shows bare flag and subcommand listings — enough for a human who already knows the tool, but insufficient for an LLM (or a new user) that needs to understand the full CLI surface to generate correct commands. Today an LLM agent has to call `-h` on every subcommand individually, parse each output, and still guess at common workflows and output formats. This wastes tokens and invites mistakes.

## Proposal

Instead of adding a custom `--help-all` flag, leverage clap's built-in two-tier help system where `-h` and `--help` produce different levels of detail:

- **`-h`** stays compact (current behavior) — quick reminder of flags and subcommands.
- **`--help`** expands to a comprehensive single-page reference using clap's `long_about` and `after_long_help` attributes, covering:

  1. **One-liner purpose** — what the command (or subcommand) does in a single sentence.
  2. **Mental model / key concepts** — frontmatter, properties, tags, vaults, and how they relate.
  3. **Complete command tree with flags** — the full hierarchy, not just the top level.
  4. **Cookbook / recipes** — goal-oriented examples mapping intent to command, e.g.:
     ```sh
     # Find all files tagged 'rust'
     hyalo tag find rust

     # List properties of a specific file
     hyalo property list iterations/iteration-06-outline.md

     # Pipe JSON output through jq
     hyalo tag find rust --format json --jq '.[].path'
     ```
  5. **Output shape examples per command family** — what JSON, text, and TSV output actually looks like for representative commands, so a consumer can parse the structure without trial and error.

This approach requires no new flags or subcommands — it uses clap's existing `long_about`, `after_long_help`, and `long_help` to enrich every level of the command tree.

## Notes

The markdown-like structured text that `--help` produces is easy to parse for both humans and LLMs, making it useful as a single-page reference or as context piped into an LLM system prompt. The content should be stable enough to snapshot and include verbatim. It can be authored directly in Rust source via clap derive attributes and kept up to date alongside the code.
