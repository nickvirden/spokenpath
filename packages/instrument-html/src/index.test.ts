import { parse } from 'parse5';
import { describe, expect, test } from 'vitest';
import { charsetFromContent } from './encoding.ts';
import { InstrumentHtmlError, instrumentHtml, packageName, SOURCE_ATTRIBUTE } from './index.ts';
import { elements } from './tree.ts';

const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { ignoreBOM: true });

function instrumentText(html: string) {
  const result = instrumentHtml(encoder.encode(html), { source: 0 });
  return { ...result, text: decoder.decode(result.html) };
}

/** The error `instrumentHtml` throws for `input`. */
function refusal(input: Uint8Array | string): InstrumentHtmlError {
  try {
    instrumentHtml(typeof input === 'string' ? encoder.encode(input) : input, { source: 0 });
  } catch (error) {
    if (error instanceof InstrumentHtmlError) {
      return error;
    }
    throw error;
  }
  throw new Error('the input was accepted');
}

test('exports the package name', () => {
  expect(packageName).toBe('@spokenpath/instrument-html');
});

test('names the source attribute', () => {
  expect(SOURCE_ATTRIBUTE).toBe('data-spokenpath-src');
});

describe('accepted inputs', () => {
  test.each([
    ['empty input', new Uint8Array()],
    ['a byte order mark alone', new Uint8Array([0xef, 0xbb, 0xbf])],
  ])('%s gives no spans and unchanged bytes', (_name, input) => {
    const { html, spans } = instrumentHtml(input, { source: 0 });
    expect(spans).toEqual([]);
    expect(Buffer.compare(html, input)).toBe(0);
  });

  test('an ASCII page with no encoding declaration is tagged', () => {
    const { text, spans } = instrumentText('<!DOCTYPE html><title>t</title><p>x</p>');
    expect(text).toBe(
      '<!DOCTYPE html><title data-spokenpath-src="0">t</title><p data-spokenpath-src="1">x</p>',
    );
    expect(spans).toHaveLength(2);
  });

  test.each(['utf-8', 'UTF8', ' Unicode-1-1-UTF-8 ', 'x-unicode20utf8'])(
    'a meta charset of "%s" is UTF-8',
    (label) => {
      expect(instrumentText(`<meta charset="${label}"><p>x`).spans).toHaveLength(2);
    },
  );

  test.each([
    '<meta charset="">',
    '<meta charset>',
    '<meta charset=" ">',
    '<meta charset="no-such-encoding">',
  ])('%s names no encoding, so it is no declaration', (meta) => {
    expect(instrumentText(`${meta}<p>x`).spans).toHaveLength(2);
  });

  test('an http-equiv content type naming no encoding is no declaration', () => {
    const meta = '<meta http-equiv="content-type" content="text/html; charset=no-such-encoding">';
    expect(instrumentText(meta).spans).toHaveLength(1);
  });

  test('a non-UTF-8 meta inside template contents is not a declaration', () => {
    expect(instrumentText('<template><meta charset="windows-1252"></template>').spans).toHaveLength(2);
  });

  test('a leading UTF-8 byte order mark is kept and counted in byte offsets but not columns', () => {
    const input = new Uint8Array([0xef, 0xbb, 0xbf, ...encoder.encode('<p>x</p>')]);
    const { html, spans } = instrumentHtml(input, { source: 0 });
    expect([...html.subarray(0, 3)]).toEqual([0xef, 0xbb, 0xbf]);
    expect(spans).toEqual([
      {
        id: 0,
        source: 0,
        start_tag: { start: 3, end: 6 },
        element: { start: 3, end: 11 },
        line: 1,
        column: 1,
      },
    ]);
  });
});

