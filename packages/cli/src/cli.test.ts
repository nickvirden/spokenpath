import { execFile } from 'node:child_process';
import {
  closeSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { CaptureError as SafariCaptureError, captureSafari } from '@spokenpath/capture-safari';
import { testDriverPath } from '@spokenpath/capture-safari/test-driver';
import { Ajv2020 } from 'ajv/dist/2020.js';
import { afterAll, describe, expect, test } from 'vitest';
import { binaryPath, repositoryRoot } from './binary.ts';
import { escapeForTerminal } from './escape.ts';
import { describeFailure, failureText } from './failure.ts';
import { SECOND_SNAPSHOT_FD_PATH, transcribe } from './transcribe.ts';

const cliPath = fileURLToPath(new URL('cli.ts', import.meta.url));
const goldenPath = join(repositoryRoot, 'fixtures/transcripts/sign-in.txt');
const signInPage = 'fixtures/pages/sign-in.html';
const UPDATE_COMMAND = 'UPDATE_FIXTURES=1 pnpm test:e2e';

interface Run {
  code: number;
  stdout: string;
  stderr: string;
}

function execute(command: string, args: string[], env: NodeJS.ProcessEnv = process.env): Promise<Run> {
  return new Promise((resolve, reject) => {
    execFile(
      command,
      args,
      { cwd: repositoryRoot, env, maxBuffer: 64 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error !== null && typeof error.code !== 'number') {
          reject(error);
          return;
        }
        resolve({ code: error === null ? 0 : (error.code as number), stdout, stderr });
      },
    );
  });
}

/** Runs the `spokenpath` command from the repository root, as `pnpm spokenpath` does. */
function runCli(args: string[], env?: NodeJS.ProcessEnv): Promise<Run> {
  return execute(process.execPath, [cliPath, ...args], env);
}

function runBinary(args: string[]): Promise<Run> {
  return execute(binaryPath(), args);
}

/** The text output with the header's spokenpath version normalised, as the golden stores it. */
function normaliseVersion(text: string): string {
  return text.replace(/^spokenpath [^ ]+ /, 'spokenpath 0.0.0 ');
}

/** Every key anywhere in a JSON value. */
function keysIn(value: unknown, keys: Set<string> = new Set()): Set<string> {
  if (Array.isArray(value)) {
    for (const item of value) {
      keysIn(item, keys);
    }
  } else if (typeof value === 'object' && value !== null) {
    for (const [key, item] of Object.entries(value)) {
      keys.add(key);
      keysIn(item, keys);
    }
  }
  return keys;
}

/** The transcript schema in strict mode, with the integer and number formats schemars writes. */
const validateTranscript = (() => {
  const ajv = new Ajv2020({ strict: true, allErrors: true });
  const integer = (min: number, max: number) => (value: number) =>
    Number.isInteger(value) && value >= min && value <= max;
  ajv.addFormat('uint', { type: 'number', validate: integer(0, Number.MAX_SAFE_INTEGER) });
  ajv.addFormat('uint32', { type: 'number', validate: integer(0, 0xffff_ffff) });
  ajv.addFormat('uint64', { type: 'number', validate: integer(0, Number.MAX_SAFE_INTEGER) });
  ajv.addFormat('int64', {
    type: 'number',
    validate: integer(Number.MIN_SAFE_INTEGER, Number.MAX_SAFE_INTEGER),
  });
  ajv.addFormat('double', { type: 'number', validate: (value: number) => Number.isFinite(value) });
  const schema: unknown = JSON.parse(readFileSync(join(repositoryRoot, 'schema/transcript.v0.json'), 'utf8'));
  return ajv.compile(schema as object);
})();

function schemaErrors(json: string): unknown[] {
  return validateTranscript(JSON.parse(json)) ? [] : (validateTranscript.errors ?? []);
}

/** Every committed capture, relative to `fixtures/snapshots/chromium/`. */
function committedCaptures(): string[] {
  const root = join(repositoryRoot, 'fixtures/snapshots/chromium');
  return readdirSync(root, { recursive: true, encoding: 'utf8' })
    .filter((path) => path.endsWith('.json'))
    .map((path) => path.split('\\').join('/'))
    .sort();
}

const temporaryDirectories: string[] = [];

function temporaryDirectory(): string {
  const directory = mkdtempSync(join(tmpdir(), 'spokenpath-cli-'));
  temporaryDirectories.push(directory);
  return directory;
}

