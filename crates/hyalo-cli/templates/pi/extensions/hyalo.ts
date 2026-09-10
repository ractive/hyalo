import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type, type Static } from "typebox";
import * as path from "node:path";
import {
  configForPi as hyaloConfig,
  createPiTransport,
  find as hyaloFind,
  lint as hyaloLint,
  mutationReport as hyaloMutationReport,
  raw as hyaloRaw,
  read as hyaloRead,
  summary as hyaloSummary,
} from "../lib/hyalo-api.js";

const HYALO_TIMEOUT_MS = 60_000;

interface HyaloToolArgs {
  /** The hyalo subcommand (find, read, set, summary, lint, etc.) */
  subcommand: string;
  /** Arguments to pass to hyalo */
  args?: string[];
  /** Use --format text for compact output (default: true) */
  formatText?: boolean;
  /** Use --jq filter (mutually exclusive with formatText) */
  jq?: string;
  /** Path to a snapshot index created with `hyalo create-index` */
  indexFile?: string;
}

function renderSuccess(text: string, diagnostics: readonly string[] = []) {
  return {
    content: [
      { type: "text" as const, text: text || "(no output)" },
      ...diagnostics
        .filter((diagnostic) => diagnostic.length > 0)
        .map((diagnostic) => ({ type: "text" as const, text: `Stderr:\n${diagnostic}` })),
    ],
    details: undefined,
  };
}

/**
 * Shared execution core for every hyalo tool (generic + typed): runs the
 * argv through `pi.exec` and renders exit codes / stderr / stdout into a
 * uniform tool result. One path — no behavioral divergence between tools.
 */
async function runHyalo(
  transport: ReturnType<typeof createPiTransport>,
  argv: string[],
  signal?: AbortSignal,
) {
  try {
    const { stdout, stderr, code } = await hyaloRaw(argv, {
      transport,
      signal,
      timeoutMs: HYALO_TIMEOUT_MS,
    });

    if (code !== 0) {
      return {
        content: [
          {
            type: "text" as const,
            text: `hyalo ${argv[0]} failed with exit code ${code}`,
          },
          ...(stderr ? [{ type: "text" as const, text: `Stderr:\n${stderr}` }] : []),
          ...(stdout ? [{ type: "text" as const, text: `Stdout:\n${stdout}` }] : []),
        ],
        details: undefined,
      };
    }

    return renderSuccess(stdout, [stderr]);
  } catch (error) {
    return {
      content: [
        {
          type: "text" as const,
          text: `Error executing hyalo: ${error instanceof Error ? error.message : String(error)}`,
        },
      ],
      details: undefined,
    };
  }
}

async function runTyped(
  command: string,
  operation: (onDiagnostics: (stderr: string) => void) => Promise<string>,
) {
  try {
    const diagnostics: string[] = [];
    const text = await operation((stderr) => { diagnostics.push(stderr); });
    return renderSuccess(text, diagnostics);
  } catch (error) {
    const value = error as {
      exitCode?: number;
      stderr?: string;
      stdout?: string;
      message?: string;
      guardrail?: string;
    };
    if (typeof value.exitCode === "number") {
      return {
        content: [
          { type: "text" as const, text: `hyalo ${command} failed with exit code ${value.exitCode}` },
          ...(value.stderr ? [{ type: "text" as const, text: `Stderr:\n${value.stderr}` }] : []),
          ...(value.stdout ? [{ type: "text" as const, text: `Stdout:\n${value.stdout}` }] : []),
          ...(value.guardrail ? [{ type: "text" as const, text: value.guardrail }] : []),
        ],
        details: undefined,
      };
    }
    return {
      content: [{ type: "text" as const, text: `Error executing hyalo: ${value.message ?? String(error)}` }],
      details: undefined,
    };
  }
}

/** Values supplied for a long option before the positional `--` terminator. */
function optionValues(argv: string[], option: string): string[] {
  const values: string[] = [];
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--") break;
    if (arg === option) {
      const value = argv[index + 1];
      if (value === undefined || value === "--") {
        throw new TypeError(`${option} requires a value`);
      }
      values.push(value);
      index += 1;
    } else if (arg.startsWith(`${option}=`)) {
      values.push(arg.slice(option.length + 1));
    }
  }
  return values;
}

