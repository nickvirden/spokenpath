import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';
import { chromiumSample } from './chromium-sample.js';
import { generateCommand, generatedTypesUrl, generateTypes, readSchema } from './generate.js';
import { packageName, type Snapshot, validateSnapshot } from './index.js';

const fixturesUrl = new URL('../../../fixtures/snapshots/', import.meta.url);
const sampleFixtureUrl = new URL('chromium-from-typescript.json', fixturesUrl);
const sourcePathCasesUrl = new URL('../source-paths.json', fixturesUrl);

const fixtureNames = readdirSync(fixturesUrl)
  .filter((name) => name.endsWith('.json'))
  .sort();

function readFixture(name: string): Json {
  return JSON.parse(readFileSync(new URL(name, fixturesUrl), 'utf8')) as Json;
}

type Json = Record<string, unknown>;

/** Removes the field instead of setting it. */
const removed = Symbol('removed');

/**
 * A fresh copy of the basic fixture with each edit applied. An edit's key is a JSON Pointer such as
 * `/capture/engine`; its value replaces the field, or `removed` deletes it.
 */
function editedBasic(edits: Record<string, unknown>): Json {
  const snapshot = readFixture('chromium-basic.json');
  for (const [pointer, value] of Object.entries(edits)) {
    const keys = pointer.split('/').slice(1);
    const last = keys.pop();
    if (last === undefined) {
      throw new Error(`edit pointer \`${pointer}\` names no field`);
    }
    const parent = keys.reduce((target, key) => target[key] as Json, snapshot);
    if (value === removed) {
      delete parent[last];
    } else {
      parent[last] = value;
    }
  }
  return snapshot;
}

function errorPaths(value: unknown): string[] {
  const result = validateSnapshot(value);
  expect(result.ok, 'the value is rejected').toBe(false);
  return result.ok ? [] : result.errors.map((error) => error.path);
}

test('exports the package name', () => {
  expect(packageName).toBe('@spokenpath/snapshot-types');
});

describe('validateSnapshot', () => {
  test('finds the committed fixtures', () => {
    expect(fixtureNames).toContain('chromium-basic.json');
    expect(fixtureNames).toContain('chromium-from-typescript.json');
  });

  test.each(fixtureNames)('accepts %s', (name) => {
    const result = validateSnapshot(readFixture(name));
    expect(result.ok ? [] : result.errors).toEqual([]);
  });

  test('rejects an unknown engine at capture.engine', () => {
    const paths = errorPaths(editedBasic({ '/capture/engine': 'firefox' }));
    expect(paths).toContain('/capture/engine');
  });

  test('rejects a missing producer at the producer field', () => {
    const paths = errorPaths(editedBasic({ '/producer': removed }));
    expect(paths).toContain('/producer');
  });

  test('rejects major version 1 at version.major', () => {
    const paths = errorPaths(editedBasic({ '/version/major': 1 }));
    expect(paths).toContain('/version/major');
  });

  test('rejects a sha256 that is not 64 hex characters', () => {
    const paths = errorPaths(editedBasic({ '/sources/0/sha256': 'abc123' }));
    expect(paths).toContain('/sources/0/sha256');
  });

  test('rejects a wrong schema identifier', () => {
    const paths = errorPaths(editedBasic({ '/schema': 'something-else' }));
    expect(paths).toContain('/schema');
  });

  test('rejects a wrong type deep in the Chromium payload', () => {
    const paths = errorPaths(editedBasic({ '/payload/dom/2/kind': 7 }));
    expect(paths).toContain('/payload/dom/2/kind');
  });

  test('accepts an unknown field and minor version 99', () => {
    const snapshot = editedBasic({
      '/added_in_a_later_minor': { anything: [1, 2] },
      '/payload/dom/2/layout': 'block',
      '/version/minor': 99,
    });
    const result = validateSnapshot(snapshot);
    expect(result.ok ? [] : result.errors).toEqual([]);
  });

  test('accepts the Safari envelopes with no payload contents', () => {
    for (const engine of ['safari-webdriver', 'safari-ax']) {
      const snapshot = editedBasic({ '/capture/engine': engine, '/payload': { engine } });
      const result = validateSnapshot(snapshot);
      expect(result.ok ? [] : result.errors).toEqual([]);
    }
  });
});

describe('validateSnapshot source paths', () => {
  const cases = JSON.parse(readFileSync(sourcePathCasesUrl, 'utf8')) as {
    accepted: string[];
    rejected: string[];
  };

  test('shares the documented cases with the Rust reader', () => {
    expect(cases.accepted).toContain('pages/basic.html');
    for (const path of ['../x', '/x', 'a/../b', 'a\\b', 'C:/x', '', 'a/\u0000b']) {
      expect(cases.rejected).toContain(path);
    }
  });

  test.each(cases.accepted)('accepts %j', (path) => {
    const result = validateSnapshot(editedBasic({ '/sources/0/path': path }));
    expect(result.ok ? [] : result.errors).toEqual([]);
  });

  test.each(cases.rejected)('rejects %j at /sources/0/path', (path) => {
    expect(errorPaths(editedBasic({ '/sources/0/path': path }))).toContain('/sources/0/path');
  });
});

