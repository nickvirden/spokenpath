import { SOURCE_ATTRIBUTE } from '@spokenpath/instrument-html';
import { attributePairs, NodeType, type ProtocolNode, recordedChildren } from './protocol.ts';

export type DomComparison =
  | { equal: true }
  | {
      equal: false;
      /** The first differing node, such as `html[0] > body[0] > p[1]`. */
      path: string;
      /** What differs there. */
      reason: string;
    };

/** A pair of nodes at the same place in both trees; one side is missing when the counts differ. */
interface Pair {
  original: ProtocolNode | undefined;
  instrumented: ProtocolNode | undefined;
  path: string;
}

function childName(node: ProtocolNode): string {
  if (node.shadowRootType !== undefined) {
    return '#shadow-root';
  }
  return node.nodeType === NodeType.element ? node.localName : node.nodeName.toLowerCase();
}

/**
 * The pairs of recorded children of two nodes, last first, so popping them off a stack visits them
 * in document order. Each child is named by its node name and its index among same-named siblings.
 */
function childPairs(original: ProtocolNode, instrumented: ProtocolNode, parentPath: string): Pair[] {
  const left = recordedChildren(original);
  const right = recordedChildren(instrumented);
  const ordinals = new Map<string, number>();
  const pairs: Pair[] = [];
  for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
    const leftChild = left[index];
    const rightChild = right[index];
    const isContent =
      leftChild === undefined
        ? rightChild === instrumented.templateContent
        : leftChild === original.templateContent;
    const sample = leftChild ?? rightChild;
    const name = isContent ? '#content' : sample === undefined ? '' : childName(sample);
    const ordinal = ordinals.get(name) ?? 0;
    ordinals.set(name, ordinal + 1);
    const segment = isContent ? name : `${name}[${ordinal}]`;
    pairs.push({
      original: leftChild,
      instrumented: rightChild,
      path: parentPath === '' ? segment : `${parentPath} > ${segment}`,
    });
  }
  return pairs.reverse();
}

function describeAttributes(pairs: [string, string][]): string {
  return JSON.stringify(pairs.map(([name, value]) => ({ name, value })));
}

/** What differs between two nodes, ignoring their children and protocol ids, or nothing. */
function compareNodes(original: ProtocolNode, instrumented: ProtocolNode): string | undefined {
  if (original.nodeType !== instrumented.nodeType) {
    return `node type ${original.nodeType} became ${instrumented.nodeType}`;
  }
  if (original.nodeName !== instrumented.nodeName) {
    return `node name ${original.nodeName} became ${instrumented.nodeName}`;
  }
  if (original.localName !== instrumented.localName) {
    return `local name ${original.localName} became ${instrumented.localName}`;
  }
  if (original.nodeValue !== instrumented.nodeValue) {
    return `node value ${JSON.stringify(original.nodeValue)} became ${JSON.stringify(instrumented.nodeValue)}`;
  }
  for (const field of ['publicId', 'systemId'] as const) {
    if (original[field] !== instrumented[field]) {
      return `doctype ${field} ${JSON.stringify(original[field])} became ${JSON.stringify(instrumented[field])}`;
    }
  }
  if (original.shadowRootType !== instrumented.shadowRootType) {
    return `shadow root type ${original.shadowRootType} became ${instrumented.shadowRootType}`;
  }
  const left = attributePairs(original);
  const right = attributePairs(instrumented).filter(([name]) => name !== SOURCE_ATTRIBUTE);
  const same =
    left.length === right.length &&
    left.every(([name, value], index) => right[index]?.[0] === name && right[index]?.[1] === value);
  return same ? undefined : `attributes ${describeAttributes(left)} became ${describeAttributes(right)}`;
}

/**
 * Compares the DOM Chromium built from a page with the DOM it built from the instrumented page,
 * and returns the first difference in document order. Node types, names, values, doctype
 * identifiers, shadow root types, attributes (in order, with the source attribute left out of the
 * instrumented side only) and the order and number of shadow roots, template content and children
 * are compared; protocol ids are not.
 */
export function compareDocuments(original: ProtocolNode, instrumented: ProtocolNode): DomComparison {
  // The walk keeps its own stack, because a page puts no limit on nesting depth.
  const pending: Pair[] = [{ original, instrumented, path: '#document' }];
  for (let pair = pending.pop(); pair !== undefined; pair = pending.pop()) {
    const { original: left, instrumented: right, path } = pair;
    if (left === undefined || right === undefined) {
      return { equal: false, path, reason: left === undefined ? 'the node is added' : 'the node is removed' };
    }
    const reason = compareNodes(left, right);
    if (reason !== undefined) {
      return { equal: false, path, reason };
    }
    for (const child of childPairs(left, right, path === '#document' ? '' : path)) {
      pending.push(child);
    }
  }
  return { equal: true };
}