function oneOptionValue(argv: string[], option: string): string | undefined {
  const values = optionValues(argv, option);
  if (values.length > 1) {
    throw new TypeError(`conflicting duplicate ${option} options`);
  }
  return values[0];
}

function buildCommand(params: HyaloToolArgs): string[] {
  const { subcommand, args: extraArgs = [], formatText = true, jq, indexFile } = params;
  const cmdArgs = [subcommand];

  // Only inject defaults the caller did not supply themselves: the model
  // often repeats `--format text` from the skill's guidance, and a duplicate
  // `--format` is a hard clap error ("cannot be used multiple times").
  const rawFormat = oneOptionValue(extraArgs, "--format");
  const rawJq = oneOptionValue(extraArgs, "--jq");
  const rawIndexFile = oneOptionValue(extraArgs, "--index-file");
  if (jq !== undefined && rawJq !== undefined && jq !== rawJq) {
    throw new TypeError("conflicting --jq values supplied by jq and args");
  }
  if (indexFile !== undefined && rawIndexFile !== undefined && indexFile !== rawIndexFile) {
    throw new TypeError("conflicting --index-file values supplied by indexFile and args");
  }
  const effectiveJq = jq ?? rawJq;
  if (effectiveJq !== undefined && rawFormat !== undefined && rawFormat !== "json") {
    throw new TypeError("--jq cannot be combined with a non-JSON --format");
  }
  if (formatText && effectiveJq === undefined && rawFormat === undefined) {
    cmdArgs.push("--format", "text");
  }
  if (jq !== undefined && rawJq === undefined) {
    cmdArgs.push("--jq", jq);
  }
  if (indexFile !== undefined && rawIndexFile === undefined) {
    cmdArgs.push("--index-file", indexFile);
  }
  cmdArgs.push(...extraArgs);
  return cmdArgs;
}

/**
 * Post-write lint guardrail.
 *
 * After pi's write/edit tools touch a .md file inside the hyalo vault,
 * run `hyalo lint <file>` and append any violations to the tool result.
 * The agent sees the findings in the same turn and fixes them immediately
 * — schema drift can no longer land silently. Clean files add nothing.
 *
 * A successfully resolved vault directory is cached for the session. A
 * failed lookup is reported for a successful observed write and retried on
 * the next mutation, so a transient config failure cannot disable checks for
 * the rest of the process.
 */
type HyaloConfigLookup =
  | { status: "available"; config: Awaited<ReturnType<typeof hyaloConfig>> }
  | { status: "unavailable"; text: string };

async function loadHyaloConfig(
  transport: ReturnType<typeof createPiTransport>,
): Promise<HyaloConfigLookup> {
  const cached = loadHyaloConfig.cache;
  if (cached !== undefined) return { status: "available", config: cached };
  try {
    const resolved = await hyaloConfig({ transport, timeoutMs: 10_000 });
    loadHyaloConfig.cache = resolved;
    return { status: "available", config: resolved };
  } catch (error) {
    const value = error as { message?: string; stderr?: string };
    return {
      status: "unavailable",
      text: value.stderr?.trim() || value.message || String(error),
    };
  }
}
// Module-level cache slot (survives across event handler invocations).
loadHyaloConfig.cache = undefined as Awaited<ReturnType<typeof hyaloConfig>> | undefined;

async function findVaultDir(
  transport: ReturnType<typeof createPiTransport>,
): Promise<{ status: "available"; vaultDir: string | null } | { status: "unavailable"; text: string }> {
  const lookup = await loadHyaloConfig(transport);
  return lookup.status === "available"
    ? { status: "available", vaultDir: lookup.config.vaultDir ?? null }
    : lookup;
}

type LintGuardResult =
  | { status: "clean" }
  | { status: "findings"; text: string }
  | { status: "unavailable"; text: string };

