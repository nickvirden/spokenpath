import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { pagePath } from '@spokenpath/capture-chromium/fixture-pages';
import { validateSnapshot } from '@spokenpath/snapshot-types';
import { afterAll, describe, expect, test } from 'vitest';
import { captureSafari, type TestDriver } from './capture.ts';
import { CaptureError } from './error.ts';
import { CONTENT_SECURITY_POLICY } from './serve.ts';
import { type DriverEvent, type DriverScenario, testDriverPath } from './test-driver.ts';

const runnerPath = fileURLToPath(new URL('capture-runner.ts', import.meta.url));
const signInPage = pagePath('sign-in.html');

const directories: string[] = [];

afterAll(() => {
  for (const directory of directories) {
    rmSync(directory, { recursive: true, force: true });
  }
});

interface Double {
  driver: TestDriver;
  events: () => DriverEvent[];
}

/** The test-double driver with `scenario`, logging to a fresh file. */
function double(scenario: Omit<DriverScenario, 'log'> = {}, versions: TestDriver['versions'] = {}): Double {
  const directory = mkdtempSync(join(tmpdir(), 'spokenpath-safari-'));
  directories.push(directory);
  const log = join(directory, 'events.jsonl');
  const scenarioPath = join(directory, 'scenario.json');
  writeFileSync(scenarioPath, JSON.stringify({ ...scenario, log }));
  return {
    driver: { command: process.execPath, args: [testDriverPath, scenarioPath], versions },
    events: () =>
      existsSync(log)
        ? readFileSync(log, 'utf8')
            .trimEnd()
            .split('\n')
            .map((line) => JSON.parse(line) as DriverEvent)
        : [],
  };
}

function names(events: DriverEvent[]): string[] {
  return events.map((event) => event.event).filter((name) => name !== 'request');
}

