/** Why a capture failed. A page the instrumenter refuses fails with its `InstrumentHtmlError`. */
export type CaptureErrorCode =
  /** The page file cannot be read. */
  | 'page-unreadable'
  /** The page did not load, or did not settle, within the time allowed. */
  | 'timeout'
  /** A file in the page's directory exists but cannot be read to answer a request. */
  | 'asset-unreadable'
  /** The snapshot capture built fails the snapshot schema. */
  | 'invalid-snapshot';

export class CaptureError extends Error {
  override readonly name = 'CaptureError';
  readonly code: CaptureErrorCode;

  constructor(code: CaptureErrorCode, message: string, options?: ErrorOptions) {
    super(message, options);
    this.code = code;
  }
}