async function lintVaultFile(
  transport: ReturnType<typeof createPiTransport>,
  filePath: string,
  signal: AbortSignal | undefined,
): Promise<LintGuardResult> {
  try {
    const { stdout, code } = await hyaloLint(filePath, { transport, signal, timeoutMs: 30_000 });
    if (code !== 0 && code !== 1) {
      return { status: "unavailable", text: `hyalo lint exited with code ${code}` };
    }
    // Clean single-file output is "N file checked, no issues".
    if (code === 0 && /no issues/.test(stdout)) return { status: "clean" };
    return { status: "findings", text: stdout.trim() };
  } catch (error) {
    return {
      status: "unavailable",
      text: error instanceof Error ? error.message : String(error),
    };
  }
}

const SURVIVING_EFFECTS = new Set([
  "committed",
  "committed_with_finalization_error",
  "kept",
  "restore_failed",
]);

async function lintMutationEffects(
  transport: ReturnType<typeof createPiTransport>,
  effects: { paths?: Array<{ file: string; state: string }> } | undefined,
  signal: AbortSignal | undefined,
): Promise<string | null> {
  if (!effects?.paths) return null;
  const files = [...new Set(effects.paths
    .filter((effect) => SURVIVING_EFFECTS.has(effect.state) && effect.file.endsWith(".md"))
    .map((effect) => effect.file))];
  if (files.length === 0) return null;
  const vault = await findVaultDir(transport);
  if (vault.status === "unavailable") {
    return `⚠ post-write hyalo lint was unavailable for ${files.join(", ")}; ` +
      `the write succeeded but its lint status is unknown: unable to resolve the vault: ${vault.text}`;
  }
  const vaultDir = vault.vaultDir;
  if (!vaultDir) return null;
  const vaultAbs = path.resolve(process.cwd(), vaultDir);
  const messages: string[] = [];
  for (const file of files) {
    const fileAbs = path.resolve(vaultAbs, file);
    if (fileAbs !== vaultAbs && !fileAbs.startsWith(vaultAbs + path.sep)) continue;
    const lint = await lintVaultFile(transport, file, signal);
    if (lint.status === "findings") {
      messages.push(
        `⚠ hyalo lint found issues in ${file} (the write succeeded; fix the violations now):\n\n${lint.text}`,
      );
    } else if (lint.status === "unavailable") {
      messages.push(
        `⚠ post-write hyalo lint was unavailable for ${file}; the write succeeded but its lint status is unknown: ${lint.text}`,
      );
    }
  }
  return messages.length > 0 ? messages.join("\n\n") : null;
}

async function runObservedMutation(
  transport: ReturnType<typeof createPiTransport>,
  argv: string[],
  signal: AbortSignal | undefined,
  onDiagnostics: (stderr: string) => void,
): Promise<string> {
  try {
    const report = await hyaloMutationReport<unknown>(argv, {
      transport,
      signal,
      timeoutMs: HYALO_TIMEOUT_MS,
      onDiagnostics,
    });
    const guardrail = await lintMutationEffects(transport, report.effects, signal);
    const text = JSON.stringify(report.results, null, 2);
    return guardrail ? `${text}\n\n${guardrail}` : text;
  } catch (error) {
    const value = error as {
      effects?: { paths?: Array<{ file: string; state: string }> };
      guardrail?: string;
    };
    const guardrail = await lintMutationEffects(transport, value.effects, signal);
    if (guardrail) value.guardrail = guardrail;
    throw error;
  }
}

const hyaloToolParams = Type.Object({
  subcommand: Type.String({
    description: "Hyalo subcommand (find, read, set, summary, lint, task, backlinks, config, ...)",
  }),
  args: Type.Optional(
    Type.Array(Type.String(), {
      description:
        "Additional arguments for the subcommand. Property filters use '--property K=V' (e.g. '--property', 'status=planned'); there is NO --status/--type/--priority flag — status is not a flag, it is a property. Other common flags: '--tag T', '--task todo|done|any', '--section H', '--count', '--limit N'.",
    }),
  ),
  formatText: Type.Optional(
    Type.Boolean({
      description:
        "Use --format text for compact LLM-friendly output (default: true; ignored when jq is set)",
    }),
  ),
  jq: Type.Optional(
    Type.String({
      description:
        "Apply a jq filter to the JSON envelope, e.g. '.results[].file' or '.total' (overrides formatText)",
    }),
  ),
  indexFile: Type.Optional(
    Type.String({
      description:
        "Path to a snapshot index (created via `hyalo create-index`) for fast queries on large vaults",
    }),
  ),
});

