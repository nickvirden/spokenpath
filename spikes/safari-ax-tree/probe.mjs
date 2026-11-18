// Drives Safari to the tagged fixture through a safaridriver session, reads
// Safari's macOS accessibility tree with ax-tree.swift for each carrier
// variant of the fixture, and counts how many tagged elements each candidate
// source-location carrier maps to exactly one AX node. The output is raw
// evidence for docs/decisions/ADR-0009.
//
// With `--untrusted`, it instead builds ax-tree.swift into a temporary folder
// and runs the binary as a transient launchd job of the invoking user. That
// binary is not the terminal's child, so it does not inherit the terminal's
// Accessibility permission, which answers what the helper reads without the
// permission without changing any setting.
//
// Exit status: 0 when the tree was read with the permission, 3 when the
// permission is missing (recorded, not an error), 1 when safaridriver, the
// session or the Swift program cannot run.
import { execFileSync, spawn } from 'node:child_process';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
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
const programTimeoutMs = 60_000;
// A fixed, namespaced label, so a leftover job is recognisable and removable
// with `launchctl remove test.spokenpath.ax-tree-untrusted`.
const launchdLabel = 'test.spokenpath.ax-tree-untrusted';
const srcPattern = /data-spokenpath-src="([^"]+)"/g;

// A class token or id derived from a source location: `tagged.html:12:3`
// becomes `sp-src-12-3`, a valid class token and id.
const token = (src) => `sp-src-${src.split(':').slice(1).join('-')}`;

// The carrier is spliced into the served text next to the source attribute,
// and `[^>]*` assumes no `>` inside an attribute value, which holds for the
// committed fixture.
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

// Runs a program to completion and resolves once its output streams have
// closed. It rejects when the program cannot start, exceeds the deadline (the
// child is then killed), or exits with a status outside `allowedExits`.
function runProgram(file, args, allowedExits = [0]) {
  return new Promise((resolve, reject) => {
    const child = spawn(file, args, { stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill('SIGKILL');
    }, programTimeoutMs);
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.once('error', (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.once('close', (code) => {
      clearTimeout(timer);
      if (timedOut) {
        reject(new Error(`${file} did not finish within ${programTimeoutMs} ms: ${stderr.trim()}`));
      } else if (!allowedExits.includes(code)) {
        reject(new Error(`${file} exited ${code}: ${stderr.trim()}`));
      } else {
        resolve({ exitCode: code, stdout, stderr });
      }
    });
  });
}

function parseTree(stdout, stderr) {
  try {
    return JSON.parse(stdout);
  } catch (error) {
    throw new Error(`ax-tree.swift printed no JSON (${error.message}): ${stderr.trim()}`);
  }
}

async function runSwift(urlSubstring, needle) {
  const { exitCode, stdout, stderr } = await runProgram(
    'swift',
    [join(here, 'ax-tree.swift'), urlSubstring, needle],
    [0, untrustedExit],
  );
  return { exitCode, tree: parseTree(stdout, stderr) };
}

// `launchctl submit` keeps a job alive, restarting it after it exits, so the
// job is removed as soon as the first run has printed its JSON, and again on
// any error, timeout or signal. The launchd output files stay in the
// temporary folder and never reach the committed output.
async function runUntrusted(urlSubstring, needle) {
  const workDir = await mkdtemp(join(tmpdir(), 'spokenpath-ax-untrusted-'));
  const binary = join(workDir, 'ax-tree');
  const stdoutPath = join(workDir, 'stdout.txt');
  const stderrPath = join(workDir, 'stderr.txt');
  const removeJob = () => {
    try {
      execFileSync('launchctl', ['remove', launchdLabel], { stdio: 'ignore' });
      return true;
    } catch {
      // `launchctl remove` fails when the job is already gone.
      return false;
    }
  };
  const onSignal = (signal) => {
    removeJob();
    process.kill(process.pid, signal);
  };
  try {
    await runProgram('swiftc', ['-o', binary, join(here, 'ax-tree.swift')]);
    process.once('SIGINT', onSignal);
    process.once('SIGTERM', onSignal);
    await runProgram('launchctl', [
      'submit',
      '-l',
      launchdLabel,
      '-o',
      stdoutPath,
      '-e',
      stderrPath,
      '--',
      binary,
      urlSubstring,
      needle,
    ]);
    const deadline = Date.now() + programTimeoutMs;
    while (Date.now() < deadline) {
      const printed = await readFile(stdoutPath, 'utf8').catch((error) => {
        if (error.code === 'ENOENT') return '';
        throw error;
      });
      // The first complete JSON document is the first run's output.
      const end = printed.indexOf('\n}\n');
      if (end !== -1) {
        const status = execFileSync('launchctl', ['list', launchdLabel], { encoding: 'utf8' });
        const lastExitStatus = Number(status.match(/"LastExitStatus" = (-?\d+);/)?.[1]);
        return {
          launchRoute: 'transient launchd job of the invoking user (launchctl submit), built with swiftc',
          launchdLabel,
          launchdLastExitStatus: lastExitStatus,
          // launchd reports a wait status; a normal exit keeps its code in the
          // high byte.
          exitStatus: (lastExitStatus & 0x7f) === 0 ? lastExitStatus >> 8 : null,
          tree: parseTree(printed.slice(0, end + 3), await readFile(stderrPath, 'utf8').catch(() => '')),
        };
      }
      await sleep(200);
    }
    throw new Error(`the launchd job printed no JSON within ${programTimeoutMs} ms`);
  } finally {
    removeJob();
    process.removeListener('SIGINT', onSignal);
    process.removeListener('SIGTERM', onSignal);
    await rm(workDir, { recursive: true, force: true });
  }
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
// The sequence comes from the same fixture, whose attribute values contain no
// `>`, so the tag pattern above and this order agree.
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

// Shows Safari the plain fixture, so a web area exists to be found, and runs
// the untrusted binary against it.
async function mainUntrusted() {
  const server = await startFixtureServer();
  const path = '/tagged.html';
  const result = {
    probe: 'safari-ax-tree',
    mode: 'untrusted',
    driver: 'safaridriver session (the automation window)',
    environment: {
      macos: macosVersion(),
      safari: safariVersion(),
      node: process.version,
      runDate: new Date().toISOString().slice(0, 10),
    },
  };
  let driver;
  let session;
  let exitCode = 0;
  try {
    driver = await startSafaridriver();
    session = await newSession(driver.base);
    result.capabilities = session.capabilities;
    await command(driver.base, 'POST', `/session/${session.id}/url`, { url: `${server.origin}${path}` });
    await sleep(500);
    Object.assign(result, await runUntrusted(path, 'tagged.html:'));
    if (result.tree.permission === 'missing') exitCode = untrustedExit;
    const [attempt] = result.tree.attempts;
    console.log(
      `untrusted: trusted ${result.tree.axIsProcessTrusted}, permission ${result.tree.permission}, ` +
        `error ${attempt?.applicationChildrenError} (${attempt?.applicationChildrenErrorName}), ` +
        `${result.tree.nodeCount} nodes, launchd last exit status ${result.launchdLastExitStatus}`,
    );
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
    await writeFile(join(outputDir, 'ax-tree-untrusted.json'), `${JSON.stringify(result, null, 2)}\n`);
  }
  return exitCode;
}

process.exitCode = process.argv.includes('--untrusted') ? await mainUntrusted() : await main();
