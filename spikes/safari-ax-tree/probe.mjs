// Drives Safari to the tagged fixture through a safaridriver session, reads
// Safari's macOS accessibility tree with ax-tree.swift for each carrier
// variant of the fixture, and counts how many tagged elements each candidate
// source-location carrier maps to exactly one AX node. The output is raw
// evidence for docs/decisions/ADR-0009.
//
// Exit status: 0 when the tree was read with the permission, 3 when the
// process is not trusted (recorded, not an error), 1 when safaridriver, the
// session or the Swift program cannot run.
import { spawn } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { tagSequence } from '../safari-webdriver/probe.mjs';
import {
  command,
  fixtureDir,
  macosVersion,
  newSession,
  safariVersion,
  startFixtureServer,
  startSafaridriver,
} from '../safari-webdriver/webdriver.mjs';

const here = fileURLToPath(new URL('.', import.meta.url));
const outputDir = join(here, 'output');
const untrustedExit = 3;
const srcPattern = /data-spokenpath-src="([^"]+)"/g;

// A class token or id derived from a source location: `tagged.html:12:3`
// becomes `sp-src-12-3`, a valid class token and id.
const token = (src) => `sp-src-${src.split(':').slice(1).join('-')}`;

// The carrier is spliced into the served text next to the source attribute,
// which every tagged start tag carries last. The HTML parser keeps the first
// of two duplicate attributes, so the class token joins an existing author
// `class` value, while an author `id` earlier in the tag wins over a spliced
// one and that element keeps no carrier.
const taggedStartTag = /<[a-z][^>]*data-spokenpath-src="([^"]+)"[^>]*>/g;
const variants = {
  plain: (html) => html,
  class: (html) =>
    html.replace(taggedStartTag, (tag, src) =>
      /\sclass="/.test(tag)
        ? tag.replace(/\sclass="([^"]*)"/, ` class="$1 ${token(src)}"`)
        : tag.replace(`data-spokenpath-src="${src}"`, `data-spokenpath-src="${src}" class="${token(src)}"`),
    ),
  id: (html) => html.replace(srcPattern, (match, src) => `${match} id="${token(src)}"`),
};

