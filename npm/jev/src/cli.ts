import { readJson, hyaloReader } from "./io.ts";
import { prepare, selection } from "./prepare.ts";
import { ask } from "./provider.ts";
import { Invalid, Unavailable, insist, manifest, policy, payload, LIMITS } from "./protocol.ts";

async function main(args: string[]) {
  const [command, ...rest] = args;
  if (command === "--help" || command === undefined) {
    return { usage: ["prepare --hyalo /absolute/hyalo --files-from files.json --policy policy.json", "check manifest.json|-", "ask manifest.json|- --allow-network"], limits: LIMITS, writes: false };
  }
  if (command === "prepare") {
    const flags = new Map<string, string>();
    insist(rest.length === 6, "prepare requires --hyalo, --files-from and --policy");
    for (let i = 0; i < rest.length; i += 2) {
      const name = rest[i]!, value = rest[i + 1]!;
      insist(["--hyalo", "--files-from", "--policy"].includes(name) && !flags.has(name) && value !== "-", "invalid prepare arguments"); flags.set(name, value);
    }
    const files = selection(await readJson(flags.get("--files-from")!));
    return prepare(files, policy(await readJson(flags.get("--policy")!)), hyaloReader(flags.get("--hyalo")!));
  }
  if (command === "check" || command === "ask") {
    insist(rest.length === (command === "ask" ? 2 : 1) && (command !== "ask" || rest[1] === "--allow-network"), "ask requires manifest and explicit --allow-network; check requires manifest only");
    const m = manifest(await readJson(rest[0]!));
    if (command === "check") return { valid: true, selected: m.selected, eligible: m.documents.length, deferred: m.deferred, requests: m.documents.filter(d => Object.keys(payload(d, m.policy).request.questions).length > 0).length, writes: false, network: false };
    const result = await ask(m, { allowNetwork: true, apiKey: process.env.TYPESAFE_API_KEY });
    process.exitCode = result.exitCode; return result;
  }
  throw new Invalid("unknown command; use --help");
}

try { process.stdout.write(JSON.stringify(await main(process.argv.slice(2))) + "\n"); }
catch (error) {
  process.exitCode = error instanceof Unavailable ? 1 : 2;
  process.stdout.write(JSON.stringify({ status: error instanceof Unavailable ? "unavailable" : "invalid", error: error instanceof Invalid || error instanceof Unavailable ? error.message : "helper failed; inspect local prerequisites", exitCode: process.exitCode }) + "\n");
}
