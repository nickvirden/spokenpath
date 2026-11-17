// Starts VoiceOver through Guidepup, moves through a three-element page in
// Safari with a fixed key-press script, and records the spoken-phrase log of
// two separate VoiceOver sessions, so the two runs can be compared phrase by
// phrase. The output is raw evidence for docs/decisions/ADR-0010.
//
// It never runs Guidepup's setup, never uses sudo and sets no VoiceOver
// option. VoiceOver is stopped in a `finally` after every run, and the
// probe refuses to start when a VoiceOver it did not start is running. A run
// that errors is recorded with its error and is not retried.
//
// The page opens in a new Safari window that the probe closes again, so
// VoiceOver never reads the maintainer's own windows, and only phrases from
// the fixture heading onwards are kept.
//
// Everything after `voiceOver.start()` succeeds has not yet run on a real
// machine (start-up has failed so far), so a first real run may need fixes.
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { homedir, hostname, release, userInfo } from 'node:os';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import guidepup from '@guidepup/guidepup';

const { voiceOver } = guidepup;
const pagePath = fileURLToPath(new URL('./pages/three-elements.html', import.meta.url));
const outputFile = fileURLToPath(new URL('./output/speech-runs.json', import.meta.url));
const runs = 2;
// The key-press script: each step is one VoiceOver cursor move.
const script = ['next', 'next', 'next', 'next'];
const overallDeadlineMs = 180_000;
const pageTitle = 'Three elements';
// The first phrase kept is the one that speaks the fixture's heading.
const fixtureHeading = 'Probe heading';
const frontWindowPollMs = 250;

// Committed output must not identify the machine.
function scrub(text) {
  return [homedir(), userInfo().username, hostname()]
    .filter(Boolean)
    .reduce((current, secret) => current.split(secret).join('<redacted>'), String(text));
}

function errorChain(error) {
  const chain = [];
  for (let current = error; current; current = current.cause) {
    chain.push(scrub(current.message ?? current));
  }
  return chain;
}

const voiceOverRunning = () => spawnSync('pgrep', ['-x', 'VoiceOver']).status === 0;

function plistValue(plist, key) {
  return execFileSync('defaults', ['read', plist, key], { encoding: 'utf8' }).trim();
}

async function guidepupVersion() {
  const manifest = new URL('../node_modules/@guidepup/guidepup/package.json', import.meta.url);
  return JSON.parse(await readFile(manifest, 'utf8')).version;
}

// Guidepup starts VoiceOver with a preferences image that `@guidepup/setup
// install` downloads into Guidepup's cache, one per macOS major version. Its
// presence is recorded (as a boolean, never as a path), read-only, because
// start() fails without it.
async function preferencesAsset() {
  const manifestUrl = new URL('../node_modules/@guidepup/guidepup/manifest.json', import.meta.url);
  const manifest = JSON.parse(await readFile(manifestUrl, 'utf8'));
  const platformVersion = release().split('.', 1)[0];
  const asset = manifest.screenReaders
    .find(({ id }) => id === 'voiceover')
    ?.assets.find((entry) => entry.platformVersion === platformVersion);
  if (!asset) return { platformVersion, listedInManifest: false };
  const cache = process.env.GUIDEPUP_SCREEN_READERS_PATH ?? join(homedir(), 'Library', 'Caches', 'guidepup');
  return {
    platformVersion,
    listedInManifest: true,
    asset: asset.asset,
    version: asset.version,
    installed: existsSync(join(cache, 'voiceover', platformVersion, asset.version, asset.asset)),
  };
}

