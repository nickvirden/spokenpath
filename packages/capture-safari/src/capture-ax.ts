import { randomBytes } from 'node:crypto';
import { sortDiagnostics } from '@spokenpath/capture-chromium';
import { instrumentHtml, sourceClassToken } from '@spokenpath/instrument-html';
import type { Diagnostic, SafariAxNode, Snapshot, Span } from '@spokenpath/snapshot-types';
import {
  type CaptureOptions,
  conditions,
  hostVersions,
  loadAndSettle,
  openSession,
  pageSource,
  readPage,
  runCapture,
  runInPage,
  type Session,
  safaridriver,
  timeoutOf,
  validated,
  versionFields,
} from './capture.ts';
import { CaptureError } from './error.ts';
import {
  axHelper,
  canaryPage,
  type HelperCommand,
  HelperProcesses,
  type HelperRun,
  type ReadTarget,
  readCanary,
  readTree,
} from './helper.ts';
import { VISIBILITY_SCRIPT } from './scripts.ts';
import { type PageServer, servePage } from './serve.ts';
import { producerVersion } from './versions.ts';
import { driverVersion } from './webdriver.ts';

export interface AxCaptureOptions extends CaptureOptions {
  /** For tests only: the accessibility helper to run instead of `spokenpath-ax-macos`. */
  helper?: HelperCommand;
  /** For tests only: the check page to serve instead of the helper's. */
  canaryPage?: string;
}

/** The query parameter whose per-capture value makes the served URL unique. */
export const NONCE_PARAMETER = 'spokenpath-nonce';

/** The file name the check page is served at. */
const CANARY_FILE_NAME = 'spokenpath-check.html';

/** The attribute whose values carry the source class tokens. */
const CLASS_LIST = 'AXDOMClassList';

/** `url` with a fresh nonce: 32 lowercase hex characters from 16 random bytes. */
function withNonce(url: string): string {
  return `${url}?${NONCE_PARAMETER}=${randomBytes(16).toString('hex')}`;
}

/** What Safari's WebDriver says identifies the automation window: the page's title and the window's rect. */
async function readTarget(session: Session, url: string): Promise<ReadTarget> {
  const title = await session.call("reading the page's title", 'GET', '/title');
  const rect = await session.call("reading the window's rect", 'GET', '/window/rect');
  const record = typeof rect === 'object' && rect !== null ? (rect as Record<string, unknown>) : {};
  const { x, y, width, height } = record;
  if (
    typeof title !== 'string' ||
    typeof x !== 'number' ||
    typeof y !== 'number' ||
    typeof width !== 'number' ||
    typeof height !== 'number'
  ) {
    throw new Error(
      `Safari answered the title and window rect with ${JSON.stringify(title)} and ${JSON.stringify(rect)}`,
    );
  }
  return { url, title, rect: { x, y, width, height } };
}

/**
 * Fails with `page-hidden` when Safari reports the page as not visible once its tree has been
 * read, because a hidden page's tree may not be the page as it shows.
 */
async function requireVisible(session: Session, servedOrigin: string): Promise<void> {
  const visibility = await runInPage(
    session,
    servedOrigin,
    "reading the page's visibility",
    'sync',
    VISIBILITY_SCRIPT,
    [],
  );
  if (visibility !== 'visible') {
    throw new CaptureError(
      'page-hidden',
      `Safari reports the page as ${JSON.stringify(visibility)} after its macOS accessibility tree was read, so the tree may not be the page as it shows; ` +
        'keep the Mac awake and unlocked while Safari capture runs (see "Enabling Safari automation" in docs/usage.md)',
    );
  }
}

/**
 * Sets each node's `span` from the first source class token in its `AXDOMClassList` that names a
 * span, and records an `unknown-source-token` diagnostic for each token that names none. The
 * recorded attribute values are left as Safari delivered them, tokens included.
 */
export function mapSourceTokens(
  nodes: readonly SafariAxNode[],
  spans: readonly Span[],
): { nodes: SafariAxNode[]; diagnostics: Diagnostic[] } {
  const byToken = new Map(
    spans.filter((span) => span.source === 0).map((span) => [sourceClassToken(span), span.id] as const),
  );
  const diagnostics: Diagnostic[] = [];
  const mapped = nodes.map((node) => {
    const classList = node.attributes[CLASS_LIST];
    const tokens =
      classList?.type === 'array'
        ? classList.items.flatMap((item) =>
            item.type === 'string' && /^sp-src-\d+-\d+$/.test(item.value) ? [item.value] : [],
          )
        : [];
    let span: number | undefined;
    for (const token of tokens) {
      const id = byToken.get(token);
      if (id === undefined) {
        diagnostics.push({
          code: 'unknown-source-token',
          message: `Node ${node.id}'s ${CLASS_LIST} holds the token ${JSON.stringify(token)}, which names no span, so the node has no span from it.`,
        });
      } else {
        span ??= id;
      }
    }
    return span === undefined ? { ...node } : { ...node, span };
  });
  return { nodes: mapped, diagnostics };
}

