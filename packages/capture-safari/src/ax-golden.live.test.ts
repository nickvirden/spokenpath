import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { capturePages, pagePath } from '@spokenpath/capture-chromium/fixture-pages';
import { type Snapshot, validateSnapshot } from '@spokenpath/snapshot-types';
import { describe, expect, test } from 'vitest';
import { captureSafariAx } from './capture-ax.ts';
import { CaptureError } from './error.ts';
import { normaliseSafariAxForGolden } from './normalise.ts';

const UPDATE_COMMAND = 'UPDATE_FIXTURES=1 pnpm test:safari';

/** The committed Safari AX goldens, one per page in `capturePages`, at the page's relative path. */
const goldensUrl = new URL('../../../fixtures/snapshots/safari-ax/', import.meta.url);

/** The how-to clause every missing-permission message ends with. */
const PERMISSION_HOW_TO =
  'Grant the Accessibility permission to the app that runs spokenpath, such as Terminal, in System Settings > Privacy & Security > Accessibility, then run the capture again; see "Granting the Accessibility permission" in docs/usage.md.';

// Safari allows one automation session, so the pages run one after another.
describe.each(capturePages)('%s', (page) => {
  test("captures Safari's macOS accessibility tree as a valid snapshot that normalises to its committed golden", async () => {
    const snapshot: Snapshot = await captureSafariAx(pagePath(page));
    const result = validateSnapshot(snapshot);
    expect(result.ok ? [] : result.errors).toEqual([]);
    const normalised = normaliseSafariAxForGolden(snapshot);
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

test('a check page without its check nodes fails as a missing permission, with no snapshot', async () => {
  let snapshot: Snapshot | undefined;
  let failure: unknown;
  try {
    snapshot = await captureSafariAx(pagePath('sign-in.html'), {
      canaryPage:
        '<!doctype html>\n<title>spokenpath check without check nodes</title>\n<p>No check tokens.</p>\n',
    });
  } catch (error) {
    failure = error;
  }
  expect(snapshot).toBeUndefined();
  expect(failure).toBeInstanceOf(CaptureError);
  const error = failure as CaptureError;
  expect(error.code).toBe('accessibility-permission-missing');
  expect(error.message.endsWith(` ${PERMISSION_HOW_TO}`), error.message).toBe(true);
  // The run's output carries the message, as the evidence of what the case printed.
  console.log(`forced check page: ${error.code}: ${error.message}`);
});
