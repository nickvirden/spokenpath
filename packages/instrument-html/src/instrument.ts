import { parse } from 'parse5';
import type { Span } from '@spokenpath/snapshot-types';
import { decodeUtf8, nonUtf8Declaration } from './encoding.ts';
import { InstrumentHtmlError } from './error.ts';
import { CLASS_TOKEN_PREFIX, sourceClassToken } from './class-token.ts';
import { SOURCE_ATTRIBUTE } from './source-attribute.ts';
import { SourceIndex } from './source-index.ts';
import { attributeInsertionOffset, classValueEdit } from './start-tag.ts';
import { compareTrees } from './tree-equality.ts';
import { type Document, type Element, elements } from './tree.ts';

export interface InstrumentHtmlOptions {
  /** The `id` of the source file, written into every span's `source`. */
  source: number;
  /**
   * Whether every tagged start tag also gets its span's class token (`sourceClassToken`), so the
   * source location reaches accessibility APIs that expose an element's class list but not its
   * attributes. The token is added to the element's first `class` attribute, or in a new one.
   * Off by default, which leaves the output exactly as without the option.
   */
  classToken?: boolean;
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
  /** Where the tag's first `class` attribute starts, when it has one. */
  classStart?: number;
}

/** Text spliced into the input at a UTF-16 offset of the decoded text. */
interface Splice {
  at: number;
  text: string;
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
 * With `classToken`, every tagged start tag also gets its span's class token, and the comparison
 * allows exactly that one change to each tagged element's `class`.
 *
 * @throws {InstrumentHtmlError} when the input is not UTF-8, declares another encoding, already
 *   carries the source attribute (or, with `classToken`, a source class token), or would parse to
 *   a different tree once instrumented.
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

  const classToken = options.classToken === true;
  const tags = locatedTags(original, index, classToken);
  const encoder = new TextEncoder();
  const chunks: Uint8Array[] = [];
  const spans: Span[] = [];
  const tokens = new Map<string, string>();
  let copiedTo = 0;
  for (const [id, tag] of tags.entries()) {
    const span = spanFor(id, tag, options.source, index);
    spans.push(span);
    let attributes = ` ${SOURCE_ATTRIBUTE}="${id}"`;
    // A token joining an existing `class` goes first: at an offset both share, it ends that
    // attribute's value before the source attribute starts.
    const splices: Splice[] = [];
    if (classToken) {
      const token = sourceClassToken(span);
      tokens.set(String(id), token);
      if (tag.classStart === undefined) {
        attributes += ` class="${token}"`;
      } else {
        splices.push(classValueEdit(text, tag.classStart, tag.end, token));
      }
    }
    splices.push({ at: attributeInsertionOffset(text, tag.start, tag.end), text: attributes });
    splices.sort((left, right) => left.at - right.at);
    for (const splice of splices) {
      const insertAt = index.byteOffset(splice.at);
      chunks.push(input.subarray(copiedTo, insertAt), encoder.encode(splice.text));
      copiedTo = insertAt;
    }
  }
  chunks.push(input.subarray(copiedTo));
  const html = concatenate(chunks);

  const difference = compareTrees(
    original,
    parseHtml(decodeUtf8(html).text, false),
    classToken ? { classTokens: tokens } : {},
  );
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
 * attributes, so the author's value would silently win over the spliced one. With `classToken`, it
 * also refuses a page whose class already holds a source class token, which would be read back as
 * a source location.
 */
function locatedTags(document: Document, index: SourceIndex, classToken: boolean): LocatedTag[] {
  const byStart = new Map<number, LocatedTag>();
  for (const element of elements(document, { templates: true })) {
    if (element.attrs.some((attr) => attr.name === SOURCE_ATTRIBUTE)) {
      throw new InstrumentHtmlError(
        'attribute-present',
        `an element already has the ${SOURCE_ATTRIBUTE} attribute ${describeLocation(element, index)}; it would shadow the source location added to it`,
      );
    }
    const authorToken = classToken ? sourceTokenIn(element) : undefined;
    if (authorToken !== undefined) {
      throw new InstrumentHtmlError(
        'class-token-present',
        `an element's class already holds the token ${JSON.stringify(authorToken)} ${describeLocation(element, index)}; it would be read back as a source location`,
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
    // parse5 locates the first of duplicate attributes, which is the one the element keeps.
    const classAttribute = classToken ? location?.attrs?.class : undefined;
    byStart.set(startTag.startOffset, {
      start: startTag.startOffset,
      end: startTag.endOffset,
      ...(endTag === undefined ? {} : { elementEnd: endTag.endOffset }),
      ...(classAttribute === undefined ? {} : { classStart: classAttribute.startOffset }),
    });
  }
  // Foster parenting moves elements out of file order, so tree order is not file order.
  return [...byStart.values()].sort((left, right) => left.start - right.start);
}

/** The first token of the element's `class` that starts like a source class token. */
function sourceTokenIn(element: Element): string | undefined {
  const value = element.attrs.find((attr) => attr.name === 'class' && attr.namespace === undefined)?.value;
  return value?.split(/[\t\n\f\r ]+/).find((token) => token.startsWith(CLASS_TOKEN_PREFIX));
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
