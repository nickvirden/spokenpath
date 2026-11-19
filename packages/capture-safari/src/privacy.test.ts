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

/**
 * The running machine's user and host names, besides the paths they usually appear in. A short
 * name such as `build` occurs inside field names like `safari_build`, so only names longer than
 * six characters are looked for, and only as whole words.
 */
const machineNames = [userInfo().username, hostname()].filter((name) => name.length > 6);

function wholeWord(name: string): RegExp {
  return new RegExp(`(?<![A-Za-z0-9_])${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![A-Za-z0-9_])`, 'i');
}

test('no committed Safari snapshot holds a home directory, a user name or a file URL', () => {
  const files = committedSafariSnapshots();
  expect(files.some((file) => file.includes('safari-webdriver/'))).toBe(true);
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    for (const marker of ['/Users/', '/home/', 'file:']) {
      expect(text.includes(marker), `${file} holds ${marker}`).toBe(false);
    }
    for (const name of machineNames) {
      expect(wholeWord(name).test(text), `${file} holds ${name}`).toBe(false);
    }
    const { capture } = JSON.parse(text) as { capture: { url: string } };
    expect(new URL(capture.url).hostname, file).toBe('127.0.0.1');
  }
});