describe('splice points', () => {
  test.each([
    ['<br/>', '<br data-spokenpath-src="0"/>'],
    ['<p />', '<p  data-spokenpath-src="0"/>'],
    ['<a href="x"/>', '<a href="x" data-spokenpath-src="0"/>'],
    ['<a href=x/>', '<a href=x/ data-spokenpath-src="0">'],
    ['<div a=1 a=x/>', '<div a=1 a=x/ data-spokenpath-src="0">'],
    ['<img src=a/b/>', '<img src=a/b/ data-spokenpath-src="0">'],
    ['<span b=/>', '<span b=/ data-spokenpath-src="0">'],
    ['<span //>', '<span / data-spokenpath-src="0"/>'],
    ['<p title="a/>b">', '<p title="a/>b" data-spokenpath-src="0">'],
    ["<p title='/'/>", `<p title='/' data-spokenpath-src="0"/>`],
    // A tag closing right after `=` takes the attribute after its name: spliced after the `=`,
    // it would become the empty attribute's value.
    ['<a href=>', '<a data-spokenpath-src="0" href=>'],
    ['<a href= >', '<a data-spokenpath-src="0" href= >'],
    ['<input value=>', '<input data-spokenpath-src="0" value=>'],
    ['<a b =>', '<a data-spokenpath-src="0" b =>'],
    ['<a\nb=>', '<a data-spokenpath-src="0"\nb=>'],
    ['<a/b=>', '<a data-spokenpath-src="0"/b=>'],
    [
      '<svg><path d="M0"/></svg>',
      '<svg data-spokenpath-src="0"><path d="M0" data-spokenpath-src="1"/></svg>',
    ],
  ])('%s', (tag, expected) => {
    expect(instrumentText(tag).text).toBe(expected);
  });
});

describe('spans', () => {
  test('give UTF-8 byte ranges and columns in scalar values', () => {
    // `→` is three bytes and `😀` four bytes, two UTF-16 units; each is one column.
    const { spans } = instrumentText('<p>→😀<b>x</b></p>');
    expect(spans[1]).toEqual({
      id: 1,
      source: 0,
      start_tag: { start: 10, end: 13 },
      element: { start: 10, end: 18 },
      line: 1,
      column: 6,
    });
  });

  test('count LF, CRLF and a lone CR as one line break each', () => {
    const { spans } = instrumentText('<p>\n<i>a</i>\r\n<i>b</i>\r<i>c</i>\r\n\r\n  <i>d</i></p>');
    expect(spans.map(({ line, column }) => [line, column])).toEqual([
      [1, 1],
      [2, 1],
      [3, 1],
      [4, 1],
      [6, 3],
    ]);
  });

  test('omit element when the parser reports no end tag', () => {
    const { spans } = instrumentText('<ul><li>one<li>two</ul>');
    expect(spans.map((span) => 'element' in span)).toEqual([true, false, false]);
  });

  test('carry the source id given in the options', () => {
    const { spans } = instrumentHtml(encoder.encode('<p>x'), { source: 7 });
    expect(spans.map((span) => span.source)).toEqual([7]);
  });
});

test('an adoption-agency clone carries its original span id and implied elements carry none', () => {
  const { text } = instrumentText('<b><p></b>t</p>');
  const document = parse(text);
  const source = (tag: string) =>
    [...elements(document, { templates: true })]
      .filter((element) => element.tagName === tag)
      .map((element) => element.attrs.find((attr) => attr.name === SOURCE_ATTRIBUTE)?.value);
  expect(source('html')).toEqual([undefined]);
  expect(source('head')).toEqual([undefined]);
  expect(source('body')).toEqual([undefined]);
  expect(source('p')).toEqual(['1']);
  expect(source('b')).toEqual(['0', '0']);
});

test("an element reopened from its original start tag shares that tag's span", () => {
  const { text, spans } = instrumentText('<p><i>a<b>b</i>c</b></p>');
  expect(spans).toHaveLength(3);
  const ids = [...elements(parse(text), { templates: true })]
    .filter((element) => element.tagName === 'b')
    .map((element) => element.attrs.find((attr) => attr.name === SOURCE_ATTRIBUTE)?.value);
  expect(ids).toEqual(['2', '2']);
});

