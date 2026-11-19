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
   * The answers to successive visibility reads, the last repeated, instead of `visibility`, for a
   * page that is hidden only after a given read.
   */
  visibilitySequence?: string[];
  /**
   * What the page measures, apart from its origin, which is the current page's; the defaults are
   * a 1280 × 720 light viewport.
   */
  measurement?: Partial<Omit<Measurement, 'origin'>>;
  /** The loaded page navigates itself to another URL, as Safari 27 answers it. */
  navigation?: PageNavigation;
  /** Answer Get Current URL with this WebDriver error. */
  currentUrlError?: { error: string; message: string };
  /** Answer the navigation with this WebDriver error instead of loading the page. */
  loadError?: { error: string; message: string };
  /**
   * Paths the page asks its own origin for on every navigation to a page whose HTML names them,
   * as a browser loads the assets a page references.
   */
  fetchOnNavigate?: string[];
}

/**
 * A navigation the loaded page starts by itself, and when it lands relative to capture's calls.
 * Safari 27 answers a script whose page goes away under it with `no such window` and keeps the
 * window on the new page; scripts after a navigation that has finished run in the new page; and
 * an element of the old page answers with `stale element reference`.
 */
export interface PageNavigation {
  /** Where the page goes. */
  to: string;
  /**
   * `settling`: under the settle script, which Safari ends with `no such window`.
   * `after-settling`: once the settle script has answered, so the measurement runs in the new page.
   * `after-measuring`: once the page is measured, so the origin read before the elements runs in it.
   * `element-reads`: once the elements are found, so every element answers as stale.
   */
  during: 'settling' | 'after-settling' | 'after-measuring' | 'element-reads';
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

/** The window rect Get Window Rect answers. */
export const TEST_WINDOW_RECT = { x: 0, y: 25, width: 1280, height: 800 } as const;

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
  let title = '';
  let visibilityReads = 0;
  let currentUrl = 'about:blank';
  let loaded = false;
  let elementsStale = false;
  const navigateAway = (during: PageNavigation['during']) => {
    if (scenario.navigation?.during !== during) {
      return false;
    }
    currentUrl = scenario.navigation.to;
    return true;
  };
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
    if (method === 'GET' && command === '/window/rect') {
      send(response, 200, TEST_WINDOW_RECT);
      return;
    }
    if (method === 'GET' && command === '/title') {
      send(response, 200, title);
      return;
    }
    if (command === '/timeouts' || command === '/window/rect') {
      send(response, 200, null);
      return;
    }
    if (method === 'GET' && command === '/url') {
      if (scenario.currentUrlError !== undefined) {
        send(response, 404, { ...scenario.currentUrlError, stacktrace: '' });
        return;
      }
      send(response, 200, currentUrl);
      return;
    }
    if (command === '/url') {
      if (scenario.loadError !== undefined) {
        send(response, 500, { ...scenario.loadError, stacktrace: '' });
        return;
      }
      const page = await fetch(String(parsed.url));
      currentUrl = String(parsed.url);
      loaded = true;
      const html = await page.text();
      for (const path of (scenario.fetchOnNavigate ?? []).filter((path) => html.includes(path.slice(1)))) {
        const asset = await fetch(new URL(path, String(parsed.url)));
        await asset.body?.cancel();
      }
      log({ event: 'navigated', status: page.status, csp: page.headers.get('content-security-policy') });
      sources = [...html.matchAll(/data-spokenpath-src="([^"]*)"/g)].map((match) => match[1] ?? '');
      title = /<title[^>]*>([^<]*)<\/title>/i.exec(html)?.[1]?.trim() ?? '';
      sources.push(...(scenario.extraSources ?? []));
      send(response, 200, null);
      return;
    }
    if (command === '/execute/sync' || command === '/execute/async') {
      const script = parsed.script;
      const origin = new URL(currentUrl).origin;
      if (script === MEASURE_SCRIPT) {
        // Before the page is loaded the window is on `about:blank`, which the overrides do not describe.
        send(response, 200, { ...DEFAULT_MEASUREMENT, ...(loaded ? scenario.measurement : {}), origin });
        if (loaded) {
          navigateAway('after-measuring');
        }
      } else if (script === ORIGIN_SCRIPT) {
        send(response, 200, origin);
      } else if (script === VISIBILITY_SCRIPT) {
        const sequence = scenario.visibilitySequence;
        const answer =
          sequence === undefined
            ? scenario.visibility
            : sequence[Math.min(visibilityReads, sequence.length - 1)];
        visibilityReads += 1;
        send(response, 200, answer ?? 'visible');
      } else if (script === SETTLE_SCRIPT) {
        if (navigateAway('settling')) {
          send(response, 404, { error: 'no such window', message: '', stacktrace: '' });
          return;
        }
        send(response, 200, true);
        navigateAway('after-settling');
      } else if (script === SELECTOR_SCRIPT) {
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
      // The elements found are the old page's, so each answers as stale once the page has gone.
      elementsStale = navigateAway('element-reads');
      return;
    }
    const element = /^\/element\/element-(\d+)\/(.+)$/.exec(command);
    const source = element === null ? undefined : sources[Number(element[1])];
    if (element === null || source === undefined) {
      send(response, 404, { error: 'no such element', message: command });
      return;
    }
    if (elementsStale) {
      send(response, 404, { error: 'stale element reference', message: '', stacktrace: '' });
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