afterAll(() => {
  for (const directory of temporaryDirectories) {
    rmSync(directory, { recursive: true, force: true });
  }
});

test('the sign-in transcript equals its committed golden', async () => {
  const { code, stdout, stderr } = await runCli([signInPage]);
  expect(stderr).toBe('');
  expect(code).toBe(0);
  const normalised = normaliseVersion(stdout);
  if (process.env.UPDATE_FIXTURES === '1') {
    mkdirSync(dirname(goldenPath), { recursive: true });
    writeFileSync(goldenPath, normalised);
  }
  expect(
    normalised,
    `the transcript differs from its golden; review the change as drift, then refresh it with \`${UPDATE_COMMAND}\``,
  ).toBe(readFileSync(goldenPath, 'utf8'));
});

test('the JSON output validates against the transcript schema', async () => {
  const { code, stdout, stderr } = await runCli([signInPage, '--format', 'json']);
  expect(stderr).toBe('');
  expect(code).toBe(0);
  expect(schemaErrors(stdout)).toEqual([]);
  expect(keysIn(JSON.parse(stdout)).has('approx')).toBe(true);
});

test('the binary JSON output of every committed capture validates against the transcript schema', async () => {
  const captures = committedCaptures();
  expect(captures).toContain('sign-in.json');
  for (const capture of captures) {
    const snapshotPath = join(repositoryRoot, 'fixtures/snapshots/chromium', capture);
    const snapshot = JSON.parse(readFileSync(snapshotPath, 'utf8')) as { sources: { path: string }[] };
    const [source] = snapshot.sources;
    expect(source, capture).toBeDefined();
    const page = join(repositoryRoot, 'fixtures/pages', dirname(capture), source?.path ?? '');
    const { code, stdout, stderr } = await runBinary([
      'transcript',
      snapshotPath,
      '--source',
      page,
      '--format',
      'json',
    ]);
    expect(code, `${capture}: ${stderr}`).toBe(0);
    expect(schemaErrors(stdout), capture).toEqual([]);
  }
});

/** Every committed capture of Safari's accessibility tree, relative to `fixtures/snapshots/safari-ax/`. */
function committedSafariCaptures(): string[] {
  const root = join(repositoryRoot, 'fixtures/snapshots/safari-ax');
  return readdirSync(root, { recursive: true, encoding: 'utf8' })
    .filter((path) => path.endsWith('.json') && basename(path) !== 'source-mapping.json')
    .map((path) => path.split('\\').join('/'))
    .sort();
}

/** The page of a capture's first source, under `fixtures/pages/` at the capture's directory. */
function pageOf(snapshotPath: string, capture: string): string {
  const snapshot = JSON.parse(readFileSync(snapshotPath, 'utf8')) as { sources: { path: string }[] };
  return join(repositoryRoot, 'fixtures/pages', dirname(capture), snapshot.sources[0]?.path ?? '');
}

test('the binary JSON of every Safari capture and every aligned pair validates against the transcript schema', async () => {
  const captures = committedSafariCaptures();
  expect(captures.length).toBeGreaterThanOrEqual(46);
  const snapshots = join(repositoryRoot, 'fixtures/snapshots');
  const runs: [string, string[]][] = [
    ['safari-ax-basic', ['transcript', join(snapshots, 'safari-ax-basic.json'), '--format', 'json']],
    [
      'the hand-written pair',
      [
        'transcript',
        join(snapshots, 'chromium-basic.json'),
        '--align-with',
        join(snapshots, 'safari-ax-basic.json'),
        '--format',
        'json',
      ],
    ],
  ];
  for (const capture of captures) {
    const safari = join(snapshots, 'safari-ax', capture);
    const page = pageOf(safari, capture);
    runs.push([capture, ['transcript', safari, '--source', page, '--format', 'json']]);
    runs.push([
      `${capture} aligned`,
      [
        'transcript',
        join(snapshots, 'chromium', capture),
        '--source',
        page,
        '--align-with',
        safari,
        '--format',
        'json',
      ],
    ]);
  }
  for (const [name, args] of runs) {
    const { code, stdout, stderr } = await runBinary(args);
    expect(code, `${name}: ${stderr}`).toBe(0);
    expect(schemaErrors(stdout), name).toEqual([]);
    const transcript = JSON.parse(stdout) as { not_available?: string[]; aligned?: { rows: unknown[] } };
    if (name.endsWith('aligned') || name === 'the hand-written pair') {
      expect(transcript.aligned?.rows.length, name).toBeGreaterThan(0);
    } else {
      expect(transcript.not_available, name).toEqual(['name-sources', 'excluded', 'tab-order']);
    }
  }
});

