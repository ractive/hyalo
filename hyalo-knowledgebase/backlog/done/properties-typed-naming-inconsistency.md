---
title: properties-typed flag uses hyphen but JSON key uses underscore
type: backlog
date: 2026-03-25
origin: dogfooding v0.3.1
priority: low
status: completed
tags:
  - backlog
  - ux
  - consistency
  - output
---

# properties-typed flag uses hyphen but JSON key uses underscore

## Problem

`--fields properties-typed` produces a JSON key `properties_typed` (underscore). This breaks the natural `--jq '.[0]["properties-typed"]'` query. Users must know to use `.[0].properties_typed` instead.

The flag and JSON key should use the same convention.

## Proposal

Either:
- Change the JSON key to `properties-typed` (matches the flag), or
- Change the flag to `properties_typed` (matches the JSON key)

Given that JSON keys with hyphens require bracket notation, the underscore convention in JSON is more ergonomic. The flag should probably stay as-is (CLI convention is hyphens) but the discrepancy should be documented.

## Acceptance criteria

- [x] Flag name and JSON key are consistent, or the discrepancy is documented in help text
