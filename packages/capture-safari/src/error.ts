/** Why a Safari capture failed. A page the instrumenter refuses fails with its `InstrumentHtmlError`. */
export type CaptureErrorCode =
  /** The options passed to capture are not usable, such as a `timeoutMs` that is not positive. */
  | 'invalid-options'
  /** Safari capture runs only on macOS. */
  | 'unsupported-platform'
  /** `safaridriver` is not installed where capture runs it from. */
  | 'safaridriver-not-found'
  /** `safaridriver` did not start, or Safari refused the automation session. */
  | 'safari-session-refused'
  /** The page file cannot be read. */
  | 'page-unreadable'
  /** A file in the page's directory exists but cannot be read to answer a request. */
  | 'asset-unreadable'
  /**
   * Safari reports the page as hidden, as it does while the Mac is locked or asleep, so animation
   * frames do not run and the page cannot settle.
   */
  | 'page-hidden'
  /** The page did not load or settle, or Safari or `safaridriver` did not answer, in time. */
  | 'timeout'
  /** The process received SIGINT or SIGTERM during capture; the session and driver were stopped. */
  | 'interrupted'
  /** The snapshot capture built fails the snapshot schema. */
  | 'invalid-snapshot';

/** The signals that interrupt a capture. */
export type InterruptSignal = 'SIGINT' | 'SIGTERM';

export class CaptureError extends Error {
  override readonly name = 'CaptureError';
  readonly code: CaptureErrorCode;
  /** The signal that interrupted the capture, for the `interrupted` code. */
  readonly signal: InterruptSignal | undefined;

  constructor(
    code: CaptureErrorCode,
    message: string,
    options?: ErrorOptions & { signal?: InterruptSignal },
  ) {
    super(message, options);
    this.code = code;
    this.signal = options?.signal;
  }
}
