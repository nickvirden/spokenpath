import { readdirSync, readFileSync } from 'node:fs';
import { parse } from 'parse5';
import { describe, expect, test } from 'vitest';
import {
  type InstrumentHtmlErrorCode,
  InstrumentHtmlError,
  instrumentHtml,
  SOURCE_ATTRIBUTE,
  sourceClassToken,
} from './index.ts';
import { compareTrees } from './tree-equality.ts';
import { elements } from './tree.ts';

const tortureUrl = new URL('../../../fixtures/pages/torture/', import.meta.url);
const rejectedUrl = new URL('rejected/', tortureUrl);

const accepted = readdirSync(tortureUrl)
  .filter((name) => name.endsWith('.html'))
  .sort();
const rejected = readdirSync(rejectedUrl)
  .filter((name) => name.endsWith('.html'))
  .sort();

const utf8 = new TextDecoder();

/** The page as a browser's UTF-8 decoder yields it: a leading byte order mark is dropped. */
function decode(bytes: Uint8Array): string {
  return utf8.decode(bytes);
}

function bomLength(bytes: Uint8Array): number {
  return bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf ? 3 : 0;
}

/** The instrumented bytes with every spliced attribute removed. */
function withoutSourceAttributes(html: Uint8Array): Buffer {
  const latin1 = Buffer.from(html).toString('latin1');
  return Buffer.from(latin1.replace(new RegExp(` ${SOURCE_ATTRIBUTE}="\\d+"`, 'g'), ''), 'latin1');
}

test('the torture directories hold fixtures', () => {
  expect(accepted.length).toBeGreaterThan(0);
  expect(rejected.length).toBeGreaterThan(0);
});

describe.each(accepted)('%s', (name) => {
  const input = readFileSync(new URL(name, tortureUrl));
  const originalText = decode(input);
  // Instrumented inside each test, so a page that fails fails its own tests, not the whole file.
  const instrument = () => {
    const { html, spans } = instrumentHtml(input, { source: 3 });
    return { html, spans, instrumentedText: decode(html) };
  };

  test.each([true, false])(
    'parses to the same tree as the original with scripting %s',
    (scriptingEnabled) => {
      const { instrumentedText } = instrument();
      const comparison = compareTrees(
        parse(originalText, { scriptingEnabled }),
        parse(instrumentedText, { scriptingEnabled }),
      );
      expect(comparison).toEqual({ equal: true });
    },
  );

  test('restores the original bytes when the spliced attributes are removed', () => {
    const { html } = instrument();
    expect(Buffer.compare(withoutSourceAttributes(html), input)).toBe(0);
  });

  test('numbers its spans from 0 in file order', () => {
    const { spans } = instrument();
    expect(spans.length).toBeGreaterThan(0);
    expect(spans.map((span) => span.id)).toEqual(spans.map((_span, index) => index));
    for (const [index, span] of spans.entries()) {
      expect(span.source).toBe(3);
      if (index > 0) {
        expect(span.start_tag.start).toBeGreaterThanOrEqual(spans[index - 1]?.start_tag.end ?? Number.NaN);
      }
    }
  });

  test('gives every span a range and position that locate its tag', () => {
    const { spans, instrumentedText } = instrument();
    const tagNames = new Map<number, string>();
    for (const element of elements(parse(instrumentedText, { sourceCodeLocationInfo: true }), {
      templates: true,
    })) {
      const value = element.attrs.find((attr) => attr.name === SOURCE_ATTRIBUTE)?.value;
      if (value !== undefined && element.sourceCodeLocation?.startTag !== undefined) {
        tagNames.set(Number(value), element.tagName.toLowerCase());
      }
    }
    const bytes = Buffer.from(input);
    const text = (start: number, end: number) => bytes.subarray(start, end).toString('utf8');
    for (const span of spans) {
      const tag = tagNames.get(span.id);
      const startTag = text(span.start_tag.start, span.start_tag.end);
      expect(tag, `span ${span.id} has no element`).toBeDefined();
      expect(startTag.toLowerCase().startsWith(`<${tag}`), startTag).toBe(true);
      expect(startTag.endsWith('>'), startTag).toBe(true);
      if (span.element !== undefined) {
        expect(span.element.start).toBe(span.start_tag.start);
        const element = text(span.element.start, span.element.end);
        expect(new RegExp(`</${tag}[\\t\\n\\f\\r ]*>$`, 'i').test(element), element).toBe(true);
      }
      const lines = decode(bytes.subarray(0, span.start_tag.start)).split(/\r\n|\r|\n/);
      expect(span.line).toBe(lines.length);
      expect(span.column).toBe(Array.from(lines.at(-1) ?? '').length + 1);
    }
  });

  test('labels every element with its own span, or a cloned element with its original span', () => {
    const { spans, instrumentedText } = instrument();
    const offsetToId = new Map(spans.map((span) => [span.start_tag.start, span.id]));
    const originalElements = [
      ...elements(parse(originalText, { sourceCodeLocationInfo: true }), { templates: true }),
    ];
    const instrumentedElements = [...elements(parse(instrumentedText), { templates: true })];
    expect(instrumentedElements.length).toBe(originalElements.length);
    const bytes = Buffer.from(input);
    for (const [index, original] of originalElements.entries()) {
      const value = instrumentedElements[index]?.attrs.find((attr) => attr.name === SOURCE_ATTRIBUTE)?.value;
      const startTag = original.sourceCodeLocation?.startTag;
      if (startTag !== undefined) {
        const byteOffset = bomLength(input) + Buffer.byteLength(originalText.slice(0, startTag.startOffset));
        expect(value).toBe(String(offsetToId.get(byteOffset)));
      } else if (value !== undefined) {
        const span = spans[Number(value)];
        expect(span, `no span ${value}`).toBeDefined();
        const source = bytes.subarray(span?.start_tag.start, span?.start_tag.end).toString('utf8');
        expect(source.toLowerCase().startsWith(`<${original.tagName}`), source).toBe(true);
      }
    }
  });
});

