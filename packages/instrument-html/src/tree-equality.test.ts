import { readFileSync } from 'node:fs';
import { html, parse, serialize } from 'parse5';
import { describe, expect, test } from 'vitest';
import { SOURCE_ATTRIBUTE } from './source-attribute.js';
import { compareTrees } from './tree-equality.js';
import { type Document, type Element, elements, isTemplate } from './tree.js';

const fosterParentingUrl = new URL('../../../fixtures/pages/torture/foster-parenting.html', import.meta.url);

/**
 * The rejected design: parse, add the attribute to every located element, and serialise. The
 * serialiser cannot reproduce every input, so its output can parse to a different tree.
 */
function reserialisingInstrumenter(text: string): string {
  const document = parse(text, { sourceCodeLocationInfo: true });
  let id = 0;
  for (const element of elements(document, { templates: true })) {
    if (element.sourceCodeLocation?.startTag !== undefined) {
      element.attrs.push({ name: SOURCE_ATTRIBUTE, value: String(id) });
      id += 1;
    }
  }
  return serialize(document);
}

const page = `<!DOCTYPE html>
<p class="a" id="b">text<b>bold</b></p>
<svg><circle r="1"/><use xlink:href="#a"/></svg>
<template><td>cell</td></template>`;

function find(document: Document, tag: string): Element {
  const element = [...elements(document, { templates: true })].find((candidate) => candidate.tagName === tag);
  if (element === undefined) {
    throw new Error(`the page has no <${tag}>`);
  }
  return element;
}

/** Parses `page` and applies `mutate` to the copy that stands for the instrumented tree. */
function compareMutated(mutate: (document: Document) => void) {
  const mutated = parse(page);
  mutate(mutated);
  return compareTrees(parse(page), mutated);
}

test('equal trees compare equal', () => {
  expect(compareTrees(parse(page), parse(page))).toEqual({ equal: true });
});

test('the source attribute on the instrumented tree is ignored', () => {
  const tagged = page.replace('<p class="a" id="b">', `<p class="a" id="b" ${SOURCE_ATTRIBUTE}="0">`);
  expect(compareTrees(parse(page), parse(tagged))).toEqual({ equal: true });
});

test('a re-serialising instrumenter is reported as changing the foster-parenting page', () => {
  const text = readFileSync(fosterParentingUrl, 'utf8');
  const comparison = compareTrees(parse(text), parse(reserialisingInstrumenter(text)));
  expect(comparison.equal).toBe(false);
});

describe('a single mutation is reported', () => {
  test.each<[string, (document: Document) => void, string]>([
    [
      'a changed attribute value',
      (document) => {
        const attr = find(document, 'p').attrs[0];
        if (attr !== undefined) attr.value = 'changed';
      },
      'html[0] > body[0] > p[0]',
    ],
    [
      'reordered attributes',
      (document) => {
        find(document, 'p').attrs.reverse();
      },
      'html[0] > body[0] > p[0]',
    ],
    [
      'a dropped text node',
      (document) => {
        find(document, 'p').childNodes.shift();
      },
      'html[0] > body[0] > p[0] > #text[0]',
    ],
    [
      'a changed text node',
      (document) => {
        const text = find(document, 'p').childNodes[0];
        if (text !== undefined && 'value' in text) text.value = 'changed';
      },
      'html[0] > body[0] > p[0] > #text[0]',
    ],
    [
      'a changed namespace',
      (document) => {
        find(document, 'circle').namespaceURI = html.NS.MATHML;
      },
      'html[0] > body[0] > svg[0] > circle[0]',
    ],
    [
      'a changed attribute namespace',
      (document) => {
        const attr = find(document, 'use').attrs[0];
        if (attr !== undefined) attr.namespace = html.NS.XML;
      },
      'html[0] > body[0] > svg[0] > use[0]',
    ],
    [
      'a changed attribute prefix',
      (document) => {
        const attr = find(document, 'use').attrs[0];
        if (attr !== undefined) attr.prefix = 'xml';
      },
      'html[0] > body[0] > svg[0] > use[0]',
    ],
    [
      'changed template contents',
      (document) => {
        const template = find(document, 'template');
        if (isTemplate(template)) template.content.childNodes.pop();
      },
      'html[0] > body[0] > template[0] > #content > td[0]',
    ],
    [
      'a changed document mode',
      (document) => {
        document.mode = parse(page.replace('<!DOCTYPE html>', '')).mode;
      },
      '#document',
    ],
  ])('%s', (_name, mutate, path) => {
    const comparison = compareMutated(mutate);
    expect(comparison.equal).toBe(false);
    expect(comparison.equal === false && comparison.path).toBe(path);
  });
});

test('a removed doctype is reported as a change of document mode', () => {
  const comparison = compareTrees(parse(page), parse(page.replace('<!DOCTYPE html>', '')));
  expect(comparison).toMatchObject({
    equal: false,
    path: '#document',
    reason: 'document mode no-quirks became quirks',
  });
});

test('a changed doctype identifier is reported', () => {
  const comparison = compareTrees(
    parse('<!DOCTYPE html SYSTEM "about:legacy-compat"><p>'),
    parse('<!DOCTYPE html SYSTEM "about:other"><p>'),
  );
  expect(comparison).toMatchObject({ equal: false, path: '#documentType[0]' });
});

test('a changed comment is reported', () => {
  const comparison = compareTrees(parse('<!--a--><p>'), parse('<!--b--><p>'));
  expect(comparison).toMatchObject({ equal: false, path: '#comment[0]' });
});
