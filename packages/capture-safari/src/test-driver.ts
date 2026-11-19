/**
 * A test-double WebDriver server that speaks the endpoints Safari capture uses, answering from a
 * per-test scenario, so capture can be tested without Safari, on any platform.
 *
 * Run as `node test-driver.ts <scenario.json> --port <n>`, or with `--version`. On navigation it
 * loads the page from capture's server, as a browser would, records the response's status and
 * Content-Security-Policy, and finds the tagged elements in the served HTML. Every request and
 * the driver's own start and stop are appended to the scenario's log as JSON lines.
 */
import { appendFileSync, readFileSync } from 'node:fs';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { fileURLToPath } from 'node:url';
import {
  MEASURE_SCRIPT,
  type Measurement,
  ORIGIN_SCRIPT,
  SELECTOR_SCRIPT,
  SETTLE_SCRIPT,
  VISIBILITY_SCRIPT,
} from './scripts.ts';
import { ELEMENT_KEY } from './webdriver.ts';

/** This script's path, for running it as a driver. */
export const testDriverPath: string = fileURLToPath(import.meta.url);

/** An answer to a role or label call: the value, a WebDriver error, or no answer at all. */
export type Reply = string | { error: string; message: string } | 'hang';

export interface DriverScenario {
  /** The file every event is appended to, one JSON object per line. */
  log: string;
  /** What `--version` prints. */
  version?: string;
  /** Written to stderr when the driver starts. */
  stderr?: string;
  /** Exit with status 1 right after writing `stderr`, before listening. */
  exitAtStart?: boolean;
  /** Refuse the session with this WebDriver error. */
  refuseSession?: { error: string; message: string };
  /** The `browserVersion` capability of the session. */
  browserVersion?: string;
  /** Answers by `data-spokenpath-src` value; an element without one is `generic` with no label. */
  answers?: Record<string, { role?: Reply; label?: Reply }>;
  /** Source attribute values of elements found after the page's own. */
  extraSources?: string[];
  /** The page's `document.visibilityState`; `visible` by default. */
  visibility?: string;
  /**
   * What the page measures; the defaults are a 1280 × 720 light viewport, with the origin of the
   * loaded page, or `null` before one has loaded.
   */
  measurement?: Partial<Measurement>;
  /** The origin the page reports just before its elements are found; the loaded page's by default. */
  originBeforeElements?: string;
  /** Answer the navigation with this WebDriver error instead of loading the page. */
  loadError?: { error: string; message: string };
}

/** One line of the log. */
export interface DriverEvent {
  event: 'started' | 'request' | 'navigated' | 'session-created' | 'session-deleted' | 'terminated';
  pid?: number;
  method?: string;
  path?: string;
  status?: number;
  csp?: string | null;
  signal?: string;
}

const DEFAULT_MEASUREMENT: Omit<Measurement, 'origin'> = {
  innerWidth: 1280,
  innerHeight: 720,
  outerWidth: 1280,
  outerHeight: 800,
  devicePixelRatio: 2,
  dark: false,
  light: true,
  reducedMotion: false,
};

