import type { AxForced, AxNode, Diagnostic, DomNode } from '@spokenpath/snapshot-types';
import type { CDPSession } from 'playwright-core';
import { CaptureError } from './error.ts';
import { bounded } from './session.ts';

export interface ForcedTrees {
  axForced: AxForced[];
  diagnostics: Diagnostic[];
}

/**
 * Calls `Accessibility.getPartialAXTree` (without relatives) for every element in `dom` that has
 * no node in the full tree, in `dom` order, and keeps each response's nodes as delivered
 * (ADR-0008). A call the browser refuses becomes a `partial-ax-tree-failed` diagnostic on the
 * element rather than failing the capture; a call that times out still fails it, because the
 * browser has stopped answering.
 */
export async function forcedAxTrees(
  cdp: Pick<CDPSession, 'send'>,
  dom: readonly DomNode[],
  ax: readonly AxNode[],
  timeoutMs: number,
): Promise<ForcedTrees> {
  const inFullTree = new Set(ax.map((node) => node.backendDOMNodeId));
  const axForced: AxForced[] = [];
  const diagnostics: Diagnostic[] = [];
  for (const node of dom) {
    if (node.kind !== 'element' || inFullTree.has(node.backend_id)) {
      continue;
    }
    try {
      const partial = await bounded('Accessibility.getPartialAXTree', timeoutMs, (options) =>
        cdp.send(
          'Accessibility.getPartialAXTree',
          { backendNodeId: node.backend_id, fetchRelatives: false },
          options,
        ),
      );
      // The protocol's AXNode declares no index signature, so TypeScript cannot relate it to the
      // schema's open AxNode; the nodes are stored as delivered and validated with the snapshot.
      axForced.push({
        node: node.id,
        backend_id: node.backend_id,
        nodes: partial.nodes as unknown as AxNode[],
      });
    } catch (error) {
      if (error instanceof CaptureError) {
        throw error;
      }
      const reason = error instanceof Error ? error.message : String(error);
      diagnostics.push({
        code: 'partial-ax-tree-failed',
        message: `Accessibility.getPartialAXTree failed for this element: ${reason}`,
        node: node.id,
      });
    }
  }
  return { axForced, diagnostics };
}