async function startPageServer() {
  const html = await readFile(pagePath);
  const server = createServer((request, response) => {
    if (new URL(request.url, 'http://127.0.0.1').pathname !== '/three-elements.html') {
      response.writeHead(404, { 'content-type': 'text/plain' });
      response.end('not found');
      return;
    }
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    response.end(html);
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  return {
    url: `http://127.0.0.1:${server.address().port}/three-elements.html`,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}

function safari(script) {
  return execFileSync('osascript', ['-e', `tell application "Safari" to ${script}`], {
    encoding: 'utf8',
  }).trim();
}

// The id of the Safari window this probe opened, so it alone is closed, on
// the normal path and on the deadline path.
let fixtureWindowId;

function openFixtureWindow(url) {
  safari(`make new document with properties {URL:"${url}"}`);
  fixtureWindowId = safari('id of front window');
}

function closeFixtureWindow(run) {
  if (fixtureWindowId === undefined) return;
  try {
    safari(`close (every window whose id is ${fixtureWindowId})`);
  } catch (error) {
    run.closeWindowError = errorChain(error);
  }
  fixtureWindowId = undefined;
}

// Waits until the fixture is Safari's front window; the overall deadline
// bounds the wait.
async function waitForFixtureInFront(run) {
  for (;;) {
    const title = safari('name of front window');
    if (title === pageTitle) {
      run.frontWindowTitle = title;
      return;
    }
    await sleep(frontWindowPollMs);
  }
}

// Phrases spoken before the fixture heading can come from outside the page,
// so only the count of those is recorded, never their text.
function keepFromHeading(log) {
  const start = log.findIndex((phrase) => phrase.includes(fixtureHeading));
  if (start === -1) return { kept: [], droppedBeforeHeading: log.length, headingFound: false };
  return { kept: log.slice(start), droppedBeforeHeading: start, headingFound: true };
}

// Whether a VoiceOver this probe started, or began starting, still needs
// stopping.
let voiceOverActive = false;

async function stopVoiceOver(run) {
  if (!voiceOverActive) return;
  voiceOverActive = false;
  await voiceOver.stop().catch((error) => {
    run.stopError = errorChain(error);
  });
}

// The run object belongs to the result from the start, so the deadline path
// writes whatever it has gathered.
async function speechRun(run, url) {
  try {
    // Set before start() resolves: a start-up that hangs may already have
    // launched VoiceOver, so the deadline path stops it too.
    // A start() that rejects has already torn its own VoiceOver down.
    voiceOverActive = true;
    await voiceOver.start().catch((error) => {
      voiceOverActive = false;
      throw error;
    });
    run.started = true;
    openFixtureWindow(url);
    await waitForFixtureInFront(run);
    await voiceOver.interact();
    await voiceOver.clearSpokenPhraseLog();
    for (const step of script) await voiceOver[step]();
    const { kept, droppedBeforeHeading, headingFound } = keepFromHeading(await voiceOver.spokenPhraseLog());
    run.spokenPhraseLog = kept;
    run.droppedBeforeHeading = droppedBeforeHeading;
    run.headingFound = headingFound;
  } catch (error) {
    run.error = errorChain(error);
  } finally {
    closeFixtureWindow(run);
    await stopVoiceOver(run);
    run.voiceOverRunningAfterStop = voiceOverRunning();
  }
}

function compare([first, second]) {
  if (!first?.spokenPhraseLog || !second?.spokenPhraseLog) return undefined;
  const length = Math.max(first.spokenPhraseLog.length, second.spokenPhraseLog.length);
  const phrases = Array.from({ length }, (_, index) => ({
    first: first.spokenPhraseLog[index],
    second: second.spokenPhraseLog[index],
    agree: first.spokenPhraseLog[index] === second.spokenPhraseLog[index],
  }));
  return { phrases: length, agreeing: phrases.filter((phrase) => phrase.agree).length, perPhrase: phrases };
}

// The one serialisation of the result, shared by the normal and the deadline
// path so both write the same shape.
async function writeResult(result) {
  result.voiceOverRunningAtEnd = voiceOverRunning();
  await writeFile(outputFile, `${JSON.stringify(result, null, 2)}\n`);
}

async function main() {
  const safariPlist = '/Applications/Safari.app/Contents/Info.plist';
  const voiceOverPlist = '/System/Library/CoreServices/VoiceOver.app/Contents/Info.plist';
  const read = (key) => execFileSync('sw_vers', [key], { encoding: 'utf8' }).trim();
  const result = {
    probe: 'voiceover-guidepup',
    page: 'three-elements.html',
    environment: {
      macos: { productVersion: read('-productVersion'), buildVersion: read('-buildVersion') },
      safari: plistValue(safariPlist, 'CFBundleShortVersionString'),
      voiceOver: {
        shortVersion: plistValue(voiceOverPlist, 'CFBundleShortVersionString'),
        bundleVersion: plistValue(voiceOverPlist, 'CFBundleVersion'),
      },
      guidepup: await guidepupVersion(),
      node: process.version,
      runDate: new Date().toISOString().slice(0, 10),
    },
    preferencesAsset: await preferencesAsset(),
    completed: false,
    runs: [],
  };
  // Written before anything else, so an interrupted run never leaves an
  // earlier run's file looking current.
  await writeResult(result);
  if (voiceOverRunning()) {
    result.refused = 'VoiceOver was already running and was not started by this probe';
    result.completed = true;
    await writeResult(result);
    return 1;
  }
  const server = await startPageServer();
  const deadline = setTimeout(async () => {
    result.deadlineExceeded = true;
    const current = result.runs.at(-1) ?? {};
    closeFixtureWindow(current);
    await stopVoiceOver(current);
    await writeResult(result);
    process.exit(1);
  }, overallDeadlineMs);
  try {
    for (let index = 0; index < runs; index += 1) {
      const run = { steps: script };
      result.runs.push(run);
      await speechRun(run, server.url);
      // A run that cannot start VoiceOver would fail the same way again.
      if (run.error) break;
    }
    result.agreement = compare(result.runs);
    result.completed = true;
  } finally {
    clearTimeout(deadline);
    await server.close();
    await writeResult(result);
  }
  const [first] = result.runs;
  console.log(
    first?.error
      ? `run 1 failed: ${first.error.join(' <- ')}`
      : `${result.runs.length} runs; agreement ${result.agreement?.agreeing} of ${result.agreement?.phrases}`,
  );
  console.log(`VoiceOver running at end: ${result.voiceOverRunningAtEnd}`);
  return result.runs.some((run) => run.error) ? 1 : 0;
}

process.exitCode = await main();
