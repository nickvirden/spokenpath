import type { Span } from '@spokenpath/snapshot-types';

/** What every source class token starts with. */
export const CLASS_TOKEN_PREFIX = 'sp-src-';

/**
 * The class token that carries a span's source location where only an element's class list is
 * exposed, such as Safari's macOS accessibility tree (`AXDOMClassList`): `sp-src-<line>-<column>`.
 * The line and column identify a start tag in its source file, so the token names one span of a
 * single-source page.
 */
export function sourceClassToken(span: Pick<Span, 'line' | 'column'>): string {
  return `${CLASS_TOKEN_PREFIX}${span.line}-${span.column}`;
}