test('the binary JSON output of a cross-check validates against the transcript schema', async () => {
  const snapshots = join(repositoryRoot, 'fixtures/snapshots');
  const { code, stdout, stderr } = await runBinary([
    'transcript',
    join(snapshots, 'chromium-basic.json'),
    '--cross-check',
    join(snapshots, 'safari-webdriver-basic.json'),
    '--format',
    'json',
  ]);
  expect(code, stderr).toBe(0);
  expect(schemaErrors(stdout)).toEqual([]);
  const { nodes } = JSON.parse(stdout) as {
    nodes: { cross_check?: { role: { outcome: string }; name: { outcome: string } } }[];
  };
  const outcomes = nodes.flatMap((node) =>
    node.cross_check === undefined ? [] : [node.cross_check.role.outcome, node.cross_check.name.outcome],
  );
  expect(new Set(outcomes)).toEqual(new Set(['same', 'differs', 'not-checked']));
});

describe('--no-speech leaves out Layer B', () => {
  test('in text', async () => {
    const { code, stdout } = await runCli([signInPage, '--no-speech']);
    expect(code).toBe(0);
    expect(stdout.split('\n').filter((line) => line.trimStart().startsWith('~'))).toEqual([]);
    expect(stdout).not.toContain('~');
    expect(stdout).not.toContain('illustrative speech');
    expect(stdout.split('\n')[1]).toBe('A = what the browser exposes');
  });

  test('in JSON', async () => {
    const { code, stdout } = await runCli([signInPage, '--format', 'json', '--no-speech']);
    expect(code).toBe(0);
    expect(schemaErrors(stdout)).toEqual([]);
    expect(keysIn(JSON.parse(stdout)).has('approx')).toBe(false);
  });
});

interface LocatedNode {
  layer_a: {
    kind: string;
    dom?: { tag?: string; id?: string };
    source?: { class: string; path?: string; line?: number; column?: number };
  };
}

/** The 1-based line and column, in Unicode scalar values, of `index` (a UTF-16 offset) in `text`. */
function position(text: string, index: number): { line: number; column: number } {
  const before = text.slice(0, index);
  const lineStart = before.lastIndexOf('\n') + 1;
  return {
    line: before.split('\n').length,
    column: [...before.slice(lineStart)].length + 1,
  };
}

test('a page that is not a fixture gets the right file, line and column for every tagged element', async () => {
  const page = [
    '<!doctype html>\n',
    '<html lang="en">\n',
    '<head><title>Locations</title></head>\n',
    '<body>\n',
    '<main id="main">\n',
    '<section id="outer" aria-label="Outer">\r\n',
    '<p id="first">Café 日本語 😀 <a id="inline" href="#here">here</a></p>\n',
    '<button id="multi" type="button"\n',
    '  title="spans\n',
    'lines">Go</button>\n',
    '</section>\n',
    '</main>\n',
    '</body>\n',
    '</html>\n',
  ].join('');
  const directory = temporaryDirectory();
  const path = join(directory, 'locations.html');
  writeFileSync(path, page);

  const json = await runCli([path, '--format', 'json']);
  expect(json.stderr).toBe('');
  expect(json.code).toBe(0);
  const transcript = JSON.parse(json.stdout) as { nodes: LocatedNode[] };
  const located = transcript.nodes.filter(
    (node) => node.layer_a.kind === 'accessible' && node.layer_a.source?.class === 'span',
  );
  const ids = located.map((node) => node.layer_a.dom?.id);
  expect(ids).toEqual(expect.arrayContaining(['main', 'outer', 'first', 'inline', 'multi']));

  const text = await runCli([path]);
  expect(text.code).toBe(0);
  for (const node of located) {
    const { dom, source } = node.layer_a;
    expect(dom?.id, JSON.stringify(dom)).toBeDefined();
    const index = page.indexOf(`<${dom?.tag} id="${dom?.id}"`);
    expect(index, JSON.stringify(dom)).toBeGreaterThanOrEqual(0);
    const expected = position(page, index);
    expect({ path: source?.path, line: source?.line, column: source?.column }, JSON.stringify(dom)).toEqual({
      path: basename(path),
      ...expected,
    });
    expect(text.stdout).toContain(
      `${basename(path)}:${expected.line}:${expected.column}  <${dom?.tag} id="${dom?.id}"`,
    );
  }
});

