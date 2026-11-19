import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { basename, dirname, resolve } from 'node:path';
import { sortDiagnostics } from '@spokenpath/capture-chromium';
import { instrumentHtml, SOURCE_ATTRIBUTE } from '@spokenpath/instrument-html';
import {
  type Capture,
  type Diagnostic,
  type SafariElement,
  type Snapshot,
  validateSnapshot,
  type WebdriverError,
} from '@spokenpath/snapshot-types';
import { CaptureError, type InterruptSignal } from './error.ts';
import {
  MEASURE_SCRIPT,
  measurement,
  ORIGIN_SCRIPT,
  SELECTOR_SCRIPT,
  SETTLE_SCRIPT,
  VISIBILITY_SCRIPT,
} from './scripts.ts';
import { type PageServer, servePage } from './serve.ts';
import { type HostVersions, producerVersion, readHostVersions } from './versions.ts';
import {
  Driver,
  type DriverCommand,
  driverVersion,
  elementIds,
  SAFARIDRIVER_PATH,
  WebDriverError,
} from './webdriver.ts';

/** How long loading, waiting for the selector, settling and each call into Safari may take, by default. */
export const DEFAULT_TIMEOUT_MS = 30_000;

/** The viewport capture asks for, as Chromium capture fixes it; the screen may not allow it. */
export const VIEWPORT = { width: 1280, height: 720 } as const;

/** How often the `waitFor` selector is checked. */
const SELECTOR_POLL_MS = 100;

/**
 * The WebDriver errors that mean the session or its window is gone, such as when the automation
 * window is closed during capture; every later call would fail too, so they fail the capture.
 */
const SESSION_LOST: ReadonlySet<string> = new Set(['invalid session id', 'no such window']);

/**
 * A WebDriver server to run instead of `/usr/bin/safaridriver`, with the host versions to record.
 * Tests use it to run capture against the test-double driver on any platform.
 */
export interface TestDriver extends DriverCommand {
  versions: HostVersions;
}

export interface CaptureOptions {
  /** A CSS selector to wait for, in the DOM, after the page has loaded. */
  waitFor?: string;
  /**
   * How long loading, waiting for the selector, settling and each later call into Safari may
   * take, in milliseconds: a positive finite number, 30 000 by default.
   */
  timeoutMs?: number;
  /** For tests only: the driver to run instead of Safari's. */
  driver?: TestDriver;
}

interface PageFile {
  root: string;
  fileName: string;
  bytes: Uint8Array;
}

function timeoutOf(options: CaptureOptions): number {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) {
    throw new CaptureError(
      'invalid-options',
      `timeoutMs must be a positive number of milliseconds, not ${String(timeoutMs)}`,
    );
  }
  return timeoutMs;
}

/** `/usr/bin/safaridriver`, on macOS where it is installed. */
function safaridriver(): DriverCommand {
  if (process.platform !== 'darwin') {
    throw new CaptureError('unsupported-platform', 'Safari capture needs macOS with Safari and safaridriver');
  }
  if (!existsSync(SAFARIDRIVER_PATH)) {
    throw new CaptureError('safaridriver-not-found', `safaridriver is not installed at ${SAFARIDRIVER_PATH}`);
  }
  return { command: SAFARIDRIVER_PATH, args: [] };
}

/**
 * Reads the page file, naming it by its base name and the reason by its code only, as Chromium
 * capture does, so the error carries no absolute path.
 */
async function readPage(pagePath: string): Promise<PageFile> {
  const absolute = resolve(pagePath);
  const fileName = basename(absolute);
  try {
    return { root: dirname(absolute), fileName, bytes: await readFile(absolute) };
  } catch (error) {
    const reason =
      error instanceof Error && 'code' in error && typeof error.code === 'string'
        ? error.code
        : 'an unknown error';
    throw new CaptureError('page-unreadable', `cannot read the page ${JSON.stringify(fileName)}: ${reason}`, {
      cause: error,
    });
  }
}

/**
 * Catches SIGINT and SIGTERM for the length of a capture, so the session is deleted and the
 * driver stopped before the process ends; the first signal aborts every request in flight.
 */
class Interrupt {
  readonly #controller = new AbortController();
  #received: InterruptSignal | undefined;
  readonly #onSigint = () => this.#receive('SIGINT');
  readonly #onSigterm = () => this.#receive('SIGTERM');

  get signal(): AbortSignal {
    return this.#controller.signal;
  }

  get received(): InterruptSignal | undefined {
    return this.#received;
  }

