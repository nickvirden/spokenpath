// Asks real Safari, over WebDriver, for the computed role and label of every
// tagged element in the fixture, with the time each call takes. The output is
// raw evidence for docs/decisions/ADR-0009.
//
// Exit status: 0 once a session started and the result is written, even when
// single elements error; 1 when safaridriver or the session cannot start.
import { writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import {
  command,
  elementId,
  macosVersion,
  newSession,
  safariVersion,
  startFixtureServer,
  startSafaridriver,
} from './webdriver.mjs';

const outputFile = fileURLToPath(new URL('./output/webdriver.json', import.meta.url));
const fixturePath = '/tagged.html';

async function timed(run) {
  const start = performance.now();
  try {
    return { value: await run(), ms: Number((performance.now() - start).toFixed(1)) };
  } catch (error) {
    return {
      error: error.body?.value ?? { message: error.message },
      ms: Number((performance.now() - start).toFixed(1)),
    };
  }
}

export async function tagSequence(base, sessionId) {
  const references = await command(base, 'POST', `/session/${sessionId}/elements`, {
    using: 'css selector',
    value: '[data-spokenpath-src]',
  });
  const elements = [];
  for (const reference of references) {
    const path = `/session/${sessionId}/element/${elementId(reference)}`;
    const src = await timed(() => command(base, 'GET', `${path}/attribute/data-spokenpath-src`));
    const tag = await timed(() => command(base, 'GET', `${path}/name`));
    const role = await timed(() => command(base, 'GET', `${path}/computedrole`));
    const label = await timed(() => command(base, 'GET', `${path}/computedlabel`));
    elements.push({
      src: src.value,
      tag: tag.value,
      role: role.value,
      roleError: role.error,
      roleMs: role.ms,
      label: label.value,
      labelError: label.error,
      labelMs: label.ms,
    });
  }
  return elements;
}

async function main() {
  const started = performance.now();
  const result = {
    probe: 'safari-webdriver',
    fixture: fixturePath,
    environment: {
      macos: macosVersion(),
      safari: safariVersion(),
      node: process.version,
      runDate: new Date().toISOString().slice(0, 10),
    },
  };
  const server = await startFixtureServer();
  let driver;
  let session;
  try {
    try {
      driver = await startSafaridriver();
      session = await newSession(driver.base);
    } catch (error) {
      result.sessionStarted = false;
      result.startError = error.body?.value ?? { message: error.message };
      return 1;
    }
    result.sessionStarted = true;
    result.capabilities = session.capabilities;
    await command(driver.base, 'POST', `/session/${session.id}/url`, {
      url: `${server.origin}${fixturePath}`,
    });
    result.elements = await tagSequence(driver.base, session.id);
    result.elementCount = result.elements.length;
    result.erroredOrEmpty = result.elements
      .filter((element) => element.roleError || element.labelError || !element.role || !element.label)
      .map(({ src, tag, role, label, roleError, labelError }) => ({
        src,
        tag,
        role,
        label,
        roleError,
        labelError,
      }));
    return 0;
  } catch (error) {
    // The session started, so a later failure is a recorded result.
    result.runError = error.body?.value ?? { message: error.message };
    return 0;
  } finally {
    if (session) {
      const deleted = await timed(() => command(driver.base, 'DELETE', `/session/${session.id}`));
      result.sessionDeleted = !deleted.error;
    }
    await driver?.stop();
    await server.close();
    result.totalMs = Number((performance.now() - started).toFixed(1));
    await writeFile(outputFile, `${JSON.stringify(result, null, 2)}\n`);
    console.log(
      `session started: ${result.sessionStarted}; elements: ${result.elementCount ?? 0}; ` +
        `errored or empty: ${result.erroredOrEmpty?.length ?? 'n/a'}; total ${result.totalMs} ms`,
    );
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exitCode = await main();
}
