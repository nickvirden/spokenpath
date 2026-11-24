import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterAll, describe, expect, test } from 'vitest';
import { CaptureError } from './error.ts';
import { canaryPage, HelperProcesses, requireRunnableHelper } from './helper.ts';

const directory = mkdtempSync(join(tmpdir(), 'spokenpath-helper-test-'));

afterAll(() => {
  rmSync(directory, { recursive: true, force: true });
});

function thrown(run: () => unknown): CaptureError {
  try {
    run();
  } catch (error) {
    if (error instanceof CaptureError) {
      return error;
    }
    throw error;
  }
  throw new Error('nothing was thrown');
}

const missing = join(directory, 'no-such-spokenpath-ax-macos');
const notFoundMessage = `the spokenpath-ax-macos helper is not installed at ${missing}; build it with \`cargo build -p spokenpath_ax_macos\``;

describe('requireRunnableHelper', () => {
  test('refuses a helper file that does not exist with ax-helper-not-found', () => {
    const error = thrown(() => requireRunnableHelper({ command: missing, args: [] }));
    expect(error.code).toBe('ax-helper-not-found');
    expect(error.message).toBe(notFoundMessage);
  });

  test.runIf(process.platform !== 'win32')(
    'refuses a helper file that is not executable with ax-helper-failed, naming the error code',
    () => {
      const command = join(directory, 'spokenpath-ax-macos');
      writeFileSync(command, '#!/bin/sh\n', { mode: 0o600 });
      const error = thrown(() => requireRunnableHelper({ command, args: [] }));
      expect(error.code).toBe('ax-helper-failed');
      expect(error.message).toMatch(/: EACCES$/);
    },
  );

  test('accepts an executable helper', () => {
    expect(() => requireRunnableHelper({ command: process.execPath, args: [] })).not.toThrow();
  });
});

test('a helper that cannot be spawned fails with the same code and message', async () => {
  const run = {
    helper: { command: missing, args: [] },
    timeoutMs: 1_000,
    signal: new AbortController().signal,
    processes: new HelperProcesses(),
  };
  const error = await canaryPage(run).then(
    () => undefined,
    (error: unknown) => error,
  );
  expect(error).toBeInstanceOf(CaptureError);
  expect((error as CaptureError).code).toBe('ax-helper-not-found');
  expect((error as CaptureError).message).toBe(notFoundMessage);
});
