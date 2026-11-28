import {
  CaptureError,
  type CaptureErrorCode as ChromiumCaptureErrorCode,
} from '@spokenpath/capture-chromium';
import { CaptureError as SafariCaptureError, type CaptureErrorCode } from '@spokenpath/capture-safari';
import { InstrumentHtmlError } from '@spokenpath/instrument-html';
import { describe, expect, test } from 'vitest';
import { EXIT, exitCodeOf } from './exit.ts';

/**
 * The exit code of every Safari capture failure. The record names every code, so a code added to
 * `CaptureErrorCode` without a stated exit code fails to type-check.
 */
const SAFARI_EXIT_CODES = {
  'invalid-options': EXIT.failed,
  'unsupported-platform': EXIT.badInput,
  'safaridriver-not-found': EXIT.failed,
  'safari-session-refused': EXIT.failed,
  'page-unreadable': EXIT.badInput,
  'asset-unreadable': EXIT.failed,
  'page-hidden': EXIT.failed,
  'page-left-origin': EXIT.failed,
  'safari-call-failed': EXIT.failed,
  timeout: EXIT.failed,
  interrupted: EXIT.failed,
  'invalid-snapshot': EXIT.failed,
  'accessibility-permission-missing': EXIT.failed,
  'safari-not-running': EXIT.failed,
  'safari-window-not-identified': EXIT.failed,
  'ax-canary-incomplete': EXIT.failed,
  'ax-tree-too-large': EXIT.failed,
  'ax-helper-not-found': EXIT.failed,
  'ax-helper-failed': EXIT.failed,
} as const satisfies Record<CaptureErrorCode, number>;

describe('a Safari capture failure exits by its code', () => {
  test.each(Object.entries(SAFARI_EXIT_CODES))('%s exits %i', (code, exit) => {
    expect(exitCodeOf(new SafariCaptureError(code as CaptureErrorCode, 'a message'))).toBe(exit);
  });

  test.each([
    ['SIGINT', EXIT.sigint],
    ['SIGTERM', EXIT.sigterm],
  ] as const)('interrupted by %s exits %i', (signal, exit) => {
    expect(exitCodeOf(new SafariCaptureError('interrupted', 'stopped', { signal }))).toBe(exit);
  });
});

test('the exit codes are the documented ones', () => {
  expect(EXIT).toEqual({ ok: 0, failed: 1, badInput: 2, sigint: 130, sigterm: 143 });
});

test('a Chromium capture failure exits 2 only for an unreadable page', () => {
  const chromium = (code: ChromiumCaptureErrorCode) => exitCodeOf(new CaptureError(code, 'a message'));
  expect(chromium('page-unreadable')).toBe(EXIT.badInput);
  expect(chromium('timeout')).toBe(EXIT.failed);
});

test('a page the instrumenter refuses exits 2, and any other error exits 1', () => {
  expect(exitCodeOf(new InstrumentHtmlError('attribute-present', 'refused'))).toBe(EXIT.badInput);
  expect(exitCodeOf(new Error('anything else'))).toBe(EXIT.failed);
  expect(exitCodeOf('not an error')).toBe(EXIT.failed);
});
