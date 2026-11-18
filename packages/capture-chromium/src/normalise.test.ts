import { readFileSync } from 'node:fs';
import { type Snapshot, validateSnapshot } from '@spokenpath/snapshot-types';
import { expect, test } from 'vitest';
import { chromiumPayload } from './fixture-pages.ts';
import { NORMALISED_FRAME_ID, normaliseForGolden } from './normalise.ts';

/** The hand-written Chromium example, with its backend ids shifted to stand for another run. */
function shiftedExample(offset: number): Snapshot {
  const text = readFileSync(
    new URL('../../../fixtures/snapshots/chromium-basic.json', import.meta.url),
    'utf8',
  );
  const shifted = text.replace(
    /("(?:backend_id|backendDOMNodeId)": )(\d+)/g,
    (_match, key: string, id: string) => `${key}${Number(id) + offset}`,
  );
  const snapshot = JSON.parse(shifted) as Snapshot;
  const documents = chromiumPayload(snapshot).dom_snapshot?.documents as {
    nodes: { backendNodeId: number[] };
  }[];
  for (const document of documents) {
    document.nodes.backendNodeId = document.nodes.backendNodeId.map((id) => id + offset);
  }
  return snapshot;
}

test('two runs that differ only in backend ids normalise to the same value', () => {
  expect(normaliseForGolden(shiftedExample(1000))).toEqual(normaliseForGolden(shiftedExample(0)));
});

test('backend ids are renumbered by first appearance, consistently across sections', () => {
  const payload = chromiumPayload(normaliseForGolden(shiftedExample(500)));
  expect(payload.dom.map((node) => node.backend_id)).toEqual(payload.dom.map((_node, index) => index + 1));
  for (const node of payload.ax) {
    if (node.backendDOMNodeId !== undefined) {
      expect(payload.dom.some((domNode) => domNode.backend_id === node.backendDOMNodeId)).toBe(true);
    }
  }
  for (const step of payload.focus.steps) {
    if (step.node !== undefined) {
      expect(step.backend_id).toBe(payload.dom[step.node - 1]?.backend_id);
    }
  }
});

/** The `[before, after]` pairs of every AX node id reference, in `ax` and then `ax_forced`. */
function axIdPairs(before: Snapshot, after: Snapshot): [string | undefined, string | undefined][] {
  const nodes = (snapshot: Snapshot) => {
    const payload = chromiumPayload(snapshot);
    return [...payload.ax, ...payload.ax_forced.flatMap((forced) => forced.nodes)];
  };
  const afterNodes = nodes(after);
  return nodes(before).flatMap((node, index) => {
    const other = afterNodes[index];
    const pairs: [string | undefined, string | undefined][] = [
      [node.nodeId, other?.nodeId],
      [node.parentId, other?.parentId],
    ];
    for (const [child, id] of (node.childIds ?? []).entries()) {
      pairs.push([id, other?.childIds?.[child]]);
    }
    return pairs;
  });
}

test('accessibility node ids that differ between hosts normalise to the same value', () => {
  const relabelled = shiftedExample(0);
  const relabel = (id: string) => `-100000${id}`;
  for (const node of [
    ...chromiumPayload(relabelled).ax,
    ...chromiumPayload(relabelled).ax_forced.flatMap((forced) => forced.nodes),
  ]) {
    node.nodeId = relabel(node.nodeId);
    if (node.parentId !== undefined) node.parentId = relabel(node.parentId);
    if (node.childIds !== undefined) node.childIds = node.childIds.map(relabel);
  }
  expect(normaliseForGolden(relabelled)).toEqual(normaliseForGolden(shiftedExample(0)));
});

test('accessibility node ids are renamed one to one, so every reference still holds', () => {
  const original = shiftedExample(0);
  const pairs = axIdPairs(original, normaliseForGolden(original));
  const forward = new Map<string | undefined, string | undefined>();
  const backward = new Map<string | undefined, string | undefined>();
  for (const [before, after] of pairs) {
    expect(forward.get(before) ?? after).toBe(after);
    expect(backward.get(after) ?? before).toBe(before);
    forward.set(before, after);
    backward.set(after, before);
  }
  expect(forward.size).toBeGreaterThan(1);
  expect([...forward.values()]).toContain('1');
});

test('volatile, producer, frame ids and geometry are fixed, and the result still validates', () => {
  const original = shiftedExample(0);
  const normalised = normaliseForGolden(original);
  expect(normalised.volatile.captured_at).toBe('1970-01-01T00:00:00Z');
  expect(normalised.producer.version).toBe('0.0.0');
  expect(chromiumPayload(normalised).ax[0]?.frameId).toBe(NORMALISED_FRAME_ID);
  const result = validateSnapshot(normalised);
  expect(result.ok ? [] : result.errors).toEqual([]);
  expect(original.volatile.captured_at).toBe('2026-11-17T12:00:00Z');
});

test('the DOM snapshot keeps its array shapes with every coordinate set to 0', () => {
  const snapshot = shiftedExample(0);
  const domSnapshot = chromiumPayload(snapshot).dom_snapshot as {
    documents: Record<string, unknown>[];
    strings: string[];
  };
  const [document] = domSnapshot.documents;
  if (document === undefined) {
    throw new Error('the example has no DOM snapshot document');
  }
  domSnapshot.strings.push('ABCDEF0123');
  Object.assign(document, {
    frameId: domSnapshot.strings.length - 1,
    contentWidth: 1280,
    scrollOffsetY: 12.5,
    layout: { ...(document.layout as object), bounds: [[8, 8, 100, 20.5], []], clientRects: [[1, 2, 3, 4]] },
    textBoxes: { layoutIndex: [0], bounds: [[8, 9, 10, 11]], start: [0], length: [4] },
  });
  const normalised = chromiumPayload(normaliseForGolden(snapshot)).dom_snapshot as typeof domSnapshot;
  const normalisedDocument = normalised.documents[0];
  expect(normalised.strings.at(-1)).toBe(NORMALISED_FRAME_ID);
  expect(normalisedDocument).toMatchObject({
    contentWidth: 0,
    scrollOffsetY: 0,
    layout: { bounds: [[0, 0, 0, 0], []], clientRects: [[0, 0, 0, 0]] },
    textBoxes: { bounds: [[0, 0, 0, 0]], start: [0], length: [4] },
  });
});