  listen(): void {
    process.on('SIGINT', this.#onSigint);
    process.on('SIGTERM', this.#onSigterm);
  }

  unlisten(): void {
    process.off('SIGINT', this.#onSigint);
    process.off('SIGTERM', this.#onSigterm);
  }

  #receive(signal: InterruptSignal): void {
    if (this.#received === undefined) {
      this.#received = signal;
      this.#controller.abort(new Error(`interrupted by ${signal}`));
    }
  }
}

/** A WebDriver session on a running driver, with every call bounded by the capture's time limit. */
class Session {
  readonly #driver: Driver;
  readonly #id: string;
  readonly #timeoutMs: number;
  readonly #signal: AbortSignal;

  constructor(driver: Driver, id: string, timeoutMs: number, signal: AbortSignal) {
    this.#driver = driver;
    this.#id = id;
    this.#timeoutMs = timeoutMs;
    this.#signal = signal;
  }

  /**
   * One command on the session that capture cannot do without: any WebDriver error fails the
   * capture, naming the step.
   */
  async call(what: string, method: 'GET' | 'POST', path: string, body?: unknown): Promise<unknown> {
    try {
      return await this.ask(what, method, path, body);
    } catch (error) {
      if (error instanceof WebDriverError) {
        throw callFailed(what, error);
      }
      throw error;
    }
  }

  /**
   * One command whose WebDriver error capture records rather than fails on, such as an element's
   * role, which throws that `WebDriverError`. A WebDriver `timeout` or `script timeout` error,
   * which Safari returns when its own limit (set to the same `timeoutMs`) runs out first, fails the
   * capture with `timeout`, as a command that gets no answer does; an error that means the session
   * is lost fails it with `safari-call-failed`.
   */
  async ask(what: string, method: 'GET' | 'POST', path: string, body?: unknown): Promise<unknown> {
    try {
      return await this.#driver.command(
        what,
        method,
        `/session/${this.#id}${path}`,
        body,
        this.#timeoutMs,
        this.#signal,
      );
    } catch (error) {
      if (
        error instanceof WebDriverError &&
        (error.error === 'timeout' || error.error === 'script timeout')
      ) {
        throw new CaptureError('timeout', `${what} did not finish within ${this.#timeoutMs} ms`, {
          cause: error,
        });
      }
      if (error instanceof WebDriverError && SESSION_LOST.has(error.error)) {
        throw callFailed(what, error);
      }
      throw error;
    }
  }

  delete(): Promise<unknown> {
    return this.#driver.command(
      'deleting the session',
      'DELETE',
      `/session/${this.#id}`,
      undefined,
      Driver.cleanupTimeoutMs,
      AbortSignal.timeout(Driver.cleanupTimeoutMs),
    );
  }
}

/** The capture failure for a call into Safari that answered with an error. */
function callFailed(what: string, error: WebDriverError): CaptureError {
  const detail = error.detail === '' ? '' : `: ${JSON.stringify(error.detail)}`;
  return new CaptureError(
    'safari-call-failed',
    `${what}: Safari answered with error ${JSON.stringify(error.error)}${detail}`,
    { cause: error },
  );
}

/**
 * Fails the capture when the page is no longer on the origin capture served it on: a page can
 * navigate itself away, which the Content-Security-Policy does not stop, and the elements read
 * would then be another page's.
 */
function requireServedOrigin(origin: unknown, servedOrigin: string, before: string): void {
  if (origin !== servedOrigin) {
    throw new CaptureError(
      'page-left-origin',
      `the page left the origin it was served on, ${JSON.stringify(servedOrigin)}, before ${before}: ` +
        `Safari reports its origin as ${JSON.stringify(origin)}; capture detects such a navigation ` +
        'and does not prevent it, so the page may already have made requests to other origins',
    );
  }
}

interface StartedSession {
  id: string;
  browserVersion: string;
}

/** Starts the session, or fails with `safari-session-refused` quoting what the driver answered. */
async function startSession(driver: Driver, timeoutMs: number, signal: AbortSignal): Promise<StartedSession> {
  let value: unknown;
  try {
    value = await driver.command(
      'starting the Safari session',
      'POST',
      '/session',
      { capabilities: { alwaysMatch: { browserName: 'safari' } } },
      timeoutMs,
      signal,
    );
  } catch (error) {
    if (error instanceof WebDriverError) {
      throw new CaptureError(
        'safari-session-refused',
        `safaridriver refused the session with ${JSON.stringify(error.error)}: ${JSON.stringify(error.detail)}; ` +
          'see "Enabling Safari automation" in docs/usage.md',
        { cause: error },
      );
    }
    throw error;
  }
  const record = typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {};
  const capabilities =
    typeof record.capabilities === 'object' && record.capabilities !== null
      ? (record.capabilities as Record<string, unknown>)
      : {};
  const { sessionId } = record;
  const { browserVersion } = capabilities;
  if (typeof sessionId !== 'string' || typeof browserVersion !== 'string') {
    throw new CaptureError(
      'safari-session-refused',
      `safaridriver started a session without a session id or a browser version: ${JSON.stringify(value)}`,
    );
  }
  return { id: sessionId, browserVersion };
}

/**
 * Sizes the window so the viewport is 1280 × 720 where the screen allows: it sets the window to
 * that size, measures the difference between the window and the viewport, and sets it again with
 * the difference added. A window the screen cannot hold keeps the size it gets, and the snapshot
 * records the viewport as measured.
 */
async function sizeWindow(session: Session): Promise<void> {
  const setRect = async (width: number, height: number) => {
    try {
      await session.ask('sizing the window', 'POST', '/window/rect', { width, height });
    } catch (error) {
      // Safari refuses a size its screen cannot hold; the measured viewport is what is recorded.
      if (!(error instanceof WebDriverError)) {
        throw error;
      }
    }
  };
  await setRect(VIEWPORT.width, VIEWPORT.height);
  const measured = measurement(
    await session.call('measuring the window', 'POST', '/execute/sync', {
      script: MEASURE_SCRIPT,
      args: [],
    }),
  );
  if (measured === undefined) {
    throw new Error('measuring the window returned an unexpected value');
  }
  const width = VIEWPORT.width + (measured.outerWidth - measured.innerWidth);
  const height = VIEWPORT.height + (measured.outerHeight - measured.innerHeight);
  if (measured.innerWidth !== VIEWPORT.width || measured.innerHeight !== VIEWPORT.height) {
    await setRect(width, height);
  }
}

/**
 * Waits for the page to settle with the same strategy as Chromium capture: the load, which
 * navigation waits for; then the selector, when one is given; then `document.fonts.ready` and two
 * animation frames.
 */
async function settle(
  session: Session,
  waitFor: string | undefined,
  timeoutMs: number,
): Promise<'load-fonts-frames' | 'load-selector-fonts-frames'> {
  if (waitFor !== undefined) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const found = await session.call('checking the selector', 'POST', '/execute/sync', {
        script: SELECTOR_SCRIPT,
        args: [waitFor],
      });
      if (found === true) {
        break;
      }
      if (Date.now() >= deadline) {
        throw new CaptureError(
          'timeout',
          `no element matched the selector ${JSON.stringify(waitFor)} within ${timeoutMs} ms`,
        );
      }
      await new Promise((done) => setTimeout(done, SELECTOR_POLL_MS));
    }
  }
  // A hidden page runs no animation frames, so waiting for two would only end in a timeout.
  const visibility = await session.call("reading the page's visibility", 'POST', '/execute/sync', {
    script: VISIBILITY_SCRIPT,
    args: [],
  });
  if (visibility !== 'visible') {
    throw new CaptureError(
      'page-hidden',
      `Safari reports the page as ${JSON.stringify(visibility)}, so its animation frames do not run and it cannot settle; ` +
        'keep the Mac awake and unlocked while Safari capture runs (see "Enabling Safari automation" in docs/usage.md)',
    );
  }
  await session.call("the page's fonts and animation frames", 'POST', '/execute/async', {
    script: SETTLE_SCRIPT,
    args: [],
  });
  return waitFor === undefined ? 'load-fonts-frames' : 'load-selector-fonts-frames';
}

