import type { ExtensionAPI, Tool, ToolContext } from "@earendil-works/pi-coding-agent";

interface HyaloToolArgs {
  /** The hyalo subcommand (find, read, set, etc.) */
  subcommand: string;
  /** Arguments to pass to hyalo */
  args?: string[];
  /** Use --format text for compact output (default: true) */
  formatText?: boolean;
  /** Use --jq filter (mutually exclusive with formatText) */
  jq?: string;
  /** Use snapshot index for performance */
  useIndex?: boolean;
}

export default function (pi: ExtensionAPI) {
  // Register hyalo tool
  pi.registerTool({
    name: "hyalo",
    description: "Run hyalo commands for markdown knowledgebase operations",
    parameters: {
      type: "object",
      properties: {
        subcommand: {
          type: "string",
          description: "Hyalo subcommand (find, read, set, summary, lint, etc.)",
        },
        args: {
          type: "array",
          items: { type: "string" },
          description: "Additional arguments to pass to hyalo",
        },
        formatText: {
          type: "boolean",
          description: "Use --format text for compact LLM-friendly output (default: true)",
        },
        jq: {
          type: "string",
          description: "JQ filter to apply to JSON output (mutually exclusive with formatText)",
        },
        useIndex: {
          type: "boolean",
          description: "Use snapshot index for performance (recommended for large vaults)",
        },
      },
      required: ["subcommand"],
    },
    async execute(args: HyaloToolArgs, ctx: ToolContext) {
      const { subcommand, args: extraArgs = [], formatText = true, jq, useIndex = false } = args;
      
      // Build command line
      const cmdArgs = [subcommand];
      
      // Add --format text unless jq is specified
      if (formatText && !jq) {
        cmdArgs.push("--format", "text");
      }
      
      // Add jq if specified
      if (jq) {
        cmdArgs.push("--jq", jq);
      }
      
      // Add index flag if requested
      if (useIndex) {
        cmdArgs.push("--index");
      }
      
      // Add extra arguments
      cmdArgs.push(...extraArgs);
      
      try {
        // Run hyalo
        const { stdout, stderr, code } = await pi.exec("hyalo", cmdArgs);
        
        if (code !== 0) {
          return {
            content: [
              {
                type: "text" as const,
                text: `Hyalo command failed with exit code ${code}`,
              },
              ...(stderr ? [{
                type: "text" as const,
                text: `Stderr:\n${stderr}`,
              }] : []),
              ...(stdout ? [{
                type: "text" as const,
                text: `Stdout:\n${stdout}`,
              }] : []),
            ],
          };
        }
        
        // Success
        return {
          content: [
            {
              type: "text" as const,
              text: stdout || "(no output)",
            },
          ],
        };
      } catch (error) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Error executing hyalo: ${error instanceof Error ? error.message : String(error)}`,
            },
          ],
        };
      }
    },
  });
  
  // Register commands for common hyalo operations
  pi.registerCommand({
    name: "hyalo-help",
    description: "Show hyalo help",
    async execute(ctx) {
      const { stdout } = await pi.exec("hyalo", ["--help"]);
      ctx.ui.print(stdout);
    },
  });
  
  pi.registerCommand({
    name: "hyalo-summary",
    description: "Show knowledgebase summary",
    async execute(ctx) {
      const { stdout } = await pi.exec("hyalo", ["summary", "--format", "text"]);
      ctx.ui.print(stdout);
    },
  });
  
  pi.registerCommand({
    name: "hyalo-lint",
    description: "Run hyalo lint on knowledgebase",
    async execute(ctx) {
      const { stdout } = await pi.exec("hyalo", ["lint", "--strict", "--format", "text"]);
      ctx.ui.print(stdout);
    },
  });
}