/**
 * Runs one capture in its own process, for the tests that signal it: `node capture-runner.ts
 * <page> <driver.json>`, where the JSON is a `TestDriver` with an optional `timeoutMs`. It prints
 * the error code or `ok`, and exits as the command-line tool does after an interrupt: 130 for
 * SIGINT and 143 for SIGTERM.
 */
import { readFileSync } from 'node:fs';
import { captureSafari, type TestDriver } from './capture.ts';
import { CaptureError } from './error.ts';

const [page, driverPath] = process.argv.slice(2);
if (page === undefined || driverPath === undefined) {
  throw new Error('usage: capture-runner.ts <page> <driver.json>');
}
const { timeoutMs, ...driver } = JSON.parse(readFileSync(driverPath, 'utf8')) as TestDriver & {
  timeoutMs?: number;
};
try {
  await captureSafari(page, { driver, ...(timeoutMs === undefined ? {} : { timeoutMs }) });
  process.stdout.write('ok\n');
} catch (error) {
  if (!(error instanceof CaptureError)) {
    throw error;
  }
  process.stdout.write(`${error.code}\n`);
  process.exitCode = error.signal === 'SIGINT' ? 130 : error.signal === 'SIGTERM' ? 143 : 1;
}
