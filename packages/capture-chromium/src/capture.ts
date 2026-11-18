import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { basename, dirname, resolve } from 'node:path';
import { instrumentHtml } from '@spokenpath/instrument-html';
import {
  type AxForced,
  type AxNode,
  type Diagnostic,
  type Snapshot,
  validateSnapshot,
} from '@spokenpath/snapshot-types';
import type { Browser } from 'playwright-core';
import { flattenDocument } from './dom.ts';
import { CaptureError } from './error.ts';
import { walkFocus } from './focus.ts';
import type { ProtocolNode } from './protocol.ts';
import {
  CONDITIONS,
  DEFAULT_TIMEOUT_MS,
  launchBrowser,
  loadAndSettle,
  openPage,
  type PageSession,
} from './session.ts';
import { compareDocuments, type DomComparison } from './verify.ts';
import { playwrightVersion, producerVersion } from './versions.ts';

export interface CaptureOptions {
  /** A CSS selector to wait for, in the DOM, after the `load` event. */
  waitFor?: string;
  /** How long loading, waiting for the selector and settling may each take; 30 000 ms by default. */
  timeoutMs?: number;
}

interface PageFile {
  root: string;
  fileName: string;
  bytes: Uint8Array;
}

async function readPage(pagePath: string): Promise<PageFile> {
  const absolute = resolve(pagePath);
  try {
    return { root: dirname(absolute), fileName: basename(absolute), bytes: await readFile(absolute) };
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    throw new CaptureError('page-unreadable', `cannot read the page ${JSON.stringify(pagePath)}: ${reason}`, {
      cause: error,
    });
  }
}

async function withBrowser<T>(run: (browser: Browser) => Promise<T>): Promise<T> {
  const browser = await launchBrowser();
  try {
    return await run(browser);
  } finally {
    await browser.close();
  }
}

async function getDocument(session: PageSession): Promise<ProtocolNode> {
  const { root } = await session.cdp.send('DOM.getDocument', { depth: -1, pierce: true });
  return root;
}

/** Sorted by code, then node, then message, so the order requests finish in cannot reorder them. */
function sortDiagnostics(diagnostics: readonly Diagnostic[]): Diagnostic[] {
  const nodeKey = (diagnostic: Diagnostic) => diagnostic.node ?? -1;
  return diagnostics.toSorted(
    (left, right) =>
      compareStrings(left.code, right.code) ||
      nodeKey(left) - nodeKey(right) ||
      compareStrings(left.message, right.message),
  );
}

