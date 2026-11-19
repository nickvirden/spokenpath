import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { capturePages, pagePath } from '@spokenpath/capture-chromium/fixture-pages';
import { type Snapshot, validateSnapshot } from '@spokenpath/snapshot-types';
import { describe, expect, test } from 'vitest';
import { captureSafari } from './capture.ts';
import { normaliseSafariForGolden } from './normalise.ts';

const UPDATE_COMMAND = 'UPDATE_FIXTURES=1 pnpm test:safari';

/** The committed Safari goldens, one per page in `capturePages`, at the page's relative path. */
const goldensUrl = new URL('../../../fixtures/snapshots/safari-webdriver/', import.meta.url);

test('the live pages include the cross-check pages', () => {
  expect(capturePages.some((page) => page.startsWith('cross-check/'))).toBe(true);
});

// Safari allows one automation session, so the pages run one after another.
describe.each(capturePages)('%s', (page) => {
  test('captures a valid snapshot that normalises to its committed golden', async () => {
    const snapshot: Snapshot = await captureSafari(pagePath(page));
    const result = validateSnapshot(snapshot);
    expect(result.ok ? [] : result.errors).toEqual([]);
    const normalised = normaliseSafariForGolden(snapshot);
    const goldenUrl = new URL(page.replace(/\.html$/, '.json'), goldensUrl);
    if (process.env.UPDATE_FIXTURES === '1') {
      mkdirSync(new URL('.', goldenUrl), { recursive: true });
      writeFileSync(goldenUrl, `${JSON.stringify(normalised, null, 2)}\n`);
    }
    expect(existsSync(goldenUrl), `no golden for ${page}; write it with \`${UPDATE_COMMAND}\``).toBe(true);
    const golden: unknown = JSON.parse(readFileSync(goldenUrl, 'utf8'));
    expect(
      normalised,
      `the capture differs from its golden; review the change as drift, then refresh it with \`${UPDATE_COMMAND}\``,
    ).toEqual(golden);
  });
});
