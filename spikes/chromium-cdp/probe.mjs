// Records what Chromium's DevTools Protocol returns for the probe pages: the
// full DOM, the full accessibility tree, and per-target follow-up calls. The
// output is raw evidence for docs/decisions/ADR-0008; nothing here interprets
// it beyond collecting the nodes each question needs.
import { execFileSync } from 'node:child_process';
import { readdir, readFile, writeFile } from 'node:fs/promises';
import { extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const here = fileURLToPath(new URL('.', import.meta.url));
const pagesDir = join(here, 'pages');
const outputDir = join(here, 'output');
const origin = 'http://spokenpath.test';
const contentTypes = { '.html': 'text/html; charset=utf-8', '.svg': 'image/svg+xml' };

function macosVersion() {
  const read = (key) => execFileSync('sw_vers', [key], { encoding: 'utf8' }).trim();
  return { productVersion: read('-productVersion'), buildVersion: read('-buildVersion') };
}

async function playwrightVersion() {
  const manifest = new URL('../node_modules/playwright/package.json', import.meta.url);
  return JSON.parse(await readFile(manifest, 'utf8')).version;
}

// Every request is answered from pages/ on a fictional origin, and anything
// else is aborted, so a probe page can never reach the network.
async function serveProbePages(context) {
  await context.route('**/*', async (route) => {
    const url = new URL(route.request().url());
    if (url.origin !== origin) {
      await route.abort('blockedbyclient');
      return;
    }
    const name = url.pathname.slice(1);
    const type = contentTypes[extname(name)];
    if (!type || name.includes('/')) {
      await route.fulfill({ status: 404, body: 'not found' });
      return;
    }
    try {
      await route.fulfill({ status: 200, contentType: type, body: await readFile(join(pagesDir, name)) });
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
      await route.fulfill({ status: 404, body: 'not found' });
    }
  });
}

// A failed protocol call is part of the evidence, so it is recorded rather
// than thrown.
async function call(cdp, method, params) {
  try {
    return { result: await cdp.send(method, params) };
  } catch (error) {
    return { error: error.message };
  }
}

async function probeTargets(cdp, documentNodeId, fullTree) {
  const { nodeIds } = await cdp.send('DOM.querySelectorAll', {
    nodeId: documentNodeId,
    selector: '[data-probe]',
  });
  const targets = [];
  for (const nodeId of nodeIds) {
    const { node } = await cdp.send('DOM.describeNode', { nodeId });
    const attributes = Object.fromEntries(
      node.attributes.flatMap((value, index, all) => (index % 2 === 0 ? [[value, all[index + 1]]] : [])),
    );
    const backendNodeId = node.backendNodeId;
    targets.push({
      probe: attributes['data-probe'],
      tag: node.localName,
      backendNodeId,
      fullTreeNodes: fullTree.nodes.filter((axNode) => axNode.backendDOMNodeId === backendNodeId),
      partialWithoutRelatives: await call(cdp, 'Accessibility.getPartialAXTree', {
        backendNodeId,
        fetchRelatives: false,
      }),
      partialWithRelatives: await call(cdp, 'Accessibility.getPartialAXTree', {
        backendNodeId,
        fetchRelatives: true,
      }),
    });
  }
  return targets;
}

// getFullAXTree may leave out inline text boxes, so each StaticText node's
// children are asked for explicitly.
async function staticTextChildren(cdp, fullTree) {
  const staticTexts = fullTree.nodes.filter((axNode) => axNode.role?.value === 'StaticText');
  const children = [];
  for (const axNode of staticTexts) {
    children.push({
      nodeId: axNode.nodeId,
      name: axNode.name?.value,
      getChildAXNodes: await call(cdp, 'Accessibility.getChildAXNodes', { id: axNode.nodeId }),
    });
  }
  return children;
}

// Whether the tree carries inline text boxes when the Accessibility domain was
// never enabled, read in a separate context so no earlier enable affects it.
async function fullTreeWithoutAccessibilityEnable(browser, url) {
  const context = await browser.newContext();
  try {
    await serveProbePages(context);
    const page = await context.newPage();
    const cdp = await context.newCDPSession(page);
    await page.goto(url);
    return await cdp.send('Accessibility.getFullAXTree', {});
  } finally {
    await context.close();
  }
}

// The AX tree reports a node's own reason only for inert and display:none
// subtrees, so the computed styles that hide an ancestor are captured too.
async function hidingStyles(cdp) {
  return await call(cdp, 'DOMSnapshot.captureSnapshot', {
    computedStyles: ['display', 'visibility'],
  });
}

async function main() {
  const pages = (await readdir(pagesDir)).filter((name) => name.endsWith('.html')).sort();
  // The full Chromium build in its new headless mode, rather than the
  // separate headless shell, so the tree comes from the browser users run.
  const browser = await chromium.launch({ channel: 'chromium', headless: true });
  try {
    const textRunsWithoutEnable = await fullTreeWithoutAccessibilityEnable(
      browser,
      `${origin}/06-text-runs.html`,
    );
    const context = await browser.newContext();
    await serveProbePages(context);
    const page = await context.newPage();
    const cdp = await context.newCDPSession(page);
    const browserVersion = await cdp.send('Browser.getVersion');
    const environment = {
      macos: macosVersion(),
      chromium: browser.version(),
      chromiumProduct: browserVersion.product,
      chromiumRevision: browserVersion.revision,
      protocolVersion: browserVersion.protocolVersion,
      playwright: await playwrightVersion(),
      node: process.version,
      launch: { channel: 'chromium', headless: true },
      runDate: new Date().toISOString().slice(0, 10),
    };
    await cdp.send('DOM.enable');
    await cdp.send('Accessibility.enable');

    for (const name of pages) {
      const url = `${origin}/${name}`;
      await page.goto(url);
      const cssSupports = await page.evaluate(() => ({
        readingFlowFlexVisual: CSS.supports('reading-flow', 'flex-visual'),
        readingFlowGridRows: CSS.supports('reading-flow', 'grid-rows'),
      }));
      const domDocument = await cdp.send('DOM.getDocument', { depth: -1, pierce: true });
      const fullAXTree = await cdp.send('Accessibility.getFullAXTree', {});
      const record = {
        page: name,
        url,
        environment,
        cssSupports,
        targets: await probeTargets(cdp, domDocument.root.nodeId, fullAXTree),
        staticTextChildren: name.includes('text-runs')
          ? await staticTextChildren(cdp, fullAXTree)
          : undefined,
        fullAXTreeWithoutAccessibilityEnable: name.includes('text-runs') ? textRunsWithoutEnable : undefined,
        domSnapshot: name.includes('subtree') ? await hidingStyles(cdp) : undefined,
        domDocument,
        fullAXTree,
      };
      const outputFile = join(outputDir, name.replace(/\.html$/, '.json'));
      await writeFile(outputFile, `${JSON.stringify(record, null, 2)}\n`);
      console.log(`${name}: ${fullAXTree.nodes.length} AX nodes, ${record.targets.length} targets`);
    }
    console.log(`Chromium ${environment.chromium}, Playwright ${environment.playwright}`);
  } finally {
    await browser.close();
  }
}

await main();
