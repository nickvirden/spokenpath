import { execFile } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { validateSnapshot } from '@spokenpath/snapshot-types';
import { describe, expect, test } from 'vitest';
import { pagePath } from './fixture-pages.ts';

const cliPath = fileURLToPath(new URL('cli.ts', import.meta.url));

interface Run {
  code: number;
  stdout: string;
  stderr: string;
}

function runCli(args: string[]): Promise<Run> {
  return new Promise((resolve, reject) => {
    execFile(
      process.execPath,
      [cliPath, ...args],
      { maxBuffer: 64 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error !== null && typeof error.code !== 'number') {
          reject(error);
          return;
        }
        resolve({ code: error === null ? 0 : (error.code as number), stdout, stderr });
      },
    );
  });
}

test('captures a page to stdout as a valid snapshot', async () => {
  const { code, stdout, stderr } = await runCli([pagePath('sign-in.html')]);
  expect(stderr).toBe('');
  expect(code).toBe(0);
  expect(stdout.endsWith('}\n')).toBe(true);
  const result = validateSnapshot(JSON.parse(stdout));
  expect(result.ok ? [] : result.errors).toEqual([]);
});

test('verifies instrumentation with one line and exit code 0', async () => {
  const { code, stdout } = await runCli([pagePath('sign-in.html'), '--verify-instrumentation']);
  expect(code).toBe(0);
  expect(stdout.trimEnd().split('\n')).toHaveLength(1);
  expect(stdout).toContain('same DOM');
});

test('exits 1 naming the instrumenter error code for a refused page', async () => {
  const { code, stdout, stderr } = await runCli([
    pagePath('torture/rejected/attribute-present.existing-source-attribute.html'),
  ]);
  expect(code).toBe(1);
  expect(stdout).toBe('');
  expect(stderr).toContain('attribute-present');
});

describe('usage errors exit 2', () => {
  test.each([
    ['no page', []],
    ['two pages', [pagePath('sign-in.html'), pagePath('local-asset.html')]],
    ['an unknown flag', [pagePath('sign-in.html'), '--max-tab-stops', '5']],
    ['a flag missing its value', [pagePath('sign-in.html'), '--out']],
    [
      '--out with --verify-instrumentation',
      [pagePath('sign-in.html'), '--verify-instrumentation', '--out', 'x'],
    ],
  ])('%s', async (_name, args) => {
    const { code, stdout, stderr } = await runCli(args);
    expect(code).toBe(2);
    expect(stdout).toBe('');
    expect(stderr).toContain('usage:');
  });
});