test('an empty page yields a transcript', async () => {
  const path = join(temporaryDirectory(), 'empty.html');
  writeFileSync(path, '');
  const { code, stdout, stderr } = await runCli([path]);
  expect(stderr).toBe('');
  expect(code).toBe(0);
  expect(stdout).toMatch(/^spokenpath [^ ]+ · snapshot v0\.0 · Chromium /);
  expect(stdout).toContain('\nTab order\n');
});

test('--verify-instrumentation prints the same-DOM line and no transcript', async () => {
  const { code, stdout } = await runCli([signInPage, '--verify-instrumentation']);
  expect(code).toBe(0);
  expect(stdout).toBe(`${signInPage}: the instrumented page builds the same DOM as the original\n`);
});

describe('usage errors exit 2', () => {
  test.each([
    ['no page', []],
    ['two pages', [signInPage, 'fixtures/pages/local-asset.html']],
    ['an unknown flag', [signInPage, '--colour']],
    ['an unknown format', [signInPage, '--format', 'xml']],
    ['a flag missing its value', [signInPage, '--wait-for']],
    ['--format with --verify-instrumentation', [signInPage, '--verify-instrumentation', '--format', 'json']],
    ['--no-speech with --verify-instrumentation', [signInPage, '--verify-instrumentation', '--no-speech']],
    ['a repeated --format', [signInPage, '--format', 'text', '--format', 'json']],
    ['a repeated --wait-for', [signInPage, '--wait-for', 'main', '--wait-for', 'h1']],
    ['a value for a flag that takes none', [signInPage, '--no-speech=yes']],
    ['an unknown engine', [signInPage, '--engine', 'firefox']],
    [
      '--cross-check safari with --engine safari',
      [signInPage, '--engine', 'safari', '--cross-check', 'safari'],
    ],
    ['--cross-check safari with --engine both', [signInPage, '--engine', 'both', '--cross-check', 'safari']],
    [
      '--engine safari with --verify-instrumentation',
      [signInPage, '--engine', 'safari', '--verify-instrumentation'],
    ],
    [
      '--engine both with --verify-instrumentation',
      [signInPage, '--engine', 'both', '--verify-instrumentation'],
    ],
    ['an unknown cross-check', [signInPage, '--cross-check', 'firefox']],
    ['a repeated --engine', [signInPage, '--engine', 'chromium', '--engine', 'chromium']],
    ['a repeated --cross-check', [signInPage, '--cross-check', 'safari', '--cross-check', 'safari']],
    [
      '--cross-check with --verify-instrumentation',
      [signInPage, '--verify-instrumentation', '--cross-check', 'safari'],
    ],
    ['--help with a value', ['--help=x']],
    ['--help with a page', ['--help', signInPage]],
    ['-h with a page', ['-h', signInPage]],
    ['--version with a page', ['--version', signInPage]],
  ])('%s', async (_name, args) => {
    const { code, stdout, stderr } = await runCli(args);
    expect(code).toBe(2);
    expect(stdout).toBe('');
    expect(stderr).toMatch(/^spokenpath: /);
    expect(stderr).toContain('usage:');
  });
});

