import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { pagePath } from '@spokenpath/capture-chromium/fixture-pages';
import { sourceClassToken } from '@spokenpath/instrument-html';
import { type SafariAxNode, type Snapshot, validateSnapshot } from '@spokenpath/snapshot-types';
import { afterAll, describe, expect, test } from 'vitest';
import { captureSafariAx, mapSourceTokens } from './capture-ax.ts';
import type { TestDriver } from './capture.ts';
import { CaptureError } from './error.ts';
import type { HelperCommand } from './helper.ts';
import { type DriverEvent, type DriverScenario, testDriverPath } from './test-driver.ts';
import { type HelperEvent, type HelperReply, type HelperScenario, testHelperPath } from './test-helper.ts';

const runnerPath = fileURLToPath(new URL('capture-runner.ts', import.meta.url));
const signInPage = pagePath('sign-in.html');

const directories: string[] = [];

afterAll(() => {
  for (const directory of directories) {
    rmSync(directory, { recursive: true, force: true });
  }
});

function readLog<T>(path: string): T[] {
  return existsSync(path)
    ? readFileSync(path, 'utf8')
        .trimEnd()
        .split('\n')
        .map((line) => JSON.parse(line) as T)
    : [];
}

interface Doubles {
  driver: TestDriver;
  helper: HelperCommand;
  driverEvents: () => DriverEvent[];
  helperEvents: () => HelperEvent[];
}