function isRunning(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** The driver's pid, from its `started` event. */
function driverPid(events: DriverEvent[]): number {
  const pid = events.find((event) => event.event === 'started')?.pid;
  if (pid === undefined) {
    throw new Error(`the driver never started: ${JSON.stringify(events)}`);
  }
  return pid;
}

async function failure(capture: Promise<unknown>): Promise<CaptureError> {
  const error = await capture.then(
    () => undefined,
    (error: unknown) => error,
  );
  if (!(error instanceof CaptureError)) {
    throw new Error(`expected a CaptureError, got ${String(error)}`);
  }
  return error;
}

describe('a capture with the test-double driver', () => {
  test('validates and records every tagged element with the versions', async () => {
    const { driver, events } = double(
      { version: 'Included with Safari 27.0.1 (22625.1.29.11.28)', answers: { '1': { role: 'heading' } } },
      { safariBuild: '22625.1.29.11.28', macosVersion: '27.0.1', macosBuild: '26A434' },
    );
    const snapshot = await captureSafari(signInPage, { driver });
    expect(validateSnapshot(snapshot).ok).toBe(true);
    expect(snapshot.version).toEqual({ major: 0, minor: 1 });
    expect(snapshot.producer.name).toBe('@spokenpath/capture-safari');
    expect(snapshot.capture).toMatchObject({
      engine: 'safari-webdriver',
      engine_version: '27.0.1',
      tool_version: 'safaridriver Included with Safari 27.0.1 (22625.1.29.11.28)',
      viewport: { width: 1280, height: 720, device_pixel_ratio: 2 },
      color_scheme: 'light',
      reduced_motion: 'no-preference',
      settle: 'load-fonts-frames',
    });
    expect(snapshot.capture.url).toMatch(/^http:\/\/127\.0\.0\.1:\d+\/sign-in\.html$/);
    expect(snapshot.sources).toEqual([expect.objectContaining({ id: 0, path: 'sign-in.html' })]);
    if (snapshot.payload.engine !== 'safari-webdriver') {
      throw new Error('a Safari payload');
    }
    const { elements = [] } = snapshot.payload;
    expect(elements.map((element) => element.span)).toEqual(snapshot.spans.map((span) => span.id));
    expect(elements[1]).toEqual({ span: 1, role: 'heading', label: '' });
    expect(snapshot.payload).toMatchObject({
      safari_build: '22625.1.29.11.28',
      macos_version: '27.0.1',
      macos_build: '26A434',
    });
    expect(snapshot.diagnostics).toEqual([]);
    const log = events();
    expect(names(log)).toEqual(['started', 'session-created', 'navigated', 'session-deleted', 'terminated']);
    expect(isRunning(driverPid(log))).toBe(false);
  });

  test('loads the page from the loopback address with the Content-Security-Policy', async () => {
    const { driver, events } = double();
    await captureSafari(signInPage, { driver });
    const navigated = events().find((event) => event.event === 'navigated');
    expect(navigated).toEqual({ event: 'navigated', status: 200, csp: CONTENT_SECURITY_POLICY });
  });

  test('records the settle strategy for a selector', async () => {
    const { driver } = double();
    const snapshot = await captureSafari(signInPage, { driver, waitFor: 'main' });
    expect(snapshot.capture.settle).toBe('load-selector-fonts-frames');
  });

  test('records an errored call as role_error or label_error and carries on', async () => {
    const error = { error: 'unknown error', message: '' };
    const { driver } = double({ answers: { '2': { role: error }, '3': { label: error } } });
    const snapshot = await captureSafari(signInPage, { driver });
    if (snapshot.payload.engine !== 'safari-webdriver') {
      throw new Error('a Safari payload');
    }
    const { elements = [] } = snapshot.payload;
    expect(elements[2]).toEqual({ span: 2, role_error: error, label: '' });
    expect(elements[3]).toEqual({ span: 3, role: 'generic', label_error: error });
    expect(validateSnapshot(snapshot).ok).toBe(true);
  });

  test('records a diagnostic and no entry for a source attribute that names no span', async () => {
    const { driver } = double({ extraSources: ['999', 'nonsense'] });
    const snapshot = await captureSafari(signInPage, { driver });
    expect(snapshot.diagnostics.map((diagnostic) => diagnostic.code)).toEqual([
      'unknown-source-attribute',
      'unknown-source-attribute',
    ]);
    if (snapshot.payload.engine !== 'safari-webdriver') {
      throw new Error('a Safari payload');
    }
    expect(snapshot.payload.elements).toHaveLength(snapshot.spans.length);
  });

  test('records the viewport and media features as the page measures them', async () => {
    const { driver } = double({
      measurement: {
        innerWidth: 1000,
        innerHeight: 600,
        devicePixelRatio: 1,
        dark: true,
        reducedMotion: true,
      },
    });
    const snapshot = await captureSafari(signInPage, { driver });
    expect(snapshot.capture).toMatchObject({
      viewport: { width: 1000, height: 600, device_pixel_ratio: 1 },
      color_scheme: 'dark',
      reduced_motion: 'reduce',
    });
  });
});

describe('a capture that fails', () => {
  test('quotes what the driver said when it refuses the session, and stops the driver', async () => {
    const message = 'You must enable "Allow remote automation".\u{1b}[31m';
    const { driver, events } = double({ refuseSession: { error: 'session not created', message } });
    const error = await failure(captureSafari(signInPage, { driver }));
    expect(error.code).toBe('safari-session-refused');
    expect(error.message).toContain(JSON.stringify(message));
    expect(error.message).toContain('see "Enabling Safari automation" in docs/usage.md');
    expect(error.message).not.toContain('\u{1b}');
    const log = events();
    expect(names(log)).toEqual(['started', 'terminated']);
    expect(isRunning(driverPid(log))).toBe(false);
  });

  test('quotes the driver output when the driver exits before it answers', async () => {
    const { driver } = double({ stderr: 'safaridriver: remote automation is off\n', exitAtStart: true });
    const error = await failure(captureSafari(signInPage, { driver }));
    expect(error.code).toBe('safari-session-refused');
    expect(error.message).toContain(JSON.stringify('safaridriver: remote automation is off'));
  });

  test('fails with timeout when a call gets no answer, then deletes the session and stops the driver', async () => {
    const { driver, events } = double({ answers: { '2': { role: 'hang' } } });
    const error = await failure(captureSafari(signInPage, { driver, timeoutMs: 500 }));
    expect(error.code).toBe('timeout');
    expect(error.message).toBe('Get Computed Role did not answer within 500 ms');
    const log = events();
    expect(names(log)).toEqual(['started', 'session-created', 'navigated', 'session-deleted', 'terminated']);
    expect(isRunning(driverPid(log))).toBe(false);
  });

  test('fails with page-hidden when Safari reports the page hidden, as on a locked Mac', async () => {
    const { driver, events } = double({ visibility: 'hidden' });
    const error = await failure(captureSafari(signInPage, { driver }));
    expect(error.code).toBe('page-hidden');
    expect(error.message).toContain('keep the Mac awake and unlocked');
    expect(names(events())).toContain('session-deleted');
  });

  test('fails with page-left-origin, naming where the window went, when the page navigates away while it settles', async () => {
    const { driver, events } = double({
      navigation: { to: 'https://other.example/elsewhere.html', during: 'settling' },
    });
    const error = await failure(captureSafari(signInPage, { driver }));
    expect(error.code).toBe('page-left-origin');
    expect(error.message).toMatch(
      /^the page's fonts and animation frames: the page left the browsing context it was served in, on "http:\/\/127\.0\.0\.1:\d+", as a page does when it navigates away \(Safari answered with error "no such window"\); Safari reports the window's origin as "https:\/\/other\.example"; capture detects such a navigation and does not prevent it, so the page may already have made requests to other origins$/,
    );
    const log = events();
    expect(log.some((event) => event.path?.endsWith('/elements') === true)).toBe(false);
    expect(names(log)).toEqual(['started', 'session-created', 'navigated', 'session-deleted', 'terminated']);
    expect(isRunning(driverPid(log))).toBe(false);
  });

  test('fails with page-left-origin and says so when Safari cannot report where the window went', async () => {
    const { driver } = double({
      navigation: { to: 'https://other.example/elsewhere.html', during: 'settling' },
      currentUrlError: { error: 'no such window', message: '' },
    });
    const error = await failure(captureSafari(signInPage, { driver }));
    expect(error.code).toBe('page-left-origin');
    expect(error.message).toContain(
      `Safari cannot say where the window went (reading the window's URL: Safari answered with error "no such window")`,
    );
    expect(error.message).not.toContain('other.example');
  });

  test('fails with page-left-origin when the navigation has finished by the time the page is measured', async () => {
    const { driver, events } = double({
      navigation: { to: 'https://other.example/elsewhere.html', during: 'after-settling' },
    });
    const error = await failure(captureSafari(signInPage, { driver }));
    expect(error.code).toBe('page-left-origin');
    expect(error.message).toMatch(
      /^the page left the origin it was served on, "http:\/\/127\.0\.0\.1:\d+", before it was measured: Safari reports its origin as "https:\/\/other\.example"; capture detects such a navigation and does not prevent it/,
    );
    expect(events().some((event) => event.path?.endsWith('/elements') === true)).toBe(false);
  });

  test('fails with page-left-origin when the navigation has finished just before the elements are read', async () => {
    const { driver, events } = double({
      navigation: { to: 'http://127.0.0.1:1/', during: 'after-measuring' },
    });
    const error = await failure(captureSafari(signInPage, { driver }));
    expect(error.code).toBe('page-left-origin');
    expect(error.message).toContain('before its elements were read');
    expect(error.message).toContain('Safari reports its origin as "http://127.0.0.1:1"');
    const log = events();
    expect(log.some((event) => event.path?.endsWith('/elements') === true)).toBe(false);
    expect(names(log)).toContain('session-deleted');
  });

  test('records no entry for the elements of a page that navigates away while they are read', async () => {
    const { driver } = double({
      navigation: { to: 'https://other.example/elsewhere.html', during: 'element-reads' },
    });
    const snapshot = await captureSafari(signInPage, { driver });
    if (snapshot.payload.engine !== 'safari-webdriver') {
      throw new Error('a Safari payload');
    }
    expect(snapshot.payload.elements).toEqual([]);
    expect(snapshot.diagnostics).toHaveLength(snapshot.spans.length);
    expect(new Set(snapshot.diagnostics.map((diagnostic) => diagnostic.message))).toEqual(
      new Set([
        'Safari answered Get Element Attribute with error "stale element reference", so the element has no entry.',
      ]),
    );
    expect(validateSnapshot(snapshot).ok).toBe(true);
  });

  test('fails with safari-call-failed, naming the step, when loading the page answers with an error', async () => {
    const { driver, events } = double({ loadError: { error: 'unknown error', message: 'load failed' } });
    const error = await failure(captureSafari(signInPage, { driver }));
    expect(error.code).toBe('safari-call-failed');
    expect(error.message).toBe('loading the page: Safari answered with error "unknown error": "load failed"');
    expect(names(events())).toEqual(['started', 'session-created', 'session-deleted', 'terminated']);
  });

  test('fails with safari-call-failed when the session is lost while elements are read', async () => {
    const { driver, events } = double({
      answers: { '3': { role: { error: 'no such window', message: 'the window was closed' } } },
    });
    const error = await failure(captureSafari(signInPage, { driver }));
    expect(error.code).toBe('safari-call-failed');
    expect(error.message).toBe(
      'Get Computed Role: Safari answered with error "no such window": "the window was closed"',
    );
    const log = events();
    expect(names(log)).toContain('session-deleted');
    expect(isRunning(driverPid(log))).toBe(false);
  });

  test('records a stale element as role_error and carries on, as it is not a lost session', async () => {
    const stale = { error: 'stale element reference', message: '' };
    const { driver } = double({ answers: { '3': { role: stale } } });
    const snapshot = await captureSafari(signInPage, { driver });
    if (snapshot.payload.engine !== 'safari-webdriver') {
      throw new Error('a Safari payload');
    }
    expect(snapshot.payload.elements?.[3]).toEqual({ span: 3, role_error: stale, label: '' });
  });

  test('fails with safaridriver-not-found when the driver is not installed', async () => {
    const driver: TestDriver = { command: join(tmpdir(), 'no-such-safaridriver'), args: [], versions: {} };
    const error = await failure(captureSafari(signInPage, { driver }));
    expect(error.code).toBe('safaridriver-not-found');
  });

  test('fails with page-unreadable for a missing page, naming only its file name', async () => {
    const { driver, events } = double();
    const error = await failure(captureSafari(join(tmpdir(), 'missing', 'page.html'), { driver }));
    expect(error.code).toBe('page-unreadable');
    expect(error.message).toBe('cannot read the page "page.html": ENOENT');
    expect(events()).toEqual([]);
  });

  test('refuses a timeout that is not positive', async () => {
    const { driver } = double();
    const error = await failure(captureSafari(signInPage, { driver, timeoutMs: 0 }));
    expect(error.code).toBe('invalid-options');
  });

  test.runIf(process.platform !== 'darwin')('fails with unsupported-platform off macOS', async () => {
    const error = await failure(captureSafari(signInPage));
    expect(error.code).toBe('unsupported-platform');
  });
});

describe('an interrupted capture', () => {
  test.each([
    ['SIGINT', 130],
    ['SIGTERM', 143],
  ] as const)('on %s deletes the session, stops the driver and exits %i', async (signal, exitCode) => {
    const { driver, events } = double({ answers: { '2': { role: 'hang' } } });
    const directory = mkdtempSync(join(tmpdir(), 'spokenpath-runner-'));
    directories.push(directory);
    const optionsPath = join(directory, 'options.json');
    writeFileSync(optionsPath, JSON.stringify({ driver, timeoutMs: 60_000 }));
    const child = spawn(process.execPath, [runnerPath, signInPage, optionsPath], {
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (chunk: string) => {
      stdout += chunk;
    });
    const exited = new Promise<number | null>((resolve) => child.once('exit', (code) => resolve(code)));
    const hung = () => events().some((event) => event.path?.endsWith('/computedrole') === true);
    for (let waited = 0; !hung(); waited += 50) {
      if (waited > 20_000) {
        throw new Error(`the capture never reached the hung call: ${JSON.stringify(events())}`);
      }
      await sleep(50);
    }
    child.kill(signal);
    expect(await exited).toBe(exitCode);
    expect(stdout).toBe('interrupted\n');
    const log = events();
    expect(names(log)).toEqual(['started', 'session-created', 'navigated', 'session-deleted', 'terminated']);
    expect(isRunning(driverPid(log))).toBe(false);
  });
});
