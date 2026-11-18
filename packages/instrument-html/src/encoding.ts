import { InstrumentHtmlError } from './error.js';
import { type Document, type Element, elements } from './tree.js';

const UTF8_BOM_LENGTH = 3;

export interface DecodedHtml {
  /** The decoded text, without the byte order mark: the text a browser's UTF-8 decoder yields. */
  text: string;
  /** The length in bytes of the UTF-8 byte order mark the input starts with, 0 or 3. */
  bomLength: 0 | 3;
}

/**
 * Decodes `input` as UTF-8, or refuses it when it starts with a UTF-16 byte order mark or is not
 * valid UTF-8. Encodings are never guessed: spans are UTF-8 byte offsets, and a page decoded any
 * other way would put them on the wrong bytes.
 */
export function decodeUtf8(input: Uint8Array): DecodedHtml {
  if (input[0] === 0xff && input[1] === 0xfe) {
    throw new InstrumentHtmlError(
      'unsupported-encoding',
      'the input starts with a UTF-16LE byte order mark; only UTF-8 input is supported',
    );
  }
  if (input[0] === 0xfe && input[1] === 0xff) {
    throw new InstrumentHtmlError(
      'unsupported-encoding',
      'the input starts with a UTF-16BE byte order mark; only UTF-8 input is supported',
    );
  }
  const invalidAt = firstInvalidUtf8(input);
  if (invalidAt !== undefined) {
    throw new InstrumentHtmlError(
      'invalid-utf-8',
      `the input is not valid UTF-8: the byte sequence at byte offset ${invalidAt} is invalid`,
    );
  }
  const bomLength = input[0] === 0xef && input[1] === 0xbb && input[2] === 0xbf ? UTF8_BOM_LENGTH : 0;
  const text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(input.subarray(bomLength));
  return { text, bomLength };
}

/**
 * The offset of the first byte of the first ill-formed sequence, per the well-formed byte
 * sequences of the Unicode Standard (table 3-7): no overlong forms, no surrogates, nothing above
 * U+10FFFF.
 */
function firstInvalidUtf8(input: Uint8Array): number | undefined {
  let index = 0;
  while (index < input.length) {
    const lead = input[index];
    if (lead === undefined) {
      break;
    }
    if (lead < 0x80) {
      index += 1;
      continue;
    }
    let length: number;
    let secondMin = 0x80;
    let secondMax = 0xbf;
    if (lead >= 0xc2 && lead <= 0xdf) {
      length = 2;
    } else if (lead >= 0xe0 && lead <= 0xef) {
      length = 3;
      if (lead === 0xe0) secondMin = 0xa0;
      if (lead === 0xed) secondMax = 0x9f;
    } else if (lead >= 0xf0 && lead <= 0xf4) {
      length = 4;
      if (lead === 0xf0) secondMin = 0x90;
      if (lead === 0xf4) secondMax = 0x8f;
    } else {
      return index;
    }
    for (let offset = 1; offset < length; offset += 1) {
      const byte = input[index + offset];
      const min = offset === 1 ? secondMin : 0x80;
      const max = offset === 1 ? secondMax : 0xbf;
      if (byte === undefined || byte < min || byte > max) {
        return index;
      }
    }
    index += length;
  }
  return undefined;
}

/** The WHATWG Encoding Standard's labels for UTF-8. */
const UTF8_LABELS: ReadonlySet<string> = new Set([
  'unicode-1-1-utf-8',
  'unicode11utf8',
  'unicode20utf8',
  'utf-8',
  'utf8',
  'x-unicode20utf8',
]);

/** HTML's ASCII whitespace: tab, LF, FF, CR and space. */
function isAsciiWhitespace(char: string | undefined): boolean {
  return char === '\t' || char === '\n' || char === '\f' || char === '\r' || char === ' ';
}

/** Lowercases ASCII letters only; HTML's case-insensitive matches never fold other characters. */
function asciiLowercase(value: string): string {
  return value.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
}

export interface EncodingDeclaration {
  /** The label as written in the page. */
  label: string;
  element: Element;
}

/**
 * The first `meta` element, outside `template` contents, that declares an encoding other than
 * UTF-8. Both `<meta charset>` and `<meta http-equiv="content-type" content>` count, as they do
 * for the HTML tree builder, with `charset` taking precedence when an element has both.
 */
export function nonUtf8Declaration(document: Document): EncodingDeclaration | undefined {
  for (const element of elements(document, { templates: false })) {
    if (element.tagName !== 'meta' || element.namespaceURI !== 'http://www.w3.org/1999/xhtml') {
      continue;
    }
    const label = declaredLabel(element);
    if (label !== undefined && !UTF8_LABELS.has(normaliseLabel(label))) {
      return { label, element };
    }
  }
  return undefined;
}

function declaredLabel(element: Element): string | undefined {
  const attribute = (name: string) => element.attrs.find((attr) => attr.name === name)?.value;
  const charset = attribute('charset');
  if (charset !== undefined) {
    return charset;
  }
  const httpEquiv = attribute('http-equiv');
  const content = attribute('content');
  if (httpEquiv === undefined || asciiLowercase(httpEquiv) !== 'content-type' || content === undefined) {
    return undefined;
  }
  return charsetFromContent(content);
}

/** Trims ASCII whitespace and lowercases ASCII letters only, as the Encoding Standard's "get an encoding" does. */
function normaliseLabel(label: string): string {
  return asciiLowercase(label.replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, ''));
}

/** HTML's "algorithm for extracting a character encoding from a meta element". */
export function charsetFromContent(content: string): string | undefined {
  const lowercase = asciiLowercase(content);
  let position = 0;
  for (;;) {
    const found = lowercase.indexOf('charset', position);
    if (found === -1) {
      return undefined;
    }
    position = skipWhitespace(content, found + 'charset'.length);
    if (content[position] !== '=') {
      continue;
    }
    position = skipWhitespace(content, position + 1);
    const next = content[position];
    if (next === undefined) {
      return undefined;
    }
    if (next === '"' || next === "'") {
      const close = content.indexOf(next, position + 1);
      return close === -1 ? undefined : content.slice(position + 1, close);
    }
    let end = position;
    while (end < content.length && content[end] !== ';' && !isAsciiWhitespace(content[end])) {
      end += 1;
    }
    return content.slice(position, end);
  }
}

function skipWhitespace(content: string, from: number): number {
  let position = from;
  while (isAsciiWhitespace(content[position])) {
    position += 1;
  }
  return position;
}
