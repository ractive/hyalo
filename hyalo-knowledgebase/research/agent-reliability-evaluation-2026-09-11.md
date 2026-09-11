---
title: Agent reliability evaluation for iteration 295
type: research
date: 2026-09-11
status: active
tags:
  - evaluation
  - agents
  - hyalo
  - verification
related:
  - "[[iterations/iteration-295-resource-safety-and-final-verification]]"
  - "[[research/rust-architecture-review-2026-09-10]]"
---

# Agent reliability evaluation for iteration 295

This focused comparison used Pi 0.84.4 through OpenRouter with
`openai/gpt-5.6-sol`, high thinking, an ephemeral session and only the Bash
tool. Skills, extensions and context files were disabled. It is local
evaluation evidence, not a broad benchmark or a statistically significant
product claim.

## Method

Two authored Markdown tasks were each run with Hyalo-only instructions and
ordinary POSIX file-tool instructions. Each paired condition began from a
byte-identical disposable tree. One task changed a frontmatter status only for
notes tagged `release`, while preserving a malformed note. The other renamed a
note and updated resolving wikilinks while preserving aliases, prose and
unrelated links.

Prompts, run order, model settings, fixtures and hand-authored expected trees
are fixed in `295-heldout-comparison.cjs`. Grading reads filesystem bytes
without considering the method or the model's completion note. Raw Pi JSON
streams and disposable trees are retained in the iteration run directory. The
pre-bulk set is immutable under `295-heldout-prebulk-archive/`; the repaired
Hyalo rename arm is under `295-heldout-final-raw/`.

## Results

| Run | Task and method | Exact grading | Calls | Wall time | Token usage |
|---|---|---:|---:|---:|---:|
| 01 | status, Hyalo | 6/6 files; 2/2 changes | 3 | 26.275 s | 35,111 |
| 02 | status, ordinary tools | 6/6 files; 2/2 changes | 3 | 34.420 s | 10,311 |
| 03 | rename, Hyalo, repaired binary | 5/5 files; 3/3 changes | 5 | 32.199 s | 55,671 |
| 04 | rename, ordinary tools | 5/5 files; 3/3 changes | 3 | 24.325 s | 6,989 |

All four comparison arms exited successfully. Every resulting tree matched its
expected bytes, with zero unintended paths, zero failed Bash calls and zero
repeated identical Bash calls. Pi does not expose a separate provider retry
count, so that field is recorded as `null`. The final affected arm observed
OpenRouter, `openai/gpt-5.6-sol` and the `openai-completions` API in every
assistant completion event.

The repaired Hyalo rename arm is bound to release SHA-256
`7ffea8862d952dcde2de40b98a58c7a51c983ac95c02e4e7987df56bce443afa`
and its exact changed-source identity in
`295-heldout-final-affected-results.json`. The earlier three reusable controls
did not capture a binary or exact dirty-source hash, so those fields remain
`null`; their raw prompts, output and event metadata are retained.

The corrected per-run usage fields include input, output, cache read, cache
write, reasoning and summed per-turn total tokens. The four comparison arms
above total 108,082 tokens. All five actual calls, including the superseded
pre-bulk Hyalo rename arm, consumed 153,934 measured tokens; its 45,852 tokens
remain usage evidence but are excluded from the final four-arm table.

A preliminary parser counted non-assistant event payloads and produced inflated
call and token values. That output remains in the original comparison log as
failed measurement evidence. Regrading preserved raw events made no model call;
`295-heldout-grade-operational-final` supersedes those measurements. Missing Pi
usage fields are `null`, rather than inferred as zero.

## Limits

The fixed order does not control provider load or filesystem cache. The same
model saw method-specific tool instructions, so tool choice could not be
blinded. Two small tasks are enough to test the exact-edit rubric, but too few
for statistical inference. Tool-call counts use assistant `message_end` and
tool execution events exposed by Pi; time is end-to-end local wall time. No
credential text is retained in the evidence.
