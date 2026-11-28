import { CaptureError } from '@spokenpath/capture-chromium';
import { CaptureError as SafariCaptureError } from '@spokenpath/capture-safari';
import { InstrumentHtmlError } from '@spokenpath/instrument-html';

/**
 * Exit codes: 0 on success, 1 when capture fails for a reason other than the page, the platform
 * or the binary is missing, 2 for a usage error, a page that cannot be read or instrumented, or a
 * Safari option on a system without Safari, and 130 or 143 when SIGINT or SIGTERM stops a Safari
 * capture. Otherwise the binary's own code passes through: 2 for a snapshot it rejects, 3 for an
 * unsupported version.
 */
export const EXIT = { ok: 0, failed: 1, badInput: 2, sigint: 130, sigterm: 143 } as const;

/**
 * The exit code of a failure. A page that cannot be read or that the instrumenter refuses, and a
 * Safari capture on a system that cannot run one, are bad input, like a usage error; a Safari
 * capture stopped by a signal exits as a process that signal ends would; any other failure is not.
 */
export function exitCodeOf(error: unknown): number {
  if (error instanceof InstrumentHtmlError) {
    return EXIT.badInput;
  }
  if (
    (error instanceof CaptureError || error instanceof SafariCaptureError) &&
    error.code === 'page-unreadable'
  ) {
    return EXIT.badInput;
  }
  if (error instanceof SafariCaptureError && error.code === 'unsupported-platform') {
    return EXIT.badInput;
  }
  if (error instanceof SafariCaptureError && error.signal !== undefined) {
    return error.signal === 'SIGINT' ? EXIT.sigint : EXIT.sigterm;
  }
  return EXIT.failed;
}
