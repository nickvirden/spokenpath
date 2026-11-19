import type { SafariAxNode, Snapshot } from '@spokenpath/snapshot-types';

/** A span by its id and the start tag it locates. */
export interface SpanRef {
  span: number;
  /** The element name of the start tag, as written. */
  tag: string;
  line: number;
  column: number;
}

/** Why a Safari node carries no span. */
export type UnmappedNodeReason = 'web-area' | 'text' | 'unknown-source-token' | 'no-source-token';

export interface PageMapping {
  /** Every span of the page. */
  tagged: number;
  /** The spans at least one Safari node carries. */
  mapped: number;
  /** The spans more than one Safari node carries, with those nodes. */
  mapped_to_several: (SpanRef & { nodes: number[] })[];
  /** The spans no Safari node carries and Chromium does not expose either, with why. */
  not_in_tree: (SpanRef & { reason: string })[];
  /** The spans no Safari node carries although Chromium exposes them. */
  unmapped_in_tree: (SpanRef & { reason: string })[];
  /** `mapped / (mapped + unmapped_in_tree)`. */
  figure: number;
  /** Every Safari node without a span, with its role and why. */
  unmapped_nodes: { id: number; role?: string; subrole?: string; reason: UnmappedNodeReason }[];
}

export interface MappingReport {
  /** By page, relative to `fixtures/pages/`. */
  pages: Record<string, PageMapping>;
  /** The counts over every page, and the figure over them. */
  total: {
    tagged: number;
    mapped: number;
    mapped_to_several: number;
    not_in_tree: number;
    unmapped_in_tree: number;
    figure: number;
    unmapped_nodes: number;
  };
}

/** The reason a span no Safari node carries is counted against the figure. */
export const EXPOSED_IN_CHROMIUM = 'exposed in Chromium; no Safari node carries its token';

const SOURCE_TOKEN = /^sp-src-\d+-\d+$/;

function stringAttribute(node: SafariAxNode, name: string): string | undefined {
  const value = node.attributes[name];
  return value?.type === 'string' ? value.value : undefined;
}

function hasSourceToken(node: SafariAxNode): boolean {
  const classList = node.attributes.AXDOMClassList;
  return (
    classList?.type === 'array' &&
    classList.items.some((item) => item.type === 'string' && SOURCE_TOKEN.test(item.value))
  );
}

function unmappedReason(node: SafariAxNode, role: string | undefined): UnmappedNodeReason {
  if (node.id === 0 || role === 'AXWebArea') {
    return 'web-area';
  }
  if (role === 'AXStaticText') {
    return 'text';
  }
  return hasSourceToken(node) ? 'unknown-source-token' : 'no-source-token';
}

/**
 * How Chromium's golden of the same page treats each span: `exposed` when a DOM element with
 * that span has a non-ignored node in `payload.ax`, otherwise why not.
 */
function chromiumTreatment(chromium: Snapshot): Map<number, 'exposed' | string> {
  if (chromium.payload.engine !== 'chromium') {
    throw new Error(`the Chromium golden's payload is for ${chromium.payload.engine}`);
  }
  const { dom, ax } = chromium.payload;
  const axByBackend = new Map<number, typeof ax>();
  for (const node of ax) {
    if (node.backendDOMNodeId !== undefined) {
      axByBackend.set(node.backendDOMNodeId, [...(axByBackend.get(node.backendDOMNodeId) ?? []), node]);
    }
  }
  const treatment = new Map<number, string>();
  for (const span of chromium.spans) {
    const nodes = dom
      .filter((node) => node.span === span.id)
      .flatMap((node) => axByBackend.get(node.backend_id) ?? []);
    if (nodes.some((node) => !node.ignored)) {
      treatment.set(span.id, 'exposed');
    } else if (nodes.length > 0) {
      const reasons = [
        ...new Set(nodes.flatMap((node) => (node.ignoredReasons ?? []).map((reason) => reason.name))),
      ];
      treatment.set(span.id, `ignored in Chromium: ${reasons.join(', ')}`);
    } else {
      treatment.set(span.id, 'no Chromium node');
    }
  }
  return treatment;
}

