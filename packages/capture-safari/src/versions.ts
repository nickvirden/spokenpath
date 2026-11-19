import { execFile } from 'node:child_process';
import { readFileSync } from 'node:fs';

/** This package's version, written into every snapshot's `producer`. */
export const producerVersion: string = (() => {
  const manifest: unknown = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
  const version =
    typeof manifest === 'object' && manifest !== null
      ? (manifest as Record<string, unknown>).version
      : undefined;
  if (typeof version !== 'string') {
    throw new Error('@spokenpath/capture-safari has no version');
  }
  return version;
})();

/** The Safari and macOS builds a capture records; a field is absent when it cannot be read. */
export interface HostVersions {
  safariBuild?: string;
  macosVersion?: string;
  macosBuild?: string;
}

/** Where Safari's bundle information is. */
const SAFARI_INFO_PLIST = '/Applications/Safari.app/Contents/Info.plist';

function output(command: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(command, args, { timeout: 10_000, encoding: 'utf8' }, (error, stdout) => {
      if (error !== null) {
        reject(error);
        return;
      }
      resolve(stdout.trim());
    });
  });
}

/**
 * Reads Safari's `CFBundleVersion` with `defaults read` and the macOS product and build versions
 * with `sw_vers`. A version that cannot be read is left out and named in `unread`, so the capture
 * can record why.
 */
export async function readHostVersions(): Promise<{ versions: HostVersions; unread: string[] }> {
  const versions: HostVersions = {};
  const unread: string[] = [];
  const reads: [keyof HostVersions, string, string[]][] = [
    ['safariBuild', 'defaults', ['read', SAFARI_INFO_PLIST, 'CFBundleVersion']],
    ['macosVersion', 'sw_vers', ['-productVersion']],
    ['macosBuild', 'sw_vers', ['-buildVersion']],
  ];
  for (const [field, command, args] of reads) {
    try {
      const value = await output(command, args);
      if (value === '') {
        unread.push(`${command} ${args.join(' ')} printed nothing`);
      } else {
        versions[field] = value;
      }
    } catch (error) {
      unread.push(
        `${command} ${args.join(' ')} failed: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }
  return { versions, unread };
}