function main(args: string[]): void {
  const [scenarioPath, flag, port] = args;
  if (scenarioPath === undefined) {
    throw new Error('usage: test-driver.ts <scenario.json> (--port <n> | --version)');
  }
  const scenario = JSON.parse(readFileSync(scenarioPath, 'utf8')) as DriverScenario;
  if (flag === '--version') {
    process.stdout.write(`${scenario.version ?? 'test double 1.0'}\n`);
    return;
  }
  const log = (event: DriverEvent) => appendFileSync(scenario.log, `${JSON.stringify(event)}\n`);
  if (scenario.stderr !== undefined) {
    process.stderr.write(scenario.stderr);
  }
  if (scenario.exitAtStart === true) {
    process.exitCode = 1;
    return;
  }

  const sessionId = 'test-session';
  let sources: string[] = [];
  let origin = 'null';
  const send = (response: ServerResponse, status: number, value: unknown) => {
    response.writeHead(status, { 'content-type': 'application/json; charset=utf-8' });
    response.end(JSON.stringify({ value }));
  };
  const reply = (response: ServerResponse, answer: Reply) => {
    if (answer === 'hang') {
      return;
    }
    if (typeof answer === 'string') {
      send(response, 200, answer);
    } else {
      send(response, 500, { ...answer, stacktrace: '' });
    }
  };

  const handle = async (request: IncomingMessage, response: ServerResponse, body: string) => {
    const method = request.method ?? 'GET';
    const path = new URL(request.url ?? '/', 'http://driver').pathname;
    log({ event: 'request', method, path });
    const parsed: Record<string, unknown> = body === '' ? {} : (JSON.parse(body) as Record<string, unknown>);
    if (path === '/status') {
      send(response, 200, { ready: true, message: '' });
      return;
    }
    if (method === 'POST' && path === '/session') {
      if (scenario.refuseSession !== undefined) {
        send(response, 500, { ...scenario.refuseSession, stacktrace: '' });
        return;
      }
      log({ event: 'session-created' });
      send(response, 200, {
        sessionId,
        capabilities: { browserName: 'Safari', browserVersion: scenario.browserVersion ?? '27.0.1' },
      });
      return;
    }
    const prefix = `/session/${sessionId}`;
    if (!path.startsWith(prefix)) {
      send(response, 404, { error: 'invalid session id', message: path });
      return;
    }
    const command = path.slice(prefix.length);
    if (method === 'DELETE' && command === '') {
      log({ event: 'session-deleted' });
      send(response, 200, null);
      return;
    }
    if (command === '/timeouts' || command === '/window/rect') {
      send(response, 200, null);
      return;
    }
    if (command === '/url') {
      if (scenario.loadError !== undefined) {
        send(response, 500, { ...scenario.loadError, stacktrace: '' });
        return;
      }
      const page = await fetch(String(parsed.url));
      origin = new URL(String(parsed.url)).origin;
      const html = await page.text();
      log({ event: 'navigated', status: page.status, csp: page.headers.get('content-security-policy') });
      sources = [...html.matchAll(/data-spokenpath-src="([^"]*)"/g)].map((match) => match[1] ?? '');
      sources.push(...(scenario.extraSources ?? []));
      send(response, 200, null);
      return;
    }
    if (command === '/execute/sync' || command === '/execute/async') {
      const script = parsed.script;
      if (script === MEASURE_SCRIPT) {
        // Before navigation the page is `about:blank`, and an origin override applies only after it.
        const measured = {
          ...DEFAULT_MEASUREMENT,
          origin,
          ...(origin === 'null' ? {} : scenario.measurement),
        };
        send(response, 200, measured);
      } else if (script === ORIGIN_SCRIPT) {
        send(response, 200, scenario.originBeforeElements ?? origin);
      } else if (script === VISIBILITY_SCRIPT) {
        send(response, 200, scenario.visibility ?? 'visible');
      } else if (script === SELECTOR_SCRIPT || script === SETTLE_SCRIPT) {
        send(response, 200, true);
      } else {
        send(response, 500, { error: 'javascript error', message: 'unknown script' });
      }
      return;
    }
    if (command === '/elements') {
      send(
        response,
        200,
        sources.map((_, index) => ({ [ELEMENT_KEY]: `element-${index}` })),
      );
      return;
    }
    const element = /^\/element\/element-(\d+)\/(.+)$/.exec(command);
    const source = element === null ? undefined : sources[Number(element[1])];
    if (element === null || source === undefined) {
      send(response, 404, { error: 'no such element', message: command });
      return;
    }
    const what = element[2];
    const answers = scenario.answers?.[source] ?? {};
    if (what === 'attribute/data-spokenpath-src') {
      send(response, 200, source);
    } else if (what === 'computedrole') {
      reply(response, answers.role ?? 'generic');
    } else if (what === 'computedlabel') {
      reply(response, answers.label ?? '');
    } else {
      send(response, 404, { error: 'unknown command', message: command });
    }
  };

  const server = createServer((request, response) => {
    let body = '';
    request.setEncoding('utf8');
    request.on('data', (chunk: string) => {
      body += chunk;
    });
    request.on('end', () => {
      handle(request, response, body).catch((error: unknown) => {
        send(response, 500, { error: 'unknown error', message: String(error) });
      });
    });
  });
  for (const signal of ['SIGTERM', 'SIGINT'] as const) {
    process.on(signal, () => {
      log({ event: 'terminated', signal });
      process.exit(0);
    });
  }
  server.listen(Number(port), '127.0.0.1', () => log({ event: 'started', pid: process.pid }));
}

if (import.meta.main) {
  main(process.argv.slice(2));
}
