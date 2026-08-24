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

function buildCommand(params: HyaloToolArgs): string[] {
  const { subcommand, args: extraArgs = [], formatText = true, jq, indexFile } = params;
  const cmdArgs = [subcommand];

  if (formatText && !jq) {
    cmdArgs.push("--format", "text");
  }
  if (jq) {
    cmdArgs.push("--jq", jq);
  }
  if (indexFile) {
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
async function findVaultDir(pi: ExtensionAPI): Promise<string | null> {
  const cached = findVaultDir.cache;
  if (cached !== undefined) return cached;
  try {
    const { stdout, code } = await pi.exec("hyalo", ["config", "--format", "json"], {
      timeout: 10_000,
    });
    if (code !== 0) {
      findVaultDir.cache = null;
      return null;
    }
    // config's JSON is a flat object; .dir is the vault directory name
    // (e.g. "hyalo-knowledgebase"), absent/null when no vault is configured.
    const dir = JSON.parse(stdout)?.dir;
    const resolved = typeof dir === "string" && dir ? dir : null;
    findVaultDir.cache = resolved;
    return resolved;
  } catch {
    // hyalo not installed or no config: no vault, no guardrail.
    findVaultDir.cache = null;
    return null;
  }
}
// Module-level cache slot (survives across event handler invocations).
findVaultDir.cache = undefined as string | null | undefined;

async function lintVaultFile(
  pi: ExtensionAPI,
  filePath: string,
  signal: AbortSignal | undefined,
): Promise<string | null> {
  try {
    const { stdout, code } = await pi.exec(
      "hyalo",
      ["lint", filePath, "--format", "text", "--no-hints"],
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
        "Additional arguments for the subcommand, e.g. ['\"search terms\"', '--tag', 'iteration', '--property', 'status=planned']",
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
      "hyalo: structured search/mutation of markdown knowledgebases (frontmatter, tags, tasks, links)",
    promptGuidelines: [
      "For .md files with YAML frontmatter in a knowledgebase/vault, prefer the hyalo tool over read/edit/grep: use `hyalo find` to search or filter by content/tags/properties, `hyalo read` to read, and `hyalo set`/`hyalo task` to bulk-mutate instead of many edit calls.",
      "hyalo output includes drill-down hints (lines starting with `->`) — follow them to refine queries; hints marked `=>` with `[writes]` modify the vault.",
    ],
    parameters: hyaloToolParams,
    async execute(_toolCallId, params: Static<typeof hyaloToolParams>, signal) {
      const cmdArgs = buildCommand(params);
      try {
        const { stdout, stderr, code } = await pi.exec("hyalo", cmdArgs, {
          signal,
          timeout: HYALO_TIMEOUT_MS,
        });

        if (code !== 0) {
          return {
            content: [
              {
                type: "text" as const,
                text: `hyalo ${params.subcommand} failed with exit code ${code}`,
              },
              ...(stderr
                ? [{ type: "text" as const, text: `Stderr:\n${stderr}` }]
                : []),
              ...(stdout
                ? [{ type: "text" as const, text: `Stdout:\n${stdout}` }]
                : []),
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
    },
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
