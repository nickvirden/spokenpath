import { parse } from 'parse5';
import type { Span } from '@spokenpath/snapshot-types';
import { decodeUtf8, nonUtf8Declaration } from './encoding.js';
import { InstrumentHtmlError } from './error.js';
import { SOURCE_ATTRIBUTE } from './source-attribute.js';
import { SourceIndex } from './source-index.js';
import { attributeInsertionOffset } from './start-tag.js';
import { compareTrees } from './tree-equality.js';
import { type Document, type Element, elements } from './tree.js';

export interface InstrumentHtmlOptions {
  /** The `id` of the source file, written into every span's `source`. */
  source: number;
}

export interface InstrumentedHtml {
  /** The input bytes with the source attribute spliced into every located start tag. */
  html: Uint8Array;
  /** One span per spliced start tag, in id order, which is the order of the tags in the file. */
  spans: Span[];
}

/** A start tag found by the parser, with offsets in UTF-16 code units of the decoded text. */
interface LocatedTag {
  start: number;
  end: number;
  /** One past the end of the element's end tag, when the element has one. */
  elementEnd?: number;
}

/** The input is parsed the way a browser with scripting enabled parses it. */
function parseHtml(text: string, sourceCodeLocationInfo: boolean): Document {
  return parse(text, { sourceCodeLocationInfo });
}

/**
 * Adds `data-spokenpath-src="<span id>"` to every start tag the parser locates in `input`, and
 * returns the instrumented bytes with the spans table that the ids refer to.
 *
 * The attribute is spliced into the original bytes, which are never re-serialised, and the result
 * is re-parsed and compared with the parse of the input before it is returned: a page whose tree
 * the splice would change is refused with `dom-changed`, never returned.
 *
 * @throws {InstrumentHtmlError} when the input is not UTF-8, declares another encoding, already
 *   carries the source attribute, or would parse to a different tree once instrumented.
 */
export function instrumentHtml(input: Uint8Array, options: InstrumentHtmlOptions): InstrumentedHtml {
  const { text, bomLength } = decodeUtf8(input);
  const index = new SourceIndex(text, bomLength);
  const original = parseHtml(text, true);

  const declaration = nonUtf8Declaration(original);
  if (declaration !== undefined) {
    throw new InstrumentHtmlError(
      'unsupported-encoding',
      `the page declares the encoding ${JSON.stringify(declaration.label)} ${describeLocation(declaration.element, index)}; only UTF-8 input is supported`,
    );
  }

  const tags = locatedTags(original, index);
  const encoder = new TextEncoder();
  const chunks: Uint8Array[] = [];
  const spans: Span[] = [];
  let copiedTo = 0;
  for (const [id, tag] of tags.entries()) {
    const insertAt = index.byteOffset(attributeInsertionOffset(text, tag.start, tag.end));
    chunks.push(input.subarray(copiedTo, insertAt), encoder.encode(` ${SOURCE_ATTRIBUTE}="${id}"`));
    copiedTo = insertAt;
    spans.push(spanFor(id, tag, options.source, index));
  }
  chunks.push(input.subarray(copiedTo));
  const html = concatenate(chunks);

  const difference = compareTrees(original, parseHtml(decodeUtf8(html).text, false));
  if (!difference.equal) {
    throw new InstrumentHtmlError(
      'dom-changed',
      `the instrumented page parses to a different tree than the input, first at ${difference.path}: ${difference.reason}`,
    );
  }
  return { html, spans };
}

/**
 * The start tags of every located element, including `template` contents, in file order. It refuses
 * a page that already carries the source attribute: the parser keeps the first of two same-named
 * attributes, so the author's value would silently win over the spliced one.
 */
function locatedTags(document: Document, index: SourceIndex): LocatedTag[] {
  const byStart = new Map<number, LocatedTag>();
  for (const element of elements(document, { templates: true })) {
    if (element.attrs.some((attr) => attr.name === SOURCE_ATTRIBUTE)) {
      throw new InstrumentHtmlError(
        'attribute-present',
        `an element already has the ${SOURCE_ATTRIBUTE} attribute ${describeLocation(element, index)}; it would shadow the source location added to it`,
      );
    }
    const location = element.sourceCodeLocation;
    const startTag = location?.startTag;
    // An element the parser reopens when it reconstructs active formatting elements
    // (`<i><b>x</i>y</b>` reopens `b` around `y`) is created from the original token and reports
    // the original's start tag, so one start tag can own several elements. It is spliced once.
    if (startTag === undefined || byStart.has(startTag.startOffset)) {
      continue;
    }
    const endTag = location?.endTag;
    byStart.set(startTag.startOffset, {
      start: startTag.startOffset,
      end: startTag.endOffset,
      ...(endTag === undefined ? {} : { elementEnd: endTag.endOffset }),
    });
  }
  // Foster parenting moves elements out of file order, so tree order is not file order.
  return [...byStart.values()].sort((left, right) => left.start - right.start);
}

function spanFor(id: number, tag: LocatedTag, source: number, index: SourceIndex): Span {
  const start = index.byteOffset(tag.start);
  const { line, column } = index.position(tag.start);
  return {
    id,
    source,
    start_tag: { start, end: index.byteOffset(tag.end) },
    ...(tag.elementEnd === undefined ? {} : { element: { start, end: index.byteOffset(tag.elementEnd) } }),
    line,
    column,
  };
}

/** "at line L, column C" for an element written in the page, or why it has no position. */
function describeLocation(element: Element, index: SourceIndex): string {
  const offset = element.sourceCodeLocation?.startTag?.startOffset;
  if (offset === undefined) {
    return `on a <${element.tagName}> element the parser created without a start tag of its own`;
  }
  const { line, column } = index.position(offset);
  return `at line ${line}, column ${column}`;
}

function concatenate(chunks: readonly Uint8Array[]): Uint8Array {
  const result = new Uint8Array(chunks.reduce((total, chunk) => total + chunk.length, 0));
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }
  return result;
}
