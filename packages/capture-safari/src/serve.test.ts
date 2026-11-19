import { request } from 'node:http';
import { afterEach, expect, test } from 'vitest';
import { pagesUrl } from '@spokenpath/capture-chromium/fixture-pages';
import { fileURLToPath } from 'node:url';
import type { Diagnostic } from '@spokenpath/snapshot-types';
import { CONTENT_SECURITY_POLICY, HOST, type PageServer, servePage } from './serve.ts';

const root = fileURLToPath(pagesUrl);
let server: PageServer | undefined;

afterEach(async () => {
  await server?.close();
  server = undefined;
});

async function serve(diagnostics: Diagnostic[] = []): Promise<PageServer> {
  server = await servePage({
    root,
    fileName: 'page one.html',
    body: Buffer.from('<p>served</p>'),
    diagnostics,
  });
  return server;
}

/** A GET of `path` on the server's port with the `Host` header given. */
function get(url: string, host?: string): Promise<{ status: number; headers: Record<string, unknown> }> {
  const { hostname, port, pathname } = new URL(url);
  return new Promise((resolve, reject) => {
    const outgoing = request(
      { hostname, port, path: pathname, headers: host === undefined ? {} : { host } },
      (response) => {
        response.resume();
        response.on('end', () => resolve({ status: response.statusCode ?? 0, headers: response.headers }));
      },
    );
    outgoing.on('error', reject);
    outgoing.end();
  });
}

test('the server listens on the loopback address only and serves the page at its file name', async () => {
  const { url } = await serve();
  expect(new URL(url).hostname).toBe(HOST);
  expect(HOST).toBe('127.0.0.1');
  expect(new URL(url).pathname).toBe('/page%20one.html');
  const response = await fetch(url);
  expect(response.status).toBe(200);
  expect(await response.text()).toBe('<p>served</p>');
  expect(response.headers.get('content-type')).toBe('text/html; charset=utf-8');
});

test('every response carries the Content-Security-Policy that limits loads to the origin', async () => {
  const { url } = await serve();
  const origin = new URL(url).origin;
  for (const path of ['/page%20one.html', '/assets/local-asset.css', '/missing.png']) {
    const response = await fetch(`${origin}${path}`);
    await response.body?.cancel();
    expect(response.headers.get('content-security-policy'), path).toBe(CONTENT_SECURITY_POLICY);
  }
  expect(CONTENT_SECURITY_POLICY).toMatch(/^default-src 'self' /);
  expect(CONTENT_SECURITY_POLICY).not.toMatch(/\*|https?:/);
});

test('a request no file answers gets 404 and a diagnostic without the port', async () => {
  const diagnostics: Diagnostic[] = [];
  const { url } = await serve(diagnostics);
  const origin = new URL(url).origin;
  const missing = await fetch(`${origin}/missing.png?token=secret`);
  await missing.body?.cancel();
  const dotted = await fetch(`${origin}/.env`);
  await dotted.body?.cancel();
  expect([missing.status, dotted.status]).toEqual([404, 404]);
  expect(diagnostics).toEqual([
    {
      code: 'asset-not-found',
      message: "No file in the page's directory answers /missing.png, so it was answered with 404.",
    },
    {
      code: 'asset-not-found',
      message: "No file in the page's directory answers /.env, so it was answered with 404.",
    },
  ]);
});

test("Safari's own favicon request gets 404 and no diagnostic", async () => {
  const diagnostics: Diagnostic[] = [];
  const { url } = await serve(diagnostics);
  const response = await fetch(new URL('/favicon.ico', url));
  await response.body?.cancel();
  expect(response.status).toBe(404);
  expect(diagnostics).toEqual([]);
});

test('a request naming another host is refused', async () => {
  const { url } = await serve();
  const { status } = await get(url, 'attacker.example');
  expect(status).toBe(421);
});
