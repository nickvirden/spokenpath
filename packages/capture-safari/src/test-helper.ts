/**
 * A test-double accessibility helper that answers as `spokenpath-ax-macos` does, from a per-test
 * scenario, so Safari AX capture can be tested without Safari or the Accessibility permission, on
 * any platform.
 *
 * Run as `node test-helper.ts <scenario.json> <helper arguments>`. `canary-page` prints the check
 * page; `read --canary` and `read` print the scenario's reply, which by default is a full check or
 * a tree built from the served page: it loads the page from capture's server, as the real helper
 * reads Safari, and gives every source class token in the page's `class` attributes a node. Every
 * start is appended to the scenario's log as a JSON line.
 */
import { appendFileSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import type { SafariAxNode } from '@spokenpath/snapshot-types';

/** This script's path, for running it as a helper. */
export const testHelperPath: string = fileURLToPath(import.meta.url);

/** The check page the double prints, with the real check page's three tokens. */
export const TEST_CANARY_PAGE = `<!doctype html>
<title>spokenpath accessibility check</title>
<h1 class="sp-canary-1">Check</h1><p class="sp-canary-2">Check</p><button class="sp-canary-3">Check</button>
`;

/** What the double does for one command. */
export type HelperReply =
  /** Print the tree built from the served page. */
  | { kind: 'tree' }
  /** Print a check result with `found` of `expected` (3 by default) tokens, and exit 0. */
  | { kind: 'canary'; found: number; expected?: number }
  /** Fail as the helper does: the JSON error on stdout, the line on stderr, and `exit`. */
  | { kind: 'error'; code: string; message: string; exit: number }
  /** Print `stdout` and `stderr` as given and exit with `exit`. */
  | { kind: 'output'; stdout: string; stderr?: string; exit: number }
  /** Never answer. */
  | { kind: 'hang' }
  /** Print without end, past any limit. */
  | { kind: 'flood' };

export interface HelperScenario {
  /** The file every start is appended to, one JSON object per line. */
  log: string;
  /** The reply to `read --canary`; a full check by default. */
  canary?: HelperReply;
  /** The reply to `read`; the served page's tree by default. */
  read?: HelperReply;
  /** Source class tokens given to nodes of their own after the page's. */
  extraTokens?: string[];
}

/** One line of the log. */
export interface HelperEvent {
  event: 'helper-started';
  pid: number;
  args: string[];
}

const TOKEN = /\bsp-src-\d+-\d+\b/g;

/** A tree like Safari's for the page at `url`: a web area, then one node per tagged element. */
async function treeOf(url: string, extraTokens: readonly string[]): Promise<SafariAxNode[]> {
  const html = await (await fetch(url)).text();
  const tokens = [...html.matchAll(/class="([^"]*)"/g)].flatMap((match) => match[1]?.match(TOKEN) ?? []);
  const children = [...tokens, ...extraTokens].map(
    (token, index): SafariAxNode => ({
      id: index + 2,
      parent: 0,
      children: [],
      attributes: {
        AXRole: { type: 'string', value: 'AXGroup' },
        AXDOMClassList: { type: 'array', items: [{ type: 'string', value: token }] },
        AXParent: { type: 'element', node: 0 },
      },
    }),
  );
  const text: SafariAxNode = {
    id: 1,
    parent: 0,
    children: [],
    attributes: {
      AXRole: { type: 'string', value: 'AXStaticText' },
      AXDOMClassList: { type: 'array', items: [] },
    },
  };
  const ids = [text, ...children].map((node) => ({ type: 'element' as const, node: node.id }));
  return [
    {
      id: 0,
      children: ids.map((element) => element.node),
      attributes: {
        AXRole: { type: 'string', value: 'AXWebArea' },
        AXURL: { type: 'url', value: url },
        AXChildren: { type: 'array', items: ids },
        AXParent: { type: 'element' },
        AXPosition: { type: 'point', x: 0, y: 87 },
      },
    },
    text,
    ...children,
  ];
}

function print(value: unknown): void {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

async function reply(answer: HelperReply, url: string, scenario: HelperScenario): Promise<void> {
  switch (answer.kind) {
    case 'tree':
      print({ nodes: await treeOf(url, scenario.extraTokens ?? []) });
      return;
    case 'canary':
      print({ canary: { expected: answer.expected ?? 3, found: answer.found } });
      return;
    case 'error':
      print({ error: { code: answer.code, message: answer.message } });
      process.stderr.write(`spokenpath-ax-macos: ${answer.code}: ${answer.message}\n`);
      process.exitCode = answer.exit;
      return;
    case 'output':
      process.stdout.write(answer.stdout);
      process.stderr.write(answer.stderr ?? '');
      process.exitCode = answer.exit;
      return;
    case 'hang':
      setInterval(() => {}, 60_000);
      return;
    case 'flood': {
      const chunk = Buffer.alloc(1024 * 1024, 0x20);
      const write = () => {
        while (process.stdout.write(chunk)) {
          // Keeps writing until the pipe is full, then waits for it to drain.
        }
        process.stdout.once('drain', write);
      };
      write();
      return;
    }
  }
}

async function main(args: string[]): Promise<void> {
  const [scenarioPath, command, ...rest] = args;
  if (scenarioPath === undefined) {
    throw new Error('usage: test-helper.ts <scenario.json> <helper arguments>');
  }
  const scenario = JSON.parse(readFileSync(scenarioPath, 'utf8')) as HelperScenario;
  appendFileSync(
    scenario.log,
    `${JSON.stringify({ event: 'helper-started', pid: process.pid, args: args.slice(1) } satisfies HelperEvent)}\n`,
  );
  if (command === 'canary-page') {
    print({ html: TEST_CANARY_PAGE });
    return;
  }
  const url = rest[rest.indexOf('--url') + 1] ?? '';
  if (rest.includes('--canary')) {
    await reply(scenario.canary ?? { kind: 'canary', found: 3 }, url, scenario);
  } else {
    await reply(scenario.read ?? { kind: 'tree' }, url, scenario);
  }
}

if (import.meta.main) {
  await main(process.argv.slice(2));
}
