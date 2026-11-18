import { readFile, realpath, stat } from 'node:fs/promises';
import { extname, isAbsolute, relative, resolve, sep } from 'node:path';
import type { Diagnostic } from '@spokenpath/snapshot-types';
import type { BrowserContext, Route } from 'playwright-core';
import { CaptureError } from './error.ts';

/**
 * The fictional origin the page is served from. Every request is answered by interception, so no
 * name is resolved and no socket is opened; any other origin is refused.
 */
export const ORIGIN = 'http://spokenpath.test';

/** The URL the page with file name `fileName` is served at. */
export function pageUrl(fileName: string): string {
  return `${ORIGIN}/${encodeURIComponent(fileName)}`;
}

/** `url` without its userinfo, query string and fragment, which can carry secrets. */
export function redactUrl(url: string): string {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    // An unparseable URL cannot be split safely, so none of it is kept.
    return '(an unparseable URL)';
  }
  parsed.username = '';
  parsed.password = '';
  parsed.search = '';
  parsed.hash = '';
  return parsed.href;
}

const CONTENT_TYPES: Readonly<Record<string, string>> = {
  '.avif': 'image/avif',
  '.css': 'text/css',
  '.gif': 'image/gif',
  '.htm': 'text/html',
  '.html': 'text/html',
  '.ico': 'image/x-icon',
  '.jpeg': 'image/jpeg',
  '.jpg': 'image/jpeg',
  '.js': 'text/javascript',
  '.json': 'application/json',
  '.mjs': 'text/javascript',
  '.otf': 'font/otf',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.ttf': 'font/ttf',
  '.txt': 'text/plain',
  '.wasm': 'application/wasm',
  '.webp': 'image/webp',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.xml': 'application/xml',
};

/** The content type for a file, by extension; text types are declared UTF-8. */
export function contentType(path: string): string {
  const type = CONTENT_TYPES[extname(path).toLowerCase()] ?? 'application/octet-stream';
  return type.startsWith('text/') ? `${type}; charset=utf-8` : type;
}

/** Errors from the file system that mean "no file answers this path". */
const NOT_FOUND_CODES: ReadonlySet<string> = new Set([
  'ENOENT',
  'ENOTDIR',
  'EISDIR',
  'ELOOP',
  'ENAMETOOLONG',
]);

function isNotFound(error: unknown): boolean {
  return (
    error instanceof Error &&
    'code' in error &&
    typeof error.code === 'string' &&
    NOT_FOUND_CODES.has(error.code)
  );
}

/**
 * The real path of the file that answers the URL path `pathname` from the directory `root`, or
 * nothing when no file does: when the path names no regular file, has a segment starting with
 * `.`, or resolves outside `root` once symbolic links are followed. Dot segments are refused
 * because they name files such as `.env` or `.git/` that a page directory can hold but a page
 * has no reason to load.
 */
export async function resolveAsset(root: string, pathname: string): Promise<string | undefined> {
  let decoded: string;
  try {
    decoded = decodeURIComponent(pathname);
  } catch (error) {
    if (error instanceof URIError) {
      return undefined;
    }
    throw error;
  }
  if (decoded.includes('\0') || decoded.split('/').some((segment) => segment.startsWith('.'))) {
    return undefined;
  }
  const realRoot = await realpath(root);
  let target: string;
  try {
    target = await realpath(resolve(realRoot, `.${decoded}`));
  } catch (error) {
    if (isNotFound(error)) {
      return undefined;
    }
    throw error;
  }
  const fromRoot = relative(realRoot, target);
  if (fromRoot === '' || fromRoot === '..' || fromRoot.startsWith(`..${sep}`) || isAbsolute(fromRoot)) {
    return undefined;
  }
  return (await stat(target)).isFile() ? target : undefined;
}

export interface ServeOptions {
  /** The directory that answers requests on the origin other than the page itself. */
  root: string;
  /** The page's file name; the page is served at `pageUrl(fileName)`. */
  fileName: string;
  /** The bytes served as the page. */
  body: Uint8Array;
  /** Receives a diagnostic for every refused request. */
  diagnostics: Diagnostic[];
}

/**
 * Serves the page and its directory to `context` by request interception, and refuses everything
 * else: requests to other origins are aborted and WebSockets are closed, each with a diagnostic.
 * Returns a check that throws the first error a request handler met, such as an unreadable file,
 * so the capture fails rather than recording a page that was served wrongly.
 */
export async function servePage(context: BrowserContext, options: ServeOptions): Promise<() => void> {
  const { root, fileName, body, diagnostics } = options;
  const documentPath = new URL(pageUrl(fileName)).pathname;
  const failures: unknown[] = [];

  const answer = async (route: Route): Promise<void> => {
    const url = new URL(route.request().url());
    if (url.origin !== ORIGIN) {
      diagnostics.push({
        code: 'external-request-blocked',
        message: `A request to ${redactUrl(url.href)} was blocked: capture serves only the page's own directory.`,
      });
      await route.abort('blockedbyclient');
      return;
    }
    if (url.pathname === documentPath) {
      await route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body: Buffer.from(body) });
      return;
    }
    const file = await resolveAsset(root, url.pathname);
    if (file === undefined) {
      diagnostics.push({
        code: 'asset-not-found',
        message: `No file in the page's directory answers ${redactUrl(url.href)}, so it was answered with 404.`,
      });
      await route.fulfill({ status: 404, contentType: 'text/plain; charset=utf-8', body: 'not found' });
      return;
    }
    let contents: Buffer;
    try {
      contents = await readFile(file);
    } catch (error) {
      throw new CaptureError(
        'asset-unreadable',
        `cannot read ${redactUrl(url.href)} from the page's directory`,
        {
          cause: error,
        },
      );
    }
    await route.fulfill({ status: 200, contentType: contentType(file), body: contents });
  };

  await context.route('**/*', async (route) => {
    try {
      await answer(route);
    } catch (error) {
      failures.push(error);
      await route.abort('failed');
    }
  });

  await context.routeWebSocket(
    () => true,
    async (webSocket) => {
      diagnostics.push({
        code: 'external-request-blocked',
        message: `A WebSocket to ${redactUrl(webSocket.url())} was closed: capture opens no network connections.`,
      });
      await webSocket.close();
    },
  );

  return () => {
    if (failures.length > 0) {
      throw failures[0];
    }
  };
}
