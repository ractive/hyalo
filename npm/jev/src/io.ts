import { createReadStream } from "node:fs";
import { spawn } from "node:child_process";
import { isAbsolute } from "node:path";
import { Invalid, Unavailable, LIMITS, insist, object } from "./protocol.ts";

export async function readJson(path: string): Promise<unknown> {
  const stream = path === "-" ? process.stdin : createReadStream(path);
  let size = 0;
  const chunks: Buffer[] = [];
  const timer = setTimeout(() => stream.destroy(new Invalid("input deadline exceeded")), LIMITS.deadline);
  try {
    for await (const chunk of stream) {
      const bytes = Buffer.from(chunk); size += bytes.length;
      insist(size <= LIMITS.input, "input exceeds 1 MiB"); chunks.push(bytes);
    }
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch (error) {
    if (error instanceof Invalid) throw error;
    throw new Invalid("cannot read JSON input");
  } finally { clearTimeout(timer); if (path !== "-") stream.destroy(); }
}

export type HyaloRead = (args: string[], allowFindings?: boolean) => Promise<Record<string, unknown>>;
export function hyaloReader(binary: string): HyaloRead {
  insist(isAbsolute(binary), "--hyalo must be an absolute executable path");
  return async (args, allowFindings = false) => {
    const allowed = args[0] === "read" || args[0] === "find" || args[0] === "config" || args[0] === "lint" || (args[0] === "types" && args[1] === "show");
    insist(allowed && !args.some(a => ["--fix", "--apply", "--index"].includes(a)), "read-only Hyalo command required");
    return new Promise((resolve, reject) => {
      // No shell and no credential inheritance. Diagnostics are never echoed.
      const env = { ...process.env };
      for (const key of Object.keys(env)) if (key.startsWith("TYPESAFE_")) delete env[key];
      const child = spawn(binary, [...args, "--format", "json", "--no-hints"], { stdio: ["ignore", "pipe", "pipe"], env, windowsHide: true });
      const chunks: Buffer[] = []; let bytes = 0; let failure: Error | undefined;
      const stop = (error: Error) => { failure ??= error; child.kill("SIGKILL"); };
      const timer = setTimeout(() => stop(new Unavailable("Hyalo read deadline exceeded")), LIMITS.deadline);
      child.stdout.on("data", (chunk: Buffer) => { bytes += chunk.length; if (bytes > LIMITS.input) stop(new Invalid("Hyalo output exceeds limit")); else chunks.push(chunk); });
      child.stderr.on("data", (chunk: Buffer) => { bytes += chunk.length; if (bytes > LIMITS.input) stop(new Invalid("Hyalo output exceeds limit")); });
      child.on("error", () => { failure = new Unavailable("Hyalo executable unavailable"); });
      child.on("close", code => {
        clearTimeout(timer);
        if (failure) return reject(failure);
        if (code !== 0 && !(allowFindings && code === 1)) return reject(new Unavailable("Hyalo read failed"));
        try { resolve(object(JSON.parse(Buffer.concat(chunks).toString("utf8")))); }
        catch { reject(new Invalid("invalid Hyalo response")); }
      });
    });
  };
}
