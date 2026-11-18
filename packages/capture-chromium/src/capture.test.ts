import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { Snapshot } from '@spokenpath/snapshot-types';
import { type Browser, chromium } from 'playwright-core';
import { afterAll, beforeAll, describe, expect, test, vi } from 'vitest';
import { captureChromium, verifyInstrumentation } from './capture.ts';
import { CaptureError } from './error.ts';
import { chromiumPayload, elementId, htmlPagesIn, pagePath } from './fixture-pages.ts';
import { normaliseForGolden } from './normalise.ts';

/** Marks a step on an element that is not in the captured DOM. */
const NOT_IN_DOM = '(not in dom)';

/**
 * Where focus lands after each Tab press: an element's `id`, `NOT_IN_DOM`, or `null` when focus
 * left the document. Content inside a frame or a closed shadow root shows as the frame element or
 * the host, once per press.
 */
const expectedFocus: Record<string, (string | null)[]> = {
  'focus/positive-tabindex.html': [
    'tabindex-one',
    'tabindex-two',
    'natural-first',
    'natural-last',
    null,
    'tabindex-one',
  ],
  'focus/disabled.html': ['before', 'after', null, 'before'],
  'focus/inert.html': ['before', 'after', null, 'before'],
  'focus/delegates-focus.html': ['before', 'inner', 'after', null, 'before'],
  'focus/closed-shadow.html': ['before', 'host', 'host', 'after', null, 'before'],
  'focus/iframe.html': ['before', 'frame', 'frame', 'after', null, 'before'],
  'focus/added-on-tab.html': ['before', NOT_IN_DOM, null, 'before'],
  'focus/no-focusable.html': [null, null],
  'focus/trap.html': ['first', 'middle', 'last', 'first'],
  'focus/self-trap.html': ['before', 'trapped', 'trapped'],
  // The cap is written out, so changing the constant fails here rather than passing silently.
  'focus/long.html': Array.from({ length: 200 }, (_, index) => `b${String(index + 1).padStart(3, '0')}`),
};

/** Each focus page captured once, by the first test that needs it. */
const focusCaptures = new Map<string, Promise<Snapshot>>();
function captureFocusPage(page: string): Promise<Snapshot> {
  let captured = focusCaptures.get(page);
  if (captured === undefined) {
    captured = captureChromium(pagePath(page));
    focusCaptures.set(page, captured);
  }
  return captured;
}

function focusOf(page: string) {
  return captureFocusPage(page).then((snapshot) => chromiumPayload(snapshot).focus);
}

describe.each(htmlPagesIn('focus/'))('focus walk of %s', (page) => {
  test('lands on the expected elements in order', async () => {
    const expected = expectedFocus[page];
    expect(expected, `${page} has no entry in expectedFocus`).toBeDefined();
    const snapshot = await captureFocusPage(page);
    const steps = chromiumPayload(snapshot).focus.steps;
    const landed = steps.map((step) => {
      if (step.node !== undefined) {
        return elementId(snapshot, step.node) ?? '(no id)';
      }
      return step.backend_id === undefined ? null : NOT_IN_DOM;
    });
    expect(landed).toEqual(expected);
  });
});