describe.each(accepted)('%s with class tokens', (name) => {
  const input = readFileSync(new URL(name, tortureUrl));
  const originalText = decode(input);
  const instrument = () => {
    const { html, spans } = instrumentHtml(input, { source: 3, classToken: true });
    return { html, spans, instrumentedText: decode(html) };
  };
  /** Each span's token, by the source attribute value that names the span. */
  const tokensOf = (spans: { id: number; line: number; column: number }[]) =>
    new Map(spans.map((span) => [String(span.id), sourceClassToken(span)]));

  test.each([true, false])(
    'parses to the same tree as the original, but for the tokens, with scripting %s',
    (scriptingEnabled) => {
      const { instrumentedText, spans } = instrument();
      const comparison = compareTrees(
        parse(originalText, { scriptingEnabled }),
        parse(instrumentedText, { scriptingEnabled }),
        { classTokens: tokensOf(spans) },
      );
      expect(comparison).toEqual({ equal: true });
    },
  );

  test('gives the same spans as without class tokens', () => {
    expect(instrument().spans).toEqual(instrumentHtml(input, { source: 3 }).spans);
  });

  test("puts each tagged element's token in its class, after the author's own tokens", () => {
    const { instrumentedText, spans } = instrument();
    const original = [...elements(parse(originalText), { templates: true })];
    const instrumented = [...elements(parse(instrumentedText), { templates: true })];
    expect(instrumented.length).toBe(original.length);
    const seen = new Set<number>();
    for (const [index, element] of instrumented.entries()) {
      const id = element.attrs.find((attr) => attr.name === SOURCE_ATTRIBUTE)?.value;
      const span = id === undefined ? undefined : spans[Number(id)];
      if (span === undefined) {
        continue;
      }
      seen.add(span.id);
      const classOf = (attrs: { name: string; value: string }[]) =>
        attrs.find((attr) => attr.name === 'class')?.value;
      const before = classOf(original[index]?.attrs ?? []);
      expect(classOf(element.attrs)).toBe(
        before === undefined ? sourceClassToken(span) : `${before} ${sourceClassToken(span)}`,
      );
    }
    expect(seen.size).toBe(spans.length);
  });

  test('is refused with dom-changed when a token reaches the wrong element', () => {
    const { instrumentedText, spans } = instrument();
    if (spans.length < 2) {
      return;
    }
    const shifted = new Map(
      spans.map((span, index) => [
        String(span.id),
        sourceClassToken(spans[(index + 1) % spans.length] ?? span),
      ]),
    );
    const comparison = compareTrees(parse(originalText), parse(instrumentedText), { classTokens: shifted });
    expect(comparison.equal).toBe(false);
  });
});

/** The code a rejected fixture must raise: its file name up to the first dot. */
function expectedCode(name: string): InstrumentHtmlErrorCode {
  const code = name.slice(0, name.indexOf('.'));
  const codes: InstrumentHtmlErrorCode[] = [
    'invalid-utf-8',
    'unsupported-encoding',
    'attribute-present',
    'class-token-present',
    'dom-changed',
  ];
  const known = codes.find((candidate) => candidate === code);
  if (known === undefined) {
    throw new Error(`${name} does not start with an error code`);
  }
  return known;
}

/** What every message for a code must say, whichever page raised it. */
const messagePatterns: Record<InstrumentHtmlErrorCode, RegExp> = {
  'invalid-utf-8': /byte offset \d+/,
  'unsupported-encoding': /UTF-16LE|UTF-16BE|"[^"]*" at line \d+, column \d+/,
  'attribute-present': /at line \d+, column \d+/,
  'class-token-present': /"sp-src-[^"]*" at line \d+, column \d+/,
  'dom-changed': /first at \S+/,
};

/** The codes only class-token mode raises; without it, such a page is instrumented as usual. */
const TOKEN_MODE_CODES: ReadonlySet<InstrumentHtmlErrorCode> = new Set(['class-token-present']);

describe.each(rejected)('rejected/%s', (name) => {
  const code = expectedCode(name);
  const input = readFileSync(new URL(name, rejectedUrl));
  const refusal = (classToken: boolean): unknown => {
    try {
      instrumentHtml(input, { source: 0, classToken });
    } catch (error) {
      return error;
    }
    return undefined;
  };

  test.each([false, true])('is refused with the code its name gives, with class tokens %s', (classToken) => {
    const thrown = refusal(classToken);
    if (!classToken && TOKEN_MODE_CODES.has(code)) {
      expect(thrown).toBeUndefined();
      return;
    }
    expect(thrown).toBeInstanceOf(InstrumentHtmlError);
    expect((thrown as InstrumentHtmlError).code).toBe(code);
    expect((thrown as InstrumentHtmlError).message).toMatch(messagePatterns[code]);
  });
});
