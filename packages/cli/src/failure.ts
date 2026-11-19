import { escapeLines } from './escape.ts';

/** The error's message, prefixed with its code when it carries one. */
export function describeFailure(error: unknown): string {
  if (!(error instanceof Error)) {
    return String(error);
  }
  const code = 'code' in error && typeof error.code === 'string' ? error.code : undefined;
  // Node's file system errors already start their message with the code.
  return code === undefined || error.message.startsWith(`${code}:`)
    ? error.message
    : `${code}: ${error.message}`;
}

/**
 * The stderr line for a failure, every line escaped. A multi-line message, such as Playwright's
 * with its call log, keeps its line breaks; every other control character is escaped, so text
 * from a page, Safari or `safaridriver` cannot rewrite the terminal.
 */
export function failureText(message: string): string {
  return `spokenpath: ${escapeLines(message)}\n`;
}
