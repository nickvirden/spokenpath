import type { Token } from 'parse5';
import { SOURCE_ATTRIBUTE } from './source-attribute.ts';
import { type ChildNode, type Document, isElement, isTemplate, type ParentNode } from './tree.ts';

export type TreeComparison =
  | { equal: true }
  | {
      equal: false;
      /** The first differing node, such as `html > body > table[0] > #text[1]`. */
      path: string;
      /** What differs there. */
      reason: string;
    };

export interface CompareOptions {
  /**
   * The class token added to each tagged element, by its source attribute value. When given, the
   * one change that adds the token to an element's `class` is ignored too: a new `class` whose
   * value is exactly the token, or a `class` whose value is the original value, one space and the
   * token.
   */
  classTokens?: ReadonlyMap<string, string>;
}

/** A pair of nodes at the same place in both trees; one side is missing when the counts differ. */
interface Pair {
  original: ChildNode | undefined;
  instrumented: ChildNode | undefined;
  path: string;
}

/**
 * Compares the tree parsed from a page with the tree parsed from its instrumented text, and
 * returns the first difference in document order. The source attribute is ignored on every
 * element of the instrumented tree; everything else that shapes the DOM a browser builds is
 * compared: the document mode, the doctype, node types, element namespaces, names and attributes
 * (name, namespace, prefix and value, in order), text and comment data, the order and number of
 * children, and `template` contents. With `classTokens`, the class token added to each tagged
 * element is ignored as well, and any other change to its `class` is a difference.
 */
export function compareTrees(
  original: Document,
  instrumented: Document,
  options: CompareOptions = {},
): TreeComparison {
  if (original.mode !== instrumented.mode) {
    return {
      equal: false,
      path: '#document',
      reason: `document mode ${original.mode} became ${instrumented.mode}`,
    };
  }
  // The walk keeps its own stack, because the parser puts no limit on nesting depth.
  const pending = childPairs(original, instrumented, '');
  for (let pair = pending.pop(); pair !== undefined; pair = pending.pop()) {
    const { original: left, instrumented: right, path } = pair;
    if (left === undefined || right === undefined) {
      return { equal: false, path, reason: left === undefined ? 'the node is added' : 'the node is removed' };
    }
    const reason = compareNodes(left, right, options);
    if (reason !== undefined) {
      return { equal: false, path, reason };
    }
    if (isElement(left) && isElement(right)) {
      const [leftParent, rightParent, parentPath]: [ParentNode, ParentNode, string] =
        isTemplate(left) && isTemplate(right)
          ? [left.content, right.content, `${path} > #content`]
          : [left, right, path];
      for (const child of childPairs(leftParent, rightParent, parentPath)) {
        pending.push(child);
      }
    }
  }
  return { equal: true };
}

/**
 * The pairs of children of two parents, last child first, so that popping them off a stack visits
 * them in document order. Each child is named by its node name and its index among the siblings of
 * that name.
 */
function childPairs(original: ParentNode, instrumented: ParentNode, parentPath: string): Pair[] {
  const ordinals = new Map<string, number>();
  const pairs: Pair[] = [];
  const count = Math.max(original.childNodes.length, instrumented.childNodes.length);
  for (let index = 0; index < count; index += 1) {
    const left = original.childNodes[index];
    const right = instrumented.childNodes[index];
    const name = (left ?? right)?.nodeName ?? '';
    const ordinal = ordinals.get(name) ?? 0;
    ordinals.set(name, ordinal + 1);
    const path = `${parentPath === '' ? '' : `${parentPath} > `}${name}[${ordinal}]`;
    pairs.push({ original: left, instrumented: right, path });
  }
  return pairs.reverse();
}

/** What differs between two nodes, ignoring their children, or nothing. */
function compareNodes(left: ChildNode, right: ChildNode, options: CompareOptions): string | undefined {
  if (left.nodeName !== right.nodeName) {
    return `${left.nodeName} became ${right.nodeName}`;
  }
  if ('data' in left && 'data' in right && left.data !== right.data) {
    return `comment ${JSON.stringify(left.data)} became ${JSON.stringify(right.data)}`;
  }
  if ('value' in left && 'value' in right && left.value !== right.value) {
    return `text ${JSON.stringify(left.value)} became ${JSON.stringify(right.value)}`;
  }
  if ('publicId' in left && 'publicId' in right) {
    for (const field of ['name', 'publicId', 'systemId'] as const) {
      if (left[field] !== right[field]) {
        return `doctype ${field} ${JSON.stringify(left[field])} became ${JSON.stringify(right[field])}`;
      }
    }
  }
  if (isElement(left) && isElement(right)) {
    if (left.namespaceURI !== right.namespaceURI) {
      return `namespace ${left.namespaceURI} became ${right.namespaceURI}`;
    }
    if (left.tagName !== right.tagName) {
      return `tag ${left.tagName} became ${right.tagName}`;
    }
    const token = options.classTokens?.get(
      right.attrs.find((attr) => attr.name === SOURCE_ATTRIBUTE)?.value ?? '',
    );
    const rightAttrs = right.attrs.filter((attr) => attr.name !== SOURCE_ATTRIBUTE);
    return compareAttributes(
      left.attrs,
      token === undefined ? rightAttrs : withoutToken(left.attrs, rightAttrs, token),
    );
  }
  return undefined;
}

/**
 * The instrumented element's attributes with the class token's one change undone, when the
 * change is exactly the expected one; otherwise the attributes as they are, so the comparison
 * reports the difference.
 */
function withoutToken(
  original: Token.Attribute[],
  instrumented: Token.Attribute[],
  token: string,
): Token.Attribute[] {
  const isClass = (attr: Token.Attribute) => attr.name === 'class' && attr.namespace === undefined;
  const originalClass = original.find(isClass);
  const index = instrumented.findIndex(isClass);
  const instrumentedClass = instrumented[index];
  if (instrumentedClass === undefined) {
    return instrumented;
  }
  if (originalClass === undefined) {
    return instrumentedClass.value === token ? instrumented.toSpliced(index, 1) : instrumented;
  }
  return instrumentedClass.value === `${originalClass.value} ${token}`
    ? instrumented.with(index, { ...instrumentedClass, value: originalClass.value })
    : instrumented;
}

function compareAttributes(left: Token.Attribute[], right: Token.Attribute[]): string | undefined {
  const describe = (attrs: Token.Attribute[]) =>
    JSON.stringify(attrs.map(({ name, value, namespace, prefix }) => ({ name, value, namespace, prefix })));
  if (left.length !== right.length) {
    return `attributes ${describe(left)} became ${describe(right)}`;
  }
  for (const [index, attr] of left.entries()) {
    const other = right[index];
    if (
      other === undefined ||
      attr.name !== other.name ||
      attr.value !== other.value ||
      attr.namespace !== other.namespace ||
      attr.prefix !== other.prefix
    ) {
      return `attributes ${describe(left)} became ${describe(right)}`;
    }
  }
  return undefined;
}
