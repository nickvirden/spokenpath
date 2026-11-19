import { readFile } from 'node:fs/promises';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import type { AddressInfo } from 'node:net';
import { contentType, redactUrl, resolveAsset } from '@spokenpath/capture-chromium';
import type { Diagnostic } from '@spokenpath/snapshot-types';
import { CaptureError } from './error.ts';

/** The loopback address the page is served on; nothing else on the network can reach it. */
export const HOST = '127.0.0.1';

/**
 * The Content-Security-Policy every response carries. It limits the page's sub-resource loads and
 * form submissions to its own origin, while allowing inline and `data:`/`blob:` content, so pages
 * behave as they do under Chromium capture, where requests to other origins are aborted. WebDriver
 * cannot intercept requests, so the browser's own policy enforcement does the blocking. The policy
 * does not stop top-level navigation (a script setting `location`, a meta refresh), resource hints
 * or WebRTC: capture detects a page that has navigated away and fails, but cannot prevent the
 * request. It is a design intent, not a sandbox: a page can still run its own scripts.
 */
export const CONTENT_SECURITY_POLICY =
  "default-src 'self' 'unsafe-inline' 'unsafe-eval' data: blob:; form-action 'self'; base-uri 'self'";

/**
 * Safari asks for `/favicon.ico` on its own, whether or not the page names one, so a missing one
 * is answered with 404 and no diagnostic: the page did not ask for it.
 */
const BROWSER_OWN_REQUESTS: ReadonlySet<string> = new Set(['/favicon.ico']);

export interface ServeOptions {
  /** The directory that answers requests other than the page itself. */
  root: string;
  /** The page's file name; the page is served at `/<file name>`. */
  fileName: string;
  /** The bytes served as the page. */
  body: Uint8Array;
  /** Receives a diagnostic for every request no file answers. */
  diagnostics: Diagnostic[];
}

export interface PageServer {
  /** The URL the page is served at. */
  url: string;
  /** Throws the first error met while answering a request. */
  throwIfServingFailed: () => void;
  close: () => Promise<void>;
}

/** The request's path, with its origin, userinfo, query string and fragment removed. */
function requestPath(url: URL): string {
  return new URL(redactUrl(url.href)).pathname;
}

/**
 * Serves the page and its directory over HTTP on the loopback address, on a port the OS chooses,
 * for the length of the capture. A request whose `Host` is not the server's own address is refused
 * with 421, so a page on another name that resolves to the loopback address cannot read it, and
 * every response carries `Cross-Origin-Resource-Policy: same-origin`, so a page on another origin
 * cannot embed it.
 */
export async function servePage(options: ServeOptions): Promise<PageServer> {
  const { root, fileName, body, diagnostics } = options;
  const documentPath = `/${encodeURIComponent(fileName)}`;
  const failures: unknown[] = [];
  let origin = '';

  const send = (response: ServerResponse, status: number, type: string, contents: Uint8Array | string) => {
    response.writeHead(status, {
      'content-type': type,
      'content-security-policy': CONTENT_SECURITY_POLICY,
      // A page open in another browser on the Mac cannot embed what this server answers.
      'cross-origin-resource-policy': 'same-origin',
      'cache-control': 'no-store',
    });
    response.end(contents);
  };

  const answer = async (request: IncomingMessage, response: ServerResponse): Promise<void> => {
    if (`http://${request.headers.host ?? ''}` !== origin) {
      send(response, 421, 'text/plain; charset=utf-8', 'misdirected request');
      return;
    }
    const url = new URL(request.url ?? '/', origin);
    if (url.pathname === documentPath) {
      send(response, 200, 'text/html; charset=utf-8', body);
      return;
    }
    const file = await resolveAsset(root, url.pathname);
    if (file === undefined) {
      if (!BROWSER_OWN_REQUESTS.has(url.pathname)) {
        diagnostics.push({
          code: 'asset-not-found',
          message: `No file in the page's directory answers ${requestPath(url)}, so it was answered with 404.`,
        });
      }
      send(response, 404, 'text/plain; charset=utf-8', 'not found');
      return;
    }
    let contents: Buffer;
    try {
      contents = await readFile(file);
    } catch (error) {
      throw new CaptureError(
        'asset-unreadable',
        `cannot read ${requestPath(url)} from the page's directory`,
        {
          cause: error,
        },
      );
    }
    send(response, 200, contentType(file), contents);
  };

  const server = createServer((request, response) => {
    answer(request, response).catch((error: unknown) => {
      failures.push(error);
      if (!response.headersSent) {
        send(response, 500, 'text/plain; charset=utf-8', 'server error');
      } else {
        response.destroy();
      }
    });
  });
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, HOST, () => {
      server.off('error', reject);
      resolve();
    });
  });
  const { port } = server.address() as AddressInfo;
  origin = `http://${HOST}:${port}`;

  return {
    url: `${origin}${documentPath}`,
    throwIfServingFailed: () => {
      if (failures.length > 0) {
        throw failures[0];
      }
    },
    close: () =>
      new Promise<void>((resolve, reject) => {
        server.close((error) => (error === undefined ? resolve() : reject(error)));
        server.closeAllConnections();
      }),
  };
}
