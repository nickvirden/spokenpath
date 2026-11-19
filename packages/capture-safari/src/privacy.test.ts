import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { hostname, userInfo } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test } from 'vitest';

const snapshots = fileURLToPath(new URL('../../../fixtures/snapshots/', import.meta.url));

/** Every committed Safari snapshot: the live goldens and the hand-written fixture. */
function committedSafariSnapshots(): string[] {
  const goldens = join(snapshots, 'safari-webdriver');
  const live = existsSync(goldens)
    ? readdirSync(goldens, { recursive: true, encoding: 'utf8' })
        .filter((path) => path.endsWith('.json'))
        .map((path) => join(goldens, path))
    : [];
  return [...live, join(snapshots, 'safari-webdriver-basic.json')].sort();
}

test('no committed Safari snapshot holds a home directory, a user name or a file URL', () => {
  const files = committedSafariSnapshots();
  expect(files.some((file) => file.includes('safari-webdriver/'))).toBe(true);
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    // The capturing Mac's own user and host names, besides the paths they usually appear in.
    for (const marker of ['/Users/', '/home/', 'file:', userInfo().username, hostname()]) {
      expect(text.includes(marker), `${file} holds ${marker}`).toBe(false);
    }
    const { capture } = JSON.parse(text) as { capture: { url: string } };
    expect(new URL(capture.url).hostname, file).toBe('127.0.0.1');
  }
});
