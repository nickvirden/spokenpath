import { NORMALISED_CAPTURED_AT, NORMALISED_PRODUCER_VERSION } from '@spokenpath/capture-chromium';
import type { Snapshot } from '@spokenpath/snapshot-types';

/** The port every `capture.url` is set to. */
export const NORMALISED_PORT = '0';

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
