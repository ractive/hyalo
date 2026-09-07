import { rm, mkdir, readFile } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { build } from "esbuild";
import { rollup } from "rollup";
import { dts } from "rollup-plugin-dts";

const execFileAsync = promisify(execFile);
const packageDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const distDir = path.resolve(process.env.HYALO_DIST_DIR || path.join(packageDir, "dist"));
const piBundle = path.resolve(
  process.env.HYALO_PI_BUNDLE_OUT || path.join(packageDir, "../../pi-package/lib/hyalo-api.js"),
);
const piDeclaration = piBundle.replace(/\.js$/, ".d.ts");
const detectLibcDir = path.join(packageDir, "node_modules/detect-libc");
const detectLibcSources = ["detect-libc.js", "elf.js", "filesystem.js", "process.js"];

async function detectLibcBanner() {
  const headers = new Set();
  for (const source of detectLibcSources) {
    const contents = await readFile(path.join(detectLibcDir, "lib", source), "utf8");
    const match = contents.match(
      /^\/\/ (Copyright[^\r\n]+)\r?\n\/\/ (SPDX-License-Identifier: Apache-2\.0)$/m,
    );
    if (!match) throw new Error(`detect-libc notice missing from lib/${source}`);
    headers.add(`${match[1]}\n${match[2]}`);
  }
  if (headers.size !== 1) throw new Error("detect-libc source notices disagree");

  const license = (await readFile(path.join(detectLibcDir, "LICENSE"), "utf8"))
    .replaceAll("\r\n", "\n")
    .trimEnd();
  if (license.includes("*/")) throw new Error("detect-libc LICENSE cannot be embedded safely");
  const lines = ["detect-libc 2.1.2", ...[...headers][0].split("\n"), "", ...license.split("\n")];
  return `/*!\n${lines.map((line) => line ? ` * ${line}` : " *").join("\n")}\n */`;
}

const thirdPartyBanner = await detectLibcBanner();

await rm(distDir, { recursive: true, force: true });
await mkdir(distDir, { recursive: true });
await mkdir(path.dirname(piBundle), { recursive: true });

const shared = {
  entryPoints: [path.join(packageDir, "src/index.ts")],
  bundle: true,
  platform: "node",
  target: "node22.14",
  sourcemap: false,
  legalComments: "none",
  logLevel: "warning",
};

const esmBanner = {
  js: `${thirdPartyBanner}\nimport { createRequire as __hyaloCreateRequire } from "node:module"; const require = __hyaloCreateRequire(import.meta.url);`,
};

await build({ ...shared, format: "esm", banner: esmBanner, outfile: path.join(distDir, "index.mjs") });
await build({
  ...shared,
  format: "cjs",
  banner: { js: thirdPartyBanner },
  outfile: path.join(distDir, "index.cjs"),
});
await build({
  entryPoints: [path.join(packageDir, "src/pi-runtime.ts")],
  bundle: true,
  platform: "node",
  target: "node22.14",
  format: "esm",
  banner: esmBanner,
  outfile: piBundle,
  sourcemap: false,
  legalComments: "none",
  logLevel: "warning",
});
await execFileAsync(
  process.execPath,
  [path.join(packageDir, "node_modules/typescript/bin/tsc"), "--project", path.join(packageDir, "tsconfig.json"), "--outDir", distDir],
  { cwd: packageDir },
);

const declarationBundle = await rollup({
  input: path.join(distDir, "pi-runtime.d.ts"),
  plugins: [dts()],
});
await declarationBundle.write({ file: piDeclaration, format: "es" });
await declarationBundle.close();

for (const bundle of [path.join(distDir, "index.mjs"), path.join(distDir, "index.cjs"), piBundle]) {
  const contents = await readFile(bundle, "utf8");
  for (const required of [
    "Copyright 2017 Lovell Fuller and others.",
    "SPDX-License-Identifier: Apache-2.0",
    "Apache License",
    "TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION",
    "END OF TERMS AND CONDITIONS",
  ]) {
    if (!contents.includes(required)) {
      throw new Error(`${path.relative(packageDir, bundle)} is missing detect-libc notice text: ${required}`);
    }
  }
}
