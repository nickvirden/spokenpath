import type { DefaultTreeAdapterTypes } from 'parse5';

export type Document = DefaultTreeAdapterTypes.Document;
export type Element = DefaultTreeAdapterTypes.Element;
export type ParentNode = DefaultTreeAdapterTypes.ParentNode;
export type ChildNode = DefaultTreeAdapterTypes.ChildNode;
export type Template = DefaultTreeAdapterTypes.Template;

export interface WalkOptions {
  /** Whether to descend into `template` contents, which parse5 keeps outside `childNodes`. */
  templates: boolean;
}

export function isElement(node: ChildNode): node is Element {
  return 'tagName' in node;
}

export function isTemplate(node: Element): node is Template {
  return 'content' in node;
}

/**
 * Every element under `root`, in tree order. The walk keeps its own stack, because the parser puts
 * no limit on nesting depth and a recursive walk would overflow the call stack on deep pages.
 */
export function* elements(root: ParentNode, options: WalkOptions): Generator<Element> {
  const pending: ChildNode[] = root.childNodes.toReversed();
  for (let node = pending.pop(); node !== undefined; node = pending.pop()) {
    if (!isElement(node)) {
      continue;
    }
    yield node;
    const children = options.templates && isTemplate(node) ? node.content.childNodes : node.childNodes;
    for (const child of children.toReversed()) {
      pending.push(child);
    }
  }
}
