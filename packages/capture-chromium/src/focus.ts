import type { Diagnostic, DomNode, Focus, FocusStep } from '@spokenpath/snapshot-types';
import type { CDPSession, Page } from 'playwright-core';

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
async function focusedBackendId(cdp: CDPSession): Promise<number | undefined> {
  const evaluated = await cdp.send('Runtime.evaluate', { expression: FOCUSED_ELEMENT, returnByValue: false });
  if (evaluated.exceptionDetails !== undefined) {
    throw new Error(`reading the focused element failed: ${evaluated.exceptionDetails.text}`);
  }
  const { objectId } = evaluated.result;
  if (objectId === undefined) {
    return undefined;
  }
  try {
    const { node } = await cdp.send('DOM.describeNode', { objectId });
    return node.backendNodeId;
  } finally {
    await cdp.send('Runtime.releaseObject', { objectId });
  }
}

export interface FocusWalk {
  focus: Focus;
  diagnostics: Diagnostic[];
}

/**
 * Presses Tab with real key events and records where focus lands after each press, starting from
 * wherever the page left focus. The walk continues past the step where focus leaves the document,
 * because Chromium then wraps to the first stop, and stops at the first step that repeats an
 * earlier element (`cycle_start`), at the second step outside the document, or after
 * `MAX_FOCUS_STEPS` presses (`truncated`).
 */
export async function walkFocus(
  page: Page,
  cdp: CDPSession,
  dom: readonly DomNode[],
  idByBackendId: ReadonlyMap<number, number>,
): Promise<FocusWalk> {
  const steps: FocusStep[] = [];
  const diagnostics: Diagnostic[] = [];
  const seen = new Map<number, number>();
  let outside = 0;
  while (steps.length < MAX_FOCUS_STEPS) {
    await page.keyboard.press('Tab');
    const backendId = await focusedBackendId(cdp);
    const index = steps.length;
    if (backendId === undefined) {
      steps.push({});
      outside += 1;
      if (outside === 2) {
        return { focus: { steps, truncated: false }, diagnostics };
      }
      continue;
    }
    const node = idByBackendId.get(backendId);
    if (node === undefined) {
      steps.push({ backend_id: backendId });
      diagnostics.push({
        code: 'focus-target-not-in-dom',
        message: `After Tab press ${index + 1}, focus is on an element that is not in the captured DOM.`,
      });
    } else {
      steps.push({ node, backend_id: backendId });
      if (hasClosedShadowRoot(dom, node)) {
        diagnostics.push({
          code: 'focus-in-closed-shadow-root',
          message: `After Tab press ${index + 1}, focus is on a host with a closed shadow root, so the element focused inside it cannot be read.`,
          node,
        });
      }
    }
    const earlier = seen.get(backendId);
    if (earlier !== undefined) {
      return { focus: { steps, truncated: false, cycle_start: index }, diagnostics };
    }
    seen.set(backendId, index);
  }
  return { focus: { steps, truncated: true }, diagnostics };
}

function hasClosedShadowRoot(dom: readonly DomNode[], id: number): boolean {
  return (dom[id]?.children ?? []).some((child) => dom[child]?.shadow === 'closed');
}
