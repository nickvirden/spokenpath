import type { Snapshot } from '@spokenpath/snapshot-types';

/** The value every `volatile.captured_at` is set to. */
export const NORMALISED_CAPTURED_AT = '1970-01-01T00:00:00Z';
/** The value every `producer.version` is set to. */
export const NORMALISED_PRODUCER_VERSION = '0.0.0';
/** The value every frame id is set to. */
export const NORMALISED_FRAME_ID = 'frame';

/** The keys whose values are backend node ids, alone or as arrays, wherever they appear. */
const BACKEND_ID_KEYS: ReadonlySet<string> = new Set(['backendNodeId', 'backend_id', 'backendDOMNodeId']);

/** The DOM snapshot's per-node rectangle lists, each a list of `[x, y, width, height]`. */
const LAYOUT_RECT_KEYS = ['bounds', 'offsetRects', 'scrollRects', 'clientRects'] as const;

/** The DOM snapshot's per-document sizes and scroll offsets. */
const DOCUMENT_GEOMETRY_KEYS = ['contentWidth', 'contentHeight', 'scrollOffsetX', 'scrollOffsetY'] as const;

type JsonObject = Record<string, unknown>;

function isObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Every object inside `value`, `value` included, parents before children, in key order. */
function* objectsIn(value: unknown): Generator<JsonObject> {
  const pending: unknown[] = [value];
  for (let item = pending.pop(); item !== undefined; item = pending.pop()) {
    const children = Array.isArray(item) ? item : isObject(item) ? Object.values(item) : [];
    if (isObject(item)) {
      yield item;
    }
    for (const child of children.toReversed()) {
      pending.push(child);
    }
  }
}

/** Renumbers backend ids from 1 in order of first appearance, the same id everywhere. */
class BackendIds {
  readonly #ids = new Map<number, number>();

  #renumber(value: unknown): unknown {
    if (typeof value !== 'number') {
      return value;
    }
    let id = this.#ids.get(value);
    if (id === undefined) {
      id = this.#ids.size + 1;
      this.#ids.set(value, id);
    }
    return id;
  }

  renumberIn(value: unknown): void {
    for (const object of objectsIn(value)) {
      for (const key of Object.keys(object)) {
        if (!BACKEND_ID_KEYS.has(key)) {
          continue;
        }
        const current = object[key];
        object[key] = Array.isArray(current)
          ? current.map((item) => this.#renumber(item))
          : this.#renumber(current);
      }
    }
  }
}

/**
 * Renumbers accessibility node ids (`nodeId`, `parentId` and `childIds`) from 1 in order of first
 * appearance, the same id everywhere. Chromium assigns them from process-wide counters; the
 * `InlineTextBox` ids in particular come out differently on different hosts.
 */
class AxNodeIds {
  readonly #ids = new Map<string, string>();

  #renumber(value: unknown): unknown {
    if (typeof value !== 'string') {
      return value;
    }
    let id = this.#ids.get(value);
    if (id === undefined) {
      id = String(this.#ids.size + 1);
      this.#ids.set(value, id);
    }
    return id;
  }

  renumberIn(nodes: readonly JsonObject[]): void {
    for (const node of nodes) {
      node.nodeId = this.#renumber(node.nodeId);
      if ('parentId' in node) {
        node.parentId = this.#renumber(node.parentId);
      }
      if (Array.isArray(node.childIds)) {
        node.childIds = node.childIds.map((id) => this.#renumber(id));
      }
    }
  }
}

/** Sets every number inside `value` to 0, keeping the shape of the arrays around it. */
function zeroed(value: unknown): unknown {
  if (typeof value === 'number') {
    return 0;
  }
  return Array.isArray(value) ? value.map(zeroed) : value;
}

/** Frame ids, geometry and sizes in the raw `DOMSnapshot.captureSnapshot` response. */
function normaliseDomSnapshot(domSnapshot: JsonObject): void {
  const strings = domSnapshot.strings;
  const documents = Array.isArray(domSnapshot.documents) ? domSnapshot.documents : [];
  for (const document of documents) {
    if (!isObject(document)) {
      continue;
    }
    // The document's frameId is an index into the shared string table.
    if (typeof document.frameId === 'number' && Array.isArray(strings) && document.frameId < strings.length) {
      strings[document.frameId] = NORMALISED_FRAME_ID;
    }
    for (const key of DOCUMENT_GEOMETRY_KEYS) {
      if (key in document) {
        document[key] = 0;
      }
    }
    const { layout, textBoxes } = document;
    if (isObject(layout)) {
      for (const key of LAYOUT_RECT_KEYS) {
        if (key in layout) {
          layout[key] = zeroed(layout[key]);
        }
      }
    }
    if (isObject(textBoxes) && 'bounds' in textBoxes) {
      textBoxes.bounds = zeroed(textBoxes.bounds);
    }
  }
}

/**
 * A copy of `snapshot` with every value that differs between runs or hosts of the same capture
 * set to a fixed value, so a golden comparison sees only what the browser exposes:
 *
 * - `volatile.captured_at` and `producer.version`, which change with every run and release;
 * - backend node ids, renumbered by first appearance in `dom`, then `ax`, then `ax_forced`, then
 *   `focus`, then `dom_snapshot`, because the browser assigns them at run time;
 * - accessibility node ids, renumbered by first appearance in `ax`, then `ax_forced`, for the same
 *   reason;
 * - frame ids, which are random per launch;
 * - the DOM snapshot's coordinates, sizes and scroll offsets, which depend on the host's fonts.
 */
export function normaliseForGolden(snapshot: Snapshot): Snapshot {
  const copy = structuredClone(snapshot);
  copy.volatile.captured_at = NORMALISED_CAPTURED_AT;
  copy.producer.version = NORMALISED_PRODUCER_VERSION;
  const { payload } = copy;
  if (payload.engine !== 'chromium') {
    return copy;
  }
  const backendIds = new BackendIds();
  for (const section of [payload.dom, payload.ax, payload.ax_forced, payload.focus, payload.dom_snapshot]) {
    backendIds.renumberIn(section);
  }
  const axNodeIds = new AxNodeIds();
  axNodeIds.renumberIn(payload.ax);
  for (const forced of payload.ax_forced) {
    axNodeIds.renumberIn(forced.nodes);
  }
  for (const object of objectsIn([payload.ax, payload.ax_forced])) {
    if (typeof object.frameId === 'string') {
      object.frameId = NORMALISED_FRAME_ID;
    }
  }
  if (payload.dom_snapshot !== undefined) {
    normaliseDomSnapshot(payload.dom_snapshot);
  }
  return copy;
}
