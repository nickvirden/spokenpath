export { type CaptureOptions, captureSafari, DEFAULT_TIMEOUT_MS, type TestDriver } from './capture.ts';
export { type AxCaptureOptions, captureSafariAx } from './capture-ax.ts';
export { CaptureError, type CaptureErrorCode, type InterruptSignal } from './error.ts';
export { normaliseSafariAxForGolden, normaliseSafariForGolden } from './normalise.ts';
export { CONTENT_SECURITY_POLICY } from './serve.ts';

export const packageName: '@spokenpath/capture-safari' = '@spokenpath/capture-safari';
