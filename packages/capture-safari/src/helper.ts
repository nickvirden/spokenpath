import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import type { SafariAxNode } from '@spokenpath/snapshot-types';
import { cargoBinaryPath } from './binary.ts';
import { CaptureError, type CaptureErrorCode } from './error.ts';
import type { DriverCommand } from './webdriver.ts';

/** The command that builds the helper, which the missing-helper message names. */
export const HELPER_BUILD_COMMAND = 'cargo build -p spokenpath_ax_macos';

/**
 * The most the helper may print on stdout: the input limit of `spokenpath-core`, so every
 * snapshot capture writes stays readable by the core.
 */
export const HELPER_OUTPUT_LIMIT = 32 * 1024 * 1024;

/** How long past its own deadline the helper may run before it is killed. */
export const HELPER_GRACE_MS = 5_000;

/** How much of the helper's stderr a failure quotes. */
const STDERR_QUOTE_LIMIT = 4_096;

/** The program that reads Safari's macOS accessibility tree; its arguments are appended to `args`. */
export type HelperCommand = DriverCommand;

/**
 * The helper's failure codes that are capture's own, or the capture code they mean. Every other
 * code the helper prints is `ax-helper-failed`.
 */
const HELPER_CODES: Readonly<Record<string, CaptureErrorCode>> = {
  'accessibility-permission-missing': 'accessibility-permission-missing',
  'safari-not-running': 'safari-not-running',
  'safari-window-not-identified': 'safari-window-not-identified',
  'ax-canary-incomplete': 'ax-canary-incomplete',
  'ax-tree-too-large': 'ax-tree-too-large',
  'screen-locked': 'page-hidden',
  'page-left-origin': 'page-left-origin',
  timeout: 'timeout',
  'unsupported-platform': 'unsupported-platform',
};

/** The exit status the helper gives a missing permission, and only that. */
const PERMISSION_EXIT = 3;

