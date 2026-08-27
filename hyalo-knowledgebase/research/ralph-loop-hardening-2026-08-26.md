---
title: "Ralph loop hardening: autonomous run + cmux/GLM workarounds"
type: research
date: 2026-08-26
status: done
tags: [research, ralph-loop, cmux, glm, pi, tooling]
related:
  - "[[research/agent-ergonomics-ralph-loop-port-2026-08-24]]"
  - "[[iterations/iteration-206-links-perf-profiling]]"
  - "[[iterations/iteration-225-arch-thin-dispatch-typed-hints-core-facade]]"
  - "[[iterations/iteration-226-arch-lint-crate-index-journal]]"
---

# Ralph loop hardening: autonomous run + cmux/GLM workarounds

2026-08-26: ported the ralph loop from manual phase-by-phase orchestration
to a fully autonomous run-loop, and fixed the bugs that surfaced in the
process. Result: 9 iterations merged in one day (234–238 semi-manual,
206/225/226 fully autonomous — the last three with zero human
intervention).

## Architecture (scripts in `~/.pi/agent/skills/ralph-loop/scripts/`)

- **`run-loop.sh <start> <end>`** — chains iterations, stops on first
  failure or STOP file (`~/.cache/ralph-loop/<repo>/STOP`); integer
  ranges auto-chain. Model preflight: pong smoke-test per distinct model
  BEFORE spawning (a dead/renamed model otherwise fails instantly with
  zero agent output — wasted run).
- **`run-iteration.sh <id> <plan>`** — implement → review → merge-verify
  → cleanup (closes panes, deletes branch, `hyalo set status=completed`,
  state.json merge SHA). **Resume support**: if the implement sentinel is
  `0` and a PR exists, skips straight to review — proven on iter-206
  after a false abort.
- **`run-phase.sh`** — per-phase pane + sentinel + wait-for handshake
  (unchanged core), see below for fixes.
- **`json-events.sh`** — pi `--mode json` event stream renderer: live
  tool calls, thinking, prose, model errors. Slim; imports the GLM fix
  optionally.
- **`glm_reasoning_fix.py`** — see below.

## Bugs found and fixed

1. **cmux `list-pane-surfaces` is focused-pane-scoped.** `--workspace`/
   `--window` are only ref-resolution context; the listing unit is one
   pane (default: focused). The death detector used
   `list-pane-surfaces --workspace` assuming it enumerated the workspace,
   so every non-focused pane read as dead ~20s after the user clicked
   away → false "pane closed mid-phase" → loop aborted while the agent
   kept working. Caused both the iter-238 and iter-206 stalls.
   **Fix**: `cmux capture-pane --surface X` exit code (focus-independent,
   0 = alive), 3 consecutive misses required. Workspace-wide enumeration
   exists as `cmux list-panels` (`surface.list`); see manaflow-ai/cmux
   #5469, #3189. Documented in the cmux skill.
2. **Lost wakeups.** `cmux wait-for` signals fired between polls are lost
   (no latch). The poll loop now also wakes on the sentinel file — the
   sentinel is ground truth, the token is just a fast path.
3. **SIGPIPE + `set -o pipefail` in preflight.** `pi ... | grep -q`
   exits grep on first match → pi gets SIGPIPE (141) → pipeline "fails"
   although the model was fine. Fix: capture output with `$(...)`, match
   in bash, no pipe.
4. **pi `--mode json` assistant errors were invisible.** A
   `stopReason: "error"` message (e.g. OpenRouter 404 "stealth/ox-alpha
   retired, was GLM-5.3 Flash") rendered as nothing. The formatter now
   prints `✗ MODEL ERROR: <message>`.
5. **GLM reasoning newline corruption (pi#8584).** GLM-via-OpenRouter
   intermittently streams reasoning deltas newline-separated per token
   (`word\n next`, `Journal<'\n_>`), garbling thinking into one fragment
   per line. Char-level heuristics (as in pi's TUI fix) fail on
   code-heavy thinking (punctuation, CamelCase). **Fix**: statistical
   per-block detection — newline density > 0.5 per token ⇒ corrupted.
   **Update 2026-08-27**: a second variant appeared — tokens separated by
   `\n\n` (blank lines), which the density detector catches but the
   original normalizer then *preserved* as paragraph breaks, so the garble
   survived. Fix v2: within corrupted blocks, compute words-per-line; if
   ≤ 3 (real prose paragraphs average many more), *flatten* mode — all
   newline runs collapse to single spaces, and sentence-final punctuation
   (`.?!`) re-introduces paragraph breaks. The original single-newline
   pattern (words-per-line ~1) also routes to flatten; clean blocks still
   pass verbatim. Self-disabling when the provider stops corrupting.
   Removal: delete `glm_reasoning_fix.py` (json-events.sh falls back to
   verbatim) or `RALPH_GLM_FIX=0`.

## Operational notes

- pi print mode (`--mode json`) is the default; interactive mode is for
  mid-phase steering. Sentinels (`iter-N-phase-done`, content 0/1) are
  the machine-checkable truth; launcher prints banner + flips tab title
  + sends `cmux notify` on completion.
- Model pairing this run: `z-ai/glm-5.3` for both implement and review
  (it was the model behind the retired `stealth/ox-alpha` alias and
  performed well in both roles).
- Review panes open BELOW the kept-open implement pane (scrollback);
  both closed after merge verification.
- Cache dir per repo: `~/.cache/ralph-loop/<repo>/` (state.json,
  sentinels, launchers, prompts, phase logs; STOP file for pausing).

## Open items

- No planned iterations remain in the vault; next work needs new plans
  (candidates: follow-ups from 206 profiling results, post-225/226
  architecture observations, another agent-ergonomics dogfooding pass).
- The ralph-loop skill is user-level (`~/.pi/agent/skills/ralph-loop/`),
  not versioned with hyalo — consider mirroring it into the repo or a
  dotfiles repo.
