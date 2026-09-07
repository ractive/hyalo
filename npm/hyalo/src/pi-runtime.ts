import type { ExecutionOptions } from "./api.js";
import {
  HyaloError,
  HyaloParseError,
  raw,
  reportDiagnostics,
} from "./api.js";

/** The only configuration fields consumed by the Pi extension. */
export interface PiConfigInfo {
  vaultDir: string | null;
  sessionSummary: boolean;
}

/**
 * Read the current config while retaining compatibility with pre-`[pi]`
 * releases and the flat config JSON shape accepted by the original extension.
 * The public `config()` contract remains the strict Rust-derived modern shape.
 */
export async function configForPi(options: ExecutionOptions = {}): Promise<PiConfigInfo> {
  const result = await raw(["config", "--format=json", "--no-hints"], options);
  if (result.code !== 0) throw new HyaloError(result);
  if (!result.stdout.trim()) throw new HyaloParseError("hyalo returned empty JSON", result);

  let parsed: unknown;
  try {
    parsed = JSON.parse(result.stdout);
  } catch (cause) {
    throw new HyaloParseError("hyalo returned invalid JSON", result, cause);
  }
  if (typeof parsed !== "object" || parsed === null) {
    throw new HyaloParseError("hyalo returned an invalid config object", result);
  }

  const top = parsed as Record<string, unknown>;
  const nested = typeof top.results === "object" && top.results !== null
    ? top.results as Record<string, unknown>
    : top;
  const candidateDir = typeof top.dir === "string" ? top.dir : nested.dir;
  const pi = typeof nested.pi === "object" && nested.pi !== null
    ? nested.pi as Record<string, unknown>
    : undefined;
  await reportDiagnostics(result, options);
  return {
    vaultDir: typeof candidateDir === "string" && candidateDir ? candidateDir : null,
    sessionSummary: pi?.session_summary === true,
  };
}

export {
  HyaloAbortError,
  HyaloError,
  HyaloParseError,
  HyaloSpawnError,
  HyaloTimeoutError,
  HyaloTransportError,
  config,
  createPiTransport,
  find,
  lint,
  raw,
  read,
  set,
  summary,
  task,
} from "./api.js";
