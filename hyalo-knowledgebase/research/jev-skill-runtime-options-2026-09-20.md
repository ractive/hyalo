---
type: research
title: "Jev-assisted tidy: skill design and TypeScript versus HTTP"
date: 2026-09-20
status: completed
---

# Jev-assisted tidy: skill design and runtime choice

Recommendation: **extend `hyalo-tidy` with an explicitly selected Jev mode**, using
TypeScript source compiled to a self-contained `scripts/jev.mjs`. Bundle the pinned
TypeSafe SDK. Prefer **Bun** for development and execution, and keep the generated
JavaScript compatible with Node. Require neither Python nor a package install
during execution. Keep the helper advisory and use existing Hyalo commands for
all document changes.

This follows [[research/jev-opt-in-classification-2026-09-20]] and narrows its pilot
to a concrete delivery approach. Implementation is planned in
[[iterations/iteration-299-jev-helper]] and
[[iterations/iteration-300-jev-tidy-integration]]. The user specifically asked to
evaluate Bun and to bootstrap it using the official quickstart. That bootstrap
was completed in an isolated prototype; details below supersede the earlier
Node-only recommendation.

The investigation and scratch probes are complete. The production helper and skill
integration are not implemented by these planning documents.

## Extend tidy rather than introduce a second workflow

Filing, missing metadata, and controlled tags are already tidy responsibilities.
A separate skill would duplicate discovery, repair authorization, and reporting.
Keep one entrypoint, with a short optional section linking to `references/jev.md`.
The normal tidy path should not load that reference, inspect the key, invoke Node,
install software, or send network requests.

An explicit request such as “Use Jev to classify the inbox during this tidy” enables
the optional branch for that scope. A plain `/hyalo-tidy`, an available API key,
or provider settings in a cloned repository do not. Existing authorization should
carry through the session without asking before each request. A runtime flag is
an execution guard; the skill still establishes the user's authorization.

The reference and helper can later support a standalone `hyalo-classify` skill if
users want classification independently. Do not ship that second skill initially.

## Runtime comparison

| Approach | Runtime requirements | Advantages | Work still required | Verdict |
| --- | --- | --- | --- | --- |
| Bash + curl + jq | Compatible shell, curl, jq | Easy HTTP calls; no Node | Complete JSON validation, bounded retry/deadline logic, secret handling, portability, result normalization | Viable for a POSIX-only tool; retain a diagnostic recipe, not a second production backend |
| TypeScript + native fetch | Node, precompiled JS | No third-party runtime code; one endpoint is straightforward | HTTP error mapping, retry handling, cancellation, runtime validation, response limits | Credible fallback if SDK maintenance becomes inconvenient |
| TypeScript + bundled TypeSafe SDK | Bun or Node, bundled JS | Official request types, typed errors, cancellation and retries; small package | Runtime response validation and Hyalo policy; explicit transport configuration | Recommended; Bun preferred, Node compatible |
| Rust HTTP helper | A distributed executable | Fits native CLI deployment without Node | Another binary or premature native command; TLS/build/release integration | Reconsider if the optional Node requirement is unacceptable |

All approaches use the same HTTP API. The choice affects maintenance and packaging,
not model quality or billed token counts. A shell implementation can be thorough,
but its HTTP line is only a fraction of the actual classifier helper.

The compatibility target is Bun 1.4.2 and Node 22.14 or later; Bun 1.4.2 and Node
24.19 were exercised here, while minimum-version/platform checks remain delivery
work. Compile TypeScript at development time; distribute `.mjs` with all package
code included. Native Hyalo users without either runtime retain ordinary tidy
and a clear optional-mode unavailable result. Do not install a runtime automatically.

### Bun bootstrap and distribution decision

Following the user's [Bun quickstart](https://bun.sh/docs/quickstart), ran
`bun init --yes` in `/tmp/hyalo-jev-skill-plan-20260920/bun-bootstrap`, verified
`bun run index.ts`, and added `@typesafe-ai/sdk@0.6.0` with `bun add --exact`.
The generated project resolved Bun types 1.4.2 and TypeScript 7.0.2. A typed SDK
fixture then passed `bun test` (six assertions), `tsc --noEmit`, and a Bun build.
The sample server and figlet tutorial steps are unrelated to this CLI helper and
were not added. The generated prototype guidance stays scoped to that project.

