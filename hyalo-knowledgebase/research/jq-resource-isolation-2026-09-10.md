---
type: research
title: Jq resource isolation and dependency evidence
date: 2026-09-10
status: active
---

# Jq resource isolation and dependency evidence

Stage 1 candidate for [[iterations/iteration-295-resource-safety-and-final-verification]],
based on landed iteration 294 at `ca5f52037b6ab95892ba12baf6f9b98205a8f38c`.
This records focused macOS evidence, not integrated review, native CI or completion.

## Process and protocol ownership

`output/jq/worker.rs` extends the existing compile-only preflight and evaluation seams.
Each request uses one child and one frame: `HJQ1`, a message-kind byte, a four-byte
big-endian payload length, the payload and EOF. Compile requests contain source and
must return an empty ready frame before command dispatch. Evaluation requests contain
source plus the final JSON envelope; only output or bounded diagnostic text returns.
Vault paths, mutation authority and effect/index reports stay in the parent.

The parent bounds source to 64 KiB, serialized input to 64 MiB, output to 10 MiB and
worker diagnostics to 4096 bytes. Evaluation also limits output to one million values.
Serialization refuses overflow while writing; readers validate frame version, kind and
length before allocating payload storage. Per-phase wall time is three seconds.
The child guard terminates and waits on every controlled exit, and pipe threads join
after child cleanup. There are no temporary protocol files or detached jq threads on
the CLI user-input path. The public legacy Rust helper retains its historical thread
timeout and native-stack risk; it is documented for trusted filters, outside this CLI
isolation contract.

## Platform and cancellation limits

| Platform | Implemented limit and lifecycle | Evidence at stage 1 |
|---|---|---|
| macOS | Parent deadlines, byte limits, kill/wait; CPU limit of three seconds; core dumps disabled. No hard memory cap. | Native focused tests passed. |
| Linux | Same Unix controls plus 512 MiB `RLIMIT_AS` address space. | Native iteration-295 CI pending. |
| Windows | Parent deadlines, byte limits and kill/wait; job with kill-on-close assigned before source is sent. No hard memory cap. | Native iteration-295 CI pending; cross-check is compilation only. |

Unix SIGINT and SIGTERM/SIGHUP cancellation flags let the effect-owning parent finish
worker cleanup and emit a structured output error. SIGTERM covers the shipped npm
transport's `child.kill()` timeout/AbortSignal path. Actual SIGINT/SIGTERM after append
are tested. Windows force termination, including Node cancellation, cannot preserve a
dead parent's diagnostic; the job closes its worker, with a native regression awaiting
Windows CI. Unix SIGKILL/parent crashes are not graceful cancellation and have no
general orphan-cleanup or effect-report guarantee. A cancellation between preflight and
evaluation does not roll back committed writes or interrupt every mutation operation.

Child isolation does not guarantee host memory safety: especially on macOS/Windows,
large intermediates can exhaust host memory before the deadline. Linux's address-space
limit is not a bound on all host resources. OS resource failure can precede the wall
deadline; either returns an error. The 291 retry observation informs this ordering,
without treating it as a new finding or weakening successful-output assertions.

## Focused behavioral evidence

The disposable jq regression runs `def f: [f]; f`, finite runtime failure, an infinite
non-yielding filter, and oversized output after indexed append. Each CLI parent exits 2
with structured `output_failure`, committed path effects and updated index metadata;
subsequent indexed queries see the append. Existing foundation tests verify malformed
preflight leaves files unchanged. Actual cancellation after append retains effects.
Protocol tests reject version/kind mismatch, oversized declarations, truncation and
trailing bytes; cleanup tests exercise cancellation, deadline and malformed responses,
and Unix `waitpid` confirms the disposable child has already been reaped.

Recorded commands under `.git/ralph-loop/run-20260909T224558Z-290-295/` include
`stage1-worker-unit` (4 passes), `stage1-jq-e2e` (22 passes),
`stage1-foundation-preflight` (17 passes), and `stage1-lint-dependency` (254 passes).
The schema report and source manifest bind final paths and exact argument/log records.
Native platform runs, generated assets, final ordered workspace gates, integrated review
and the held-out model comparison remain with stage 2 and the supervisor.

## Dependency ownership and refreshed audits

On 2026-09-10, `bincode 1.3.3` was enabled by `syntect 5.3.0`, through `comrak 0.21.0`.
`yaml-rust 0.4.5` was the syntect YAML-loading dependency on its wasm feature path;
it existed in the lockfile but not the macOS active dependency tree. Hyalo uses Comrak
AST types; its direct default features unnecessarily enabled CLI/syntax highlighting.
Both mdbook lint dependencies already disabled Comrak defaults. Disabling the same
unused defaults in `hyalo-mdlint` removes syntect, bincode and yaml-rust from the entire
lockfile. The CLI now explicitly enables Clap's `string` feature that it previously
received through the unused Comrak CLI feature. Parser APIs and versions are unchanged.

`cargo audit --json` reports 213 dependencies, zero vulnerabilities, zero warnings and
an empty ignore list, against advisory commit
`b50980aad8b8f14f77e25a97b32dd94bf008b0af`. `npm --prefix npm/hyalo audit --json`
reports zero vulnerabilities across its 156-dependency lockfile inventory. The Pi package
has no separate dependency lockfile; this is not an audit of globally installed Pi or
external consumers. No suppression or pending maintenance exception was added.

The original notices concern maintenance, not proof of exploitable vulnerabilities:
[bincode notice](https://rustsec.org/advisories/RUSTSEC-2025-0141.html) and
[yaml-rust notice](https://rustsec.org/advisories/RUSTSEC-2024-0320.html).
Repository maintainers own future dependency audits; revisit if syntax-highlighting
features or a dependency upgrade reintroduces either package. Iteration 287 external
consumer work remains deferred.
