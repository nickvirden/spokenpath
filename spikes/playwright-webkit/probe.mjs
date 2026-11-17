// Asks Playwright's bundled WebKit build, over the Web Inspector protocol,
// for `DOM.getAccessibilityPropertiesForNode` on each probe element. The
// output is raw evidence for docs/decisions/ADR-0009.
//
// Playwright's public API has no protocol session for WebKit. This script
// reaches the page's inspector session through Playwright internals
// (`page._connection.toImpl(page).delegate._session`), which is unsupported
// API and may change in any Playwright release; a failure to reach it is
// recorded as the result.
import { execFileSync } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { webkit } from 'playwright';

const here = fileURLToPath(new URL('.', import.meta.url));
const pagesDir = join(here, 'pages');
const outputFile = join(here, 'output', 'accessibility-properties.json');
const origin = 'http://spokenpath.test';
const pageName = 'accessibility-properties.html';
const contentTypes = { '.html': 'text/html; charset=utf-8', '.svg': 'image/svg+xml' };
const route = 'page._connection.toImpl(page).delegate._session (Playwright internals; unsupported API)';

function macosVersion() {
  const read = (key) => execFileSync('sw_vers', [key], { encoding: 'utf8' }).trim();
  return { productVersion: read('-productVersion'), buildVersion: read('-buildVersion') };
}

async function playwrightVersion() {
  const manifest = new URL('../node_modules/playwright/package.json', import.meta.url);
  return JSON.parse(await readFile(manifest, 'utf8')).version;
}

// Every request is answered from pages/ on a fictional origin, and anything
// else is aborted, so the probe page can never reach the network.
async function serveProbePages(context) {
  await context.route('**/*', async (request) => {
    const url = new URL(request.request().url());
    const name = url.pathname.slice(1);
    const type = contentTypes[extname(name)];
    if (url.origin !== origin) {
      await request.abort('blockedbyclient');
      return;
    }
    if (!type || name.includes('/')) {
      await request.fulfill({ status: 404, body: 'not found' });
      return;
    }
    await request.fulfill({ status: 200, contentType: type, body: await readFile(join(pagesDir, name)) });
  });
}

function inspectorSession(page) {
  const session = page._connection?.toImpl?.(page)?.delegate?._session;
  if (typeof session?.send !== 'function') {
    throw new Error('the page inspector session is not reachable through Playwright internals');
  }
  return session;
}

async function main() {
  const browser = await webkit.launch({ headless: true });
  const result = {
    probe: 'playwright-webkit',
    page: pageName,
    route,
    environment: {
      macos: macosVersion(),
      webkit: browser.version(),
      playwright: await playwrightVersion(),
      node: process.version,
      launch: { headless: true },
      runDate: new Date().toISOString().slice(0, 10),
    },
  };
  try {
    const context = await browser.newContext();
    await serveProbePages(context);
    const page = await context.newPage();
    await page.goto(`${origin}/${pageName}`);
    let session;
    try {
      session = inspectorSession(page);
    } catch (error) {
      result.sessionError = error.message;
      return;
    }
    const { root } = await session.send('DOM.getDocument');
    const { nodeIds } = await session.send('DOM.querySelectorAll', {
      nodeId: root.nodeId,
      selector: '[data-probe]',
    });
    result.elements = [];
    for (const nodeId of nodeIds) {
      const { attributes = [] } = await session.send('DOM.getAttributes', { nodeId });
      const probe = attributes[attributes.indexOf('data-probe') + 1];
      try {
        const response = await session.send('DOM.getAccessibilityPropertiesForNode', { nodeId });
        result.elements.push({ probe, nodeId, response });
      } catch (error) {
        result.elements.push({ probe, nodeId, error: error.message });
      }
    }
    const answered = result.elements.filter((element) => element.response).length;
    console.log(`WebKit ${result.environment.webkit}: ${answered} of ${result.elements.length} answered`);
  } finally {
    await browser.close();
    await writeFile(outputFile, `${JSON.stringify(result, null, 2)}\n`);
  }
}

await main();
