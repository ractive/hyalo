import { spawn } from "node:child_process";
import { resolveBinary } from "../lib/resolve-platform.js";

import type { Envelope } from "./generated/Envelope.js";
import type { MutationReportEnvelope } from "./generated/MutationReportEnvelope.js";
import type { ErrorEnvelope } from "./generated/ErrorEnvelope.js";
import type {
  BacklinksOptions,
  BacklinksResult,
  ConfigOptions,
  ConfigResult,
  FindOptions,
  FindResult,
  ReadOptions,
  ReadResult,
  SummaryOptions,
  SummaryResult,
  TagsOptions,
  TagsResult,
  TermsOptions,
  TermsResult,
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

/** Receives the original successful stderr once; returned promises are awaited. */
export type DiagnosticsCallback = (stderr: string) => void | Promise<void>;

export interface ExecutionOptions {
  /**
   * Successful typed-call stderr. Defaults to process.stderr.write.
   * Empty stderr is ignored. Callback throws/rejections reject the call unchanged.
   * Failed calls retain diagnostics in their error; raw()/execute() retain streams only.
   */
  onDiagnostics?: DiagnosticsCallback;
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
export type TermsCallOptions = TermsOptions & ExecutionOptions;
export type TagsCallOptions = TagsOptions & ExecutionOptions;
export type BacklinksCallOptions = BacklinksOptions & ExecutionOptions;

/**
 * Brand shared by every copy of the error classes. The ESM, CommonJS and Pi
 * bundles each carry their own class objects, so `instanceof` fails across
 * them; `Symbol.for` resolves to the same registry symbol in every copy.
 */
const HYALO_ERROR_BRAND = Symbol.for("@ractive-ch/hyalo/error");

function brand(error: Error): void {
  Object.defineProperty(error, HYALO_ERROR_BRAND, { value: true, enumerable: false });
}

/** @internal True for an error constructed by any copy of this module, optionally of the given class names. */
export function isHyaloBranded(error: unknown, names?: ReadonlySet<string>): error is Error {
  if (typeof error !== "object" || error === null) return false;
  if ((error as { [HYALO_ERROR_BRAND]?: unknown })[HYALO_ERROR_BRAND] !== true) return false;
  return names === undefined || names.has((error as Error).name);
}

export class HyaloError extends Error {
  readonly exitCode: number;
  readonly stdout: string;
  readonly stderr: string;
  readonly envelope?: ErrorEnvelope;
  /** Committed paths and index disposition, retained even after output failure. */
  readonly effects?: ErrorEnvelope["effects"];
  readonly category?: ErrorEnvelope["category"];

  constructor(result: ProcessResult, envelope?: ErrorEnvelope, message?: string) {
    super(message ?? envelope?.error ?? `hyalo exited with code ${result.code}`);
    brand(this);
    this.name = "HyaloError";
    this.exitCode = result.code;
    this.stdout = result.stdout;
    this.stderr = result.stderr;
    this.envelope = envelope;
    this.effects = envelope?.effects;
    this.category = envelope?.category;
  }
}

export class HyaloSpawnError extends Error {
  readonly cause: unknown;

  constructor(cause: unknown) {
    super(cause instanceof Error ? cause.message : String(cause));
    brand(this);
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
    brand(this);
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
    brand(this);
    this.name = "HyaloTimeoutError";
    this.timeoutMs = timeoutMs;
  }
}

export class HyaloAbortError extends Error {
  constructor() {
    super("hyalo execution was aborted");
    brand(this);
    this.name = "HyaloAbortError";
  }
}

export class HyaloTransportError extends Error {
  constructor(message: string) {
    super(message);
    brand(this);
    this.name = "HyaloTransportError";
  }
}

const DEFAULT_TIMEOUT_MS = 60_000;
/** Time a cancelled child gets to exit after SIGTERM before SIGKILL, and after SIGKILL before its pipes are abandoned. */
const KILL_GRACE_MS = 2_000;
const PASSTHROUGH_ERRORS: ReadonlySet<string> = new Set([
  "HyaloAbortError",
  "HyaloTimeoutError",
  "HyaloSpawnError",
  "HyaloTransportError",
]);
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

/** @internal Cross-platform closed-peer errors emitted while writing child stdin. */
export function isClosedStdinWriteError(
  error: Pick<NodeJS.ErrnoException, "code">,
): boolean {
  return error.code === "EPIPE" || error.code === "EOF";
}

function executionOptions(options: Record<string, unknown>): ExecutionOptions {
  return {
    onDiagnostics: options.onDiagnostics as DiagnosticsCallback | undefined,
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
        env: { ...process.env, HYALO_INTERNAL_JSON_ERRORS: "1" },
        shell: false,
        stdio: ["pipe", "pipe", "pipe"],
        windowsHide: true,
      });
      const stdout: Buffer[] = [];
      const stderr: Buffer[] = [];
      let settled = false;
      let termination: Error | undefined;
      let killTimer: ReturnType<typeof setTimeout> | undefined;

      const onStdout = (chunk: Buffer) => { stdout.push(chunk); };
      const onStderr = (chunk: Buffer) => { stderr.push(chunk); };
      const onClose = (code: number | null) => {
        if (termination !== undefined) {
          const error = termination;
          finish(() => reject(error));
          return;
        }
        finish(() => resolve({
          stdout: Buffer.concat(stdout).toString("utf8"),
          stderr: Buffer.concat(stderr).toString("utf8"),
          code: code ?? 2,
        }));
      };
      // Error listeners stay attached after settling (guarded by `settled`),
      // so a late EPIPE or kill failure can never become an unhandled event.
      const finish = (settle: () => void) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        clearTimeout(killTimer);
        options.signal?.removeEventListener("abort", abort);
        child.stdout.off("data", onStdout);
        child.stderr.off("data", onStderr);
        child.off("close", onClose);
        settle();
      };
      const fail = (error: unknown) => finish(() => reject(error));
      // Cancellation settles only once the child has closed, so a retry can
      // never overlap a mutation that is still running. SIGTERM first, SIGKILL
      // after the grace period; a grandchild holding the pipes open after
      // SIGKILL cannot keep the call pending forever.
      const terminate = (error: Error) => {
        if (settled || termination !== undefined) return;
        termination = error;
        clearTimeout(timer);
        options.signal?.removeEventListener("abort", abort);
        child.kill("SIGTERM");
        killTimer = setTimeout(() => {
          child.kill("SIGKILL");
          killTimer = setTimeout(() => {
            child.stdout.destroy();
            child.stderr.destroy();
            child.stdin.destroy();
            fail(error);
          }, KILL_GRACE_MS);
        }, KILL_GRACE_MS);
      };
      const abort = () => terminate(new HyaloAbortError());
      const timer = setTimeout(() => {
        terminate(new HyaloTimeoutError(options.timeoutMs));
      }, options.timeoutMs);

      options.signal?.addEventListener("abort", abort, { once: true });
      child.stdout.on("data", onStdout);
      child.stderr.on("data", onStderr);
      child.stdin.on("error", (error: NodeJS.ErrnoException) => {
        // A command may reject argv and close stdin before a large input has
        // finished writing. Its exit status/stderr is the useful result;
        // Unix reports EPIPE and Windows reports EOF for the same closed peer.
        if (!isClosedStdinWriteError(error) && termination === undefined) {
          fail(new HyaloSpawnError(error));
        }
      });
      child.on("error", (error) => {
        if (termination === undefined) fail(new HyaloSpawnError(error));
      });
      child.on("close", onClose);
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
    // Brand check, not `instanceof`: a transport built against another copy of
    // this module (ESM, CommonJS or the Pi bundle) throws that copy's classes.
    if (isHyaloBranded(error, PASSTHROUGH_ERRORS)) throw error;
    throw new HyaloSpawnError(error);
  }
}