describe('focus walk stop rules', () => {
  test('a trap reports a cycle with no step outside the document before it', async () => {
    const focus = await focusOf('focus/trap.html');
    expect(focus.truncated).toBe(false);
    expect(focus.cycle_start).toBeTypeOf('number');
    expect(focus.steps.slice(0, focus.cycle_start).some((step) => step.node === undefined)).toBe(false);
  });

  test('a script that keeps focus on one element reports a cycle at the repeat, not the cap', async () => {
    const focus = await focusOf('focus/self-trap.html');
    expect(focus.cycle_start).toBe(2);
    expect(focus.truncated).toBe(false);
    expect(focus.steps).toHaveLength(3);
  });

  test('a page with more stops than the cap is truncated at 200 steps with no cycle', async () => {
    const focus = await focusOf('focus/long.html');
    expect(focus.steps).toHaveLength(200);
    expect(focus.truncated).toBe(true);
    expect(focus).not.toHaveProperty('cycle_start');
  });

  test('a page with no tab stops stops at the second step outside the document', async () => {
    const focus = await focusOf('focus/no-focusable.html');
    expect(focus.steps).toEqual([{}, {}]);
    expect(focus.truncated).toBe(false);
    expect(focus).not.toHaveProperty('cycle_start');
  });

  test.each([
    ['focus/closed-shadow.html', 'focus-in-closed-shadow-root', 'host'],
    ['focus/iframe.html', 'iframe-not-captured', 'frame'],
  ])('focus moving inside %s is not a cycle, and the stop after it is reached', async (page, code, id) => {
    const snapshot = await captureFocusPage(page);
    const { focus } = chromiumPayload(snapshot);
    const ids = focus.steps.map((step) => elementId(snapshot, step.node));
    // The only cycle is the wrap back to the first stop, after focus left the document.
    expect(focus.cycle_start).toBe(ids.lastIndexOf('before'));
    expect(focus.steps[(focus.cycle_start ?? 0) - 1]).toEqual({});
    expect(ids).toContain('after');
    const diagnostics = snapshot.diagnostics.filter((diagnostic) => diagnostic.code === code);
    expect(diagnostics).toHaveLength(1);
    expect(elementId(snapshot, diagnostics[0]?.node)).toBe(id);
  });

  test('focus on an element added after the DOM was read records its backend id and a diagnostic', async () => {
    const snapshot = await captureFocusPage('focus/added-on-tab.html');
    const step = chromiumPayload(snapshot).focus.steps[1];
    expect(step).toEqual({ backend_id: expect.any(Number) });
    expect(snapshot.diagnostics).toEqual([
      expect.objectContaining({
        code: 'focus-target-not-in-dom',
        message: expect.stringContaining('Tab press 2'),
      }),
    ]);
    expect(snapshot.diagnostics[0]).not.toHaveProperty('node');
  });
});

describe('capture failures', () => {
  let directory: string;
  const write = (name: string, contents: string) => {
    const path = join(directory, name);
    writeFileSync(path, contents);
    return path;
  };

  beforeAll(() => {
    directory = mkdtempSync(join(tmpdir(), 'capture-failures-'));
  });

  afterAll(() => {
    rmSync(directory, { recursive: true, force: true });
  });

  test('a missing page is page-unreadable, named by its base name only', async () => {
    const capture = captureChromium(join(directory, 'missing.html'));
    await expect(capture).rejects.toThrow(CaptureError);
    await expect(capture).rejects.toMatchObject({ code: 'page-unreadable' });
    const message = await capture.catch((error: Error) => error.message);
    expect(message).toContain('"missing.html"');
    expect(message).not.toContain(directory);
  });

  test.skipIf(process.getuid?.() === 0)(
    'an asset that cannot be read aborts the capture as asset-unreadable',
    async () => {
      const page = write(
        'locked-asset.html',
        '<!doctype html><link rel="stylesheet" href="locked.css"><p>x</p>',
      );
      chmodSync(write('locked.css', 'p { color: red; }'), 0o000);
      await expect(captureChromium(page)).rejects.toMatchObject({
        name: 'CaptureError',
        code: 'asset-unreadable',
      });
    },
  );

  test('a page that never finishes loading times out', { timeout: 30_000 }, async () => {
    const page = write('busy-load.html', '<!doctype html><p>x</p><script>for (;;) {}</script>');
    await expect(captureChromium(page, { timeoutMs: 1_000 })).rejects.toMatchObject({
      name: 'CaptureError',
      code: 'timeout',
    });
  });

  test('a page that keeps its main thread busy after load times out', { timeout: 30_000 }, async () => {
    const page = write(
      'busy-after-load.html',
      '<!doctype html><p>x</p><script>addEventListener("load", () => setTimeout(() => { for (;;) {} }, 0));</script>',
    );
    await expect(captureChromium(page, { timeoutMs: 1_000 })).rejects.toMatchObject({
      name: 'CaptureError',
      code: 'timeout',
    });
  });

  test('a key handler that never returns times out, and the browser is closed', {
    timeout: 30_000,
  }, async () => {
    const launched: Browser[] = [];
    const launch = chromium.launch.bind(chromium);
    const spy = vi.spyOn(chromium, 'launch').mockImplementation(async (options) => {
      const browser = await launch(options);
      launched.push(browser);
      return browser;
    });
    try {
      await expect(
        captureChromium(pagePath('timeout/keydown-loop.html'), { timeoutMs: 1_000 }),
      ).rejects.toMatchObject({
        name: 'CaptureError',
        code: 'timeout',
        message: 'pressing Tab did not finish within 1000 ms',
      });
    } finally {
      spy.mockRestore();
    }
    expect(launched).toHaveLength(1);
    expect(launched[0]?.isConnected()).toBe(false);
  });

  test.each([0, -1, Number.NaN, Number.POSITIVE_INFINITY])('timeoutMs %s is refused', async (timeoutMs) => {
    const page = pagePath('sign-in.html');
    await expect(captureChromium(page, { timeoutMs })).rejects.toMatchObject({ code: 'invalid-options' });
    await expect(verifyInstrumentation(page, { timeoutMs })).rejects.toMatchObject({
      code: 'invalid-options',
    });
  });
});

