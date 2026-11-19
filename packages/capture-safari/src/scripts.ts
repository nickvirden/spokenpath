/**
 * The scripts capture runs in the page over WebDriver. The test-double driver recognises them by
 * identity, so each is defined once, here.
 */

/** Returns the viewport, the device pixel ratio, the window's outer size and the media features. */
export const MEASURE_SCRIPT = `return {
  innerWidth: window.innerWidth,
  innerHeight: window.innerHeight,
  outerWidth: window.outerWidth,
  outerHeight: window.outerHeight,
  devicePixelRatio: window.devicePixelRatio,
  dark: window.matchMedia('(prefers-color-scheme: dark)').matches,
  light: window.matchMedia('(prefers-color-scheme: light)').matches,
  reducedMotion: window.matchMedia('(prefers-reduced-motion: reduce)').matches,
};`;

/** Returns `document.visibilityState`. */
export const VISIBILITY_SCRIPT = 'return document.visibilityState;';

/** Returns whether an element matches the selector in `arguments[0]`. */
export const SELECTOR_SCRIPT = 'return document.querySelector(arguments[0]) !== null;';

/** Resolves once web fonts have loaded and two animation frames have run, as Chromium capture does. */
export const SETTLE_SCRIPT = `const done = arguments[arguments.length - 1];
document.fonts.ready.then(() => requestAnimationFrame(() => requestAnimationFrame(() => done(true))));`;

/** What `MEASURE_SCRIPT` returns. */
export interface Measurement {
  innerWidth: number;
  innerHeight: number;
  outerWidth: number;
  outerHeight: number;
  devicePixelRatio: number;
  dark: boolean;
  light: boolean;
  reducedMotion: boolean;
}

/** The measurement, when `value` has its shape. */
export function measurement(value: unknown): Measurement | undefined {
  if (typeof value !== 'object' || value === null) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  const numbers = ['innerWidth', 'innerHeight', 'outerWidth', 'outerHeight', 'devicePixelRatio'] as const;
  const flags = ['dark', 'light', 'reducedMotion'] as const;
  if (
    numbers.every((key) => typeof record[key] === 'number' && Number.isFinite(record[key])) &&
    flags.every((key) => typeof record[key] === 'boolean')
  ) {
    return record as unknown as Measurement;
  }
  return undefined;
}
