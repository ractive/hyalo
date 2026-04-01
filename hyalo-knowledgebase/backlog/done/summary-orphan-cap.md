---
title: Cap orphan list in summary output
type: backlog
date: 2026-03-28
tags:
  - backlog
  - ux
priority: medium
status: wont-do
origin: dogfooding legalize-es
---

# Cap orphan list in summary output

## Problem

On repos with many files but no wikilinks (like legalize-es with 8,642 law files), the `summary` output is dominated by 8,640 orphan file paths. The hints and other useful information at the bottom are buried.

## Proposal

Cap the orphan list in summary text output to a configurable limit (default ~10), with a "(and N more)" note. Options:

1. `summary --max-orphans N` to control the cap
2. `summary --no-orphans` to suppress entirely
3. Default cap of 10 in text mode, full list in JSON mode

## Acceptance criteria

- [ ] Summary text mode shows at most 10 orphans by default with a "(and N more orphans)" line
- [ ] `--max-orphans 0` or `--no-orphans` suppresses orphan list entirely
- [ ] JSON mode still includes full orphan list (for programmatic use)
