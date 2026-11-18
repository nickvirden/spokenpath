import { mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterAll, beforeAll, describe, expect, test } from 'vitest';
import { contentType, pageUrl, redactUrl, resolveAsset } from './serve.ts';

describe('resolveAsset', () => {
  let base: string;
  let root: string;

  beforeAll(() => {
    base = realpathSync(mkdtempSync(join(tmpdir(), 'capture-serve-')));
    root = join(base, 'site');
    mkdirSync(join(root, 'assets'), { recursive: true });
    writeFileSync(join(root, 'assets', 'style.css'), 'p {}');
    writeFileSync(join(root, 'page.html'), '<p>');
    writeFileSync(join(base, 'secret.txt'), 'outside');
    symlinkSync(join(base, 'secret.txt'), join(root, 'escape.txt'));
    symlinkSync(join(root, 'page.html'), join(root, 'alias.html'));
  });

  afterAll(() => {
    rmSync(base, { recursive: true, force: true });
  });

  test.each([
    ['/page.html', 'page.html'],
    ['/assets/style.css', 'assets/style.css'],
    ['/assets/%73tyle.css', 'assets/style.css'],
    ['/alias.html', 'page.html'],
  ])('%s is answered from the directory', async (pathname, file) => {
    expect(await resolveAsset(root, pathname)).toBe(join(root, file));
  });

  test.each([
    ['a missing file', '/missing.css'],
    ['the directory itself', '/'],
    ['a subdirectory', '/assets'],
    ['an encoded traversal', '/..%2fsecret.txt'],
    ['an encoded traversal through a subdirectory', '/assets/..%2f..%2fsecret.txt'],
    ['a symbolic link out of the directory', '/escape.txt'],
    ['an encoded NUL', '/page.html%00.css'],
    ['a malformed escape', '/%E0%A4%A.css'],
  ])('%s is not', async (_name, pathname) => {
    expect(await resolveAsset(root, pathname)).toBeUndefined();
  });
});

describe('redactUrl', () => {
  test.each([
    ['https://user:pass@example.com/a/b.png?token=x#frag', 'https://example.com/a/b.png'],
    ['wss://example.com/socket?session=1', 'wss://example.com/socket'],
    ['http://spokenpath.test/assets/x.css', 'http://spokenpath.test/assets/x.css'],
  ])('%s becomes %s', (url, expected) => {
    expect(redactUrl(url)).toBe(expected);
  });

  test('an unparseable URL keeps none of its text', () => {
    expect(redactUrl('not a url ?secret')).not.toContain('secret');
  });
});

test('text content types are declared UTF-8, others are not', () => {
  expect(contentType('a/b.CSS')).toBe('text/css; charset=utf-8');
  expect(contentType('x.js')).toBe('text/javascript; charset=utf-8');
  expect(contentType('x.png')).toBe('image/png');
  expect(contentType('x.unknown')).toBe('application/octet-stream');
});

test('the page URL percent-encodes its file name', () => {
  expect(pageUrl('a b#c.html')).toBe('http://spokenpath.test/a%20b%23c.html');
});
