import type { Diagnostic, DomNode, Focus, FocusStep } from '@spokenpath/snapshot-types';
import type { CDPSession, Page } from 'playwright-core';
import { CaptureError } from './error.ts';
import { bounded } from './session.ts';

/** The most Tab presses a walk makes before it stops and reports itself truncated. */
export const MAX_FOCUS_STEPS = 200;

/**
 * The element the page reports as focused, descending through each open shadow root's own active
 * element, or `null` when the document does not have focus or only `body` is active. Evaluated as a
 * protocol expression so the result is a remote object that `DOM.describeNode` can resolve. It
 * stands in only when the accessibility tree has no focused node: focus inside a frame, whose tree
 * this walk does not read, or on an element that has no accessibility node.
 */
const ACTIVE_ELEMENT = `(() => {
  if (!document.hasFocus()) return null;
  let element = document.activeElement;
  while (element !== null && element.shadowRoot !== null && element.shadowRoot.activeElement !== null) {
    element = element.shadowRoot.activeElement;
  }
  return element === null || element === document.body ? null : element;
})()`;

/**
 * Where focus is after a press. `accessibility` is the DOM node of the focused accessibility node,
 * which tells apart the stops inside one element: the fields of a date input, or the controls in a
 * closed shadow root. `active-element` is the fallback, which shows only the element that contains
 * focus.
 */
interface FocusPosition {
  backendId: number;
  source: 'accessibility' | 'active-element';
}

/** Where focus is, or nothing when it is outside the document or only on `body`. */
async function focusPosition(
  cdp: CDPSession,
  dom: readonly DomNode[],
  idByBackendId: ReadonlyMap<number, number>,
  timeoutMs: number,
): Promise<FocusPosition | undefined> {
  const { nodes } = await bounded('Accessibility.getFullAXTree', timeoutMs, (options) =>
    cdp.send('Accessibility.getFullAXTree', {}, options),
  );
  // The root node is focused whenever the document has focus, so it says nothing about where.
  const focused = nodes.find(
    (node) =>
      node.backendDOMNodeId !== undefined &&
      idByBackendId.get(node.backendDOMNodeId) !== 0 &&
      (node.properties ?? []).some(
        (property) => property.name === 'focused' && property.value.value === true,
      ),
  );
  if (focused?.backendDOMNodeId !== undefined) {
    const id = idByBackendId.get(focused.backendDOMNodeId);
    if (id !== undefined && dom[id]?.tag === 'body') {
      return undefined;
    }
    return { backendId: focused.backendDOMNodeId, source: 'accessibility' };
  }
  const backendId = await activeElementBackendId(cdp, timeoutMs);
  return backendId === undefined ? undefined : { backendId, source: 'active-element' };
}

/** The `backendNodeId` of the active element, or nothing when focus is outside the document. */
async function activeElementBackendId(cdp: CDPSession, timeoutMs: number): Promise<number | undefined> {
  const evaluated = await bounded('reading the focused element', timeoutMs, (options) =>
    cdp.send('Runtime.evaluate', { expression: ACTIVE_ELEMENT, returnByValue: false }, options),
  );
  if (evaluated.exceptionDetails !== undefined) {
    throw new Error(`reading the focused element failed: ${evaluated.exceptionDetails.text}`);
  }
  const { objectId } = evaluated.result;
  if (objectId === undefined) {
    return undefined;
  }
  try {
    const { node } = await bounded('DOM.describeNode', timeoutMs, (options) =>
      cdp.send('DOM.describeNode', { objectId }, options),
    );
    return node.backendNodeId;
  } finally {
    await bounded('Runtime.releaseObject', timeoutMs, (options) =>
      cdp.send('Runtime.releaseObject', { objectId }, options),
    );
  }
}

/**
 * Presses Tab within the time budget. A page's `keydown` handler runs inside the press, so one that
 * never returns would otherwise hang the walk.
 */