/**
 * The conditions the page was captured under, as measured in the page after settling, which
 * fails the capture when the page has left the origin it was served on.
 */
async function conditions(
  session: Session,
  servedOrigin: string,
): Promise<Pick<Capture, 'viewport' | 'color_scheme' | 'reduced_motion'>> {
  const measured = measurement(
    await session.call('measuring the viewport', 'POST', '/execute/sync', {
      script: MEASURE_SCRIPT,
      args: [],
    }),
  );
  if (measured === undefined) {
    throw new Error('measuring the viewport returned an unexpected value');
  }
  requireServedOrigin(measured.origin, servedOrigin, 'it was measured');
  return {
    viewport: {
      width: Math.max(0, Math.round(measured.innerWidth)),
      height: Math.max(0, Math.round(measured.innerHeight)),
      device_pixel_ratio: measured.devicePixelRatio,
    },
    color_scheme: measured.dark ? 'dark' : measured.light ? 'light' : 'no-preference',
    reduced_motion: measured.reducedMotion ? 'reduce' : 'no-preference',
  };
}

/** The answer to one role or label call: the string, or the error Safari answered with. */
async function answer(session: Session, what: string, path: string): Promise<string | WebdriverError> {
  try {
    const value = await session.ask(what, 'GET', path);
    if (typeof value !== 'string') {
      throw new Error(`${what} answered with ${JSON.stringify(value)}, not a string`);
    }
    return value;
  } catch (error) {
    if (error instanceof WebDriverError) {
      return { error: error.error, message: error.detail };
    }
    throw error;
  }
}

