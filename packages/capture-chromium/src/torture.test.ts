import { type InstrumentHtmlErrorCode, InstrumentHtmlError } from '@spokenpath/instrument-html';
import { validateSnapshot } from '@spokenpath/snapshot-types';
import { describe, expect, test } from 'vitest';
import { captureChromium, verifyInstrumentation } from './capture.ts';
import { capturePages, htmlPagesIn, pageBytes, pagePath, sourceTaggingProblems } from './fixture-pages.ts';

const torturePages = htmlPagesIn('torture/');
const rejectedPages = htmlPagesIn('torture/rejected/');

test('the torture directories hold pages', () => {
  expect(torturePages.length).toBeGreaterThan(0);
  expect(rejectedPages.length).toBeGreaterThan(0);
});

describe.each(torturePages)('%s', (page) => {
  test('captures a valid snapshot whose source elements locate their tags', async () => {
    const snapshot = await captureChromium(pagePath(page));
    const result = validateSnapshot(snapshot);
    expect(result.ok ? [] : result.errors).toEqual([]);
    expect(sourceTaggingProblems(snapshot, pageBytes(page))).toEqual([]);
  });
});

describe.each([...torturePages, ...capturePages])('%s', (page) => {
  test('builds the same DOM in Chromium once instrumented', async () => {
    expect(await verifyInstrumentation(pagePath(page))).toEqual({ equal: true });
  });
});

/** The code a rejected page must raise: its file name up to the first dot. */
function expectedCode(page: string): InstrumentHtmlErrorCode {
  const name = page.slice(page.lastIndexOf('/') + 1);
  const code = name.slice(0, name.indexOf('.'));
  const codes: InstrumentHtmlErrorCode[] = [
    'invalid-utf-8',
    'unsupported-encoding',
    'attribute-present',
    'dom-changed',
  ];
  const known = codes.find((candidate) => candidate === code);
  if (known === undefined) {
    throw new Error(`${page} does not start with an error code`);
  }
  return known;
}

describe.each(rejectedPages)('%s', (page) => {
  test('is refused with the code its name gives', async () => {
    const capture = captureChromium(pagePath(page));
    await expect(capture).rejects.toBeInstanceOf(InstrumentHtmlError);
    await expect(capture).rejects.toMatchObject({ code: expectedCode(page) });
  });
});
