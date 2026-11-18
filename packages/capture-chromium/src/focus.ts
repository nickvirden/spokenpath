import type { Diagnostic, DomNode, Focus, FocusStep } from '@spokenpath/snapshot-types';
import type { CDPSession, Page } from 'playwright-core';
import { CaptureError } from './error.ts';
import { bounded } from './session.ts';

/** The most Tab presses a walk makes before it stops and reports itself truncated. */
export const MAX_FOCUS_STEPS = 200;

/**
 * The focused element, descending through each open shadow root's own active element, or `null`
 * when the document does not have focus or only `body` is active. Evaluated as a protocol
 * expression so the result is a remote object that `DOM.describeNode` can resolve.
 */
const FOCUSED_ELEMENT = `(() => {
  if (!document.hasFocus()) return null;
  let element = document.activeElement;
  while (element !== null && element.shadowRoot !== null && element.shadowRoot.activeElement !== null) {
    element = element.shadowRoot.activeElement;
  }
  return element === null || element === document.body ? null : element;
})()`;

/** The `backendNodeId` of the focused element, or nothing when focus is outside the document. */
async function focusedBackendId(cdp: CDPSession, timeoutMs: number): Promise<number | undefined> {
  const evaluated = await bounded('reading the focused element', timeoutMs, (options) =>
    cdp.send('Runtime.evaluate', { expression: FOCUSED_ELEMENT, returnByValue: false }, options),
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
 * press, starting from wherever the page left focus. The walk continues past the step where focus
 * leaves the document, because Chromium then wraps to the first stop. It stops at the first step
 * that repeats an element an earlier, non-adjacent step focused (`cycle_start`), at the second
 * step outside the document, or after `MAX_FOCUS_STEPS` presses (`truncated`).
 *
 * A step that repeats the element of the step just before it is no cycle only when that element is
 * an `iframe` or `frame`, or a host with a closed shadow root: focus then moves inside content
 * capture cannot read, and only the frame element or the host is visible. The walk records the
 * press and keeps going, and the element's diagnostic covers the whole run of such steps. On any
 * other element, including one missing from `dom`, the repeat means a script kept focus where it
 * was, and it is a cycle like any other.
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
    const backendId = await focusedBackendId(cdp, timeoutMs);
    const index = steps.length;
    const stillOnPrevious = backendId !== undefined && steps[index - 1]?.backend_id === backendId;
    if (backendId === undefined) {
      steps.push({});
      outside += 1;
      if (outside === 2) {
        return { focus: { steps, truncated: false }, diagnostics };
      }
      continue;
    }
    const node = idByBackendId.get(backendId);
    steps.push(node === undefined ? { backend_id: backendId } : { node, backend_id: backendId });
    if (stillOnPrevious && node !== undefined && hidesFocusedElement(dom, node)) {
      continue;
    }
    // The previous step already reported the element; an adjacent repeat here is a cycle.
    if (!stillOnPrevious) {
      diagnostics.push(...stepDiagnostics(dom, node, index));
    }
    if (seen.has(backendId)) {
      return { focus: { steps, truncated: false, cycle_start: index }, diagnostics };
    }
    seen.add(backendId);
  }
  return { focus: { steps, truncated: true }, diagnostics };
}

/** What capture reports about the element focused after Tab press `index + 1`. */
function stepDiagnostics(dom: readonly DomNode[], node: number | undefined, index: number): Diagnostic[] {
  if (node === undefined) {
    return [
      {
        code: 'focus-target-not-in-dom',
        message: `After Tab press ${index + 1}, focus is on an element that is not in the captured DOM.`,
      },
    ];
  }
  if (hasClosedShadowRoot(dom, node)) {
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

/** Whether focus can move inside the element while only the element itself shows as focused. */
function hidesFocusedElement(dom: readonly DomNode[], id: number): boolean {
  const tag = dom[id]?.tag;
  return tag === 'iframe' || tag === 'frame' || hasClosedShadowRoot(dom, id);
}

function hasClosedShadowRoot(dom: readonly DomNode[], id: number): boolean {
  return (dom[id]?.children ?? []).some((child) => dom[child]?.shadow === 'closed');
}
