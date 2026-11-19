import { type ChildProcess, execFile, spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { setTimeout as sleep } from 'node:timers/promises';
import { CaptureError } from './error.ts';

/** WebDriver's element reference key, from the W3C WebDriver specification. */
export const ELEMENT_KEY = 'element-6066-11e4-a52e-4f735466cecf';

/** Where macOS installs `safaridriver`. */
export const SAFARIDRIVER_PATH = '/usr/bin/safaridriver';

/** How long `safaridriver` may take to answer `/status` after it starts. */
const STARTUP_TIMEOUT_MS = 10_000;

/** How long deleting the session and stopping the driver may each take during cleanup. */
const CLEANUP_TIMEOUT_MS = 10_000;

/** The command that starts a WebDriver server; `--port <n>` is appended to `args`. */
export interface DriverCommand {
  command: string;
  args: readonly string[];
}

/** A WebDriver error response: the error code and message the browser returned. */
export class WebDriverError extends Error {
  override readonly name = 'WebDriverError';
  readonly status: number;
  readonly error: string;
  readonly detail: string;

  constructor(status: number, error: string, detail: string) {
    super(detail === '' ? error : `${error}: ${detail}`);
    this.status = status;
    this.error = error;
    this.detail = detail;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** The text of a value the driver sent, which is untrusted and may not be a string. */
function text(value: unknown): string {
  return typeof value === 'string' ? value : (JSON.stringify(value) ?? '');
}

async function freePort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  await new Promise<void>((resolve) => server.close(() => resolve()));
  if (address === null || typeof address === 'string') {
    throw new Error('the OS gave no port for the driver');
  }
  return address.port;
}

/** The output of `command --version`, trimmed. */
export function driverVersion(driver: DriverCommand, timeoutMs: number): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(
      driver.command,
      [...driver.args, '--version'],
      { timeout: timeoutMs, encoding: 'utf8' },
      (error, stdout) => {
        if (error !== null) {
          reject(
            'code' in error && error.code === 'ENOENT'
              ? new CaptureError(
                  'safaridriver-not-found',
                  `safaridriver is not installed at ${driver.command}`,
                  {
                    cause: error,
                  },
                )
              : error,
          );
          return;
        }
        resolve(stdout.trim());
      },
    );
  });
}

/**
 * A running WebDriver server: `safaridriver`, or the test double, on a free loopback port.
 *
 * The process is started in its own process group, so a Ctrl-C in the terminal reaches only the
 * capturing process, which deletes the session before it stops the driver. An `exit` handler
 * kills the driver if the capturing process ends without stopping it.
 */
export class Driver {
  readonly base: string;
  readonly #child: ChildProcess;
  readonly #exited: Promise<void>;
  readonly #killOnExit: () => void;

