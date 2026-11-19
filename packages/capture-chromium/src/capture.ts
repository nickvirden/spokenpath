import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { basename, dirname, resolve } from 'node:path';
import { instrumentHtml } from '@spokenpath/instrument-html';
import { type AxNode, type Diagnostic, type Snapshot, validateSnapshot } from '@spokenpath/snapshot-types';
import type { Browser } from 'playwright-core';
import { flattenDocument } from './dom.ts';
import { CaptureError } from './error.ts';
import { walkFocus } from './focus.ts';
import { forcedAxTrees } from './forced.ts';
import type { ProtocolNode } from './protocol.ts';
import {
  bounded,
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
  /**
   * How long loading, waiting for the selector, settling and each later call into the browser may
   * take, in milliseconds: a positive finite number, 30 000 by default.
   */
  timeoutMs?: number;
}

interface PageFile {
  root: string;
  fileName: string;
  bytes: Uint8Array;
}

function timeoutOf(options: CaptureOptions): number {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) {
    // Playwright reads 0 as "no limit", which would let a busy page hang the capture.
    throw new CaptureError(
      'invalid-options',
      `timeoutMs must be a positive number of milliseconds, not ${String(timeoutMs)}`,
    );
  }
  return timeoutMs;
}

/**
 * Reads the page file. The error names the file by its base name and the reason by its code
 * only, because the file system's message carries the absolute path, which output shared from a
 * capture should not reveal; the full error stays in `cause`.
 */
async function readPage(pagePath: string): Promise<PageFile> {
  const absolute = resolve(pagePath);
  const fileName = basename(absolute);
  try {
    return { root: dirname(absolute), fileName, bytes: await readFile(absolute) };
  } catch (error) {
    const reason =
      error instanceof Error && 'code' in error && typeof error.code === 'string'
        ? error.code
        : 'an unknown error';
    throw new CaptureError('page-unreadable', `cannot read the page ${JSON.stringify(fileName)}: ${reason}`, {
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

async function getDocument(session: PageSession, timeoutMs: number): Promise<ProtocolNode> {
  const { root } = await bounded('DOM.getDocument', timeoutMs, (options) =>
    session.cdp.send('DOM.getDocument', { depth: -1, pierce: true }, options),
  );
  return root;
}

/** Sorted by code, then node, then message, so the order requests finish in cannot reorder them. */
export function sortDiagnostics(diagnostics: readonly Diagnostic[]): Diagnostic[] {
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
 * @throws {CaptureError} when the options are unusable, the page cannot be read, the page or the
 *   browser does not answer in time, the page cannot be served, or the snapshot fails the schema.
 */
export async function captureChromium(pagePath: string, options: CaptureOptions = {}): Promise<Snapshot> {
  const timeoutMs = timeoutOf(options);
  const { root, fileName, bytes } = await readPage(pagePath);
  const { html, spans } = instrumentHtml(bytes, { source: 0 });
  const servedText = new TextDecoder('utf-8').decode(html);

  return withBrowser(async (browser) => {
    const diagnostics: Diagnostic[] = [];
    const session = await openPage(browser, { root, fileName, body: html, diagnostics });
    const settle = await loadAndSettle(session, options.waitFor, timeoutMs);
    const capturedAt = new Date().toISOString();
    const { cdp, page } = session;

    const document = await getDocument(session, timeoutMs);
    const { nodes } = await bounded('Accessibility.getFullAXTree', timeoutMs, (callOptions) =>
      cdp.send('Accessibility.getFullAXTree', {}, callOptions),
    );
    // The protocol's AXNode declares no index signature, so TypeScript cannot relate it to the
    // schema's open AxNode; the nodes are stored as delivered and validated below.
    const ax = nodes as unknown as AxNode[];
    const flat = flattenDocument(document, spans, servedText);
    diagnostics.push(...flat.diagnostics);

    const forced = await forcedAxTrees(cdp, flat.dom, ax, timeoutMs);
    diagnostics.push(...forced.diagnostics);

    const domSnapshot = await bounded('DOMSnapshot.captureSnapshot', timeoutMs, (callOptions) =>
      cdp.send('DOMSnapshot.captureSnapshot', { computedStyles: ['display', 'visibility'] }, callOptions),
    );
    const walk = await walkFocus(page, cdp, flat.dom, flat.idByBackendId, timeoutMs);
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
        ax_forced: forced.axForced,
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
 * @throws {CaptureError} as `captureChromium` does, for options, reading, loading and serving.
 */
export async function verifyInstrumentation(
  pagePath: string,
  options: CaptureOptions = {},
): Promise<DomComparison> {
  const timeoutMs = timeoutOf(options);
  const { root, fileName, bytes } = await readPage(pagePath);
  const { html } = instrumentHtml(bytes, { source: 0 });

  return withBrowser(async (browser) => {
    const load = async (body: Uint8Array): Promise<ProtocolNode> => {
      const session = await openPage(browser, { root, fileName, body, diagnostics: [] });
      try {
        await loadAndSettle(session, options.waitFor, timeoutMs);
        const document = await getDocument(session, timeoutMs);
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