test('an external request is blocked and recorded without its query string or fragment', async () => {
  const snapshot = await captureChromium(pagePath('external-request.html'));
  const blocked = snapshot.diagnostics.filter((diagnostic) => diagnostic.code === 'external-request-blocked');
  const messages = blocked.map((diagnostic) => diagnostic.message);
  expect(messages.some((message) => message.includes('https://example.com/pixel.png'))).toBe(true);
  expect(messages.some((message) => message.includes('wss://example.com/socket'))).toBe(true);
  for (const message of messages) {
    expect(message).not.toMatch(/[?#]|not-a-secret/);
  }
});

test('the header names the browser version and the pinned playwright-core version', async () => {
  const manifest = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')) as {
    dependencies: Record<string, string>;
  };
  const browser = await chromium.launch({ channel: 'chromium', headless: true });
  let browserVersion: string;
  try {
    browserVersion = browser.version();
  } finally {
    await browser.close();
  }
  const { capture } = await captureChromium(pagePath('sign-in.html'));
  expect(capture.engine_version).toBe(browserVersion);
  expect(capture.tool_version).toBe(`playwright ${manifest.dependencies['playwright-core']}`);
});

describe('settling', () => {
  test('waits for a selector, so content a script adds later is captured', async () => {
    const snapshot = await captureChromium(pagePath('settle/late-content.html'), { waitFor: '#late' });
    expect(snapshot.capture.settle).toBe('load-selector-fonts-frames');
    const late = chromiumPayload(snapshot).dom.find((node) =>
      node.attributes.some((attribute) => attribute.name === 'id' && attribute.value === 'late'),
    );
    expect(late?.origin).toBe('script');
  });

  test('without a selector, records the default settle name', async () => {
    const snapshot = await captureChromium(pagePath('sign-in.html'));
    expect(snapshot.capture.settle).toBe('load-fonts-frames');
  });

  test('fails naming the selector when it never matches', async () => {
    const capture = captureChromium(pagePath('settle/late-content.html'), {
      waitFor: '#never-added',
      timeoutMs: 1_000,
    });
    await expect(capture).rejects.toThrow(CaptureError);
    await expect(capture).rejects.toThrow('#never-added');
  });
});

describe('origins', () => {
  test('html, head and body the parser creates are implied', async () => {
    const { dom } = chromiumPayload(await captureChromium(pagePath('torture/implied-elements.html')));
    for (const tag of ['html', 'head', 'body', 'tbody']) {
      expect(dom.find((node) => node.tag === tag)?.origin, tag).toBe('implied');
    }
  });

  test('user-agent shadow content is unknown', async () => {
    const { dom } = chromiumPayload(await captureChromium(pagePath('sign-in.html')));
    const userAgentRoots = dom.filter((node) => node.shadow === 'user-agent');
    expect(userAgentRoots.length).toBeGreaterThan(0);
    const inside = userAgentRoots.flatMap((root) =>
      root.children.map((child) => dom[child]).filter((node) => node?.kind === 'element'),
    );
    expect(inside.length).toBeGreaterThan(0);
    expect(inside.map((node) => node?.origin)).toEqual(inside.map(() => 'unknown'));
  });
});

test('two browser launches capture the same normalised snapshot', async () => {
  const first = normaliseForGolden(await captureChromium(pagePath('sign-in.html')));
  const second = normaliseForGolden(await captureChromium(pagePath('sign-in.html')));
  expect(second).toEqual(first);
});