describe('argument errors are worded by the command', () => {
  test.each([
    ['an unknown option', [signInPage, '--colour'], 'spokenpath: unknown option "--colour"'],
    ['an unknown short option', [signInPage, '-x'], 'spokenpath: unknown option "-x"'],
    ['a missing value', [signInPage, '--format'], 'spokenpath: "--format" needs a value'],
    [
      'an option as a value',
      [signInPage, '--wait-for', '--no-speech'],
      'spokenpath: "--wait-for" needs a value',
    ],
    [
      'a repeated option',
      [signInPage, '--format', 'json', '--format', 'json'],
      'spokenpath: "--format" is given more than once',
    ],
    [
      'a value for a flag that takes none',
      [signInPage, '--no-speech=yes'],
      'spokenpath: "--no-speech" takes no value',
    ],
    ['a value for --help', ['--help=x'], 'spokenpath: "--help" takes no value'],
    ['--help with a page', ['--help', signInPage], 'spokenpath: "--help" takes no other arguments'],
    ['--version with a page', ['--version', signInPage], 'spokenpath: "--version" takes no other arguments'],
    [
      'an unknown engine',
      [signInPage, '--engine', 'firefox'],
      'spokenpath: unknown engine "firefox"; the engines are chromium, safari and both',
    ],
    [
      '--cross-check safari with --engine safari',
      [signInPage, '--engine', 'safari', '--cross-check', 'safari'],
      'spokenpath: --cross-check safari can be used only with --engine chromium',
    ],
    [
      '--cross-check safari with --engine both',
      [signInPage, '--cross-check', 'safari', '--engine', 'both'],
      'spokenpath: --cross-check safari can be used only with --engine chromium',
    ],
    [
      '--engine safari with --verify-instrumentation',
      [signInPage, '--engine', 'safari', '--verify-instrumentation'],
      'spokenpath: --engine safari cannot be used with --verify-instrumentation, which checks the page in Chromium',
    ],
    [
      '--engine both with --verify-instrumentation',
      [signInPage, '--verify-instrumentation', '--engine', 'both'],
      'spokenpath: --engine both cannot be used with --verify-instrumentation, which checks the page in Chromium',
    ],
    [
      'an unknown cross-check',
      [signInPage, '--cross-check', 'chromium'],
      'spokenpath: unknown cross-check "chromium"; the only cross-check is safari',
    ],
    [
      '--cross-check with --verify-instrumentation',
      [signInPage, '--cross-check', 'safari', '--verify-instrumentation'],
      'spokenpath: --cross-check cannot be used with --verify-instrumentation, which prints no transcript',
    ],
  ])('%s', async (_name, args, message) => {
    const { code, stderr } = await runCli(args);
    expect(code).toBe(2);
    const [first, second, ...rest] = stderr.split('\n');
    expect(first).toBe(message);
    expect(second).toMatch(/^usage: spokenpath /);
    expect(rest).toEqual(['']);
    expect(stderr).not.toContain("'--'");
    expect(stderr).not.toContain('positional');
    expect((first?.match(/"/g) ?? []).length % 2).toBe(0);
  });
});

test.each([['--help'], ['-h']])('%s prints the usage and every option to stdout', async (flag) => {
  const { code, stdout, stderr } = await runCli([flag]);
  expect(stderr).toBe('');
  expect(code).toBe(0);
  expect(stdout).toMatch(/^usage: spokenpath <page\.html> /);
  for (const part of [
    '[--engine chromium|safari|both]',
    '[--cross-check safari]',
    '-h|--help',
    '--version',
  ]) {
    expect(stdout.split('\n')[0]).toContain(part);
  }
  for (const option of [
    '--engine chromium|safari|both',
    '--cross-check safari',
    '--format',
    '--no-speech',
    '--wait-for',
    '--verify-instrumentation',
    '--help',
    '--version',
  ]) {
    expect(stdout).toContain(`  ${option === '--help' ? '-h, --help' : option}`);
  }
});

test('--version prints the workspace version', async () => {
  const manifest = JSON.parse(readFileSync(join(repositoryRoot, 'package.json'), 'utf8')) as {
    version: string;
  };
  const { code, stdout, stderr } = await runCli(['--version']);
  expect(stderr).toBe('');
  expect(code).toBe(0);
  expect(stdout).toBe(`spokenpath ${manifest.version}\n`);
});

test('a multi-line capture failure keeps its line breaks and escapes every line', async () => {
  // Playwright rejects the selector with a message that ends in a call log quoting it.
  const { code, stdout, stderr } = await runCli([signInPage, '--wait-for', '!!\u{1b}[31m']);
  expect(code).toBe(1);
  expect(stdout).toBe('');
  const lines = stderr.trimEnd().split('\n');
  expect(lines.length).toBeGreaterThan(1);
  expect(lines[0]).toMatch(/^spokenpath: /);
  expect(stderr).not.toContain('\u{1b}');
  expect(lines.filter((line) => line.includes('!!\\u{1b}[31m')).length).toBeGreaterThan(1);
});

test('a missing page exits 2 naming page-unreadable', async () => {
  const { code, stdout, stderr } = await runCli(['fixtures/pages/no-such-page.html']);
  expect(code).toBe(2);
  expect(stdout).toBe('');
  expect(stderr).toMatch(/^spokenpath: page-unreadable: /);
});

test('a page the instrumenter refuses exits 2 naming its code', async () => {
  const { code, stdout, stderr } = await runCli([
    'fixtures/pages/torture/rejected/attribute-present.existing-source-attribute.html',
  ]);
  expect(code).toBe(2);
  expect(stdout).toBe('');
  expect(stderr).toMatch(/^spokenpath: attribute-present: /);
});

test('a missing binary exits 1 naming the command that builds it', async () => {
  const { code, stdout, stderr } = await runCli([signInPage], {
    ...process.env,
    CARGO_TARGET_DIR: temporaryDirectory(),
  });
  expect(code).toBe(1);
  expect(stdout).toBe('');
  expect(stderr).toMatch(/^spokenpath: the spokenpath-core binary is not built at /);
  expect(stderr).toContain('`cargo build -p spokenpath_cli`');
});

test('escapeForTerminal escapes exactly what the binary escapes', async () => {
  // Every code point from U+0000 to U+3000 and the ranges around the variation selectors and the
  // tag characters, so each escaped range and its neighbours are compared.
  const codePoints: number[] = [];
  for (const [first, last] of [
    [0x0, 0x3000],
    [0xfdf0, 0xfffd],
    [0x1f3f0, 0x1f600],
    [0xdfff0, 0xe01ff],
  ] as const) {
    for (let codePoint = first; codePoint <= last; codePoint += 1) {
      codePoints.push(codePoint);
    }
  }
  const name = String.fromCodePoint(...codePoints.filter((codePoint) => codePoint !== 0x22));
  const snapshot = JSON.parse(
    readFileSync(join(repositoryRoot, 'fixtures/snapshots/chromium/sign-in.json'), 'utf8'),
  ) as { payload: { ax: { role?: { value?: string }; name?: { value?: string } }[] } };
  const heading = snapshot.payload.ax.find((node) => node.role?.value === 'heading');
  if (heading?.name === undefined) {
    throw new Error('sign-in.json has a named heading');
  }
  heading.name.value = name;
  const path = join(temporaryDirectory(), 'every-character.json');
  writeFileSync(path, JSON.stringify(snapshot));
  const { code, stdout, stderr } = await runBinary(['transcript', path]);
  expect(code, stderr).toBe(0);
  expect(stdout).toContain(`A heading "${escapeForTerminal(name)}"`);
});

test('--engine chromium captures as it does without the option', async () => {
  const { code, stdout } = await runCli([signInPage, '--engine', 'chromium']);
  expect(code).toBe(0);
  expect(normaliseVersion(stdout)).toBe(readFileSync(goldenPath, 'utf8'));
});

describe.runIf(process.platform !== 'darwin')('off macOS, a Safari option exits 2 before any capture', () => {
  test('--cross-check safari', async () => {
    const { code, stdout, stderr } = await runCli([
      'fixtures/pages/no-such-page.html',
      '--cross-check',
      'safari',
    ]);
    expect(code).toBe(2);
    expect(stdout).toBe('');
    expect(stderr).toBe(
      'spokenpath: unsupported-platform: --cross-check safari needs macOS with Safari and safaridriver\n',
    );
  });

  test.each(['safari', 'both'])('--engine %s', async (engine) => {
    // A missing page and a missing binary would each fail later; the platform check comes first.
    const { code, stdout, stderr } = await runCli(['fixtures/pages/no-such-page.html', '--engine', engine], {
      ...process.env,
      CARGO_TARGET_DIR: temporaryDirectory(),
    });
    expect(code).toBe(2);
    expect(stdout).toBe('');
    expect(stderr).toBe(
      `spokenpath: unsupported-platform: --engine ${engine} needs macOS with Safari, safaridriver and the Accessibility permission\n`,
    );
  });

  test('the sign-in page with --engine safari', async () => {
    const { code, stdout, stderr } = await runCli([signInPage, '--engine', 'safari']);
    expect([code, stdout]).toEqual([2, '']);
    expect(stderr.split('\n')).toHaveLength(2);
  });
});

/** The body of an insta snapshot of the text printer, without its front matter. */
function textGolden(name: string): string {
  const text = readFileSync(
    join(repositoryRoot, 'crates/spokenpath_cli/src/snapshots', `spokenpath_core__tests__${name}.snap`),
    'utf8',
  );
  const end = text.indexOf('\n---\n', 4);
  expect(text.startsWith('---\n') && end > 0, `${name} has front matter`).toBe(true);
  return text.slice(end + '\n---\n'.length);
}

/** Where the second snapshot reaches the binary: a file path, or the extra pipe on fd 3. */
type SecondSnapshotRoute = 'path' | 'fd 3';

/**
 * Runs the binary with the Chromium snapshot on stdin and the Safari snapshot, after `flag`, named
 * by path or, as the command does, written to the extra pipe on fd 3. Its stdout and stderr are
 * captured in files.
 */
async function withSafari(
  route: SecondSnapshotRoute,
  chromiumJson: string,
  safari: string,
  page: string,
  flag: '--cross-check' | '--align-with' = '--cross-check',
): Promise<Run> {
  const directory = temporaryDirectory();
  const stdoutPath = join(directory, 'stdout.txt');
  const stderrPath = join(directory, 'stderr.txt');
  const stdout = openSync(stdoutPath, 'w');
  const stderr = openSync(stderrPath, 'w');
  const args = ['transcript', '-', '--source', page, '--format', 'text', flag];
  try {
    const code =
      route === 'path'
        ? await transcribe(binaryPath(), [...args, safari], chromiumJson, undefined, { stdout, stderr })
        : await transcribe(
            binaryPath(),
            [...args, SECOND_SNAPSHOT_FD_PATH],
            chromiumJson,
            readFileSync(safari, 'utf8'),
            {
              stdout,
              stderr,
            },
          );
    return { code, stdout: readFileSync(stdoutPath, 'utf8'), stderr: readFileSync(stderrPath, 'utf8') };
  } finally {
    closeSync(stdout);
    closeSync(stderr);
  }
}

const snapshotsDirectory = join(repositoryRoot, 'fixtures/snapshots');

/** A Chromium snapshot, its Safari snapshot, the page, and the cross-check text golden they print. */
const crossCheckCases = [
  ...(['cross-check/list-style-none', 'cross-check/select-optgroup'] as const).map((page) => ({
    name: page,
    chromium: join(snapshotsDirectory, 'chromium', `${page}.json`),
    safari: join(snapshotsDirectory, 'safari-webdriver', `${page}.json`),
    page: join(repositoryRoot, 'fixtures/pages', `${page}.html`),
    golden: `safari-cross-check__${page.replace('/', '__')}`,
  })),
  {
    name: 'the hand-written pair',
    chromium: join(snapshotsDirectory, 'chromium-basic.json'),
    safari: join(snapshotsDirectory, 'safari-webdriver-basic.json'),
    page: join(snapshotsDirectory, 'pages/basic.html'),
    golden: 'safari-cross-check__basic',
  },
];

/** A Chromium snapshot, Safari's accessibility-tree snapshot, the page, and their aligned golden. */
const alignedCases = [
  {
    name: 'cross-check/list-style-none',
    chromium: join(snapshotsDirectory, 'chromium/cross-check/list-style-none.json'),
    safari: join(snapshotsDirectory, 'safari-ax/cross-check/list-style-none.json'),
    page: join(repositoryRoot, 'fixtures/pages/cross-check/list-style-none.html'),
    golden: 'both__cross-check__list-style-none',
  },
  {
    name: 'the hand-written pair',
    chromium: join(snapshotsDirectory, 'chromium-basic.json'),
    safari: join(snapshotsDirectory, 'safari-ax-basic.json'),
    page: join(snapshotsDirectory, 'pages/basic.html'),
    golden: 'both__basic',
  },
];

// The command passes the Safari snapshot on fd 3 only on macOS, where it runs; Linux cannot reopen
// that socket by path, so every system checks the same output with the snapshot named by path.
describe.each([
  ['named by path', 'path', true],
  ['fed on fd 3', 'fd 3', process.platform === 'darwin'],
] as const)('the binary with a Safari snapshot %s', (_, route, runs) => {
  test.runIf(runs).each(crossCheckCases)('prints the committed cross-check golden of $name', async (pair) => {
    const run = await withSafari(route, readFileSync(pair.chromium, 'utf8'), pair.safari, pair.page);
    expect(run.code, run.stderr).toBe(0);
    expect(normaliseVersion(run.stdout).trimEnd()).toBe(textGolden(pair.golden).trimEnd());
  });

  test.runIf(runs).each(alignedCases)('prints the committed aligned golden of $name', async (pair) => {
    const run = await withSafari(
      route,
      readFileSync(pair.chromium, 'utf8'),
      pair.safari,
      pair.page,
      '--align-with',
    );
    expect(run.code, run.stderr).toBe(0);
    expect(normaliseVersion(run.stdout).trimEnd()).toBe(textGolden(pair.golden).trimEnd());
  });

  test.runIf(runs)('reports an aligned snapshot of another page with exit 2 and no transcript', async () => {
    const run = await withSafari(
      route,
      readFileSync(join(snapshotsDirectory, 'chromium/sign-in.json'), 'utf8'),
      join(snapshotsDirectory, 'safari-ax-basic.json'),
      join(repositoryRoot, 'fixtures/pages/sign-in.html'),
      '--align-with',
    );
    expect(run.code).toBe(2);
    expect(run.stdout).toBe('');
    expect(run.stderr).toMatch(/^spokenpath-core: the aligned snapshot was captured from a different page/);
  });

  test.runIf(runs)('reports a Safari snapshot of another page with exit 2 and no transcript', async () => {
    const run = await withSafari(
      route,
      readFileSync(join(snapshotsDirectory, 'chromium/sign-in.json'), 'utf8'),
      join(snapshotsDirectory, 'safari-webdriver-basic.json'),
      join(repositoryRoot, 'fixtures/pages/sign-in.html'),
    );
    expect(run.code).toBe(2);
    expect(run.stdout).toBe('');
    expect(run.stderr).toMatch(
      /^spokenpath-core: the cross-check snapshot was captured from a different page/,
    );
  });
});

test('passes through the exit code and message of a binary that rejects its stdin before reading fd 3', async () => {
  // Far more than a pipe buffers, so the write is still in flight when the binary exits unread.
  const unread = ' '.repeat(16 * 1024 * 1024);
  const directory = temporaryDirectory();
  const stderrPath = join(directory, 'stderr.txt');
  const stderr = openSync(stderrPath, 'w');
  try {
    const code = await transcribe(
      binaryPath(),
      ['transcript', '-', '--format', 'text', '--cross-check', SECOND_SNAPSHOT_FD_PATH],
      'not a snapshot',
      unread,
      { stdout: stderr, stderr },
    );
    expect(code).toBe(2);
  } finally {
    closeSync(stderr);
  }
  expect(readFileSync(stderrPath, 'utf8')).toBe(
    'spokenpath-core: snapshot is not valid JSON: expected ident at line 1 column 2\n',
  );
});

/** ESC with an SGR sequence, the C1 CSI introducer, a bidi override and a tag character. */
const HOSTILE = '\u{1b}[31m\u{9b}2J\u{202e}\u{e0041}';

/** The stderr line the command prints for a Safari capture with the test-double driver. */
async function printedFailure(scenario: Record<string, unknown>): Promise<string> {
  const directory = temporaryDirectory();
  const scenarioPath = join(directory, 'scenario.json');
  writeFileSync(scenarioPath, JSON.stringify({ ...scenario, log: join(directory, 'events.jsonl') }));
  const error = await captureSafari(join(repositoryRoot, signInPage), {
    driver: { command: process.execPath, args: [testDriverPath, scenarioPath], versions: {} },
  }).then(
    () => undefined,
    (failure: unknown) => failure,
  );
  expect(error).toBeInstanceOf(SafariCaptureError);
  return failureText(describeFailure(error));
}

test('a helper failure whose message holds terminal controls prints each only escaped', () => {
  const error = new SafariCaptureError(
    'ax-helper-failed',
    'the helper said \u{1b}[31m, \u{9b}2J, \u{202e} and \u{e0041}',
  );
  const line = failureText(describeFailure(error));
  expect(line).toMatch(/^spokenpath: ax-helper-failed: /);
  for (const raw of ['\u{1b}', '\u{9b}', '\u{202e}', '\u{e0041}']) {
    expect(line.includes(raw), `${JSON.stringify(line)} holds ${JSON.stringify(raw)}`).toBe(false);
  }
  for (const escaped of ['\\u{1b}[31m', '\\u{9b}2J', '\\u{202e}', '\\u{e0041}']) {
    expect(line, escaped).toContain(escaped);
  }
});

describe('driver-originated text is printed escaped', () => {
  test.each([
    ['the session error code', { refuseSession: { error: `refused ${HOSTILE}`, message: '' } }],
    [
      'the session error message',
      { refuseSession: { error: 'session not created', message: `no ${HOSTILE}` } },
    ],
    ["safaridriver's stderr", { stderr: `failed ${HOSTILE}`, exitAtStart: true }],
  ])('%s', async (_field, scenario) => {
    const line = await printedFailure(scenario);
    expect(line).toMatch(/^spokenpath: safari-session-refused: /);
    for (const raw of ['\u{1b}', '\u{9b}', '\u{202e}', '\u{e0041}']) {
      expect(line.includes(raw), `${JSON.stringify(line)} holds ${JSON.stringify(raw)}`).toBe(false);
    }
    for (const escaped of ['[31m', '\\u{9b}', '\\u{202e}', '\\u{e0041}']) {
      expect(line, escaped).toContain(escaped);
    }
  });
});
