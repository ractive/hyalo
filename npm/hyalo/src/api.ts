import { spawn } from "node:child_process";
import { resolveBinary } from "../lib/resolve-platform.js";

import type { Envelope } from "./generated/Envelope.js";
import type { ErrorEnvelope } from "./generated/ErrorEnvelope.js";
import type {
  ConfigOptions,
  ConfigResult,
  FindOptions,
  FindResult,
  ReadOptions,
  ReadResult,
  SummaryOptions,
  SummaryResult,
} from "./types.js";

export interface ProcessResult {
  stdout: string;
  stderr: string;
  code: number;
}

export interface TransportOptions {
  cwd?: string;
  timeoutMs: number;
  signal?: AbortSignal;
  stdin?: string | Uint8Array;
}

export type HyaloTransport = (
  argv: readonly string[],
  options: TransportOptions,
) => Promise<ProcessResult>;

export interface ExecutionOptions {
  /** Explicit native binary. Useful for Cargo/Homebrew installations and tests. */
  binaryPath?: string;
  /** Process transport override. Pi uses this to retain its `pi.exec("hyalo", ...)` path. */
  transport?: HyaloTransport;
  /** Child working directory. Defaults to the caller's current directory. */
  cwd?: string;
  /** Wall-clock timeout in milliseconds. Defaults to 60 seconds. */
  timeoutMs?: number;
  /** Cancels the running child and rejects with `HyaloAbortError`. */
  signal?: AbortSignal;
  /** Standard input for commands such as `--files-from -`. */
  stdin?: string | Uint8Array;
}

export type FindCallOptions = FindOptions & ExecutionOptions;
export type ReadCallOptions = ReadOptions & ExecutionOptions;
export type SummaryCallOptions = SummaryOptions & ExecutionOptions;
export type ConfigCallOptions = ConfigOptions & ExecutionOptions;

export class HyaloError extends Error {
  readonly exitCode: number;
  readonly stdout: string;
  readonly stderr: string;
  readonly envelope?: ErrorEnvelope;

  constructor(result: ProcessResult, envelope?: ErrorEnvelope) {
    super(envelope?.error ?? `hyalo exited with code ${result.code}`);
    this.name = "HyaloError";
    this.exitCode = result.code;
    this.stdout = result.stdout;
    this.stderr = result.stderr;
    this.envelope = envelope;
  }
}

export class HyaloSpawnError extends Error {
  readonly cause: unknown;

  constructor(cause: unknown) {
    super(cause instanceof Error ? cause.message : String(cause));
    this.name = "HyaloSpawnError";
    this.cause = cause;
  }
}

export class HyaloParseError extends Error {
  readonly stdout: string;
  readonly stderr: string;
  readonly cause?: unknown;

  constructor(message: string, result: ProcessResult, cause?: unknown) {
    super(message);
    this.name = "HyaloParseError";
    this.stdout = result.stdout;
    this.stderr = result.stderr;
    this.cause = cause;
  }
}

export class HyaloTimeoutError extends Error {
  readonly timeoutMs: number;

  constructor(timeoutMs: number) {
    super(`hyalo timed out after ${timeoutMs}ms`);
    this.name = "HyaloTimeoutError";
    this.timeoutMs = timeoutMs;
  }
}

export class HyaloAbortError extends Error {
  constructor() {
    super("hyalo execution was aborted");
    this.name = "HyaloAbortError";
  }
}

export class HyaloTransportError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "HyaloTransportError";
  }
}

const DEFAULT_TIMEOUT_MS = 60_000;
const RESERVED_OUTPUT_KEYS = new Set([
  "format",
  "jq",
  "count",
  "hints",
  "no_hints",
  "filenames_only",
  "filenames0",
  "strict",
]);

function executionOptions(options: Record<string, unknown>): ExecutionOptions {
  return {
    binaryPath: options.binaryPath as string | undefined,
    transport: options.transport as HyaloTransport | undefined,
    cwd: options.cwd as string | undefined,
    timeoutMs: options.timeoutMs as number | undefined,
    signal: options.signal as AbortSignal | undefined,
    stdin: options.stdin as string | Uint8Array | undefined,
  };
}

