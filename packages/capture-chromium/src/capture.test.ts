import { readFileSync } from 'node:fs';
import { chromium } from 'playwright-core';
import { describe, expect, test } from 'vitest';
import { captureChromium } from './capture.ts';
import { CaptureError } from './error.ts';
import { chromiumPayload, elementId, htmlPagesIn, pagePath } from './fixture-pages.ts';
import { MAX_FOCUS_STEPS } from './focus.ts';
import { normaliseForGolden } from './normalise.ts';

/** Where focus lands after each Tab press: an element's `id`, or `null` when it left the document. */
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
  'focus/trap.html': ['first', 'middle', 'last', 'first'],
  'focus/long.html': Array.from(
    { length: MAX_FOCUS_STEPS },
    (_, index) => `b${String(index + 1).padStart(3, '0')}`,
  ),
};

describe.each(htmlPagesIn('focus/'))('focus walk of %s', (page) => {
  test('lands on the expected elements in order', async () => {
    const expected = expectedFocus[page];
    expect(expected, `${page} has no entry in expectedFocus`).toBeDefined();
    const snapshot = await captureChromium(pagePath(page));
    const steps = chromiumPayload(snapshot).focus.steps;
    expect(
      steps.map((step) => (step.node === undefined ? null : (elementId(snapshot, step.node) ?? '(no id)'))),
    ).toEqual(expected);
  });
});

describe('focus walk stop rules', () => {
  test('a trap reports a cycle with no step outside the document before it', async () => {
    const { focus } = chromiumPayload(await captureChromium(pagePath('focus/trap.html')));
    expect(focus.truncated).toBe(false);
    expect(focus.cycle_start).toBeTypeOf('number');
    expect(focus.steps.slice(0, focus.cycle_start).some((step) => step.node === undefined)).toBe(false);
  });

  test('a page with more stops than the cap is truncated at the cap with no cycle', async () => {
    const { focus } = chromiumPayload(await captureChromium(pagePath('focus/long.html')));
    expect(focus.steps).toHaveLength(MAX_FOCUS_STEPS);
    expect(focus.truncated).toBe(true);
    expect(focus).not.toHaveProperty('cycle_start');
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
