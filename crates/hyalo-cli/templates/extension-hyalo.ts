import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type, type Static } from "typebox";

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