export default function (pi: ExtensionAPI) {
  const transport = createPiTransport(pi);
  pi.registerTool({
    name: "hyalo",
    label: "Hyalo",
    description:
      "Run hyalo commands to search, read, and mutate a markdown knowledgebase " +
      "(YAML frontmatter, tags, tasks, wikilinks). Subcommands: find, read, set, " +
      "append, remove, task, summary, properties, tags, lint, backlinks, config, ...",
    promptSnippet:
      "hyalo: structured search/mutation of markdown knowledgebases (frontmatter, tags, tasks, links). Prefer hyalo_find/hyalo_read/hyalo_set/hyalo_task for common operations; use this tool for everything else.",
    promptGuidelines: [
      "For .md files with YAML frontmatter in a knowledgebase/vault, prefer the typed hyalo tools first — `hyalo_find` (search/filter by query/property/tag/task status), `hyalo_read` (read a file or section), `hyalo_set` (set one frontmatter property), `hyalo_task` (toggle checkboxes) — they take structured parameters, no flags or quoting. Fall back to the generic hyalo tool for anything they don't cover (summary, lint, mv, links, views, --jq, ...).",
      "hyalo output includes drill-down hints (lines starting with `->`) — follow them to refine queries; hints marked `=>` with `[writes]` modify the vault.",
    ],
    parameters: hyaloToolParams,
    async execute(_toolCallId, params: Static<typeof hyaloToolParams>, signal) {
      return runHyalo(transport, buildCommand(params), signal);
    },
  });

  // --- typed tools -------------------------------------------------------
  // Structured parameters for the common operations. The schema is the
  // interface; adapters assemble raw argv without involving a shell.

  const hyaloFindParams = Type.Object({
    query: Type.Optional(
      Type.String({
        description:
          "BM25 full-text search term(s). Supports 'a OR b', '" +
          '"quoted phrase"' +
          "', and '-term' exclusions.",
      }),
    ),
    property: Type.Optional(
      Type.Array(Type.String(), {
        description:
          "Property filter(s) as 'K=V'. Also supports 'K!=V', 'K>=V', 'K<=V', 'K~=pattern', '!K'. Repeatable.",
      }),
    ),
    tag: Type.Optional(
      Type.String({ description: "Filter by tag." }),
    ),
    glob: Type.Optional(
      Type.String({ description: "Restrict to files matching a glob, e.g. 'iterations/*.md'." }),
    ),
    taskStatus: Type.Optional(
      Type.Union([
        Type.Literal("todo"),
        Type.Literal("done"),
        Type.Literal("any"),
      ], {
        description: "Filter by task checkbox status in the file.",
      }),
    ),
    countOnly: Type.Optional(
      Type.Boolean({ description: "Return only the match count (--count)." }),
    ),
    limit: Type.Optional(
      Type.Number({ description: "Maximum number of results to return." }),
    ),
  });

  pi.registerTool({
    name: "hyalo_find",
    label: "Hyalo Find",
    description:
      "Search/filter a markdown knowledgebase by full-text query, frontmatter " +
      "properties, tags, glob, or task status. Ranked queries include score and up to 3 body matches " +
      "with line, section, text. Preferred over the generic hyalo tool for queries.",
    promptSnippet: "hyalo_find: search/filter knowledgebase files (query, property, tag, task status)",
    parameters: hyaloFindParams,
    async execute(_toolCallId, params: Static<typeof hyaloFindParams>, signal) {
      return runTyped("find", async (onDiagnostics) => {
        const result = await hyaloFind({
          pattern: params.query,
          properties: params.property,
          tag: params.tag === undefined ? undefined : [params.tag],
          glob: params.glob === undefined ? undefined : [params.glob],
          task: params.taskStatus,
          limit: params.limit === undefined ? undefined : Math.trunc(params.limit),
          transport,
          signal,
          onDiagnostics,
        });
        return params.countOnly
          ? String(result.total ?? result.results.length)
          : JSON.stringify(result, null, 2);
      });
    },
  });

  const hyaloReadParams = Type.Object({
    file: Type.String({ description: "File to read (relative to the vault directory)." }),
    section: Type.Optional(
      Type.String({
        description:
          "Extract only this section by heading (case-insensitive substring; prefix '#' pins level; '/regex/' form also accepted). Nested subsections included.",
      }),
    ),
  });

  pi.registerTool({
    name: "hyalo_read",
    label: "Hyalo Read",
    description:
      "Read a markdown file's body from the knowledgebase (frontmatter stripped), optionally only one section. Returns plain text.",
    promptSnippet: "hyalo_read: read a vault file (optionally a single section) as text",
    parameters: hyaloReadParams,
    async execute(_toolCallId, params: Static<typeof hyaloReadParams>, signal) {
      return runTyped("read", async (onDiagnostics) => {
        const result = await hyaloRead({ file: [params.file], section: params.section, transport, signal, onDiagnostics });
        return result.results.content ?? result.results.frontmatter_raw ?? "";
      });
    },
  });

  const hyaloSetParams = Type.Object({
    file: Type.String({ description: "File to mutate (relative to the vault directory)." }),
    property: Type.String({
      description:
        "Frontmatter assignment as 'K=V'. Type is auto-inferred (number/bool/text); use K=[a,b,c] for lists.",
    }),
    tag: Type.Optional(
      Type.String({ description: "Additionally add this tag (idempotent; creates the tags list if absent)." }),
    ),
  });

  pi.registerTool({
    name: "hyalo_set",
    label: "Hyalo Set",
    description:
      "Set (create or overwrite) one frontmatter property on a knowledgebase file, optionally adding a tag. The post-write lint guardrail applies to writes automatically.",
    promptSnippet: "hyalo_set: set a file's frontmatter property (K=V), optionally add a tag",
    parameters: hyaloSetParams,
    async execute(_toolCallId, params: Static<typeof hyaloSetParams>, signal) {
      return runTyped("set", async (onDiagnostics) => {
        const argv = ["set", `--property=${params.property}`];
        if (params.tag !== undefined) argv.push(`--tag=${params.tag}`);
        argv.push("--", params.file);
        return runObservedMutation(transport, argv, signal, onDiagnostics);
      });
    },
  });

  const hyaloTaskParams = Type.Object({
    file: Type.String({ description: "File containing the tasks (relative to the vault directory)." }),
    mode: Type.Union([Type.Literal("all"), Type.Literal("section"), Type.Literal("line")], {
      description:
        "'all': toggle every task in the file; 'section': all tasks under one heading; 'line': specific lines.",
    }),
    section: Type.Optional(
      Type.String({ description: "Heading for mode='section' (case-insensitive substring)." }),
    ),
    lines: Type.Optional(
      Type.Array(Type.Number(), { description: "1-based line numbers for mode='line'." }),
    ),
  });

  pi.registerTool({
    name: "hyalo_task",
    label: "Hyalo Task",
    description:
      "Toggle task checkboxes ([ ] <-> [x]) in a knowledgebase file: every task, all tasks under a section heading, or specific lines.",
    promptSnippet: "hyalo_task: toggle task checkboxes (all / by section / by line)",
    parameters: hyaloTaskParams,
    async execute(_toolCallId, params: Static<typeof hyaloTaskParams>, signal) {
      return runTyped("task", async (onDiagnostics) => {
        const argv = ["task", "toggle"];
        if (params.mode === "section") {
          if (params.section === undefined) {
            throw new TypeError("task mode 'section' requires section");
          }
          argv.push(`--section=${params.section}`);
        } else if (params.mode === "line") {
          if (!params.lines?.length) {
            throw new TypeError("task mode 'line' requires at least one line");
          }
          argv.push(`--line=${params.lines.map(Math.trunc).join(",")}`);
        } else {
          argv.push("--all");
        }
        argv.push("--", params.file);
        return runObservedMutation(transport, argv, signal, onDiagnostics);
      });
    },
  });

  // Opt-in vault summary injection: with `[pi] session_summary = true` in
  // .hyalo.toml, inject a `hyalo summary` snapshot into the LLM context at
  // session start (custom message, not displayed in the TUI). Injected at
  // most once per pi process — a fork/new session in the same process keeps
  // the summary already present in the transcript or skips a duplicate.
  let summaryInjected = false;
  pi.on("session_start", async () => {
    if (summaryInjected) return;
    const lookup = await loadHyaloConfig(transport);
    if (lookup.status === "unavailable") return;
    const config = lookup.config;
    if (!config.sessionSummary || !config.vaultDir) return;
    summaryInjected = true;
    try {
      const snapshot = await hyaloSummary({ transport, timeoutMs: 30_000 });
      const summaryText = JSON.stringify(snapshot.results, null, 2);
      if (!summaryText) return;
      pi.sendMessage({
        customType: "hyalo-vault-summary",
        content:
          `Knowledgebase snapshot (${config.vaultDir}/) for this session:\n\n` +
          summaryText +
          "\n\nUse this to orient yourself; refine with the hyalo tool.",
        display: false,
        details: undefined,
      });
    } catch {
      // summary unavailable: skip injection silently
    }
  });

  // Post-write lint guardrail: append hyalo lint findings to write/edit
  // tool results for vault .md files (see findVaultDir/lintVaultFile above).
  pi.on("tool_result", async (event, ctx) => {
    if (event.toolName !== "write" && event.toolName !== "edit") return;
    if (event.isError) return; // failed writes: don't pile on

    const rawPath = event.input.path;
    if (typeof rawPath !== "string" || !rawPath.endsWith(".md")) return;

    const vault = await findVaultDir(transport);
    if (vault.status === "unavailable") {
      return {
        content: [
          ...event.content,
          {
            type: "text" as const,
            text: `⚠ post-write hyalo lint was unavailable for ${rawPath}; ` +
              `the write succeeded but its lint status is unknown: unable to resolve the vault: ${vault.text}`,
          },
        ],
      };
    }
    const vaultDir = vault.vaultDir;
    if (!vaultDir) return;

    // Vault membership: normalized absolute path containment check.
    const vaultAbs = path.resolve(process.cwd(), vaultDir);
    const fileAbs = path.resolve(process.cwd(), rawPath);
    if (fileAbs !== vaultAbs && !fileAbs.startsWith(vaultAbs + path.sep)) return;

    const lintOutput = await lintVaultFile(transport, rawPath, ctx.signal);
    if (lintOutput.status === "clean") return;

    const relative = path.relative(vaultAbs, fileAbs);
    const message = lintOutput.status === "findings"
      ? `⚠ hyalo lint found issues in ${relative} ` +
        `(write succeeded, but fix the violations now — e.g. 'hyalo set' for ` +
        `frontmatter, 'hyalo lint --fix' for formatting):\n\n${lintOutput.text}`
      : `⚠ post-write hyalo lint was unavailable for ${relative}; ` +
        `the write succeeded but its lint status is unknown: ${lintOutput.text}`;

    return {
      content: [
        ...event.content,
        {
          type: "text" as const,
          text: message,
        },
      ],
    };
  });

  // Register commands for common hyalo operations
  pi.registerCommand("hyalo-help", {
    description: "Show hyalo help",
    handler: async (_args, ctx) => {
      const { stdout } = await hyaloRaw(["--help"], { transport });
      ctx.ui.notify(stdout, "info");
    },
  });

  pi.registerCommand("hyalo-summary", {
    description: "Show knowledgebase summary",
    handler: async (_args, ctx) => {
      const { stdout } = await hyaloRaw(["summary", "--format", "text"], { transport });
      ctx.ui.notify(stdout, "info");
    },
  });

  pi.registerCommand("hyalo-lint", {
    description: "Run hyalo lint on knowledgebase",
    handler: async (_args, ctx) => {
      const { stdout } = await hyaloRaw([
        "lint",
        "--strict",
        "--format",
        "text",
      ], { transport });
      ctx.ui.notify(stdout, "info");
    },
  });
}
