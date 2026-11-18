// Checks, read-only, whether Guidepup's macOS setup has been run: the two
// switches that let a process drive VoiceOver through AppleScript. It never
// writes a setting, never starts VoiceOver and never runs the setup. The
// output is raw evidence for docs/decisions/ADR-0010.
import { execFileSync, spawnSync } from 'node:child_process';
import { statSync } from 'node:fs';
import { writeFile } from 'node:fs/promises';
import { homedir, hostname, userInfo } from 'node:os';
import { fileURLToPath } from 'node:url';

const outputFile = fileURLToPath(new URL('./output/setup-check.json', import.meta.url));
const markerFile = '/private/var/db/Accessibility/.VoiceOverAppleScriptEnabled';
const defaultsDomain = 'com.apple.VoiceOver4/default';
const defaultsKey = 'SCREnableAppleScript';
const voiceOverPlist = '/System/Library/CoreServices/VoiceOver.app/Contents/Info.plist';

// Committed output must not identify the machine, so the home directory, user
// name and host name are replaced wherever a command echoes them.
function scrub(text) {
  return [homedir(), userInfo().username, hostname()]
    .filter(Boolean)
    .reduce((current, secret) => current.split(secret).join('<redacted>'), text);
}

function run(file, args) {
  const result = spawnSync(file, args, { encoding: 'utf8' });
  if (result.error) return { command: [file, ...args].join(' '), error: result.error.message };
  return {
    command: [file, ...args].join(' '),
    exitCode: result.status,
    stdout: scrub(result.stdout.trim()),
    stderr: scrub(result.stderr.trim()),
  };
}

function markerState() {
  try {
    statSync(markerFile);
    return { exists: true };
  } catch (error) {
    return { exists: false, code: error.code };
  }
}

function plistValue(plist, key) {
  return execFileSync('defaults', ['read', plist, key], { encoding: 'utf8' }).trim();
}

const read = (key) => execFileSync('sw_vers', [key], { encoding: 'utf8' }).trim();
const defaultsRead = run('defaults', ['read', defaultsDomain, defaultsKey]);
const marker = markerState();
const voiceOverRunning = run('pgrep', ['-x', 'VoiceOver']);
const setupRun = marker.exists && defaultsRead.exitCode === 0 && defaultsRead.stdout === '1';

const result = {
  probe: 'voiceover-guidepup',
  environment: {
    macos: { productVersion: read('-productVersion'), buildVersion: read('-buildVersion') },
    safari: plistValue('/Applications/Safari.app/Contents/Info.plist', 'CFBundleShortVersionString'),
    voiceOver: {
      shortVersion: plistValue(voiceOverPlist, 'CFBundleShortVersionString'),
      bundleVersion: plistValue(voiceOverPlist, 'CFBundleVersion'),
    },
    node: process.version,
    runDate: new Date().toISOString().slice(0, 10),
  },
  appleScriptMarkerFile: { path: markerFile, ...marker },
  defaultsRead,
  voiceOverRunning: { ...voiceOverRunning, running: voiceOverRunning.exitCode === 0 },
  setupRun,
  result: setupRun ? 'setup run: VoiceOver AppleScript control is enabled' : 'not run: setup pending',
};
await writeFile(outputFile, `${JSON.stringify(result, null, 2)}\n`);
console.log(
  `${result.result} (marker file exists: ${marker.exists}; defaults exit ${defaultsRead.exitCode})`,
);
