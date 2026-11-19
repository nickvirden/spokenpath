/**
 * Runs one capture in its own process, for the tests that signal it: `node capture-runner.ts
 * <page> <options.json>`, where the JSON holds a `TestDriver` as `driver`, an optional
 * `timeoutMs`, and, for a capture of Safari's macOS accessibility tree, the test-double `helper`.
 * It prints the error code or `ok`, and exits as the command-line tool does after an interrupt:
 * 130 for SIGINT and 143 for SIGTERM.
 */
import { readFileSync } from 'node:fs';
import { captureSafari, type TestDriver } from './capture.ts';
import { captureSafariAx } from './capture-ax.ts';
import { CaptureError } from './error.ts';
import type { HelperCommand } from './helper.ts';

const [page, optionsPath] = process.argv.slice(2);
if (page === undefined || optionsPath === undefined) {
  throw new Error('usage: capture-runner.ts <page> <options.json>');
}
const { driver, helper, timeoutMs } = JSON.parse(readFileSync(optionsPath, 'utf8')) as {
  driver: TestDriver;
  helper?: HelperCommand;
  timeoutMs?: number;
};
const limit = timeoutMs === undefined ? {} : { timeoutMs };
try {
  await (helper === undefined
    ? captureSafari(page, { driver, ...limit })
    : captureSafariAx(page, { driver, helper, ...limit }));
  process.stdout.write('ok\n');
} catch (error) {
  if (!(error instanceof CaptureError)) {
    throw error;
  }
  process.stdout.write(`${error.code}\n`);
  process.exitCode = error.signal === 'SIGINT' ? 130 : error.signal === 'SIGTERM' ? 143 : 1;
}
