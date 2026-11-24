import { NORMALISED_CAPTURED_AT, NORMALISED_PRODUCER_VERSION } from '@spokenpath/capture-chromium';
import type { SafariAxValue, Snapshot } from '@spokenpath/snapshot-types';
import { NONCE_PARAMETER } from './capture-ax.ts';

/** The port every `capture.url` is set to. */
export const NORMALISED_PORT = '0';

/** The nonce every `safari-ax` URL is set to. */
export const NORMALISED_NONCE = '0';

/** The conditions every Safari golden records, which are Chromium capture's fixed conditions. */
export const NORMALISED_CONDITIONS = {
  viewport: { width: 1280, height: 720, device_pixel_ratio: 1 },
  color_scheme: 'light',
  reduced_motion: 'no-preference',
} as const;

/**
 * A copy of a Safari snapshot with every value that differs between runs or Macs of the same
 * capture set to a fixed value:
 *
 * - `volatile.captured_at` and `producer.version`, as Chromium's normaliser sets them;
 * - the port in `capture.url`, which the OS chooses for each capture;
 * - `capture.viewport`, `capture.color_scheme` and `capture.reduced_motion`, which come from the
 *   Mac's display and appearance settings, since WebDriver cannot set them.
 *
 * Everything else is kept, including Safari's version and build, the macOS version and build, and
 * every role, label and error, so a Safari or macOS upgrade shows as a reviewed diff.
 */
export function normaliseSafariForGolden(snapshot: Snapshot): Snapshot {
  const copy = structuredClone(snapshot);
  copy.volatile.captured_at = NORMALISED_CAPTURED_AT;
  copy.producer.version = NORMALISED_PRODUCER_VERSION;
  const url = new URL(copy.capture.url);
  url.port = NORMALISED_PORT;
  copy.capture.url = url.href;
  copy.capture.viewport = { ...NORMALISED_CONDITIONS.viewport };
  copy.capture.color_scheme = NORMALISED_CONDITIONS.color_scheme;
  copy.capture.reduced_motion = NORMALISED_CONDITIONS.reduced_motion;
  return copy;
}

/**
 * The attributes whose values depend on the Mac's display, window placement or layout passes
 * rather than on the page, each set to zero in goldens, keeping its type and shape.
 */
export const DISPLAY_DEPENDENT_ATTRIBUTES: ReadonlySet<string> = new Set([
  // Screen coordinates of the element, which move with the window.
  'AXPosition',
  'AXFrame',
  // The element's size, which follows the viewport the screen allows.
  'AXSize',
  // The height of the Mac's main screen.
  '_AXPrimaryScreenHeight',
  // The part of the text scrolled into view.
  'AXVisibleCharacterRange',
  // A count of layout passes, which varies from run to run.
  'AXLayoutCount',
]);

/** A value with every number in it set to zero, keeping its type and shape. */
function zeroed(value: SafariAxValue): SafariAxValue {
  switch (value.type) {
    case 'number':
      return { type: 'number', value: 0 };
    case 'point':
      return { type: 'point', x: 0, y: 0 };
    case 'size':
      return { type: 'size', width: 0, height: 0 };
    case 'rect':
      return { type: 'rect', x: 0, y: 0, width: 0, height: 0 };
    case 'range':
      return { type: 'range', location: 0, length: 0 };
    case 'array':
      return { type: 'array', items: value.items.map(zeroed) };
    default:
      return value;
  }
}

/** A value with `replace` applied to every string and URL in it. */
function withText(value: SafariAxValue, replace: (text: string) => string): SafariAxValue {
  switch (value.type) {
    case 'string':
    case 'url':
      return { ...value, value: replace(value.value) };
    case 'array':
      return { type: 'array', items: value.items.map((item) => withText(item, replace)) };
    default:
      return value;
  }
}

/**
 * A copy of a `safari-ax` snapshot with every value that differs between runs or Macs of the same
 * capture set to a fixed value: the envelope as `normaliseSafariForGolden` sets it, the nonce in
 * `capture.url` set to `0`, the served port and the nonce set to `0` wherever a string or URL
 * value holds them, and each `DISPLAY_DEPENDENT_ATTRIBUTES` value set to zero. Everything else is
 * kept, so a Safari or macOS upgrade shows as a reviewed diff.
 */
export function normaliseSafariAxForGolden(snapshot: Snapshot): Snapshot {
  const served = new URL(snapshot.capture.url);
  const nonce = served.searchParams.get(NONCE_PARAMETER);
  const copy = normaliseSafariForGolden(snapshot);
  const url = new URL(copy.capture.url);
  url.searchParams.set(NONCE_PARAMETER, NORMALISED_NONCE);
  copy.capture.url = url.href;
  if (copy.payload.engine !== 'safari-ax') {
    return copy;
  }
  const replace = (text: string) => {
    const ported = text.replaceAll(served.host, `${served.hostname}:${NORMALISED_PORT}`);
    return nonce === null || nonce === '' ? ported : ported.replaceAll(nonce, NORMALISED_NONCE);
  };
  copy.payload.nodes = (copy.payload.nodes ?? []).map((node) => ({
    ...node,
    attributes: Object.fromEntries(
      Object.entries(node.attributes).map(([name, value]) => [
        name,
        DISPLAY_DEPENDENT_ATTRIBUTES.has(name) ? zeroed(value) : withText(value, replace),
      ]),
    ),
  }));
  return copy;
}