/** @internal Parse the trailing JSON error envelope of a failed call. */
export function parseErrorEnvelope(result: ProcessResult): ErrorEnvelope | undefined {
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

/** @internal Report only a successfully interpreted typed call, never failed/raw streams. */
export async function reportDiagnostics(result: ProcessResult, options: ExecutionOptions): Promise<void> {
  if (!result.stderr) return;
  if (options.onDiagnostics) await options.onDiagnostics(result.stderr);
  else process.stderr.write(result.stderr);
}

async function jsonCall<T>(argv: string[], options: Record<string, unknown>): Promise<Envelope<T>> {
  assertNoOutputTransforms(options);
  const terminator = argv.indexOf("--");
  argv.splice(terminator === -1 ? argv.length : terminator, 0, "--format=json", "--no-hints");
  const execution = executionOptions(options);
  const result = await execute(argv, execution);
  const envelope = parseEnvelope<T>(result);
  await reportDiagnostics(result, execution);
  return envelope;
}

/** @internal JSON mutation accessor for adapters; not exported by the package barrel.
 * Public set/task keep their successful ProcessResult streams. This accessor
 * executes exactly once and retains structured effects on HyaloError.
 */
export async function mutationReport<T>(argv: readonly string[], options: ExecutionOptions = {}): Promise<MutationReportEnvelope<T>> {
  const args = [...argv];
  const terminator = args.indexOf("--");
  args.splice(terminator === -1 ? args.length : terminator, 0, "--internal-mutation-report");
  let envelope: Envelope<T>;
  try {
    envelope = await jsonCall<T>(args, options as unknown as Record<string, unknown>);
  } catch (error) {
    if (
      isHyaloBranded(error, new Set(["HyaloError"])) &&
      (error as HyaloError).exitCode === 2 &&
      /unexpected argument '--internal-mutation-report'/.test((error as HyaloError).stderr)
    ) {
      const failure = error as HyaloError;
      throw new HyaloError(
        { code: failure.exitCode, stdout: failure.stdout, stderr: failure.stderr },
        undefined,
        "hyalo is too old for typed mutations: hyalo_set and hyalo_task need hyalo >= 0.24.0 " +
          "(the installed binary rejects --internal-mutation-report); upgrade hyalo",
      );
    }
    throw error;
  }
  if (!("effects" in envelope)) throw new HyaloParseError("hyalo returned no internal mutation report", { code: 0, stdout: JSON.stringify(envelope), stderr: "" });
  return envelope as MutationReportEnvelope<T>;
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

/** `hyalo terms [PREFIX]`: BM25 dictionary terms with their document frequency. */
export function terms(options: TermsCallOptions = {}): Promise<Envelope<TermsResult>> {
  const values = options as unknown as Record<string, unknown>;
  const argv = ["terms"];
  addFlag(argv, "--glob", values.glob);
  addFlag(argv, "--limit", values.limit);
  addFlag(argv, "--index", values.index);
  addGlobals(argv, values);
  if (values.prefix !== undefined && values.prefix !== null) argv.push("--", String(values.prefix));
  return jsonCall<TermsResult>(argv, values);
}

/** `hyalo tags summary`: unique frontmatter tags with file counts. */
export function tags(options: TagsCallOptions = {}): Promise<Envelope<TagsResult>> {
  const values = options as unknown as Record<string, unknown>;
  const argv = ["tags", "summary"];
  addFlag(argv, "--glob", values.glob);
  addFlag(argv, "--limit", values.limit);
  addFlag(argv, "--index", values.index);
  addGlobals(argv, values);
  return jsonCall<TagsResult>(argv, values);
}

/** `hyalo backlinks`: every authored link that points at one file. */
export function backlinks(options: BacklinksCallOptions = {}): Promise<Envelope<BacklinksResult>> {
  const values = options as unknown as Record<string, unknown>;
  const argv = ["backlinks"];
  addFlag(argv, "--file", values.file);
  addFlag(argv, "--glob", values.glob);
  addFlag(argv, "--files-from", values.files_from);
  addFlag(argv, "--limit", values.limit);
  addFlag(argv, "--index", values.index);
  addGlobals(argv, values);
  if (values.file_positional !== undefined && values.file_positional !== null) {
    argv.push("--", String(values.file_positional));
  }
  return jsonCall<BacklinksResult>(argv, values);
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
  await reportDiagnostics(result, options);
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
  await reportDiagnostics(result, options);
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
  // Findings always reach stdout, so exit 1 with empty stdout is a refusal
  // (a missing file, a malformed .hyalo.toml), never a lint with no findings.
  if ((result.code !== 0 && result.code !== 1) || (result.code === 1 && !result.stdout.trim())) {
    throw new HyaloError(result, parseErrorEnvelope(result));
  }
  if (result.code === 0) await reportDiagnostics(result, options);
  return result;
}

/** Run an arbitrary command without forcing JSON parsing or interpreting exit codes. */
export function raw(argv: readonly string[], options: ExecutionOptions = {}): Promise<ProcessResult> {
  return execute(argv, options);
}