function assertNoOutputTransforms(options: Record<string, unknown>): void {
  for (const key of RESERVED_OUTPUT_KEYS) {
    if (options[key] !== undefined) {
      throw new TypeError(`typed hyalo calls own --format json --no-hints; '${key}' is reserved`);
    }
  }
}

function addFlag(argv: string[], flag: string, value: unknown): void {
  if (value === undefined || value === null || value === false) return;
  if (value === true) {
    argv.push(flag);
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) argv.push(`${flag}=${String(item)}`);
    return;
  }
  argv.push(`${flag}=${String(value)}`);
}

function addGlobals(argv: string[], options: Record<string, unknown>): void {
  addFlag(argv, "--dir", options.dir);
  addFlag(argv, "--site-prefix", options.site_prefix);
  addFlag(argv, "--quiet", options.quiet);
  addFlag(argv, "--index-file", options.index_file);
}

function findArgv(options: Record<string, unknown>): string[] {
  const argv = ["find"];
  addFlag(argv, "--view", options.view);
  addFlag(argv, "--regexp", options.regexp);
  addFlag(argv, "--property", options.properties);
  addFlag(argv, "--tag", options.tag);
  addFlag(argv, "--task", options.task);
  addFlag(argv, "--section", options.sections);
  addFlag(argv, "--file", options.file);
  addFlag(argv, "--glob", options.glob);
  addFlag(argv, "--files-from", options.files_from);
  addFlag(argv, "--fields", options.fields);
  addFlag(argv, "--sort", options.sort);
  addFlag(argv, "--reverse", options.reverse);
  addFlag(argv, "--limit", options.limit);
  addFlag(argv, "--broken-links", options.broken_links);
  addFlag(argv, "--orphan", options.orphan);
  addFlag(argv, "--dead-end", options.dead_end);
  addFlag(argv, "--title", options.title);
  addFlag(argv, "--language", options.language);
  addFlag(argv, "--index", options.index);
  addGlobals(argv, options);
  const positionalFiles = Array.isArray(options.file_positional)
    ? options.file_positional.map(String)
    : [];
  const positionals: string[] = [];
  if (options.pattern !== undefined && options.pattern !== null) {
    positionals.push(String(options.pattern));
    positionals.push(...positionalFiles);
  } else {
    addFlag(argv, "--file", positionalFiles);
  }
  if (positionals.length > 0) argv.push("--", ...positionals);
  return argv;
}

function readArgv(options: Record<string, unknown>): string[] {
  const argv = ["read"];
  addFlag(argv, "--file", options.file);
  addFlag(argv, "--glob", options.glob);
  addFlag(argv, "--files-from", options.files_from);
  addFlag(argv, "--section", options.section);
  addFlag(argv, "--lines", options.lines);
  addFlag(argv, "--frontmatter", options.frontmatter);
  addFlag(argv, "--index", options.index);
  addGlobals(argv, options);
  if (options.file_positional !== undefined && options.file_positional !== null) {
    argv.push("--", String(options.file_positional));
  }
  return argv;
}

function summaryArgv(options: Record<string, unknown>): string[] {
  const argv = ["summary"];
  addFlag(argv, "--glob", options.glob);
  addFlag(argv, "--recent", options.recent);
  addFlag(argv, "--depth", options.depth);
  addFlag(argv, "--index", options.index);
  addGlobals(argv, options);
  return argv;
}