function compareStrings(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

/**
 * Captures what Chromium exposes for the HTML page at `pagePath`: the page is instrumented with its
 * source locations, served by request interception with external requests blocked, settled, and
 * read through the DevTools Protocol, then walked with real Tab presses. Capture records the
 * protocol's responses as delivered; it never joins, filters or interprets them (ADR-0002).
 *
 * @throws {InstrumentHtmlError} when the instrumenter refuses the page.
 * @throws {CaptureError} when the page cannot be read, does not load or settle in time, cannot be
 *   served, or yields a snapshot that fails the schema.
 */
export async function captureChromium(pagePath: string, options: CaptureOptions = {}): Promise<Snapshot> {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const { root, fileName, bytes } = await readPage(pagePath);
  const { html, spans } = instrumentHtml(bytes, { source: 0 });
  const servedText = new TextDecoder('utf-8').decode(html);

  return withBrowser(async (browser) => {
    const diagnostics: Diagnostic[] = [];
    const session = await openPage(browser, { root, fileName, body: html, diagnostics });
    const settle = await loadAndSettle(session, options.waitFor, timeoutMs);
    const capturedAt = new Date().toISOString();
    const { cdp, page } = session;

    const document = await getDocument(session);
    // The protocol's AXNode declares no index signature, so TypeScript cannot relate it to the
    // schema's open AxNode; the nodes are stored as delivered and validated below.
    const { nodes } = await cdp.send('Accessibility.getFullAXTree', {});
    const ax = nodes as unknown as AxNode[];
    const flat = flattenDocument(document, spans, servedText);
    diagnostics.push(...flat.diagnostics);

    const inFullTree = new Set(ax.map((node) => node.backendDOMNodeId));
    const axForced: AxForced[] = [];
    for (const node of flat.dom) {
      if (node.kind !== 'element' || inFullTree.has(node.backend_id)) {
        continue;
      }
      try {
        const partial = await cdp.send('Accessibility.getPartialAXTree', {
          backendNodeId: node.backend_id,
          fetchRelatives: false,
        });
        axForced.push({
          node: node.id,
          backend_id: node.backend_id,
          nodes: partial.nodes as unknown as AxNode[],
        });
      } catch (error) {
        const reason = error instanceof Error ? error.message : String(error);
        diagnostics.push({
          code: 'partial-ax-tree-failed',
          message: `Accessibility.getPartialAXTree failed for this element: ${reason}`,
          node: node.id,
        });
      }
    }

    const domSnapshot = await cdp.send('DOMSnapshot.captureSnapshot', {
      computedStyles: ['display', 'visibility'],
    });
    const walk = await walkFocus(page, cdp, flat.dom, flat.idByBackendId);
    diagnostics.push(...walk.diagnostics);
    session.throwIfServingFailed();

    const snapshot: Snapshot = {
      schema: 'spokenpath-snapshot',
      version: { major: 0, minor: 0 },
      producer: { name: '@spokenpath/capture-chromium', version: producerVersion },
      capture: {
        engine: 'chromium',
        engine_version: browser.version(),
        tool_version: `playwright ${playwrightVersion}`,
        viewport: { ...CONDITIONS.viewport },
        color_scheme: CONDITIONS.color_scheme,
        reduced_motion: CONDITIONS.reduced_motion,
        url: session.url,
        settle,
      },
      volatile: { captured_at: capturedAt },
      sources: [
        {
          id: 0,
          path: fileName,
          sha256: createHash('sha256').update(bytes).digest('hex'),
          encoding: 'utf-8',
        },
      ],
      spans,
      diagnostics: sortDiagnostics(diagnostics),
      payload: {
        engine: 'chromium',
        dom: flat.dom,
        ax,
        ax_forced: axForced,
        // Stored as delivered, for the same reason as the AX nodes.
        dom_snapshot: domSnapshot as unknown as Record<string, unknown>,
        focus: walk.focus,
      },
    };
    const result = validateSnapshot(snapshot);
    if (!result.ok) {
      const issues = result.errors.map((issue) => `${issue.path || '/'}: ${issue.message}`).join('; ');
      throw new CaptureError(
        'invalid-snapshot',
        `the captured snapshot fails the snapshot schema at ${issues}`,
      );
    }
    return result.snapshot;
  });
}

/**
 * Loads the page's original bytes and its instrumented bytes in two separate page loads, served
 * and settled the same way, and compares the DOM Chromium builds from each. Instrumentation is
 * sound for the page when they are equal.
 *
 * @throws {InstrumentHtmlError} when the instrumenter refuses the page.
 * @throws {CaptureError} as `captureChromium` does, for reading, loading and serving.
 */
export async function verifyInstrumentation(
  pagePath: string,
  options: CaptureOptions = {},
): Promise<DomComparison> {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const { root, fileName, bytes } = await readPage(pagePath);
  const { html } = instrumentHtml(bytes, { source: 0 });

  return withBrowser(async (browser) => {
    const load = async (body: Uint8Array): Promise<ProtocolNode> => {
      const session = await openPage(browser, { root, fileName, body, diagnostics: [] });
      try {
        await loadAndSettle(session, options.waitFor, timeoutMs);
        const document = await getDocument(session);
        session.throwIfServingFailed();
        return document;
      } finally {
        await session.context.close();
      }
    };
    const original = await load(bytes);
    const instrumented = await load(html);
    return compareDocuments(original, instrumented);
  });
}
