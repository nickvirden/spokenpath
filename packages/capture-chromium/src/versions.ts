import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';

function manifestVersion(path: string | URL): string {
  const manifest: unknown = JSON.parse(readFileSync(path, 'utf8'));
  const version =
    typeof manifest === 'object' && manifest !== null
      ? (manifest as Record<string, unknown>).version
      : undefined;
  if (typeof version !== 'string') {
    throw new Error(`${String(path)} has no version`);
  }
  return version;
}

/** This package's version, written into every snapshot's `producer`. */
export const producerVersion: string = manifestVersion(new URL('../package.json', import.meta.url));

/** The installed playwright-core version, which also fixes the Chromium build it drives. */
export const playwrightVersion: string = manifestVersion(
  createRequire(import.meta.url).resolve('playwright-core/package.json'),
);