async function pressTab(page: Page, timeoutMs: number): Promise<void> {
  const press = page.keyboard.press('Tab');
  let timer: NodeJS.Timeout | undefined;
  const timedOut = new Promise<'timed-out'>((resolve) => {
    timer = setTimeout(() => resolve('timed-out'), timeoutMs);
  });
  try {
    if ((await Promise.race([press, timedOut])) === 'timed-out') {
      // The capture fails here and closes the browser, which then rejects the pending press; that
      // rejection carries nothing the timeout does not already report.
      press.catch(() => undefined);
      throw new CaptureError('timeout', `pressing Tab did not finish within ${timeoutMs} ms`);
    }
  } finally {
    clearTimeout(timer);
  }
}

export interface FocusWalk {
  focus: Focus;
  diagnostics: Diagnostic[];
}

/**
 * Presses Tab with real key events and records where focus lands after each press, one step per
 * press, starting from wherever the page left focus. A step is the DOM node of the focused
 * accessibility node, so each stop inside a date input or a closed shadow root is its own step.
 * The walk continues past the step where focus leaves the document, because Chromium then wraps to
 * the first stop. It stops at the first step that repeats an earlier step's node (`cycle_start`),
 * at the second step outside the document, or after `MAX_FOCUS_STEPS` presses (`truncated`).
 *
 * The one exception is focus inside a frame. Capture does not read frame contents, so the step is
 * the `iframe` or `frame` element, which stays the same while focus moves between the frame's own
 * stops. A step that repeats the frame element of the step just before it is therefore recorded
 * and the walk keeps going.
 */
export async function walkFocus(
  page: Page,
  cdp: CDPSession,
  dom: readonly DomNode[],
  idByBackendId: ReadonlyMap<number, number>,
  timeoutMs: number,
): Promise<FocusWalk> {
  const steps: FocusStep[] = [];
  const diagnostics: Diagnostic[] = [];
  const seen = new Set<number>();
  let outside = 0;
  while (steps.length < MAX_FOCUS_STEPS) {
    await pressTab(page, timeoutMs);
    const position = await focusPosition(cdp, dom, idByBackendId, timeoutMs);
    const index = steps.length;
    if (position === undefined) {
      steps.push({});
      outside += 1;
      if (outside === 2) {
        return { focus: { steps, truncated: false }, diagnostics };
      }
      continue;
    }
    const { backendId, source } = position;
    const node = idByBackendId.get(backendId);
    const stillOnPrevious = steps[index - 1]?.backend_id === backendId;
    steps.push(node === undefined ? { backend_id: backendId } : { node, backend_id: backendId });
    if (stillOnPrevious && source === 'active-element' && node !== undefined && isFrame(dom, node)) {
      continue;
    }
    if (seen.has(backendId)) {
      return { focus: { steps, truncated: false, cycle_start: index }, diagnostics };
    }
    seen.add(backendId);
    diagnostics.push(...stepDiagnostics(dom, node, source, index));
  }
  return { focus: { steps, truncated: true }, diagnostics };
}

/** What capture reports about the element focused after Tab press `index + 1`. */
function stepDiagnostics(
  dom: readonly DomNode[],
  node: number | undefined,
  source: FocusPosition['source'],
  index: number,
): Diagnostic[] {
  if (node === undefined) {
    return [
      {
        code: 'focus-target-not-in-dom',
        message: `After Tab press ${index + 1}, focus is on an element that is not in the captured DOM.`,
      },
    ];
  }
  // The accessibility tree reaches into a closed shadow root; the active element stops at its host.
  if (source === 'active-element' && hasClosedShadowRoot(dom, node)) {
    return [
      {
        code: 'focus-in-closed-shadow-root',
        message: `After Tab press ${index + 1}, focus is on a host with a closed shadow root, so the element focused inside it cannot be read.`,
        node,
      },
    ];
  }
  return [];
}

function isFrame(dom: readonly DomNode[], id: number): boolean {
  const tag = dom[id]?.tag;
  return tag === 'iframe' || tag === 'frame';
}

function hasClosedShadowRoot(dom: readonly DomNode[], id: number): boolean {
  return (dom[id]?.children ?? []).some((child) => dom[child]?.shadow === 'closed');
}