  private constructor(base: string, child: ChildProcess) {
    this.base = base;
    this.#child = child;
    this.#exited = new Promise((resolve) => {
      if (child.exitCode !== null || child.signalCode !== null) {
        resolve();
        return;
      }
      child.once('exit', () => resolve());
      // A process that never started has no pid and may never emit `exit`.
      child.once('error', () => {
        if (child.pid === undefined) {
          resolve();
        }
      });
    });
    this.#killOnExit = () => {
      child.kill('SIGKILL');
    };
    process.once('exit', this.#killOnExit);
  }

  /**
   * Starts the driver and waits until it answers `/status`.
   *
   * @throws {CaptureError} `safaridriver-not-found` when the command does not exist, or
   *   `safari-session-refused` when the driver exits or stays silent before it answers.
   */
  static async start(driver: DriverCommand, signal: AbortSignal): Promise<Driver> {
    const port = await freePort();
    const child = spawn(driver.command, [...driver.args, '--port', String(port)], {
      stdio: ['ignore', 'ignore', 'pipe'],
      detached: true,
    });
    let stderr = '';
    child.stderr?.setEncoding('utf8');
    child.stderr?.on('data', (chunk: string) => {
      // Only the start of the output is kept for the message.
      if (stderr.length < 4_096) {
        stderr += chunk;
      }
    });
    let spawnError: Error | undefined;
    child.once('error', (error) => {
      spawnError = error;
    });
    const instance = new Driver(`http://127.0.0.1:${port}`, child);
    const deadline = Date.now() + STARTUP_TIMEOUT_MS;
    try {
      while (Date.now() < deadline) {
        await sleep(0);
        if (spawnError !== undefined) {
          throw 'code' in spawnError && spawnError.code === 'ENOENT'
            ? new CaptureError(
                'safaridriver-not-found',
                `safaridriver is not installed at ${driver.command}`,
                {
                  cause: spawnError,
                },
              )
            : spawnError;
        }
        if (child.exitCode !== null || child.signalCode !== null) {
          throw new CaptureError(
            'safari-session-refused',
            `safaridriver exited before it answered: ${JSON.stringify(stderr.trim() || 'no output')}; ` +
              'see "Enabling Safari automation" in docs/usage.md',
          );
        }
        signal.throwIfAborted();
        try {
          const response = await fetch(`${instance.base}/status`, {
            signal: AbortSignal.any([signal, AbortSignal.timeout(1_000)]),
          });
          await response.body?.cancel();
          if (response.ok) {
            return instance;
          }
        } catch (error) {
          if (signal.aborted) {
            throw error;
          }
          // Not listening yet; the loop retries until the deadline.
        }
        await sleep(100, undefined, { signal });
      }
      throw new CaptureError(
        'safari-session-refused',
        `safaridriver did not answer within ${STARTUP_TIMEOUT_MS} ms: ${JSON.stringify(stderr.trim() || 'no output')}`,
      );
    } catch (error) {
      await instance.stop();
      throw error;
    }
  }

  /**
   * Sends one command and returns its `value`. Every request has a deadline: a command that gets
   * no answer within `timeoutMs` fails with `timeout`, naming `what`.
   *
   * @throws {WebDriverError} for a WebDriver error response.
   * @throws {CaptureError} `timeout` when no answer comes in time.
   */
  async command(
    what: string,
    method: 'GET' | 'POST' | 'DELETE',
    path: string,
    body: unknown,
    timeoutMs: number,
    signal: AbortSignal,
  ): Promise<unknown> {
    const deadline = AbortSignal.timeout(timeoutMs);
    let response: Response;
    let json: unknown;
    try {
      response = await fetch(`${this.base}${path}`, {
        method,
        headers: { 'content-type': 'application/json' },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
        signal: AbortSignal.any([signal, deadline]),
      });
      json = await response.json();
    } catch (error) {
      if (deadline.aborted && !signal.aborted) {
        throw new CaptureError('timeout', `${what} did not answer within ${timeoutMs} ms`, { cause: error });
      }
      throw error;
    }
    const value = isRecord(json) ? json.value : undefined;
    if (!response.ok) {
      const error = isRecord(value) ? value : {};
      throw new WebDriverError(
        response.status,
        text(error.error ?? 'unknown error'),
        text(error.message ?? ''),
      );
    }
    return value;
  }

  /** Stops the driver: SIGTERM to its process group, then SIGKILL if it is still running. */
  async stop(): Promise<void> {
    const child = this.#child;
    if (child.exitCode === null && child.signalCode === null && child.pid !== undefined) {
      signalGroup(child.pid, 'SIGTERM');
      const stopped = await Promise.race([
        this.#exited.then(() => true),
        // Unreferenced, so a driver that stops at once does not hold the process open.
        sleep(CLEANUP_TIMEOUT_MS, false, { ref: false }),
      ]);
      if (!stopped) {
        signalGroup(child.pid, 'SIGKILL');
        await this.#exited;
      }
    }
    await this.#exited;
    process.off('exit', this.#killOnExit);
  }

  /** The time cleanup steps may each take. */
  static readonly cleanupTimeoutMs = CLEANUP_TIMEOUT_MS;
}

/** Sends `signal` to the process group `pid` leads, or to `pid` alone where groups do not apply. */
function signalGroup(pid: number, signal: NodeJS.Signals): void {
  try {
    process.kill(-pid, signal);
  } catch (error) {
    if (error instanceof Error && 'code' in error && error.code === 'ESRCH') {
      return;
    }
    process.kill(pid, signal);
  }
}

/** The element ids in a Find Elements answer. */
export function elementIds(value: unknown): string[] {
  if (!Array.isArray(value)) {
    throw new Error(`Find Elements answered with ${text(value)}, not a list of elements`);
  }
  return value.map((reference) => {
    const id = isRecord(reference) ? reference[ELEMENT_KEY] : undefined;
    if (typeof id !== 'string') {
      throw new Error(`Find Elements answered with ${text(reference)}, not an element reference`);
    }
    return id;
  });
}