describe('validateSnapshot engine rule', () => {
  test.each(['chromium', 'safari-webdriver', 'safari-ax'])('accepts matching %s engines', (engine) => {
    const edits: Record<string, unknown> = { '/capture/engine': engine };
    if (engine !== 'chromium') {
      edits['/payload'] = { engine };
    }
    const result = validateSnapshot(editedBasic(edits));
    expect(result.ok ? [] : result.errors).toEqual([]);
  });

  test('rejects a capture.engine that differs from payload.engine', () => {
    const result = validateSnapshot(editedBasic({ '/capture/engine': 'safari-ax' }));
    expect(result.ok).toBe(false);
    const errors = result.ok ? [] : result.errors;
    expect(errors.map((error) => error.path)).toEqual(['/payload/engine']);
    expect(errors[0]?.message).toContain('capture.engine');
    expect(errors[0]?.message).toContain('payload.engine');
  });

  test('reports a payload failure first at the failing field, not at the other variants', () => {
    const paths = errorPaths(editedBasic({ '/payload/dom/2/kind': 'paragraph' }));
    expect(paths[0]).toBe('/payload/dom/2/kind');
    expect(paths).not.toContain('/payload');
    expect(paths).not.toContain('/payload/engine');
  });
});

describe('validateSnapshot accessibility nodes', () => {
  test('accepts an AX field the types do not name', () => {
    const result = validateSnapshot(editedBasic({ '/payload/ax/0/futureField': { nested: [true, null] } }));
    expect(result.ok ? [] : result.errors).toEqual([]);
  });

  test('accepts nodes without name provenance or exclusion reasons', () => {
    const result = validateSnapshot(
      editedBasic({ '/payload/ax/3/name/sources': removed, '/payload/ax/1/ignoredReasons': removed }),
    );
    expect(result.ok ? [] : result.errors).toEqual([]);
  });

  test('rejects a Chromium payload without ax_forced', () => {
    expect(errorPaths(editedBasic({ '/payload/ax_forced': removed }))).toContain('/payload/ax_forced');
  });
});

test('safari-webdriver-basic.json is a Safari snapshot of the same page as chromium-basic.json', () => {
  const chromium = readFixture('chromium-basic.json');
  const safari = readFixture('safari-webdriver-basic.json');
  expect(safari.sources).toEqual(chromium.sources);
  expect(safari.spans).toEqual(chromium.spans);
  expect(safari.version).toEqual({ major: 0, minor: 1 });
});

describe('chromium-basic.json against its page source', () => {
  const snapshot = readFixture('chromium-basic.json') as unknown as Snapshot;
  const source = snapshot.sources[0];
  if (source === undefined) {
    throw new Error('chromium-basic.json lists no source');
  }
  const bytes = readFileSync(new URL(source.path, fixturesUrl));
  const tags = new Map<number, string>();
  if (snapshot.payload.engine === 'chromium') {
    for (const node of snapshot.payload.dom) {
      if (node.span !== undefined && node.tag !== undefined) {
        tags.set(node.span, node.tag);
      }
    }
  }

  test('the source hash matches the committed page', () => {
    expect(createHash('sha256').update(bytes).digest('hex')).toBe(source.sha256);
  });

  test.each(snapshot.spans.map((span) => [span.id, span] as const))(
    'span %i locates its tag',
    (_id, span) => {
      const text = (start: number, end: number) => bytes.subarray(start, end).toString('utf8');
      const tag = tags.get(span.id);
      const startTag = text(span.start_tag.start, span.start_tag.end);
      expect(startTag.startsWith(`<${tag}`), startTag).toBe(true);
      expect(startTag.endsWith('>'), startTag).toBe(true);
      if (span.element !== undefined) {
        expect(span.element.start).toBe(span.start_tag.start);
        expect(text(span.element.start, span.element.end).endsWith(`</${tag}>`)).toBe(true);
      }
      const before = text(0, span.start_tag.start);
      const lineStart = before.lastIndexOf('\n') + 1;
      expect(span.line).toBe(before.split('\n').length);
      expect(span.column).toBe(Array.from(before.slice(lineStart)).length + 1);
    },
  );
});

test('the committed generated types match the committed schema', async () => {
  const expected = await generateTypes(await readSchema());
  const committed = readFileSync(generatedTypesUrl, 'utf8');
  expect(
    committed === expected,
    `src/snapshot.generated.ts is out of date with schema/snapshot.v0.json; run \`${generateCommand}\``,
  ).toBe(true);
});

test('the typed sample matches fixtures/snapshots/chromium-from-typescript.json', () => {
  const json = `${JSON.stringify(chromiumSample, null, 2)}\n`;
  if (process.env.UPDATE_FIXTURES === '1') {
    writeFileSync(sampleFixtureUrl, json);
  }
  const committed = readFileSync(sampleFixtureUrl, 'utf8');
  expect(
    committed === json,
    'fixtures/snapshots/chromium-from-typescript.json differs from src/chromium-sample.ts; refresh it with `UPDATE_FIXTURES=1 pnpm --filter @spokenpath/snapshot-types test`',
  ).toBe(true);
});
