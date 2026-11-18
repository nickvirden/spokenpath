import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { type Snapshot, validateSnapshot } from '@spokenpath/snapshot-types';
import { describe, expect, test } from 'vitest';
import { captureChromium } from './capture.ts';
import { capturePages, goldensUrl, pageBytes, pagePath, sourceTaggingProblems } from './fixture-pages.ts';
import { normaliseForGolden } from './normalise.ts';

const UPDATE_COMMAND = 'UPDATE_FIXTURES=1 pnpm test:capture';

test('the capture fixture directories hold pages', () => {
  expect(capturePages).toContain('sign-in.html');
  expect(capturePages.some((page) => page.startsWith('focus/'))).toBe(true);
});

describe.each(capturePages)('%s', (page) => {
  // Captured once, by the first test that needs it, and shared by the tests below.
  let captured: Promise<Snapshot> | undefined;
  const capture = () => {
    captured ??= captureChromium(pagePath(page));
    return captured;
  };

  test('captures a valid snapshot that carries no source attribute in the DOM', async () => {
    const snapshot = await capture();
    const result = validateSnapshot(snapshot);
    expect(result.ok ? [] : result.errors).toEqual([]);
    expect(sourceTaggingProblems(snapshot, pageBytes(page))).toEqual([]);
  });

  test('normalises to its committed golden', async () => {
    const normalised = normaliseForGolden(await capture());
    const goldenUrl = new URL(page.replace(/\.html$/, '.json'), goldensUrl);
    if (process.env.UPDATE_FIXTURES === '1') {
      mkdirSync(new URL('.', goldenUrl), { recursive: true });
      writeFileSync(goldenUrl, `${JSON.stringify(normalised, null, 2)}\n`);
    }
    expect(existsSync(goldenUrl), `no golden for ${page}; write it with \`${UPDATE_COMMAND}\``).toBe(true);
    const golden: unknown = JSON.parse(readFileSync(goldenUrl, 'utf8'));
    expect(
      normalised,
      `the capture differs from its golden; review the change, then refresh it with \`${UPDATE_COMMAND}\``,
    ).toEqual(golden);
  });
});
