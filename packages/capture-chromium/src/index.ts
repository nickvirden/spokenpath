export { type CaptureOptions, captureChromium, sortDiagnostics, verifyInstrumentation } from './capture.ts';
export { CaptureError, type CaptureErrorCode } from './error.ts';
export { MAX_FOCUS_STEPS } from './focus.ts';
export { NORMALISED_CAPTURED_AT, NORMALISED_PRODUCER_VERSION, normaliseForGolden } from './normalise.ts';
export { contentType, redactUrl, resolveAsset } from './serve.ts';
export type { DomComparison } from './verify.ts';

export const packageName: '@spokenpath/capture-chromium' = '@spokenpath/capture-chromium';
