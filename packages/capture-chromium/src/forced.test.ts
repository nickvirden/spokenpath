import type { AxNode, DomNode } from '@spokenpath/snapshot-types';
import { errors } from 'playwright-core';
import { expect, test } from 'vitest';
import { forcedAxTrees } from './forced.ts';

const dom: DomNode[] = [
  { id: 0, backend_id: 1, children: [1, 2], kind: 'document', attributes: [] },
  { id: 1, backend_id: 2, parent: 0, children: [], kind: 'element', tag: 'head', attributes: [] },
  { id: 2, backend_id: 3, parent: 0, children: [], kind: 'element', tag: 'p', attributes: [] },
];
const ax: AxNode[] = [{ nodeId: '1', ignored: false, backendDOMNodeId: 3 }];

/** A CDP session whose `send` answers with `answer`, recording each call's parameters. */
function session(answer: (params: unknown) => Promise<unknown>) {
  const calls: unknown[] = [];
  const send = (_method: string, params?: unknown) => {
    calls.push(params);
    return answer(params);
  };
  return { cdp: { send } as unknown as Parameters<typeof forcedAxTrees>[0], calls };
}

test('queries only the elements missing from the full tree and keeps the nodes as delivered', async () => {
  const delivered = [{ nodeId: '9', ignored: true, futureField: 1 }];
  const { cdp, calls } = session(async () => ({ nodes: delivered }));
  const result = await forcedAxTrees(cdp, dom, ax, 1_000);
  expect(calls).toEqual([{ backendNodeId: 2, fetchRelatives: false }]);
  expect(result).toEqual({ axForced: [{ node: 1, backend_id: 2, nodes: delivered }], diagnostics: [] });
});

test('a refused call becomes a partial-ax-tree-failed diagnostic on the element', async () => {
  const { cdp } = session(async () => {
    throw new Error('Protocol error (Accessibility.getPartialAXTree): No node with given id found');
  });
  const result = await forcedAxTrees(cdp, dom, ax, 1_000);
  expect(result.axForced).toEqual([]);
  expect(result.diagnostics).toEqual([
    expect.objectContaining({
      code: 'partial-ax-tree-failed',
      node: 1,
      message: expect.stringContaining('No node'),
    }),
  ]);
});

test('a call that times out fails the capture instead', async () => {
  const { cdp } = session(async () => {
    throw new errors.TimeoutError('cdpSession.send: Timeout 1000ms exceeded.');
  });
  await expect(forcedAxTrees(cdp, dom, ax, 1_000)).rejects.toMatchObject({ code: 'timeout' });
});
