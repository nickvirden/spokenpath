import type { Span } from '@spokenpath/snapshot-types';
import { describe, expect, test } from 'vitest';
import { flattenDocument } from './dom.ts';
import type { ProtocolNode } from './protocol.ts';
import { compareDocuments } from './verify.ts';

let nextBackendId = 1;

function node(fields: Partial<ProtocolNode> & Pick<ProtocolNode, 'nodeType'>): ProtocolNode {
  nextBackendId += 1;
  return { backendNodeId: nextBackendId, nodeName: '', localName: '', nodeValue: '', ...fields };
}

function element(localName: string, fields: Partial<ProtocolNode> = {}): ProtocolNode {
  return node({ nodeType: 1, nodeName: localName.toUpperCase(), localName, ...fields });
}

function text(value: string): ProtocolNode {
  return node({ nodeType: 3, nodeName: '#text', nodeValue: value });
}

function documentOf(...children: ProtocolNode[]): ProtocolNode {
  return node({ nodeType: 9, nodeName: '#document', children });
}

const span = (id: number): Span => ({ id, source: 0, start_tag: { start: 0, end: 1 }, line: 1, column: 1 });

describe('flattenDocument', () => {
  test('lists shadow roots, then template content, then children, in pre-order from id 0', () => {
    const shadow = node({
      nodeType: 11,
      nodeName: '#document-fragment',
      shadowRootType: 'open',
      children: [element('i')],
    });
    const content = node({ nodeType: 11, nodeName: '#document-fragment' });
    const host = element('template', {
      attributes: ['data-spokenpath-src', '0', 'id', 't'],
      shadowRoots: [shadow],
      templateContent: content,
      children: [text('x')],
    });
    const { dom } = flattenDocument(documentOf(element('html', { children: [host] })), [span(0)], '');
    expect(dom.map(({ id, parent, children, kind }) => ({ id, parent, children, kind }))).toEqual([
      { id: 0, parent: undefined, children: [1], kind: 'document' },
      { id: 1, parent: 0, children: [2], kind: 'element' },
      { id: 2, parent: 1, children: [3, 5, 6], kind: 'element' },
      { id: 3, parent: 2, children: [4], kind: 'document-fragment' },
      { id: 4, parent: 3, children: [], kind: 'element' },
      { id: 5, parent: 2, children: [], kind: 'document-fragment' },
      { id: 6, parent: 2, children: [], kind: 'text' },
    ]);
    expect(dom[0]).not.toHaveProperty('parent');
    expect(dom[2]).toMatchObject({ attributes: [{ name: 'id', value: 't' }], span: 0, origin: 'source' });
    expect(dom[3]?.shadow).toBe('open');
    expect(dom[4]?.origin).toBe('script');
    expect(dom[6]?.text).toEqual({ value: 'x', truncated: false });
  });

  test('records a frame element with a diagnostic and does not follow its document', () => {
    const frameDocument = documentOf(element('html'));
    const iframe = element('iframe', { contentDocument: frameDocument });
    const { dom, diagnostics } = flattenDocument(documentOf(element('html', { children: [iframe] })), [], '');
    expect(dom).toHaveLength(3);
    expect(diagnostics).toEqual([expect.objectContaining({ code: 'iframe-not-captured', node: 2 })]);
  });

  test('a source attribute that names no span gives origin unknown and a diagnostic', () => {
    const tagged = element('p', { attributes: ['data-spokenpath-src', '7'] });
    const { dom, diagnostics } = flattenDocument(documentOf(tagged), [span(0)], '');
    expect(dom[1]).toMatchObject({ origin: 'unknown', attributes: [] });
    expect(dom[1]).not.toHaveProperty('span');
    expect(diagnostics).toEqual([expect.objectContaining({ code: 'unknown-source-attribute', node: 1 })]);
  });

  test('a parser-created element at the same place is implied; a script-added one is not', () => {
    const body = element('body', {
      children: [element('p', { attributes: ['data-spokenpath-src', '0'] }), element('div')],
    });
    const html = element('html', { children: [element('head'), body] });
    const { dom } = flattenDocument(documentOf(html), [span(0)], '<p data-spokenpath-src="0">x');
    expect(dom.map((record) => [record.tag, record.origin])).toEqual([
      [undefined, undefined],
      ['html', 'implied'],
      ['head', 'implied'],
      ['body', 'implied'],
      ['p', 'source'],
      ['div', 'script'],
    ]);
  });

  test('refuses a node type the snapshot has no kind for', () => {
    expect(() => flattenDocument(documentOf(node({ nodeType: 2 })), [], '')).toThrow('node of type 2');
  });
});

describe('compareDocuments', () => {
  const page = (attributes: string[], value = 'x') =>
    documentOf(element('html', { children: [element('p', { attributes, children: [text(value)] })] }));

  test('ignores the source attribute on the instrumented side and protocol ids', () => {
    expect(compareDocuments(page(['id', 'a']), page(['id', 'a', 'data-spokenpath-src', '0']))).toEqual({
      equal: true,
    });
  });

  test('reports the first differing node by path', () => {
    expect(compareDocuments(page(['id', 'a']), page(['id', 'b']))).toMatchObject({
      equal: false,
      path: 'html[0] > p[0]',
    });
    expect(compareDocuments(page([]), page([], 'y'))).toMatchObject({
      equal: false,
      path: 'html[0] > p[0] > #text[0]',
    });
  });

  test('keeps the source attribute on the original side', () => {
    expect(compareDocuments(page(['data-spokenpath-src', '0']), page([])).equal).toBe(false);
  });
});