Bun ran the same SDK mock suite successfully, including retry cancellation, and
both the SDK and direct fetch made successful live calls. SDK source explicitly
recognizes Bun in its runtime metadata. A Bun-generated portable bundle ran under
both Bun and Node from outside its source directory without node_modules.
These are evidence for the chosen APIs, not proof of full Node compatibility.
Sources: [Bun TypeScript](https://bun.com/docs/runtime/typescript),
[Bun compatibility](https://bun.com/docs/runtime/nodejs-compat).

Use Bun for this helper's isolated development package, with a committed Bun
lockfile, Bun tests, and `bun build --target=node --format=esm`. Keep the transport
and shared code on standard web APIs / compatible Node builtins; the build/test
harness can use Bun-specific APIs. Type-check separately: Bun's transpiler/bundler
does not replace `tsc`. See the [Bun bundler](https://bun.com/docs/bundler).

Use a package script `"typecheck": "tsc --noEmit"` and run `bun run typecheck`.
The earlier direct `bun run node_modules/typescript/bin/tsc --noEmit` probe was
valid — the installed entrypoint is JavaScript — but couples the command to an
internal package path. The package-script form is the maintained interface.
Ordinary package execution can honor tsc's Node shebang; `bun run --bun typecheck`
explicitly uses Bun if a Bun-only development environment is desired. Both forms
passed in the bootstrap. The compiler performs checking; Bun's TS execution alone
does not. Runtime response validation remains a separate application concern.

Prefer a Bun invocation when available:

```sh
bun --no-install --no-env-file /path/to/hyalo-tidy/scripts/jev.mjs check request.json
# Supported fallback for the same artifact:
node /path/to/hyalo-tidy/scripts/jev.mjs check request.json
```

The explicit Bun flags disable package auto-install and implicit .env loading.
The bundled helper should have no bare external package imports to resolve. Runtime
selection occurs only after the user selects Jev assistance and preserves a stated
runtime preference; changing runtimes must not retry an already-sent request.

`bun build --compile` also worked, but the offline fixture's executable was
**62,243,442 bytes**, versus **22,690 bytes** for its portable JS bundle. These are
local fixture sizes, not final product sizes. A bundled runtime would introduce
platform-specific release artifacts for this small helper. Ship `.mjs` initially;
keep standalone executables as a future deployment option. See
[Bun executables](https://bun.com/docs/bundler/executables).

Cross-compilation is supported for macOS, Linux and Windows, with x64/arm64 targets
and glibc/musl choices on Linux. Thus a future binary distribution could remove
the end user's Bun/Node prerequisite without rewriting the helper. Only the local
macOS binary was executed in this research. Cross-building is not platform testing;
release adoption would need native smoke tests, appropriate signing, checksums and
an update/distribution path. The user's question about this option does not itself
add a binary release pipeline to the two planned iterations.

## What the SDK inspection found

As checked on 2026-09-20, npm publishes `@typesafe-ai/sdk` version **0.6.0**. It has
no runtime dependencies, uses the MIT license, and provides ESM, CommonJS, and
TypeScript declarations. The unpacked package is 209,203 bytes. A local esbuild
probe bundling the client plus Choice/Noul helpers produced approximately **24.4
KiB** of unminified JavaScript; this excludes our wrapper and the full license
banner. Source: [SDK documentation](https://docs.typesafe.ai/sdk/javascript) and
[versioned package metadata](https://github.com/typesafe-ai/typesafe-sdk-js/blob/v0.6.0/package.json).

Important differences from a complete Hyalo integration:

- The client parses successful JSON and casts it to its TypeScript result type.
  It does not verify model identity, question IDs, selected options, probability
  ranges, or usage at runtime. An offline probe confirmed it accepts a malformed
  200 response containing an unknown option and confidence 2.
- Defaults can come from `TYPESAFE_BASE_URL`, `TYPESAFE_DEFAULT_MODEL`, and
  `TYPESAFE_LOG_LEVEL`. Set all three corresponding client options explicitly;
  only the API key should come from the user's environment for v1.
- Debug logging includes request/response bodies. Set `logLevel: "off"` in the
  wrapper and emit only our own sanitized status and usage metadata.
- The timeout is per attempt. Use a single overall abort signal for the request
  and its retry wait; an offline probe verified cancellation during backoff.
- The SDK buffers response delivery without a size ceiling. Supply a bounded
  fetch wrapper with `redirect: "error"`, exact endpoint validation, and a streaming
  response-byte cap before the SDK buffers it. A declared Content-Length alone
  is insufficient.
- Configure retry behavior explicitly. For this pilot use one retry for 429/529,
  no connection/timeout retry, and no retry for auth/validation errors. A long
  Retry-After must exhaust the overall budget and defer, not be shortened into
  an immediate retry. Do not wrap SDK retries in another retry loop.

These observations come from the [v0.6.0 client source](https://github.com/typesafe-ai/typesafe-sdk-js/blob/v0.6.0/src/client.ts),
[client configuration](https://docs.typesafe.ai/sdk/javascript/api/interfaces/TypeSafeClientConfig),
[request options](https://docs.typesafe.ai/sdk/javascript/api/interfaces/RequestOptions),
and [retry policy](https://docs.typesafe.ai/sdk/javascript/api/interfaces/RetryPolicy).

The SDK saves routine transport code and provides useful types while developing
known questions. Dynamic user-defined categories still require validation at
runtime. Keep our provider adapter narrow so replacing SDK calls with native
fetch does not change the skill, manifest, thresholds, or output contract.

## The transport experiments

Three live requests used the same short synthetic document, same three options,
and pinned `jev-1.13.0`. No repository documents were sent in this follow-up.

| Transport | Observed elapsed time | Input / output tokens | Answer |
| --- | ---: | ---: | --- |
| TypeSafe SDK 0.6.0 | 722 ms | 362 / 39 | docs, probability 1.00 |
| Node fetch | 649 ms | 362 / 39 | docs, probability 1.00 |
| Bash/curl | 675 ms | 362 / 39 | docs, probability 1.00 |

These are one-call feasibility checks with different timing boundaries, not a
performance comparison. Total live usage was 1,086 input and 117 output tokens,
approximately **$0.000045612** at the rate recorded in the earlier research.

Offline SDK probes also verified the configured 401/422 paths make one attempt,
429/529 paths can succeed on their second attempt, an overall 40 ms abort cancels
a one-second retry wait without a second call, and explicit endpoint/model/log
options override conflicting environment defaults. The SDK's default fetch
configuration did not set a redirect policy. These probes establish specific
behaviors of the pinned SDK; they are not finished product tests.

Two later Bun feasibility calls added 724 input and 78 output tokens. Combined
follow-up usage across Node, curl, and Bun was 1,810 input and 195 output tokens,
approximately $0.00007602. The Bun SDK call took 651 ms and its following fetch
call 258 ms; sequential warm connections and one observation per mode make this
unsuitable for attributing a performance advantage to the runtime or SDK.

The scratch artifacts are in `/tmp/hyalo-jev-skill-plan-20260920/`:
`sdk-probe/probe.mjs`, `probe-results.json`, `http-probe.sh`, `payload.json`, and
`curl-response.json`, `bun-probe-results.json`, and `bun-bootstrap/`. The API key was used only through the environment and
Authorization header; it was not stored in these artifacts or placed in curl's
argument vector. No Python was used in this follow-up investigation.

### Can the shell approach be done properly?

Yes. The successful probe used Bash's builtin `printf` to supply an Authorization
header to `curl --header @-`; the request body came from a JSON file through
`--data-binary @file`. It disabled curl configuration loading, restricted the URL
scheme, did not follow redirects, set connection and total timeouts, and checked
HTTP status separately from JSON. See the [curl manual](https://curl.se/docs/manpage.html).

For a shipped implementation, additionally require and test compatible curl/jq
versions, check unknown-length responses against a byte limit, validate all typed
answers with jq, normalize error exits, implement bounded Retry-After handling,
and ensure tracing never prints headers. Do not construct JSON by interpolating
document text into a shell string, put the key in `curl -H` arguments, or blindly
enable `--retry-all-errors` on POST requests.

Hyalo's embedded `--jq` is a filter over Hyalo output, not a general replacement
for standalone jq parsing an HTTP response. Windows users would additionally
need an agreed shell distribution such as Git Bash or WSL. Maintaining that shell
stack alongside a TypeScript stack gives us two clients to test and keep aligned.
Ship one production implementation.

## Proposed skill and helper contract

Initial scope: missing document types, approved tag additions, and suggestions
among existing destination folders. Preserve valid existing metadata. Keep
workflow statuses, automatic tag renames, new folders, arbitrary text generation,
deletion, and unattended application out of v1.

Use three explicit helper operations, all reading bounded JSON or explicit path
lists and writing only to stdout/stderr:

1. `prepare`: local-only selection and evidence preparation through a resolved
   Hyalo executable, using a user-scoped file list and an explicit policy. It
   never reads the API key or makes a network request.
2. `check`: validate a prepared manifest, report document/question/byte counts,
   and reject malformed or oversized inputs. No network and no key required.
3. `ask --allow-network`: send only the manifest's explicit API payload after
   validation, and return normalized suggestions/deferrals and usage. Missing
   `--allow-network` makes zero requests, even with a key present.

Example interface, proposed rather than currently available:

```sh
bun --no-install --no-env-file /path/to/hyalo-tidy/scripts/jev.mjs prepare \
  --hyalo /path/to/hyalo --files-from /tmp/tidy-files.txt \
  --policy /tmp/tidy-policy.json > /tmp/tidy-request.json

bun --no-install --no-env-file /path/to/hyalo-tidy/scripts/jev.mjs check /tmp/tidy-request.json
bun --no-install --no-env-file /path/to/hyalo-tidy/scripts/jev.mjs ask /tmp/tidy-request.json --allow-network
```

The caller owns any redirected output file and its cleanup; the helper creates
no cache, index, config, or implicit temporary file. In a strict no-write audit,
compose the operations with pipes/in-memory data and keep existing indexes
untouched. Support `-` as bounded stdin input. A prepare/check pass is the preview
mechanism; avoid giving a network request a misleading `--dry-run` label.

The policy is a small, explicit JSON object for the selected session: described
type/folder/tag candidates, permitted fields, and exclusions. It is not a new
`.hyalo.toml` section or persistent consent store. Resolve schema constraints
through Hyalo and reject policy options incompatible with them. Derive simple
type-to-folder mappings in code; use Jev only where a semantic choice remains.

`prepare` should accept only a fixed allowlist of read commands: config, schema
inspection, and file reads. Spawn the resolved executable with an argument array,
never a shell. Use bounded child stdout/stderr and a deadline. Do not trust the
default find limit when assembling a selected list: request an explicit limit and
record incompleteness rather than silently claiming an exhaustive vault audit.

Evidence should contain permitted frontmatter fields and sufficient body text,
with explicit truncation/completeness metadata. Begin with bounded full small
documents; defer oversized documents for agent-directed section selection rather
than silently using the first paragraph. Hash the canonical evidence actually
returned by Hyalo and the policy; call this an **evidence fingerprint**, not a raw
file hash or a filesystem concurrency guarantee. Preserve local source identities
and current values in a manifest for later review.

Keep local paths and diagnostic metadata separate from the payload. `ask` builds
the outgoing object from an explicit allowlist of `model`, `state`, and `questions`;
it must not serialize the full local manifest. Opaque IDs map answers back to files.
Question instructions must still identify the corresponding document inside state;
question IDs alone are not seen by the model.

Initial limits should be deliberately conservative: 25 selected documents per
prepare operation; 24,000 UTF-8 bytes and at most 48 questions per API request;
1 MiB of response bytes; a 12-second request/retry deadline; and small batches of
related evidence. Multiple bounded batches may cover the selected documents.
Do not claim bytes are tokens or that an estimated cost limit is an exact spend
cap. Report attempts and known usage; if delivery fails after processing, actual
charged usage may be unknown.

Return exit 0 for a structurally valid result, including ordinary deferrals; exit 1
for unavailable service/credentials and exit 2 for invalid input/protocol failure.
Always include a machine-readable status and counts. This intentionally avoids
treating expected model uncertainty as a shell failure. Tidy must report failed
batches and continue independent deterministic work; a standalone `ask` must never
turn an unavailable service into empty success.

Validate answer IDs/types against the request, model identity, known options,
finite probabilities in range, distribution sums, winning option, and nonnegative
integer usage. Use explicit unknown/no-match options and the research's existing
conservative thresholds. Schema validity and probability cannot establish semantic
truth. Keep incomplete, contradictory, and low-confidence results visible.

## Application remains an agent workflow

The helper has no `apply` operation and cannot launch mutations. The tidy agent
reviews suggested metadata and folders, rereads current evidence, and previews
`hyalo set --dry-run` / `hyalo mv --dry-run`. It applies only within the existing
repair request, preserving fields that already have valid values.

A changed evidence/policy fingerprint requires a fresh assessment. The pilot does
not promise atomic compare-and-swap between an earlier read and a later CLI call;
unattended durable application remains a separate future feature.

The preceding research demonstrated that `set --validate` checks assigned values
without necessarily detecting all missing required fields after a type change.
Before a type change, inspect its complete effective schema and required sections;
defer incomplete candidates. After any approved edit, run strict lint and recheck
the original findings. For a move, review the destination's schema binding and all
link-rewrite effects, including files outside the original classification list.

This is a deliberate boundary: descriptive suggestions can be cheap and useful
without pretending to provide a new fully validated transaction engine.

## Packaging and ownership work

Use an isolated private Bun source package under `npm/jev/`, bootstrapped from the
quickstart. It produces the shipped skill runtime, consistent with DEC-332's
exception for JavaScript deliverables; it is not a replacement for general Rust
repository tooling. Keep its Bun lockfile separate from the existing npm package.
Build `npm/jev/dist/jev.mjs` and synchronize that exact artifact to the distributed
skill assets. Do not import it from the ordinary npm API entrypoint, add an
automatic pi extension hook, migrate existing npm tooling to Bun, or add a Rust
HTTP dependency. Pin the SDK and embed its MIT copyright/license in the generated
helper. Cargo installs consume prebuilt embedded assets and do not require Bun.

Proposed installed layout:

```text
hyalo-tidy/
  SKILL.md
  references/jev.md
  scripts/jev.mjs
  agents/openai.yaml       # Codex only
```

Canonical optional guidance can live in
`plugins/hyalo/skills/hyalo-tidy/references/jev.md`. Generate/copy the helper and
reference consistently to pi and crate-local assets. Preserve each platform's
existing frontmatter and invocation metadata; the existing Codex skill is already
explicit-only.

Code inspection found these concrete integration gaps:

- Codex's package sync scans the full skill tree, but `init/codex.rs` currently
  embeds/installs/removes only `SKILL.md` and `agents/openai.yaml`. Extend its
  declared asset manifest, ownership checks, and directory cleanup for extras.
- pi's existing sync command/gate copy `skills/*/SKILL.md`, not recursive skill
  resources. Extend them for explicitly approved reference/script assets.
- pi installation ownership has an exact five-path artifact allowlist and content
  receipts. Add the two resources through that receipt mechanism, including update
  and removal behavior; simply copying extra files bypasses ownership.
- Claude embeds one tidy Markdown template. Add declared auxiliary assets to
  its preflight, installation, and removal paths; preserve conflicting user-owned
  files and guard symlinks. Do not expand this into an unrelated installer rewrite.
- Every asset referenced by an embedded `include_str!` must be inside the CLI
  crate so a Cargo source package can build without repository-relative files.
- Generated `.mjs` files need an appropriate managed marker or exact ownership
  receipt. Existing HTML/YAML markers should not be pasted as invalid JavaScript.

Run all existing package synchronization and bundled-skill checks, plus a build
from a self-contained package/fixture. Verify the installed helper runs with only
either Bun or Node plus Hyalo available, from an unrelated current directory, without
node_modules. Shipping a script that works only in the source checkout would miss
the main packaging requirement.

## Draft of the optional skill entrypoint

The following is proposed wording, to be adapted to each platform's existing skill:

> If the user explicitly requests Jev-assisted classification, read
> `references/jev.md` and use the bundled helper for the selected documents.
> Otherwise follow the ordinary tidy workflow. An API key or repository policy
> does not enable Jev. Its answers are suggestions; preserve the current audit or
> repair scope and report deferrals or unavailable batches.

The reference should contain the helper contract, local category-description
guidance, and the evidence-review/application boundary. Keep SDK configuration and
HTTP mechanics in tested code rather than asking the agent to recreate them.

## Delivery and evaluation

Iteration 299 delivers the independent optional runtime and local evidence
preparation, with mock HTTP/protocol tests and a bundled artifact. Iteration 300
connects it to tidy across Codex, Claude, and pi, verifies installation/ownership,
and exercises realistic audit and repair scenarios in scratch vaults.

Required behavioral cases include ordinary tidy with a key present making zero
requests; explicit assistance without Node/key; no-network preparation; malformed
responses; cancellation/rate limits; ignored model/endpoint/log environment
overrides; source/policy changes; missing-only preservation; folder collisions;
ambiguous links; audit byte preservation; user-owned installation files; and
execution from a package with no development dependencies.

Keep the earlier rubric-repair cases separate from held-out evaluation. Include a
different dogfooding report, an upstream communication note, an ambiguous inbox
note, a multilingual document, and injected classification instructions. Record
accepted-label precision and deferral coverage separately. Live evaluations stay
explicitly invoked, use synthetic or approved scoped content, and never run in
ordinary CI. Measure primary-agent context/calls avoided before claiming tidy
speedups; the successful transport probes alone cannot establish them.