function nativeTransport(binaryPath?: string): HyaloTransport {
  return async (argv, options) => {
    if (options.signal?.aborted) throw new HyaloAbortError();
    const binary = binaryPath ?? resolveBinary().binary;
    return new Promise<ProcessResult>((resolve, reject) => {
      const child = spawn(binary, [...argv], {
        cwd: options.cwd,
        shell: false,
        stdio: ["pipe", "pipe", "pipe"],
        windowsHide: true,
      });
      const stdout: Buffer[] = [];
      const stderr: Buffer[] = [];
      let settled = false;

      const cleanup = () => {
        clearTimeout(timer);
        options.signal?.removeEventListener("abort", abort);
      };
      const fail = (error: unknown) => {
        if (settled) return;
        settled = true;
        cleanup();
        reject(error);
      };
      const abort = () => {
        child.kill();
        fail(new HyaloAbortError());
      };
      const timer = setTimeout(() => {
        child.kill();
        fail(new HyaloTimeoutError(options.timeoutMs));
      }, options.timeoutMs);

      options.signal?.addEventListener("abort", abort, { once: true });
      child.stdout.on("data", (chunk: Buffer) => stdout.push(chunk));
      child.stderr.on("data", (chunk: Buffer) => stderr.push(chunk));
      child.stdin.on("error", (error: NodeJS.ErrnoException) => {
        // A command may reject argv and close stdin before a large input has
        // finished writing. Its exit status/stderr is the useful result;
        // swallowing EPIPE prevents Node from crashing before `close` reports it.
        if (error.code !== "EPIPE") fail(new HyaloSpawnError(error));
      });
      child.on("error", (error) => fail(new HyaloSpawnError(error)));
      child.on("close", (code) => {
        if (settled) return;
        settled = true;
        cleanup();
        resolve({
          stdout: Buffer.concat(stdout).toString("utf8"),
          stderr: Buffer.concat(stderr).toString("utf8"),
          code: code ?? 2,
        });
      });
      if (options.stdin === undefined) child.stdin.end();
      else child.stdin.end(options.stdin);
    });
  };
}

export function createPiTransport(pi: {
  exec(
    command: string,
    args: string[],
    options: { cwd?: string; signal?: AbortSignal; timeout: number },
  ): Promise<ProcessResult & { killed: boolean }>;
}): HyaloTransport {
  return async (argv, options) => {
    if (options.stdin !== undefined) {
      throw new HyaloTransportError("Pi transport does not support stdin");
    }
    const result = await pi.exec("hyalo", [...argv], {
      cwd: options.cwd,
      signal: options.signal,
      timeout: options.timeoutMs,
    });
    if (result.killed) {
      if (options.signal?.aborted) throw new HyaloAbortError();
      throw new HyaloTimeoutError(options.timeoutMs);
    }
    return { stdout: result.stdout, stderr: result.stderr, code: result.code };
  };
}

export async function execute(
  argv: readonly string[],
  options: ExecutionOptions = {},
): Promise<ProcessResult> {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const transport = options.transport ?? nativeTransport(options.binaryPath);
  try {
    return await transport(argv, {
      cwd: options.cwd,
      timeoutMs,
      signal: options.signal,
      stdin: options.stdin,
    });
  } catch (error) {
    if (
      error instanceof HyaloAbortError ||
      error instanceof HyaloTimeoutError ||
      error instanceof HyaloSpawnError ||
      error instanceof HyaloTransportError
    ) {
      throw error;
    }
    throw new HyaloSpawnError(error);
  }
}

function parseErrorEnvelope(result: ProcessResult): ErrorEnvelope | undefined {
  for (const raw of [result.stderr, result.stdout]) {
    const text = raw.trim();
    if (!text) continue;
    const starts = [0];
    for (let index = text.indexOf("\n{"); index !== -1; index = text.indexOf("\n{", index + 2)) {
      starts.push(index + 1);
    }
    for (const start of starts.reverse()) {
      try {
        const parsed: unknown = JSON.parse(text.slice(start));
        if (
          typeof parsed === "object" &&
          parsed !== null &&
          typeof (parsed as { error?: unknown }).error === "string"
        ) {
          return parsed as ErrorEnvelope;
        }
      } catch {
        // Error envelopes may follow plaintext diagnostics; only a complete
        // JSON object at a line boundary is accepted.
      }
    }
  }
  return undefined;
}

