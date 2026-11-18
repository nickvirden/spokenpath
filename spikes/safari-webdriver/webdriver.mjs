// A dependency-free WebDriver client for safaridriver, plus the fixture
// server both Safari probes load pages from. Only what the probes call is here.
import { execFileSync, spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { createServer as createNetServer } from 'node:net';
import { extname, join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

export const fixtureDir = fileURLToPath(new URL('./fixture/', import.meta.url));

// WebDriver's element key, from the W3C WebDriver specification.
const elementKey = 'element-6066-11e4-a52e-4f735466cecf';
const contentTypes = { '.html': 'text/html; charset=utf-8', '.svg': 'image/svg+xml' };

export function macosVersion() {
  const read = (key) => execFileSync('sw_vers', [key], { encoding: 'utf8' }).trim();
  return { productVersion: read('-productVersion'), buildVersion: read('-buildVersion') };
}

export function safariVersion() {
  const plist = '/Applications/Safari.app/Contents/Info.plist';
  const read = (key) => execFileSync('defaults', ['read', plist, key], { encoding: 'utf8' }).trim();
  return { shortVersion: read('CFBundleShortVersionString'), bundleVersion: read('CFBundleVersion') };
}

async function freePort() {
  const server = createNetServer();
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const { port } = server.address();
  await new Promise((resolve) => server.close(resolve));
  return port;
}

// Serves the fixture folder on 127.0.0.1. `transform` rewrites an HTML page
// per request path, so one fixture yields the carrier variants.
export async function startFixtureServer(transform = (_path, html) => html) {
  const server = createServer(async (request, response) => {
    const { pathname } = new URL(request.url, 'http://127.0.0.1');
    const name = pathname.slice(1);
    try {
      const html = name.endsWith('.html');
      const fileName = html ? name.replace(/-[a-z]+\.html$/, '.html') : name;
      const type = contentTypes[extname(fileName)];
      if (!type || fileName.includes('/')) throw Object.assign(new Error('unsupported'), { code: 'ENOENT' });
      let body = await readFile(join(fixtureDir, fileName));
      if (html) body = transform(pathname, body.toString('utf8'));
      response.writeHead(200, { 'content-type': type });
      response.end(body);
    } catch (error) {
      response.writeHead(error.code === 'ENOENT' ? 404 : 500, { 'content-type': 'text/plain' });
      response.end(error.code === 'ENOENT' ? 'not found' : 'server error');
    }
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  return {
    origin: `http://127.0.0.1:${server.address().port}`,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}

export class WebDriverError extends Error {
  constructor(status, body) {
    super(`${status} ${body?.value?.error ?? ''}: ${body?.value?.message ?? JSON.stringify(body)}`);
    this.status = status;
    this.body = body;
  }
}

// Every request has a deadline, so a hung safaridriver or Safari surfaces as a
// recorded error and the caller's cleanup still deletes the session and stops
// safaridriver. Starting a session launches Safari and a navigation loads a
// page, so those two get a longer deadline than an element call.
const statusTimeoutMs = 1_000;
const commandTimeoutMs = 10_000;
const launchOrLoadTimeoutMs = 60_000;

function timeoutFor(method, path) {
  const launchesOrLoads = method === 'POST' && (path === '/session' || path.endsWith('/url'));
  return launchesOrLoads ? launchOrLoadTimeoutMs : commandTimeoutMs;
}

export async function startSafaridriver() {
  const port = await freePort();
  const child = spawn('safaridriver', ['--port', String(port)], { stdio: ['ignore', 'pipe', 'pipe'] });
  let stderr = '';
  child.stderr.on('data', (chunk) => {
    stderr += chunk;
  });
  const exited = new Promise((resolve) => child.once('exit', (code) => resolve(code)));
  const base = `http://127.0.0.1:${port}`;
  for (let attempt = 0; attempt < 50; attempt += 1) {
    if (child.exitCode !== null) break;
    try {
      const response = await fetch(`${base}/status`, { signal: AbortSignal.timeout(statusTimeoutMs) });
      if (response.ok) {
        return {
          base,
          stop: async () => {
            if (child.exitCode === null) child.kill('SIGTERM');
            await exited;
          },
        };
      }
    } catch {
      // Not listening yet; the loop retries until the deadline.
    }
    await sleep(100);
  }
  if (child.exitCode === null) child.kill('SIGTERM');
  await exited;
  throw new Error(`safaridriver did not start: ${stderr.trim() || 'no output'}`);
}

export async function command(base, method, path, body) {
  const response = await fetch(`${base}${path}`, {
    method,
    headers: { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(timeoutFor(method, path)),
  });
  const json = await response.json();
  if (!response.ok) throw new WebDriverError(response.status, json);
  return json.value;
}

export async function newSession(base) {
  const value = await command(base, 'POST', '/session', {
    capabilities: { alwaysMatch: { browserName: 'safari' } },
  });
  const { capabilities } = value;
  return {
    id: value.sessionId,
    capabilities: {
      browserName: capabilities.browserName,
      browserVersion: capabilities.browserVersion,
      platformName: capabilities.platformName,
      'safari:platformVersion': capabilities['safari:platformVersion'],
    },
  };
}

export function elementId(reference) {
  return reference[elementKey];
}