function runSwift(urlSubstring, needle) {
  return new Promise((resolve, reject) => {
    const child = spawn('swift', [join(here, 'ax-tree.swift'), urlSubstring, needle], {
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.once('error', reject);
    child.once('exit', (code) => {
      if (code !== 0 && code !== untrustedExit) {
        reject(new Error(`ax-tree.swift exited ${code}: ${stderr.trim()}`));
        return;
      }
      resolve({ exitCode: code, tree: JSON.parse(stdout) });
    });
  });
}

function mapByAttribute(sources, nodes, read) {
  const perElement = sources.map((src) => {
    const matches = nodes.filter((node) => read(node, token(src)));
    return { src, matchingNodeIndexes: matches.map((node) => node.index) };
  });
  return summarise(perElement);
}

// Positional matching: walk the AX nodes in tree order and, for each tagged
// element in document order, take the next node whose title or description
// equals Safari's computed label. Elements with an empty label cannot match.
function mapByPosition(sequence, nodes) {
  let cursor = 0;
  const perElement = sequence.map(({ src, label }) => {
    if (!label) return { src, matchingNodeIndexes: [], reason: 'empty computed label' };
    const rest = nodes.slice(cursor);
    const matches = rest.filter((node) => node.AXTitle === label || node.AXDescription === label);
    if (matches.length === 0) return { src, matchingNodeIndexes: [], reason: 'no node with that label' };
    cursor = matches[0].index + 1;
    return {
      src,
      matchingNodeIndexes: [matches[0].index],
      laterNodesWithSameLabel: matches.length - 1,
    };
  });
  return summarise(perElement);
}

function summarise(perElement) {
  const mapped = perElement.filter((entry) => entry.matchingNodeIndexes.length === 1);
  return {
    tagged: perElement.length,
    mappedToExactlyOneNode: mapped.length,
    misses: perElement.filter((entry) => entry.matchingNodeIndexes.length !== 1),
    perElement,
  };
}

async function main() {
  const fixture = await readFile(join(fixtureDir, 'tagged.html'), 'utf8');
  const sources = [...fixture.matchAll(srcPattern)].map((match) => match[1]);
  const server = await startFixtureServer((pathname, html) => {
    const variant = pathname.match(/-([a-z]+)\.html$/)?.[1] ?? 'plain';
    return variants[variant](html);
  });
  const result = {
    probe: 'safari-ax-tree',
    driver: 'safaridriver session (the automation window)',
    environment: {
      macos: macosVersion(),
      safari: safariVersion(),
      node: process.version,
      runDate: new Date().toISOString().slice(0, 10),
    },
    taggedSources: sources,
  };
  let driver;
  let session;
  let exitCode = 0;
  try {
    driver = await startSafaridriver();
    session = await newSession(driver.base);
    result.capabilities = session.capabilities;
    const trees = {};
    for (const variant of Object.keys(variants)) {
      const path = `/tagged-${variant}.html`;
      await command(driver.base, 'POST', `/session/${session.id}/url`, { url: `${server.origin}${path}` });
      if (variant === 'plain') result.webDriverSequence = await tagSequence(driver.base, session.id);
      // Layout settles before the first query; the Swift program itself
      // records whether that first query already returns a populated tree.
      await sleep(500);
      const { exitCode: swiftExit, tree } = await runSwift(path, 'tagged.html:');
      trees[variant] = tree;
      if (swiftExit === untrustedExit) exitCode = untrustedExit;
      await writeFile(join(outputDir, `ax-tree-${variant}.json`), `${JSON.stringify(tree, null, 2)}\n`);
      console.log(
        `${variant}: trusted ${tree.axIsProcessTrusted}, ${tree.nodeCount} nodes, ` +
          `first query populated ${tree.populatedOnFirstQuery}`,
      );
    }
    result.trusted = trees.plain.axIsProcessTrusted;
    result.trigger = Object.fromEntries(
      Object.entries(trees).map(([variant, tree]) => [
        variant,
        { populatedOnFirstQuery: tree.populatedOnFirstQuery, attempts: tree.attempts },
      ]),
    );
    result.directDataAttributeExposure = trees.plain.nodes
      .filter((node) => node.attributesContainingNeedle.length > 0)
      .map((node) => ({ index: node.index, attributes: node.attributesContainingNeedle }));
    result.carriers = {
      classToken: mapByAttribute(sources, trees.class.nodes, (node, value) =>
        (node.AXDOMClassList ?? []).includes(value),
      ),
      elementId: mapByAttribute(sources, trees.id.nodes, (node, value) => node.AXDOMIdentifier === value),
      position: mapByPosition(result.webDriverSequence, trees.plain.nodes),
    };
    // A positional match is only useful when it lands on the right node. The
    // plain and class variants have the same tree shape, so a match is
    // checked against the class token at the same index.
    const { position } = result.carriers;
    position.treesHaveSameShape = trees.plain.nodeCount === trees.class.nodeCount;
    position.matchesConfirmedByClassToken = position.perElement.filter(
      ({ src, matchingNodeIndexes }) =>
        matchingNodeIndexes.length === 1 &&
        (trees.class.nodes[matchingNodeIndexes[0]].AXDOMClassList ?? []).includes(token(src)),
    ).length;
    for (const [name, carrier] of Object.entries(result.carriers)) {
      console.log(
        `${name}: ${carrier.mappedToExactlyOneNode} of ${carrier.tagged} mapped to exactly one node`,
      );
    }
  } catch (error) {
    result.runError = error.body?.value ?? { message: error.message };
    exitCode = 1;
  } finally {
    if (session) {
      result.sessionDeleted = await command(driver.base, 'DELETE', `/session/${session.id}`).then(
        () => true,
        (error) => ({ error: error.message }),
      );
    }
    await driver?.stop();
    await server.close();
    await writeFile(join(outputDir, 'carriers.json'), `${JSON.stringify(result, null, 2)}\n`);
  }
  return exitCode;
}

process.exitCode = await main();