function parseEnvelope<T>(result: ProcessResult): Envelope<T> {
  if (result.code !== 0) throw new HyaloError(result, parseErrorEnvelope(result));
  if (!result.stdout.trim()) throw new HyaloParseError("hyalo returned empty JSON", result);
  let parsed: unknown;
  try {
    parsed = JSON.parse(result.stdout);
  } catch (cause) {
    throw new HyaloParseError("hyalo returned invalid JSON", result, cause);
  }
  if (
    typeof parsed !== "object" ||
    parsed === null ||
    !("results" in parsed) ||
    !Array.isArray((parsed as { hints?: unknown }).hints)
  ) {
    throw new HyaloParseError("hyalo returned an invalid envelope", result);
  }
  return parsed as Envelope<T>;
}

async function jsonCall<T>(argv: string[], options: Record<string, unknown>): Promise<Envelope<T>> {
  assertNoOutputTransforms(options);
  const terminator = argv.indexOf("--");
  argv.splice(terminator === -1 ? argv.length : terminator, 0, "--format=json", "--no-hints");
  return parseEnvelope<T>(await execute(argv, executionOptions(options)));
}

export function find(options: FindCallOptions = {}): Promise<Envelope<FindResult>> {
  const values = options as unknown as Record<string, unknown>;
  return jsonCall<FindResult>(findArgv(values), values);
}

export function read(options: ReadCallOptions = {}): Promise<Envelope<ReadResult>> {
  const values = options as unknown as Record<string, unknown>;
  return jsonCall<ReadResult>(readArgv(values), values);
}

export function summary(options: SummaryCallOptions = {}): Promise<Envelope<SummaryResult>> {
  const values = options as unknown as Record<string, unknown>;
  return jsonCall<SummaryResult>(summaryArgv(values), values);
}

export function config(options: ConfigCallOptions = {}): Promise<Envelope<ConfigResult>> {
  const values = options as unknown as Record<string, unknown>;
  const argv = ["config"];
  addGlobals(argv, values);
  return jsonCall<ConfigResult>(argv, values);
}

export interface SetOptions extends ExecutionOptions {
  file: string;
  property: string;
  tag?: string;
}

export async function set(options: SetOptions): Promise<ProcessResult> {
  const argv = ["set", "--format=text"];
  addFlag(argv, "--property", options.property);
  addFlag(argv, "--tag", options.tag);
  argv.push("--", options.file);
  const result = await execute(argv, options);
  if (result.code !== 0) throw new HyaloError(result, parseErrorEnvelope(result));
  return result;
}

export interface TaskOptions extends ExecutionOptions {
  file: string;
  mode: "all" | "section" | "line";
  section?: string;
  lines?: number[];
}

export async function task(options: TaskOptions): Promise<ProcessResult> {
  const argv = ["task", "toggle"];
  if (options.mode === "section") {
    if (options.section === undefined) throw new TypeError("task mode 'section' requires section");
    addFlag(argv, "--section", options.section);
  } else if (options.mode === "line") {
    if (!options.lines?.length) throw new TypeError("task mode 'line' requires at least one line");
    addFlag(argv, "--line", options.lines.map(Math.trunc).join(","));
  } else if (options.mode === "all") {
    argv.push("--all");
  } else {
    throw new TypeError(`unknown task mode '${String(options.mode)}'`);
  }
  argv.push("--", options.file);
  const result = await execute(argv, options);
  if (result.code !== 0) throw new HyaloError(result, parseErrorEnvelope(result));
  return result;
}

export async function lint(
  file: string | undefined,
  options: ExecutionOptions = {},
): Promise<ProcessResult> {
  const argv = ["lint"];
  argv.push("--format=text", "--no-hints");
  if (file !== undefined) argv.push("--", file);
  const result = await execute(argv, options);
  if (result.code !== 0 && result.code !== 1) {
    throw new HyaloError(result, parseErrorEnvelope(result));
  }
  return result;
}

/** Run an arbitrary command without forcing JSON parsing or interpreting exit codes. */
export function raw(argv: readonly string[], options: ExecutionOptions = {}): Promise<ProcessResult> {
  return execute(argv, options);
}