/**
 * Asks Safari for the computed role and label of every element carrying the source attribute, in
 * document order, one call after another, once the page is confirmed to be on the origin it was
 * served on. Elements inside shadow roots are not found, because Find Elements does not search
 * them.
 */
async function readElements(
  session: Session,
  servedOrigin: string,
  spanIds: ReadonlySet<number>,
  diagnostics: Diagnostic[],
): Promise<SafariElement[]> {
  requireServedOrigin(
    await session.call("reading the page's origin", 'POST', '/execute/sync', {
      script: ORIGIN_SCRIPT,
      args: [],
    }),
    servedOrigin,
    'its elements were read',
  );
  const ids = elementIds(
    await session.call('finding the tagged elements', 'POST', '/elements', {
      using: 'css selector',
      value: `[${SOURCE_ATTRIBUTE}]`,
    }),
  );
  const elements: SafariElement[] = [];
  for (const id of ids) {
    const path = `/element/${id}`;
    let value: unknown;
    try {
      value = await session.ask(
        "reading an element's source attribute",
        'GET',
        `${path}/attribute/${SOURCE_ATTRIBUTE}`,
      );
    } catch (error) {
      if (!(error instanceof WebDriverError)) {
        throw error;
      }
      diagnostics.push({
        code: 'source-attribute-unreadable',
        message: `Safari answered Get Element Attribute with error ${JSON.stringify(error.error)}, so the element has no entry.`,
      });
      continue;
    }
    const span = typeof value === 'string' && /^(0|[1-9][0-9]{0,9})$/.test(value) ? Number(value) : undefined;
    if (span === undefined || !spanIds.has(span)) {
      diagnostics.push({
        code: 'unknown-source-attribute',
        message: `An element's ${SOURCE_ATTRIBUTE} value ${JSON.stringify(value)} names no span, so the element has no entry.`,
      });
      continue;
    }
    const role = await answer(session, 'Get Computed Role', `${path}/computedrole`);
    const label = await answer(session, 'Get Computed Label', `${path}/computedlabel`);
    elements.push({
      span,
      ...(typeof role === 'string' ? { role } : { role_error: role }),
      ...(typeof label === 'string' ? { label } : { label_error: label }),
    });
  }
  return elements;
}

/**
 * Captures what Safari answers over WebDriver for the HTML page at `pagePath`: the page is
 * instrumented with its source locations exactly as Chromium capture instruments it, served on
 * the loopback address with a Content-Security-Policy that limits its sub-resource loads to that
 * origin, loaded in one
 * `safaridriver` session and settled, and every tagged element's computed role and label are
 * read. Capture records Safari's answers as delivered, errors included, and never corrects them.
 *
 * While it runs, capture handles SIGINT and SIGTERM itself: the first deletes the session, stops
 * `safaridriver` and closes the server, and the capture then fails with `interrupted`, naming the
 * signal. The session is deleted, the driver stopped and the server closed on every exit path.
 *
 * @throws {InstrumentHtmlError} when the instrumenter refuses the page.
 * @throws {CaptureError} when the options are unusable, the platform or the driver is missing,
 *   Safari refuses the session, the page cannot be read, a call gets no answer in time, a call
 *   capture cannot do without answers with an error or the session is lost, the page navigates
 *   away from the origin it was served on, the capture is interrupted, or the snapshot fails the
 *   schema.
 */
