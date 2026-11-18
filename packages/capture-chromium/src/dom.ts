import { SOURCE_ATTRIBUTE } from '@spokenpath/instrument-html';
import type { Diagnostic, DomNode, DomNodeKind, Origin, Span } from '@spokenpath/snapshot-types';
import { type DefaultTreeAdapterTypes, parse } from 'parse5';
import { attributePairs, NodeType, type ProtocolNode, recordedChildren } from './protocol.ts';

const KINDS: ReadonlyMap<number, DomNodeKind> = new Map([
  [NodeType.document, 'document'],
  [NodeType.doctype, 'doctype'],
  [NodeType.element, 'element'],
  [NodeType.text, 'text'],
  [NodeType.comment, 'comment'],
  [NodeType.cdataSection, 'cdata-section'],
  [NodeType.processingInstruction, 'processing-instruction'],
  [NodeType.documentFragment, 'document-fragment'],
]);

const TEXT_KINDS: ReadonlySet<DomNodeKind> = new Set([
  'text',
  'comment',
  'cdata-section',
  'processing-instruction',
]);

/** Where a node sits, which decides an element's origin when it carries no source attribute. */
type Placement =
  /** In the document's light tree; `path` holds the element-child index at each level. */
  | { tree: 'light'; path: readonly number[] }
  /** Inside a user-agent shadow root, which the browser builds itself. */
  | { tree: 'user-agent' }
  /** Inside an author shadow root, or inside template content. */
  | { tree: 'other' };

export interface FlattenedDom {
  dom: DomNode[];
  diagnostics: Diagnostic[];
  /** The `dom` id of each `backendNodeId`. */
  idByBackendId: Map<number, number>;
}

/** The parse5 elements of the served text, found by element-child-index path from the document. */
class ParsedPaths {
  readonly #document: DefaultTreeAdapterTypes.Document;

  constructor(servedText: string) {
    this.#document = parse(servedText, { scriptingEnabled: true, sourceCodeLocationInfo: true });
  }

  /** Whether the parser created an element named `localName` at `path` with no start tag. */
  isImplied(path: readonly number[], localName: string): boolean {
    let parent: DefaultTreeAdapterTypes.ParentNode = this.#document;
    let element: DefaultTreeAdapterTypes.Element | undefined;
    for (const index of path) {
      element = parent.childNodes.filter((child) => 'tagName' in child)[index];
      if (element === undefined) {
        return false;
      }
      parent = element;
    }
    return (
      element !== undefined &&
      element.tagName === localName &&
      element.sourceCodeLocation?.startTag === undefined
    );
  }
}

const SPAN_ID = /^(?:0|[1-9][0-9]*)$/;

/**
 * Flattens the tree from one `DOM.getDocument({ depth: -1, pierce: true })` call into `dom[]` in
 * pre-order, so the document is id 0. A node lists its shadow roots first, then its template
 * content, then its children. Nothing is filtered or reshaped beyond that, except the source
 * attribute, which becomes the element's `span` (ADR-0002).
 *
 * `servedText` is the decoded text the page was served as; an element with no source attribute in
 * the light tree is `implied` when its parse has a parser-created element at the same place.
 */
