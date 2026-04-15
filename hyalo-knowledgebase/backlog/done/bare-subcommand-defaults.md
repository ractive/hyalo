---
title: Bare tags/properties should default to summary subcommand
type: backlog
date: 2026-03-26
origin: dogfooding v0.4.1
priority: low
status: completed
tags:
  - cli
  - ux
---

`hyalo tags` and `hyalo properties` exit with code 2 (usage error) instead of defaulting to the `summary` subcommand. This is a friction point — `hyalo tags` is the intuitive first thing to try.

Fix: use clap's `#[command(subcommand_required = false)]` or set `default_subcommand` so that bare `tags` / `properties` runs `summary`. Alternatively, use clap's `#[command(args_conflicts_with_subcommands = false)]` pattern.
