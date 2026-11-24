import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import type { Snapshot } from '@spokenpath/snapshot-types';
import { expect, test } from 'vitest';
import { EXPOSED_IN_CHROMIUM, mappingReport, type PageMapping, pageMapping } from './source-mapping.ts';

const UPDATE_COMMAND = 'UPDATE_FIXTURES=1 pnpm --filter @spokenpath/capture-safari test';

const snapshotsUrl = new URL('../../../fixtures/snapshots/', import.meta.url);
const axGoldensUrl = new URL('safari-ax/', snapshotsUrl);
const reportUrl = new URL('source-mapping.json', axGoldensUrl);
const pagesUrl = new URL('../../../fixtures/pages/', import.meta.url);

/** The least share of the spans Chromium exposes that Safari's tree must carry, over every page. */
const MINIMUM_FIGURE = 0.95;

/** Every committed `safari-ax` golden, by its page relative to `fixtures/pages/`. */
function axGoldenPages(): string[] {
  if (!existsSync(axGoldensUrl)) {
    return [];
  }
  return readdirSync(axGoldensUrl, { recursive: true, encoding: 'utf8' })
    .filter((path) => path.endsWith('.json') && path !== 'source-mapping.json')
    .map((path) => path.replace(/\.json$/, '.html'))
    .sort();
}

function readSnapshot(url: URL): Snapshot {
  return JSON.parse(readFileSync(url, 'utf8')) as Snapshot;
}

function report() {
  const pages: Record<string, PageMapping> = {};
  for (const page of axGoldenPages()) {
    const json = page.replace(/\.html$/, '.json');
    pages[page] = pageMapping(
      readSnapshot(new URL(json, axGoldensUrl)),
      readSnapshot(new URL(`chromium/${json}`, snapshotsUrl)),
      readFileSync(new URL(page, pagesUrl)),
    );
  }
  return mappingReport(pages);
}

test("Safari's macOS accessibility tree carries the source location of at least 95% of the elements Chromium exposes", () => {
  const pages = axGoldenPages();
  expect(pages.length, 'the safari-ax goldens are committed').toBeGreaterThan(0);
  const measured = report();
  if (process.env.UPDATE_FIXTURES === '1') {
    writeFileSync(reportUrl, `${JSON.stringify(measured, null, 2)}\n`);
  }
  expect(existsSync(reportUrl), `no source-mapping report; write it with \`${UPDATE_COMMAND}\``).toBe(true);
  expect(
    measured,
    `the measured mapping differs from source-mapping.json; review it, then refresh it with \`${UPDATE_COMMAND}\``,
  ).toEqual(JSON.parse(readFileSync(reportUrl, 'utf8')));
  expect(measured.total.figure).toBeGreaterThanOrEqual(MINIMUM_FIGURE);
  for (const mapping of Object.values(measured.pages)) {
    expect(mapping.mapped + mapping.not_in_tree.length + mapping.unmapped_in_tree.length).toBe(
      mapping.tagged,
    );
    for (const span of mapping.unmapped_in_tree) {
      expect(span.reason).toBe(EXPOSED_IN_CHROMIUM);
    }
  }
});