export function flattenDocument(
  root: ProtocolNode,
  spans: readonly Span[],
  servedText: string,
): FlattenedDom {
  const parsed = new ParsedPaths(servedText);
  const spanIds = new Set(spans.map((span) => span.id));
  const dom: DomNode[] = [];
  const diagnostics: Diagnostic[] = [];
  const idByBackendId = new Map<number, number>();

  // The walk keeps its own stack, because a page puts no limit on nesting depth.
  const pending: { node: ProtocolNode; parent?: DomNode; placement: Placement }[] = [
    { node: root, placement: { tree: 'light', path: [] } },
  ];
  for (let item = pending.pop(); item !== undefined; item = pending.pop()) {
    const { node, parent, placement } = item;
    const kind = KINDS.get(node.nodeType);
    if (kind === undefined) {
      throw new Error(
        `the protocol returned a node of type ${node.nodeType}, which the snapshot has no kind for`,
      );
    }
    const id = dom.length;
    const pairs = attributePairs(node);
    const record: DomNode = {
      id,
      backend_id: node.backendNodeId,
      ...(parent === undefined ? {} : { parent: parent.id }),
      children: [],
      kind,
      ...(kind === 'element' ? { tag: node.localName } : {}),
      attributes: pairs
        .filter(([name]) => name !== SOURCE_ATTRIBUTE)
        .map(([name, value]) => ({ name, value })),
      ...(TEXT_KINDS.has(kind) ? { text: { value: node.nodeValue, truncated: false } } : {}),
      ...(node.shadowRootType === undefined ? {} : { shadow: node.shadowRootType }),
    };
    if (kind === 'element') {
      const sourceValue = pairs.find(([name]) => name === SOURCE_ATTRIBUTE)?.[1];
      const origin = elementOrigin(sourceValue, spanIds, placement, node.localName, parsed);
      if (origin.span !== undefined) {
        record.span = origin.span;
      }
      record.origin = origin.origin;
      if (sourceValue !== undefined && origin.span === undefined) {
        diagnostics.push({
          code: 'unknown-source-attribute',
          message: `The element's ${SOURCE_ATTRIBUTE} value ${JSON.stringify(sourceValue)} names no span, so its source is unknown.`,
          node: id,
        });
      }
      if (node.contentDocument !== undefined || node.localName === 'iframe' || node.localName === 'frame') {
        diagnostics.push({
          code: 'iframe-not-captured',
          message: `The contents of this <${node.localName}> are not captured; only the element itself is recorded.`,
          node: id,
        });
      }
    }
    dom.push(record);
    parent?.children.push(id);
    idByBackendId.set(node.backendNodeId, id);

    const children = recordedChildren(node);
    const lightChildren = new Set(node.children ?? []);
    let elementIndex = 0;
    const childItems = children.map((child) => {
      const childPlacement = placementOf(child, placement, lightChildren.has(child), elementIndex);
      if (lightChildren.has(child) && child.nodeType === NodeType.element) {
        elementIndex += 1;
      }
      return { node: child, parent: record, placement: childPlacement };
    });
    for (const childItem of childItems.reverse()) {
      pending.push(childItem);
    }
  }
  return { dom, diagnostics, idByBackendId };
}

/** A shadow root and template content leave the light tree; a user-agent shadow root is marked. */
function placementOf(
  child: ProtocolNode,
  parentPlacement: Placement,
  isLightChild: boolean,
  elementIndex: number,
): Placement {
  if (parentPlacement.tree === 'user-agent' || child.shadowRootType === 'user-agent') {
    return { tree: 'user-agent' };
  }
  if (!isLightChild || parentPlacement.tree === 'other') {
    return { tree: 'other' };
  }
  return child.nodeType === NodeType.element
    ? { tree: 'light', path: [...parentPlacement.path, elementIndex] }
    : parentPlacement;
}

/** An element's origin by the first rule that matches, and its span when it has one. */
function elementOrigin(
  sourceValue: string | undefined,
  spanIds: ReadonlySet<number>,
  placement: Placement,
  localName: string,
  parsed: ParsedPaths,
): { origin: Origin; span?: number } {
  if (sourceValue !== undefined) {
    const span = SPAN_ID.test(sourceValue) ? Number(sourceValue) : undefined;
    return span !== undefined && spanIds.has(span) ? { origin: 'source', span } : { origin: 'unknown' };
  }
  if (placement.tree === 'user-agent') {
    return { origin: 'unknown' };
  }
  if (placement.tree === 'light' && parsed.isImplied(placement.path, localName)) {
    return { origin: 'implied' };
  }
  return { origin: 'script' };
}