/**
 * Captures Safari's macOS accessibility tree for the HTML page at `pagePath`. The page is
 * instrumented with its source locations as class tokens, served on the loopback address at a
 * URL with a per-capture nonce, and loaded in one `safaridriver` session. First a built-in check
 * page is loaded and read, which tells a missing Accessibility permission apart from an empty
 * tree; then the page is loaded, settled and read by the `spokenpath-ax-macos` helper, which reads
 * only the automation window. Every node whose class list holds a token gets that token's span.
 * The tree is recorded as Safari delivered it and never interpreted.
 *
 * SIGINT and SIGTERM are handled as `captureSafari` handles them; a helper still running is
 * killed before the session is deleted. No snapshot is returned after a check-page, permission,
 * window or lock failure.
 *
 * @throws {InstrumentHtmlError} when the instrumenter refuses the page.
 * @throws {CaptureError} for every failure `captureSafari` reports, and when the helper is not
 *   built, the permission is missing, Safari is not running, the window cannot be identified, the
 *   check page's tree is incomplete, the tree is too large, the screen is locked or the page is
 *   hidden after the read (`page-hidden`), or the helper fails in another way.
 */
export async function captureSafariAx(pagePath: string, options: AxCaptureOptions = {}): Promise<Snapshot> {
  const timeoutMs = timeoutOf(options);
  const driverCommand = options.driver ?? safaridriver();
  const helper = options.helper ?? axHelper();
  const { root, fileName, bytes } = await readPage(pagePath);
  const { html, spans } = instrumentHtml(bytes, { source: 0, classToken: true });
  const diagnostics: Diagnostic[] = [];

  return runCapture(async (resources, interrupt) => {
    const processes = new HelperProcesses();
    resources.helpers = processes;
    const helperRun: HelperRun = { helper, timeoutMs, signal: interrupt.signal, processes };
    const toolVersion = await driverVersion(driverCommand, timeoutMs);
    const versions = await hostVersions(options.driver, diagnostics);
    const canaryHtml = options.canaryPage ?? (await canaryPage(helperRun));
    const serve = async (name: string, body: Uint8Array): Promise<PageServer> => {
      const server = await servePage({ root, fileName: name, body, diagnostics });
      resources.servers.push(server);
      return server;
    };
    const canaryServer = await serve(CANARY_FILE_NAME, new TextEncoder().encode(canaryHtml));
    const server = await serve(fileName, html);
    const { session, browserVersion } = await openSession(driverCommand, timeoutMs, interrupt, resources);

    const canaryUrl = withNonce(canaryServer.url);
    await loadAndSettle(session, canaryServer, canaryUrl, undefined, timeoutMs);
    await readCanary(helperRun, await readTarget(session, canaryUrl));

    const url = withNonce(server.url);
    const settleName = await loadAndSettle(session, server, url, options.waitFor, timeoutMs);
    const servedOrigin = new URL(server.url).origin;
    const capturedAt = new Date().toISOString();
    const measured = await conditions(session, servedOrigin);
    const tree = await readTree(helperRun, await readTarget(session, url));
    await requireVisible(session, servedOrigin);
    server.throwIfServingFailed();
    canaryServer.throwIfServingFailed();
    const mapped = mapSourceTokens(tree, spans);
    diagnostics.push(...mapped.diagnostics);

    return validated({
      schema: 'spokenpath-snapshot',
      version: { major: 0, minor: 2 },
      producer: { name: '@spokenpath/capture-safari', version: producerVersion },
      capture: {
        engine: 'safari-ax',
        engine_version: browserVersion,
        tool_version: `safaridriver ${toolVersion}`,
        ...measured,
        url,
        settle: settleName,
      },
      volatile: { captured_at: capturedAt },
      sources: [pageSource(fileName, bytes)],
      spans,
      diagnostics: sortDiagnostics(diagnostics),
      payload: {
        engine: 'safari-ax',
        nodes: mapped.nodes,
        ...versionFields(versions),
      },
    });
  });
}
