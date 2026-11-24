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
   * frames do not run and the page cannot settle, or its accessibility tree may not be the visible
   * page's; or the accessibility helper finds the Mac's screen locked.
   */
  | 'page-hidden'
  /**
   * The page navigated away from the loopback origin capture served it on while it settled or was
   * measured, before its elements were read. Capture detects the navigation after it happens; it
   * does not prevent it. A navigation during the element reads is not this failure: Safari answers
   * those reads with `stale element reference`, and the elements are recorded without an entry.
   */
  | 'page-left-origin'
  /** A call into Safari that capture cannot do without, such as loading the page, answered with an error. */
  | 'safari-call-failed'
  /** The page did not load or settle, or Safari or `safaridriver` did not answer, in time. */
  | 'timeout'
  /** The process received SIGINT or SIGTERM during capture; the session and driver were stopped. */
  | 'interrupted'
  /** The snapshot capture built fails the snapshot schema. */
  | 'invalid-snapshot'
  /**
   * The process that runs capture lacks the macOS Accessibility permission, so Safari's macOS
   * accessibility tree cannot be read: macOS reports the process untrusted, refuses the request
   * with `kAXErrorAPIDisabled`, or returns none of the check page's nodes.
   */
  | 'accessibility-permission-missing'
  /** No single Safari process runs in the user's session for the accessibility helper to read. */
  | 'safari-not-running'
  /**
   * The accessibility helper found no single Safari window and web area that show the served page,
   * so it read nothing.
   */
  | 'safari-window-not-identified'
  /**
   * Safari's macOS accessibility tree for the check page has some but not all of its expected
   * nodes, so the tree cannot be relied on.
   */
  | 'ax-canary-incomplete'
  /** Safari's macOS accessibility tree is larger than capture records. */
  | 'ax-tree-too-large'
  /** The `spokenpath-ax-macos` helper is not built where capture runs it from. */
  | 'ax-helper-not-found'
  /** The `spokenpath-ax-macos` helper failed in a way it does not name, or printed no answer capture reads. */
  | 'ax-helper-failed';

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
