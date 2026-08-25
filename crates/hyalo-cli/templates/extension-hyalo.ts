import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type, type Static } from "typebox";
import * as path from "node:path";

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

/**
 * Shared execution core for every hyalo tool (generic + typed): runs the
 * argv through `pi.exec` and renders exit codes / stderr / stdout into a
 * uniform tool result. One path — no behavioral divergence between tools.
 */
async function runHyalo(
  pi: ExtensionAPI,
  argv: string[],
  signal?: AbortSignal,
) {
  try {
    const { stdout, stderr, code } = await pi.exec("hyalo", argv, {
      signal,
      timeout: HYALO_TIMEOUT_MS,
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

    return {
      content: [{ type: "text" as const, text: stdout || "(no output)" }],
      details: undefined,
    };
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

/** Whether the argv already carries a value-taking flag (long or short). */
function hasFlag(argv: string[], long: string, short?: string): boolean {
  return argv.includes(long) || (short !== undefined && argv.includes(short));
}

function buildCommand(params: HyaloToolArgs): string[] {
  const { subcommand, args: extraArgs = [], formatText = true, jq, indexFile } = params;
  const cmdArgs = [subcommand];

  // Only inject defaults the caller did not supply themselves: the model
  // often repeats `--format text` from the skill's guidance, and a duplicate
  // `--format` is a hard clap error ("cannot be used multiple times").
  const hasFormat = hasFlag(extraArgs, "--format", "-f");
  if (formatText && !jq && !hasFormat) {
    cmdArgs.push("--format", "text");
  }
  if (jq && !hasFlag(extraArgs, "--jq")) {
    cmdArgs.push("--jq", jq);
  }
  if (indexFile && !hasFlag(extraArgs, "--index-file")) {
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
 * The vault directory is resolved once via `hyalo config` and cached for
 * the session; files outside the vault (or a failed config lookup) skip
 * the check entirely, so non-hyalo projects pay zero cost.
 */
/** Parsed `hyalo config --format json` — the bits the extension needs. */
interface HyaloConfigInfo {
  /** Vault directory name, or null when no vault is configured. */
  vaultDir: string | null;
  /** Effective `[pi] session_summary` (opt-in LLM context injection). */
  sessionSummary: boolean;
}

async function loadHyaloConfig(pi: ExtensionAPI): Promise<HyaloConfigInfo | null> {
  const cached = loadHyaloConfig.cache;
  if (cached !== undefined) return cached;
  try {
    const { stdout, code } = await pi.exec("hyalo", ["config", "--format", "json"], {
      timeout: 10_000,
    });
    if (code !== 0) {
      loadHyaloConfig.cache = null;
      return null;
    }
    // The JSON is an envelope with a top-level `dir` compat key (older
    // hyalo versions printed the flat object without an envelope — both
    // shapes carry the top-level `dir`). `pi` lives under `.results.pi`.
    const parsed = JSON.parse(stdout);
    const dir = parsed?.dir ?? parsed?.results?.dir;
    const sessionSummary = parsed?.results?.pi?.session_summary === true;
    const resolved: HyaloConfigInfo = {
      vaultDir: typeof dir === "string" && dir ? dir : null,
      sessionSummary,
    };
    loadHyaloConfig.cache = resolved;
    return resolved;
  } catch {
    // hyalo not installed or no config: no vault, no guardrail.
    loadHyaloConfig.cache = null;
    return null;
  }
}
// Module-level cache slot (survives across event handler invocations).
loadHyaloConfig.cache = undefined as HyaloConfigInfo | null | undefined;

async function findVaultDir(pi: ExtensionAPI): Promise<string | null> {
  return (await loadHyaloConfig(pi))?.vaultDir ?? null;
}

async function lintVaultFile(
  pi: ExtensionAPI,
  filePath: string,
  signal: AbortSignal | undefined,
): Promise<string | null> {
  try {
    const { stdout, code } = await pi.exec(
      "hyalo",
      ["lint", filePath, "--format", "text"],
      { signal, timeout: 30_000 },
    );
    // lint exits 0 = clean, 1 = violations found (stdout holds them),
    // anything else = lint itself failed: stay silent, don't mask the write.
    if (code !== 0 && code !== 1) return null;
    // Clean single-file output is "N file checked, no issues".
    if (/no issues/.test(stdout)) return null;
    return stdout.trim();
  } catch {
    return null;
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
      return runHyalo(pi, buildCommand(params), signal);
    },
  });

  // --- typed tools -------------------------------------------------------
  // Structured parameters for the ~80% operations. No argv assembly, no
  // flag spelling, no quoting — the schema is the interface. All route
  // through runHyalo, so behavior (timeouts, signals, error rendering) is
  // identical to the generic tool.

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
      "properties, tags, glob, or task status. Preferred over the generic hyalo tool for queries.",
    promptSnippet: "hyalo_find: search/filter knowledgebase files (query, property, tag, task status)",
    parameters: hyaloFindParams,
    async execute(_toolCallId, params: Static<typeof hyaloFindParams>, signal) {
      const argv = ["find", "--format", "text"];
      if (params.query !== undefined) argv.push(params.query);
      for (const prop of params.property ?? []) argv.push("--property", prop);
      if (params.tag !== undefined) argv.push("--tag", params.tag);
      if (params.glob !== undefined) argv.push("--glob", params.glob);
      if (params.taskStatus !== undefined) argv.push("--task", params.taskStatus);
      if (params.countOnly) argv.push("--count");
      if (params.limit !== undefined) argv.push("--limit", String(Math.trunc(params.limit)));
      return runHyalo(pi, argv, signal);
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
      const argv = ["read", "--format", "text", "--file", params.file];
      if (params.section !== undefined) argv.push("--section", params.section);
      return runHyalo(pi, argv, signal);
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
      const argv = [
        "set",
        "--format",
        "text",
        "--property",
        params.property,
      ];
      if (params.tag !== undefined) argv.push("--tag", params.tag);
      argv.push(params.file);
      return runHyalo(pi, argv, signal);
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
      const argv = ["task", "toggle"];
      if (params.mode === "section") {
        if (params.section === undefined) {
          return {
            content: [{ type: "text" as const, text: "hyalo_task: mode 'section' requires the section parameter" }],
            details: undefined,
          };
        }
        argv.push("--section", params.section);
      } else if (params.mode === "line") {
        const lines = (params.lines ?? []).map((l) => Math.trunc(l));
        if (lines.length === 0) {
          return {
            content: [{ type: "text" as const, text: "hyalo_task: mode 'line' requires at least one line number" }],
            details: undefined,
          };
        }
        argv.push("--line", lines.join(","));
      } else {
        argv.push("--all");
      }
      argv.push(params.file);
      return runHyalo(pi, argv, signal);
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
    const config = await loadHyaloConfig(pi);
    if (!config?.sessionSummary || !config.vaultDir) return;
    summaryInjected = true;
    try {
      const { stdout, code } = await pi.exec(
        "hyalo",
        ["summary", "--format", "text"],
        { timeout: 30_000 },
      );
      if (code !== 0 || !stdout.trim()) return;
      pi.sendMessage({
        customType: "hyalo-vault-summary",
        content:
          `Knowledgebase snapshot (${config.vaultDir}/) for this session:\n\n` +
          stdout.trim() +
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

    const vaultDir = await findVaultDir(pi);
    if (!vaultDir) return;

    // Vault membership: normalized absolute path containment check.
    const vaultAbs = path.resolve(process.cwd(), vaultDir);
    const fileAbs = path.resolve(process.cwd(), rawPath);
    if (fileAbs !== vaultAbs && !fileAbs.startsWith(vaultAbs + path.sep)) return;

    const lintOutput = await lintVaultFile(pi, rawPath, ctx.signal);
    if (!lintOutput) return; // clean or lint unavailable

    return {
      content: [
        ...event.content,
        {
          type: "text" as const,
          text:
            `⚠ hyalo lint found issues in ${path.relative(vaultAbs, fileAbs)} ` +
            `(write succeeded, but fix the violations now — e.g. 'hyalo set' for ` +
            `frontmatter, 'hyalo lint --fix' for formatting):\n\n${lintOutput}`,
        },
      ],
    };
  });

  // Register commands for common hyalo operations
  pi.registerCommand("hyalo-help", {
    description: "Show hyalo help",
    handler: async (_args, ctx) => {
      const { stdout } = await pi.exec("hyalo", ["--help"]);
      ctx.ui.notify(stdout, "info");
    },
  });

  pi.registerCommand("hyalo-summary", {
    description: "Show knowledgebase summary",
    handler: async (_args, ctx) => {
      const { stdout } = await pi.exec("hyalo", ["summary", "--format", "text"]);
      ctx.ui.notify(stdout, "info");
    },
  });

  pi.registerCommand("hyalo-lint", {
    description: "Run hyalo lint on knowledgebase",
    handler: async (_args, ctx) => {
      const { stdout } = await pi.exec("hyalo", [
        "lint",
        "--strict",
        "--format",
        "text",
      ]);
      ctx.ui.notify(stdout, "info");
    },
  });
}
