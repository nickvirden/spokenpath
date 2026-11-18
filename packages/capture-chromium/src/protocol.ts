/**
 * The fields of the DevTools Protocol's `DOM.Node` that capture reads. playwright-core types its
 * CDP session with the full protocol, but does not export the `Protocol` namespace, so the subset
 * is declared here; the protocol's own node type is assignable to it.
 */
export interface ProtocolNode {
  backendNodeId: number;
  nodeType: number;
  nodeName: string;
  localName: string;
  nodeValue: string;
  children?: ProtocolNode[];
  /** Attribute names and values, interleaved: `[name1, value1, name2, value2, ...]`. */
  attributes?: string[];
  publicId?: string;
  systemId?: string;
  shadowRootType?: 'user-agent' | 'open' | 'closed';
  shadowRoots?: ProtocolNode[];
  templateContent?: ProtocolNode;
  contentDocument?: ProtocolNode;
}

/** The protocol's `nodeType` values, as the DOM Standard numbers them. */
export const NodeType = {
  element: 1,
  text: 3,
  cdataSection: 4,
  processingInstruction: 7,
  comment: 8,
  document: 9,
  doctype: 10,
  documentFragment: 11,
} as const;

/** `[name, value]` pairs from the protocol's interleaved attribute list. */
export function attributePairs(node: ProtocolNode): [name: string, value: string][] {
  const flat = node.attributes ?? [];
  const pairs: [string, string][] = [];
  for (let index = 0; index + 1 < flat.length; index += 2) {
    pairs.push([flat[index] ?? '', flat[index + 1] ?? '']);
  }
  return pairs;
}

/**
 * The node's children in the order capture records them: its shadow roots in protocol order, then
 * its template content, then its light-tree children. Pseudo-elements and frame documents are left
 * out, because the snapshot has no kind for the first and capture does not follow the second.
 */
export function recordedChildren(node: ProtocolNode): ProtocolNode[] {
  return [
    ...(node.shadowRoots ?? []),
    ...(node.templateContent === undefined ? [] : [node.templateContent]),
    ...(node.children ?? []),
  ];
}