describe('refused inputs', () => {
  test.each([
    ['UTF-16LE', [0xff, 0xfe, 0x3c, 0x00]],
    ['UTF-16BE', [0xfe, 0xff, 0x00, 0x3c]],
  ])('a %s byte order mark', (encoding, bytes) => {
    const error = refusal(new Uint8Array(bytes));
    expect(error.code).toBe('unsupported-encoding');
    expect(error.message).toContain(encoding);
  });

  test.each([
    ['a Latin-1 byte', [0x3c, 0x70, 0x3e, 0xe9, 0x3c], 3],
    ['a truncated sequence', [0x3c, 0x70, 0x3e, 0xe2, 0x86], 3],
    ['an overlong encoding', [0x20, 0xc0, 0xbc], 1],
    ['an encoded surrogate', [0x20, 0x20, 0xed, 0xa0, 0x80], 2],
    ['a code point above U+10FFFF', [0xf4, 0x90, 0x80, 0x80], 0],
  ])('invalid UTF-8: %s', (_name, bytes, offset) => {
    const error = refusal(new Uint8Array(bytes));
    expect(error.code).toBe('invalid-utf-8');
    expect(error.message).toContain(`byte offset ${offset}`);
  });

  test('a meta charset naming another encoding, quoted with its position', () => {
    const error = refusal('<!DOCTYPE html>\n<head>\n  → <meta charset=" Shift_JIS ">');
    expect(error.code).toBe('unsupported-encoding');
    expect(error.message).toContain('" Shift_JIS " at line 3, column 5');
  });

  test('a meta http-equiv content type naming another encoding', () => {
    const error = refusal('<meta http-equiv="content-type" content="text/html;charset=\'koi8-r\'">');
    expect(error.code).toBe('unsupported-encoding');
    expect(error.message).toContain('"koi8-r" at line 1, column 1');
  });

  test('a meta declaring UTF-16, which is not a UTF-8 label', () => {
    expect(refusal('<meta charset="utf-16">').code).toBe('unsupported-encoding');
  });

  test.each(['windows-1252', 'shift_jis', 'koi8-r', 'gb18030', 'iso-2022-kr', 'x-user-defined'])(
    'a meta charset of %s, a known non-UTF-8 label',
    (label) => {
      expect(refusal(`<meta charset="${label}">`).code).toBe('unsupported-encoding');
    },
  );

  test('an unknown charset falls through to the http-equiv declaration', () => {
    const error = refusal(
      '<meta charset="bogus" http-equiv="content-type" content="text/html; charset=koi8-r">',
    );
    expect(error.message).toContain('"koi8-r"');
  });

  test('a declared label is escaped in the message', () => {
    // Surrounding whitespace is trimmed when the label is looked up, so the page's raw text,
    // control characters included, reaches the message.
    const error = refusal('<meta charset="\n\twindows-1252\n">');
    expect(error.code).toBe('unsupported-encoding');
    expect(error.message).toContain('"\\n\\twindows-1252\\n"');
    for (const control of ['\n', '\t']) {
      expect(error.message).not.toContain(control);
    }
  });

  test('an element that already has the source attribute, inside template contents too', () => {
    const error = refusal('<p>a</p>\n<template><i data-spokenpath-src="1"></i></template>');
    expect(error.code).toBe('attribute-present');
    expect(error.message).toContain('at line 2, column 11');
  });

  test('a page whose tree the splice would change', () => {
    // The parser's list of active formatting elements keeps at most three elements with equal
    // attributes; distinct span ids make four `b` elements unequal, so a fourth is reopened.
    const error = refusal('<p><b><b><b><b>x</p><p>y');
    expect(error.code).toBe('dom-changed');
    expect(error.message).toContain('html[0] > body[0] > p[1]');
  });
});

describe('the charset in a meta content attribute', () => {
  test.each([
    ['text/html; charset=utf-8', 'utf-8'],
    ['text/html;CHARSET = "ISO-8859-1" ', 'ISO-8859-1'],
    ["charset='a b'", 'a b'],
    ['text/html; charset=koi8-r; x=y', 'koi8-r'],
    ['charsetcharset=x', 'x'],
    ['charset', undefined],
    ['charset=', undefined],
    ['charset="unterminated', undefined],
    ['text/html', undefined],
  ])('%j gives %j', (content, expected) => {
    expect(charsetFromContent(content)).toBe(expected);
  });
});
