import { execFile } from 'node:child_process';
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Ajv2020 } from 'ajv/dist/2020.js';
import { afterAll, describe, expect, test } from 'vitest';
import { binaryPath, repositoryRoot } from './binary.ts';
import { escapeForTerminal } from './escape.ts';

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
  ])('%s', async (_name, args) => {
    const { code, stdout, stderr } = await runCli(args);
    expect(code).toBe(2);
    expect(stdout).toBe('');
    expect(stderr).toMatch(/^spokenpath: /);
    expect(stderr).toContain('usage:');
  });
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