/** One page's mapping, from its `safari-ax` and Chromium goldens and its bytes. */
export function pageMapping(safari: Snapshot, chromium: Snapshot, page: Uint8Array): PageMapping {
  if (safari.payload.engine !== 'safari-ax') {
    throw new Error(`the Safari AX golden's payload is for ${safari.payload.engine}`);
  }
  const nodes = safari.payload.nodes ?? [];
  const bytes = Buffer.from(page);
  const ref = (id: number): SpanRef => {
    const span = safari.spans[id];
    if (span === undefined || span.id !== id) {
      throw new Error(`the Safari AX golden has no span ${id} at index ${id}`);
    }
    const startTag = bytes.subarray(span.start_tag.start, span.start_tag.end).toString('utf8');
    return {
      span: id,
      tag: /^<([^\t\n\f\r />]+)/.exec(startTag)?.[1] ?? '',
      line: span.line,
      column: span.column,
    };
  };
  const carriers = new Map<number, number[]>();
  for (const node of nodes) {
    if (node.span !== undefined) {
      carriers.set(node.span, [...(carriers.get(node.span) ?? []), node.id]);
    }
  }
  const treatment = chromiumTreatment(chromium);
  const notInTree: PageMapping['not_in_tree'] = [];
  const unmappedInTree: PageMapping['unmapped_in_tree'] = [];
  for (const span of safari.spans) {
    if (carriers.has(span.id)) {
      continue;
    }
    const chromiumSays = treatment.get(span.id) ?? 'no Chromium node';
    if (chromiumSays === 'exposed') {
      unmappedInTree.push({ ...ref(span.id), reason: EXPOSED_IN_CHROMIUM });
    } else {
      notInTree.push({ ...ref(span.id), reason: chromiumSays });
    }
  }
  const mapped = carriers.size;
  return {
    tagged: safari.spans.length,
    mapped,
    mapped_to_several: [...carriers.entries()]
      .filter(([, ids]) => ids.length > 1)
      .sort(([left], [right]) => left - right)
      .map(([span, ids]) => ({ ...ref(span), nodes: ids })),
    not_in_tree: notInTree,
    unmapped_in_tree: unmappedInTree,
    figure: ratio(mapped, unmappedInTree.length),
    unmapped_nodes: nodes
      .filter((node) => node.span === undefined)
      .map((node) => {
        const role = stringAttribute(node, 'AXRole');
        const subrole = stringAttribute(node, 'AXSubrole');
        return {
          id: node.id,
          ...(role === undefined ? {} : { role }),
          ...(subrole === undefined ? {} : { subrole }),
          reason: unmappedReason(node, role),
        };
      }),
  };
}

/** `mapped / (mapped + unmapped)`, which is 1 when there is nothing to map. */
function ratio(mapped: number, unmapped: number): number {
  return mapped + unmapped === 0 ? 1 : mapped / (mapped + unmapped);
}

/** The report over every page's mapping. */
export function mappingReport(pages: Record<string, PageMapping>): MappingReport {
  const all = Object.values(pages);
  const sum = (count: (page: PageMapping) => number) => all.reduce((total, page) => total + count(page), 0);
  const mapped = sum((page) => page.mapped);
  const unmapped = sum((page) => page.unmapped_in_tree.length);
  return {
    pages,
    total: {
      tagged: sum((page) => page.tagged),
      mapped,
      mapped_to_several: sum((page) => page.mapped_to_several.length),
      not_in_tree: sum((page) => page.not_in_tree.length),
      unmapped_in_tree: unmapped,
      figure: ratio(mapped, unmapped),
      unmapped_nodes: sum((page) => page.unmapped_nodes.length),
    },
  };
}