export async function captureSafari(pagePath: string, options: CaptureOptions = {}): Promise<Snapshot> {
  const timeoutMs = timeoutOf(options);
  const driverCommand = options.driver ?? safaridriver();
  const { root, fileName, bytes } = await readPage(pagePath);
  const { html, spans } = instrumentHtml(bytes, { source: 0 });
  const spanIds = new Set(spans.map((span) => span.id));
  const diagnostics: Diagnostic[] = [];

  const interrupt = new Interrupt();
  interrupt.listen();
  let server: PageServer | undefined;
  let driver: Driver | undefined;
  let session: Session | undefined;

  const run = async (): Promise<Snapshot> => {
    const toolVersion = await driverVersion(driverCommand, timeoutMs);
    let versions: HostVersions;
    if (options.driver === undefined) {
      const read = await readHostVersions();
      versions = read.versions;
      for (const problem of read.unread) {
        diagnostics.push({ code: 'version-unreadable', message: `A version was not recorded: ${problem}.` });
      }
    } else {
      versions = options.driver.versions;
    }
    server = await servePage({ root, fileName, body: html, diagnostics });
    driver = await Driver.start(driverCommand, interrupt.signal);
    const started = await startSession(driver, timeoutMs, interrupt.signal);
    session = new Session(driver, started.id, timeoutMs, interrupt.signal);
    await session.call('setting the time limits', 'POST', '/timeouts', {
      implicit: 0,
      pageLoad: timeoutMs,
      script: timeoutMs,
    });
    await sizeWindow(session);
    await session.call('loading the page', 'POST', '/url', { url: server.url });
    server.throwIfServingFailed();
    const settleName = await settle(session, options.waitFor, timeoutMs);
    const capturedAt = new Date().toISOString();
    const servedOrigin = new URL(server.url).origin;
    const measured = await conditions(session, servedOrigin);
    const elements = await readElements(session, servedOrigin, spanIds, diagnostics);
    server.throwIfServingFailed();

    const snapshot: Snapshot = {
      schema: 'spokenpath-snapshot',
      version: { major: 0, minor: 1 },
      producer: { name: '@spokenpath/capture-safari', version: producerVersion },
      capture: {
        engine: 'safari-webdriver',
        engine_version: started.browserVersion,
        tool_version: `safaridriver ${toolVersion}`,
        ...measured,
        url: server.url,
        settle: settleName,
      },
      volatile: { captured_at: capturedAt },
      sources: [
        {
          id: 0,
          path: fileName,
          sha256: createHash('sha256').update(bytes).digest('hex'),
          encoding: 'utf-8',
        },
      ],
      spans,
      diagnostics: sortDiagnostics(diagnostics),
      payload: {
        engine: 'safari-webdriver',
        elements,
        ...(versions.safariBuild === undefined ? {} : { safari_build: versions.safariBuild }),
        ...(versions.macosVersion === undefined ? {} : { macos_version: versions.macosVersion }),
        ...(versions.macosBuild === undefined ? {} : { macos_build: versions.macosBuild }),
      },
    };
    const result = validateSnapshot(snapshot);
    if (!result.ok) {
      const issues = result.errors.map((issue) => `${issue.path || '/'}: ${issue.message}`).join('; ');
      throw new CaptureError(
        'invalid-snapshot',
        `the captured snapshot fails the snapshot schema at ${issues}`,
      );
    }
    return result.snapshot;
  };

  let snapshot: Snapshot | undefined;
  let failure: unknown;
  try {
    snapshot = await run();
  } catch (error) {
    failure = error;
  }
  const cleanupFailures = await cleanUp(session, driver, server);
  interrupt.unlisten();

  const signal = interrupt.received;
  if (signal !== undefined) {
    const stopped =
      cleanupFailures.length === 0
        ? 'the session was deleted and safaridriver stopped'
        : `cleanup failed: ${cleanupFailures.map(describe).join('; ')}`;
    throw new CaptureError('interrupted', `the Safari capture was interrupted by ${signal}; ${stopped}`, {
      cause: failure,
      signal,
    });
  }
  if (failure !== undefined) {
    throw failure;
  }
  const [cleanupFailure] = cleanupFailures;
  if (cleanupFailure !== undefined) {
    throw cleanupFailure;
  }
  if (snapshot === undefined) {
    throw new Error('the capture finished without a snapshot');
  }
  return snapshot;
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Deletes the session, stops the driver and closes the server, each even when another fails. */
async function cleanUp(
  session: Session | undefined,
  driver: Driver | undefined,
  server: PageServer | undefined,
): Promise<unknown[]> {
  const failures: unknown[] = [];
  const attempt = async (step: () => Promise<unknown>) => {
    try {
      await step();
    } catch (error) {
      failures.push(error);
    }
  };
  if (session !== undefined) {
    await attempt(() => session.delete());
  }
  if (driver !== undefined) {
    await attempt(() => driver.stop());
  }
  if (server !== undefined) {
    await attempt(() => server.close());
  }
  return failures;
}