/** The built `spokenpath-ax-macos` debug binary, or `ax-helper-not-found` naming how to build it. */
export function axHelper(env: NodeJS.ProcessEnv = process.env): HelperCommand {
  const path = cargoBinaryPath('spokenpath-ax-macos', env);
  if (!existsSync(path)) {
    throw new CaptureError(
      'ax-helper-not-found',
      `the spokenpath-ax-macos helper is not built at ${path}; build it with \`${HELPER_BUILD_COMMAND}\``,
    );
  }
  return { command: path, args: [] };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** The helper's running children, which cleanup kills before the session is deleted. */
export class HelperProcesses {
  readonly #running = new Set<{ kill: () => void; exited: Promise<void> }>();

  add(child: { kill: () => void; exited: Promise<void> }): () => void {
    this.#running.add(child);
    return () => this.#running.delete(child);
  }

  /** Kills every helper still running and waits for each to exit. */
  async stop(): Promise<void> {
    const running = [...this.#running];
    for (const child of running) {
      child.kill();
    }
    await Promise.all(running.map((child) => child.exited));
  }
}

export interface HelperRun {
  helper: HelperCommand;
  /** The time limit of the helper's own work; it is killed this plus `HELPER_GRACE_MS` later. */
  timeoutMs: number;
  /** Aborts the run, killing the helper. */
  signal: AbortSignal;
  processes: HelperProcesses;
}

/**
 * Runs the helper with `args`, from an argument array with no shell, and returns the JSON value it
 * printed on success, once `accept` has checked its shape.
 *
 * @throws {CaptureError} the capture code of the helper's failure, with its message unchanged;
 *   `ax-tree-too-large` when it prints more than `HELPER_OUTPUT_LIMIT`; `timeout` when it runs
 *   past its deadline and the grace; `ax-helper-not-found` when it cannot start; and
 *   `ax-helper-failed` for any other failure or an answer of another shape.
 */
export function runHelper<T>(
  run: HelperRun,
  args: readonly string[],
  accept: (value: unknown) => T | undefined,
): Promise<T> {
  const { helper, timeoutMs, signal, processes } = run;
  return new Promise<T>((resolve, reject) => {
    if (signal.aborted) {
      reject(signal.reason);
      return;
    }
    const child = spawn(helper.command, [...helper.args, ...args], {
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const exited = new Promise<void>((done) => {
      child.once('close', () => done());
      child.once('error', () => {
        if (child.pid === undefined) {
          done();
        }
      });
    });
    const kill = () => {
      if (child.exitCode === null && child.signalCode === null) {
        child.kill('SIGKILL');
      }
    };
    const forget = processes.add({ kill, exited });
    const chunks: Buffer[] = [];
    let size = 0;
    let stderr = '';
    let failure: unknown;
    const fail = (error: unknown) => {
      if (failure === undefined) {
        failure = error;
        kill();
      }
    };
    const timer = setTimeout(
      () =>
        fail(
          new CaptureError(
            'timeout',
            `the spokenpath-ax-macos helper did not finish within ${timeoutMs + HELPER_GRACE_MS} ms, so it was killed`,
          ),
        ),
      timeoutMs + HELPER_GRACE_MS,
    );
    const onAbort = () => fail(signal.reason);
    signal.addEventListener('abort', onAbort, { once: true });
    child.stdout.on('data', (chunk: Buffer) => {
      size += chunk.length;
      if (size > HELPER_OUTPUT_LIMIT) {
        fail(
          new CaptureError(
            'ax-tree-too-large',
            `Safari's macOS accessibility tree for the page is larger than the ${HELPER_OUTPUT_LIMIT / (1024 * 1024)} MiB a capture records, so the helper was stopped`,
          ),
        );
        return;
      }
      chunks.push(chunk);
    });
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk: string) => {
      if (stderr.length < STDERR_QUOTE_LIMIT) {
        stderr = (stderr + chunk).slice(0, STDERR_QUOTE_LIMIT);
      }
    });
    child.once('error', (error) => {
      fail(
        'code' in error && error.code === 'ENOENT'
          ? new CaptureError(
              'ax-helper-not-found',
              `the spokenpath-ax-macos helper is not installed at ${helper.command}; build it with \`${HELPER_BUILD_COMMAND}\``,
              { cause: error },
            )
          : error,
      );
    });
    child.once('close', (code) => {
      clearTimeout(timer);
      signal.removeEventListener('abort', onAbort);
      forget();
      if (failure !== undefined) {
        reject(failure);
        return;
      }
      try {
        resolve(answer(Buffer.concat(chunks).toString('utf8'), code, stderr, accept));
      } catch (error) {
        reject(error);
      }
    });
  });
}

/** The helper's answer, or the failure it states, from its stdout and exit status. */
function answer<T>(
  stdout: string,
  code: number | null,
  stderr: string,
  accept: (value: unknown) => T | undefined,
): T {
  const unreadable = () =>
    new CaptureError(
      'ax-helper-failed',
      `the spokenpath-ax-macos helper exited with ${code === null ? 'a signal' : `status ${code}`} and printed no answer capture reads; its stderr begins ${JSON.stringify(stderr.trim())}`,
    );
  let value: unknown;
  try {
    value = JSON.parse(stdout);
  } catch {
    throw unreadable();
  }
  if (code === 0) {
    const accepted = accept(value);
    if (accepted === undefined) {
      throw unreadable();
    }
    return accepted;
  }
  const error = isRecord(value) && isRecord(value.error) ? value.error : undefined;
  if (error === undefined || typeof error.code !== 'string' || typeof error.message !== 'string') {
    throw unreadable();
  }
  const mapped = HELPER_CODES[error.code];
  // The permission is the only failure with its own exit status, so the two must agree.
  if (
    mapped === undefined ||
    (code === PERMISSION_EXIT) !== (mapped === 'accessibility-permission-missing')
  ) {
    throw new CaptureError(
      'ax-helper-failed',
      `the spokenpath-ax-macos helper failed with ${JSON.stringify(error.code)}, exit status ${String(code)}: ${error.message}`,
    );
  }
  throw new CaptureError(mapped, error.message);
}

/** The check page's HTML, from `spokenpath-ax-macos canary-page`. */
export function canaryPage(run: HelperRun): Promise<string> {
  return runHelper(run, ['canary-page'], (value) =>
    isRecord(value) && typeof value.html === 'string' ? value.html : undefined,
  );
}

/** What capture tells the helper about the automation window and its page. */
export interface ReadTarget {
  url: string;
  title: string;
  rect: { x: number; y: number; width: number; height: number };
}

function readArgs(target: ReadTarget, timeoutMs: number): string[] {
  const { x, y, width, height } = target.rect;
  return [
    'read',
    '--url',
    target.url,
    '--title',
    target.title,
    '--window-rect',
    [x, y, width, height].map((value) => String(Math.round(value))).join(','),
    '--timeout-ms',
    String(Math.ceil(timeoutMs)),
  ];
}

/** Reads the check page's tree, which succeeds only when every check node is there. */
export async function readCanary(run: HelperRun, target: ReadTarget): Promise<void> {
  await runHelper(run, [...readArgs(target, run.timeoutMs), '--canary'], (value) => {
    const canary = isRecord(value) && isRecord(value.canary) ? value.canary : undefined;
    return canary !== undefined &&
      typeof canary.expected === 'number' &&
      canary.expected > 0 &&
      canary.found === canary.expected
      ? true
      : undefined;
  });
}

/**
 * Reads the page's tree: the nodes, as the helper printed them, once each is an object with an
 * `attributes` object. The snapshot schema checks the rest.
 */
export function readTree(run: HelperRun, target: ReadTarget): Promise<SafariAxNode[]> {
  return runHelper(run, readArgs(target, run.timeoutMs), (value) => {
    const nodes = isRecord(value) ? value.nodes : undefined;
    return Array.isArray(nodes) &&
      nodes.length > 0 &&
      nodes.every((node) => isRecord(node) && isRecord(node.attributes))
      ? (nodes as SafariAxNode[])
      : undefined;
  });
}
