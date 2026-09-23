import { readFile, mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";

const root = fileURLToPath(new URL("./", import.meta.url));
const license = await readFile(root + "node_modules/@typesafe-ai/sdk/LICENSE", "utf8");
const result = await Bun.build({ entrypoints: [root + "src/cli.ts"], target: "node", format: "esm", minify: false, banner: "// hyalo:managed\n// Generated from npm/jev; do not edit.\n/*! @typesafe-ai/sdk 0.6.0\n" + license + "*/" });
if (!result.success || result.outputs.length !== 1) throw new Error("Jev bundle failed");
const bytes = await result.outputs[0]!.text();
const destination = process.env.HYALO_JEV_BUNDLE_OUT ?? root + "dist/jev.mjs";
if (process.argv.includes("--check")) {
  if (await readFile(destination, "utf8") !== bytes) throw new Error("Jev bundle is stale; bun run build");
} else { await mkdir(dirname(destination), { recursive: true }); await writeFile(destination, bytes); }