/** The test-double driver and helper with their scenarios, logging to fresh files. */
function doubles(
  helperScenario: Omit<HelperScenario, 'log'> = {},
  driverScenario: Omit<DriverScenario, 'log'> = {},
): Doubles {
  const directory = mkdtempSync(join(tmpdir(), 'spokenpath-safari-ax-'));
  directories.push(directory);
  const driverLog = join(directory, 'driver.jsonl');
  const helperLog = join(directory, 'helper.jsonl');
  const driverPath = join(directory, 'driver.json');
  const helperPath = join(directory, 'helper.json');
  writeFileSync(driverPath, JSON.stringify({ ...driverScenario, log: driverLog }));
  writeFileSync(helperPath, JSON.stringify({ ...helperScenario, log: helperLog }));
  return {
    driver: {
      command: process.execPath,
      args: [testDriverPath, driverPath],
      versions: { safariBuild: '22625.1.29.11.28', macosVersion: '27.0.1', macosBuild: '26A434' },
    },
    helper: { command: process.execPath, args: [testHelperPath, helperPath] },
    driverEvents: () => readLog<DriverEvent>(driverLog),
    helperEvents: () => readLog<HelperEvent>(helperLog),
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

/** The helper's `read` runs, by whether they read the check page. */
function reads(events: HelperEvent[]): { canary: HelperEvent[]; page: HelperEvent[] } {
  const all = events.filter((event) => event.args[0] === 'read');
  return {
    canary: all.filter((event) => event.args.includes('--canary')),
    page: all.filter((event) => !event.args.includes('--canary')),
  };
}

function argument(event: HelperEvent | undefined, flag: string): string | undefined {
  const args = event?.args ?? [];
  const index = args.indexOf(flag);
  return index === -1 ? undefined : args[index + 1];
}

function axNodes(snapshot: Snapshot): SafariAxNode[] {
  if (snapshot.payload.engine !== 'safari-ax') {
    throw new Error(`a Safari AX payload, not ${snapshot.payload.engine}`);
  }
  return snapshot.payload.nodes ?? [];
}

const PERMISSION_MESSAGE =
  'macOS reports that this process is not trusted for Accessibility (AXIsProcessTrusted() is false). Grant the Accessibility permission to the app that runs spokenpath, such as Terminal, in System Settings > Privacy & Security > Accessibility, then run the capture again; see "Granting the Accessibility permission" in docs/usage.md.';

const LOCKED_MESSAGE =
  'the Mac\'s screen is locked, so Safari\'s macOS accessibility tree cannot be read; keep the Mac awake and unlocked while Safari capture runs (see "Enabling Safari automation" in docs/usage.md)';

describe('a Safari AX capture with the test doubles', () => {
  test('reads the check page, then the page, and records a valid minor-2 snapshot', async () => {
    const { driver, helper, driverEvents, helperEvents } = doubles();
    const snapshot = await captureSafariAx(signInPage, { driver, helper });
    expect(validateSnapshot(snapshot).ok).toBe(true);
    expect(snapshot.version).toEqual({ major: 0, minor: 2 });
    expect(snapshot.capture).toMatchObject({
      engine: 'safari-ax',
      engine_version: '27.0.1',
      tool_version: 'safaridriver test double 1.0',
      viewport: { width: 1280, height: 720, device_pixel_ratio: 2 },
      settle: 'load-fonts-frames',
    });
    expect(snapshot.capture.url).toMatch(
      /^http:\/\/127\.0\.0\.1:\d+\/sign-in\.html\?spokenpath-nonce=[0-9a-f]{32}$/,
    );
    expect(snapshot.payload).toMatchObject({
      engine: 'safari-ax',
      safari_build: '22625.1.29.11.28',
      macos_version: '27.0.1',
      macos_build: '26A434',
    });
    expect(snapshot.diagnostics).toEqual([]);

    const events = helperEvents();
    expect(events.map((event) => event.args[0])).toEqual(['canary-page', 'read', 'read']);
    const { canary, page } = reads(events);
    expect(canary).toHaveLength(1);
    expect(page).toHaveLength(1);
    expect(argument(page[0], '--url')).toBe(snapshot.capture.url);
    expect(argument(page[0], '--title')).toBe('Sign in');
    expect(argument(page[0], '--window-rect')).toBe('0,25,1280,800');
    const canaryUrl = argument(canary[0], '--url') ?? '';
    expect(canaryUrl).toMatch(
      /^http:\/\/127\.0\.0\.1:\d+\/spokenpath-check\.html\?spokenpath-nonce=[0-9a-f]{32}$/,
    );
    expect(new URL(canaryUrl).searchParams.get('spokenpath-nonce')).not.toBe(
      new URL(snapshot.capture.url).searchParams.get('spokenpath-nonce'),
    );
    expect(argument(canary[0], '--title')).toBe('spokenpath accessibility check');
    expect(names(driverEvents())).toEqual([
      'started',
      'session-created',
      'navigated',
      'navigated',
      'session-deleted',
      'terminated',
    ]);
  });

  test("gives every node the span its class token names and keeps Safari's values", async () => {
    const { driver, helper } = doubles();
    const snapshot = await captureSafariAx(signInPage, { driver, helper });
    const nodes = axNodes(snapshot);
    const carried = nodes.filter((node) => node.span !== undefined);
    expect(carried.map((node) => node.span)).toEqual(snapshot.spans.map((span) => span.id));
    for (const node of carried) {
      const span = snapshot.spans[node.span ?? -1];
      expect(node.attributes.AXDOMClassList).toEqual({
        type: 'array',
        items: [{ type: 'string', value: sourceClassToken(span ?? { line: 0, column: 0 }) }],
      });
    }
    expect(nodes[0]?.span).toBeUndefined();
    expect(nodes[1]?.span).toBeUndefined();
  });

  test('records unknown-source-token for a token that names no span, and gives that node none', async () => {
    const { driver, helper } = doubles({ extraTokens: ['sp-src-999-1'] });
    const snapshot = await captureSafariAx(signInPage, { driver, helper });
    expect(snapshot.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(['unknown-source-token']);
    expect(snapshot.diagnostics[0]?.message).toContain('"sp-src-999-1"');
    const last = axNodes(snapshot).at(-1);
    expect(last?.span).toBeUndefined();
    expect(last?.attributes.AXDOMClassList).toEqual({
      type: 'array',
      items: [{ type: 'string', value: 'sp-src-999-1' }],
    });
    expect(validateSnapshot(snapshot).ok).toBe(true);
  });

  test('serves the check page override instead of asking the helper for it', async () => {
    const { driver, helper, helperEvents } = doubles();
    await captureSafariAx(signInPage, {
      driver,
      helper,
      canaryPage: '<!doctype html><title>t</title><p>x</p>',
    });
    const events = helperEvents();
    expect(events.map((event) => event.args[0])).toEqual(['read', 'read']);
    expect(argument(reads(events).canary[0], '--title')).toBe('t');
  });
});

describe('mapSourceTokens', () => {
  test('takes the first token that names a span and reports each that names none', () => {
    const spans = [{ id: 4, source: 0, start_tag: { start: 0, end: 3 }, line: 2, column: 5 }];
    const node: SafariAxNode = {
      id: 0,
      children: [],
      attributes: {
        AXDOMClassList: {
          type: 'array',
          items: [
            { type: 'string', value: 'note' },
            { type: 'string', value: 'sp-src-9-9' },
            { type: 'string', value: 'sp-src-2-5' },
          ],
        },
      },
    };
    const { nodes, diagnostics } = mapSourceTokens([node], spans);
    expect(nodes[0]?.span).toBe(4);
    expect(nodes[0]?.attributes).toEqual(node.attributes);
    expect(diagnostics.map((diagnostic) => diagnostic.code)).toEqual(['unknown-source-token']);
  });
});

/** A helper failure in the helper's own words. */
function helperError(code: string, message: string, exit = 1): HelperReply {
  return { kind: 'error', code, message, exit };
}

describe('a Safari AX capture that fails', () => {
  test.each([
    ['check page', { canary: helperError('accessibility-permission-missing', PERMISSION_MESSAGE, 3) }],
    ['page', { read: helperError('accessibility-permission-missing', PERMISSION_MESSAGE, 3) }],
  ] as const)(
    'fails with accessibility-permission-missing when the %s read lacks the permission',
    async (_, scenario) => {
      const { driver, helper, driverEvents } = doubles(scenario);
      const error = await failure(captureSafariAx(signInPage, { driver, helper }));
      expect(error.code).toBe('accessibility-permission-missing');
      expect(error.message).toBe(PERMISSION_MESSAGE);
      expect(names(driverEvents())).toContain('session-deleted');
    },
  );

  test('fails with accessibility-permission-missing before the page when the check page is empty', async () => {
    const empty = `Safari's macOS accessibility tree for the check page has none of its 3 expected nodes while the page is visible and the screen is unlocked; macOS returns an empty tree when the process lacks the Accessibility permission. ${PERMISSION_MESSAGE.slice(PERMISSION_MESSAGE.indexOf('Grant'))}`;
    const { driver, helper, helperEvents } = doubles({
      canary: helperError('accessibility-permission-missing', empty, 3),
    });
    const error = await failure(captureSafariAx(signInPage, { driver, helper }));
    expect(error.code).toBe('accessibility-permission-missing');
    expect(error.message).toBe(empty);
    expect(reads(helperEvents()).page).toEqual([]);
  });

  test('fails with ax-canary-incomplete when the check page has some of its nodes', async () => {
    const message =
      "Safari's macOS accessibility tree for the check page has 2 of its 3 expected nodes while the page is visible and the screen is unlocked, so the tree cannot be relied on; run the capture again, and if it repeats, report it with the Safari and macOS versions.";
    const { driver, helper, helperEvents } = doubles({
      canary: helperError('ax-canary-incomplete', message),
    });
    const error = await failure(captureSafariAx(signInPage, { driver, helper }));
    expect(error.code).toBe('ax-canary-incomplete');
    expect(error.message).toBe(message);
    expect(reads(helperEvents()).page).toEqual([]);
  });

  test('fails with page-hidden, saying the screen is locked, when the helper finds it locked', async () => {
    const { driver, helper } = doubles({ read: helperError('screen-locked', LOCKED_MESSAGE) });
    const error = await failure(captureSafariAx(signInPage, { driver, helper }));
    expect(error.code).toBe('page-hidden');
    expect(error.message).toBe(LOCKED_MESSAGE);
  });

  test('fails with page-hidden, saying the page was hidden after the read, when it is', async () => {
    const { driver, helper } = doubles({}, { visibilitySequence: ['visible', 'visible', 'hidden'] });
    const error = await failure(captureSafariAx(signInPage, { driver, helper }));
    expect(error.code).toBe('page-hidden');
    expect(error.message).toMatch(
      /^Safari reports the page as "hidden" after its macOS accessibility tree was read, so the tree may not be the page as it shows; keep the Mac awake and unlocked/,
    );
    expect(error.message).not.toContain('locked,');
  });

  test('fails with page-hidden when the check page cannot settle', async () => {
    const { driver, helper, helperEvents } = doubles({}, { visibility: 'hidden' });
    const error = await failure(captureSafariAx(signInPage, { driver, helper }));
    expect(error.code).toBe('page-hidden');
    expect(reads(helperEvents()).canary).toEqual([]);
  });

  test.each([
    [
      'no window',
      "the automation window cannot be identified, so nothing was read: no Safari window has the page's title and the automation window's position and size; capture reads only the one window showing the page it served",
    ],
    [
      'two windows',
      "the automation window cannot be identified, so nothing was read: 2 Safari windows have the page's title and the automation window's position and size; capture reads only the one window showing the page it served",
    ],
  ])('fails with safari-window-not-identified for %s', async (_, message) => {
    const { driver, helper } = doubles({ read: helperError('safari-window-not-identified', message) });
    const error = await failure(captureSafariAx(signInPage, { driver, helper }));
    expect(error.code).toBe('safari-window-not-identified');
    expect(error.message).toBe(message);
  });

  test.each([
    ['safari-not-running', 'safari-not-running'],
    ['ax-tree-too-large', 'ax-tree-too-large'],
    ['page-left-origin', 'page-left-origin'],
    ['timeout', 'timeout'],
    ['unsupported-platform', 'unsupported-platform'],
  ])('fails with %s when the helper reports %s', async (code, helperCode) => {
    const { driver, helper, driverEvents } = doubles({
      read: helperError(helperCode, `the helper says ${helperCode}`),
    });
    const error = await failure(captureSafariAx(signInPage, { driver, helper }));
    expect(error.code).toBe(code);
    expect(error.message).toBe(`the helper says ${helperCode}`);
    expect(names(driverEvents())).toContain('session-deleted');
  });

  test('fails with ax-tree-too-large and kills the helper when it prints more than 32 MiB', async () => {
    const { driver, helper, helperEvents } = doubles({ read: { kind: 'flood' } });
    const error = await failure(captureSafariAx(signInPage, { driver, helper }));
    expect(error.code).toBe('ax-tree-too-large');
    expect(error.message).toContain('32 MiB');
    const pid = reads(helperEvents()).page[0]?.pid ?? 0;
    expect(isRunning(pid)).toBe(false);
  });

  test('fails with timeout and kills the helper when it runs past its deadline', async () => {
    const { driver, helper, helperEvents, driverEvents } = doubles({ read: { kind: 'hang' } });
    const error = await failure(captureSafariAx(signInPage, { driver, helper, timeoutMs: 500 }));
    expect(error.code).toBe('timeout');
    expect(error.message).toBe(
      'the spokenpath-ax-macos helper did not finish within 5500 ms, so it was killed',
    );
    expect(isRunning(reads(helperEvents()).page[0]?.pid ?? 0)).toBe(false);
    expect(names(driverEvents())).toContain('session-deleted');
  });

  test.runIf(process.platform !== 'darwin')('fails with unsupported-platform off macOS', async () => {
    const error = await failure(captureSafariAx(signInPage));
    expect(error.code).toBe('unsupported-platform');
  });

  test('fails with ax-helper-not-found when the helper cannot be started', async () => {
    const { driver } = doubles();
    const helper = { command: join(tmpdir(), 'no-such-spokenpath-ax-macos'), args: [] };
    const error = await failure(captureSafariAx(signInPage, { driver, helper }));
    expect(error.code).toBe('ax-helper-not-found');
    expect(error.message).toContain('cargo build -p spokenpath_ax_macos');
  });

  test('fails with ax-helper-failed, quoting stderr escaped, for output of no documented shape', async () => {
    const { driver, helper } = doubles({
      read: { kind: 'output', stdout: 'not json', stderr: 'panicked\u{1b}[31m at main', exit: 101 },
    });
    const error = await failure(captureSafariAx(signInPage, { driver, helper }));
    expect(error.code).toBe('ax-helper-failed');
    expect(error.message).toBe(
      'the spokenpath-ax-macos helper exited with status 101 and printed no answer capture reads; its stderr begins "panicked\\u001b[31m at main"',
    );
  });

  test.each<[string, HelperReply]>([
    ['a code capture does not know', helperError('ax-read-failed', 'the names cannot be read')],
    [
      'a missing permission with the wrong exit status',
      helperError('accessibility-permission-missing', 'x', 1),
    ],
    ['another failure with the permission exit status', helperError('timeout', 'x', 3)],
    ['an incomplete check printed as a success', { kind: 'canary', found: 2 }],
  ])('fails with ax-helper-failed for %s', async (_, reply) => {
    const scenario = reply.kind === 'canary' ? { canary: reply } : { read: reply };
    const { driver, helper } = doubles(scenario);
    const error = await failure(captureSafariAx(signInPage, { driver, helper }));
    expect(error.code).toBe('ax-helper-failed');
  });
});

describe('an interrupted Safari AX capture', () => {
  test.each([
    ['SIGINT', 130],
    ['SIGTERM', 143],
  ] as const)(
    'on %s kills the helper, deletes the session, stops the driver and exits %i',
    async (signal, exitCode) => {
      const { driver, helper, driverEvents, helperEvents } = doubles({ read: { kind: 'hang' } });
      const directory = mkdtempSync(join(tmpdir(), 'spokenpath-runner-'));
      directories.push(directory);
      const optionsPath = join(directory, 'options.json');
      writeFileSync(optionsPath, JSON.stringify({ driver, helper, timeoutMs: 60_000 }));
      const child = spawn(process.execPath, [runnerPath, signInPage, optionsPath], {
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      let stdout = '';
      child.stdout.setEncoding('utf8');
      child.stdout.on('data', (chunk: string) => {
        stdout += chunk;
      });
      const exited = new Promise<number | null>((resolve) => child.once('exit', (code) => resolve(code)));
      for (let waited = 0; reads(helperEvents()).page.length === 0; waited += 50) {
        if (waited > 20_000) {
          throw new Error(`the capture never reached the page read: ${JSON.stringify(helperEvents())}`);
        }
        await sleep(50);
      }
      child.kill(signal);
      expect(await exited).toBe(exitCode);
      expect(stdout).toBe('interrupted\n');
      expect(isRunning(reads(helperEvents()).page[0]?.pid ?? 0)).toBe(false);
      const events = driverEvents();
      expect(names(events)).toEqual([
        'started',
        'session-created',
        'navigated',
        'navigated',
        'session-deleted',
        'terminated',
      ]);
      const pid = events.find((event) => event.event === 'started')?.pid ?? 0;
      expect(isRunning(pid)).toBe(false);
    },
  );
});
